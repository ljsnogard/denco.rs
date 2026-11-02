//! 可反序列化数据结构的抽象。

use abs_cancel::{TrCancellationToken, TrMayCancel};

use super::{Error, NoArena, TrArena, TrDecoder};

/// 可以从任意数据格式反序列化的数据结构。
///
/// 生命周期 `'de` 表示反序列化结果可能借用输入数据的寿命；`'alloc` 表示
/// 结果可以借用 decoder 所携带 arena 的寿命；`Alloc` 是该 arena 的类型。
/// `decode_async` 返回 GAT 关联类型 [`TrDecodable::DecodeAsync`]，它实现
/// [`TrMayCancel`]；取消生命周期 `'a` 由调用者根据反序列化器和令牌的寿命选择。
///
/// # Errors
///
/// 当输入与 `Self` 不兼容、数据格式报错或实现者响应取消信号时返回
/// `D::Error`。
pub trait TrDecodable<'de, 'alloc, Alloc: TrArena<'alloc> + 'alloc>: Sized {
    /// 反序列化本值时产生的可取消 future。
    ///
    /// 具体类型由实现者决定；实现者可以在该关联类型上附加 `Send` 等约束。
    type DecodeAsync<'a, D>: TrMayCancel<'a, MayCancelOutput = Result<Self, D::Error>>
    where
        'de: 'a,
        'alloc: 'a,
        D: TrDecoder<'de, 'alloc, Alloc> + 'a,
        D::Error: 'a,
        Self: 'a;

    /// 从给定的 `deserializer` 构造 `Self`。
    ///
    /// # Errors
    ///
    /// 输入不合法或底层读取失败时返回错误。
    fn decode_async<'a, D>(deserializer: D) -> Self::DecodeAsync<'a, D>
    where
        'de: 'a,
        'alloc: 'a,
        D: TrDecoder<'de, 'alloc, Alloc> + 'a,
        D::Error: 'a,
        Self: 'a;
}

/// 可以从任意数据格式反序列化、且不借用输入数据的数据结构。
///
/// 这是 `for<'de, 'alloc> TrDecodable<'de, 'alloc, NoArena>` 的简写：
/// 值版实现对所有 allocator 都一样，用 [`NoArena`] 作为不涉及 arena 的
/// 规范形态即可判定「owned」。
pub trait TrDecodableOwned: for<'de, 'alloc> TrDecodable<'de, 'alloc, NoArena> {}

impl<T> TrDecodableOwned for T where T: for<'de, 'alloc> TrDecodable<'de, 'alloc, NoArena> {}

/// 把「如何构造值」与「反序列化器」解耦的 seed。
///
/// `TrDecodableSeed` 与 [`TrDecodable`] 的区别在于它自带状态：映射的键、
/// 带参数的构造函数等场景可以把参数放在 seed 里。
///
/// # Errors
///
/// 与 [`TrDecodable`] 相同，由具体的反序列化器和 seed 决定。
pub trait TrDecodableSeed<'de, 'alloc, Alloc: TrArena<'alloc> + 'alloc>: Sized {
    /// seed 最终产生的值。
    type Value;

    /// 反序列化本 seed 时产生的可取消 future。
    type DecodeAsync<'a, D>: TrMayCancel<'a, MayCancelOutput = Result<Self::Value, D::Error>>
    where
        'de: 'a,
        'alloc: 'a,
        D: TrDecoder<'de, 'alloc, Alloc> + 'a,
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
        'de: 'a,
        'alloc: 'a,
        D: TrDecoder<'de, 'alloc, Alloc> + 'a,
        D::Error: 'a,
        Self: 'a,
        Self::Value: 'a;
}

impl<'de, 'alloc, Alloc, T> TrDecodableSeed<'de, 'alloc, Alloc> for core::marker::PhantomData<T>
where
    Alloc: TrArena<'alloc> + 'alloc,
    T: TrDecodable<'de, 'alloc, Alloc>,
{
    type Value = T;

    type DecodeAsync<'a, D>
        = <T as TrDecodable<'de, 'alloc, Alloc>>::DecodeAsync<'a, D>
    where
        'de: 'a,
        'alloc: 'a,
        D: TrDecoder<'de, 'alloc, Alloc> + 'a,
        D::Error: 'a,
        Self: 'a,
        Self::Value: 'a;

    fn decode_async<'a, D>(self, deserializer: D) -> Self::DecodeAsync<'a, D>
    where
        'de: 'a,
        'alloc: 'a,
        D: TrDecoder<'de, 'alloc, Alloc> + 'a,
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
async fn decode_arena_ref_async_<'de, 'alloc, 'a, Alloc, T, D, C>(
    de: D,
    cancel: C,
) -> Result<&'alloc mut T, D::Error>
where
    'de: 'a,
    'alloc: 'a,
    Alloc: TrArena<'alloc, Ref<T> = &'alloc mut T> + 'alloc,
    T: TrDecodable<'de, 'alloc, Alloc> + 'alloc,
    D: TrDecoder<'de, 'alloc, Alloc> + 'a,
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

impl<'de, 'alloc, Alloc, T> TrDecodable<'de, 'alloc, Alloc> for &'alloc mut T
where
    Alloc: TrArena<'alloc, Ref<T> = &'alloc mut T> + 'alloc,
    T: TrDecodable<'de, 'alloc, Alloc> + 'alloc,
{
    type DecodeAsync<'a, D>
        = DecodeArenaRefOpAsync<'de, 'alloc, 'a, 'a, Alloc, T, D>
    where
        'de: 'a,
        'alloc: 'a,
        D: TrDecoder<'de, 'alloc, Alloc> + 'a,
        D::Error: 'a,
        Self: 'a;

    fn decode_async<'a, D>(de: D) -> Self::DecodeAsync<'a, D>
    where
        'de: 'a,
        'alloc: 'a,
        D: TrDecoder<'de, 'alloc, Alloc> + 'a,
        D::Error: 'a,
        Self: 'a,
    {
        DecodeArenaRefOpAsync::new(de)
    }
}
