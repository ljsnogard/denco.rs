//! 可反序列化数据结构的抽象。

use abs_cancel::{TrCancellationToken, TrMayCancel};

use super::{Error, NoArena, TrArena, TrDecoder};

/// 可以从任意数据格式反序列化的数据结构。
///
/// `'alloc` 是结果可以借用的 decoder arena 的寿命，`Alloc` 是该 arena 的类型。
/// 解码方向**没有**「借用输入数据」的生命周期参数：输入借用入口已被移除，
/// 结果要么完全不借用（owned 值 / `Global` allocator），要么借用调用者提供的
/// arena。`decode_async` 返回 GAT 关联类型 [`TrDecodable::DecodeAsync`]，它实现
/// [`TrMayCancel`]；取消生命周期 `'a` 由调用者根据反序列化器和令牌的寿命选择。
///
/// # Errors
///
/// 当输入与 `Self` 不兼容、数据格式报错或实现者响应取消信号时返回
/// `D::Error`。
pub trait TrDecodable<'alloc, Alloc: TrArena<'alloc> + 'alloc>: Sized {
    /// 反序列化本值时产生的可取消 future。
    ///
    /// 具体类型由实现者决定；实现者可以在该关联类型上附加 `Send` 等约束。
    type DecodeAsync<'a, D>: TrMayCancel<'a, MayCancelOutput = Result<Self, D::Error>>
    where
        'alloc: 'a,
        D: TrDecoder<'alloc, Alloc> + 'a,
        D::Error: 'a,
        Self: 'a;

    /// 从给定的 `deserializer` 构造 `Self`。
    ///
    /// # Errors
    ///
    /// 输入不合法或底层读取失败时返回错误。
    fn decode_async<'a, D>(deserializer: D) -> Self::DecodeAsync<'a, D>
    where
        'alloc: 'a,
        D: TrDecoder<'alloc, Alloc> + 'a,
        D::Error: 'a,
        Self: 'a;
}

/// 可以在任意 arena 寿命下用 [`NoArena`] 反序列化、且不借用 arena 的数据结构。
///
/// 定义是 `for<'alloc> TrDecodable<'alloc, NoArena>`。解码方向不再有输入借用
/// 生命周期之后，这个约束表达的是「与 arena 无关的 owned 结果」：
///
/// - `String`、`Vec<T>`、`Cow<'a, str>`、`BumpString<Global>` 等满足——
///   它们的 impl 对所有 `'alloc` 都成立；
/// - `&'alloc str` / `&'alloc [u8]` **不**满足：它们需要 `TrArenaBytes`，
///   而 `NoArena` 不实现；
/// - [`BumpBox<'alloc, T>`](https://docs.rs/denco_bumpalo) 不满足：`Self`
///   本身携带 `'alloc`；
/// - derive 生成的 `Foo<'x>` **通常也不满足**：宏把用户生命周期绑定到
///   `'alloc`，`Foo<'x>` 只实现 `TrDecodable<'x, ...>`，不是对所有 `'alloc`。
///
/// 因此它不能当作「所有 owned 类型」的统称，而是「与 arena 解耦」的精确约束。
///
/// # Examples
///
/// ```
/// use std::borrow::Cow;
///
/// use abs_denco::de::TrDecodableOwned;
///
/// fn assert_owned<T: TrDecodableOwned>() {}
///
/// assert_owned::<String>();
/// assert_owned::<Vec<u32>>();
/// assert_owned::<Cow<'static, str>>();
/// ```
///
/// `&'alloc str` 与 derive 生成的 `Foo<'x>` 不满足该约束；例如下面的代码
/// 无法编译：
///
/// ```compile_fail
/// use abs_denco::de::TrDecodableOwned;
///
/// fn assert_owned<T: TrDecodableOwned>() {}
///
/// assert_owned::<&'static str>();
/// ```
pub trait TrDecodableOwned: for<'alloc> TrDecodable<'alloc, NoArena> {}

impl<T> TrDecodableOwned for T where T: for<'alloc> TrDecodable<'alloc, NoArena> {}

/// 把「如何构造值」与「反序列化器」解耦的 seed。
///
/// `TrDecodableSeed` 与 [`TrDecodable`] 的区别在于它自带状态：映射的键、
/// 带参数的构造函数等场景可以把参数放在 seed 里。
///
/// # Errors
///
/// 与 [`TrDecodable`] 相同，由具体的反序列化器和 seed 决定。
pub trait TrDecodableSeed<'alloc, Alloc: TrArena<'alloc> + 'alloc>: Sized {
    /// seed 最终产生的值。
    type Value;

    /// 反序列化本 seed 时产生的可取消 future。
    type DecodeAsync<'a, D>: TrMayCancel<'a, MayCancelOutput = Result<Self::Value, D::Error>>
    where
        'alloc: 'a,
        D: TrDecoder<'alloc, Alloc> + 'a,
        D::Error: 'a,
        Self: 'a,
        Self::Value: 'a;

    /// 用给定的 `deserializer` 产生 `Self::Value`。
    ///
    /// # Errors
    ///
    /// 输入不合法或底层读取失败时返回错误。
    fn decode_async<'a, D>(self, deserializer: D) -> Self::DecodeAsync<'a, D>
    where
        'alloc: 'a,
        D: TrDecoder<'alloc, Alloc> + 'a,
        D::Error: 'a,
        Self: 'a,
        Self::Value: 'a;
}

impl<'alloc, Alloc, T> TrDecodableSeed<'alloc, Alloc> for core::marker::PhantomData<T>
where
    Alloc: TrArena<'alloc> + 'alloc,
    T: TrDecodable<'alloc, Alloc>,
{
    type Value = T;

    type DecodeAsync<'a, D>
        = <T as TrDecodable<'alloc, Alloc>>::DecodeAsync<'a, D>
    where
        'alloc: 'a,
        D: TrDecoder<'alloc, Alloc> + 'a,
        D::Error: 'a,
        Self: 'a,
        Self::Value: 'a;

    fn decode_async<'a, D>(self, deserializer: D) -> Self::DecodeAsync<'a, D>
    where
        'alloc: 'a,
        D: TrDecoder<'alloc, Alloc> + 'a,
        D::Error: 'a,
        Self: 'a,
        Self::Value: 'a,
    {
        T::decode_async(deserializer)
    }
}

/// 引用版解码：先按值解出 `T`，再通过 allocator 搬入 arena。
///
/// 这里用 `Alloc::Ref<T> = &'alloc mut T` 把 allocator 的引用形态钉死为
/// `Self`，因此同一个 `&'alloc mut T` 对任何「返回 `&mut T`」的 arena
/// （例如 bumpalo）都成立；返回其它形态（例如 sw-scope 的 `Owning`）的
/// allocator 不会匹配本实现。
///
/// 值版 `T` 可以是任意 `TrDecodable` 实现（包括 derive 生成的、对 allocator
/// 泛化的实现），所以引用版仍然是值版的薄包装，不需要改 `decode_*` 读取逻辑。
#[gen_mcf2::gen_may_cancel_future(DecodeArenaRefOp, pub, new(pub))]
async fn decode_arena_ref_async_<'alloc, 'a, Alloc, T, D, C>(
    de: D,
    cancel: C,
) -> Result<&'alloc mut T, D::Error>
where
    'alloc: 'a,
    Alloc: TrArena<'alloc, Ref<T> = &'alloc mut T> + 'alloc,
    T: TrDecodable<'alloc, Alloc> + 'alloc,
    D: TrDecoder<'alloc, Alloc> + 'a,
    D::Error: Error + 'a,
    C: TrCancellationToken,
{
    let alloc = de.allocator();
    let value = T::decode_async(de)
        .may_cancel_with(cancel.child_token())
        .await?;
    alloc
        .alloc_value_async(value)
        .may_cancel_with(cancel)
        .await
        .map_err(<D::Error as Error>::custom)
}

impl<'alloc, Alloc, T> TrDecodable<'alloc, Alloc> for &'alloc mut T
where
    Alloc: TrArena<'alloc, Ref<T> = &'alloc mut T> + 'alloc,
    T: TrDecodable<'alloc, Alloc> + 'alloc,
{
    type DecodeAsync<'a, D>
        = DecodeArenaRefOpAsync<'alloc, 'a, 'a, Alloc, T, D>
    where
        'alloc: 'a,
        D: TrDecoder<'alloc, Alloc> + 'a,
        D::Error: 'a,
        Self: 'a;

    fn decode_async<'a, D>(de: D) -> Self::DecodeAsync<'a, D>
    where
        'alloc: 'a,
        D: TrDecoder<'alloc, Alloc> + 'a,
        D::Error: 'a,
        Self: 'a,
    {
        DecodeArenaRefOpAsync::new(de)
    }
}
