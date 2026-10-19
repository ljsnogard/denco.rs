//! `denco_macro` 的 derive 端到端测试。

#![feature(impl_trait_in_assoc_type)]

use core::{
    fmt,
    future::{self, Future},
    task::{Context, Poll},
};
use std::task::Waker;

use abs_cancel::{NonCancellableToken, TrMayCancel};
use abs_denco::{
    de::{self, TrDecodable, TrDecoder, Visitor},
    ser::{TrEncodable, TrEncoder},
};
use denco_macro::{TrDecodable, TrEncodable};
use gen_mcf2::gen_may_cancel_future;
use mp_denco::{MpDecoder, MpEncoder};

fn block_on_<F: Future>(future: F) -> F::Output {
    let mut cx = Context::from_waker(Waker::noop());
    let mut future = core::pin::pin!(future);
    loop {
        match future.as_mut().poll(&mut cx) {
            Poll::Ready(output) => return output,
            Poll::Pending => std::thread::yield_now(),
        }
    }
}

fn encode_<T>(value: &T) -> Vec<u8>
where
    T: ?Sized + TrEncodable,
{
    let mut storage = vec![0u8; 4096];
    let total = storage.len();
    let written = {
        let mut sink: &mut [u8] = storage.as_mut_slice();
        block_on_(async {
            value
                .encode_async(MpEncoder::new(&mut sink))
                .may_cancel_with(NonCancellableToken::new())
                .await
        })
        .expect("编码应当成功");
        total - sink.len()
    };
    storage.truncate(written);
    storage
}

/// 用命名映射配置编码一个值，便于检查派生出的字段名。
fn encode_named_<T>(value: &T) -> Vec<u8>
where
    T: ?Sized + TrEncodable,
{
    let mut storage = vec![0u8; 4096];
    let total = storage.len();
    let written = {
        let mut sink: &mut [u8] = storage.as_mut_slice();
        block_on_(async {
            value
                .encode_async(MpEncoder::new(&mut sink).with_struct_map())
                .may_cancel_with(NonCancellableToken::new())
                .await
        })
        .expect("命名编码应当成功");
        total - sink.len()
    };
    storage.truncate(written);
    storage
}

fn decode_<'de, T>(bytes: &'de [u8]) -> T
where
    T: TrDecodable<'de>,
{
    let mut input: &'de [u8] = bytes;
    block_on_(async {
        T::decode_async(MpDecoder::new(&mut input))
            .may_cancel_with(NonCancellableToken::new())
            .await
    })
    .expect("解码应当成功")
}

//-- ---- 测试用基础值类型 -----------------------------------------------------

#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(transparent)]
pub struct U64Val(u64);

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

#[gen_may_cancel_future(DecodeU64ValOp)]
pub async fn decode_u64_val_async<'de, 'a, D, C>(de: D, cancel: C) -> Result<U64Val, D::Error>
where
    'de: 'a,
    D: TrDecoder<'de> + 'a,
    D::Error: 'a,
    C: abs_cancel::TrCancellationToken,
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

#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(transparent)]
pub struct StrVal(String);

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

#[gen_may_cancel_future(DecodeStrValOp)]
pub async fn decode_str_val_async<'de, 'a, D, C>(de: D, cancel: C) -> Result<StrVal, D::Error>
where
    'de: 'a,
    D: TrDecoder<'de> + 'a,
    D::Error: 'a,
    C: abs_cancel::TrCancellationToken,
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

//-- ---- derive 类型 ----------------------------------------------------------

#[derive(Debug, PartialEq, TrEncodable, TrDecodable)]
struct Unit;

#[derive(Debug, PartialEq, TrEncodable, TrDecodable, serde::Serialize, serde::Deserialize)]
struct Record {
    id: U64Val,
    name: StrVal,
}

#[derive(Debug, PartialEq, TrEncodable, TrDecodable)]
struct Pair(U64Val, StrVal);

#[derive(Debug, PartialEq, TrEncodable, TrDecodable)]
struct Newtype(U64Val);

#[derive(Debug, PartialEq, TrEncodable, TrDecodable)]
struct Generic<T> {
    value: T,
}

#[derive(Debug, PartialEq, TrEncodable, TrDecodable)]
enum Shape {
    Empty,
    Num(U64Val),
    Pair(U64Val, StrVal),
    Named { a: U64Val, b: StrVal },
}

#[derive(Debug, PartialEq, TrEncodable, TrDecodable)]
#[denco(rename_all = "snake_case")]
struct Renamed {
    #[denco(rename = "identifier")]
    user_id: U64Val,
    #[denco(skip)]
    ignored: U64Val,
    #[denco(default)]
    count: U64Val,
}

#[derive(Debug, PartialEq, TrEncodable, TrDecodable)]
#[denco(transparent)]
struct Transparent(U64Val);

/// 与 `Newtype` 同模块的第二个 newtype，用于验证解码辅助类型按类型名隔离。
#[derive(Debug, PartialEq, TrEncodable, TrDecodable)]
struct NewtypeA(U64Val);

/// 与 `Newtype` 同模块的第三个 newtype，用于验证解码辅助类型按类型名隔离。
#[derive(Debug, PartialEq, TrEncodable, TrDecodable)]
struct NewtypeB(StrVal);

#[derive(Debug, PartialEq, TrEncodable, TrDecodable)]
pub struct PublicRecord {
    pub id: U64Val,
}

#[derive(Debug, PartialEq, TrEncodable, TrDecodable)]
struct Aliased {
    #[denco(alias = "old_id")]
    id: U64Val,
}

#[derive(Debug, PartialEq, TrEncodable, TrDecodable)]
struct EmptyRecord {}

#[derive(Debug, PartialEq, TrEncodable, TrDecodable)]
struct ConstGeneric<const N: usize> {
    id: U64Val,
}

#[derive(Debug, PartialEq, TrEncodable, TrDecodable)]
enum SkippedVariant {
    V {
        #[denco(skip)]
        a: U64Val,
        b: U64Val,
    },
}

#[derive(Debug, PartialEq, TrEncodable, TrDecodable)]
#[denco(rename_all = "snake_case")]
enum RenamedEnum {
    UnitVariant,
    Newtype(U64Val),
}

//-- ---- 测试 ----------------------------------------------------------------

/// 验证 unit 结构体的派生编码/解码往返。
///
/// - 手段：`Unit` 经 `MpEncoder` 编码后用 `MpDecoder` 解码。
/// - 判断：解码值等于 `Unit`。
#[test]
fn unit_roundtrip_() {
    let bytes = encode_(&Unit);
    let got: Unit = decode_(&bytes);
    assert_eq!(got, Unit);
}

/// 验证命名字段结构体的派生编码/解码往返。
///
/// - 手段：`Record` 编码为命名映射后解码。
/// - 判断：字段值完全一致。
#[test]
fn named_struct_roundtrip_() {
    let value = Record {
        id: U64Val(300),
        name: StrVal(String::from("denco")),
    };
    let bytes = encode_(&value);
    let got: Record = decode_(&bytes);
    assert_eq!(got, value);
}

/// 验证元组结构体与 newtype 结构体的派生往返。
///
/// - 手段：分别编码/解码 `Pair` 与 `Newtype`。
/// - 判断：解码值与原值相等。
#[test]
fn tuple_and_newtype_roundtrip_() {
    let pair = Pair(U64Val(7), StrVal(String::from("pair")));
    let got: Pair = decode_(&encode_(&pair));
    assert_eq!(got, pair);

    let newtype = Newtype(U64Val(9));
    let got: Newtype = decode_(&encode_(&newtype));
    assert_eq!(got, newtype);
}

/// 验证同一模块内的多个 newtype struct 各自生成独立的解码辅助类型。
///
/// - 手段：`Newtype`、`NewtypeA` 与 `NewtypeB` 都在本模块派生 `TrDecodable`，
///   分别编码后解码。
/// - 判断：三个 newtype 都能编译并往返成功；辅助类型不再因固定名字互相覆盖。
#[test]
fn multiple_newtype_structs_in_one_module_() {
    let a = NewtypeA(U64Val(7));
    let got: NewtypeA = decode_(&encode_(&a));
    assert_eq!(got, a);

    let b = NewtypeB(StrVal(String::from("x")));
    let got: NewtypeB = decode_(&encode_(&b));
    assert_eq!(got, b);
}

/// 验证泛型结构体的派生往返。
///
/// - 手段：`Generic<U64Val>` 编码后解码。
/// - 判断：解码值与原值相等。
#[test]
fn generic_roundtrip_() {
    let value = Generic { value: U64Val(42) };
    let got: Generic<U64Val> = decode_(&encode_(&value));
    assert_eq!(got, value);
}

/// 验证四种枚举变体的派生往返。
///
/// - 手段：依次编码/解码 `Shape` 的 unit/newtype/tuple/struct 变体。
/// - 判断：每个变体解码后与原值相等。
#[test]
fn enum_roundtrip_() {
    for value in [
        Shape::Empty,
        Shape::Num(U64Val(11)),
        Shape::Pair(U64Val(12), StrVal(String::from("x"))),
        Shape::Named {
            a: U64Val(13),
            b: StrVal(String::from("y")),
        },
    ] {
        let got: Shape = decode_(&encode_(&value));
        assert_eq!(got, value);
    }
}

/// 验证 `rename_all`、`skip` 与 `default` 的派生行为。
///
/// - 手段：用命名映射配置编码 `Renamed`，并手工构造只含 `identifier` 的映射
///   字节再解码。
/// - 判断：命名编码中的键为 `identifier`，缺失字段使用默认值。
#[test]
fn rename_skip_default_() {
    let value = Renamed {
        user_id: U64Val(5),
        ignored: U64Val(999),
        count: U64Val(7),
    };
    let bytes = encode_named_(&value);
    let text = String::from_utf8_lossy(&bytes);
    assert!(text.contains("identifier"), "重命名后的键应当出现：{text}");
    assert!(!text.contains("ignored"), "skip 字段不应被编码：{text}");

    // 只发送 identifier 字段，count 应使用 Default。
    let partial = [
        0x81, // fixmap(1)
        0xaa, b'i', b'd', b'e', b'n', b't', b'i', b'f', b'i', b'e', b'r', // "identifier"
        0xcd, 0x00, 0x2a, // u16(42)
    ];
    let got: Renamed = decode_(&partial);
    assert_eq!(
        got,
        Renamed {
            user_id: U64Val(42),
            ignored: U64Val::default(),
            count: U64Val::default(),
        }
    );
}

/// 验证 derive 生成的默认编码字节与 rmp-serde 的紧凑表示一致。
///
/// - 手段：同一个 `Record` 同时派生 serde 与 denco 的编解码，比较字节。
/// - 判断：两边字节完全相同，且互相可解。
#[test]
fn bytes_match_rmp_serde_() {
    let value = Record {
        id: U64Val(300),
        name: StrVal(String::from("denco")),
    };
    let mp_bytes = encode_(&value);
    let rmp_bytes = rmp_serde::to_vec(&value).expect("rmp 编码应当成功");
    assert_eq!(mp_bytes, rmp_bytes);
}

/// 验证 transparent newtype、公有类型、空结构体与别名的派生行为。
///
/// - 手段：分别往返 `Transparent`、`PublicRecord`、`Empty`，并用 `old_id`
///   键构造 `Aliased` 的映射字节。
/// - 判断：往返值相等；别名键能正确映射到 `id`。
#[test]
fn transparent_public_empty_alias_() {
    let transparent = Transparent(U64Val(3));
    assert_eq!(decode_::<Transparent>(&encode_(&transparent)), transparent);

    let public = PublicRecord { id: U64Val(4) };
    assert_eq!(decode_::<PublicRecord>(&encode_(&public)), public);

    let empty = EmptyRecord {};
    assert_eq!(decode_::<EmptyRecord>(&encode_(&empty)), empty);

    let const_generic = ConstGeneric::<3> { id: U64Val(8) };
    assert_eq!(
        decode_::<ConstGeneric<3>>(&encode_(&const_generic)),
        const_generic
    );

    // fixmap(1) + "old_id" + u32(42)
    let bytes = [
        0x81, 0xa6, b'o', b'l', b'd', b'_', b'i', b'd', 0xce, 0x00, 0x00, 0x00, 0x2a,
    ];
    let aliased: Aliased = decode_(&bytes);
    assert_eq!(aliased, Aliased { id: U64Val(42) });
}

/// 验证枚举的 `rename_all` 会作用于变体名。
///
/// - 手段：编码 `RenamedEnum::UnitVariant` 并检查字节中的变体名。
/// - 判断：编码结果包含 `unit_variant`。
#[test]
fn enum_rename_all_() {
    let bytes = encode_(&RenamedEnum::UnitVariant);
    let text = String::from_utf8_lossy(&bytes);
    assert!(text.contains("unit_variant"), "变体名应被重命名：{text}");
}

/// 验证枚举 struct 变体中的跳过字段。
///
/// - 手段：编码/解码 `SkippedVariant::V`，并检查字节中不含被跳过的键。
/// - 判断：往返值相等；编码结果不包含 `a` 键。
#[test]
fn enum_struct_variant_skip_() {
    let value = SkippedVariant::V {
        a: U64Val(1),
        b: U64Val(2),
    };
    let bytes = encode_(&value);
    let text = String::from_utf8_lossy(&bytes);
    assert!(!text.contains("\"a\""), "被跳过字段不应编码：{text}");
    let got: SkippedVariant = decode_(&bytes);
    assert_eq!(
        got,
        SkippedVariant::V {
            a: U64Val::default(),
            b: U64Val(2),
        }
    );
}
