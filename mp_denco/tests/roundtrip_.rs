//! 最小 MessagePack 的端到端往返测试。
//!
//! 这些测试同时验证 GAT 版 `TrEncodable` / `TrDecodable` / `TrEncoder` /
//! `TrDecoder` 的组合方式，以及 `gen_mcf2` 生成的 helper future 与
//! `TrMayCancel` 的配合。

#![feature(impl_trait_in_assoc_type)]

use core::{
    fmt,
    future::{self, Future},
    task::{Context, Poll},
};

use std::{pin::pin, task::Waker};

use abs_cancel::{CancelledToken, NonCancellableToken, TrCancellationToken, TrMayCancel};
use abs_denco::{
    de::{
        self, IgnoredAny, MapAccess, MapAccessExt, SeqAccess, SeqAccessExt, TrDecodable, TrDecoder,
        Visitor,
    },
    ser::{
        TrEncodable, TrEncodeMap, TrEncodeMapExt, TrEncodeSeq, TrEncodeStruct, TrEncoder,
        TrEncoderExt,
    },
};
use gen_mcf2::gen_may_cancel_future;
use mp_denco::{MpDecoder, MpEncoder, MpError};

/// 立即完成的测试执行器。
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

//-- ---- 测试类型：u64 newtype ---- ------------------------------------------

/// 一个只包含 `u64` 的 newtype。
#[derive(Debug, PartialEq, Eq)]
struct Age(u64);

impl TrEncodable for Age {
    type EncodeAsync<'a, S>
        = S::EncodeU64Async<'a>
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
        serializer.encode_u64(self.0)
    }
}

struct AgeVisitor;

impl<'de> Visitor<'de> for AgeVisitor {
    type Value = Age;

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
        future::ready(Ok(Age(v)))
    }
}

#[gen_may_cancel_future(DecodeAgeOp)]
async fn decode_age_async<'de, 'a, D, C>(de: D, cancel: C) -> Result<Age, D::Error>
where
    'de: 'a,
    D: TrDecoder<'de> + 'a,
    D::Error: 'a,
    C: TrCancellationToken,
{
    de.decode_u64(AgeVisitor).may_cancel_with(cancel).await
}

impl<'de> TrDecodable<'de> for Age {
    type DecodeAsync<'a, D>
        = DecodeAgeOpAsync<'de, 'a, 'a, D>
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
        DecodeAgeOpAsync::new(deserializer)
    }
}

//-- ---- 测试类型：String newtype ---- ---------------------------------------

/// 一个只包含 `String` 的 newtype。
#[derive(Debug, PartialEq, Eq)]
struct Name(String);

impl TrEncodable for Name {
    type EncodeAsync<'a, S>
        = S::EncodeStrAsync<'a, 'a>
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
        serializer.encode_str(self.0.as_str())
    }
}

struct NameVisitor;

impl<'de> Visitor<'de> for NameVisitor {
    type Value = Name;

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
        future::ready(Ok(Name(String::from(v))))
    }
}

#[gen_may_cancel_future(DecodeNameOp)]
async fn decode_name_async<'de, 'a, D, C>(de: D, cancel: C) -> Result<Name, D::Error>
where
    'de: 'a,
    D: TrDecoder<'de> + 'a,
    D::Error: 'a,
    C: TrCancellationToken,
{
    de.decode_str(NameVisitor).may_cancel_with(cancel).await
}

impl<'de> TrDecodable<'de> for Name {
    type DecodeAsync<'a, D>
        = DecodeNameOpAsync<'de, 'a, 'a, D>
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
        DecodeNameOpAsync::new(deserializer)
    }
}

//-- ---- 测试类型：bool newtype ---- -----------------------------------------

/// 一个只包含 `bool` 的 newtype。
#[derive(Debug, PartialEq, Eq)]
struct Flag(bool);

impl TrEncodable for Flag {
    type EncodeAsync<'a, S>
        = S::EncodeBoolAsync<'a>
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
        serializer.encode_bool(self.0)
    }
}

struct FlagVisitor;

impl<'de> Visitor<'de> for FlagVisitor {
    type Value = Flag;

    fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("a boolean")
    }

    fn visit_bool<'a, E>(
        self,
        v: bool,
    ) -> impl TrMayCancel<'a, MayCancelOutput = Result<Self::Value, E>>
    where
        Self: 'a,
        Self::Value: 'a,
        E: de::Error + 'a,
    {
        future::ready(Ok(Flag(v)))
    }
}

#[gen_may_cancel_future(DecodeFlagOp)]
async fn decode_flag_async<'de, 'a, D, C>(de: D, cancel: C) -> Result<Flag, D::Error>
where
    'de: 'a,
    D: TrDecoder<'de> + 'a,
    D::Error: 'a,
    C: TrCancellationToken,
{
    de.decode_bool(FlagVisitor).may_cancel_with(cancel).await
}

impl<'de> TrDecodable<'de> for Flag {
    type DecodeAsync<'a, D>
        = DecodeFlagOpAsync<'de, 'a, 'a, D>
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
        DecodeFlagOpAsync::new(deserializer)
    }
}

//-- ---- 测试类型：Age 数组 ---- ---------------------------------------------

/// 一个 `Age` 数组。
#[derive(Debug, PartialEq, Eq)]
struct Ages(Vec<Age>);

#[gen_may_cancel_future(EncodeAgesOp)]
async fn encode_ages_async<'a, S, C>(
    ages: &'a Ages,
    serializer: S,
    cancel: C,
) -> Result<S::Ok, S::Error>
where
    S: TrEncoder + 'a,
    S::Ok: 'a,
    S::Error: 'a,
    C: TrCancellationToken,
{
    let mut seq = serializer
        .encode_seq(Some(ages.0.len()))
        .may_cancel_with(cancel.child_token())
        .await?;
    for age in &ages.0 {
        seq.encode_element(age)
            .may_cancel_with(cancel.child_token())
            .await?;
    }
    seq.end().may_cancel_with(cancel).await
}

impl TrEncodable for Ages {
    type EncodeAsync<'a, S>
        = EncodeAgesOpAsync<'a, 'a, S>
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
        EncodeAgesOpAsync::<'a, 'a, S>::new(self, serializer)
    }
}

#[gen_may_cancel_future(VisitAges)]
async fn visit_ages_async<'de, 'a, A, C>(mut seq: A, cancel: C) -> Result<Ages, A::Error>
where
    'de: 'a,
    A: SeqAccess<'de> + 'a,
    A::Error: 'a,
    C: TrCancellationToken,
{
    let mut ages = Vec::new();
    while let Some(age) = seq
        .next_element::<Age>()
        .may_cancel_with(cancel.child_token())
        .await?
    {
        ages.push(age);
    }
    Ok(Ages(ages))
}

struct AgesVisitor;

impl<'de> Visitor<'de> for AgesVisitor {
    type Value = Ages;

    fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("an array of ages")
    }

    fn visit_seq<'a, A>(
        self,
        seq: A,
    ) -> impl TrMayCancel<'a, MayCancelOutput = Result<Self::Value, A::Error>>
    where
        'de: 'a,
        Self: 'a,
        Self::Value: 'a,
        A: SeqAccess<'de> + 'a,
        A::Error: 'a,
    {
        VisitAgesAsync::<'de, 'a, 'a, A>::new(seq)
    }
}

#[gen_may_cancel_future(DecodeAgesOp)]
async fn decode_ages_async<'de, 'a, D, C>(de: D, cancel: C) -> Result<Ages, D::Error>
where
    'de: 'a,
    D: TrDecoder<'de> + 'a,
    D::Error: 'a,
    C: TrCancellationToken,
{
    de.decode_seq(AgesVisitor).may_cancel_with(cancel).await
}

impl<'de> TrDecodable<'de> for Ages {
    type DecodeAsync<'a, D>
        = DecodeAgesOpAsync<'de, 'a, 'a, D>
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
        DecodeAgesOpAsync::new(deserializer)
    }
}

//-- ---- 测试类型：结构体映射 ---- -------------------------------------------

/// 一个由 `Name` 和 `Age` 组成的记录。
#[derive(Debug, PartialEq, Eq)]
struct Record {
    name: Name,
    age: Age,
}

#[gen_may_cancel_future(EncodeRecordOp)]
async fn encode_record_async<'a, S, C>(
    record: &'a Record,
    serializer: S,
    cancel: C,
) -> Result<S::Ok, S::Error>
where
    S: TrEncoder + 'a,
    S::Ok: 'a,
    S::Error: 'a,
    C: TrCancellationToken,
{
    let mut st = serializer
        .encode_struct("Record", 2)
        .may_cancel_with(cancel.child_token())
        .await?;
    st.encode_field("name", &record.name)
        .may_cancel_with(cancel.child_token())
        .await?;
    st.encode_field("age", &record.age)
        .may_cancel_with(cancel.child_token())
        .await?;
    st.end().may_cancel_with(cancel).await
}

impl TrEncodable for Record {
    type EncodeAsync<'a, S>
        = EncodeRecordOpAsync<'a, 'a, S>
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
        EncodeRecordOpAsync::<'a, 'a, S>::new(self, serializer)
    }
}

#[gen_may_cancel_future(VisitRecord)]
async fn visit_record_async<'de, 'a, A, C>(mut map: A, cancel: C) -> Result<Record, A::Error>
where
    'de: 'a,
    A: MapAccess<'de> + 'a,
    A::Error: 'a,
    C: TrCancellationToken,
{
    let mut name: Option<Name> = None;
    let mut age: Option<Age> = None;
    while let Some(key) = map
        .next_key::<Name>()
        .may_cancel_with(cancel.child_token())
        .await?
    {
        match key.0.as_str() {
            "name" => {
                name = Some(
                    map.next_value::<Name>()
                        .may_cancel_with(cancel.child_token())
                        .await?,
                );
            }
            "age" => {
                age = Some(
                    map.next_value::<Age>()
                        .may_cancel_with(cancel.child_token())
                        .await?,
                );
            }
            _ => {
                let _: IgnoredAny = map
                    .next_value::<IgnoredAny>()
                    .may_cancel_with(cancel.child_token())
                    .await?;
            }
        }
    }
    let name = name.ok_or_else(|| <A::Error as de::Error>::missing_field("name"))?;
    let age = age.ok_or_else(|| <A::Error as de::Error>::missing_field("age"))?;
    Ok(Record { name, age })
}

struct RecordVisitor;

impl<'de> Visitor<'de> for RecordVisitor {
    type Value = Record;

    fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("a Record map")
    }

    fn visit_map<'a, A>(
        self,
        map: A,
    ) -> impl TrMayCancel<'a, MayCancelOutput = Result<Self::Value, A::Error>>
    where
        'de: 'a,
        Self: 'a,
        Self::Value: 'a,
        A: MapAccess<'de> + 'a,
        A::Error: 'a,
    {
        VisitRecordAsync::<'de, 'a, 'a, A>::new(map)
    }
}

#[gen_may_cancel_future(DecodeRecordOp)]
async fn decode_record_async<'de, 'a, D, C>(de: D, cancel: C) -> Result<Record, D::Error>
where
    'de: 'a,
    D: TrDecoder<'de> + 'a,
    D::Error: 'a,
    C: TrCancellationToken,
{
    de.decode_struct("Record", &["name", "age"], RecordVisitor)
        .may_cancel_with(cancel)
        .await
}

impl<'de> TrDecodable<'de> for Record {
    type DecodeAsync<'a, D>
        = DecodeRecordOpAsync<'de, 'a, 'a, D>
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
        DecodeRecordOpAsync::new(deserializer)
    }
}

//-- ---- 测试类型：Option<Age> ---- ------------------------------------------

/// 一个可选的 `Age`。
#[derive(Debug, PartialEq, Eq)]
struct MaybeAge(Option<Age>);

#[gen_may_cancel_future(EncodeMaybeAgeOp)]
async fn encode_maybe_age_async<'a, S, C>(
    value: &'a MaybeAge,
    serializer: S,
    cancel: C,
) -> Result<S::Ok, S::Error>
where
    S: TrEncoder + 'a,
    S::Ok: 'a,
    S::Error: 'a,
    C: TrCancellationToken,
{
    match &value.0 {
        Some(age) => serializer.encode_some(age).may_cancel_with(cancel).await,
        None => serializer.encode_none().may_cancel_with(cancel).await,
    }
}

impl TrEncodable for MaybeAge {
    type EncodeAsync<'a, S>
        = EncodeMaybeAgeOpAsync<'a, 'a, S>
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
        EncodeMaybeAgeOpAsync::<'a, 'a, S>::new(self, serializer)
    }
}

#[gen_may_cancel_future(VisitMaybeAgeSome)]
async fn visit_maybe_age_some_async<'de, 'a, D, C>(de: D, cancel: C) -> Result<MaybeAge, D::Error>
where
    'de: 'a,
    D: TrDecoder<'de> + 'a,
    D::Error: 'a,
    C: TrCancellationToken,
{
    let age = Age::decode_async(de).may_cancel_with(cancel).await?;
    Ok(MaybeAge(Some(age)))
}

struct MaybeAgeVisitor;

impl<'de> Visitor<'de> for MaybeAgeVisitor {
    type Value = MaybeAge;

    fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("an optional age")
    }

    fn visit_none<'a, E>(self) -> impl TrMayCancel<'a, MayCancelOutput = Result<Self::Value, E>>
    where
        Self: 'a,
        Self::Value: 'a,
        E: de::Error + 'a,
    {
        future::ready(Ok(MaybeAge(None)))
    }

    fn visit_some<'a, D>(
        self,
        de: D,
    ) -> impl TrMayCancel<'a, MayCancelOutput = Result<Self::Value, D::Error>>
    where
        'de: 'a,
        Self: 'a,
        Self::Value: 'a,
        D: TrDecoder<'de> + 'a,
        D::Error: 'a,
    {
        VisitMaybeAgeSomeAsync::<'de, 'a, 'a, D>::new(de)
    }
}

#[gen_may_cancel_future(DecodeMaybeAgeOp)]
async fn decode_maybe_age_async<'de, 'a, D, C>(de: D, cancel: C) -> Result<MaybeAge, D::Error>
where
    'de: 'a,
    D: TrDecoder<'de> + 'a,
    D::Error: 'a,
    C: TrCancellationToken,
{
    de.decode_option(MaybeAgeVisitor)
        .may_cancel_with(cancel)
        .await
}

impl<'de> TrDecodable<'de> for MaybeAge {
    type DecodeAsync<'a, D>
        = DecodeMaybeAgeOpAsync<'de, 'a, 'a, D>
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
        DecodeMaybeAgeOpAsync::new(deserializer)
    }
}

//-- ---- 测试 ---- -----------------------------------------------------------

/// 用 `&mut [u8]` 写端编码任意 `TrEncodable`，返回已写前缀。
///
/// 测试辅助函数在编码完成后才把前缀复制进 `Vec` 供断言使用；`mp_denco`
/// 的异步编码方法本身直接写这个切片，不做堆拷贝。
fn encode_vec_<T>(value: &T) -> Vec<u8>
where
    T: ?Sized + TrEncodable,
{
    let mut storage = vec![0u8; 4096];
    let total = storage.len();
    let written = {
        let mut sink: &mut [u8] = storage.as_mut_slice();
        block_on(async {
            value
                .encode_async(MpEncoder::new(&mut sink))
                .may_cancel_with(NonCancellableToken::new())
                .await
        })
        .expect("mp_denco encoding should succeed");
        total - sink.len()
    };
    storage.truncate(written);
    storage
}

/// 从 `&[u8]` 解码任意 `TrDecodable`。
fn decode_slice_<'de, T>(bytes: &'de [u8]) -> Result<T, MpError>
where
    T: TrDecodable<'de>,
{
    let mut input: &'de [u8] = bytes;
    block_on(async {
        T::decode_async(MpDecoder::new(&mut input))
            .may_cancel_with(NonCancellableToken::new())
            .await
    })
}

/// 验证 `u64` newtype 的序列化与反序列化往返。
/// Verify the round trip of a `u64` newtype.
///
/// - 手段 / Method: `Age(42)` 编码后应得到 fixint `0x2a`，再解码回 `Age(42)`。
/// - 判断 / Assert: 字节完全相等，解码结果为 `Age(42)`。
#[test]
fn roundtrip_age() {
    let buf = encode_vec_(&Age(42));
    assert_eq!(buf, vec![0x2a]);
    assert_eq!(decode_slice_::<Age>(&buf), Ok(Age(42)));
}

/// 验证较大的 `u64` 使用 MessagePack `uint16` 编码。
/// Verify that a larger `u64` uses MessagePack `uint16`.
///
/// - 手段 / Method: `Age(300)` 编码后应为 `0xcd 0x01 0x2c`。
/// - 判断 / Assert: 字节完全相等，解码结果仍为 `Age(300)`。
#[test]
fn roundtrip_age_uint16() {
    let buf = encode_vec_(&Age(300));
    assert_eq!(buf, vec![0xcd, 0x01, 0x2c]);
    assert_eq!(decode_slice_::<Age>(&buf), Ok(Age(300)));
}

/// 验证字符串 newtype 的往返。
/// Verify the round trip of a string newtype.
///
/// - 手段 / Method: `Name("denco")` 编码为 fixstr `0xa5` 加 UTF-8 内容。
/// - 判断 / Assert: 字节完全相等，解码结果为同名 `Name`。
#[test]
fn roundtrip_name() {
    let buf = encode_vec_(&Name(String::from("denco")));
    assert_eq!(buf, vec![0xa5, b'd', b'e', b'n', b'c', b'o']);
    assert_eq!(decode_slice_::<Name>(&buf), Ok(Name(String::from("denco"))));
}

/// 验证布尔 newtype 的往返。
/// Verify the round trip of a boolean newtype.
///
/// - 手段 / Method: `Flag(true)` 编码为 `0xc3`，再解码。
/// - 判断 / Assert: 字节完全相等，解码结果为 `Flag(true)`。
#[test]
fn roundtrip_flag() {
    let buf = encode_vec_(&Flag(true));
    assert_eq!(buf, vec![0xc3]);
    assert_eq!(decode_slice_::<Flag>(&buf), Ok(Flag(true)));
}

/// 验证类型不匹配时走 `Visitor` 的默认错误路径。
/// Verify that a type mismatch goes through the default `Visitor` error path.
///
/// - 手段 / Method: 把 MessagePack 字符串 `"foo"` 当成 `Age` 解码。
/// - 判断 / Assert: 返回 `Err`。
#[test]
fn age_rejects_string() {
    let buf = [0xa3, b'f', b'o', b'o'];
    assert!(decode_slice_::<Age>(&buf).is_err());
}

/// 验证 `TrEncoderExt::collect_seq` 能把迭代器写成 MessagePack 数组。
/// Verify that `TrEncoderExt::collect_seq` writes an iterator as a MessagePack array.
///
/// - 手段 / Method: 用 `collect_seq` 序列化 `Age(1)`、`Age(2)`。
/// - 判断 / Assert: 得到 fixarray 2 加两个 fixint。
#[test]
fn collect_seq_writes_array() {
    let mut storage = vec![0u8; 64];
    let total = storage.len();
    let written = {
        let mut sink: &mut [u8] = storage.as_mut_slice();
        block_on(async {
            MpEncoder::new(&mut sink)
                .collect_seq(vec![Age(1), Age(2)])
                .may_cancel_with(NonCancellableToken::new())
                .await
        })
        .expect("collect_seq should succeed");
        total - sink.len()
    };
    assert_eq!(&storage[..written], &[0x92, 0x01, 0x02]);
}

/// 验证 `TrEncoderExt::collect_map` 能把键值对写成 MessagePack 映射。
/// Verify that `TrEncoderExt::collect_map` writes key-value pairs as a MessagePack map.
///
/// - 手段 / Method: 用 `collect_map` 序列化 `Name("a") => Age(1)`。
/// - 判断 / Assert: 得到 fixmap 1、fixstr "a" 与 fixint 1。
#[test]
fn collect_map_writes_map() {
    let mut storage = vec![0u8; 64];
    let total = storage.len();
    let written = {
        let mut sink: &mut [u8] = storage.as_mut_slice();
        block_on(async {
            MpEncoder::new(&mut sink)
                .collect_map(vec![(Name(String::from("a")), Age(1))])
                .may_cancel_with(NonCancellableToken::new())
                .await
        })
        .expect("collect_map should succeed");
        total - sink.len()
    };
    assert_eq!(&storage[..written], &[0x81, 0xa1, b'a', 0x01]);
}

/// 验证 `TrEncoderExt::collect_str` 会先格式化再写成字符串。
/// Verify that `TrEncoderExt::collect_str` formats and writes a string.
///
/// - 手段 / Method: 用 `collect_str` 序列化 `&"hello"`。
/// - 判断 / Assert: 得到 fixstr "hello"。
#[test]
fn collect_str_writes_str() {
    let mut storage = vec![0u8; 64];
    let total = storage.len();
    let written = {
        let mut sink: &mut [u8] = storage.as_mut_slice();
        block_on(async {
            MpEncoder::new(&mut sink)
                .collect_str("hello")
                .may_cancel_with(NonCancellableToken::new())
                .await
        })
        .expect("collect_str should succeed");
        total - sink.len()
    };
    assert_eq!(&storage[..written], b"\xa5hello");
}

/// 验证 `TrEncodeMapExt::encode_entry` 的默认实现。
/// Verify the default implementation of `TrEncodeMapExt::encode_entry`.
///
/// - 手段 / Method: 打开一个 map 状态机，用 `encode_entry` 写入一对键值。
/// - 判断 / Assert: 得到 fixmap 1、键 `"k"` 与值 `Age(7)`。
#[test]
fn encode_entry_writes_pair() {
    let mut storage = vec![0u8; 64];
    let total = storage.len();
    let written = {
        let mut sink: &mut [u8] = storage.as_mut_slice();
        block_on(async {
            let mut map = MpEncoder::new(&mut sink)
                .encode_map(Some(1))
                .may_cancel_with(NonCancellableToken::new())
                .await
                .unwrap();
            map.encode_entry(&Name(String::from("k")), &Age(7))
                .may_cancel_with(NonCancellableToken::new())
                .await
                .unwrap();
            TrEncodeMap::end(map)
                .may_cancel_with(NonCancellableToken::new())
                .await
        })
        .expect("encode_entry should succeed");
        total - sink.len()
    };
    assert_eq!(&storage[..written], &[0x81, 0xa1, b'k', 0x07]);
}

/// 验证 `Vec<Age>` 风格的数组往返，并覆盖 `SeqAccessExt::next_element`。
/// Verify an array round trip in the `Vec<Age>` style, covering `SeqAccessExt::next_element`.
///
/// - 手段 / Method: `Ages(vec![Age(1), Age(2)])` 编码为 fixarray 2，再解码。
/// - 判断 / Assert: 字节为 `0x92 0x01 0x02`，解码结果与原始值相等。
#[test]
fn roundtrip_ages_array() {
    let ages = Ages(vec![Age(1), Age(2)]);
    let buf = encode_vec_(&ages);
    assert_eq!(buf, vec![0x92, 0x01, 0x02]);
    assert_eq!(decode_slice_::<Ages>(&buf), Ok(Ages(vec![Age(1), Age(2)])));
}

/// 验证 `IgnoredAny` 能丢弃任意嵌套值。
/// Verify that `IgnoredAny` can discard arbitrary nested values.
///
/// - 手段 / Method: 把 fixarray `[1, "a"]` 反序列化为 `IgnoredAny`。
/// - 判断 / Assert: 返回 `Ok(IgnoredAny)`，数组元素被完整消费。
#[test]
fn ignored_any_consumes_array() {
    let buf = [0x92, 0x01, 0xa1, b'a'];
    assert_eq!(decode_slice_::<IgnoredAny>(&buf), Ok(IgnoredAny));
}

/// 验证结构体映射的往返，并覆盖 `MapAccessExt::next_key` / `next_value`。
/// Verify a struct-map round trip, covering `MapAccessExt::next_key` / `next_value`.
///
/// - 手段 / Method: `Record { name: "denco", age: 42 }` 编码为 fixmap 2。
/// - 判断 / Assert: 字节符合 MessagePack 映射布局，解码结果与原值相等。
#[test]
fn roundtrip_record_map() {
    let record = Record {
        name: Name(String::from("denco")),
        age: Age(42),
    };
    let buf = encode_vec_(&record);
    assert_eq!(
        buf,
        vec![
            0x82, 0xa4, b'n', b'a', b'm', b'e', 0xa5, b'd', b'e', b'n', b'c', b'o', 0xa3, b'a',
            b'g', b'e', 0x2a,
        ]
    );
    assert_eq!(decode_slice_::<Record>(&buf), Ok(record));
}

/// 验证 `Option<Age>` 的 `Some` 与 `None` 往返。
/// Verify the round trip of `Option<Age>` for both `Some` and `None`.
///
/// - 手段 / Method: `Some(Age(7))` 编码为 `0x07`，`None` 编码为 `0xc0`。
/// - 判断 / Assert: 两条路径都能解码回原值。
#[test]
fn roundtrip_option_some_and_none() {
    let buf = encode_vec_(&MaybeAge(Some(Age(7))));
    assert_eq!(buf, vec![0x07]);
    assert_eq!(decode_slice_::<MaybeAge>(&buf), Ok(MaybeAge(Some(Age(7)))));

    let buf = [0xc0];
    assert_eq!(decode_slice_::<MaybeAge>(&buf), Ok(MaybeAge(None)));
}

/// 验证已取消的令牌会让 MessagePack 解码立即失败。
/// Verify that an already-cancelled token makes MessagePack decoding fail immediately.
///
/// - 手段 / Method: 用 `CancelledToken` 解码一个合法 fixint。
/// - 判断 / Assert: 返回 `Err`，说明预取消检查生效。
#[test]
fn decoder_rejects_cancelled_token() {
    let mut input: &[u8] = &[0x2a];
    let out = block_on(async {
        Age::decode_async(MpDecoder::new(&mut input))
            .may_cancel_with(CancelledToken::new())
            .await
    });
    assert!(out.is_err());
}

/// 验证已取消的令牌会让 MessagePack 编码立即失败。
/// Verify that an already-cancelled token makes MessagePack encoding fail immediately.
///
/// - 手段 / Method: 用 `CancelledToken` 编码 `Age(7)`。
/// - 判断 / Assert: 返回 `Err`，说明写入路径的预取消检查生效。
#[test]
fn encoder_rejects_cancelled_token() {
    let mut storage = [0u8; 16];
    let mut sink: &mut [u8] = &mut storage;
    let out = block_on(async {
        Age(7)
            .encode_async(MpEncoder::new(&mut sink))
            .may_cancel_with(CancelledToken::new())
            .await
    });
    assert!(out.is_err());
}
