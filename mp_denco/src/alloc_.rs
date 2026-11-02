//! 基于 bumpalo 的 arena 分配器与带 allocator 的字符串。
//!
//! 这些类型属于格式实现（`mp_denco`），不属于抽象层 `abs_denco`：
//! `abs_denco` 只定义 `Allocator` 约束，不关心具体分配器来自哪里。

use core::{
    alloc::{AllocError, Allocator, Layout},
    fmt,
    future,
    ops::Deref,
    ptr::NonNull,
    str,
};

use alloc::{string::String, vec::Vec};

use abs_cancel::{TrCancellationToken, TrMayCancel};
use abs_denco::de::{Error, TrDecodable, TrDecoder, Visitor};
use atomic_sync::mutex::preemptive::SpinningMutexOwned;
use bumpalo::Bump;
use gen_mcf2::gen_may_cancel_future;


/// 基于 bumpalo 的 [`Allocator`]，每次 [`Default::default`] 创建一个独立 arena。
///
/// 它**不实现 `Clone`**：clone 一个 arena 没有共享语义。反序列化过程中需要
/// 「借用」同一个 allocator 时，应通过 `&mut BumpAlloc` / `&BumpAlloc` 传递，
/// 而不是复制。
///
/// # 语义
///
/// - `allocate` 是 O(1) 的 bump；`deallocate` 是 no-op，内存随 [`BumpAlloc`]
///   析构整体释放；
/// - `shrink` 直接返回原块（bump 无法原地收缩）；
/// - 不保证 `Send` / `Sync`（`Bump` 本身 `!Sync`），`Arc<T, BumpAlloc>` 不能
///   跨线程 `Send`；单线程解码不受影响。
pub struct BumpAlloc {
    inner_: SpinningMutexOwned<Bump>,
}

impl BumpAlloc {
    /// 创建一个新的 arena。
    pub fn new() -> Self {
        Self {
            inner_: SpinningMutexOwned::new_owned(Bump::new()),
        }
    }

    /// 本 arena 已向底层分配器申请的字节数（观测用）。
    pub fn allocated_bytes(&self) -> usize {
        let mut session = self.inner_.lock_session();
        match session.lock().wait() {
            Result::Ok(guard) => guard.allocated_bytes(),
            Result::Err(_) => 0,
        }
    }
}

impl Default for BumpAlloc {
    fn default() -> Self {
        Self::new()
    }
}

impl fmt::Debug for BumpAlloc {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("BumpAlloc")
    }
}

/// 为零尺寸分配构造一个非空、满足 `layout.align()` 的悬垂指针。
fn dangling_for_(layout: Layout) -> NonNull<u8> {
    let align = layout.align().max(1);
    // SAFETY: align 是 2 的幂且非零，因此该地址满足对齐要求
    unsafe { NonNull::new_unchecked(align as *mut u8) }
}

// SAFETY: `allocate` 返回的块由 arena 持有，直到 `BumpAlloc` 析构；`deallocate`
// 是 no-op，符合 `Allocator` 对「释放可以推迟」的约定。`shrink` 返回原块，其大小
// 一定不小于 `new_layout.size()`。
unsafe impl Allocator for BumpAlloc {
    fn allocate(&self, layout: Layout) -> Result<NonNull<[u8]>, AllocError> {
        if layout.size() == 0 {
            return Result::Ok(NonNull::slice_from_raw_parts(dangling_for_(layout), 0));
        }
        let mut session = self.inner_.lock_session();
        let guard = session.lock().wait().map_err(|_| AllocError)?;
        let ptr = guard.try_alloc_layout(layout).map_err(|_| AllocError)?;
        Result::Ok(NonNull::slice_from_raw_parts(ptr, layout.size()))
    }

    unsafe fn deallocate(&self, _ptr: NonNull<u8>, _layout: Layout) {
        // SAFETY: 调用方保证 ptr 来自本分配器；本实现不访问该指针，只让 arena
        // 在析构时整体释放。
    }

    unsafe fn shrink(
        &self,
        ptr: NonNull<u8>,
        _old_layout: Layout,
        new_layout: Layout,
    ) -> Result<NonNull<[u8]>, AllocError> {
        if new_layout.size() == 0 {
            return Result::Ok(NonNull::slice_from_raw_parts(
                dangling_for_(new_layout),
                0,
            ));
        }
        // bump 不支持原地收缩；原块至少和 old_layout 一样大，满足 new_layout。
        // SAFETY: 调用方保证 ptr 来自本分配器且仍有 old_layout 大小；返回的块
        // 长度不小于 new_layout.size()，且 deallocate 是 no-op。
        Result::Ok(NonNull::slice_from_raw_parts(ptr, new_layout.size()))
    }
}

/// `Vec<u8, A>` 的薄封装，用来给 `String` 补上 allocator 参数。
///
/// 标准库的 `alloc::string::String` 目前没有 allocator 参数；本类型把
/// `Vec<u8, A>` 包成字符串，构造路径只接受 `&str` / `String`，因此内容始终是
/// 合法 UTF-8。
///
/// # 注意
///
/// - 与 `String` 一样，`A` 必须由调用方提供；解码时用 [`Default::default`]
///   创建一个独立 arena（`BumpString<BumpAlloc>` 可直接使用）。
/// - 若要让字符串借用 decoder 的 arena（`BumpString<'a>` 形态），需要
///   `abs_denco` 提供 allocator 注入能力，见 `dev-notes` 中的限制分析。
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

struct BumpStringVisitor<A: Allocator> {
    value_: BumpString<A>,
}

impl<'de, A> Visitor<'de> for BumpStringVisitor<A>
where
    A: Allocator + Default,
{
    type Value = BumpString<A>;

    fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("a string")
    }

    fn visit_str<'a, E>(
        mut self,
        value: &'a str,
    ) -> impl TrMayCancel<'a, MayCancelOutput = Result<Self::Value, E>>
    where
        Self: 'a,
        Self::Value: 'a,
        E: Error + 'a,
    {
        self.value_.bytes_.extend_from_slice(value.as_bytes());
        future::ready(Result::Ok(self.value_))
    }

    fn visit_string<'a, E>(
        mut self,
        value: String,
    ) -> impl TrMayCancel<'a, MayCancelOutput = Result<Self::Value, E>>
    where
        Self: 'a,
        Self::Value: 'a,
        E: Error + 'a,
        'de: 'a,
    {
        self.value_.bytes_.extend_from_slice(value.as_bytes());
        future::ready(Result::Ok(self.value_))
    }
}

#[gen_may_cancel_future(DecodeBumpStringOp, pub, new(pub))]
async fn decode_bump_string_async_<'de, 'a, A, D, C>(
    de: D,
    cancel: C,
) -> Result<BumpString<A>, D::Error>
where
    'de: 'a,
    A: Allocator + Default + 'a,
    D: TrDecoder<'de> + 'a,
    D::Error: 'a,
    C: TrCancellationToken,
{
    de.decode_string(BumpStringVisitor {
        value_: BumpString::default(),
    })
    .may_cancel_with(cancel)
    .await
}

impl<'de, A> TrDecodable<'de> for BumpString<A>
where
    A: Allocator + Default + 'de,
{
    type DecodeAsync<'a, D>
        = DecodeBumpStringOpAsync<'de, 'a, 'a, A, D>
    where
        'de: 'a,
        A: Allocator + Default + 'a,
        D: TrDecoder<'de> + 'a,
        D::Error: 'a,
        Self: 'a;

    fn decode_async<'a, D>(de: D) -> Self::DecodeAsync<'a, D>
    where
        'de: 'a,
        A: Allocator + Default + 'a,
        D: TrDecoder<'de> + 'a,
        D::Error: 'a,
        Self: 'a,
    {
        DecodeBumpStringOpAsync::<'de, 'a, 'a, A, D>::new(de)
    }
}
