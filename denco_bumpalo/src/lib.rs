//! 基于 bumpalo 的 `abs_denco` arena 实现。
//!
//! 本 crate 只负责把 bumpalo 暴露成 [`abs_denco`] 需要的接口：
//!
//! - [`BumpArena`] 同时实现 [`core::alloc::Allocator`]（供 `Vec<T, BumpArena>`
//!   这类容器使用）与 [`abs_denco::de::TrArena`]（供引用版解码使用，
//!   `Ref<T> = &'alloc mut T`）；
//! - [`BumpString`] 是 `Vec<u8, A>` 的薄封装，用 `Allocator` 参数补上
//!   `String` 缺少的 allocator 能力。
//!
//! 它不依赖任何具体数据格式：`MpDecoder` / `JsonDecoder` 对 allocator 泛型化
//! 之后，直接把 `&BumpArena` 传给解码器即可。

#![no_std]
#![feature(impl_trait_in_assoc_type)]

extern crate alloc;

use core::{
    alloc::{AllocError, Allocator, Layout},
    fmt, future,
    ops::{Deref, DerefMut},
    ptr::NonNull,
    str,
};

use alloc::{string::String, vec::Vec};

use abs_cancel::{TrCancellationToken, TrMayCancel};
use abs_denco::{
    TrBoxed, TrUnique,
    de::{Error, TrArena, TrDecodable, TrDecoder, Visitor},
};
use atomic_sync::mutex::preemptive::SpinningMutexOwned;
use bumpalo::{Bump, boxed::Box as BumpaloBox};
use gen_mcf2::gen_may_cancel_future;

/// 基于 bumpalo 的 [`Allocator`] 与 [`TrArena`]，每次 [`Default::default`]
/// 创建一个独立 arena。
///
/// 它**不实现 `Clone`**：clone 一个 arena 没有共享语义。反序列化过程中需要
/// 「借用」同一个 arena 时，通过 `&BumpArena` 传给解码器（`MpDecoder::new_in`
/// 等），而不是复制。
///
/// # 语义
///
/// - `allocate` 是 O(1) 的 bump；`deallocate` 是 no-op，内存随 [`BumpArena`]
///   析构整体释放；
/// - `shrink` 直接返回原块（bump 无法原地收缩）；
/// - 不保证 `Send` / `Sync`（`Bump` 本身 `!Sync`），`Arc<T, BumpArena>` 不能
///   跨线程 `Send`；单线程解码不受影响。
pub struct BumpArena {
    inner_: SpinningMutexOwned<Bump>,
}

impl BumpArena {
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

impl Default for BumpArena {
    fn default() -> Self {
        Self::new()
    }
}

impl fmt::Debug for BumpArena {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("BumpArena")
    }
}

/// 为零尺寸分配构造一个非空、满足 `layout.align()` 的悬垂指针。
fn dangling_for_(layout: Layout) -> NonNull<u8> {
    let align = layout.align().max(1);
    // SAFETY: align 是 2 的幂且非零，因此该地址满足对齐要求
    unsafe { NonNull::new_unchecked(align as *mut u8) }
}

// SAFETY: `allocate` 返回的块由 arena 持有，直到 `BumpArena` 析构；`deallocate`
// 是 no-op，符合 `Allocator` 对「释放可以推迟」的约定。`shrink` 返回原块，其大小
// 一定不小于 `new_layout.size()`。
unsafe impl Allocator for BumpArena {
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
            return Result::Ok(NonNull::slice_from_raw_parts(dangling_for_(new_layout), 0));
        }
        // bump 不支持原地收缩；原块至少和 old_layout 一样大，满足 new_layout。
        // SAFETY: 调用方保证 ptr 来自本分配器且仍有 old_layout 大小；返回的块
        // 长度不小于 new_layout.size()，且 deallocate 是 no-op。
        Result::Ok(NonNull::slice_from_raw_parts(ptr, new_layout.size()))
    }
}

/// bumpalo 特化版的独占智能指针，作为 [`BumpArena`] 的 `Ref<T>`。
///
/// 内部是 [`bumpalo::boxed::Box`]：内存属于 arena，句柄被 drop 时运行 `T` 的
/// 析构函数但不归还内存。它实现 [`Deref`] / [`DerefMut`]，并适配
/// [`abs_mm::res_man::TrBoxed`] / [`abs_mm::res_man::TrUnique`]。
pub struct BumpBox<'alloc, T: ?Sized>(BumpaloBox<'alloc, T>);

impl<'alloc, T: ?Sized> BumpBox<'alloc, T> {
    /// 从 arena 分配出的裸指针接管所有权。
    fn from_raw_(raw: *mut T) -> Self {
        // SAFETY: 指针来自 `BumpArena` 的 `allocate`，按 `T` 的对齐与大小划定且
        // 只写入一次；之后由 `BumpBox` 独占。释放由 arena 整体负责，
        // `bumpalo::boxed::Box` 的 Drop 只运行 `T` 的析构。
        Self(unsafe { BumpaloBox::from_raw(raw) })
    }

    /// 取出裸指针，调用方负责之后不再使用本句柄。
    pub fn into_raw(self) -> *mut T {
        BumpaloBox::into_raw(self.0)
    }
}

impl<T: ?Sized> Deref for BumpBox<'_, T> {
    type Target = T;

    fn deref(&self) -> &T {
        &self.0
    }
}

impl<T: ?Sized> DerefMut for BumpBox<'_, T> {
    fn deref_mut(&mut self) -> &mut T {
        &mut self.0
    }
}

impl<T: ?Sized> TrBoxed for BumpBox<'_, T> {
    type Item = T;
}

impl<T: ?Sized> TrUnique for BumpBox<'_, T> {}

/// `TrArena` 分配失败。
///
/// bumpalo 的 [`bumpalo::AllocErr`] 只实现了 `Display`，没有实现
/// [`core::error::Error`]，因此这里提供一个薄适配。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BumpArenaError;

impl fmt::Display for BumpArenaError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("bump arena allocation failed")
    }
}

impl core::error::Error for BumpArenaError {}

impl<'alloc> TrArena<'alloc> for BumpArena {
    type Error = BumpArenaError;

    type Ref<T: 'alloc>
        = BumpBox<'alloc, T>
    where
        Self: 'alloc;

    type AllocValueAsync<'a, T>
        = future::Ready<Result<BumpBox<'alloc, T>, BumpArenaError>>
    where
        'alloc: 'a,
        T: 'alloc + 'a,
        Self: 'alloc + 'a;

    fn alloc_value_async<'a, T>(&'alloc self, value: T) -> Self::AllocValueAsync<'a, T>
    where
        'alloc: 'a,
        T: 'alloc + 'a,
        Self: 'alloc + 'a,
    {
        let layout = Layout::new::<T>();
        let result = match self.allocate(layout) {
            Result::Ok(block) => {
                let ptr = block.as_ptr().cast::<T>();
                // SAFETY: `allocate` 返回按 `layout` 对齐、大小至少
                // `size_of::<T>()` 的未初始化块；写入一次后由 arena 持有到
                // `BumpArena` 析构，期间不会再被移动或释放。
                unsafe {
                    ptr.write(value);
                    Result::Ok(BumpBox::from_raw_(ptr))
                }
            }
            Result::Err(_) => Result::Err(BumpArenaError),
        };
        future::ready(result)
    }
}

/// 把值版解码结果放进 bump arena，返回 [`BumpBox`] 句柄。
#[gen_may_cancel_future(DecodeBumpBoxOp, pub, new(pub))]
async fn decode_bump_box_async_<'de, 'alloc, 'a, T, D, C>(
    de: D,
    cancel: C,
) -> Result<BumpBox<'alloc, T>, D::Error>
where
    'de: 'a,
    'alloc: 'a,
    T: TrDecodable<'de, 'alloc, BumpArena> + 'alloc,
    D: TrDecoder<'de, 'alloc, BumpArena> + 'a,
    D::Error: Error + 'a,
    C: TrCancellationToken,
{
    let arena = de.allocator();
    let value = T::decode_async(de)
        .may_cancel_with(cancel.child_token())
        .await?;
    arena
        .alloc_value_async(value)
        .may_cancel_with(cancel)
        .await
        .map_err(<D::Error as Error>::custom)
}

impl<'de, 'alloc, T> TrDecodable<'de, 'alloc, BumpArena> for BumpBox<'alloc, T>
where
    T: TrDecodable<'de, 'alloc, BumpArena> + 'alloc,
{
    type DecodeAsync<'a, D>
        = DecodeBumpBoxOpAsync<'de, 'alloc, 'a, 'a, T, D>
    where
        'de: 'a,
        'alloc: 'a,
        D: TrDecoder<'de, 'alloc, BumpArena> + 'a,
        D::Error: 'a,
        Self: 'a;

    fn decode_async<'a, D>(de: D) -> Self::DecodeAsync<'a, D>
    where
        'de: 'a,
        'alloc: 'a,
        D: TrDecoder<'de, 'alloc, BumpArena> + 'a,
        D::Error: 'a,
        Self: 'a,
    {
        DecodeBumpBoxOpAsync::new(de)
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
///   创建一个独立 arena（`BumpString<BumpArena>` 可直接使用）。
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

impl<'de, 'alloc, Alloc, A> Visitor<'de, 'alloc, Alloc> for BumpStringVisitor<A>
where
    Alloc: TrArena<'alloc> + 'alloc,
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
async fn decode_bump_string_async_<'alloc, 'de, 'a, Alloc, A, D, C>(
    de: D,
    cancel: C,
) -> Result<BumpString<A>, D::Error>
where
    'de: 'a,
    'alloc: 'a,
    Alloc: TrArena<'alloc> + 'alloc,
    A: Allocator + Default + 'a,
    D: TrDecoder<'de, 'alloc, Alloc> + 'a,
    D::Error: 'a,
    C: TrCancellationToken,
{
    de.decode_string(BumpStringVisitor {
        value_: BumpString::default(),
    })
    .may_cancel_with(cancel)
    .await
}

impl<'de, 'alloc, Alloc, A> TrDecodable<'de, 'alloc, Alloc> for BumpString<A>
where
    Alloc: TrArena<'alloc> + 'alloc,
    A: Allocator + Default + 'de,
{
    type DecodeAsync<'a, D>
        = DecodeBumpStringOpAsync<'alloc, 'de, 'a, 'a, Alloc, A, D>
    where
        'de: 'a,
        'alloc: 'a,
        Alloc: TrArena<'alloc> + 'alloc,
        A: Allocator + Default + 'a,
        D: TrDecoder<'de, 'alloc, Alloc> + 'a,
        D::Error: 'a,
        Self: 'a;

    fn decode_async<'a, D>(de: D) -> Self::DecodeAsync<'a, D>
    where
        'de: 'a,
        'alloc: 'a,
        Alloc: TrArena<'alloc> + 'alloc,
        A: Allocator + Default + 'a,
        D: TrDecoder<'de, 'alloc, Alloc> + 'a,
        D::Error: 'a,
        Self: 'a,
    {
        DecodeBumpStringOpAsync::<'alloc, 'de, 'a, 'a, Alloc, A, D>::new(de)
    }
}
