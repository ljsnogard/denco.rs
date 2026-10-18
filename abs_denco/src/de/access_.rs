//! 序列、映射、枚举以及变体的访问器 trait。

use abs_cancel::TrMayCancel;

use super::{Error, TrDecodableSeed, Visitor};

/// 提供对序列中各个元素的访问。
///
/// 每次调用 [`SeqAccess::next_element_seed`] 都返回一个异步 future：
/// 完成后得到 `Some(value)` 或 `None`（序列结束）。
///
/// # Errors
///
/// 当输入不是合法序列、元素无法反序列化或底层 I/O 失败时返回
/// `Self::Error`。
pub trait SeqAccess<'de> {
    /// 反序列化失败时产生的错误类型。
    type Error: Error;

    /// 用给定的 seed 反序列化下一个元素。
    ///
    /// # Errors
    ///
    /// 元素不合法或底层读取失败时返回错误。
    fn next_element_seed<'a, T>(
        &'a mut self,
        seed: T,
    ) -> impl TrMayCancel<'a, MayCancelOutput = Result<Option<T::Value>, Self::Error>>
    where
        'de: 'a,
        T: TrDecodableSeed<'de> + 'a,
        T::Value: 'a,
        Self: 'a,
        Self::Error: 'a;

    /// 返回剩余元素数量的提示。
    ///
    /// 默认返回 `None`，表示格式不提供可靠提示。
    fn size_hint(&self) -> Option<usize> {
        None
    }
}

/// 提供对映射中各个键值对的访问。
///
/// 调用顺序应当是先 [`MapAccess::next_key_seed`]，再
/// [`MapAccess::next_value_seed`]；两者都返回异步 future。
///
/// # Errors
///
/// 当输入不是合法映射、键或值无法反序列化或底层 I/O 失败时返回
/// `Self::Error`。
pub trait MapAccess<'de> {
    /// 反序列化失败时产生的错误类型。
    type Error: Error;

    /// 用给定的 seed 反序列化下一个键。
    ///
    /// 返回 `Ok(None)` 表示映射结束。
    ///
    /// # Errors
    ///
    /// 键不合法或底层读取失败时返回错误。
    fn next_key_seed<'a, K>(
        &'a mut self,
        seed: K,
    ) -> impl TrMayCancel<'a, MayCancelOutput = Result<Option<K::Value>, Self::Error>>
    where
        'de: 'a,
        K: TrDecodableSeed<'de> + 'a,
        K::Value: 'a,
        Self: 'a,
        Self::Error: 'a;

    /// 用给定的 seed 反序列化当前键对应的值。
    ///
    /// # Errors
    ///
    /// 值不合法、没有对应的键或底层读取失败时返回错误。
    fn next_value_seed<'a, V>(
        &'a mut self,
        seed: V,
    ) -> impl TrMayCancel<'a, MayCancelOutput = Result<V::Value, Self::Error>>
    where
        'de: 'a,
        V: TrDecodableSeed<'de> + 'a,
        V::Value: 'a,
        Self: 'a,
        Self::Error: 'a;

    /// 返回剩余键值对数量的提示。
    ///
    /// 默认返回 `None`，表示格式不提供可靠提示。
    fn size_hint(&self) -> Option<usize> {
        None
    }
}

/// 提供对枚举变体及其内容的访问。
///
/// # Errors
///
/// 当输入不是合法枚举或变体名无法识别时返回 `Self::Error`。
pub trait EnumAccess<'de>: Sized {
    /// 反序列化失败时产生的错误类型。
    type Error: Error;

    /// 继续读取变体内容的状态机。
    type Variant: VariantAccess<'de, Error = Self::Error>;

    /// 用给定的 seed 读取变体名，并返回变体内容的访问器。
    ///
    /// # Errors
    ///
    /// 变体名不合法或底层读取失败时返回错误。
    fn variant_seed<'a, V>(
        self,
        seed: V,
    ) -> impl TrMayCancel<'a, MayCancelOutput = Result<(V::Value, Self::Variant), Self::Error>>
    where
        'de: 'a,
        V: TrDecodableSeed<'de> + 'a,
        V::Value: 'a,
        Self: 'a,
        Self::Variant: 'a,
        Self::Error: 'a;
}

/// 提供对某个具体枚举变体内容的访问。
///
/// # Errors
///
/// 变体内容与声明的形状不符，或底层读取失败时返回 `Self::Error`。
pub trait VariantAccess<'de>: Sized {
    /// 反序列化失败时产生的错误类型。
    type Error: Error;

    /// 反序列化一个单元变体。
    ///
    /// # Errors
    ///
    /// 变体带有内容或底层读取失败时返回错误。
    fn unit_variant<'a>(self) -> impl TrMayCancel<'a, MayCancelOutput = Result<(), Self::Error>>
    where
        'de: 'a,
        Self: 'a,
        Self::Error: 'a;

    /// 用给定的 seed 反序列化 newtype 变体的内容。
    ///
    /// # Errors
    ///
    /// 变体形状不符或内容不合法时返回错误。
    fn newtype_variant_seed<'a, T>(
        self,
        seed: T,
    ) -> impl TrMayCancel<'a, MayCancelOutput = Result<T::Value, Self::Error>>
    where
        'de: 'a,
        T: TrDecodableSeed<'de> + 'a,
        T::Value: 'a,
        Self: 'a,
        Self::Error: 'a;

    /// 反序列化元组变体的内容。
    ///
    /// # Errors
    ///
    /// 变体形状或字段数不符时返回错误。
    fn tuple_variant<'a, V>(
        self,
        len: usize,
        visitor: V,
    ) -> impl TrMayCancel<'a, MayCancelOutput = Result<V::Value, Self::Error>>
    where
        'de: 'a,
        V: Visitor<'de> + 'a,
        V::Value: 'a,
        Self: 'a,
        Self::Error: 'a;

    /// 反序列化结构体变体的内容。
    ///
    /// # Errors
    ///
    /// 变体形状或字段集合不符时返回错误。
    fn struct_variant<'a, V>(
        self,
        fields: &'static [&'static str],
        visitor: V,
    ) -> impl TrMayCancel<'a, MayCancelOutput = Result<V::Value, Self::Error>>
    where
        'de: 'a,
        V: Visitor<'de> + 'a,
        V::Value: 'a,
        Self: 'a,
        Self::Error: 'a;
}
