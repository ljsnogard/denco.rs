//! `alloc::borrow::Cow` 的解码实现。
//!
//! 由 `cow` feature 控制。与 serde 一样，是否借用由数据格式决定；当前
//! `MpDecoder` / `JsonDecoder` 都只产出 owned 值，因此这里一律得到
//! `Cow::Owned`。`Borrowed` 变体只会在未来的零拷贝格式里出现。
//!
//! 这里不要求 `'de: 'a`：解码结果始终是 owned 的，`Cow` 的借用生命周期与
//! 输入数据的生命周期不必相连，`Cow<'static, str>` 这类常见形态因此也能用。

use alloc::{borrow::Cow, string::String, vec::Vec};

use abs_cancel::{TrCancellationToken, TrMayCancel};

use super::{TrDecodable, TrDecoder};

#[gen_mcf2::gen_may_cancel_future(DecodeCowStrOp, pub)]
async fn decode_cow_str_async_<'de, 'a, 'f, D, TyTok>(
    de: D,
    cancel: TyTok,
) -> Result<Cow<'a, str>, D::Error>
where
    D: TrDecoder<'de> + 'f,
    D::Error: 'f,
    TyTok: TrCancellationToken,
{
    let text = <String as TrDecodable<'de>>::decode_async(de)
        .may_cancel_with(cancel)
        .await?;
    Result::Ok(Cow::Owned(text))
}

impl<'de, 'a> TrDecodable<'de> for Cow<'a, str> {
    type DecodeAsync<'x, D>
        = DecodeCowStrOpAsync<'de, 'a, 'x, 'x, D>
    where
        'de: 'x,
        D: TrDecoder<'de> + 'x,
        D::Error: 'x,
        Self: 'x;

    fn decode_async<'x, D>(de: D) -> Self::DecodeAsync<'x, D>
    where
        'de: 'x,
        D: TrDecoder<'de> + 'x,
        D::Error: 'x,
        Self: 'x,
    {
        DecodeCowStrOpAsync::new(de)
    }
}

#[gen_mcf2::gen_may_cancel_future(DecodeCowSliceOp, pub)]
async fn decode_cow_slice_async_<'de, 'a, 'f, T, D, TyTok>(
    de: D,
    cancel: TyTok,
) -> Result<Cow<'a, [T]>, D::Error>
where
    T: Clone + TrDecodable<'de> + 'de + 'a,
    D: TrDecoder<'de> + 'f,
    D::Error: 'f,
    TyTok: TrCancellationToken,
{
    let items = <Vec<T> as TrDecodable<'de>>::decode_async(de)
        .may_cancel_with(cancel)
        .await?;
    Result::Ok(Cow::Owned(items))
}

impl<'de, 'a, T> TrDecodable<'de> for Cow<'a, [T]>
where
    T: Clone + TrDecodable<'de> + 'de + 'a,
{
    type DecodeAsync<'x, D>
        = DecodeCowSliceOpAsync<'de, 'a, 'x, 'x, T, D>
    where
        'de: 'x,
        D: TrDecoder<'de> + 'x,
        D::Error: 'x,
        Self: 'x;

    fn decode_async<'x, D>(de: D) -> Self::DecodeAsync<'x, D>
    where
        'de: 'x,
        D: TrDecoder<'de> + 'x,
        D::Error: 'x,
        Self: 'x,
    {
        DecodeCowSliceOpAsync::new(de)
    }
}
