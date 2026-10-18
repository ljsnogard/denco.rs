//! 可反序列化数据结构的抽象。

use abs_cancel::TrMayCancel;

use super::TrDecoder;

/// 可以从任意数据格式反序列化的数据结构。
///
/// 生命周期 `'de` 表示反序列化结果可能借用输入数据的寿命。`decode_async`
/// 返回 GAT 关联类型 [`TrDecodable::DecodeAsync`]，它实现
/// [`TrMayCancel`]；取消生命周期 `'a` 由调用者根据反序列化器和令牌的
/// 寿命选择。
///
/// # Errors
///
/// 当输入与 `Self` 不兼容、数据格式报错或实现者响应取消信号时返回
/// `D::Error`。
pub trait TrDecodable<'de>: Sized {
    /// 反序列化本值时产生的可取消 future。
    ///
    /// 具体类型由实现者决定；实现者可以在该关联类型上附加 `Send` 等约束。
    type DecodeAsync<'a, D>: TrMayCancel<'a, MayCancelOutput = Result<Self, D::Error>>
    where
        'de: 'a,
        D: TrDecoder<'de> + 'a,
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
        D: TrDecoder<'de> + 'a,
        D::Error: 'a,
        Self: 'a;
}

/// 可以从任意数据格式反序列化、且不借用输入数据的数据结构。
///
/// 这是 `for<'de> TrDecodable<'de>` 的简写。
pub trait TrDecodableOwned: for<'de> TrDecodable<'de> {}

impl<T> TrDecodableOwned for T where T: for<'de> TrDecodable<'de> {}

/// 把「如何构造值」与「反序列化器」解耦的 seed。
///
/// `TrDecodableSeed` 与 [`TrDecodable`] 的区别在于它自带状态：映射的键、
/// 带参数的构造函数等场景可以把参数放在 seed 里。
///
/// # Errors
///
/// 与 [`TrDecodable`] 相同，由具体的反序列化器和 seed 决定。
pub trait TrDecodableSeed<'de>: Sized {
    /// seed 最终产生的值。
    type Value;

    /// 反序列化本 seed 时产生的可取消 future。
    type DecodeAsync<'a, D>: TrMayCancel<'a, MayCancelOutput = Result<Self::Value, D::Error>>
    where
        'de: 'a,
        D: TrDecoder<'de> + 'a,
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
        D: TrDecoder<'de> + 'a,
        D::Error: 'a,
        Self: 'a,
        Self::Value: 'a;
}

impl<'de, T> TrDecodableSeed<'de> for core::marker::PhantomData<T>
where
    T: TrDecodable<'de>,
{
    type Value = T;

    type DecodeAsync<'a, D>
        = <T as TrDecodable<'de>>::DecodeAsync<'a, D>
    where
        'de: 'a,
        D: TrDecoder<'de> + 'a,
        D::Error: 'a,
        Self: 'a,
        Self::Value: 'a;

    fn decode_async<'a, D>(self, deserializer: D) -> Self::DecodeAsync<'a, D>
    where
        D: TrDecoder<'de> + 'a,
        D::Error: 'a,
        Self: 'a,
        Self::Value: 'a,
    {
        T::decode_async(deserializer)
    }
}
