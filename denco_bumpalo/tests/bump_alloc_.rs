//! `BumpAlloc` 与 `BumpString` 的集成测试。
//!
//! Integration tests for `BumpAlloc` and `BumpString`.

#![feature(impl_trait_in_assoc_type)]

use core::{
    future::Future,
    task::{Context, Poll, Waker},
};

use abs_cancel::{NonCancellableToken, TrMayCancel};
use abs_denco::de::TrDecodable;
use denco_bumpalo::{BumpAlloc, BumpString};
use mp_denco::MpDecoder;

/// 立即完成的测试执行器。
fn block_on_<F: Future>(future: F) -> F::Output {
    let mut future = core::pin::pin!(future);
    let waker = Waker::noop();
    let mut context = Context::from_waker(waker);
    loop {
        match future.as_mut().poll(&mut context) {
            Poll::Ready(value) => return value,
            Poll::Pending => core::hint::spin_loop(),
        }
    }
}

/// 测试目标：`Vec` 可以用 `BumpAlloc` 作为 allocator 完成解码。
/// Test goal: `Vec` can decode with `BumpAlloc` as its allocator.
///
/// - 手段：用 MessagePack 数组 `[0x93, 0x01, 0x02, 0x03]` 解码
///   `Vec<u8, BumpAlloc>`。
///   / How: decode `Vec<u8, BumpAlloc>` from the MessagePack array
///   `[0x93, 0x01, 0x02, 0x03]`.
/// - 判断：结果等于 `[1, 2, 3]`，说明 `A: Allocator + Default` 的容器路径
///   在 `BumpAlloc` 上成立。
///   / What: the result equals `[1, 2, 3]`, proving the `A: Allocator + Default`
///   container path works with `BumpAlloc`.
#[test]
fn decode_vec_with_bump_alloc() {
    let bytes = [0x93u8, 0x01, 0x02, 0x03];
    let mut input: &[u8] = &bytes;
    let decoder = MpDecoder::new(&mut input);
    let value: Vec<u8, BumpAlloc> = block_on_(
        Vec::<u8, BumpAlloc>::decode_async(decoder).may_cancel_with(NonCancellableToken::new()),
    )
    .expect("解码应当成功");
    assert_eq!(&*value, &[1u8, 2, 3]);
}

/// 测试目标：`BumpString`（`Vec<u8, A>` 的薄封装）可以用 `BumpAlloc` 解码。
/// Test goal: `BumpString` (a thin wrapper over `Vec<u8, A>`) decodes with `BumpAlloc`.
///
/// - 手段：用 MessagePack fixstr `0xa5` + `"hello"` 解码 `BumpString<BumpAlloc>`。
///   / How: decode `BumpString<BumpAlloc>` from the MessagePack fixstr `0xa5` + `"hello"`.
/// - 判断：字符串内容为 `"hello"`，说明给 `String` 补 allocator 参数的薄封装可行。
///   / What: the content is `"hello"`, proving the thin wrapper that gives `String`
///   an allocator parameter works.
#[test]
fn decode_bump_string() {
    let bytes = [0xa5u8, b'h', b'e', b'l', b'l', b'o'];
    let mut input: &[u8] = &bytes;
    let decoder = MpDecoder::new(&mut input);
    let value: BumpString<BumpAlloc> = block_on_(
        BumpString::<BumpAlloc>::decode_async(decoder).may_cancel_with(NonCancellableToken::new()),
    )
    .expect("解码应当成功");
    assert_eq!(&*value, "hello");
    assert_eq!(value.as_bytes_(), b"hello");
}

/// 测试目标：引用版解码把值直接放进 bump arena，返回 `&'alloc mut Point`。
/// Test goal: the reference version places the value in the bump arena and
/// returns `&'alloc mut Point`.
///
/// - 手段：用 MessagePack 数组 `[0x92, 0x01, 0x02]` 解码 `&mut Point`，
///   decoder 由 `MpDecoder::new_in(&mut input, &bump)` 构造。
///   / How: decode `&mut Point` from the MessagePack array `[0x92, 0x01, 0x02]`
///   with a decoder built by `MpDecoder::new_in(&mut input, &bump)`.
/// - 判断：返回的引用字段为 `1` / `2`，且 arena 的 `allocated_bytes()` 增长，
///   说明 `BumpAlloc` 的 `TrArena::Ref<T> = &'alloc mut T` 路径成立。
///   / What: the referenced fields are `1` / `2` and the arena's
///   `allocated_bytes()` grows, proving the `TrArena::Ref<T> = &'alloc mut T`
///   path of `BumpAlloc`.
#[derive(Debug, PartialEq, denco_macro::TrDecodable)]
struct Point {
    x_: u32,
    y_: u32,
}

#[test]
fn decode_into_bump_arena_reference() {
    let bump = BumpAlloc::new();
    let bytes = [0x92u8, 0x01, 0x02];
    let mut input: &[u8] = &bytes;
    let decoder = MpDecoder::new_in(&mut input, &bump);
    let value: &mut Point = block_on_(
        <&mut Point as TrDecodable<'_, '_, BumpAlloc>>::decode_async(decoder)
            .may_cancel_with(NonCancellableToken::new()),
    )
    .expect("引用版解码应当成功");
    assert_eq!(value.x_, 1);
    assert_eq!(value.y_, 2);
    assert!(bump.allocated_bytes() > 0, "引用版解码应使用 arena");
}
