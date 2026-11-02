//! `alloc::rc::Rc` 的解码实现：解出内部值后包成 `Rc`。
//!
//! 由 `rc` feature 控制；`Arc` 的同类实现保持无条件提供。
//!
//! `Rc` 的 allocator 由 [`TrAllocSource`] 从 decoder 的 arena 投影得到，
//! 不再使用 `A::default()`。

use alloc::rc::Rc;

use abs_cancel::{TrCancellationToken, TrMayCancel};

use super::{TrAllocSource, TrArena, TrDecodable, TrDecoder};

#[gen_mcf2::gen_may_cancel_future(DecodeRcOp, pub)]
async fn decode_rc_async_<'de, 'alloc, 'a, Alloc, T, CA, D, TyTok>(
    de: D,
    cancel: TyTok,
) -> Result<Rc<T, CA>, D::Error>
where
    Alloc: TrArena<'alloc> + 'alloc,
    'de: 'a,
    T: TrDecodable<'de, 'alloc, Alloc> + 'de,
    CA: TrAllocSource<'alloc, Alloc> + 'a,
    D: TrDecoder<'de, 'alloc, Alloc> + 'a,
    D::Error: 'a,
    TyTok: TrCancellationToken,
{
    let alloc = CA::alloc_from(de.allocator());
    let value = T::decode_async(de).may_cancel_with(cancel).await?;
    Result::Ok(Rc::new_in(value, alloc))
}

impl<'de, 'alloc, Alloc: TrArena<'alloc> + 'alloc, T, CA> TrDecodable<'de, 'alloc, Alloc>
    for Rc<T, CA>
where
    T: TrDecodable<'de, 'alloc, Alloc> + 'de,
    CA: TrAllocSource<'alloc, Alloc> + 'alloc,
{
    type DecodeAsync<'a, D>
        = DecodeRcOpAsync<'de, 'alloc, 'a, 'a, Alloc, T, CA, D>
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
