//! `&'alloc [u8]` 的解码实现。
//!
//! denco 的 `[u8]` / `Vec<u8>` 编码是整数序列（见 `ser::impls_` 的
//! `[T]: TrEncodable`），因此这里也走序列路径：先用 [`Global`] allocator 的
//! `Vec<u8>` 解出 owned 字节，再用 [`TrArenaBytes`] 复制进 decoder 的 arena。
//! 这样 MessagePack 与 JSON 的往返都能成立，也不需要额外的 `decode_bytes_as`。

use alloc::vec::Vec;

use abs_cancel::{TrCancellationToken, TrMayCancel};

use super::{Error, TrArenaBytes, TrDecodable, TrDecoder};

/// 把字节复制进 arena 的解码 helper。
#[gen_mcf2::gen_may_cancel_future(DecodeArenaBytesOp, pub, new(pub))]
async fn decode_arena_bytes_async_<'de, 'alloc, 'a, Alloc, D, C>(
    de: D,
    cancel: C,
) -> Result<&'alloc [u8], D::Error>
where
    'de: 'a,
    'alloc: 'a,
    Alloc: TrArenaBytes<'alloc> + 'alloc,
    D: TrDecoder<'de, 'alloc, Alloc> + 'a,
    D::Error: Error + 'a,
    C: TrCancellationToken,
{
    let alloc = de.allocator();
    let owned = <Vec<u8> as TrDecodable<'de, 'alloc, Alloc>>::decode_async(de)
        .may_cancel_with(cancel.child_token())
        .await?;
    alloc
        .alloc_bytes_async(owned.as_slice())
        .may_cancel_with(cancel)
        .await
        .map_err(<D::Error as Error>::custom)
}

impl<'de, 'alloc, Alloc> TrDecodable<'de, 'alloc, Alloc> for &'alloc [u8]
where
    Alloc: TrArenaBytes<'alloc> + 'alloc,
{
    type DecodeAsync<'a, D>
        = DecodeArenaBytesOpAsync<'de, 'alloc, 'a, 'a, Alloc, D>
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
        DecodeArenaBytesOpAsync::new(de)
    }
}
