//! 带生命周期结构体的端到端测试：`&'alloc str` / `&'alloc [u8]` 借用 arena。
//!
//! End-to-end tests for lifetime-carrying structs whose `&'alloc str` /
//! `&'alloc [u8]` fields borrow the decoder's arena.

#![feature(impl_trait_in_assoc_type)]

use core::{
    future::IntoFuture,
    task::{Context, Poll, Waker},
};

use abs_cancel::{NonCancellableToken, TrMayCancel};
use abs_denco::{de::TrDecodable, ser::TrEncodable};
use denco_bumpalo::{BumpArena, BumpBox};
use json_denco::{JsonDecoder, JsonEncoder};
use mp_denco::{MpDecoder, MpEncoder};

/// 立即完成的测试执行器。
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

/// 字段同时借用 arena 的结构体。
#[derive(Debug, PartialEq, denco_macro::TrEncodable, denco_macro::TrDecodable)]
struct Borrowed<'alloc> {
    name_: &'alloc str,
    data_: &'alloc [u8],
}

/// 用 denco 的 MessagePack 编码器把值写入 `Vec<u8>`。
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

/// 测试目标：MessagePack 能把 `Borrowed<'alloc>` 解码成 `BumpBox<'alloc, Borrowed<'alloc>>`。
/// Test goal: MessagePack decodes `Borrowed<'alloc>` into
/// `BumpBox<'alloc, Borrowed<'alloc>>`.
///
/// - 手段：用 denco 编码器编码 `Borrowed { name_: "hello", data_: [1,2,3] }`，
///   再用 `MpDecoder::new_in` 解码到同一个 bump arena。
///   / How: encode `Borrowed { name_: "hello", data_: [1,2,3] }` with the denco
///   encoder, then decode it with `MpDecoder::new_in` into the same bump arena.
/// - 判断：字段内容一致；arena 的 `allocated_bytes()` 增长；输入缓冲离开作用域后
///   仍能读取字段。
///   / What: the fields match, the arena's `allocated_bytes()` grows, and the
///   fields are still readable after the input buffer is dropped.
#[test]
fn decode_borrowed_from_mp() {
    let bump = BumpArena::new();
    let bytes = encode_mp_(&Borrowed {
        name_: "hello",
        data_: &[1, 2, 3],
    });
    let value: BumpBox<'_, Borrowed<'_>> = {
        let mut input: &[u8] = &bytes;
        let decoder = MpDecoder::new_in(&mut input, &bump);
        block_on_(
            <BumpBox<'_, Borrowed<'_>> as TrDecodable<'_, '_, BumpArena>>::decode_async(decoder)
                .may_cancel_with(NonCancellableToken::new()),
        )
        .expect("MessagePack 借用版解码应当成功")
    };
    assert_eq!(value.name_, "hello");
    assert_eq!(value.data_, &[1, 2, 3]);
    assert!(bump.allocated_bytes() > 0, "解码应使用传入的 arena");
}

/// 测试目标：JSON 能把 `Borrowed<'alloc>` 解码成 `BumpBox<'alloc, Borrowed<'alloc>>`。
/// Test goal: JSON decodes `Borrowed<'alloc>` into
/// `BumpBox<'alloc, Borrowed<'alloc>>`.
///
/// - 手段：用 denco 编码器编码同一结构体，再用 `JsonDecoder::new_in` 解码到
///   同一个 bump arena。
///   / How: encode the same struct with the denco encoder, then decode it with
///   `JsonDecoder::new_in` into the same bump arena.
/// - 判断：字段内容一致，arena 的 `allocated_bytes()` 增长。
///   / What: the fields match and the arena's `allocated_bytes()` grows.
#[test]
fn decode_borrowed_from_json() {
    let bump = BumpArena::new();
    let bytes = encode_json_(&Borrowed {
        name_: "hel\"lo",
        data_: &[7, 8, 9],
    });
    let mut input: &[u8] = &bytes;
    let mut decoder = JsonDecoder::new_in(&mut input, &bump);
    let value: BumpBox<'_, Borrowed<'_>> = block_on_(
        <BumpBox<'_, Borrowed<'_>> as TrDecodable<'_, '_, BumpArena>>::decode_async(&mut decoder)
            .may_cancel_with(NonCancellableToken::new()),
    )
    .expect("JSON 借用版解码应当成功");
    assert_eq!(value.name_, "hel\"lo");
    assert_eq!(value.data_, &[7, 8, 9]);
    assert!(bump.allocated_bytes() > 0, "JSON 解码应使用传入的 arena");
}

/// 测试目标：`BumpBox` 的生命周期只跟随 arena，不跟随输入缓冲。
/// Test goal: `BumpBox`'s lifetime follows the arena, not the input buffer.
///
/// - 手段：在一个内层作用域里编码并解码，返回 `BumpBox`，内层作用域结束后再使用。
///   / How: encode and decode inside an inner scope, return the `BumpBox`, and use
///   it after the inner scope ends.
/// - 判断：输入缓冲已释放后仍能读到字段。
///   / What: the fields are still readable after the input buffer is gone.
#[test]
fn borrowed_fields_outlive_input() {
    let bump = BumpArena::new();
    let value: BumpBox<'_, Borrowed<'_>> = {
        let bytes = encode_mp_(&Borrowed {
            name_: "persisted",
            data_: &[4, 5, 6],
        });
        let mut input: &[u8] = &bytes;
        let decoder = MpDecoder::new_in(&mut input, &bump);
        block_on_(
            <BumpBox<'_, Borrowed<'_>> as TrDecodable<'_, '_, BumpArena>>::decode_async(decoder)
                .may_cancel_with(NonCancellableToken::new()),
        )
        .expect("MessagePack 借用版解码应当成功")
    };
    assert_eq!(value.name_, "persisted");
    assert_eq!(value.data_, &[4, 5, 6]);
}
