#![feature(allocator_ext)]
//! `BumpAlloc` 与 MessagePack 解码的集成测试。
//!
//! Integration tests for `BumpAlloc` with the MessagePack decoder.

use core::{
    future::Future,
    task::{Context, Poll, Waker},
};
use std::{rc::Rc, sync::Arc};

use abs_cancel::{NonCancellableToken, TrMayCancel};
use abs_denco::de::{BumpAlloc, TrDecodable};
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
/// - 判断：解码结果等于 `[1, 2, 3]`，说明 `A: Allocator + Default` 的容器路径
///   在 `BumpAlloc` 上成立。
///   / What: the result equals `[1, 2, 3]`, proving the `A: Allocator + Default`
///   container path works with `BumpAlloc`.
#[test]
fn decode_vec_with_bump_alloc() {
    let bytes = [0x93u8, 0x01, 0x02, 0x03];
    let mut input: &[u8] = &bytes;
    let decoder = MpDecoder::new(&mut input);
    let value: Vec<u8, BumpAlloc> = block_on_(
        Vec::<u8, BumpAlloc>::decode_async(decoder)
            .may_cancel_with(NonCancellableToken::new()),
    )
    .expect("解码应当成功");
    assert_eq!(&*value, &[1u8, 2, 3]);
}

/// 测试目标：`Box` / `Rc` / `Arc` 都能用 `BumpAlloc` 作为 allocator 解码。
/// Test goal: `Box` / `Rc` / `Arc` can decode with `BumpAlloc` as their allocator.
///
/// - 手段：用 MessagePack 正整数 `0x2a`（42）分别解码
///   `Box<u32, BumpAlloc>`、`Rc<u32, BumpAlloc>`、`Arc<u32, BumpAlloc>`。
///   / How: decode `Box<u32, BumpAlloc>`, `Rc<u32, BumpAlloc>` and
///   `Arc<u32, BumpAlloc>` from the MessagePack positive fixint `0x2a` (42).
/// - 判断：三种容器的内容都是 42，说明 `Rc` / `Arc` 的自定义 allocator 路径
///   没有被 `Default` 限制挡住。
///   / What: all three containers hold 42, proving the custom-allocator paths of
///   `Rc` / `Arc` are not blocked by the `Default` bound.
#[test]
fn decode_box_rc_arc_with_bump_alloc() {
    let bytes = [0x2au8];

    let mut input: &[u8] = &bytes;
    let decoder = MpDecoder::new(&mut input);
    let boxed: Box<u32, BumpAlloc> = block_on_(
        Box::<u32, BumpAlloc>::decode_async(decoder)
            .may_cancel_with(NonCancellableToken::new()),
    )
    .expect("Box 解码应当成功");
    assert_eq!(*boxed, 42);

    let mut input: &[u8] = &bytes;
    let decoder = MpDecoder::new(&mut input);
    let rc: Rc<u32, BumpAlloc> = block_on_(
        Rc::<u32, BumpAlloc>::decode_async(decoder)
            .may_cancel_with(NonCancellableToken::new()),
    )
    .expect("Rc 解码应当成功");
    assert_eq!(*rc, 42);

    let mut input: &[u8] = &bytes;
    let decoder = MpDecoder::new(&mut input);
    let arc: Arc<u32, BumpAlloc> = block_on_(
        Arc::<u32, BumpAlloc>::decode_async(decoder)
            .may_cancel_with(NonCancellableToken::new()),
    )
    .expect("Arc 解码应当成功");
    assert_eq!(*arc, 42);
}
