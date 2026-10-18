//! `mp_denco` 集成测试共享的测试类型与辅助函数。
//!
//! 这里只放 `ring_` 与 `socket_` 两个集成测试都会用到的内容：一个立即完成的
//! 执行器、一个带 5 秒上限的线程包装、`buffex::Ring` 的读写/排空辅助函数，
//! 以及若干同时实现 serde 与 denco 编解码的值类型。
//!
//! Shared test types and helpers for the `mp_denco` integration tests.

// 两个测试文件各自只使用其中一部分类型，未使用项不应产生告警。
#![allow(dead_code)]

use core::{
    borrow::{Borrow, BorrowMut},
    fmt,
    future::{self, Future},
    mem::MaybeUninit,
    task::{Context, Poll},
};
use std::{
    sync::mpsc::{self, RecvTimeoutError},
    task::Waker,
    time::Duration,
};

use abs_buff::{Demand, buffer::TrBuffSegmRef};
use abs_cancel::{NonCancellableToken, TrCancellationToken, TrMayCancel};
use abs_denco::{
    de::{self, TrDecodable, TrDecoder, Visitor},
    ser::{TrEncodable, TrEncoder},
};
use buffex::ring::{Ring, RingReader};
use gen_mcf2::gen_may_cancel_future;
use mp_denco::{MpDecoder, MpEncoder};
use serde::{Deserialize, Serialize};

/// park/取消类测试的统一超时上限。
pub const TIMEOUT_: Duration = Duration::from_secs(5);

/// 立即完成、但对 `Pending` 让出 CPU 的测试执行器。
///
/// 使用 noop waker 轮询：取消令牌与 ring 的 park/wake 都由真实线程推进，
/// 本执行器只要反复 poll 就能观察到状态变化。
///
/// A test executor that yields on `Pending` and polls with a noop waker.
pub fn block_on_<F: Future>(future: F) -> F::Output {
    let mut cx = Context::from_waker(Waker::noop());
    let mut future = core::pin::pin!(future);
    loop {
        match future.as_mut().poll(&mut cx) {
            Poll::Ready(output) => return output,
            Poll::Pending => std::thread::yield_now(),
        }
    }
}

/// 在独立线程中执行 `f`，超过 [`TIMEOUT_`] 仍未结束则让测试失败。
///
/// 这样即使被测 future 在 park 点挂死，测试也会在 5 秒内报错而不是拖住
/// 整个测试进程。
///
/// Run `f` on a separate thread and fail the test if it does not finish
/// within [`TIMEOUT_`].
pub fn with_timeout_<F>(label: &'static str, f: F)
where
    F: FnOnce() + Send + 'static,
{
    let (tx, rx) = mpsc::channel::<()>();
    let handle = std::thread::spawn(move || {
        f();
        let _ = tx.send(());
    });
    match rx.recv_timeout(TIMEOUT_) {
        Ok(()) | Err(RecvTimeoutError::Disconnected) => {
            handle.join().expect("带超时的测试线程不应 panic");
        }
        Err(RecvTimeoutError::Timeout) => {
            panic!("`{label}` 未能在 {TIMEOUT_:?} 内结束（疑似 park 死锁）");
        }
    }
}

/// 测试用 ring 的具体类型：定长 `u8` 缓冲。
///
/// Concrete ring type used by the ring tests.
pub type TestRing = Ring<Box<[MaybeUninit<u8>]>>;

/// 按容量构造一个测试 ring。
///
/// Construct a test ring with the given byte capacity.
pub fn new_ring_(cap: usize) -> TestRing {
    TestRing::try_new(Box::<[u8]>::new_uninit_slice(cap)).expect("ring 容量应当在允许区间内")
}

/// 把 `value` 编码进 ring。
///
/// Encode `value` into the ring.
pub fn encode_ring_<T>(ring: &mut TestRing, value: &T)
where
    T: TrEncodable,
{
    let (mut writer, _reader) = TestRing::split(ring);
    block_on_(async {
        value
            .encode_async(MpEncoder::new(&mut writer))
            .may_cancel_with(NonCancellableToken::new())
            .await
    })
    .expect("向 ring 编码应当成功");
}

/// 从 ring 解码一个值；读端直接交给解码器走 owned 路径。
///
/// Decode one value from the ring; the reader is passed to the decoder
/// directly and always takes the owned path.
pub fn decode_ring_<'de, T>(ring: &mut TestRing) -> T
where
    T: TrDecodable<'de>,
{
    let (_writer, mut reader) = TestRing::split(ring);
    block_on_(async {
        T::decode_async(MpDecoder::new(&mut reader))
            .may_cancel_with(NonCancellableToken::new())
            .await
    })
    .expect("从 ring 解码应当成功")
}

/// 把 ring 中当前全部已提交字节搬进 `Vec`（仅测试断言使用）。
///
/// Drain all committed bytes from the ring into a `Vec` for assertions.
pub fn drain_ring_(ring: &mut TestRing) -> Vec<u8> {
    let (_writer, mut reader) = TestRing::split(ring);
    drain_reader_(&mut reader)
}

/// 把读端当前可读的数据全部消费到 `Vec`。
///
/// 段内容是**窥视**，必须通过 `move_items_to_as_buff` 搬出才会推进读位置。
///
/// Consume every readable byte from `reader` into a `Vec`.
pub fn drain_reader_<S, B>(reader: &mut RingReader<S, B, u8>) -> Vec<u8>
where
    S: Borrow<Ring<B, u8>>,
    B: BorrowMut<[MaybeUninit<u8>]>,
{
    let total = reader.ring_state().data_size();
    let mut out = vec![0u8; total];
    let mut filled = 0usize;
    while filled < total {
        let demand = Demand::no_more_than(total - filled);
        let mut segm = reader
            .try_read(&demand)
            .pick_left()
            .expect("ring 中应当仍有可读数据");
        let moved = segm.move_items_to_as_buff(&mut out[filled..]);
        assert!(moved > 0usize, "可读段至少应搬出 1 字节");
        filled += moved;
    }
    out
}

//-- ---- 测试值类型 -----------------------------------------------------------

/// 透明包装 `u64` 的测试类型。
///
/// A transparent `u64` newtype for scalar roundtrips.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct U64Val(pub u64);

impl TrEncodable for U64Val {
    type EncodeAsync<'a, S>
        = S::EncodeU64Async<'a>
    where
        S: TrEncoder + 'a,
        S::Ok: 'a,
        S::Error: 'a,
        Self: 'a;

    fn encode_async<'a, S>(&'a self, encoder: S) -> Self::EncodeAsync<'a, S>
    where
        S: TrEncoder + 'a,
        S::Ok: 'a,
        S::Error: 'a,
        Self: 'a,
    {
        encoder.encode_u64(self.0)
    }
}

/// [`U64Val`] 的访问者。
struct U64ValVisitor;

impl<'de> Visitor<'de> for U64ValVisitor {
    type Value = U64Val;

    fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("a u64")
    }

    fn visit_u64<'a, E>(
        self,
        v: u64,
    ) -> impl TrMayCancel<'a, MayCancelOutput = Result<Self::Value, E>>
    where
        Self: 'a,
        Self::Value: 'a,
        E: de::Error + 'a,
    {
        future::ready(Ok(U64Val(v)))
    }
}

#[gen_may_cancel_future(DecodeU64ValOp, pub)]
async fn decode_u64_val_async_<'de, 'a, D, C>(de: D, cancel: C) -> Result<U64Val, D::Error>
where
    'de: 'a,
    D: TrDecoder<'de> + 'a,
    D::Error: 'a,
    C: TrCancellationToken,
{
    de.decode_u64(U64ValVisitor).may_cancel_with(cancel).await
}

impl<'de> TrDecodable<'de> for U64Val {
    type DecodeAsync<'a, D>
        = DecodeU64ValOpAsync<'de, 'a, 'a, D>
    where
        'de: 'a,
        D: TrDecoder<'de> + 'a,
        D::Error: 'a,
        Self: 'a;

    fn decode_async<'a, D>(de: D) -> Self::DecodeAsync<'a, D>
    where
        'de: 'a,
        D: TrDecoder<'de> + 'a,
        D::Error: 'a,
        Self: 'a,
    {
        DecodeU64ValOpAsync::new(de)
    }
}

/// 透明包装 `String` 的测试类型。
///
/// A transparent `String` newtype for long-string roundtrips.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct StrVal(pub String);

impl TrEncodable for StrVal {
    type EncodeAsync<'a, S>
        = S::EncodeStrAsync<'a, 'a>
    where
        S: TrEncoder + 'a,
        S::Ok: 'a,
        S::Error: 'a,
        Self: 'a;

    fn encode_async<'a, S>(&'a self, encoder: S) -> Self::EncodeAsync<'a, S>
    where
        S: TrEncoder + 'a,
        S::Ok: 'a,
        S::Error: 'a,
        Self: 'a,
    {
        encoder.encode_str(self.0.as_str())
    }
}

/// [`StrVal`] 的访问者。
struct StrValVisitor;

impl<'de> Visitor<'de> for StrValVisitor {
    type Value = StrVal;

    fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("a string")
    }

    fn visit_str<'a, E>(
        self,
        v: &'a str,
    ) -> impl TrMayCancel<'a, MayCancelOutput = Result<Self::Value, E>>
    where
        Self: 'a,
        Self::Value: 'a,
        E: de::Error + 'a,
    {
        future::ready(Ok(StrVal(String::from(v))))
    }
}

#[gen_may_cancel_future(DecodeStrValOp, pub)]
async fn decode_str_val_async_<'de, 'a, D, C>(de: D, cancel: C) -> Result<StrVal, D::Error>
where
    'de: 'a,
    D: TrDecoder<'de> + 'a,
    D::Error: 'a,
    C: TrCancellationToken,
{
    de.decode_string(StrValVisitor)
        .may_cancel_with(cancel)
        .await
}

impl<'de> TrDecodable<'de> for StrVal {
    type DecodeAsync<'a, D>
        = DecodeStrValOpAsync<'de, 'a, 'a, D>
    where
        'de: 'a,
        D: TrDecoder<'de> + 'a,
        D::Error: 'a,
        Self: 'a;

    fn decode_async<'a, D>(de: D) -> Self::DecodeAsync<'a, D>
    where
        'de: 'a,
        D: TrDecoder<'de> + 'a,
        D::Error: 'a,
        Self: 'a,
    {
        DecodeStrValOpAsync::new(de)
    }
}

/// 使用 `serde_bytes` 的二进制测试类型。
///
/// A binary newtype (`serde_bytes`) for long-binary roundtrips.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BytesVal(#[serde(with = "serde_bytes")] pub Vec<u8>);

impl TrEncodable for BytesVal {
    type EncodeAsync<'a, S>
        = S::EncodeBytesAsync<'a, 'a>
    where
        S: TrEncoder + 'a,
        S::Ok: 'a,
        S::Error: 'a,
        Self: 'a;

    fn encode_async<'a, S>(&'a self, encoder: S) -> Self::EncodeAsync<'a, S>
    where
        S: TrEncoder + 'a,
        S::Ok: 'a,
        S::Error: 'a,
        Self: 'a,
    {
        encoder.encode_bytes(self.0.as_slice())
    }
}

/// [`BytesVal`] 的访问者。
struct BytesValVisitor;

impl<'de> Visitor<'de> for BytesValVisitor {
    type Value = BytesVal;

    fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("a byte array")
    }

    fn visit_bytes<'a, E>(
        self,
        v: &'a [u8],
    ) -> impl TrMayCancel<'a, MayCancelOutput = Result<Self::Value, E>>
    where
        Self: 'a,
        Self::Value: 'a,
        E: de::Error + 'a,
    {
        future::ready(Ok(BytesVal(v.to_vec())))
    }
}

#[gen_may_cancel_future(DecodeBytesValOp, pub)]
async fn decode_bytes_val_async_<'de, 'a, D, C>(de: D, cancel: C) -> Result<BytesVal, D::Error>
where
    'de: 'a,
    D: TrDecoder<'de> + 'a,
    D::Error: 'a,
    C: TrCancellationToken,
{
    de.decode_bytes(BytesValVisitor)
        .may_cancel_with(cancel)
        .await
}

impl<'de> TrDecodable<'de> for BytesVal {
    type DecodeAsync<'a, D>
        = DecodeBytesValOpAsync<'de, 'a, 'a, D>
    where
        'de: 'a,
        D: TrDecoder<'de> + 'a,
        D::Error: 'a,
        Self: 'a;

    fn decode_async<'a, D>(de: D) -> Self::DecodeAsync<'a, D>
    where
        'de: 'a,
        D: TrDecoder<'de> + 'a,
        D::Error: 'a,
        Self: 'a,
    {
        DecodeBytesValOpAsync::new(de)
    }
}
