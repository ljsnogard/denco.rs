//! 常见标准类型的 `TrDecodable` 实现。

#[cfg(target_has_atomic = "ptr")]
use alloc::sync::Arc;
use alloc::{boxed::Box, string::String, vec::Vec};
use core::{alloc::Allocator, future, marker::PhantomData};

use abs_cancel::{TrCancellationToken, TrMayCancel};

use super::{
    Error, SeqAccess, TrDecodable, TrDecoder, Unexpected, Visitor, collect_::SeqCollectVisitor,
};

macro_rules! impl_decode_unsigned_ {
    ($ty:ty, $visitor:ident, $op:ident, $async_ty:ident, $fn:ident, $decode:ident, $expect:literal) => {
        struct $visitor;

        impl<'de> Visitor<'de> for $visitor {
            type Value = $ty;

            fn expecting(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
                formatter.write_str($expect)
            }

            fn visit_u64<'a, E>(
                self,
                value: u64,
            ) -> impl TrMayCancel<'a, MayCancelOutput = Result<Self::Value, E>>
            where
                Self: 'a,
                Self::Value: 'a,
                E: Error + 'a,
            {
                let result = match <$ty>::try_from(value) {
                    Ok(value) => Ok(value),
                    Err(_) => Err(<E as Error>::invalid_value(
                        Unexpected::Unsigned(value),
                        &self,
                    )),
                };
                future::ready(result)
            }

            fn visit_u128<'a, E>(
                self,
                value: u128,
            ) -> impl TrMayCancel<'a, MayCancelOutput = Result<Self::Value, E>>
            where
                Self: 'a,
                Self::Value: 'a,
                E: Error + 'a,
            {
                let result = match <$ty>::try_from(value) {
                    Ok(value) => Ok(value),
                    Err(_) => Err(<E as Error>::invalid_value(
                        Unexpected::Other("u128"),
                        &self,
                    )),
                };
                future::ready(result)
            }
        }

        #[gen_mcf2::gen_may_cancel_future($op)]
        pub async fn $fn<'de, 'a, D, C>(de: D, cancel: C) -> Result<$ty, D::Error>
        where
            'de: 'a,
            D: TrDecoder<'de> + 'a,
            D::Error: 'a,
            C: TrCancellationToken,
        {
            de.$decode($visitor).may_cancel_with(cancel).await
        }

        impl<'de> TrDecodable<'de> for $ty {
            type DecodeAsync<'a, D>
                = $async_ty<'de, 'a, 'a, D>
            where
                'de: 'a,
                D: TrDecoder<'de> + 'a,
                D::Error: 'a,
                Self: 'a;

            fn decode_async<'a, D>(de: D) -> Self::DecodeAsync<'a, D>
            where
                'de: 'a,
                D: TrDecoder<'de> + 'a,
                D::Error: 'a,
                Self: 'a,
            {
                $async_ty::new(de)
            }
        }
    };
}

macro_rules! impl_decode_signed_ {
    ($ty:ty, $visitor:ident, $op:ident, $async_ty:ident, $fn:ident, $decode:ident, $expect:literal) => {
        struct $visitor;

        impl<'de> Visitor<'de> for $visitor {
            type Value = $ty;

            fn expecting(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
                formatter.write_str($expect)
            }

            fn visit_i64<'a, E>(
                self,
                value: i64,
            ) -> impl TrMayCancel<'a, MayCancelOutput = Result<Self::Value, E>>
            where
                Self: 'a,
                Self::Value: 'a,
                E: Error + 'a,
            {
                let result = match <$ty>::try_from(value) {
                    Ok(value) => Ok(value),
                    Err(_) => Err(<E as Error>::invalid_value(
                        Unexpected::Signed(value),
                        &self,
                    )),
                };
                future::ready(result)
            }

            fn visit_u64<'a, E>(
                self,
                value: u64,
            ) -> impl TrMayCancel<'a, MayCancelOutput = Result<Self::Value, E>>
            where
                Self: 'a,
                Self::Value: 'a,
                E: Error + 'a,
            {
                let result = match <$ty>::try_from(value) {
                    Ok(value) => Ok(value),
                    Err(_) => Err(<E as Error>::invalid_value(
                        Unexpected::Unsigned(value),
                        &self,
                    )),
                };
                future::ready(result)
            }

            fn visit_i128<'a, E>(
                self,
                value: i128,
            ) -> impl TrMayCancel<'a, MayCancelOutput = Result<Self::Value, E>>
            where
                Self: 'a,
                Self::Value: 'a,
                E: Error + 'a,
            {
                let result = match <$ty>::try_from(value) {
                    Ok(value) => Ok(value),
                    Err(_) => Err(<E as Error>::invalid_value(
                        Unexpected::Other("i128"),
                        &self,
                    )),
                };
                future::ready(result)
            }

            fn visit_u128<'a, E>(
                self,
                value: u128,
            ) -> impl TrMayCancel<'a, MayCancelOutput = Result<Self::Value, E>>
            where
                Self: 'a,
                Self::Value: 'a,
                E: Error + 'a,
            {
                let result = match <$ty>::try_from(value) {
                    Ok(value) => Ok(value),
                    Err(_) => Err(<E as Error>::invalid_value(
                        Unexpected::Other("u128"),
                        &self,
                    )),
                };
                future::ready(result)
            }
        }

        #[gen_mcf2::gen_may_cancel_future($op)]
        pub async fn $fn<'de, 'a, D, C>(de: D, cancel: C) -> Result<$ty, D::Error>
        where
            'de: 'a,
            D: TrDecoder<'de> + 'a,
            D::Error: 'a,
            C: TrCancellationToken,
        {
            de.$decode($visitor).may_cancel_with(cancel).await
        }

        impl<'de> TrDecodable<'de> for $ty {
            type DecodeAsync<'a, D>
                = $async_ty<'de, 'a, 'a, D>
            where
                'de: 'a,
                D: TrDecoder<'de> + 'a,
                D::Error: 'a,
                Self: 'a;

            fn decode_async<'a, D>(de: D) -> Self::DecodeAsync<'a, D>
            where
                'de: 'a,
                D: TrDecoder<'de> + 'a,
                D::Error: 'a,
                Self: 'a,
            {
                $async_ty::new(de)
            }
        }
    };
}

macro_rules! impl_decode_float_ {
    ($ty:ty, $visitor:ident, $op:ident, $async_ty:ident, $fn:ident, $decode:ident, $expect:literal) => {
        struct $visitor;

        impl<'de> Visitor<'de> for $visitor {
            type Value = $ty;

            fn expecting(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
                formatter.write_str($expect)
            }

            fn visit_f64<'a, E>(
                self,
                value: f64,
            ) -> impl TrMayCancel<'a, MayCancelOutput = Result<Self::Value, E>>
            where
                Self: 'a,
                Self::Value: 'a,
                E: Error + 'a,
            {
                future::ready(Ok(value as $ty))
            }
        }

        #[gen_mcf2::gen_may_cancel_future($op)]
        pub async fn $fn<'de, 'a, D, C>(de: D, cancel: C) -> Result<$ty, D::Error>
        where
            'de: 'a,
            D: TrDecoder<'de> + 'a,
            D::Error: 'a,
            C: TrCancellationToken,
        {
            de.$decode($visitor).may_cancel_with(cancel).await
        }

        impl<'de> TrDecodable<'de> for $ty {
            type DecodeAsync<'a, D>
                = $async_ty<'de, 'a, 'a, D>
            where
                'de: 'a,
                D: TrDecoder<'de> + 'a,
                D::Error: 'a,
                Self: 'a;

            fn decode_async<'a, D>(de: D) -> Self::DecodeAsync<'a, D>
            where
                'de: 'a,
                D: TrDecoder<'de> + 'a,
                D::Error: 'a,
                Self: 'a,
            {
                $async_ty::new(de)
            }
        }
    };
}

impl_decode_float_!(
    f32,
    F32Visitor,
    DecodeF32Op,
    DecodeF32OpAsync,
    decode_f32_op_async_,
    decode_f32,
    "an f32"
);
impl_decode_float_!(
    f64,
    F64Visitor,
    DecodeF64Op,
    DecodeF64OpAsync,
    decode_f64_op_async_,
    decode_f64,
    "an f64"
);

impl_decode_signed_!(
    i8,
    I8Visitor,
    DecodeI8Op,
    DecodeI8OpAsync,
    decode_i8_op_async_,
    decode_i8,
    "an i8"
);
impl_decode_signed_!(
    i16,
    I16Visitor,
    DecodeI16Op,
    DecodeI16OpAsync,
    decode_i16_op_async_,
    decode_i16,
    "an i16"
);
impl_decode_signed_!(
    i32,
    I32Visitor,
    DecodeI32Op,
    DecodeI32OpAsync,
    decode_i32_op_async_,
    decode_i32,
    "an i32"
);
impl_decode_signed_!(
    i64,
    I64Visitor,
    DecodeI64Op,
    DecodeI64OpAsync,
    decode_i64_op_async_,
    decode_i64,
    "an i64"
);
impl_decode_signed_!(
    i128,
    I128Visitor,
    DecodeI128Op,
    DecodeI128OpAsync,
    decode_i128_op_async_,
    decode_i128,
    "an i128"
);

impl_decode_unsigned_!(
    u8,
    U8Visitor,
    DecodeU8Op,
    DecodeU8OpAsync,
    decode_u8_op_async_,
    decode_u8,
    "a u8"
);
impl_decode_unsigned_!(
    u16,
    U16Visitor,
    DecodeU16Op,
    DecodeU16OpAsync,
    decode_u16_op_async_,
    decode_u16,
    "a u16"
);
impl_decode_unsigned_!(
    u32,
    U32Visitor,
    DecodeU32Op,
    DecodeU32OpAsync,
    decode_u32_op_async_,
    decode_u32,
    "a u32"
);
impl_decode_unsigned_!(
    u64,
    U64Visitor,
    DecodeU64Op,
    DecodeU64OpAsync,
    decode_u64_op_async_,
    decode_u64,
    "a u64"
);
impl_decode_unsigned_!(
    u128,
    U128Visitor,
    DecodeU128Op,
    DecodeU128OpAsync,
    decode_u128_op_async_,
    decode_u128,
    "a u128"
);

struct BoolVisitor;

impl<'de> Visitor<'de> for BoolVisitor {
    type Value = bool;

    fn expecting(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        formatter.write_str("a boolean")
    }

    fn visit_bool<'a, E>(
        self,
        value: bool,
    ) -> impl TrMayCancel<'a, MayCancelOutput = Result<Self::Value, E>>
    where
        Self: 'a,
        Self::Value: 'a,
        E: Error + 'a,
    {
        future::ready(Ok(value))
    }
}

#[gen_mcf2::gen_may_cancel_future(DecodeBoolOp)]
pub async fn decode_bool_op_async_<'de, 'a, D, C>(de: D, cancel: C) -> Result<bool, D::Error>
where
    'de: 'a,
    D: TrDecoder<'de> + 'a,
    D::Error: 'a,
    C: TrCancellationToken,
{
    de.decode_bool(BoolVisitor).may_cancel_with(cancel).await
}

impl<'de> TrDecodable<'de> for bool {
    type DecodeAsync<'a, D>
        = DecodeBoolOpAsync<'de, 'a, 'a, D>
    where
        'de: 'a,
        D: TrDecoder<'de> + 'a,
        D::Error: 'a,
        Self: 'a;

    fn decode_async<'a, D>(de: D) -> Self::DecodeAsync<'a, D>
    where
        'de: 'a,
        D: TrDecoder<'de> + 'a,
        D::Error: 'a,
        Self: 'a,
    {
        DecodeBoolOpAsync::new(de)
    }
}

struct CharVisitor;

impl<'de> Visitor<'de> for CharVisitor {
    type Value = char;

    fn expecting(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        formatter.write_str("a character")
    }

    fn visit_char<'a, E>(
        self,
        value: char,
    ) -> impl TrMayCancel<'a, MayCancelOutput = Result<Self::Value, E>>
    where
        Self: 'a,
        Self::Value: 'a,
        E: Error + 'a,
    {
        future::ready(Ok(value))
    }

    fn visit_str<'a, E>(
        self,
        value: &'a str,
    ) -> impl TrMayCancel<'a, MayCancelOutput = Result<Self::Value, E>>
    where
        Self: 'a,
        Self::Value: 'a,
        E: Error + 'a,
    {
        let mut chars = value.chars();
        let result = match (chars.next(), chars.next()) {
            (Some(ch), None) => Ok(ch),
            _ => Err(<E as Error>::invalid_value(Unexpected::Str(value), &self)),
        };
        future::ready(result)
    }
}

#[gen_mcf2::gen_may_cancel_future(DecodeCharOp)]
pub async fn decode_char_op_async_<'de, 'a, D, C>(de: D, cancel: C) -> Result<char, D::Error>
where
    'de: 'a,
    D: TrDecoder<'de> + 'a,
    D::Error: 'a,
    C: TrCancellationToken,
{
    de.decode_char(CharVisitor).may_cancel_with(cancel).await
}

impl<'de> TrDecodable<'de> for char {
    type DecodeAsync<'a, D>
        = DecodeCharOpAsync<'de, 'a, 'a, D>
    where
        'de: 'a,
        D: TrDecoder<'de> + 'a,
        D::Error: 'a,
        Self: 'a;

    fn decode_async<'a, D>(de: D) -> Self::DecodeAsync<'a, D>
    where
        'de: 'a,
        D: TrDecoder<'de> + 'a,
        D::Error: 'a,
        Self: 'a,
    {
        DecodeCharOpAsync::new(de)
    }
}

struct StringVisitor;

impl<'de> Visitor<'de> for StringVisitor {
    type Value = String;

    fn expecting(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        formatter.write_str("a string")
    }

    fn visit_str<'a, E>(
        self,
        value: &'a str,
    ) -> impl TrMayCancel<'a, MayCancelOutput = Result<Self::Value, E>>
    where
        Self: 'a,
        Self::Value: 'a,
        E: Error + 'a,
    {
        future::ready(Ok(String::from(value)))
    }

    fn visit_string<'a, E>(
        self,
        value: String,
    ) -> impl TrMayCancel<'a, MayCancelOutput = Result<Self::Value, E>>
    where
        Self: 'a,
        Self::Value: 'a,
        E: Error + 'a,
    {
        future::ready(Ok(value))
    }
}

#[gen_mcf2::gen_may_cancel_future(DecodeStringOp)]
pub async fn decode_string_op_async_<'de, 'a, D, C>(de: D, cancel: C) -> Result<String, D::Error>
where
    'de: 'a,
    D: TrDecoder<'de> + 'a,
    D::Error: 'a,
    C: TrCancellationToken,
{
    de.decode_string(StringVisitor)
        .may_cancel_with(cancel)
        .await
}

impl<'de> TrDecodable<'de> for String {
    type DecodeAsync<'a, D>
        = DecodeStringOpAsync<'de, 'a, 'a, D>
    where
        'de: 'a,
        D: TrDecoder<'de> + 'a,
        D::Error: 'a,
        Self: 'a;

    fn decode_async<'a, D>(de: D) -> Self::DecodeAsync<'a, D>
    where
        'de: 'a,
        D: TrDecoder<'de> + 'a,
        D::Error: 'a,
        Self: 'a,
    {
        DecodeStringOpAsync::new(de)
    }
}

struct OptionVisitor<T>(PhantomData<T>);

impl<'de, T> Visitor<'de> for OptionVisitor<T>
where
    T: TrDecodable<'de> + 'de,
{
    type Value = Option<T>;

    fn expecting(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        formatter.write_str("an optional value")
    }

    fn visit_none<'a, E>(self) -> impl TrMayCancel<'a, MayCancelOutput = Result<Self::Value, E>>
    where
        Self: 'a,
        Self::Value: 'a,
        E: Error + 'a,
    {
        future::ready(Ok(None))
    }

    fn visit_some<'a, D>(
        self,
        de: D,
    ) -> impl TrMayCancel<'a, MayCancelOutput = Result<Self::Value, D::Error>>
    where
        'de: 'a,
        Self: 'a,
        Self::Value: 'a,
        D: TrDecoder<'de> + 'a,
        D::Error: 'a,
    {
        DecodeSomeOpAsync::<'de, 'a, 'a, T, D>::new(de)
    }
}

#[gen_mcf2::gen_may_cancel_future(DecodeSomeOp)]
pub async fn decode_some_op_async_<'de, 'a, T, D, C>(
    de: D,
    cancel: C,
) -> Result<Option<T>, D::Error>
where
    'de: 'a,
    T: TrDecodable<'de> + 'de,
    D: TrDecoder<'de> + 'a,
    D::Error: 'a,
    C: TrCancellationToken,
{
    let value = T::decode_async(de).may_cancel_with(cancel).await?;
    Ok(Some(value))
}

#[gen_mcf2::gen_may_cancel_future(DecodeOptionOp)]
pub async fn decode_option_op_async_<'de, 'a, T, D, C>(
    de: D,
    cancel: C,
) -> Result<Option<T>, D::Error>
where
    'de: 'a,
    T: TrDecodable<'de> + 'de,
    D: TrDecoder<'de> + 'a,
    D::Error: 'a,
    C: TrCancellationToken,
{
    de.decode_option(OptionVisitor::<T>(PhantomData))
        .may_cancel_with(cancel)
        .await
}

impl<'de, T> TrDecodable<'de> for Option<T>
where
    T: TrDecodable<'de> + 'de,
{
    type DecodeAsync<'a, D>
        = DecodeOptionOpAsync<'de, 'a, 'a, T, D>
    where
        'de: 'a,
        D: TrDecoder<'de> + 'a,
        D::Error: 'a,
        Self: 'a;

    fn decode_async<'a, D>(de: D) -> Self::DecodeAsync<'a, D>
    where
        'de: 'a,
        D: TrDecoder<'de> + 'a,
        D::Error: 'a,
        Self: 'a,
    {
        DecodeOptionOpAsync::new(de)
    }
}

impl<'de, T, A> TrDecodable<'de> for Vec<T, A>
where
    T: TrDecodable<'de> + 'de,
    A: Allocator + Default,
{
    type DecodeAsync<'a, D>
        = D::DecodeSeqAsync<'a, SeqCollectVisitor<T, Vec<T, A>>>
    where
        'de: 'a,
        D: TrDecoder<'de> + 'a,
        D::Error: 'a,
        Self: 'a;

    fn decode_async<'a, D>(de: D) -> Self::DecodeAsync<'a, D>
    where
        'de: 'a,
        D: TrDecoder<'de> + 'a,
        D::Error: 'a,
        Self: 'a,
    {
        de.decode_seq(SeqCollectVisitor::<T, Self>::new_())
    }
}

struct ArrayVisitor<T, const N: usize>(PhantomData<T>);

impl<'de, T, const N: usize> Visitor<'de> for ArrayVisitor<T, N>
where
    T: TrDecodable<'de> + 'de,
{
    type Value = [T; N];

    fn expecting(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        formatter.write_str("an array")
    }

    fn visit_seq<'a, A>(
        self,
        seq: A,
    ) -> impl TrMayCancel<'a, MayCancelOutput = Result<Self::Value, A::Error>>
    where
        'de: 'a,
        Self: 'a,
        Self::Value: 'a,
        A: SeqAccess<'de> + 'a,
        A::Error: 'a,
    {
        DecodeArraySeqOpAsync::<'de, 'a, 'a, T, A, N>::new(seq)
    }
}

#[gen_mcf2::gen_may_cancel_future(DecodeArraySeqOp)]
pub async fn decode_array_seq_async_<'de, 'a, T, A, const N: usize, C>(
    mut seq: A,
    cancel: C,
) -> Result<[T; N], A::Error>
where
    'de: 'a,
    T: TrDecodable<'de> + 'de,
    A: SeqAccess<'de> + 'a,
    A::Error: 'a,
    C: TrCancellationToken,
{
    let mut out = Vec::with_capacity(N);
    for index in 0..N {
        let value = seq
            .next_element_seed(PhantomData::<T>)
            .may_cancel_with(cancel.child_token())
            .await?
            .ok_or_else(|| <A::Error as Error>::invalid_length(index, &"an array"))?;
        out.push(value);
    }
    out.try_into()
        .map_err(|_| <A::Error as Error>::custom("array length mismatch"))
}

#[gen_mcf2::gen_may_cancel_future(DecodeArrayOp)]
pub async fn decode_array_op_async_<'de, 'a, T, const N: usize, D, C>(
    de: D,
    cancel: C,
) -> Result<[T; N], D::Error>
where
    'de: 'a,
    T: TrDecodable<'de> + 'de,
    D: TrDecoder<'de> + 'a,
    D::Error: 'a,
    C: TrCancellationToken,
{
    de.decode_tuple(N, ArrayVisitor::<T, N>(PhantomData))
        .may_cancel_with(cancel)
        .await
}

impl<'de, T, const N: usize> TrDecodable<'de> for [T; N]
where
    T: TrDecodable<'de> + 'de,
{
    type DecodeAsync<'a, D>
        = DecodeArrayOpAsync<'de, 'a, 'a, T, D, N>
    where
        'de: 'a,
        D: TrDecoder<'de> + 'a,
        D::Error: 'a,
        Self: 'a;

    fn decode_async<'a, D>(de: D) -> Self::DecodeAsync<'a, D>
    where
        'de: 'a,
        D: TrDecoder<'de> + 'a,
        D::Error: 'a,
        Self: 'a,
    {
        DecodeArrayOpAsync::new(de)
    }
}

#[gen_mcf2::gen_may_cancel_future(DecodeBoxOp)]
pub async fn decode_box_op_async_<'de, 'a, T, A, D, C>(
    de: D,
    cancel: C,
) -> Result<Box<T, A>, D::Error>
where
    'de: 'a,
    T: TrDecodable<'de> + 'de,
    A: Allocator + Default + 'a,
    D: TrDecoder<'de> + 'a,
    D::Error: 'a,
    C: TrCancellationToken,
{
    let value = T::decode_async(de).may_cancel_with(cancel).await?;
    Ok(Box::new_in(value, A::default()))
}

impl<'de, T, A> TrDecodable<'de> for Box<T, A>
where
    T: TrDecodable<'de> + 'de,
    A: Allocator + Default,
{
    type DecodeAsync<'a, D>
        = DecodeBoxOpAsync<'de, 'a, 'a, T, A, D>
    where
        'de: 'a,
        D: TrDecoder<'de> + 'a,
        D::Error: 'a,
        Self: 'a;

    fn decode_async<'a, D>(de: D) -> Self::DecodeAsync<'a, D>
    where
        'de: 'a,
        D: TrDecoder<'de> + 'a,
        D::Error: 'a,
        Self: 'a,
    {
        DecodeBoxOpAsync::new(de)
    }
}

#[cfg(target_has_atomic = "ptr")]
#[gen_mcf2::gen_may_cancel_future(DecodeArcOp)]
pub async fn decode_arc_op_async_<'de, 'a, T, A, D, C>(
    de: D,
    cancel: C,
) -> Result<Arc<T, A>, D::Error>
where
    'de: 'a,
    T: TrDecodable<'de> + 'de,
    A: Allocator + Default + 'a,
    D: TrDecoder<'de> + 'a,
    D::Error: 'a,
    C: TrCancellationToken,
{
    let value = T::decode_async(de).may_cancel_with(cancel).await?;
    Ok(Arc::new_in(value, A::default()))
}

#[cfg(target_has_atomic = "ptr")]
impl<'de, T, A> TrDecodable<'de> for Arc<T, A>
where
    T: TrDecodable<'de> + 'de,
    A: Allocator + Default,
{
    type DecodeAsync<'a, D>
        = DecodeArcOpAsync<'de, 'a, 'a, T, A, D>
    where
        'de: 'a,
        D: TrDecoder<'de> + 'a,
        D::Error: 'a,
        Self: 'a;

    fn decode_async<'a, D>(de: D) -> Self::DecodeAsync<'a, D>
    where
        'de: 'a,
        D: TrDecoder<'de> + 'a,
        D::Error: 'a,
        Self: 'a,
    {
        DecodeArcOpAsync::new(de)
    }
}
