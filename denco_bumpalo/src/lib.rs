//! 基于 bumpalo 的 `abs_denco` arena 实现。
//!
//! 本 crate 只负责把 bumpalo 暴露成 [`abs_denco`] 需要的接口：
//!
//! - [`BumpArena`] 同时实现 [`core::alloc::Allocator`]（供
//!   `Vec<T, &'alloc BumpArena>` 这类借用 arena 的容器使用）、
//!   [`abs_denco::de::TrArena`]（供引用版解码使用，`Ref<T> = BumpBox<'alloc, T>`）
//!   与 [`abs_denco::de::TrArenaBytes`]（供 `&'alloc str` / `&'alloc [u8]`）；
//! - [`String`] 是 `abs_str::str_alloc::String<A>` 的重新导出：带 allocator
//!   参数的字符串类型，编解码实现由 `abs_denco` 的 `str_alloc` feature 提供；
//!   `String<&'alloc BumpArena>` 因此与 decoder 共享 arena。
//!
//! 它不依赖任何具体数据格式：`MpDecoder` / `JsonDecoder` 对 allocator 泛型化
//! 之后，直接把 `&BumpArena` 传给解码器即可。
//!
//! # 模块划分
//!
//! - `arena_`：`BumpArena`、分配错误与 `TrArena` / `TrArenaBytes` 实现；
//! - `box_`：`BumpBox` 句柄与引用版 `TrDecodable`。
//!
//! 字符串类型不在本 crate 内定义，而是直接重导出 `abs_str` 的实现；公开类型
//! 统一在本模块逐项重导出。

#![no_std]
#![feature(impl_trait_in_assoc_type)]

mod arena_;
mod box_;

pub use abs_denco::x_deps::abs_str::str_alloc::String;

pub use arena_::{BumpArena, BumpArenaError};
pub use box_::BumpBox;
