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

#![no_std]
#![feature(impl_trait_in_assoc_type)]
#![warn(missing_docs)]

extern crate alloc;
#[cfg(feature = "std")]
extern crate std;

pub mod de;
pub mod ser;
