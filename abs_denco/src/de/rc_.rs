//! `alloc::rc::Rc` 的解码实现：解出内部值后包成 `Rc`。
//!
//! 由 `rc` feature 控制；`Arc` 的同类实现保持无条件提供。

use alloc::rc::Rc;
use core::alloc::Allocator;

use abs_cancel::{TrCancellationToken, TrMayCancel};

use super::{TrArena, TrDecodable, TrDecoder};

#[gen_mcf2::gen_may_cancel_future(DecodeRcOp, pub)]
async fn decode_rc_async_<'de, 'alloc, 'a, Alloc, T, A, D, TyTok>(
    de: D,
    cancel: TyTok,
) -> Result<Rc<T, A>, D::Error>
where
    Alloc: TrArena<'alloc> + 'alloc,
    'de: 'a,
    T: TrDecodable<'de, 'alloc, Alloc> + 'de,
    A: Allocator + Default + 'a,
    D: TrDecoder<'de, 'alloc, Alloc> + 'a,
    D::Error: 'a,
    TyTok: TrCancellationToken,
{
    let value = T::decode_async(de).may_cancel_with(cancel).await?;
    Result::Ok(Rc::new_in(value, A::default()))
}

impl<'de, 'alloc, Alloc: TrArena<'alloc> + 'alloc, T, A> TrDecodable<'de, 'alloc, Alloc>
    for Rc<T, A>
where
    T: TrDecodable<'de, 'alloc, Alloc> + 'de,
    A: Allocator + Default,
{
    type DecodeAsync<'a, D>
        = DecodeRcOpAsync<'de, 'alloc, 'a, 'a, Alloc, T, A, D>
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
        DecodeRcOpAsync::new(de)
    }
}
