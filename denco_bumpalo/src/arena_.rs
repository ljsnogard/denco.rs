//! bumpalo arena：`BumpArena`、分配错误，以及 `TrArena` / `TrArenaBytes` 实现。

use core::{
    alloc::{AllocError, Allocator, Layout},
    fmt, future,
    ptr::NonNull,
    slice,
};

use abs_denco::de::{TrArena, TrArenaBytes};
use atomic_sync::mutex::preemptive::SpinningMutexOwned;
use bumpalo::Bump;

use crate::box_::BumpBox;

/// 基于 bumpalo 的 [`Allocator`]、[`TrArena`] 与 [`TrArenaBytes`]，每次
/// [`Default::default`] 创建一个独立 arena。
///
/// 它**不实现 `Clone`**：clone 一个 arena 没有共享语义。反序列化过程中需要
/// 「借用」同一个 arena 时，通过 `&'alloc BumpArena` 传给解码器
/// （`MpDecoder::new_in` 等），容器与 `BumpBox` / `BumpString` 因此共享它，
/// 而不是各自新建 arena。
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

impl<'alloc> TrArenaBytes<'alloc> for BumpArena {
    type AllocBytesAsync<'a>
        = future::Ready<Result<&'alloc [u8], BumpArenaError>>
    where
        'alloc: 'a,
        Self: 'alloc + 'a;

    fn alloc_bytes_async<'a>(&'alloc self, src: &'a [u8]) -> Self::AllocBytesAsync<'a>
    where
        'alloc: 'a,
        Self: 'alloc + 'a,
    {
        let result = match self.allocate(Layout::for_value(src)) {
            Result::Ok(block) => {
                let ptr = block.as_ptr().cast::<u8>();
                // SAFETY: `allocate` 返回按 `u8` 对齐、长度不小于 `src.len()` 的
                // 未初始化块；这里只写入一次，之后由 arena 持有到 `BumpArena`
                // 析构。切片只以共享引用交出，不会与其它分配重叠。
                unsafe {
                    core::ptr::copy_nonoverlapping(src.as_ptr(), ptr, src.len());
                    let bytes: &'alloc [u8] = slice::from_raw_parts(ptr, src.len());
                    Result::Ok(bytes)
                }
            }
            Result::Err(_) => Result::Err(BumpArenaError),
        };
        future::ready(result)
    }
}
