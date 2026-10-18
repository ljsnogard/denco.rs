//! 常见标准类型的 `TrEncodable` 实现。

#[cfg(target_has_atomic = "ptr")]
use alloc::sync::Arc;
use alloc::{boxed::Box, string::String, vec::Vec};

use core::alloc::Allocator;

use abs_cancel::{TrCancellationToken, TrMayCancel};

use super::{TrEncodable, TrEncodeSeq, TrEncoder};

macro_rules! impl_encode_scalar_ {
    ($ty:ty, $method:ident, $gat:ident) => {
        impl TrEncodable for $ty {
            type EncodeAsync<'a, S>
                = S::$gat<'a>
            where
                Self: 'a,
                S: TrEncoder + 'a,
                S::Ok: 'a,
                S::Error: 'a;

            fn encode_async<'a, S>(&'a self, encoder: S) -> Self::EncodeAsync<'a, S>
            where
                Self: 'a,
                S: TrEncoder + 'a,
                S::Ok: 'a,
                S::Error: 'a,
            {
                encoder.$method(*self)
            }
        }
    };
}

impl_encode_scalar_!(bool, encode_bool, EncodeBoolAsync);
impl_encode_scalar_!(i8, encode_i8, EncodeI8Async);
impl_encode_scalar_!(i16, encode_i16, EncodeI16Async);
impl_encode_scalar_!(i32, encode_i32, EncodeI32Async);
impl_encode_scalar_!(i64, encode_i64, EncodeI64Async);
impl_encode_scalar_!(i128, encode_i128, EncodeI128Async);
impl_encode_scalar_!(u8, encode_u8, EncodeU8Async);
impl_encode_scalar_!(u16, encode_u16, EncodeU16Async);
impl_encode_scalar_!(u32, encode_u32, EncodeU32Async);
impl_encode_scalar_!(u64, encode_u64, EncodeU64Async);
impl_encode_scalar_!(u128, encode_u128, EncodeU128Async);
impl_encode_scalar_!(f32, encode_f32, EncodeF32Async);
impl_encode_scalar_!(f64, encode_f64, EncodeF64Async);
impl_encode_scalar_!(char, encode_char, EncodeCharAsync);

impl TrEncodable for str {
    type EncodeAsync<'a, S>
        = S::EncodeStrAsync<'a, 'a>
    where
        Self: 'a,
        S: TrEncoder + 'a,
        S::Ok: 'a,
        S::Error: 'a;

    fn encode_async<'a, S>(&'a self, encoder: S) -> Self::EncodeAsync<'a, S>
    where
        Self: 'a,
        S: TrEncoder + 'a,
        S::Ok: 'a,
        S::Error: 'a,
    {
        encoder.encode_str(self)
    }
}

impl TrEncodable for String {
    type EncodeAsync<'a, S>
        = S::EncodeStrAsync<'a, 'a>
    where
        Self: 'a,
        S: TrEncoder + 'a,
        S::Ok: 'a,
        S::Error: 'a;

    fn encode_async<'a, S>(&'a self, encoder: S) -> Self::EncodeAsync<'a, S>
    where
        Self: 'a,
        S: TrEncoder + 'a,
        S::Ok: 'a,
        S::Error: 'a,
    {
        encoder.encode_str(self.as_str())
    }
}

#[gen_mcf2::gen_may_cancel_future(EncodeOptionOp)]
pub async fn encode_option_async_<'a, T, S, C>(
    value: &'a Option<T>,
    encoder: S,
    cancel: C,
) -> Result<S::Ok, S::Error>
where
    T: TrEncodable + 'a,
    S: TrEncoder + 'a,
    S::Ok: 'a,
    S::Error: 'a,
    C: TrCancellationToken,
{
    match value {
        Some(value) => encoder.encode_some(value).may_cancel_with(cancel).await,
        None => encoder.encode_none().may_cancel_with(cancel).await,
    }
}

impl<T> TrEncodable for Option<T>
where
    T: TrEncodable,
{
    type EncodeAsync<'a, S>
        = EncodeOptionOpAsync<'a, 'a, T, S>
    where
        Self: 'a,
        S: TrEncoder + 'a,
        S::Ok: 'a,
        S::Error: 'a;

    fn encode_async<'a, S>(&'a self, encoder: S) -> Self::EncodeAsync<'a, S>
    where
        Self: 'a,
        S: TrEncoder + 'a,
        S::Ok: 'a,
        S::Error: 'a,
    {
        EncodeOptionOpAsync::new(self, encoder)
    }
}

#[gen_mcf2::gen_may_cancel_future(EncodeSeqSliceOp)]
pub async fn encode_seq_slice_async_<'a, T, S, C>(
    items: &'a [T],
    encoder: S,
    cancel: C,
) -> Result<S::Ok, S::Error>
where
    T: TrEncodable + 'a,
    S: TrEncoder + 'a,
    S::Ok: 'a,
    S::Error: 'a,
    C: TrCancellationToken,
{
    let mut seq = encoder
        .encode_seq(Some(items.len()))
        .may_cancel_with(cancel.child_token())
        .await?;
    for item in items {
        seq.encode_element(item)
            .may_cancel_with(cancel.child_token())
            .await?;
    }
    seq.end().may_cancel_with(cancel).await
}

impl<T> TrEncodable for [T]
where
    T: TrEncodable,
{
    type EncodeAsync<'a, S>
        = EncodeSeqSliceOpAsync<'a, 'a, T, S>
    where
        Self: 'a,
        S: TrEncoder + 'a,
        S::Ok: 'a,
        S::Error: 'a;

    fn encode_async<'a, S>(&'a self, encoder: S) -> Self::EncodeAsync<'a, S>
    where
        Self: 'a,
        S: TrEncoder + 'a,
        S::Ok: 'a,
        S::Error: 'a,
    {
        EncodeSeqSliceOpAsync::new(self, encoder)
    }
}

impl<T, A> TrEncodable for Vec<T, A>
where
    T: TrEncodable,
    A: Allocator,
{
    type EncodeAsync<'a, S>
        = EncodeSeqSliceOpAsync<'a, 'a, T, S>
    where
        Self: 'a,
        S: TrEncoder + 'a,
        S::Ok: 'a,
        S::Error: 'a;

    fn encode_async<'a, S>(&'a self, encoder: S) -> Self::EncodeAsync<'a, S>
    where
        Self: 'a,
        S: TrEncoder + 'a,
        S::Ok: 'a,
        S::Error: 'a,
    {
        EncodeSeqSliceOpAsync::new(self.as_slice(), encoder)
    }
}

impl<T, const N: usize> TrEncodable for [T; N]
where
    T: TrEncodable,
{
    type EncodeAsync<'a, S>
        = EncodeSeqSliceOpAsync<'a, 'a, T, S>
    where
        Self: 'a,
        S: TrEncoder + 'a,
        S::Ok: 'a,
        S::Error: 'a;

    fn encode_async<'a, S>(&'a self, encoder: S) -> Self::EncodeAsync<'a, S>
    where
        Self: 'a,
        S: TrEncoder + 'a,
        S::Ok: 'a,
        S::Error: 'a,
    {
        EncodeSeqSliceOpAsync::new(self.as_slice(), encoder)
    }
}

impl<T> TrEncodable for &T
where
    T: ?Sized + TrEncodable,
{
    type EncodeAsync<'a, S>
        = T::EncodeAsync<'a, S>
    where
        Self: 'a,
        T: 'a,
        S: TrEncoder + 'a,
        S::Ok: 'a,
        S::Error: 'a;

    fn encode_async<'a, S>(&'a self, encoder: S) -> Self::EncodeAsync<'a, S>
    where
        Self: 'a,
        T: 'a,
        S: TrEncoder + 'a,
        S::Ok: 'a,
        S::Error: 'a,
    {
        T::encode_async(*self, encoder)
    }
}

impl<T, A> TrEncodable for Box<T, A>
where
    T: ?Sized + TrEncodable,
    A: Allocator,
{
    type EncodeAsync<'a, S>
        = T::EncodeAsync<'a, S>
    where
        Self: 'a,
        T: 'a,
        S: TrEncoder + 'a,
        S::Ok: 'a,
        S::Error: 'a;

    fn encode_async<'a, S>(&'a self, encoder: S) -> Self::EncodeAsync<'a, S>
    where
        Self: 'a,
        T: 'a,
        S: TrEncoder + 'a,
        S::Ok: 'a,
        S::Error: 'a,
    {
        T::encode_async(&**self, encoder)
    }
}

#[cfg(target_has_atomic = "ptr")]
impl<T, A> TrEncodable for Arc<T, A>
where
    T: ?Sized + TrEncodable,
    A: Allocator,
{
    type EncodeAsync<'a, S>
        = T::EncodeAsync<'a, S>
    where
        Self: 'a,
        T: 'a,
        S: TrEncoder + 'a,
        S::Ok: 'a,
        S::Error: 'a;

    fn encode_async<'a, S>(&'a self, encoder: S) -> Self::EncodeAsync<'a, S>
    where
        Self: 'a,
        T: 'a,
        S: TrEncoder + 'a,
        S::Ok: 'a,
        S::Error: 'a,
    {
        T::encode_async(self.as_ref(), encoder)
    }
}
