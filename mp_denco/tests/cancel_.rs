//! 取消令牌在解码/编码路径上的传播测试。

#![feature(impl_trait_in_assoc_type)]

use core::{
    fmt,
    future::Future,
    task::{Context, Poll},
};
use std::{pin::pin, task::Waker, time::Duration};

use abs_cancel::{TrCancellationToken, TrMayCancel};
use abs_denco::{
    de::{self, TrDecodable, TrDecoder, Visitor},
    forward_to_decode_any,
    ser::{self, TrEncodable, TrEncoder},
};
use cantok_rs::CancellationSource;
use gen_mcf2::gen_may_cancel_future;
use mp_denco::{MpEncoder, MpError};

/// 测试用错误类型。
#[derive(Debug)]
struct TestError;

impl fmt::Display for TestError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("test error")
    }
}

impl core::error::Error for TestError {}

impl de::Error for TestError {
    fn custom<T>(_msg: T) -> Self
    where
        T: fmt::Display,
    {
        TestError
    }
}

/// 立即完成、但会轮询等待取消信号的执行器。
fn block_on<F: Future>(future: F) -> F::Output {
    let mut cx = Context::from_waker(Waker::noop());
    let mut future = pin!(future);
    loop {
        match future.as_mut().poll(&mut cx) {
            Poll::Ready(output) => return output,
            Poll::Pending => std::thread::yield_now(),
        }
    }
}

//-- ---- 解码方向 ---- -------------------------------------------------------

/// 一个只用于验证取消传播的解码目标。
struct SlowValue;

/// 慢速反序列化器：`decode_any` 会先等待取消，再返回错误。
struct SlowDecoder;

/// `SlowValue` 的访问者；慢速反序列化器不会真正调用它的 visit 方法。
struct SlowVisitor;

impl<'de> Visitor<'de> for SlowVisitor {
    type Value = SlowValue;

    fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("a slow value")
    }
}

#[gen_may_cancel_future(DecodeAnySlowOp)]
async fn decode_any_slow_async<'de, 'a, V, C>(visitor: V, cancel: C) -> Result<V::Value, TestError>
where
    'de: 'a,
    V: Visitor<'de> + 'a,
    V::Value: 'a,
    C: TrCancellationToken,
{
    let _ = visitor;
    cancel.cancellation().await;
    Err(TestError)
}

impl<'de> TrDecoder<'de> for SlowDecoder {
    type Error = TestError;

    type DecodeAnyAsync<'a, V>
        = DecodeAnySlowOpAsync<'de, 'a, 'a, V>
    where
        'de: 'a,
        V: Visitor<'de> + 'a,
        V::Value: 'a,
        Self: 'a,
        Self::Error: 'a;

    fn decode_any<'a, V>(self, visitor: V) -> Self::DecodeAnyAsync<'a, V>
    where
        'de: 'a,
        V: Visitor<'de> + 'a,
        V::Value: 'a,
        Self: 'a,
        Self::Error: 'a,
    {
        DecodeAnySlowOpAsync::new(visitor)
    }

    forward_to_decode_any! {
        decode_bool => DecodeBoolAsync,
        decode_i8 => DecodeI8Async,
        decode_i16 => DecodeI16Async,
        decode_i32 => DecodeI32Async,
        decode_i64 => DecodeI64Async,
        decode_i128 => DecodeI128Async,
        decode_u8 => DecodeU8Async,
        decode_u16 => DecodeU16Async,
        decode_u32 => DecodeU32Async,
        decode_u64 => DecodeU64Async,
        decode_u128 => DecodeU128Async,
        decode_f32 => DecodeF32Async,
        decode_f64 => DecodeF64Async,
        decode_char => DecodeCharAsync,
        decode_str => DecodeStrAsync,
        decode_string => DecodeStringAsync,
        decode_bytes => DecodeBytesAsync,
        decode_byte_buf => DecodeByteBufAsync,
        decode_option => DecodeOptionAsync,
        decode_unit => DecodeUnitAsync,
        decode_unit_struct => DecodeUnitStructAsync(name: &'static str),
        decode_newtype_struct => DecodeNewtypeStructAsync(name: &'static str),
        decode_seq => DecodeSeqAsync,
        decode_tuple => DecodeTupleAsync(len: usize),
        decode_tuple_struct => DecodeTupleStructAsync(name: &'static str, len: usize),
        decode_map => DecodeMapAsync,
        decode_struct => DecodeStructAsync(name: &'static str, fields: &'static [&'static str]),
        decode_enum => DecodeEnumAsync(name: &'static str, variants: &'static [&'static str]),
        decode_identifier => DecodeIdentifierAsync,
        decode_ignored_any => DecodeIgnoredAnyAsync,
    }
}

#[gen_may_cancel_future(DecodeSlowValueOp)]
async fn decode_slow_value_async<'de, 'a, D, C>(de: D, cancel: C) -> Result<SlowValue, D::Error>
where
    'de: 'a,
    D: TrDecoder<'de> + 'a,
    D::Error: 'a,
    C: TrCancellationToken,
{
    de.decode_u64(SlowVisitor).may_cancel_with(cancel).await
}

impl<'de> TrDecodable<'de> for SlowValue {
    type DecodeAsync<'a, D>
        = DecodeSlowValueOpAsync<'de, 'a, 'a, D>
    where
        'de: 'a,
        D: TrDecoder<'de> + 'a,
        D::Error: 'a,
        Self: 'a;

    fn decode_async<'a, D>(deserializer: D) -> Self::DecodeAsync<'a, D>
    where
        'de: 'a,
        D: TrDecoder<'de> + 'a,
        D::Error: 'a,
        Self: 'a,
    {
        DecodeSlowValueOpAsync::new(deserializer)
    }
}

//-- ---- 编码方向 ---- -------------------------------------------------------

/// 一个只用于验证编码取消的待编码值。
struct SlowEncodable;

#[gen_may_cancel_future(EncodeSlowOp)]
async fn encode_slow_async<'a, S, C>(
    value: &'a SlowEncodable,
    serializer: S,
    cancel: C,
) -> Result<S::Ok, S::Error>
where
    S: TrEncoder + 'a,
    S::Ok: 'a,
    S::Error: 'a,
    C: TrCancellationToken,
{
    let _ = (value, serializer);
    cancel.cancellation().await;
    Err(<S::Error as ser::Error>::custom("cancelled"))
}

impl TrEncodable for SlowEncodable {
    type EncodeAsync<'a, S>
        = EncodeSlowOpAsync<'a, 'a, S>
    where
        S: TrEncoder + 'a,
        S::Ok: 'a,
        S::Error: 'a,
        Self: 'a;

    fn encode_async<'a, S>(&'a self, serializer: S) -> Self::EncodeAsync<'a, S>
    where
        S: TrEncoder + 'a,
        S::Ok: 'a,
        S::Error: 'a,
        Self: 'a,
    {
        EncodeSlowOpAsync::<'a, 'a, S>::new(self, serializer)
    }
}

//-- ---- 测试 ---- -----------------------------------------------------------

/// 验证取消信号能穿过转发宏传播到最内层反序列化器。
/// Verify that a cancellation signal propagates through the forwarding macro
/// to the innermost deserializer.
///
/// - 手段 / Method: `SlowValue::decode_async` 调用 `decode_u64`，后者由
///   `forward_to_decode_any!` 转发到会等待取消的 `decode_any`；
///   另一个线程在 20ms 后触发取消。
/// - 判断 / Assert: 解码 future 以 `Err` 结束，说明令牌到达了最内层。
#[test]
fn cancellation_propagates_through_decoder() {
    let source = CancellationSource::new();
    let token = source.child_token();
    std::thread::scope(|scope| {
        scope.spawn(|| {
            std::thread::sleep(Duration::from_millis(20));
            source.cancel();
        });
        let out: Result<SlowValue, TestError> = block_on(async {
            SlowValue::decode_async(SlowDecoder)
                .may_cancel_with(token)
                .await
        });
        assert!(out.is_err(), "decoder should observe cancellation");
    });
}

/// 验证取消信号能到达顶层编码操作。
/// Verify that a cancellation signal reaches the top-level encode operation.
///
/// - 手段 / Method: `SlowEncodable::encode_async` 的 helper 等待取消；
///   另一个线程在 20ms 后触发取消。
/// - 判断 / Assert: 编码 future 以 `Err` 结束。
#[test]
fn cancellation_propagates_to_encoder() {
    let source = CancellationSource::new();
    let token = source.child_token();
    let value = SlowEncodable;
    let mut storage = [0u8; 1];
    let mut buf: &mut [u8] = &mut storage;
    std::thread::scope(|scope| {
        scope.spawn(|| {
            std::thread::sleep(Duration::from_millis(20));
            source.cancel();
        });
        let out: Result<(), MpError> = block_on(async {
            value
                .encode_async(MpEncoder::new(&mut buf))
                .may_cancel_with(token)
                .await
        });
        assert!(out.is_err(), "encoder should observe cancellation");
    });
}
