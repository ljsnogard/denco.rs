//! 重导出的 allocator `String<Global>` 对 `TrDecodableOwned` 的验证。
//!
//! Checks that the re-exported allocator `String<Global>` satisfies
//! `TrDecodableOwned`.

use std::alloc::Global;

use abs_denco::de::TrDecodableOwned;
use denco_bumpalo::String as AllocString;

/// 要求 `T` 满足 [`TrDecodableOwned`]。
/// Require `T` to satisfy [`TrDecodableOwned`].
fn assert_owned_<T: TrDecodableOwned>() {}

/// 测试目标：`denco_bumpalo` 重导出的 `String<Global>` 与 arena 无关，满足
/// `TrDecodableOwned`。
/// Test goal: the allocator `String<Global>` re-exported by `denco_bumpalo` is
/// arena-agnostic and satisfies `TrDecodableOwned`.
///
/// - 手段：编译期断言 `assert_owned_::<AllocString<Global>>()`。
///   / How: a compile-time assertion on `AllocString<Global>`.
/// - 判断：断言能通过编译；该 impl 对所有 `'alloc` 成立。
///   / What: the assertion compiles; the impl holds for every `'alloc`.
#[test]
fn alloc_string_global_is_decodable_owned_() {
    assert_owned_::<AllocString<Global>>();
}
