//! `BumpString`：给 `String` 补上 allocator 参数的薄封装。

use core::{alloc::Allocator, convert::Infallible, fmt, future, ops::Deref, str};

use alloc::vec::Vec;

use abs_cancel::{TrCancellationToken, TrMayCancel};
use abs_denco::de::{TrAllocSource, TrArena, TrDecodable, TrDecoder, TrStrFromUtf8};
use gen_mcf2::gen_may_cancel_future;

/// `Vec<u8, A>` 的薄封装，用来给 `String` 补上 allocator 参数。
///
/// 标准库的 `alloc::string::String` 目前没有 allocator 参数；本类型把
/// `Vec<u8, A>` 包成字符串，构造路径只接受 `&str` / `String`，因此内容始终是
/// 合法 UTF-8。
///
/// # 解码
///
/// 通过 [`TrStrFromUtf8`] 构造：`BumpString<&'alloc BumpArena>` 把内容复制进
/// decoder 的同一个 arena；`BumpString<Global>` 与普通 `String` 一样使用全局
/// 分配器。解码路径不再用 `Default` 新建 arena。
pub struct BumpString<A: Allocator> {
    bytes_: Vec<u8, A>,
}

impl<A: Allocator> BumpString<A> {
    /// 用给定 allocator 创建一个空字符串。
    pub fn new_in(alloc: A) -> Self {
        Self {
            bytes_: Vec::new_in(alloc),
        }
    }

    /// 字节视图。
    pub fn as_bytes_(&self) -> &[u8] {
        &self.bytes_
    }

    /// 解析为 `&str`。
    pub fn as_str_(&self) -> &str {
        // SAFETY: 构造路径只接受 `&str` / `String`，内容始终是合法 UTF-8；
        // `BumpString` 不暴露可变字节访问。
        unsafe { str::from_utf8_unchecked(&self.bytes_) }
    }
}

impl<A: Allocator + Default> Default for BumpString<A> {
    fn default() -> Self {
        Self::new_in(A::default())
    }
}

impl<A: Allocator> Deref for BumpString<A> {
    type Target = str;

    fn deref(&self) -> &Self::Target {
        self.as_str_()
    }
}

impl<A: Allocator> AsRef<str> for BumpString<A> {
    fn as_ref(&self) -> &str {
        self.as_str_()
    }
}

impl<A: Allocator> fmt::Debug for BumpString<A> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Debug::fmt(self.as_str_(), f)
    }
}

impl<A: Allocator> fmt::Display for BumpString<A> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(self.as_str_(), f)
    }
}

impl<'alloc, Alloc, CA> TrStrFromUtf8<'alloc, Alloc> for BumpString<CA>
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
        let mut out = BumpString::new_in(CA::alloc_from(alloc));
        out.bytes_.extend_from_slice(value.as_bytes());
        future::ready(Result::Ok(out))
    }
}

#[gen_may_cancel_future(DecodeBumpStringOp, pub, new(pub))]
async fn decode_bump_string_async_<'alloc, 'a, Alloc, CA, D, C>(
    de: D,
    cancel: C,
) -> Result<BumpString<CA>, D::Error>
where
    'alloc: 'a,
    Alloc: TrArena<'alloc> + 'alloc,
    CA: TrAllocSource<'alloc, Alloc> + 'alloc,
    D: TrDecoder<'alloc, Alloc> + 'a,
    D::Error: 'a,
    C: TrCancellationToken,
{
    de.decode_string_as::<BumpString<CA>>()
        .may_cancel_with(cancel)
        .await
}

impl<'alloc, Alloc, CA> TrDecodable<'alloc, Alloc> for BumpString<CA>
where
    Alloc: TrArena<'alloc> + 'alloc,
    CA: TrAllocSource<'alloc, Alloc> + 'alloc,
{
    type DecodeAsync<'a, D>
        = DecodeBumpStringOpAsync<'alloc, 'a, 'a, Alloc, CA, D>
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
        DecodeBumpStringOpAsync::new(de)
    }
}
