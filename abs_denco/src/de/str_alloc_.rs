//! `abs_str::String` 的解码实现（`str_alloc` feature）。
//!
//! 解码路径与标准库 `String` 一致：格式负责校验 UTF-8 并通过
//! [`TrStrFromUtf8`] 构造目标类型。allocator 由 [`TrAllocSource`] 从 decoder
//! 的 arena 投影：
//!
//! - `abs_str::str_alloc::String<Global>` 使用全局分配器，与普通 `String` 等价；
//! - `abs_str::str_alloc::String<&'alloc A>` 借用 decoder 携带的 arena（`A: Allocator`，
//!   例如 `denco_bumpalo::BumpArena`）。

use core::{convert::Infallible, future};

use abs_cancel::{TrCancellationToken, TrMayCancel};
use abs_str::str_alloc::String as AllocString;
use gen_mcf2::gen_may_cancel_future;

use super::{TrAllocSource, TrArena, TrDecodable, TrDecoder, TrStrFromUtf8};

impl<'alloc, Alloc, CA> TrStrFromUtf8<'alloc, Alloc> for AllocString<CA>
where
    Alloc: TrArena<'alloc> + 'alloc,
    CA: TrAllocSource<'alloc, Alloc>,
{
    type Error = Infallible;

    type FromUtf8Async<'a>
        = future::Ready<Result<Self, Infallible>>
    where
        'alloc: 'a,
        Alloc: 'alloc + 'a,
        Self: 'a;

    fn from_utf8_async<'a>(alloc: &'alloc Alloc, value: &'a str) -> Self::FromUtf8Async<'a>
    where
        'alloc: 'a,
        Alloc: 'alloc + 'a,
        Self: 'a,
    {
        future::ready(Result::Ok(AllocString::from_str_in(
            value,
            CA::alloc_from(alloc),
        )))
    }
}

/// 读取字符串并构造 `abs_str::str_alloc::String<CA>`。
#[gen_may_cancel_future(DecodeAllocStringOp, pub, new(pub))]
async fn decode_alloc_string_async_<'alloc, 'a, Alloc, CA, D, C>(
    de: D,
    cancel: C,
) -> Result<AllocString<CA>, D::Error>
where
    'alloc: 'a,
    Alloc: TrArena<'alloc> + 'alloc,
    CA: TrAllocSource<'alloc, Alloc> + 'alloc,
    D: TrDecoder<'alloc, Alloc> + 'a,
    D::Error: 'a,
    C: TrCancellationToken,
{
    de.decode_string_as::<AllocString<CA>>()
        .may_cancel_with(cancel)
        .await
}

impl<'alloc, Alloc, CA> TrDecodable<'alloc, Alloc> for AllocString<CA>
where
    Alloc: TrArena<'alloc> + 'alloc,
    CA: TrAllocSource<'alloc, Alloc> + 'alloc,
{
    type DecodeAsync<'a, D>
        = DecodeAllocStringOpAsync<'alloc, 'a, 'a, Alloc, CA, D>
    where
        'alloc: 'a,
        D: TrDecoder<'alloc, Alloc> + 'a,
        D::Error: 'a,
        Self: 'a;

    fn decode_async<'a, D>(de: D) -> Self::DecodeAsync<'a, D>
    where
        'alloc: 'a,
        D: TrDecoder<'alloc, Alloc> + 'a,
        D::Error: 'a,
        Self: 'a,
    {
        DecodeAllocStringOpAsync::new(de)
    }
}
