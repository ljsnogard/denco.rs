//! 编码器以及各类复合值的编码状态机。

use core::future;

use abs_cancel::TrMayCancel;

use super::{Error, TrEncodable};

/// 一个可以把数据结构写成某种格式的编码器。
///
/// 本 trait 是同步版 `serde::Serializer` 的异步版本。每个 `encode_*`
/// 方法都消耗 `self`，并返回一个 [`TrMayCancel`] 的实现；future 完成后得到
/// `Result<Self::Ok, Self::Error>`。每个方法都有独立的 GAT 关联类型，
/// 实现者可以按需给 future 附加 `Send` 等约束。
///
/// 复合值（序列、映射、结构体等）通过 [`TrEncoder::encode_seq`] 等
/// 方法取得一个状态机对象；这些对象的方法同样是异步的。
///
/// # Errors
///
/// 所有 `encode_*` 方法在数据格式拒绝该值、底层 I/O 失败，或实现者响应
/// 取消信号时返回 `Self::Error`。`i128` / `u128` 没有默认实现；格式可以
/// 自行决定支持还是返回错误。
pub trait TrEncoder: Sized {
    /// 序列化成功时产生的结果类型。
    type Ok;

    /// 序列化失败时产生的错误类型。
    type Error: Error;

    /// 序列化序列的状态机。
    type EncodeSeq: TrEncodeSeq<Ok = Self::Ok, Error = Self::Error>;

    /// 序列化定长元组的状态机。
    type EncodeTuple: TrEncodeTuple<Ok = Self::Ok, Error = Self::Error>;

    /// 序列化元组结构体的状态机。
    type EncodeTupleStruct: TrEncodeTupleStruct<Ok = Self::Ok, Error = Self::Error>;

    /// 序列化元组枚举变体的状态机。
    type EncodeTupleVariant: TrEncodeTupleVariant<Ok = Self::Ok, Error = Self::Error>;

    /// 序列化映射的状态机。
    type EncodeMap: TrEncodeMap<Ok = Self::Ok, Error = Self::Error>;

    /// 序列化结构体的状态机。
    type EncodeStruct: TrEncodeStruct<Ok = Self::Ok, Error = Self::Error>;

    /// 序列化结构体枚举变体的状态机。
    type EncodeStructVariant: TrEncodeStructVariant<Ok = Self::Ok, Error = Self::Error>;

    /// 序列化一个 `bool`。
    type EncodeBoolAsync<'a>: TrMayCancel<'a, MayCancelOutput = Result<Self::Ok, Self::Error>>
    where
        Self: 'a,
        Self::Error: 'a,
        Self::Ok: 'a;

    /// 序列化一个 `bool`。
    fn encode_bool<'a>(self, v: bool) -> Self::EncodeBoolAsync<'a>
    where
        Self: 'a,
        Self::Error: 'a,
        Self::Ok: 'a;

    /// 序列化一个 `i8`。
    type EncodeI8Async<'a>: TrMayCancel<'a, MayCancelOutput = Result<Self::Ok, Self::Error>>
    where
        Self: 'a,
        Self::Error: 'a,
        Self::Ok: 'a;

    /// 序列化一个 `i8`。
    fn encode_i8<'a>(self, v: i8) -> Self::EncodeI8Async<'a>
    where
        Self: 'a,
        Self::Error: 'a,
        Self::Ok: 'a;

    /// 序列化一个 `i16`。
    type EncodeI16Async<'a>: TrMayCancel<'a, MayCancelOutput = Result<Self::Ok, Self::Error>>
    where
        Self: 'a,
        Self::Error: 'a,
        Self::Ok: 'a;

    /// 序列化一个 `i16`。
    fn encode_i16<'a>(self, v: i16) -> Self::EncodeI16Async<'a>
    where
        Self: 'a,
        Self::Error: 'a,
        Self::Ok: 'a;

    /// 序列化一个 `i32`。
    type EncodeI32Async<'a>: TrMayCancel<'a, MayCancelOutput = Result<Self::Ok, Self::Error>>
    where
        Self: 'a,
        Self::Error: 'a,
        Self::Ok: 'a;

    /// 序列化一个 `i32`。
    fn encode_i32<'a>(self, v: i32) -> Self::EncodeI32Async<'a>
    where
        Self: 'a,
        Self::Error: 'a,
        Self::Ok: 'a;

    /// 序列化一个 `i64`。
    type EncodeI64Async<'a>: TrMayCancel<'a, MayCancelOutput = Result<Self::Ok, Self::Error>>
    where
        Self: 'a,
        Self::Error: 'a,
        Self::Ok: 'a;

    /// 序列化一个 `i64`。
    fn encode_i64<'a>(self, v: i64) -> Self::EncodeI64Async<'a>
    where
        Self: 'a,
        Self::Error: 'a,
        Self::Ok: 'a;

    /// 序列化一个 `i128`。
    ///
    /// 是否支持 128 位整数由格式决定；不支持时应返回 `Self::Error`。
    type EncodeI128Async<'a>: TrMayCancel<'a, MayCancelOutput = Result<Self::Ok, Self::Error>>
    where
        Self: 'a,
        Self::Error: 'a,
        Self::Ok: 'a;

    /// 序列化一个 `i128`。
    ///
    /// 是否支持 128 位整数由格式决定；不支持时应返回 `Self::Error`。
    fn encode_i128<'a>(self, v: i128) -> Self::EncodeI128Async<'a>
    where
        Self: 'a,
        Self::Error: 'a,
        Self::Ok: 'a;

    /// 序列化一个 `u8`。
    type EncodeU8Async<'a>: TrMayCancel<'a, MayCancelOutput = Result<Self::Ok, Self::Error>>
    where
        Self: 'a,
        Self::Error: 'a,
        Self::Ok: 'a;

    /// 序列化一个 `u8`。
    fn encode_u8<'a>(self, v: u8) -> Self::EncodeU8Async<'a>
    where
        Self: 'a,
        Self::Error: 'a,
        Self::Ok: 'a;

    /// 序列化一个 `u16`。
    type EncodeU16Async<'a>: TrMayCancel<'a, MayCancelOutput = Result<Self::Ok, Self::Error>>
    where
        Self: 'a,
        Self::Error: 'a,
        Self::Ok: 'a;

    /// 序列化一个 `u16`。
    fn encode_u16<'a>(self, v: u16) -> Self::EncodeU16Async<'a>
    where
        Self: 'a,
        Self::Error: 'a,
        Self::Ok: 'a;

    /// 序列化一个 `u32`。
    type EncodeU32Async<'a>: TrMayCancel<'a, MayCancelOutput = Result<Self::Ok, Self::Error>>
    where
        Self: 'a,
        Self::Error: 'a,
        Self::Ok: 'a;

    /// 序列化一个 `u32`。
    fn encode_u32<'a>(self, v: u32) -> Self::EncodeU32Async<'a>
    where
        Self: 'a,
        Self::Error: 'a,
        Self::Ok: 'a;

    /// 序列化一个 `u64`。
    type EncodeU64Async<'a>: TrMayCancel<'a, MayCancelOutput = Result<Self::Ok, Self::Error>>
    where
        Self: 'a,
        Self::Error: 'a,
        Self::Ok: 'a;

    /// 序列化一个 `u64`。
    fn encode_u64<'a>(self, v: u64) -> Self::EncodeU64Async<'a>
    where
        Self: 'a,
        Self::Error: 'a,
        Self::Ok: 'a;

    /// 序列化一个 `u128`。
    ///
    /// 是否支持 128 位整数由格式决定；不支持时应返回 `Self::Error`。
    type EncodeU128Async<'a>: TrMayCancel<'a, MayCancelOutput = Result<Self::Ok, Self::Error>>
    where
        Self: 'a,
        Self::Error: 'a,
        Self::Ok: 'a;

    /// 序列化一个 `u128`。
    ///
    /// 是否支持 128 位整数由格式决定；不支持时应返回 `Self::Error`。
    fn encode_u128<'a>(self, v: u128) -> Self::EncodeU128Async<'a>
    where
        Self: 'a,
        Self::Error: 'a,
        Self::Ok: 'a;

    /// 序列化一个 `f32`。
    type EncodeF32Async<'a>: TrMayCancel<'a, MayCancelOutput = Result<Self::Ok, Self::Error>>
    where
        Self: 'a,
        Self::Error: 'a,
        Self::Ok: 'a;

    /// 序列化一个 `f32`。
    fn encode_f32<'a>(self, v: f32) -> Self::EncodeF32Async<'a>
    where
        Self: 'a,
        Self::Error: 'a,
        Self::Ok: 'a;

    /// 序列化一个 `f64`。
    type EncodeF64Async<'a>: TrMayCancel<'a, MayCancelOutput = Result<Self::Ok, Self::Error>>
    where
        Self: 'a,
        Self::Error: 'a,
        Self::Ok: 'a;

    /// 序列化一个 `f64`。
    fn encode_f64<'a>(self, v: f64) -> Self::EncodeF64Async<'a>
    where
        Self: 'a,
        Self::Error: 'a,
        Self::Ok: 'a;

    /// 序列化一个 `char`。
    type EncodeCharAsync<'a>: TrMayCancel<'a, MayCancelOutput = Result<Self::Ok, Self::Error>>
    where
        Self: 'a,
        Self::Error: 'a,
        Self::Ok: 'a;

    /// 序列化一个 `char`。
    fn encode_char<'a>(self, v: char) -> Self::EncodeCharAsync<'a>
    where
        Self: 'a,
        Self::Error: 'a,
        Self::Ok: 'a;

    /// 序列化一个字符串切片。
    type EncodeStrAsync<'a, 'b>: TrMayCancel<'a, MayCancelOutput = Result<Self::Ok, Self::Error>>
    where
        Self: 'a,
        Self::Error: 'a,
        'b: 'a,
        Self::Ok: 'a;

    /// 序列化一个字符串切片。
    fn encode_str<'a, 'b>(self, v: &'b str) -> Self::EncodeStrAsync<'a, 'b>
    where
        Self: 'a,
        Self::Error: 'a,
        'b: 'a,
        Self::Ok: 'a;

    /// 序列化一段字节。
    type EncodeBytesAsync<'a, 'b>: TrMayCancel<'a, MayCancelOutput = Result<Self::Ok, Self::Error>>
    where
        Self: 'a,
        Self::Error: 'a,
        'b: 'a,
        Self::Ok: 'a;

    /// 序列化一段字节。
    fn encode_bytes<'a, 'b>(self, v: &'b [u8]) -> Self::EncodeBytesAsync<'a, 'b>
    where
        Self: 'a,
        Self::Error: 'a,
        'b: 'a,
        Self::Ok: 'a;

    /// 序列化 `None`。
    type EncodeNoneAsync<'a>: TrMayCancel<'a, MayCancelOutput = Result<Self::Ok, Self::Error>>
    where
        Self: 'a,
        Self::Error: 'a,
        Self::Ok: 'a;

    /// 序列化 `None`。
    fn encode_none<'a>(self) -> Self::EncodeNoneAsync<'a>
    where
        Self: 'a,
        Self::Error: 'a,
        Self::Ok: 'a;

    /// 序列化 `Some(value)` 中的值。
    type EncodeSomeAsync<'a, 'b, T>: TrMayCancel<'a, MayCancelOutput = Result<Self::Ok, Self::Error>>
    where
        T: ?Sized + TrEncodable,
        T: 'a,
        T: 'b,
        Self: 'a,
        Self::Error: 'a,
        'b: 'a,
        Self::Ok: 'a;

    /// 序列化 `Some(value)` 中的值。
    fn encode_some<'a, 'b, T>(self, value: &'b T) -> Self::EncodeSomeAsync<'a, 'b, T>
    where
        T: ?Sized + TrEncodable,
        T: 'a,
        T: 'b,
        Self: 'a,
        Self::Error: 'a,
        'b: 'a,
        Self::Ok: 'a;

    /// 序列化 `()`。
    type EncodeUnitAsync<'a>: TrMayCancel<'a, MayCancelOutput = Result<Self::Ok, Self::Error>>
    where
        Self: 'a,
        Self::Error: 'a,
        Self::Ok: 'a;

    /// 序列化 `()`。
    fn encode_unit<'a>(self) -> Self::EncodeUnitAsync<'a>
    where
        Self: 'a,
        Self::Error: 'a,
        Self::Ok: 'a;

    /// 序列化一个单元结构体。
    type EncodeUnitStructAsync<'a>: TrMayCancel<'a, MayCancelOutput = Result<Self::Ok, Self::Error>>
    where
        Self: 'a,
        Self::Error: 'a,
        Self::Ok: 'a;

    /// 序列化一个单元结构体。
    fn encode_unit_struct<'a>(self, name: &'static str) -> Self::EncodeUnitStructAsync<'a>
    where
        Self: 'a,
        Self::Error: 'a,
        Self::Ok: 'a;

    /// 序列化一个单元枚举变体。
    type EncodeUnitVariantAsync<'a>: TrMayCancel<'a, MayCancelOutput = Result<Self::Ok, Self::Error>>
    where
        Self: 'a,
        Self::Error: 'a,
        Self::Ok: 'a;

    /// 序列化一个单元枚举变体。
    fn encode_unit_variant<'a>(
        self,
        name: &'static str,
        variant_index: u32,
        variant: &'static str,
    ) -> Self::EncodeUnitVariantAsync<'a>
    where
        Self: 'a,
        Self::Error: 'a,
        Self::Ok: 'a;

    /// 序列化一个 newtype 结构体。
    type EncodeNewtypeStructAsync<'a, 'b, T>: TrMayCancel<'a, MayCancelOutput = Result<Self::Ok, Self::Error>>
    where
        T: ?Sized + TrEncodable,
        T: 'a,
        T: 'b,
        Self: 'a,
        Self::Error: 'a,
        'b: 'a,
        Self::Ok: 'a;

    /// 序列化一个 newtype 结构体。
    fn encode_newtype_struct<'a, 'b, T>(
        self,
        name: &'static str,
        value: &'b T,
    ) -> Self::EncodeNewtypeStructAsync<'a, 'b, T>
    where
        T: ?Sized + TrEncodable,
        T: 'a,
        T: 'b,
        Self: 'a,
        Self::Error: 'a,
        'b: 'a,
        Self::Ok: 'a;

    /// 序列化一个 newtype 枚举变体。
    type EncodeNewtypeVariantAsync<'a, 'b, T>: TrMayCancel<'a, MayCancelOutput = Result<Self::Ok, Self::Error>>
    where
        T: ?Sized + TrEncodable,
        T: 'a,
        T: 'b,
        Self: 'a,
        Self::Error: 'a,
        'b: 'a,
        Self::Ok: 'a;

    /// 序列化一个 newtype 枚举变体。
    fn encode_newtype_variant<'a, 'b, T>(
        self,
        name: &'static str,
        variant_index: u32,
        variant: &'static str,
        value: &'b T,
    ) -> Self::EncodeNewtypeVariantAsync<'a, 'b, T>
    where
        T: ?Sized + TrEncodable,
        T: 'a,
        T: 'b,
        Self: 'a,
        Self::Error: 'a,
        'b: 'a,
        Self::Ok: 'a;

    /// 开始序列化一个序列。
    type EncodeSeqAsync<'a>: TrMayCancel<'a, MayCancelOutput = Result<Self::EncodeSeq, Self::Error>>
    where
        Self: 'a,
        Self::Error: 'a,
        Self::EncodeSeq: 'a;

    /// 开始序列化一个序列。
    fn encode_seq<'a>(self, len: Option<usize>) -> Self::EncodeSeqAsync<'a>
    where
        Self: 'a,
        Self::Error: 'a,
        Self::EncodeSeq: 'a;

    /// 开始序列化一个定长元组。
    type EncodeTupleAsync<'a>: TrMayCancel<'a, MayCancelOutput = Result<Self::EncodeTuple, Self::Error>>
    where
        Self: 'a,
        Self::Error: 'a,
        Self::EncodeTuple: 'a;

    /// 开始序列化一个定长元组。
    fn encode_tuple<'a>(self, len: usize) -> Self::EncodeTupleAsync<'a>
    where
        Self: 'a,
        Self::Error: 'a,
        Self::EncodeTuple: 'a;

    /// 开始序列化一个元组结构体。
    type EncodeTupleStructAsync<'a>: TrMayCancel<'a, MayCancelOutput = Result<Self::EncodeTupleStruct, Self::Error>>
    where
        Self: 'a,
        Self::Error: 'a,
        Self::EncodeTupleStruct: 'a;

    /// 开始序列化一个元组结构体。
    fn encode_tuple_struct<'a>(
        self,
        name: &'static str,
        len: usize,
    ) -> Self::EncodeTupleStructAsync<'a>
    where
        Self: 'a,
        Self::Error: 'a,
        Self::EncodeTupleStruct: 'a;

    /// 开始序列化一个元组枚举变体。
    type EncodeTupleVariantAsync<'a>: TrMayCancel<'a, MayCancelOutput = Result<Self::EncodeTupleVariant, Self::Error>>
    where
        Self: 'a,
        Self::Error: 'a,
        Self::EncodeTupleVariant: 'a;

    /// 开始序列化一个元组枚举变体。
    fn encode_tuple_variant<'a>(
        self,
        name: &'static str,
        variant_index: u32,
        variant: &'static str,
        len: usize,
    ) -> Self::EncodeTupleVariantAsync<'a>
    where
        Self: 'a,
        Self::Error: 'a,
        Self::EncodeTupleVariant: 'a;

    /// 开始序列化一个映射。
    type EncodeMapAsync<'a>: TrMayCancel<'a, MayCancelOutput = Result<Self::EncodeMap, Self::Error>>
    where
        Self: 'a,
        Self::Error: 'a,
        Self::EncodeMap: 'a;

    /// 开始序列化一个映射。
    fn encode_map<'a>(self, len: Option<usize>) -> Self::EncodeMapAsync<'a>
    where
        Self: 'a,
        Self::Error: 'a,
        Self::EncodeMap: 'a;

    /// 开始序列化一个结构体。
    type EncodeStructAsync<'a>: TrMayCancel<'a, MayCancelOutput = Result<Self::EncodeStruct, Self::Error>>
    where
        Self: 'a,
        Self::Error: 'a,
        Self::EncodeStruct: 'a;

    /// 开始序列化一个结构体。
    fn encode_struct<'a>(self, name: &'static str, len: usize) -> Self::EncodeStructAsync<'a>
    where
        Self: 'a,
        Self::Error: 'a,
        Self::EncodeStruct: 'a;

    /// 开始序列化一个结构体枚举变体。
    type EncodeStructVariantAsync<'a>: TrMayCancel<'a, MayCancelOutput = Result<Self::EncodeStructVariant, Self::Error>>
    where
        Self: 'a,
        Self::Error: 'a,
        Self::EncodeStructVariant: 'a;

    /// 开始序列化一个结构体枚举变体。
    fn encode_struct_variant<'a>(
        self,
        name: &'static str,
        variant_index: u32,
        variant: &'static str,
        len: usize,
    ) -> Self::EncodeStructVariantAsync<'a>
    where
        Self: 'a,
        Self::Error: 'a,
        Self::EncodeStructVariant: 'a;

    /// 判断数据格式是否期望人类可读的表示。
    ///
    /// 默认返回 `true`。纯同步判断，不需要取消令牌。
    fn is_human_readable(&self) -> bool {
        true
    }
}

/// 序列化序列时的状态机。
pub trait TrEncodeSeq {
    /// 序列化成功时产生的结果类型。
    type Ok;

    /// 序列化失败时产生的错误类型。
    type Error: Error;

    /// 序列化序列中的一个元素。
    ///
    /// # Errors
    ///
    /// 默认实现返回「未实现」错误；具体格式应覆盖本方法。
    fn encode_element<'a, 'b, T>(
        &'a mut self,
        value: &'b T,
    ) -> impl TrMayCancel<'a, MayCancelOutput = Result<(), Self::Error>>
    where
        T: ?Sized + TrEncodable,
        T: 'a,
        T: 'b,
        Self: 'a,
        Self::Error: 'a,
        'b: 'a,
    {
        let _ = value;
        future::ready(Err(<Self::Error as Error>::custom(
            "TrEncodeSeq::encode_element is not implemented",
        )))
    }

    /// 结束序列的序列化。
    ///
    /// # Errors
    ///
    /// 当格式在收尾时发现长度不符或底层 I/O 失败时返回错误。
    fn end<'a>(self) -> impl TrMayCancel<'a, MayCancelOutput = Result<Self::Ok, Self::Error>>
    where
        Self: 'a,
        Self::Ok: 'a,
        Self::Error: 'a;
}

/// 序列化定长元组时的状态机。
pub trait TrEncodeTuple {
    /// 序列化成功时产生的结果类型。
    type Ok;

    /// 序列化失败时产生的错误类型。
    type Error: Error;

    /// 序列化元组中的一个元素。
    ///
    /// # Errors
    ///
    /// 默认实现返回「未实现」错误；具体格式应覆盖本方法。
    fn encode_element<'a, 'b, T>(
        &'a mut self,
        value: &'b T,
    ) -> impl TrMayCancel<'a, MayCancelOutput = Result<(), Self::Error>>
    where
        T: ?Sized + TrEncodable,
        T: 'a,
        T: 'b,
        Self: 'a,
        Self::Error: 'a,
        'b: 'a,
    {
        let _ = value;
        future::ready(Err(<Self::Error as Error>::custom(
            "TrEncodeTuple::encode_element is not implemented",
        )))
    }

    /// 结束元组的序列化。
    ///
    /// # Errors
    ///
    /// 当格式在收尾时发现长度不符或底层 I/O 失败时返回错误。
    fn end<'a>(self) -> impl TrMayCancel<'a, MayCancelOutput = Result<Self::Ok, Self::Error>>
    where
        Self: 'a,
        Self::Ok: 'a,
        Self::Error: 'a;
}

/// 序列化元组结构体时的状态机。
pub trait TrEncodeTupleStruct {
    /// 序列化成功时产生的结果类型。
    type Ok;

    /// 序列化失败时产生的错误类型。
    type Error: Error;

    /// 序列化元组结构体中的一个字段。
    ///
    /// # Errors
    ///
    /// 默认实现返回「未实现」错误；具体格式应覆盖本方法。
    fn encode_field<'a, 'b, T>(
        &'a mut self,
        value: &'b T,
    ) -> impl TrMayCancel<'a, MayCancelOutput = Result<(), Self::Error>>
    where
        T: ?Sized + TrEncodable,
        T: 'a,
        T: 'b,
        Self: 'a,
        Self::Error: 'a,
        'b: 'a,
    {
        let _ = value;
        future::ready(Err(<Self::Error as Error>::custom(
            "TrEncodeTupleStruct::encode_field is not implemented",
        )))
    }

    /// 结束元组结构体的序列化。
    ///
    /// # Errors
    ///
    /// 当格式在收尾时发现字段数不符或底层 I/O 失败时返回错误。
    fn end<'a>(self) -> impl TrMayCancel<'a, MayCancelOutput = Result<Self::Ok, Self::Error>>
    where
        Self: 'a,
        Self::Ok: 'a,
        Self::Error: 'a;
}

/// 序列化元组枚举变体时的状态机。
pub trait TrEncodeTupleVariant {
    /// 序列化成功时产生的结果类型。
    type Ok;

    /// 序列化失败时产生的错误类型。
    type Error: Error;

    /// 序列化元组变体中的一个字段。
    ///
    /// # Errors
    ///
    /// 默认实现返回「未实现」错误；具体格式应覆盖本方法。
    fn encode_field<'a, 'b, T>(
        &'a mut self,
        value: &'b T,
    ) -> impl TrMayCancel<'a, MayCancelOutput = Result<(), Self::Error>>
    where
        T: ?Sized + TrEncodable,
        T: 'a,
        T: 'b,
        Self: 'a,
        Self::Error: 'a,
        'b: 'a,
    {
        let _ = value;
        future::ready(Err(<Self::Error as Error>::custom(
            "TrEncodeTupleVariant::encode_field is not implemented",
        )))
    }

    /// 结束元组变体的序列化。
    ///
    /// # Errors
    ///
    /// 当格式在收尾时发现字段数不符或底层 I/O 失败时返回错误。
    fn end<'a>(self) -> impl TrMayCancel<'a, MayCancelOutput = Result<Self::Ok, Self::Error>>
    where
        Self: 'a,
        Self::Ok: 'a,
        Self::Error: 'a;
}

/// 序列化映射时的状态机。
pub trait TrEncodeMap {
    /// 序列化成功时产生的结果类型。
    type Ok;

    /// 序列化失败时产生的错误类型。
    type Error: Error;

    /// 序列化映射中的一个键。
    ///
    /// # Errors
    ///
    /// 默认实现返回「未实现」错误；具体格式应覆盖本方法。
    fn encode_key<'a, 'b, T>(
        &'a mut self,
        key: &'b T,
    ) -> impl TrMayCancel<'a, MayCancelOutput = Result<(), Self::Error>>
    where
        T: ?Sized + TrEncodable,
        T: 'a,
        T: 'b,
        Self: 'a,
        Self::Error: 'a,
        'b: 'a,
    {
        let _ = key;
        future::ready(Err(<Self::Error as Error>::custom(
            "TrEncodeMap::encode_key is not implemented",
        )))
    }

    /// 序列化映射中的一个值。
    ///
    /// # Errors
    ///
    /// 默认实现返回「未实现」错误；具体格式应覆盖本方法。
    fn encode_value<'a, 'b, T>(
        &'a mut self,
        value: &'b T,
    ) -> impl TrMayCancel<'a, MayCancelOutput = Result<(), Self::Error>>
    where
        T: ?Sized + TrEncodable,
        T: 'a,
        T: 'b,
        Self: 'a,
        Self::Error: 'a,
        'b: 'a,
    {
        let _ = value;
        future::ready(Err(<Self::Error as Error>::custom(
            "TrEncodeMap::encode_value is not implemented",
        )))
    }

    /// 结束映射的序列化。
    ///
    /// # Errors
    ///
    /// 当格式在收尾时发现键值不成对、长度不符或底层 I/O 失败时返回错误。
    fn end<'a>(self) -> impl TrMayCancel<'a, MayCancelOutput = Result<Self::Ok, Self::Error>>
    where
        Self: 'a,
        Self::Ok: 'a,
        Self::Error: 'a;
}

/// 序列化结构体时的状态机。
pub trait TrEncodeStruct {
    /// 序列化成功时产生的结果类型。
    type Ok;

    /// 序列化失败时产生的错误类型。
    type Error: Error;

    /// 序列化结构体的一个字段。
    ///
    /// # Errors
    ///
    /// 默认实现返回「未实现」错误；具体格式应覆盖本方法。
    fn encode_field<'a, 'b, T>(
        &'a mut self,
        key: &'static str,
        value: &'b T,
    ) -> impl TrMayCancel<'a, MayCancelOutput = Result<(), Self::Error>>
    where
        T: ?Sized + TrEncodable,
        T: 'a,
        T: 'b,
        Self: 'a,
        Self::Error: 'a,
        'b: 'a,
    {
        let _ = (key, value);
        future::ready(Err(<Self::Error as Error>::custom(
            "TrEncodeStruct::encode_field is not implemented",
        )))
    }

    /// 跳过结构体的一个字段。
    ///
    /// 默认实现什么都不做并返回 `Ok(())`。
    ///
    /// # Errors
    ///
    /// 默认实现不会返回错误；覆盖本方法的格式可以报告自身错误。
    fn skip_field<'a>(
        &'a mut self,
        key: &'static str,
    ) -> impl TrMayCancel<'a, MayCancelOutput = Result<(), Self::Error>>
    where
        Self: 'a,
        Self::Error: 'a,
    {
        let _ = key;
        future::ready(Ok(()))
    }

    /// 结束结构体的序列化。
    ///
    /// # Errors
    ///
    /// 当格式在收尾时发现字段数不符或底层 I/O 失败时返回错误。
    fn end<'a>(self) -> impl TrMayCancel<'a, MayCancelOutput = Result<Self::Ok, Self::Error>>
    where
        Self: 'a,
        Self::Ok: 'a,
        Self::Error: 'a;
}

/// 序列化结构体枚举变体时的状态机。
pub trait TrEncodeStructVariant {
    /// 序列化成功时产生的结果类型。
    type Ok;

    /// 序列化失败时产生的错误类型。
    type Error: Error;

    /// 序列化结构体变体的一个字段。
    ///
    /// # Errors
    ///
    /// 默认实现返回「未实现」错误；具体格式应覆盖本方法。
    fn encode_field<'a, 'b, T>(
        &'a mut self,
        key: &'static str,
        value: &'b T,
    ) -> impl TrMayCancel<'a, MayCancelOutput = Result<(), Self::Error>>
    where
        T: ?Sized + TrEncodable,
        T: 'a,
        T: 'b,
        Self: 'a,
        Self::Error: 'a,
        'b: 'a,
    {
        let _ = (key, value);
        future::ready(Err(<Self::Error as Error>::custom(
            "TrEncodeStructVariant::encode_field is not implemented",
        )))
    }

    /// 跳过结构体变体的一个字段。
    ///
    /// 默认实现什么都不做并返回 `Ok(())`。
    ///
    /// # Errors
    ///
    /// 默认实现不会返回错误；覆盖本方法的格式可以报告自身错误。
    fn skip_field<'a>(
        &'a mut self,
        key: &'static str,
    ) -> impl TrMayCancel<'a, MayCancelOutput = Result<(), Self::Error>>
    where
        Self: 'a,
        Self::Error: 'a,
    {
        let _ = key;
        future::ready(Ok(()))
    }

    /// 结束结构体变体的序列化。
    ///
    /// # Errors
    ///
    /// 当格式在收尾时发现字段数不符或底层 I/O 失败时返回错误。
    fn end<'a>(self) -> impl TrMayCancel<'a, MayCancelOutput = Result<Self::Ok, Self::Error>>
    where
        Self: 'a,
        Self::Ok: 'a,
        Self::Error: 'a;
}
