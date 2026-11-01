//! 基于 bumpalo 的 [`Allocator`] 包装。
//!
//! [`BumpAlloc`] 内部用 [`SpinningMutexOwned<Bump>`] 串行化对 arena 的访问，
//! 并实现 [`Default`]：每次 `Default::default()` 创建一个独立的 arena。
//! 因此 `abs_denco` 现有容器实现的 `A: Allocator + Default` 约束可以直接满足，
//! 不需要把 allocator 从 decoder 注入。
//!
//! # 语义
//!
//! - `allocate` 是 O(1) 的 bump；`deallocate` 是 no-op，内存随 [`BumpAlloc`] 析构
//!   整体释放；
//! - `shrink` 直接返回原块（bump 无法原地收缩），容量不会真正下降；
//! - 不实现 `Clone`：clone 一个 arena 没有共享语义；需要共享 arena 的调用方
//!   应自行用 `Arc` 包装；
//! - 不保证 `Send` / `Sync`（`Bump` 本身 `!Sync`），因此 `Arc<T, BumpAlloc>`
//!   不能跨线程 `Send`；单线程解码不受影响。

use core::{
    alloc::{AllocError, Allocator, Layout},
    fmt,
    ptr::NonNull,
};

use atomic_sync::mutex::preemptive::SpinningMutexOwned;
use bumpalo::Bump;

/// 基于 bumpalo 的 [`Allocator`]，每次 [`Default::default`] 创建一个独立 arena。
///
/// # Examples
///
/// ```
/// use abs_denco::de::BumpAlloc;
///
/// let mut values: Vec<u32, BumpAlloc> = Vec::new_in(BumpAlloc::default());
/// values.push(7);
/// assert_eq!(&*values, &[7]);
/// ```
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

/// 为零尺寸分配构造一个非空、满足 `layout.align()` 的悬垂指针。
fn dangling_for_(layout: Layout) -> NonNull<u8> {
    let align = layout.align().max(1);
    // SAFETY: align 是 2 的幂且非零，因此该地址满足对齐要求
    unsafe { NonNull::new_unchecked(align as *mut u8) }
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

#[cfg(test)]
mod tests_ {
    use alloc::vec::Vec;
    use core::{
        alloc::Allocator,
        ptr::NonNull,
    };

    use super::BumpAlloc;

    /// 测试目标：`BumpAlloc` 可以直接作为 `Vec` 的 allocator，并支持增长与收缩。
    /// Test goal: `BumpAlloc` works directly as a `Vec` allocator and survives growth
    /// and shrinking.
    ///
    /// - 手段：用 `BumpAlloc::default()` 构造 `Vec`，推入 1024 个元素后调用
    ///   `shrink_to_fit`。
    ///   / How: build a `Vec` with `BumpAlloc::default()`, push 1024 elements and call
    ///   `shrink_to_fit`.
    /// - 判断：元素数量、内容与顺序都保持正确，`shrink_to_fit` 不破坏数据。
    ///   / What: length, contents and order stay correct and `shrink_to_fit` preserves
    ///   the data.
    #[test]
    fn vec_in_bump_alloc_grows_and_shrinks() {
        let mut values: Vec<u32, BumpAlloc> = Vec::new_in(BumpAlloc::default());
        for index in 0..1024u32 {
            values.push(index);
        }
        values.shrink_to_fit();
        assert_eq!(values.len(), 1024);
        assert_eq!(values[0], 0);
        assert_eq!(values[1023], 1023);
    }

    /// 测试目标：`BumpAlloc` 的 `allocate` / `deallocate` 满足 `Allocator` 契约。
    /// Test goal: `BumpAlloc::allocate` / `deallocate` satisfy the `Allocator` contract.
    ///
    /// - 手段：直接分配一块 64 字节、8 字节对齐的内存，写入并读回，再 `deallocate`。
    ///   / How: allocate a 64-byte, 8-byte-aligned block directly, write and read it back,
    ///   then `deallocate`.
    /// - 判断：返回的块长度与对齐正确，读写一致；`deallocate` 不 panic。
    ///   / What: the returned block has the requested length and alignment, reads back
    ///   the written value, and `deallocate` does not panic.
    #[test]
    fn raw_allocate_roundtrip() {
        use core::alloc::Layout;

        let alloc = BumpAlloc::default();
        let layout = Layout::from_size_align(64, 8).expect("布局合法");
        let block = alloc.allocate(layout).expect("分配应当成功");
        assert_eq!(block.len(), 64);
        assert!(block.cast::<u8>().as_ptr() as usize % 8 == 0);
        // SAFETY: block 至少 64 字节且对齐正确，测试独占该块
        unsafe {
            let ptr = block.as_ptr() as *mut u64;
            ptr.write(0x0123_4567_89AB_CDEF);
            assert_eq!(ptr.read(), 0x0123_4567_89AB_CDEF);
            alloc.deallocate(NonNull::new_unchecked(ptr.cast()), layout);
        }
    }

    /// 测试目标：零尺寸分配返回非空、对齐的指针。
    /// Test goal: a zero-sized allocation returns a non-null, aligned pointer.
    ///
    /// - 手段：用 `Layout::from_size_align(0, 16)` 调用 `allocate`。
    ///   / How: call `allocate` with `Layout::from_size_align(0, 16)`.
    /// - 判断：返回的块长度为 0，且地址满足 16 字节对齐。
    ///   / What: the returned block has length 0 and a 16-byte-aligned address.
    #[test]
    fn zero_sized_allocation_is_dangling() {
        use core::alloc::Layout;

        let alloc = BumpAlloc::default();
        let layout = Layout::from_size_align(0, 16).expect("布局合法");
        let block = alloc.allocate(layout).expect("零尺寸分配应当成功");
        assert_eq!(block.len(), 0);
        assert_eq!(block.cast::<u8>().as_ptr() as usize % 16, 0);
    }
}
