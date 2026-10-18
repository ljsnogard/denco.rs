//! 容器解码的构造抽象。
//!
//! 标准库只为默认 allocator 的容器实现 `FromIterator`，因此非 `Global`
//! allocator 的容器不能靠 `FromIterator` 构造。这里改为「建空容器 + 逐个
//! 插入」，allocator 实例由 `A: Default` 提供。
//!
//! 这些 trait 只服务于本 crate 内建的容器实现；所在模块不对外导出，容器
//! 类型本身仍通过 `TrDecodable` 暴露能力。

use alloc::vec::Vec;
use core::alloc::Allocator;

/// 能逐个接收元素、最终构成一个序列型容器的构造器。
pub trait TrSeqSink<T>: Sized {
    /// 用默认 allocator 造一个空容器。
    fn empty_() -> Self;

    /// 追加一个元素。
    fn push_(&mut self, item: T);
}

/// 能逐个接收键值对、最终构成一个映射型容器的构造器。
#[cfg(any(feature = "alloc-collections", feature = "std"))]
pub trait TrMapSink<K, V>: Sized {
    /// 用默认 allocator 造一个空容器。
    fn empty_() -> Self;

    /// 插入一个键值对；重复键按容器的通常语义覆盖。
    fn insert_(&mut self, key: K, value: V);
}

impl<T, A> TrSeqSink<T> for Vec<T, A>
where
    A: Allocator + Default,
{
    fn empty_() -> Self {
        Vec::new_in(A::default())
    }

    fn push_(&mut self, item: T) {
        self.push(item);
    }
}
