//! 解码器抽象。

use abs_cancel::TrMayCancel;

use super::{Error, Visitor};

/// 一个可以从某种格式读出数据结构的解码器。
///
/// 本 trait 是同步版 `serde::de::Deserializer` 的异步版本。每个
/// `decode_*` 方法消耗 `self`，返回一个 GAT 关联类型，它实现
/// [`TrMayCancel`]。
///
/// 与同步版 serde 一样，所有方法都是显式声明的，没有默认转发。格式
/// 实现可以使用 [`crate::forward_to_decode_any`] 宏把不需要特化的
/// 方法转发到 [`TrDecoder::decode_any`]。
///
/// # Errors
///
/// 所有 `decode_*` 方法在输入不合法、底层 I/O 失败，或实现者响应
/// 取消信号时返回 `Self::Error`。
pub trait TrDecoder<'de>: Sized {
    /// 反序列化失败时产生的错误类型。
    type Error: Error;

    /// 驱动 `visitor`，由格式自行判断输入类型。
    ///
    /// 返回的 future 类型由实现者通过关联类型 `DecodeAnyAsync` 指定。
    type DecodeAnyAsync<'a, V>: TrMayCancel<'a, MayCancelOutput = Result<V::Value, Self::Error>>
    where
        'de: 'a,
        V: Visitor<'de> + 'a,
        V::Value: 'a,
        Self: 'a,
        Self::Error: 'a;

    /// 驱动 `visitor`，由格式自行判断输入类型。
    ///
    /// # Errors
    ///
    /// 输入不合法或底层读取失败时返回错误。
    fn decode_any<'a, V>(self, visitor: V) -> Self::DecodeAnyAsync<'a, V>
    where
        'de: 'a,
        V: Visitor<'de> + 'a,
        V::Value: 'a,
        Self: 'a,
        Self::Error: 'a;

    /// 提示输入是一个 `bool`。
    ///
    /// 返回的 future 类型由实现者通过关联类型 `DecodeBoolAsync` 指定。
    type DecodeBoolAsync<'a, V>: TrMayCancel<'a, MayCancelOutput = Result<V::Value, Self::Error>>
    where
        'de: 'a,
        V: Visitor<'de> + 'a,
        V::Value: 'a,
        Self: 'a,
        Self::Error: 'a;

    /// 提示输入是一个 `bool`。
    ///
    /// # Errors
    ///
    /// 输入不合法或底层读取失败时返回错误。
    fn decode_bool<'a, V>(self, visitor: V) -> Self::DecodeBoolAsync<'a, V>
    where
        'de: 'a,
        V: Visitor<'de> + 'a,
        V::Value: 'a,
        Self: 'a,
        Self::Error: 'a;

    /// 提示输入是一个 `i8`。
    ///
    /// 返回的 future 类型由实现者通过关联类型 `DecodeI8Async` 指定。
    type DecodeI8Async<'a, V>: TrMayCancel<'a, MayCancelOutput = Result<V::Value, Self::Error>>
    where
        'de: 'a,
        V: Visitor<'de> + 'a,
        V::Value: 'a,
        Self: 'a,
        Self::Error: 'a;

    /// 提示输入是一个 `i8`。
    ///
    /// # Errors
    ///
    /// 输入不合法或底层读取失败时返回错误。
    fn decode_i8<'a, V>(self, visitor: V) -> Self::DecodeI8Async<'a, V>
    where
        'de: 'a,
        V: Visitor<'de> + 'a,
        V::Value: 'a,
        Self: 'a,
        Self::Error: 'a;

    /// 提示输入是一个 `i16`。
    ///
    /// 返回的 future 类型由实现者通过关联类型 `DecodeI16Async` 指定。
    type DecodeI16Async<'a, V>: TrMayCancel<'a, MayCancelOutput = Result<V::Value, Self::Error>>
    where
        'de: 'a,
        V: Visitor<'de> + 'a,
        V::Value: 'a,
        Self: 'a,
        Self::Error: 'a;

    /// 提示输入是一个 `i16`。
    ///
    /// # Errors
    ///
    /// 输入不合法或底层读取失败时返回错误。
    fn decode_i16<'a, V>(self, visitor: V) -> Self::DecodeI16Async<'a, V>
    where
        'de: 'a,
        V: Visitor<'de> + 'a,
        V::Value: 'a,
        Self: 'a,
        Self::Error: 'a;

    /// 提示输入是一个 `i32`。
    ///
    /// 返回的 future 类型由实现者通过关联类型 `DecodeI32Async` 指定。
    type DecodeI32Async<'a, V>: TrMayCancel<'a, MayCancelOutput = Result<V::Value, Self::Error>>
    where
        'de: 'a,
        V: Visitor<'de> + 'a,
        V::Value: 'a,
        Self: 'a,
        Self::Error: 'a;

    /// 提示输入是一个 `i32`。
    ///
    /// # Errors
    ///
    /// 输入不合法或底层读取失败时返回错误。
    fn decode_i32<'a, V>(self, visitor: V) -> Self::DecodeI32Async<'a, V>
    where
        'de: 'a,
        V: Visitor<'de> + 'a,
        V::Value: 'a,
        Self: 'a,
        Self::Error: 'a;

    /// 提示输入是一个 `i64`。
    ///
    /// 返回的 future 类型由实现者通过关联类型 `DecodeI64Async` 指定。
    type DecodeI64Async<'a, V>: TrMayCancel<'a, MayCancelOutput = Result<V::Value, Self::Error>>
    where
        'de: 'a,
        V: Visitor<'de> + 'a,
        V::Value: 'a,
        Self: 'a,
        Self::Error: 'a;

    /// 提示输入是一个 `i64`。
    ///
    /// # Errors
    ///
    /// 输入不合法或底层读取失败时返回错误。
    fn decode_i64<'a, V>(self, visitor: V) -> Self::DecodeI64Async<'a, V>
    where
        'de: 'a,
        V: Visitor<'de> + 'a,
        V::Value: 'a,
        Self: 'a,
        Self::Error: 'a;

    /// 提示输入是一个 `i128`。
    ///
    /// 返回的 future 类型由实现者通过关联类型 `DecodeI128Async` 指定。
    type DecodeI128Async<'a, V>: TrMayCancel<'a, MayCancelOutput = Result<V::Value, Self::Error>>
    where
        'de: 'a,
        V: Visitor<'de> + 'a,
        V::Value: 'a,
        Self: 'a,
        Self::Error: 'a;

    /// 提示输入是一个 `i128`。
    ///
    /// # Errors
    ///
    /// 输入不合法或底层读取失败时返回错误。
    fn decode_i128<'a, V>(self, visitor: V) -> Self::DecodeI128Async<'a, V>
    where
        'de: 'a,
        V: Visitor<'de> + 'a,
        V::Value: 'a,
        Self: 'a,
        Self::Error: 'a;

    /// 提示输入是一个 `u8`。
    ///
    /// 返回的 future 类型由实现者通过关联类型 `DecodeU8Async` 指定。
    type DecodeU8Async<'a, V>: TrMayCancel<'a, MayCancelOutput = Result<V::Value, Self::Error>>
    where
        'de: 'a,
        V: Visitor<'de> + 'a,
        V::Value: 'a,
        Self: 'a,
        Self::Error: 'a;

    /// 提示输入是一个 `u8`。
    ///
    /// # Errors
    ///
    /// 输入不合法或底层读取失败时返回错误。
    fn decode_u8<'a, V>(self, visitor: V) -> Self::DecodeU8Async<'a, V>
    where
        'de: 'a,
        V: Visitor<'de> + 'a,
        V::Value: 'a,
        Self: 'a,
        Self::Error: 'a;

    /// 提示输入是一个 `u16`。
    ///
    /// 返回的 future 类型由实现者通过关联类型 `DecodeU16Async` 指定。
    type DecodeU16Async<'a, V>: TrMayCancel<'a, MayCancelOutput = Result<V::Value, Self::Error>>
    where
        'de: 'a,
        V: Visitor<'de> + 'a,
        V::Value: 'a,
        Self: 'a,
        Self::Error: 'a;

    /// 提示输入是一个 `u16`。
    ///
    /// # Errors
    ///
    /// 输入不合法或底层读取失败时返回错误。
    fn decode_u16<'a, V>(self, visitor: V) -> Self::DecodeU16Async<'a, V>
    where
        'de: 'a,
        V: Visitor<'de> + 'a,
        V::Value: 'a,
        Self: 'a,
        Self::Error: 'a;

    /// 提示输入是一个 `u32`。
    ///
    /// 返回的 future 类型由实现者通过关联类型 `DecodeU32Async` 指定。
    type DecodeU32Async<'a, V>: TrMayCancel<'a, MayCancelOutput = Result<V::Value, Self::Error>>
    where
        'de: 'a,
        V: Visitor<'de> + 'a,
        V::Value: 'a,
        Self: 'a,
        Self::Error: 'a;

    /// 提示输入是一个 `u32`。
    ///
    /// # Errors
    ///
    /// 输入不合法或底层读取失败时返回错误。
    fn decode_u32<'a, V>(self, visitor: V) -> Self::DecodeU32Async<'a, V>
    where
        'de: 'a,
        V: Visitor<'de> + 'a,
        V::Value: 'a,
        Self: 'a,
        Self::Error: 'a;

    /// 提示输入是一个 `u64`。
    ///
    /// 返回的 future 类型由实现者通过关联类型 `DecodeU64Async` 指定。
    type DecodeU64Async<'a, V>: TrMayCancel<'a, MayCancelOutput = Result<V::Value, Self::Error>>
    where
        'de: 'a,
        V: Visitor<'de> + 'a,
        V::Value: 'a,
        Self: 'a,
        Self::Error: 'a;

    /// 提示输入是一个 `u64`。
    ///
    /// # Errors
    ///
    /// 输入不合法或底层读取失败时返回错误。
    fn decode_u64<'a, V>(self, visitor: V) -> Self::DecodeU64Async<'a, V>
    where
        'de: 'a,
        V: Visitor<'de> + 'a,
        V::Value: 'a,
        Self: 'a,
        Self::Error: 'a;

    /// 提示输入是一个 `u128`。
    ///
    /// 返回的 future 类型由实现者通过关联类型 `DecodeU128Async` 指定。
    type DecodeU128Async<'a, V>: TrMayCancel<'a, MayCancelOutput = Result<V::Value, Self::Error>>
    where
        'de: 'a,
        V: Visitor<'de> + 'a,
        V::Value: 'a,
        Self: 'a,
        Self::Error: 'a;

    /// 提示输入是一个 `u128`。
    ///
    /// # Errors
    ///
    /// 输入不合法或底层读取失败时返回错误。
    fn decode_u128<'a, V>(self, visitor: V) -> Self::DecodeU128Async<'a, V>
    where
        'de: 'a,
        V: Visitor<'de> + 'a,
        V::Value: 'a,
        Self: 'a,
        Self::Error: 'a;

    /// 提示输入是一个 `f32`。
    ///
    /// 返回的 future 类型由实现者通过关联类型 `DecodeF32Async` 指定。
    type DecodeF32Async<'a, V>: TrMayCancel<'a, MayCancelOutput = Result<V::Value, Self::Error>>
    where
        'de: 'a,
        V: Visitor<'de> + 'a,
        V::Value: 'a,
        Self: 'a,
        Self::Error: 'a;

    /// 提示输入是一个 `f32`。
    ///
    /// # Errors
    ///
    /// 输入不合法或底层读取失败时返回错误。
    fn decode_f32<'a, V>(self, visitor: V) -> Self::DecodeF32Async<'a, V>
    where
        'de: 'a,
        V: Visitor<'de> + 'a,
        V::Value: 'a,
        Self: 'a,
        Self::Error: 'a;

    /// 提示输入是一个 `f64`。
    ///
    /// 返回的 future 类型由实现者通过关联类型 `DecodeF64Async` 指定。
    type DecodeF64Async<'a, V>: TrMayCancel<'a, MayCancelOutput = Result<V::Value, Self::Error>>
    where
        'de: 'a,
        V: Visitor<'de> + 'a,
        V::Value: 'a,
        Self: 'a,
        Self::Error: 'a;

    /// 提示输入是一个 `f64`。
    ///
    /// # Errors
    ///
    /// 输入不合法或底层读取失败时返回错误。
    fn decode_f64<'a, V>(self, visitor: V) -> Self::DecodeF64Async<'a, V>
    where
        'de: 'a,
        V: Visitor<'de> + 'a,
        V::Value: 'a,
        Self: 'a,
        Self::Error: 'a;

    /// 提示输入是一个 `char`。
    ///
    /// 返回的 future 类型由实现者通过关联类型 `DecodeCharAsync` 指定。
    type DecodeCharAsync<'a, V>: TrMayCancel<'a, MayCancelOutput = Result<V::Value, Self::Error>>
    where
        'de: 'a,
        V: Visitor<'de> + 'a,
        V::Value: 'a,
        Self: 'a,
        Self::Error: 'a;

    /// 提示输入是一个 `char`。
    ///
    /// # Errors
    ///
    /// 输入不合法或底层读取失败时返回错误。
    fn decode_char<'a, V>(self, visitor: V) -> Self::DecodeCharAsync<'a, V>
    where
        'de: 'a,
        V: Visitor<'de> + 'a,
        V::Value: 'a,
        Self: 'a,
        Self::Error: 'a;

    /// 提示输入是一个字符串，访问者不需要取得所有权。
    ///
    /// 返回的 future 类型由实现者通过关联类型 `DecodeStrAsync` 指定。
    type DecodeStrAsync<'a, V>: TrMayCancel<'a, MayCancelOutput = Result<V::Value, Self::Error>>
    where
        'de: 'a,
        V: Visitor<'de> + 'a,
        V::Value: 'a,
        Self: 'a,
        Self::Error: 'a;

    /// 提示输入是一个字符串，访问者不需要取得所有权。
    ///
    /// # Errors
    ///
    /// 输入不合法或底层读取失败时返回错误。
    fn decode_str<'a, V>(self, visitor: V) -> Self::DecodeStrAsync<'a, V>
    where
        'de: 'a,
        V: Visitor<'de> + 'a,
        V::Value: 'a,
        Self: 'a,
        Self::Error: 'a;

    /// 提示输入是一个字符串，访问者可以取得所有权。
    ///
    /// 返回的 future 类型由实现者通过关联类型 `DecodeStringAsync` 指定。
    type DecodeStringAsync<'a, V>: TrMayCancel<'a, MayCancelOutput = Result<V::Value, Self::Error>>
    where
        'de: 'a,
        V: Visitor<'de> + 'a,
        V::Value: 'a,
        Self: 'a,
        Self::Error: 'a;

    /// 提示输入是一个字符串，访问者可以取得所有权。
    ///
    /// # Errors
    ///
    /// 输入不合法或底层读取失败时返回错误。
    fn decode_string<'a, V>(self, visitor: V) -> Self::DecodeStringAsync<'a, V>
    where
        'de: 'a,
        V: Visitor<'de> + 'a,
        V::Value: 'a,
        Self: 'a,
        Self::Error: 'a;

    /// 提示输入是一段字节，访问者不需要取得所有权。
    ///
    /// 返回的 future 类型由实现者通过关联类型 `DecodeBytesAsync` 指定。
    type DecodeBytesAsync<'a, V>: TrMayCancel<'a, MayCancelOutput = Result<V::Value, Self::Error>>
    where
        'de: 'a,
        V: Visitor<'de> + 'a,
        V::Value: 'a,
        Self: 'a,
        Self::Error: 'a;

    /// 提示输入是一段字节，访问者不需要取得所有权。
    ///
    /// # Errors
    ///
    /// 输入不合法或底层读取失败时返回错误。
    fn decode_bytes<'a, V>(self, visitor: V) -> Self::DecodeBytesAsync<'a, V>
    where
        'de: 'a,
        V: Visitor<'de> + 'a,
        V::Value: 'a,
        Self: 'a,
        Self::Error: 'a;

    /// 提示输入是一段字节，访问者可以取得所有权。
    ///
    /// 返回的 future 类型由实现者通过关联类型 `DecodeByteBufAsync` 指定。
    type DecodeByteBufAsync<'a, V>: TrMayCancel<'a, MayCancelOutput = Result<V::Value, Self::Error>>
    where
        'de: 'a,
        V: Visitor<'de> + 'a,
        V::Value: 'a,
        Self: 'a,
        Self::Error: 'a;

    /// 提示输入是一段字节，访问者可以取得所有权。
    ///
    /// # Errors
    ///
    /// 输入不合法或底层读取失败时返回错误。
    fn decode_byte_buf<'a, V>(self, visitor: V) -> Self::DecodeByteBufAsync<'a, V>
    where
        'de: 'a,
        V: Visitor<'de> + 'a,
        V::Value: 'a,
        Self: 'a,
        Self::Error: 'a;

    /// 提示输入是一个可选值。
    ///
    /// 返回的 future 类型由实现者通过关联类型 `DecodeOptionAsync` 指定。
    type DecodeOptionAsync<'a, V>: TrMayCancel<'a, MayCancelOutput = Result<V::Value, Self::Error>>
    where
        'de: 'a,
        V: Visitor<'de> + 'a,
        V::Value: 'a,
        Self: 'a,
        Self::Error: 'a;

    /// 提示输入是一个可选值。
    ///
    /// # Errors
    ///
    /// 输入不合法或底层读取失败时返回错误。
    fn decode_option<'a, V>(self, visitor: V) -> Self::DecodeOptionAsync<'a, V>
    where
        'de: 'a,
        V: Visitor<'de> + 'a,
        V::Value: 'a,
        Self: 'a,
        Self::Error: 'a;

    /// 提示输入是一个 `()`。
    ///
    /// 返回的 future 类型由实现者通过关联类型 `DecodeUnitAsync` 指定。
    type DecodeUnitAsync<'a, V>: TrMayCancel<'a, MayCancelOutput = Result<V::Value, Self::Error>>
    where
        'de: 'a,
        V: Visitor<'de> + 'a,
        V::Value: 'a,
        Self: 'a,
        Self::Error: 'a;

    /// 提示输入是一个 `()`。
    ///
    /// # Errors
    ///
    /// 输入不合法或底层读取失败时返回错误。
    fn decode_unit<'a, V>(self, visitor: V) -> Self::DecodeUnitAsync<'a, V>
    where
        'de: 'a,
        V: Visitor<'de> + 'a,
        V::Value: 'a,
        Self: 'a,
        Self::Error: 'a;

    /// 提示输入是一个单元结构体。
    ///
    /// 返回的 future 类型由实现者通过关联类型 `DecodeUnitStructAsync` 指定。
    type DecodeUnitStructAsync<'a, V>: TrMayCancel<'a, MayCancelOutput = Result<V::Value, Self::Error>>
    where
        'de: 'a,
        V: Visitor<'de> + 'a,
        V::Value: 'a,
        Self: 'a,
        Self::Error: 'a;

    /// 提示输入是一个单元结构体。
    ///
    /// # Errors
    ///
    /// 输入不合法或底层读取失败时返回错误。
    fn decode_unit_struct<'a, V>(
        self,
        name: &'static str,
        visitor: V,
    ) -> Self::DecodeUnitStructAsync<'a, V>
    where
        'de: 'a,
        V: Visitor<'de> + 'a,
        V::Value: 'a,
        Self: 'a,
        Self::Error: 'a;

    /// 提示输入是一个 newtype 结构体。
    ///
    /// 返回的 future 类型由实现者通过关联类型 `DecodeNewtypeStructAsync` 指定。
    type DecodeNewtypeStructAsync<'a, V>: TrMayCancel<'a, MayCancelOutput = Result<V::Value, Self::Error>>
    where
        'de: 'a,
        V: Visitor<'de> + 'a,
        V::Value: 'a,
        Self: 'a,
        Self::Error: 'a;

    /// 提示输入是一个 newtype 结构体。
    ///
    /// # Errors
    ///
    /// 输入不合法或底层读取失败时返回错误。
    fn decode_newtype_struct<'a, V>(
        self,
        name: &'static str,
        visitor: V,
    ) -> Self::DecodeNewtypeStructAsync<'a, V>
    where
        'de: 'a,
        V: Visitor<'de> + 'a,
        V::Value: 'a,
        Self: 'a,
        Self::Error: 'a;

    /// 提示输入是一个序列。
    ///
    /// 返回的 future 类型由实现者通过关联类型 `DecodeSeqAsync` 指定。
    type DecodeSeqAsync<'a, V>: TrMayCancel<'a, MayCancelOutput = Result<V::Value, Self::Error>>
    where
        'de: 'a,
        V: Visitor<'de> + 'a,
        V::Value: 'a,
        Self: 'a,
        Self::Error: 'a;

    /// 提示输入是一个序列。
    ///
    /// # Errors
    ///
    /// 输入不合法或底层读取失败时返回错误。
    fn decode_seq<'a, V>(self, visitor: V) -> Self::DecodeSeqAsync<'a, V>
    where
        'de: 'a,
        V: Visitor<'de> + 'a,
        V::Value: 'a,
        Self: 'a,
        Self::Error: 'a;

    /// 提示输入是一个定长元组。
    ///
    /// 返回的 future 类型由实现者通过关联类型 `DecodeTupleAsync` 指定。
    type DecodeTupleAsync<'a, V>: TrMayCancel<'a, MayCancelOutput = Result<V::Value, Self::Error>>
    where
        'de: 'a,
        V: Visitor<'de> + 'a,
        V::Value: 'a,
        Self: 'a,
        Self::Error: 'a;

    /// 提示输入是一个定长元组。
    ///
    /// # Errors
    ///
    /// 输入不合法或底层读取失败时返回错误。
    fn decode_tuple<'a, V>(self, len: usize, visitor: V) -> Self::DecodeTupleAsync<'a, V>
    where
        'de: 'a,
        V: Visitor<'de> + 'a,
        V::Value: 'a,
        Self: 'a,
        Self::Error: 'a;

    /// 提示输入是一个元组结构体。
    ///
    /// 返回的 future 类型由实现者通过关联类型 `DecodeTupleStructAsync` 指定。
    type DecodeTupleStructAsync<'a, V>: TrMayCancel<'a, MayCancelOutput = Result<V::Value, Self::Error>>
    where
        'de: 'a,
        V: Visitor<'de> + 'a,
        V::Value: 'a,
        Self: 'a,
        Self::Error: 'a;

    /// 提示输入是一个元组结构体。
    ///
    /// # Errors
    ///
    /// 输入不合法或底层读取失败时返回错误。
    fn decode_tuple_struct<'a, V>(
        self,
        name: &'static str,
        len: usize,
        visitor: V,
    ) -> Self::DecodeTupleStructAsync<'a, V>
    where
        'de: 'a,
        V: Visitor<'de> + 'a,
        V::Value: 'a,
        Self: 'a,
        Self::Error: 'a;

    /// 提示输入是一个映射。
    ///
    /// 返回的 future 类型由实现者通过关联类型 `DecodeMapAsync` 指定。
    type DecodeMapAsync<'a, V>: TrMayCancel<'a, MayCancelOutput = Result<V::Value, Self::Error>>
    where
        'de: 'a,
        V: Visitor<'de> + 'a,
        V::Value: 'a,
        Self: 'a,
        Self::Error: 'a;

    /// 提示输入是一个映射。
    ///
    /// # Errors
    ///
    /// 输入不合法或底层读取失败时返回错误。
    fn decode_map<'a, V>(self, visitor: V) -> Self::DecodeMapAsync<'a, V>
    where
        'de: 'a,
        V: Visitor<'de> + 'a,
        V::Value: 'a,
        Self: 'a,
        Self::Error: 'a;

    /// 提示输入是一个结构体。
    ///
    /// 返回的 future 类型由实现者通过关联类型 `DecodeStructAsync` 指定。
    type DecodeStructAsync<'a, V>: TrMayCancel<'a, MayCancelOutput = Result<V::Value, Self::Error>>
    where
        'de: 'a,
        V: Visitor<'de> + 'a,
        V::Value: 'a,
        Self: 'a,
        Self::Error: 'a;

    /// 提示输入是一个结构体。
    ///
    /// # Errors
    ///
    /// 输入不合法或底层读取失败时返回错误。
    fn decode_struct<'a, V>(
        self,
        name: &'static str,
        fields: &'static [&'static str],
        visitor: V,
    ) -> Self::DecodeStructAsync<'a, V>
    where
        'de: 'a,
        V: Visitor<'de> + 'a,
        V::Value: 'a,
        Self: 'a,
        Self::Error: 'a;

    /// 提示输入是一个枚举。
    ///
    /// 返回的 future 类型由实现者通过关联类型 `DecodeEnumAsync` 指定。
    type DecodeEnumAsync<'a, V>: TrMayCancel<'a, MayCancelOutput = Result<V::Value, Self::Error>>
    where
        'de: 'a,
        V: Visitor<'de> + 'a,
        V::Value: 'a,
        Self: 'a,
        Self::Error: 'a;

    /// 提示输入是一个枚举。
    ///
    /// # Errors
    ///
    /// 输入不合法或底层读取失败时返回错误。
    fn decode_enum<'a, V>(
        self,
        name: &'static str,
        variants: &'static [&'static str],
        visitor: V,
    ) -> Self::DecodeEnumAsync<'a, V>
    where
        'de: 'a,
        V: Visitor<'de> + 'a,
        V::Value: 'a,
        Self: 'a,
        Self::Error: 'a;

    /// 提示输入是结构体字段名或枚举变体名。
    ///
    /// 返回的 future 类型由实现者通过关联类型 `DecodeIdentifierAsync` 指定。
    type DecodeIdentifierAsync<'a, V>: TrMayCancel<'a, MayCancelOutput = Result<V::Value, Self::Error>>
    where
        'de: 'a,
        V: Visitor<'de> + 'a,
        V::Value: 'a,
        Self: 'a,
        Self::Error: 'a;

    /// 提示输入是结构体字段名或枚举变体名。
    ///
    /// # Errors
    ///
    /// 输入不合法或底层读取失败时返回错误。
    fn decode_identifier<'a, V>(self, visitor: V) -> Self::DecodeIdentifierAsync<'a, V>
    where
        'de: 'a,
        V: Visitor<'de> + 'a,
        V::Value: 'a,
        Self: 'a,
        Self::Error: 'a;

    /// 提示输入的值会被忽略。
    ///
    /// 返回的 future 类型由实现者通过关联类型 `DecodeIgnoredAnyAsync` 指定。
    type DecodeIgnoredAnyAsync<'a, V>: TrMayCancel<'a, MayCancelOutput = Result<V::Value, Self::Error>>
    where
        'de: 'a,
        V: Visitor<'de> + 'a,
        V::Value: 'a,
        Self: 'a,
        Self::Error: 'a;

    /// 提示输入的值会被忽略。
    ///
    /// # Errors
    ///
    /// 输入不合法或底层读取失败时返回错误。
    fn decode_ignored_any<'a, V>(self, visitor: V) -> Self::DecodeIgnoredAnyAsync<'a, V>
    where
        'de: 'a,
        V: Visitor<'de> + 'a,
        V::Value: 'a,
        Self: 'a,
        Self::Error: 'a;

    /// 判断数据格式是否期望人类可读的表示。
    ///
    /// 默认返回 `true`。纯同步判断，不需要取消令牌。
    fn is_human_readable(&self) -> bool {
        true
    }
}
