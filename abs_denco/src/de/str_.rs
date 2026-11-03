//! 用 decoder 的 arena 构造「类似 `String`」的类型。
//!
//! [`TrStrFromUtf8`] 把「如何从一段已校验的 UTF-8 文本得到目标字符串」
//! 与具体格式解耦：`String`、`BumpString`、`&'alloc str` 都可以实现它，
//! 反序列化路径只依赖该 trait。
//!
//! 与本 crate 其它异步点一致，构造返回 [`TrMayCancel`] future；同步实现
//! （`String`、bumpalo 的 `BumpString`）用 `core::future::Ready`。

use alloc::string::String;
use core::{convert::Infallible, future, str};

use abs_cancel::{TrCancellationToken, TrMayCancel};

use super::{TrArena, TrArenaBytes, TrDecodable, TrDecoder};

/// 用 decoder 的 arena 从 UTF-8 构造目标字符串类型。
///
/// `'alloc` 是 arena 的借用寿命，`Alloc` 是 arena 的类型。实现方只在
/// `from_utf8_async` 里决定「复制到哪、要付出什么错误」；UTF-8 校验由调用方
/// （具体格式的 decoder）负责，参数因此是 `&str` 而不是 `&[u8]`。
///
/// # 错误处理
///
/// [`TrStrFromUtf8::Error`] 由 decoder 在调用点映射为 `D::Error::custom`；
/// 不会向 `TrDecodable` 的 GAT 添加 `From` 约束。
///
/// # Examples
///
/// ```
/// use abs_cancel::{NonCancellableToken, TrMayCancel};
/// use abs_denco::de::{NoArena, TrStrFromUtf8};
///
/// let text = <String as TrStrFromUtf8<'_, NoArena>>::from_utf8_async(&NoArena, "hello")
///     .may_cancel_with(NonCancellableToken::new());
/// let _ = text;
/// ```
pub trait TrStrFromUtf8<'alloc, Alloc: TrArena<'alloc> + 'alloc>: Sized {
    /// 构造失败时产生的错误类型。
    type Error: core::error::Error;

    /// 构造目标字符串时产生的可取消 future。
    type FromUtf8Async<'a>: TrMayCancel<'a, MayCancelOutput = Result<Self, Self::Error>>
    where
        'alloc: 'a,
        Alloc: 'alloc + 'a,
        Self: 'a;

    /// 用 `alloc` 构造本类型；`value` 已通过 UTF-8 校验。
    fn from_utf8_async<'a>(alloc: &'alloc Alloc, value: &'a str) -> Self::FromUtf8Async<'a>
    where
        'alloc: 'a,
        Alloc: 'alloc + 'a,
        Self: 'a;
}

impl<'alloc, Alloc> TrStrFromUtf8<'alloc, Alloc> for String
where
    Alloc: TrArena<'alloc> + 'alloc,
{
    type Error = Infallible;

    type FromUtf8Async<'a>
        = future::Ready<Result<Self, Infallible>>
    where
        'alloc: 'a,
        Alloc: 'alloc + 'a,
        Self: 'a;

    fn from_utf8_async<'a>(_alloc: &'alloc Alloc, value: &'a str) -> Self::FromUtf8Async<'a>
    where
        'alloc: 'a,
        Alloc: 'alloc + 'a,
        Self: 'a,
    {
        future::ready(Result::Ok(String::from(value)))
    }
}

/// 把字符串字节拷进 arena 的 helper future。
#[gen_mcf2::gen_may_cancel_future(AllocStrOp, pub, new(pub))]
async fn alloc_str_op_async_<'alloc, 'a, Alloc, C>(
    alloc: &'alloc Alloc,
    value: &'a str,
    cancel: C,
) -> Result<&'alloc str, Alloc::Error>
where
    'alloc: 'a,
    Alloc: TrArenaBytes<'alloc> + 'alloc,
    C: TrCancellationToken,
{
    let bytes = alloc
        .alloc_bytes_async(value.as_bytes())
        .may_cancel_with(cancel.child_token())
        .await?;
    // SAFETY: `bytes` 是 `value` 的逐字节拷贝，`value` 已是合法 UTF-8，
    // 因此拷贝结果仍是合法 UTF-8。
    Result::Ok(unsafe { str::from_utf8_unchecked(bytes) })
}

impl<'alloc, Alloc> TrStrFromUtf8<'alloc, Alloc> for &'alloc str
where
    Alloc: TrArenaBytes<'alloc> + 'alloc,
{
    type Error = Alloc::Error;

    type FromUtf8Async<'a>
        = AllocStrOpAsync<'alloc, 'a, 'a, Alloc>
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
        AllocStrOpAsync::new(alloc, value)
    }
}

/// 把字符串读进 arena 的解码 helper。
#[gen_mcf2::gen_may_cancel_future(DecodeArenaStrOp, pub, new(pub))]
async fn decode_arena_str_async_<'alloc, 'a, Alloc, D, C>(
    de: D,
    cancel: C,
) -> Result<&'alloc str, D::Error>
where
    'alloc: 'a,
    Alloc: TrArenaBytes<'alloc> + 'alloc,
    D: TrDecoder<'alloc, Alloc> + 'a,
    D::Error: 'a,
    C: TrCancellationToken,
{
    de.decode_string_as::<&'alloc str>()
        .may_cancel_with(cancel)
        .await
}

impl<'alloc, Alloc> TrDecodable<'alloc, Alloc> for &'alloc str
where
    Alloc: TrArenaBytes<'alloc> + 'alloc,
{
    type DecodeAsync<'a, D>
        = DecodeArenaStrOpAsync<'alloc, 'a, 'a, Alloc, D>
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
        DecodeArenaStrOpAsync::new(de)
    }
}
