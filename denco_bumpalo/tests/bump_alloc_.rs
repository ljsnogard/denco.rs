//! `BumpArena` 与 `BumpString` 的集成测试。
//!
//! Integration tests for `BumpArena` and `BumpString`.

#![feature(impl_trait_in_assoc_type)]

use core::{
    future::Future,
    task::{Context, Poll, Waker},
};

use abs_cancel::{NonCancellableToken, TrMayCancel};
use abs_denco::de::TrDecodable;
use denco_bumpalo::{BumpArena, BumpBox, BumpString};
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

/// 测试目标：`Vec` 可以借用 decoder 的 `BumpArena` 作为 allocator 完成解码。
/// Test goal: `Vec` can borrow the decoder's `BumpArena` as its allocator.
///
/// - 手段：用 `MpDecoder::new_in(&mut input, &bump)` 解码
///   `Vec<u8, &BumpArena>`，输入是 MessagePack 数组 `[0x93, 0x01, 0x02, 0x03]`。
///   / How: decode `Vec<u8, &BumpArena>` from the MessagePack array
///   `[0x93, 0x01, 0x02, 0x03]` with `MpDecoder::new_in(&mut input, &bump)`.
/// - 判断：结果等于 `[1, 2, 3]`，且 `bump.allocated_bytes() > 0`，说明容器与
///   decoder 共享同一个 arena，而不是各自 `Default` 出一个新 arena。
///   / What: the result equals `[1, 2, 3]` and `bump.allocated_bytes() > 0`,
///   proving the container shares the decoder's arena instead of creating a new
///   one via `Default`.
#[test]
fn decode_vec_with_bump_alloc() {
    let bump = BumpArena::new();
    let bytes = [0x93u8, 0x01, 0x02, 0x03];
    let mut input: &[u8] = &bytes;
    let decoder = MpDecoder::new_in(&mut input, &bump);
    let value: Vec<u8, &BumpArena> = block_on_(
        Vec::<u8, &BumpArena>::decode_async(decoder).may_cancel_with(NonCancellableToken::new()),
    )
    .expect("解码应当成功");
    assert_eq!(&*value, &[1u8, 2, 3]);
    assert!(bump.allocated_bytes() > 0, "容器应使用 decoder 的 arena");
}

/// 测试目标：`BumpString` 可以借用 decoder 的 `BumpArena` 完成解码。
/// Test goal: `BumpString` can borrow the decoder's `BumpArena`.
///
/// - 手段：用 `MpDecoder::new_in(&mut input, &bump)` 解码
///   `BumpString<&BumpArena>`，输入是 MessagePack fixstr `0xa5` + `"hello"`。
///   / How: decode `BumpString<&BumpArena>` from the MessagePack fixstr
///   `0xa5` + `"hello"` with `MpDecoder::new_in(&mut input, &bump)`.
/// - 判断：字符串内容为 `"hello"`，且 arena 的 `allocated_bytes()` 增长。
///   / What: the content is `"hello"` and the arena's `allocated_bytes()` grows.
#[test]
fn decode_bump_string() {
    let bump = BumpArena::new();
    let bytes = [0xa5u8, b'h', b'e', b'l', b'l', b'o'];
    let mut input: &[u8] = &bytes;
    let decoder = MpDecoder::new_in(&mut input, &bump);
    let value: BumpString<&BumpArena> = block_on_(
        BumpString::<&BumpArena>::decode_async(decoder).may_cancel_with(NonCancellableToken::new()),
    )
    .expect("解码应当成功");
    assert_eq!(&*value, "hello");
    assert_eq!(value.as_bytes_(), b"hello");
    assert!(bump.allocated_bytes() > 0, "字符串应使用 decoder 的 arena");
}

/// 测试目标：引用版解码把值放进 bump arena，返回 `BumpBox<Point>` 句柄。
/// Test goal: the reference version places the value in the bump arena and
/// returns a `BumpBox<Point>` handle.
///
/// - 手段：用 MessagePack 数组 `[0x92, 0x01, 0x02]` 解码 `BumpBox<Point>`，
///   decoder 由 `MpDecoder::new_in(&mut input, &bump)` 构造。
///   / How: decode `BumpBox<Point>` from the MessagePack array
///   `[0x92, 0x01, 0x02]` with a decoder built by
///   `MpDecoder::new_in(&mut input, &bump)`.
/// - 判断：句柄解引用后的字段为 `1` / `2`，`BumpBox` 满足
///   `abs_mm::res_man::TrUnique`，且 arena 的 `allocated_bytes()` 增长。
///   / What: the dereferenced fields are `1` / `2`, `BumpBox` satisfies
///   `abs_mm::res_man::TrUnique`, and the arena's `allocated_bytes()` grows.
#[derive(Debug, PartialEq, denco_macro::TrDecodable)]
struct Point {
    x_: u32,
    y_: u32,
}

#[test]
fn decode_into_bump_arena_reference() {
    let bump = BumpArena::new();
    let bytes = [0x92u8, 0x01, 0x02];
    let mut input: &[u8] = &bytes;
    let decoder = MpDecoder::new_in(&mut input, &bump);
    let value: BumpBox<'_, Point> = block_on_(
        <BumpBox<'_, Point> as TrDecodable<'_, '_, BumpArena>>::decode_async(decoder)
            .may_cancel_with(NonCancellableToken::new()),
    )
    .expect("引用版解码应当成功");
    assert_eq!(value.x_, 1);
    assert_eq!(value.y_, 2);
    assert!(bump.allocated_bytes() > 0, "引用版解码应使用 arena");

    fn assert_unique<T: abs_denco::TrUnique<Item = Point>>(_: &T) {}
    assert_unique(&value);
}

/// 测试目标：同一个 `BumpArena` 也能用于 JSON 的引用版解码。
/// Test goal: the same `BumpArena` also works for JSON reference decoding.
///
/// - 手段：用 JSON 对象 `{"x_": 1, "y_": 2}` 解码 `BumpBox<Point>`，decoder
///   由 `JsonDecoder::new_in(&mut input, &bump)` 构造。
///   / How: decode `BumpBox<Point>` from the JSON object `{"x_": 1, "y_": 2}`
///   with a decoder built by `JsonDecoder::new_in(&mut input, &bump)`.
/// - 判断：句柄解引用后的字段为 `1` / `2`，且 arena 的 `allocated_bytes()` 增长。
///   / What: the dereferenced fields are `1` / `2`, and the arena's
///   `allocated_bytes()` grows.
#[test]
fn decode_into_bump_arena_reference_from_json() {
    let bump = BumpArena::new();
    let bytes = br#"{"x_": 1, "y_": 2}"#;
    let mut input: &[u8] = bytes;
    let mut decoder = json_denco::JsonDecoder::new_in(&mut input, &bump);
    let value: BumpBox<'_, Point> = block_on_(
        <BumpBox<'_, Point> as TrDecodable<'_, '_, BumpArena>>::decode_async(&mut decoder)
            .may_cancel_with(NonCancellableToken::new()),
    )
    .expect("JSON 引用版解码应当成功");
    assert_eq!(value.x_, 1);
    assert_eq!(value.y_, 2);
    assert!(bump.allocated_bytes() > 0, "JSON 引用版解码应使用 arena");
}

/// 测试目标：`BumpBox` 的生命周期只跟随 arena，不跟随输入缓冲。
/// Test goal: `BumpBox`'s lifetime follows the arena, not the input buffer.
///
/// - 手段：在一个内层作用域里用局部 `bytes` / `input` 解码，返回 `BumpBox`，
///   内层作用域结束后再使用它。
///   / How: decode inside an inner scope with local `bytes` / `input`, return the
///   `BumpBox`, and use it after the inner scope ends.
/// - 判断：输入缓冲已释放后仍能读到字段 `1` / `2`。
///   / What: fields `1` / `2` are still readable after the input buffer is gone.
#[test]
fn decode_bump_box_outlives_input() {
    let bump = BumpArena::new();
    let value: BumpBox<'_, Point> = {
        let bytes = [0x92u8, 0x01, 0x02];
        let mut input: &[u8] = &bytes;
        let decoder = MpDecoder::new_in(&mut input, &bump);
        block_on_(
            <BumpBox<'_, Point> as TrDecodable<'_, '_, BumpArena>>::decode_async(decoder)
                .may_cancel_with(NonCancellableToken::new()),
        )
        .expect("引用版解码应当成功")
    };
    assert_eq!(value.x_, 1);
    assert_eq!(value.y_, 2);
}
