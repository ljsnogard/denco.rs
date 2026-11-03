//! 基于 bumpalo 的 `abs_denco` arena 实现。
//!
//! 本 crate 只负责把 bumpalo 暴露成 [`abs_denco`] 需要的接口：
//!
//! - [`BumpArena`] 同时实现 [`core::alloc::Allocator`]（供
//!   `Vec<T, &'alloc BumpArena>` 这类借用 arena 的容器使用）、
//!   [`abs_denco::de::TrArena`]（供引用版解码使用，`Ref<T> = BumpBox<'alloc, T>`）
//!   与 [`abs_denco::de::TrArenaBytes`]（供 `&'alloc str` / `&'alloc [u8]`）；
//! - [`BumpString`] 是 `Vec<u8, A>` 的薄封装，用 `Allocator` 参数补上
//!   `String` 缺少的 allocator 能力；通过
//!   [`abs_denco::de::TrStrFromUtf8`] 解码，`BumpString<&'alloc BumpArena>`
//!   因此与 decoder 共享 arena。
//!
//! 它不依赖任何具体数据格式：`MpDecoder` / `JsonDecoder` 对 allocator 泛型化
//! 之后，直接把 `&BumpArena` 传给解码器即可。
//!
//! # 模块划分
//!
//! - `arena_`：`BumpArena`、分配错误与 `TrArena` / `TrArenaBytes` 实现；
//! - `box_`：`BumpBox` 句柄与引用版 `TrDecodable`；
//! - `string_`：`BumpString` 与字符串构造 / 解码。
//!
//! 公开类型统一在本模块逐项重导出，crate 的公开路径保持不变。

#![no_std]
#![feature(impl_trait_in_assoc_type)]

extern crate alloc;

mod arena_;
mod box_;
mod string_;

pub use arena_::{BumpArena, BumpArenaError};
pub use box_::BumpBox;
pub use string_::BumpString;
