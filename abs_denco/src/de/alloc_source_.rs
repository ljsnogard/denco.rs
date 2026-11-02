//! 容器 allocator 的投影。
//!
//! 标准容器的 allocator 是容器类型的一部分，而 decoder 只能提供
//! `&'alloc Alloc`。本模块把「如何从 arena 得到一个容器 allocator 实例」
//! 抽成 [`TrAllocSource`]，从而让同一个容器 `impl` 同时覆盖两种形态：
//!
//! - [`Global`]：容器与 arena 无关，`Vec<T>` 这类类型在任意 decoder 下都可解码；
//! - `&'alloc Alloc`：容器借用 decoder 的同一个 arena（要求 `Alloc: Allocator`）。
//!
//! 两个实现的 self 类型分别是 `Global` 与引用，不可能统一，因此 coherence
//! 成立。容器实现不再依赖 `A::default()`，也就不会为每个容器各自新建 arena。

use alloc::alloc::Global;
use core::alloc::Allocator;

use super::TrArena;

/// 从 decoder 的 arena 推导出容器 allocator。
///
/// 实现方返回的 allocator 会被容器用来划出元素缓冲区；对 `&'alloc Alloc`
/// 而言，容器因此与引用版解码结果共享同一个 arena。
///
/// # Examples
///
/// ```
/// use abs_denco::de::{NoArena, TrAllocSource};
///
/// // `Global` 的投影与 arena 无关，仍然返回 `Global`。
/// let alloc = <std::alloc::Global as TrAllocSource<'_, NoArena>>::alloc_from(&NoArena);
/// let _ = alloc;
/// ```
pub trait TrAllocSource<'alloc, Alloc: TrArena<'alloc> + 'alloc>: Allocator + Sized {
    /// 从 `arena` 取得容器 allocator 实例。
    fn alloc_from(arena: &'alloc Alloc) -> Self;
}

impl<'alloc, Alloc> TrAllocSource<'alloc, Alloc> for Global
where
    Alloc: TrArena<'alloc> + 'alloc,
{
    fn alloc_from(_arena: &'alloc Alloc) -> Self {
        Global
    }
}

impl<'alloc, Alloc> TrAllocSource<'alloc, Alloc> for &'alloc Alloc
where
    Alloc: TrArena<'alloc> + 'alloc + Allocator,
{
    fn alloc_from(arena: &'alloc Alloc) -> Self {
        arena
    }
}
