//! `json_denco` 集成测试共享的类型与辅助函数。
//!
//! 这里放三种运行时后端都会用到的内容：同时派生 serde 与 denco 编解码的测试
//! 值类型、切片上的编码/解码辅助，以及面向任意 `TrBuffWrite` / `TrBuffRead`
//! 的编解码辅助。
//!
//! Shared test types and helpers for the `json_denco` integration tests.

#![allow(dead_code)]

use core::{
    fmt,
    future::Future,
    task::{Context, Poll},
};
use std::task::Waker;

use abs_buff::{Demand, TrBuffRead, TrBuffWrite, buffer::TrBuffSegmMut};
use abs_cancel::{NonCancellableToken, TrCancellationToken, TrMayCancel};
use abs_denco::{
    de::{self, TrDecodable},
    ser::TrEncodable,
};
use denco_macro::{TrDecodable, TrEncodable};
use json_denco::{JsonDecoder, JsonEncoder, JsonError};
use serde::{Deserialize, Serialize};

/// park/取消类测试的统一超时上限。
pub const TIMEOUT_: std::time::Duration = std::time::Duration::from_secs(5);

/// 立即完成、但对 `Pending` 让出 CPU 的测试执行器。
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

//-- ---- 测试值类型 -----------------------------------------------------------

/// 内层结构体。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TrEncodable, TrDecodable)]
pub struct Inner {
    pub id: u64,
    pub label: String,
}

/// 覆盖字符串、数组、Vec、Option、char、浮点与嵌套结构体的复合值。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TrEncodable, TrDecodable)]
pub struct Payload {
    pub name: String,
    pub tags: Vec<String>,
    pub numbers: [u32; 3],
    pub bytes: Vec<u8>,
    pub maybe: Option<u32>,
    pub flag: bool,
    pub letter: char,
    pub ratio: f32,
    pub score: f64,
    pub nested: Inner,
    pub shape: Shape,
}

/// 只包含常见标量与集合的线上结构体，用于 socket 测试以控制类型深度。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TrEncodable, TrDecodable)]
pub struct Wire {
    pub name: String,
    pub count: u64,
    pub tags: Vec<String>,
    pub maybe: Option<u32>,
    pub flag: bool,
}

/// 构造一个 socket 测试用的 `Wire` 值。
pub fn sample_wire_() -> Wire {
    Wire {
        name: String::from("wire \"json\"\n\t\\ test"),
        count: 0x0102_0304_0506_0708,
        tags: vec![String::from("a"), String::from("中文"), String::from("😀")],
        maybe: Some(7),
        flag: true,
    }
}

/// 覆盖四种枚举变体。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TrEncodable, TrDecodable)]
pub enum Shape {
    Empty,
    Text(String),
    Pair(u32, u32),
    Named { left: u32, right: String },
}

/// 构造一个覆盖各种字段的样本值。
pub fn sample_payload_() -> Payload {
    Payload {
        name: String::from("denco \"json\"\n\t\\ test"),
        tags: vec![String::from("a"), String::from("中文"), String::from("😀")],
        numbers: [1, 2, u32::MAX],
        bytes: vec![0x00, 0x7f, 0xde, 0xad, 0xbe, 0xef],
        maybe: Some(7),
        flag: true,
        letter: '中',
        ratio: 1.5,
        score: -2.25,
        nested: Inner {
            id: 300,
            label: String::from("inner"),
        },
        shape: Shape::Named {
            left: 10,
            right: String::from("right"),
        },
    }
}

/// 透明包装 `u64` 的测试类型。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TrEncodable, TrDecodable)]
#[serde(transparent)]
#[denco(transparent)]
pub struct U64Val(pub u64);

/// 透明包装字符串的测试类型。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TrEncodable, TrDecodable)]
#[serde(transparent)]
#[denco(transparent)]
pub struct StrVal(pub String);

/// 用 `encode_bytes` / `decode_bytes` 表示二进制数据的测试类型。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BytesVal(pub Vec<u8>);

impl TrEncodable for BytesVal {
    type EncodeAsync<'a, S>
        = S::EncodeBytesAsync<'a, 'a>
    where
        S: abs_denco::ser::TrEncoder + 'a,
        S::Ok: 'a,
        S::Error: 'a,
        Self: 'a;

    fn encode_async<'a, S>(&'a self, encoder: S) -> Self::EncodeAsync<'a, S>
    where
        S: abs_denco::ser::TrEncoder + 'a,
        S::Ok: 'a,
        S::Error: 'a,
        Self: 'a,
    {
        encoder.encode_bytes(&self.0)
    }
}

pub struct BytesValVisitor;

impl<'de> abs_denco::de::Visitor<'de> for BytesValVisitor {
    type Value = BytesVal;

    fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("a byte array")
    }

    fn visit_byte_buf<'a, E>(
        self,
        value: Vec<u8>,
    ) -> impl TrMayCancel<'a, MayCancelOutput = Result<Self::Value, E>>
    where
        Self: 'a,
        Self::Value: 'a,
        E: de::Error + 'a,
    {
        core::future::ready(Ok(BytesVal(value)))
    }

    fn visit_bytes<'a, E>(
        self,
        value: &'a [u8],
    ) -> impl TrMayCancel<'a, MayCancelOutput = Result<Self::Value, E>>
    where
        Self: 'a,
        Self::Value: 'a,
        E: de::Error + 'a,
    {
        core::future::ready(Ok(BytesVal(value.to_vec())))
    }
}

impl<'de> TrDecodable<'de> for BytesVal {
    type DecodeAsync<'a, D>
        = D::DecodeByteBufAsync<'a, BytesValVisitor>
    where
        'de: 'a,
        D: abs_denco::de::TrDecoder<'de> + 'a,
        D::Error: 'a,
        Self: 'a;

    fn decode_async<'a, D>(deserializer: D) -> Self::DecodeAsync<'a, D>
    where
        'de: 'a,
        D: abs_denco::de::TrDecoder<'de> + 'a,
        D::Error: 'a,
        Self: 'a,
    {
        deserializer.decode_byte_buf(BytesValVisitor)
    }
}

//-- ---- 切片辅助 -------------------------------------------------------------

/// 用 `json_denco` 把一个值编码到新分配的 `Vec<u8>`。
pub fn encode_to_vec_<T>(value: &T) -> Vec<u8>
where
    T: ?Sized + TrEncodable,
{
    let mut storage = vec![0u8; 64 * 1024];
    let total = storage.len();
    let written = {
        let mut sink: &mut [u8] = storage.as_mut_slice();
        block_on_(async {
            value
                .encode_async(JsonEncoder::new(&mut sink))
                .may_cancel_with(NonCancellableToken::new())
                .await
        })
        .expect("切片编码应当成功");
        total - sink.len()
    };
    storage.truncate(written);
    storage
}

/// 从切片解码一个值。
pub fn decode_from_slice_<'de, T>(bytes: &'de [u8]) -> T
where
    T: TrDecodable<'de>,
{
    let mut input: &'de [u8] = bytes;
    let mut decoder = JsonDecoder::new(&mut input);
    block_on_(async {
        T::decode_async(&mut decoder)
            .may_cancel_with(NonCancellableToken::new())
            .await
    })
    .expect("切片解码应当成功")
}

//-- ---- 流式辅助 -------------------------------------------------------------

/// 向任意写端编码一个值（不负责收尾）。
pub async fn encode_into_<T, W>(value: &T, writer: &mut W) -> Result<(), JsonError>
where
    T: ?Sized + TrEncodable,
    W: TrBuffWrite<u8>,
{
    value
        .encode_async(JsonEncoder::new(writer))
        .may_cancel_with(NonCancellableToken::new())
        .await
}

/// 向任意写端写入原始字节（用于在多消息流之间插入空白分隔符）。
pub async fn write_raw_<W>(writer: &mut W, mut bytes: &[u8]) -> Result<(), JsonError>
where
    W: TrBuffWrite<u8>,
{
    while !bytes.is_empty() {
        let demand = Demand::no_more_than(bytes.len());
        let mut some = writer
            .write_async(&demand)
            .may_cancel_with(NonCancellableToken::new())
            .await;
        let moved = match some.as_mut().pick_left() {
            Some(segm) => segm.move_items_from_as_buff(bytes),
            None => 0,
        };
        if moved == 0 {
            return Err(JsonError::new("buff write returned no segment"));
        }
        bytes = &bytes[moved..];
    }
    Ok(())
}

/// 从任意读端解码一个值。
///
/// 单值场景可以直接新建解码器；同一连接上的多消息流应复用同一个
/// [`JsonDecoder`]，否则解码器预读进缓冲的后续消息会随解码器一起丢弃。
pub async fn decode_from_<'de, T, R>(reader: &mut R) -> Result<T, JsonError>
where
    T: TrDecodable<'de>,
    R: TrBuffRead<u8>,
{
    let mut decoder = JsonDecoder::new(reader);
    T::decode_async(&mut decoder)
        .may_cancel_with(NonCancellableToken::new())
        .await
}

/// 从任意读端解码一个值，并允许外部取消。
pub async fn decode_from_cancellable_<'de, T, R, C>(
    reader: &mut R,
    cancel: C,
) -> Result<T, JsonError>
where
    T: TrDecodable<'de>,
    R: TrBuffRead<u8>,
    C: TrCancellationToken,
{
    let mut decoder = JsonDecoder::new(reader);
    T::decode_async(&mut decoder).may_cancel_with(cancel).await
}

/// 用已有的解码器继续解码一个值。
pub async fn decode_next_<'de, T, R>(decoder: &mut JsonDecoder<'_, R>) -> Result<T, JsonError>
where
    T: TrDecodable<'de>,
    R: TrBuffRead<u8>,
{
    T::decode_async(&mut *decoder)
        .may_cancel_with(NonCancellableToken::new())
        .await
}
