//! 容器解码的构造抽象。
//!
//! 标准库只为默认 allocator 的容器实现 `FromIterator`，因此非 `Global`
//! allocator 的容器不能靠 `FromIterator` 构造。这里改为「建空容器 + 逐个
//! 插入」，allocator 实例由 [`TrAllocSource`](super::TrAllocSource) 从 decoder
//! 的 arena 投影得到，不再使用 `A::default()`。
//!
//! 这些 trait 只服务于本 crate 内建的容器实现；所在模块不对外导出，容器
//! 类型本身仍通过 `TrDecodable` 暴露能力。

use alloc::vec::Vec;
use core::alloc::Allocator;

/// 能逐个接收元素、最终构成一个序列型容器的构造器。
pub trait TrSeqSink<T>: Sized {
    /// 容器使用的 allocator 类型。
    type Alloc: Allocator;

    /// 用给定 allocator 造一个空容器。
    fn empty_(alloc: Self::Alloc) -> Self;

    /// 追加一个元素。
    fn push_(&mut self, item: T);
}

/// 能逐个接收键值对、最终构成一个映射型容器的构造器。
#[cfg(any(feature = "alloc-collections", feature = "std"))]
pub trait TrMapSink<K, V>: Sized {
    /// 容器使用的 allocator 类型。
    type Alloc: Allocator;

    /// 用给定 allocator 造一个空容器。
    fn empty_(alloc: Self::Alloc) -> Self;

    /// 插入一个键值对；重复键按容器的通常语义覆盖。
    fn insert_(&mut self, key: K, value: V);
}

impl<T, CA> TrSeqSink<T> for Vec<T, CA>
where
    CA: Allocator,
{
    type Alloc = CA;

    fn empty_(alloc: CA) -> Self {
        Vec::new_in(alloc)
    }

    fn push_(&mut self, item: T) {
        self.push(item);
    }
}
