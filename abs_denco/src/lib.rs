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
//! | `str_alloc` | `abs_str::str_alloc::String<A>` 的编解码（allocator 由 decoder 投影） |
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
//! allocator。解码侧的 allocator 由 [`TrAllocSource`](de::TrAllocSource) 从
//! decoder 的 arena 投影得到：
//!
//! - `Vec<T, Global>`（以及默认的 `Vec<T>`）与 arena 无关，任意 decoder 都可解码；
//! - `Vec<T, &'alloc A>` 借用 decoder 的同一个 arena，适合把容器与引用版解码
//!   结果放在一起；要求 `A: Allocator`（`&A` 的 `Allocator` 由 core 的 blanket
//!   impl 提供，bumpalo 不需要开启额外的 feature）；
//! - `BTreeMap` / `BTreeSet` 额外要求 allocator 实现 `AllocatorClone`。
//!
//! 不再用 `A::default()` 构造容器，因此容器不会各自新建 arena；自定义 allocator
//! 若与 arena 无关，需要自行实现 [`TrAllocSource`](de::TrAllocSource)。
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

pub use de::{TrDecodable, TrDecoder};
pub use ser::{TrEncodable, TrEncoder};

/// arena 句柄的智能指针语义，转发自 [`abs_mm`]。
///
/// [`TrArena::Ref<T>`](de::TrArena::Ref) 的实现方（例如
/// `denco_bumpalo::BumpBox`）应当同时实现 [`TrBoxed`] / [`TrUnique`]，
/// 这样通用代码可以把解码结果当作普通智能指针使用。`abs_denco` 本身不强制
/// 这个约束，因为 [`NoArena`](de::NoArena) 的 `Ref<T>` 就是 `T`。
pub use abs_mm::res_man::{TrBoxed, TrUnique};

/// 外部依赖的再导出，便于下游在不直接依赖这些 crate 时命名类型。
pub mod x_deps {
    /// `abs_mm` 的再导出。
    pub use abs_mm;

    /// `abs_str` 的再导出；由 `str_alloc` feature 控制。
    #[cfg(feature = "str_alloc")]
    pub use abs_str;
}
