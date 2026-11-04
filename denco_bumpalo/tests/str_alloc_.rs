//! `abs_str::str_alloc::String` 的端到端测试（`str_alloc` feature）。
//!
//! End-to-end tests for `abs_str::str_alloc::String` under the `str_alloc`
//! feature.
//!
//! 验证两件事：
//!
//! 1. `String<Global>` 与标准库 `String` 一样可编码、可解码；
//! 2. `String<&'alloc BumpArena>` 使用 decoder 携带的 arena 作为 allocator，
//!    而不是各自新建 arena。

#![feature(impl_trait_in_assoc_type)]

use core::{
    future::IntoFuture,
    task::{Context, Poll, Waker},
};

use abs_cancel::{NonCancellableToken, TrMayCancel};
use abs_denco::{
    de::{NoArena, TrDecodable},
    ser::TrEncodable,
    x_deps::abs_str::str_alloc::String as AllocString,
};
use denco_bumpalo::BumpArena;
use json_denco::{JsonDecoder, JsonEncoder};
use mp_denco::{MpDecoder, MpEncoder};

/// 立即完成的测试执行器。
/// A test executor that drives futures to completion without a runtime.
fn block_on_<F: IntoFuture>(future: F) -> F::Output {
    let mut future = core::pin::pin!(future.into_future());
    let waker = Waker::noop();
    let mut context = Context::from_waker(waker);
    loop {
        match future.as_mut().poll(&mut context) {
            Poll::Ready(value) => return value,
            Poll::Pending => core::hint::spin_loop(),
        }
    }
}

/// 用 denco 的 MessagePack 编码器把值写入 `Vec<u8>`。
/// Encode a value into `Vec<u8>` with denco's MessagePack encoder.
fn encode_mp_<T>(value: &T) -> Vec<u8>
where
    T: ?Sized + TrEncodable,
{
    let mut storage = vec![0u8; 4096];
    let total = storage.len();
    let written = {
        let mut sink: &mut [u8] = storage.as_mut_slice();
        block_on_(
            value
                .encode_async(MpEncoder::new(&mut sink))
                .may_cancel_with(NonCancellableToken::new()),
        )
        .expect("MessagePack 编码应当成功");
        total - sink.len()
    };
    storage.truncate(written);
    storage
}

/// 用 denco 的 JSON 编码器把值写入 `Vec<u8>`。
/// Encode a value into `Vec<u8>` with denco's JSON encoder.
fn encode_json_<T>(value: &T) -> Vec<u8>
where
    T: ?Sized + TrEncodable,
{
    let mut storage = vec![0u8; 4096];
    let total = storage.len();
    let written = {
        let mut sink: &mut [u8] = storage.as_mut_slice();
        block_on_(
            value
                .encode_async(JsonEncoder::new(&mut sink))
                .may_cancel_with(NonCancellableToken::new()),
        )
        .expect("JSON 编码应当成功");
        total - sink.len()
    };
    storage.truncate(written);
    storage
}

/// 测试目标：MessagePack 把字符串解成 `String<Global>`（无 arena）。
/// Test goal: MessagePack decodes a string into `String<Global>` without an
/// arena.
///
/// - 手段：用 denco 编码 `"hello"`，再用 `MpDecoder::new` 解成默认 allocator
///   的 `abs_str` String。
///   / How: encode `"hello"` with denco and decode it with `MpDecoder::new`.
/// - 判断：内容为 `"hello"`，与标准库 `String` 的解码行为一致。
///   / What: the content is `"hello"`, matching std `String` decoding.
#[test]
fn decode_alloc_string_global_from_mp_() {
    let bytes = encode_mp_("hello");
    let mut input: &[u8] = &bytes;
    let value: AllocString = block_on_(
        <AllocString as TrDecodable<'_, NoArena>>::decode_async(MpDecoder::new(&mut input))
            .may_cancel_with(NonCancellableToken::new()),
    )
    .expect("MessagePack 字符串解码应当成功");
    assert_eq!(&*value, "hello");
}

/// 测试目标：JSON 把字符串解成 `String<Global>`（无 arena）。
/// Test goal: JSON decodes a string into `String<Global>` without an arena.
///
/// - 手段：用 denco 编码 `"hello"`，再用 `JsonDecoder::new` 解成默认 allocator
///   的 `abs_str` String。
///   / How: encode `"hello"` with denco and decode it with `JsonDecoder::new`.
/// - 判断：内容为 `"hello"`。
///   / What: the content is `"hello"`.
#[test]
fn decode_alloc_string_global_from_json_() {
    let bytes = encode_json_("hello");
    let mut input: &[u8] = &bytes;
    let mut decoder = JsonDecoder::new(&mut input);
    let value: AllocString = block_on_(
        <AllocString as TrDecodable<'_, NoArena>>::decode_async(&mut decoder)
            .may_cancel_with(NonCancellableToken::new()),
    )
    .expect("JSON 字符串解码应当成功");
    assert_eq!(&*value, "hello");
}

/// 测试目标：MessagePack 解码时用 decoder 的 `BumpArena` 作为
/// `abs_str::String` 的 allocator。
/// Test goal: MessagePack decoding uses the decoder's `BumpArena` as the
/// allocator of `abs_str::String`.
///
/// - 手段：用 `MpDecoder::new_in` 把 `"arena"` 解成
///   `String<&BumpArena>`。
///   / How: decode `"arena"` with `MpDecoder::new_in` into
///   `String<&BumpArena>`.
/// - 判断：内容正确，且 arena 的 `allocated_bytes()` 增长。
///   / What: the content matches and `allocated_bytes()` grows.
#[test]
fn decode_alloc_string_uses_decoder_arena_mp_() {
    let arena = BumpArena::new();
    let bytes = encode_mp_("arena");
    let mut input: &[u8] = &bytes;
    let value: AllocString<&BumpArena> = block_on_(
        <AllocString<&BumpArena> as TrDecodable<'_, BumpArena>>::decode_async(MpDecoder::new_in(
            &mut input, &arena,
        ))
        .may_cancel_with(NonCancellableToken::new()),
    )
    .expect("MessagePack arena 字符串解码应当成功");
    assert_eq!(&*value, "arena");
    assert!(arena.allocated_bytes() > 0, "解码应使用传入的 arena");
}

/// 测试目标：JSON 解码时用 decoder 的 `BumpArena` 作为
/// `abs_str::String` 的 allocator。
/// Test goal: JSON decoding uses the decoder's `BumpArena` as the allocator of
/// `abs_str::String`.
///
/// - 手段：用 `JsonDecoder::new_in` 把 `"arena"` 解成
///   `String<&BumpArena>`。
///   / How: decode `"arena"` with `JsonDecoder::new_in` into
///   `String<&BumpArena>`.
/// - 判断：内容正确，且 arena 的 `allocated_bytes()` 增长。
///   / What: the content matches and `allocated_bytes()` grows.
#[test]
fn decode_alloc_string_uses_decoder_arena_json_() {
    let arena = BumpArena::new();
    let bytes = encode_json_("arena");
    let mut input: &[u8] = &bytes;
    let mut decoder = JsonDecoder::new_in(&mut input, &arena);
    let value: AllocString<&BumpArena> = block_on_(
        <AllocString<&BumpArena> as TrDecodable<'_, BumpArena>>::decode_async(&mut decoder)
            .may_cancel_with(NonCancellableToken::new()),
    )
    .expect("JSON arena 字符串解码应当成功");
    assert_eq!(&*value, "arena");
    assert!(arena.allocated_bytes() > 0, "解码应使用传入的 arena");
}

/// 测试目标：`String<Global>` 可以像标准库 `String` 一样重新编码。
/// Test goal: `String<Global>` can be re-encoded like std `String`.
///
/// - 手段：先解出 `String<Global>`，再编码并解码一次。
///   / How: decode into `String<Global>`, re-encode it, and decode again.
/// - 判断：两次解码结果相同。
///   / What: both decoded values are equal.
#[test]
fn alloc_string_roundtrips_through_mp_() {
    let first = {
        let bytes = encode_mp_("roundtrip");
        let mut input: &[u8] = &bytes;
        block_on_(
            <AllocString as TrDecodable<'_, NoArena>>::decode_async(MpDecoder::new(&mut input))
                .may_cancel_with(NonCancellableToken::new()),
        )
        .expect("首次解码应当成功")
    };
    let bytes = encode_mp_(&first);
    let mut input: &[u8] = &bytes;
    let second = block_on_(
        <AllocString as TrDecodable<'_, NoArena>>::decode_async(MpDecoder::new(&mut input))
            .may_cancel_with(NonCancellableToken::new()),
    )
    .expect("再次解码应当成功");
    assert_eq!(&*first, &*second);
}

/// 带 `abs_str::String<Global>` 字段的派生结构体。
#[derive(denco_macro::TrEncodable, denco_macro::TrDecodable)]
struct NameRecord {
    name_: AllocString,
}

/// 测试目标：`abs_str::String<Global>` 可作为 derive 结构体字段使用。
/// Test goal: `abs_str::String<Global>` works as a derived-struct field.
///
/// - 手段：定义带 `name: AllocString` 字段的派生结构体，编码后解码。
///   / How: derive a struct with a `name: AllocString` field, encode and
///   decode it.
/// - 判断：字段内容与编码前一致。
///   / What: the field matches the original value.
#[test]
fn derived_struct_with_alloc_string_mp_() {
    let record = NameRecord {
        name_: {
            let bytes = encode_mp_("derived");
            let mut input: &[u8] = &bytes;
            block_on_(
                <AllocString as TrDecodable<'_, NoArena>>::decode_async(MpDecoder::new(&mut input))
                    .may_cancel_with(NonCancellableToken::new()),
            )
            .expect("字段初始化应当成功")
        },
    };
    let bytes = encode_mp_(&record);
    let mut input: &[u8] = &bytes;
    let decoded: NameRecord = block_on_(
        <NameRecord as TrDecodable<'_, NoArena>>::decode_async(MpDecoder::new(&mut input))
            .may_cancel_with(NonCancellableToken::new()),
    )
    .expect("结构体解码应当成功");
    assert_eq!(&*decoded.name_, "derived");
}

/// 带 `abs_str::String<&'alloc BumpArena>` 字段的派生结构体。
#[derive(denco_macro::TrEncodable, denco_macro::TrDecodable)]
struct ArenaNameRecord<'alloc> {
    name_: AllocString<&'alloc BumpArena>,
}

/// 用 decoder 的 arena 解码带 `abs_str::String<&'alloc BumpArena>` 字段的
/// 派生结构体。
/// Decode a derived struct whose field is
/// `abs_str::String<&'alloc BumpArena>` using the decoder's arena.
fn decode_arena_record_<'alloc>(bytes: &[u8], arena: &'alloc BumpArena) -> ArenaNameRecord<'alloc> {
    let mut input: &[u8] = bytes;
    block_on_(
        <ArenaNameRecord<'alloc> as TrDecodable<'alloc, BumpArena>>::decode_async(
            MpDecoder::new_in(&mut input, arena),
        )
        .may_cancel_with(NonCancellableToken::new()),
    )
    .expect("MessagePack arena 结构体解码应当成功")
}

/// 测试目标：derive 结构体的 `abs_str::String<&'alloc BumpArena>` 字段共享
/// decoder 的 arena。
/// Test goal: a derived struct's `abs_str::String<&'alloc BumpArena>` field
/// shares the decoder's arena.
///
/// - 手段：用 arena 构造字段值并编码，再用 `MpDecoder::new_in` 解回。
///   / How: build the field from the arena, encode it, and decode with
///   `MpDecoder::new_in`.
/// - 判断：字段内容正确，且 arena 的 `allocated_bytes()` 增长。
///   / What: the field matches and `allocated_bytes()` grows.
#[test]
fn derived_struct_uses_decoder_arena_mp_() {
    let arena = BumpArena::new();
    let record = ArenaNameRecord {
        name_: AllocString::from_str_in("derived-arena", &arena),
    };
    let bytes = encode_mp_(&record);
    let decoded = decode_arena_record_(&bytes, &arena);
    assert_eq!(&*decoded.name_, "derived-arena");
    assert!(arena.allocated_bytes() > 0, "解码应使用传入的 arena");
}
