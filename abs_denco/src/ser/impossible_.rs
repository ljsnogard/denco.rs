//! 一个永远不会成功的序列化器。

// 生成的 impl 需要保持与 trait 完全一致的参数列表，但所有参数都不会被使用。
#![allow(unused_variables)]

use core::{future, marker::PhantomData};

use super::{
    Error, TrEncodable, TrEncodeMap, TrEncodeSeq, TrEncodeStruct, TrEncodeStructVariant,
    TrEncodeTuple, TrEncodeTupleStruct, TrEncodeTupleVariant, TrEncoder,
};

/// 一个拒绝所有序列化请求的序列化器。
///
/// 它适合作为「该格式不支持复合值」时的占位实现：所有方法都返回
/// [`Error::custom`] 构造的错误。
pub struct Impossible<Ok, Err> {
    _marker_: PhantomData<fn() -> (Ok, Err)>,
}

impl<Ok, Err> Impossible<Ok, Err> {
    /// 创建一个 `Impossible`。
    pub const fn new() -> Self {
        Self {
            _marker_: PhantomData,
        }
    }
}

impl<Ok, Err> Default for Impossible<Ok, Err> {
    fn default() -> Self {
        Self::new()
    }
}

impl<Ok, Err> TrEncoder for Impossible<Ok, Err>
where
    Err: Error,
{
    type Ok = Ok;
    type Error = Err;

    type EncodeSeq = Self;
    type EncodeTuple = Self;
    type EncodeTupleStruct = Self;
    type EncodeTupleVariant = Self;
    type EncodeMap = Self;
    type EncodeStruct = Self;
    type EncodeStructVariant = Self;

    type EncodeBoolAsync<'a>
        = future::Ready<Result<Self::Ok, Self::Error>>
    where
        Self: 'a,
        Self::Ok: 'a,
        Self::Error: 'a;

    type EncodeI8Async<'a>
        = future::Ready<Result<Self::Ok, Self::Error>>
    where
        Self: 'a,
        Self::Ok: 'a,
        Self::Error: 'a;

    type EncodeI16Async<'a>
        = future::Ready<Result<Self::Ok, Self::Error>>
    where
        Self: 'a,
        Self::Ok: 'a,
        Self::Error: 'a;

    type EncodeI32Async<'a>
        = future::Ready<Result<Self::Ok, Self::Error>>
    where
        Self: 'a,
        Self::Ok: 'a,
        Self::Error: 'a;

    type EncodeI64Async<'a>
        = future::Ready<Result<Self::Ok, Self::Error>>
    where
        Self: 'a,
        Self::Ok: 'a,
        Self::Error: 'a;

    type EncodeI128Async<'a>
        = future::Ready<Result<Self::Ok, Self::Error>>
    where
        Self: 'a,
        Self::Ok: 'a,
        Self::Error: 'a;

    type EncodeU8Async<'a>
        = future::Ready<Result<Self::Ok, Self::Error>>
    where
        Self: 'a,
        Self::Ok: 'a,
        Self::Error: 'a;

    type EncodeU16Async<'a>
        = future::Ready<Result<Self::Ok, Self::Error>>
    where
        Self: 'a,
        Self::Ok: 'a,
        Self::Error: 'a;

    type EncodeU32Async<'a>
        = future::Ready<Result<Self::Ok, Self::Error>>
    where
        Self: 'a,
        Self::Ok: 'a,
        Self::Error: 'a;

    type EncodeU64Async<'a>
        = future::Ready<Result<Self::Ok, Self::Error>>
    where
        Self: 'a,
        Self::Ok: 'a,
        Self::Error: 'a;

    type EncodeU128Async<'a>
        = future::Ready<Result<Self::Ok, Self::Error>>
    where
        Self: 'a,
        Self::Ok: 'a,
        Self::Error: 'a;

    type EncodeF32Async<'a>
        = future::Ready<Result<Self::Ok, Self::Error>>
    where
        Self: 'a,
        Self::Ok: 'a,
        Self::Error: 'a;

    type EncodeF64Async<'a>
        = future::Ready<Result<Self::Ok, Self::Error>>
    where
        Self: 'a,
        Self::Ok: 'a,
        Self::Error: 'a;

    type EncodeCharAsync<'a>
        = future::Ready<Result<Self::Ok, Self::Error>>
    where
        Self: 'a,
        Self::Ok: 'a,
        Self::Error: 'a;

    type EncodeStrAsync<'a, 'b>
        = future::Ready<Result<Self::Ok, Self::Error>>
    where
        Self: 'a,
        'b: 'a,
        Self::Ok: 'a,
        Self::Error: 'a;

    type EncodeBytesAsync<'a, 'b>
        = future::Ready<Result<Self::Ok, Self::Error>>
    where
        Self: 'a,
        'b: 'a,
        Self::Ok: 'a,
        Self::Error: 'a;

    type EncodeNoneAsync<'a>
        = future::Ready<Result<Self::Ok, Self::Error>>
    where
        Self: 'a,
        Self::Ok: 'a,
        Self::Error: 'a;

    type EncodeSomeAsync<'a, 'b, T>
        = future::Ready<Result<Self::Ok, Self::Error>>
    where
        T: ?Sized + TrEncodable,
        T: 'a,
        T: 'b,
        Self: 'a,
        'b: 'a,
        Self::Ok: 'a,
        Self::Error: 'a;

    type EncodeUnitAsync<'a>
        = future::Ready<Result<Self::Ok, Self::Error>>
    where
        Self: 'a,
        Self::Ok: 'a,
        Self::Error: 'a;

    type EncodeUnitStructAsync<'a>
        = future::Ready<Result<Self::Ok, Self::Error>>
    where
        Self: 'a,
        Self::Ok: 'a,
        Self::Error: 'a;

    type EncodeUnitVariantAsync<'a>
        = future::Ready<Result<Self::Ok, Self::Error>>
    where
        Self: 'a,
        Self::Ok: 'a,
        Self::Error: 'a;

    type EncodeNewtypeStructAsync<'a, 'b, T>
        = future::Ready<Result<Self::Ok, Self::Error>>
    where
        T: ?Sized + TrEncodable,
        T: 'a,
        T: 'b,
        Self: 'a,
        'b: 'a,
        Self::Ok: 'a,
        Self::Error: 'a;

    type EncodeNewtypeVariantAsync<'a, 'b, T>
        = future::Ready<Result<Self::Ok, Self::Error>>
    where
        T: ?Sized + TrEncodable,
        T: 'a,
        T: 'b,
        Self: 'a,
        'b: 'a,
        Self::Ok: 'a,
        Self::Error: 'a;

    type EncodeSeqAsync<'a>
        = future::Ready<Result<Self::EncodeSeq, Self::Error>>
    where
        Self: 'a,
        Self::EncodeSeq: 'a,
        Self::Error: 'a;

    type EncodeTupleAsync<'a>
        = future::Ready<Result<Self::EncodeTuple, Self::Error>>
    where
        Self: 'a,
        Self::EncodeTuple: 'a,
        Self::Error: 'a;

    type EncodeTupleStructAsync<'a>
        = future::Ready<Result<Self::EncodeTupleStruct, Self::Error>>
    where
        Self: 'a,
        Self::EncodeTupleStruct: 'a,
        Self::Error: 'a;

    type EncodeTupleVariantAsync<'a>
        = future::Ready<Result<Self::EncodeTupleVariant, Self::Error>>
    where
        Self: 'a,
        Self::EncodeTupleVariant: 'a,
        Self::Error: 'a;

    type EncodeMapAsync<'a>
        = future::Ready<Result<Self::EncodeMap, Self::Error>>
    where
        Self: 'a,
        Self::EncodeMap: 'a,
        Self::Error: 'a;

    type EncodeStructAsync<'a>
        = future::Ready<Result<Self::EncodeStruct, Self::Error>>
    where
        Self: 'a,
        Self::EncodeStruct: 'a,
        Self::Error: 'a;

    type EncodeStructVariantAsync<'a>
        = future::Ready<Result<Self::EncodeStructVariant, Self::Error>>
    where
        Self: 'a,
        Self::EncodeStructVariant: 'a,
        Self::Error: 'a;

    fn encode_bool<'a>(self, v: bool) -> Self::EncodeBoolAsync<'a>
    where
        Self: 'a,
        Self::Ok: 'a,
        Self::Error: 'a,
    {
        future::ready(Err(<Self::Error as Error>::custom(
            "Impossible encoder cannot encode any value",
        )))
    }

    fn encode_i8<'a>(self, v: i8) -> Self::EncodeI8Async<'a>
    where
        Self: 'a,
        Self::Ok: 'a,
        Self::Error: 'a,
    {
        future::ready(Err(<Self::Error as Error>::custom(
            "Impossible encoder cannot encode any value",
        )))
    }

    fn encode_i16<'a>(self, v: i16) -> Self::EncodeI16Async<'a>
    where
        Self: 'a,
        Self::Ok: 'a,
        Self::Error: 'a,
    {
        future::ready(Err(<Self::Error as Error>::custom(
            "Impossible encoder cannot encode any value",
        )))
    }

    fn encode_i32<'a>(self, v: i32) -> Self::EncodeI32Async<'a>
    where
        Self: 'a,
        Self::Ok: 'a,
        Self::Error: 'a,
    {
        future::ready(Err(<Self::Error as Error>::custom(
            "Impossible encoder cannot encode any value",
        )))
    }

    fn encode_i64<'a>(self, v: i64) -> Self::EncodeI64Async<'a>
    where
        Self: 'a,
        Self::Ok: 'a,
        Self::Error: 'a,
    {
        future::ready(Err(<Self::Error as Error>::custom(
            "Impossible encoder cannot encode any value",
        )))
    }

    fn encode_i128<'a>(self, v: i128) -> Self::EncodeI128Async<'a>
    where
        Self: 'a,
        Self::Ok: 'a,
        Self::Error: 'a,
    {
        future::ready(Err(<Self::Error as Error>::custom(
            "Impossible encoder cannot encode any value",
        )))
    }

    fn encode_u8<'a>(self, v: u8) -> Self::EncodeU8Async<'a>
    where
        Self: 'a,
        Self::Ok: 'a,
        Self::Error: 'a,
    {
        future::ready(Err(<Self::Error as Error>::custom(
            "Impossible encoder cannot encode any value",
        )))
    }

    fn encode_u16<'a>(self, v: u16) -> Self::EncodeU16Async<'a>
    where
        Self: 'a,
        Self::Ok: 'a,
        Self::Error: 'a,
    {
        future::ready(Err(<Self::Error as Error>::custom(
            "Impossible encoder cannot encode any value",
        )))
    }

    fn encode_u32<'a>(self, v: u32) -> Self::EncodeU32Async<'a>
    where
        Self: 'a,
        Self::Ok: 'a,
        Self::Error: 'a,
    {
        future::ready(Err(<Self::Error as Error>::custom(
            "Impossible encoder cannot encode any value",
        )))
    }

    fn encode_u64<'a>(self, v: u64) -> Self::EncodeU64Async<'a>
    where
        Self: 'a,
        Self::Ok: 'a,
        Self::Error: 'a,
    {
        future::ready(Err(<Self::Error as Error>::custom(
            "Impossible encoder cannot encode any value",
        )))
    }

    fn encode_u128<'a>(self, v: u128) -> Self::EncodeU128Async<'a>
    where
        Self: 'a,
        Self::Ok: 'a,
        Self::Error: 'a,
    {
        future::ready(Err(<Self::Error as Error>::custom(
            "Impossible encoder cannot encode any value",
        )))
    }

    fn encode_f32<'a>(self, v: f32) -> Self::EncodeF32Async<'a>
    where
        Self: 'a,
        Self::Ok: 'a,
        Self::Error: 'a,
    {
        future::ready(Err(<Self::Error as Error>::custom(
            "Impossible encoder cannot encode any value",
        )))
    }

    fn encode_f64<'a>(self, v: f64) -> Self::EncodeF64Async<'a>
    where
        Self: 'a,
        Self::Ok: 'a,
        Self::Error: 'a,
    {
        future::ready(Err(<Self::Error as Error>::custom(
            "Impossible encoder cannot encode any value",
        )))
    }

    fn encode_char<'a>(self, v: char) -> Self::EncodeCharAsync<'a>
    where
        Self: 'a,
        Self::Ok: 'a,
        Self::Error: 'a,
    {
        future::ready(Err(<Self::Error as Error>::custom(
            "Impossible encoder cannot encode any value",
        )))
    }

    fn encode_str<'a, 'b>(self, v: &'b str) -> Self::EncodeStrAsync<'a, 'b>
    where
        Self: 'a,
        'b: 'a,
        Self::Ok: 'a,
        Self::Error: 'a,
    {
        future::ready(Err(<Self::Error as Error>::custom(
            "Impossible encoder cannot encode any value",
        )))
    }

    fn encode_bytes<'a, 'b>(self, v: &'b [u8]) -> Self::EncodeBytesAsync<'a, 'b>
    where
        Self: 'a,
        'b: 'a,
        Self::Ok: 'a,
        Self::Error: 'a,
    {
        future::ready(Err(<Self::Error as Error>::custom(
            "Impossible encoder cannot encode any value",
        )))
    }

    fn encode_none<'a>(self) -> Self::EncodeNoneAsync<'a>
    where
        Self: 'a,
        Self::Ok: 'a,
        Self::Error: 'a,
    {
        future::ready(Err(<Self::Error as Error>::custom(
            "Impossible encoder cannot encode any value",
        )))
    }

    fn encode_some<'a, 'b, T>(self, value: &'b T) -> Self::EncodeSomeAsync<'a, 'b, T>
    where
        T: ?Sized + TrEncodable,
        T: 'a,
        T: 'b,
        Self: 'a,
        'b: 'a,
        Self::Ok: 'a,
        Self::Error: 'a,
    {
        future::ready(Err(<Self::Error as Error>::custom(
            "Impossible encoder cannot encode any value",
        )))
    }

    fn encode_unit<'a>(self) -> Self::EncodeUnitAsync<'a>
    where
        Self: 'a,
        Self::Ok: 'a,
        Self::Error: 'a,
    {
        future::ready(Err(<Self::Error as Error>::custom(
            "Impossible encoder cannot encode any value",
        )))
    }

    fn encode_unit_struct<'a>(self, name: &'static str) -> Self::EncodeUnitStructAsync<'a>
    where
        Self: 'a,
        Self::Ok: 'a,
        Self::Error: 'a,
    {
        future::ready(Err(<Self::Error as Error>::custom(
            "Impossible encoder cannot encode any value",
        )))
    }

    fn encode_unit_variant<'a>(
        self,
        name: &'static str,
        variant_index: u32,
        variant: &'static str,
    ) -> Self::EncodeUnitVariantAsync<'a>
    where
        Self: 'a,
        Self::Ok: 'a,
        Self::Error: 'a,
    {
        future::ready(Err(<Self::Error as Error>::custom(
            "Impossible encoder cannot encode any value",
        )))
    }

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
        'b: 'a,
        Self::Ok: 'a,
        Self::Error: 'a,
    {
        future::ready(Err(<Self::Error as Error>::custom(
            "Impossible encoder cannot encode any value",
        )))
    }

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
        'b: 'a,
        Self::Ok: 'a,
        Self::Error: 'a,
    {
        future::ready(Err(<Self::Error as Error>::custom(
            "Impossible encoder cannot encode any value",
        )))
    }

    fn encode_seq<'a>(self, len: Option<usize>) -> Self::EncodeSeqAsync<'a>
    where
        Self: 'a,
        Self::EncodeSeq: 'a,
        Self::Error: 'a,
    {
        future::ready(Err(<Self::Error as Error>::custom(
            "Impossible encoder cannot encode any value",
        )))
    }

    fn encode_tuple<'a>(self, len: usize) -> Self::EncodeTupleAsync<'a>
    where
        Self: 'a,
        Self::EncodeTuple: 'a,
        Self::Error: 'a,
    {
        future::ready(Err(<Self::Error as Error>::custom(
            "Impossible encoder cannot encode any value",
        )))
    }

    fn encode_tuple_struct<'a>(
        self,
        name: &'static str,
        len: usize,
    ) -> Self::EncodeTupleStructAsync<'a>
    where
        Self: 'a,
        Self::EncodeTupleStruct: 'a,
        Self::Error: 'a,
    {
        future::ready(Err(<Self::Error as Error>::custom(
            "Impossible encoder cannot encode any value",
        )))
    }

    fn encode_tuple_variant<'a>(
        self,
        name: &'static str,
        variant_index: u32,
        variant: &'static str,
        len: usize,
    ) -> Self::EncodeTupleVariantAsync<'a>
    where
        Self: 'a,
        Self::EncodeTupleVariant: 'a,
        Self::Error: 'a,
    {
        future::ready(Err(<Self::Error as Error>::custom(
            "Impossible encoder cannot encode any value",
        )))
    }

    fn encode_map<'a>(self, len: Option<usize>) -> Self::EncodeMapAsync<'a>
    where
        Self: 'a,
        Self::EncodeMap: 'a,
        Self::Error: 'a,
    {
        future::ready(Err(<Self::Error as Error>::custom(
            "Impossible encoder cannot encode any value",
        )))
    }

    fn encode_struct<'a>(self, name: &'static str, len: usize) -> Self::EncodeStructAsync<'a>
    where
        Self: 'a,
        Self::EncodeStruct: 'a,
        Self::Error: 'a,
    {
        future::ready(Err(<Self::Error as Error>::custom(
            "Impossible encoder cannot encode any value",
        )))
    }

    fn encode_struct_variant<'a>(
        self,
        name: &'static str,
        variant_index: u32,
        variant: &'static str,
        len: usize,
    ) -> Self::EncodeStructVariantAsync<'a>
    where
        Self: 'a,
        Self::EncodeStructVariant: 'a,
        Self::Error: 'a,
    {
        future::ready(Err(<Self::Error as Error>::custom(
            "Impossible encoder cannot encode any value",
        )))
    }

    fn is_human_readable(&self) -> bool {
        true
    }
}

impl<Ok, Err> TrEncodeSeq for Impossible<Ok, Err>
where
    Err: Error,
{
    type Ok = Ok;
    type Error = Err;

    fn end<'a>(
        self,
    ) -> impl abs_cancel::TrMayCancel<'a, MayCancelOutput = Result<Self::Ok, Self::Error>>
    where
        Self: 'a,
        Self::Ok: 'a,
        Self::Error: 'a,
    {
        future::ready(Err(<Self::Error as Error>::custom(
            "Impossible serializer cannot end a composite value",
        )))
    }
}

impl<Ok, Err> TrEncodeTuple for Impossible<Ok, Err>
where
    Err: Error,
{
    type Ok = Ok;
    type Error = Err;

    fn end<'a>(
        self,
    ) -> impl abs_cancel::TrMayCancel<'a, MayCancelOutput = Result<Self::Ok, Self::Error>>
    where
        Self: 'a,
        Self::Ok: 'a,
        Self::Error: 'a,
    {
        future::ready(Err(<Self::Error as Error>::custom(
            "Impossible serializer cannot end a composite value",
        )))
    }
}

impl<Ok, Err> TrEncodeTupleStruct for Impossible<Ok, Err>
where
    Err: Error,
{
    type Ok = Ok;
    type Error = Err;

    fn end<'a>(
        self,
    ) -> impl abs_cancel::TrMayCancel<'a, MayCancelOutput = Result<Self::Ok, Self::Error>>
    where
        Self: 'a,
        Self::Ok: 'a,
        Self::Error: 'a,
    {
        future::ready(Err(<Self::Error as Error>::custom(
            "Impossible serializer cannot end a composite value",
        )))
    }
}

impl<Ok, Err> TrEncodeTupleVariant for Impossible<Ok, Err>
where
    Err: Error,
{
    type Ok = Ok;
    type Error = Err;

    fn end<'a>(
        self,
    ) -> impl abs_cancel::TrMayCancel<'a, MayCancelOutput = Result<Self::Ok, Self::Error>>
    where
        Self: 'a,
        Self::Ok: 'a,
        Self::Error: 'a,
    {
        future::ready(Err(<Self::Error as Error>::custom(
            "Impossible serializer cannot end a composite value",
        )))
    }
}

impl<Ok, Err> TrEncodeMap for Impossible<Ok, Err>
where
    Err: Error,
{
    type Ok = Ok;
    type Error = Err;

    fn end<'a>(
        self,
    ) -> impl abs_cancel::TrMayCancel<'a, MayCancelOutput = Result<Self::Ok, Self::Error>>
    where
        Self: 'a,
        Self::Ok: 'a,
        Self::Error: 'a,
    {
        future::ready(Err(<Self::Error as Error>::custom(
            "Impossible serializer cannot end a composite value",
        )))
    }
}

impl<Ok, Err> TrEncodeStruct for Impossible<Ok, Err>
where
    Err: Error,
{
    type Ok = Ok;
    type Error = Err;

    fn end<'a>(
        self,
    ) -> impl abs_cancel::TrMayCancel<'a, MayCancelOutput = Result<Self::Ok, Self::Error>>
    where
        Self: 'a,
        Self::Ok: 'a,
        Self::Error: 'a,
    {
        future::ready(Err(<Self::Error as Error>::custom(
            "Impossible serializer cannot end a composite value",
        )))
    }
}

impl<Ok, Err> TrEncodeStructVariant for Impossible<Ok, Err>
where
    Err: Error,
{
    type Ok = Ok;
    type Error = Err;

    fn end<'a>(
        self,
    ) -> impl abs_cancel::TrMayCancel<'a, MayCancelOutput = Result<Self::Ok, Self::Error>>
    where
        Self: 'a,
        Self::Ok: 'a,
        Self::Error: 'a,
    {
        future::ready(Err(<Self::Error as Error>::custom(
            "Impossible serializer cannot end a composite value",
        )))
    }
}
