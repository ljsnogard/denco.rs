//! `alloc::rc::Rc` 的解码实现：解出内部值后包成 `Rc`。
//!
//! 由 `rc` feature 控制；`Arc` 的同类实现保持无条件提供。

use alloc::rc::Rc;

use abs_cancel::{TrCancellationToken, TrMayCancel};

use super::{TrDecodable, TrDecoder};

#[gen_mcf2::gen_may_cancel_future(DecodeRcOp, pub)]
async fn decode_rc_async_<'de, 'a, T, D, TyTok>(de: D, cancel: TyTok) -> Result<Rc<T>, D::Error>
where
    'de: 'a,
    T: TrDecodable<'de> + 'de,
    D: TrDecoder<'de> + 'a,
    D::Error: 'a,
    TyTok: TrCancellationToken,
{
    let value = T::decode_async(de).may_cancel_with(cancel).await?;
    Result::Ok(Rc::new(value))
}

impl<'de, T> TrDecodable<'de> for Rc<T>
where
    T: TrDecodable<'de> + 'de,
{
    type DecodeAsync<'a, D>
        = DecodeRcOpAsync<'de, 'a, 'a, T, D>
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
        DecodeRcOpAsync::new(de)
    }
}
