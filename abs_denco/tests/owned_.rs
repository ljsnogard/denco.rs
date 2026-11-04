//! `TrDecodableOwned` 的语义验证。
//!
//! Semantics checks for `TrDecodableOwned`.
//!
//! 新形状下它是 `for<'alloc> TrDecodable<'alloc, NoArena>`：只有实现对
//! **所有** arena 寿命都成立的 owned 类型才满足。

#[cfg(feature = "cow")]
use std::borrow::Cow;

use abs_denco::de::{NoArena, TrDecodable, TrDecodableOwned};

/// 要求 `T` 满足 [`TrDecodableOwned`]。
/// Require `T` to satisfy [`TrDecodableOwned`].
fn assert_owned_<T: TrDecodableOwned>() {}

/// 要求 `T` 在给定 arena 寿命下可用 [`NoArena`] 解码。
/// Require `T` to decode with [`NoArena`] at the given arena lifetime.
fn assert_alloc_<'alloc, T: TrDecodable<'alloc, NoArena>>() {}

/// 测试目标：与 arena 解耦的 owned 类型满足 `TrDecodableOwned`。
/// Test goal: arena-agnostic owned types satisfy `TrDecodableOwned`.
///
/// - 手段：用编译期断言检查 `u64`、`String`、`Vec<u32>`、`Option<Vec<u8>>`，
///   以及 `cow` feature 下的 `Cow<'static, str>`。
///   / How: use compile-time assertions on `u64`, `String`, `Vec<u32>`,
///   `Option<Vec<u8>>`, and `Cow<'static, str>` under the `cow` feature.
/// - 判断：`assert_owned_::<T>()` 能通过编译；同时普通 `NoArena` 约束也可用。
///   / What: `assert_owned_::<T>()` compiles, and the plain `NoArena` bound is
///   usable too.
#[test]
fn owned_and_arena_agnostic_types_satisfy_owned_trait_() {
    assert_owned_::<u64>();
    assert_owned_::<String>();
    assert_owned_::<Vec<u32>>();
    #[cfg(feature = "cow")]
    assert_owned_::<Cow<'static, str>>();
    assert_owned_::<Option<Vec<u8>>>();

    assert_alloc_::<String>();
    assert_alloc_::<Vec<u32>>();
}

/// 测试目标：`abs_str::str_alloc::String<Global>` 与 arena 解耦，满足
/// `TrDecodableOwned`。
/// Test goal: `abs_str::str_alloc::String<Global>` is arena-agnostic and
/// satisfies `TrDecodableOwned`.
///
/// - 手段：在 `str_alloc` feature 下编译期断言
///   `assert_owned_::<abs_str::str_alloc::String>()`。
///   / How: under the `str_alloc` feature, assert the bound at compile time.
/// - 判断：断言能通过编译；该 impl 对所有 `'alloc` 成立。
///   / What: the assertion compiles; the impl holds for every `'alloc`.
#[cfg(feature = "str_alloc")]
#[test]
fn alloc_string_global_is_decodable_owned_() {
    assert_owned_::<abs_denco::x_deps::abs_str::str_alloc::String>();
}
