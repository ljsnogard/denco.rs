//! `BumpString<Global>` 对 `TrDecodableOwned` 的验证。
//!
//! Checks that `BumpString<Global>` satisfies `TrDecodableOwned`.

use std::alloc::Global;

use abs_denco::de::TrDecodableOwned;
use denco_bumpalo::BumpString;

/// 要求 `T` 满足 [`TrDecodableOwned`]。
/// Require `T` to satisfy [`TrDecodableOwned`].
fn assert_owned_<T: TrDecodableOwned>() {}

/// 测试目标：`BumpString<Global>` 与 arena 无关，满足 `TrDecodableOwned`。
/// Test goal: `BumpString<Global>` is arena-agnostic and satisfies
/// `TrDecodableOwned`.
///
/// - 手段：编译期断言 `assert_owned_::<BumpString<Global>>()`。
///   / How: a compile-time assertion on `BumpString<Global>`.
/// - 判断：断言能通过编译；`BumpString<Global>` 的 impl 对所有 `'alloc` 成立。
///   / What: the assertion compiles; the impl holds for every `'alloc`.
#[test]
fn bump_string_global_is_decodable_owned_() {
    assert_owned_::<BumpString<Global>>();
}
