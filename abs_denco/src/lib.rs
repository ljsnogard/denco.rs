//! # DENCO
//!
//! Decode and Encode，但一切皆异步。
//!
//! `abs_denco` 是 `serde` 的异步抽象层：它只定义数据格式与数据结构之间
//! 交互所需的 trait，不绑定具体的序列化格式，也不绑定具体的异步运行时。
//!
//! ## 取消模型
//!
//! 所有可能执行异步工作的 trait 方法都返回 [`abs_cancel::TrMayCancel`]
//! 的实现，而不是直接返回 `Result`。调用者可以：
//!
//! 1. 直接 `.await`，以 [`abs_cancel::NonCancellableToken`] 执行；
//! 2. 先调用 `.may_cancel_with(token)`，再 `.await`，让实现有机会响应
//!    取消信号。
//!
//! 取消令牌不会改变输出类型；被取消时具体返回什么错误由实现者与数据格式
//! 约定。
//!
//! 纯计算、不执行异步工作的方法（例如错误构造、`Visitor::expecting`、
//! `is_human_readable`）保持同步。
//!
//! ## 可选特性
//!
//! 标准库容器的编解码实现全部是**可选**的，默认一个都不开：
//!
//! | feature | 增加的实现 |
//! | --- | --- |
//! | `alloc-collections` | `BTreeMap` / `BTreeSet` / `VecDeque` / `LinkedList` / `BinaryHeap` |
//! | `rc` | `Rc<T>`（`Arc<T>` 保持无条件提供） |
//! | `cow` | `Cow<'a, str>` / `Cow<'a, [T]>`（解码一律得到 `Owned`） |
//! | `std` | `HashMap` / `HashSet`；隐含 `alloc-collections` |
//!
//! 需要组合多个特性时直接相加，例如：
//!
//! ```toml
//! abs_denco = { version = "0.1", features = ["std", "rc", "cow"] }
//! ```
//!
//! ## allocator 泛化
//!
//! 所有容器实现都按 `A: Allocator` 泛化：`Vec`、`Box`、五个
//! `alloc::collections` 容器、`HashMap` / `HashSet`、`Rc` / `Arc` 都接受自定义
//! allocator。解码用 `A::default()` 创建目标容器，因此 `TrDecodable` 要求
//! `A: Default`；`BTreeMap` / `BTreeSet` 还要求 `A: AllocatorClone`。没有默认
//! 实例的 allocator（例如 `&A`）需要先解码到默认 allocator 的容器，再自行迁移。
//!
//! 这些构造器（`VecDeque::new_in`、`BTreeMap::new_in`、
//! `HashMap::with_hasher_in` 等）在当前 nightly 上仍受 `allocator_ext` 控制，
//! BTree 的 allocator 构造还额外需要 `btreemap_alloc`。

#![no_std]
// `Allocator` 本身与 `Vec` / `Box` 的 allocator 参数在 1.100 已稳定，但
// `VecDeque` / `BTreeMap` / `HashMap` 等容器的 `new_in` 仍受 `allocator_ext`
// 控制，BTree 的 allocator 构造还额外需要 `btreemap_alloc`。这里无条件开启，
// 让所有容器实现都能按 `A: Allocator` 泛化。
#![feature(allocator_ext)]
#![cfg_attr(feature = "alloc-collections", feature(btreemap_alloc))]
#![feature(impl_trait_in_assoc_type)]
#![warn(missing_docs)]

extern crate alloc;
#[cfg(feature = "std")]
extern crate std;

pub mod de;
pub mod ser;
