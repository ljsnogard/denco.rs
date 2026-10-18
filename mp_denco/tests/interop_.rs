//! `mp_denco` 与 `rmp-serde` 的双向互操作验证。
//!
//! 本文件用同一批测试类型同时实现 serde 的 `Serialize` / `Deserialize`
//! 与 denco 的 `TrEncodable` / `TrDecodable`，验证三件事：
//!
//! 1. `mp_denco` 编码出的字节能被 `rmp_serde::from_slice` 解码；
//! 2. `rmp_serde::to_vec_named` 编码出的字节能被 `mp_denco` 解码；
//! 3. `rmp_serde::to_vec`（紧凑数组表示）编码出的字节也能被 `mp_denco`
//!    解码，覆盖结构体的「命名映射」与「位置数组」两种形态。
//!
//! 在命名表示下，两边生成的字节应当完全一致，因此测试同时做字节级断言。

#![feature(impl_trait_in_assoc_type)]

use core::{
    fmt,
    future::{self, Future},
    task::{Context, Poll},
};
use std::{pin::pin, task::Waker};

use abs_cancel::{NonCancellableToken, TrCancellationToken, TrMayCancel};
use abs_denco::{
    de::{
        self, EnumAccess, EnumAccessExt, IgnoredAny, MapAccess, MapAccessExt, SeqAccess,
        SeqAccessExt, TrDecodable, TrDecoder, VariantAccess, VariantAccessExt, Visitor,
    },
    ser::{
        TrEncodable, TrEncodeSeq, TrEncodeStruct, TrEncodeStructVariant, TrEncodeTupleStruct,
        TrEncodeTupleVariant, TrEncoder,
    },
};
use gen_mcf2::gen_may_cancel_future;
use mp_denco::{MpDecoder, MpEncoder};
use serde::{Deserialize, Serialize};

/// 立即完成的测试执行器。
fn block_on_<F: Future>(future: F) -> F::Output {
    let mut cx = Context::from_waker(Waker::noop());
    let mut future = pin!(future);
    loop {
        match future.as_mut().poll(&mut cx) {
            Poll::Ready(output) => return output,
            Poll::Pending => std::thread::yield_now(),
        }
    }
}

/// 用 `mp_denco` 编码任意 `TrEncodable`。
///
/// 写端是栈上固定容量的 `&mut [u8]`；测试辅助函数在编码完成后把已写前缀
/// 拷进 `Vec` 供断言使用，库内异步方法本身不做这种拷贝。
fn denco_encode_<T>(value: &T) -> Vec<u8>
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
        .expect("mp_denco 编码应当成功");
        total - sink.len()
    };
    storage.truncate(written);
    storage
}

/// 用 `mp_denco` 解码任意 `TrDecodable`。
fn denco_decode_<'de, T>(bytes: &'de [u8]) -> T
where
    T: TrDecodable<'de>,
{
    let mut input: &'de [u8] = bytes;
    block_on_(async {
        T::decode_async(MpDecoder::new(&mut input))
            .may_cancel_with(NonCancellableToken::new())
            .await
    })
    .expect("mp_denco 解码应当成功")
}

/// 对同一个值做四向验证：字节一致、mp→rmp、rmp named→mp、rmp compact→mp。
///
/// 测试目标 / Goal：
/// - 验证 `mp_denco` 与 `rmp-serde` 对同一数据模型的编码/解码结果一致。
///
/// 测试手段 / Method：
/// - 先用 `mp_denco` 编码，再分别用 `rmp_serde::from_slice`、
///   `rmp_serde::to_vec_named`、`rmp_serde::to_vec` 交叉处理。
///
/// 判定标准 / Assert：
/// - 命名编码字节完全相等；三条交叉路径解码出的值都等于原值。
fn check_cross_<T>(value: &T)
where
    T: fmt::Debug
        + PartialEq
        + Serialize
        + serde::de::DeserializeOwned
        + TrEncodable
        + for<'de> TrDecodable<'de>,
{
    let mp_bytes = denco_encode_(value);
    let named_bytes = rmp_serde::to_vec_named(value).expect("rmp_serde named 编码应当成功");
    assert_eq!(
        mp_bytes, named_bytes,
        "mp_denco 与 rmp_serde 的命名表示字节应当一致"
    );

    let from_mp: T =
        rmp_serde::from_slice(&mp_bytes).expect("rmp_serde 应当能解码 mp_denco 的字节");
    assert_eq!(&from_mp, value, "mp→rmp 解码结果应当等于原值");

    let from_named: T = denco_decode_(&named_bytes);
    assert_eq!(&from_named, value, "rmp named→mp 解码结果应当等于原值");

    let compact_bytes = rmp_serde::to_vec(value).expect("rmp_serde compact 编码应当成功");
    let from_compact: T = denco_decode_(&compact_bytes);
    assert_eq!(&from_compact, value, "rmp compact→mp 解码结果应当等于原值");
}

//-- ---- 标量包装类型 ---------------------------------------------------------

/// 定义一个透明的标量 newtype，并同时实现 serde 与 denco 的编解码。
macro_rules! value_newtype_ {
    (
        $name:ident,
        $inner:ty,
        $gat:ident,
        $encode:ident,
        $decode:ident,
        $visit:ident,
        $visitor:ident,
        $func:ident,
        $op:ident,
        $op_ty:ident
    ) => {
        /// 与 serde 的 transparent newtype 对应的测试类型。
        #[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
        #[serde(transparent)]
        struct $name(pub $inner);

        impl TrEncodable for $name {
            type EncodeAsync<'a, S>
                = S::$gat<'a>
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
                encoder.$encode(self.0)
            }
        }

        /// 对应的访问者。
        struct $visitor;

        impl<'de> Visitor<'de> for $visitor {
            type Value = $name;

            fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str(concat!("a ", stringify!($inner)))
            }

            fn $visit<'a, E>(
                self,
                v: $inner,
            ) -> impl TrMayCancel<'a, MayCancelOutput = Result<Self::Value, E>>
            where
                Self: 'a,
                Self::Value: 'a,
                E: de::Error + 'a,
            {
                future::ready(Ok($name(v)))
            }
        }

        #[gen_may_cancel_future($op)]
        async fn $func<'de, 'a, D, C>(de: D, cancel: C) -> Result<$name, D::Error>
        where
            'de: 'a,
            D: TrDecoder<'de> + 'a,
            D::Error: 'a,
            C: TrCancellationToken,
        {
            de.$decode($visitor).may_cancel_with(cancel).await
        }

        impl<'de> TrDecodable<'de> for $name {
            type DecodeAsync<'a, D>
                = $op_ty<'de, 'a, 'a, D>
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
                $op_ty::new(de)
            }
        }
    };
}

value_newtype_!(
    BoolVal,
    bool,
    EncodeBoolAsync,
    encode_bool,
    decode_bool,
    visit_bool,
    BoolValVisitor,
    decode_bool_val_async_,
    DecodeBoolValOp,
    DecodeBoolValOpAsync
);
value_newtype_!(
    U64Val,
    u64,
    EncodeU64Async,
    encode_u64,
    decode_u64,
    visit_u64,
    U64ValVisitor,
    decode_u64_val_async_,
    DecodeU64ValOp,
    DecodeU64ValOpAsync
);
value_newtype_!(
    F32Val,
    f32,
    EncodeF32Async,
    encode_f32,
    decode_f32,
    visit_f32,
    F32ValVisitor,
    decode_f32_val_async_,
    DecodeF32ValOp,
    DecodeF32ValOpAsync
);
value_newtype_!(
    F64Val,
    f64,
    EncodeF64Async,
    encode_f64,
    decode_f64,
    visit_f64,
    F64ValVisitor,
    decode_f64_val_async_,
    DecodeF64ValOp,
    DecodeF64ValOpAsync
);
value_newtype_!(
    I128Val,
    i128,
    EncodeI128Async,
    encode_i128,
    decode_i128,
    visit_i128,
    I128ValVisitor,
    decode_i128_val_async_,
    DecodeI128ValOp,
    DecodeI128ValOpAsync
);
value_newtype_!(
    U128Val,
    u128,
    EncodeU128Async,
    encode_u128,
    decode_u128,
    visit_u128,
    U128ValVisitor,
    decode_u128_val_async_,
    DecodeU128ValOp,
    DecodeU128ValOpAsync
);

/// 与 serde 的 transparent newtype 对应的 `i64` 测试类型。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(transparent)]
struct I64Val(pub i64);

impl TrEncodable for I64Val {
    type EncodeAsync<'a, S>
        = S::EncodeI64Async<'a>
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
        encoder.encode_i64(self.0)
    }
}

/// `I64Val` 的访问者；非负整数会以 `visit_u64` 到达，需要显式转换。
struct I64ValVisitor;

impl<'de> Visitor<'de> for I64ValVisitor {
    type Value = I64Val;

    fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("an i64")
    }

    fn visit_i64<'a, E>(
        self,
        v: i64,
    ) -> impl TrMayCancel<'a, MayCancelOutput = Result<Self::Value, E>>
    where
        Self: 'a,
        Self::Value: 'a,
        E: de::Error + 'a,
    {
        future::ready(Ok(I64Val(v)))
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
        future::ready(
            i64::try_from(v)
                .map(I64Val)
                .map_err(|_| <E as de::Error>::custom("u64 超出 i64 范围")),
        )
    }
}

#[gen_may_cancel_future(DecodeI64ValOp)]
async fn decode_i64_val_async_<'de, 'a, D, C>(de: D, cancel: C) -> Result<I64Val, D::Error>
where
    'de: 'a,
    D: TrDecoder<'de> + 'a,
    D::Error: 'a,
    C: TrCancellationToken,
{
    de.decode_i64(I64ValVisitor).may_cancel_with(cancel).await
}

impl<'de> TrDecodable<'de> for I64Val {
    type DecodeAsync<'a, D>
        = DecodeI64ValOpAsync<'de, 'a, 'a, D>
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
        DecodeI64ValOpAsync::new(de)
    }
}

/// 与 serde 的 transparent newtype 对应的字符串测试类型。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(transparent)]
struct StrVal(pub String);

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

/// `StrVal` 的访问者。
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

/// 与 serde 的 transparent newtype 对应的字符测试类型。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(transparent)]
struct CharVal(pub char);

impl TrEncodable for CharVal {
    type EncodeAsync<'a, S> = S::EncodeCharAsync<'a>
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
        encoder.encode_char(self.0)
    }
}

/// `CharVal` 的访问者；MessagePack 把 char 写成字符串。
struct CharValVisitor;

impl<'de> Visitor<'de> for CharValVisitor {
    type Value = CharVal;

    fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("a single character")
    }

    fn visit_char<'a, E>(
        self,
        v: char,
    ) -> impl TrMayCancel<'a, MayCancelOutput = Result<Self::Value, E>>
    where
        Self: 'a,
        Self::Value: 'a,
        E: de::Error + 'a,
    {
        future::ready(Ok(CharVal(v)))
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
        let mut chars = v.chars();
        let result = match (chars.next(), chars.next()) {
            (Some(c), None) => Ok(CharVal(c)),
            _ => Err(<E as de::Error>::custom("期望单个字符")),
        };
        future::ready(result)
    }
}

#[gen_may_cancel_future(DecodeCharValOp)]
async fn decode_char_val_async_<'de, 'a, D, C>(de: D, cancel: C) -> Result<CharVal, D::Error>
where
    'de: 'a,
    D: TrDecoder<'de> + 'a,
    D::Error: 'a,
    C: TrCancellationToken,
{
    de.decode_char(CharValVisitor).may_cancel_with(cancel).await
}

impl<'de> TrDecodable<'de> for CharVal {
    type DecodeAsync<'a, D> = DecodeCharValOpAsync<'de, 'a, 'a, D>
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
        DecodeCharValOpAsync::new(de)
    }
}

/// 与 `serde_bytes` 对应的二进制测试类型。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
struct BytesVal(#[serde(with = "serde_bytes")] pub Vec<u8>);

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

/// `BytesVal` 的访问者。
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

#[gen_may_cancel_future(DecodeBytesValOp)]
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

//-- ---- 结构体 ---------------------------------------------------------------

/// 覆盖 bool、有符号/无符号整数、浮点、字符与字符串的结构体。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
struct Scalars {
    pub b: bool,
    pub i: i64,
    pub u: u64,
    pub f: f32,
    pub d: f64,
    pub c: char,
    pub s: String,
}

#[gen_may_cancel_future(EncodeScalarsOp)]
async fn encode_scalars_async_<'a, S, C>(
    value: &'a Scalars,
    encoder: S,
    cancel: C,
) -> Result<S::Ok, S::Error>
where
    S: TrEncoder + 'a,
    S::Ok: 'a,
    S::Error: 'a,
    C: TrCancellationToken,
{
    let mut st = encoder
        .encode_struct("Scalars", 7)
        .may_cancel_with(cancel.child_token())
        .await?;
    st.encode_field("b", &BoolVal(value.b))
        .may_cancel_with(cancel.child_token())
        .await?;
    st.encode_field("i", &I64Val(value.i))
        .may_cancel_with(cancel.child_token())
        .await?;
    st.encode_field("u", &U64Val(value.u))
        .may_cancel_with(cancel.child_token())
        .await?;
    st.encode_field("f", &F32Val(value.f))
        .may_cancel_with(cancel.child_token())
        .await?;
    st.encode_field("d", &F64Val(value.d))
        .may_cancel_with(cancel.child_token())
        .await?;
    st.encode_field("c", &CharVal(value.c))
        .may_cancel_with(cancel.child_token())
        .await?;
    st.encode_field("s", &StrVal(value.s.clone()))
        .may_cancel_with(cancel.child_token())
        .await?;
    st.end().may_cancel_with(cancel).await
}

impl TrEncodable for Scalars {
    type EncodeAsync<'a, S>
        = EncodeScalarsOpAsync<'a, 'a, S>
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
        EncodeScalarsOpAsync::<'a, 'a, S>::new(self, encoder)
    }
}

#[gen_may_cancel_future(VisitScalarsMap)]
async fn visit_scalars_map_async_<'de, 'a, A, C>(mut map: A, cancel: C) -> Result<Scalars, A::Error>
where
    'de: 'a,
    A: MapAccess<'de> + 'a,
    A::Error: 'a,
    C: TrCancellationToken,
{
    let mut b = None;
    let mut i = None;
    let mut u = None;
    let mut f = None;
    let mut d = None;
    let mut c = None;
    let mut s = None;

    while let Some(key) = map
        .next_key::<StrVal>()
        .may_cancel_with(cancel.child_token())
        .await?
    {
        match key.0.as_str() {
            "b" => {
                b = Some(
                    map.next_value::<BoolVal>()
                        .may_cancel_with(cancel.child_token())
                        .await?
                        .0,
                );
            }
            "i" => {
                i = Some(
                    map.next_value::<I64Val>()
                        .may_cancel_with(cancel.child_token())
                        .await?
                        .0,
                );
            }
            "u" => {
                u = Some(
                    map.next_value::<U64Val>()
                        .may_cancel_with(cancel.child_token())
                        .await?
                        .0,
                );
            }
            "f" => {
                f = Some(
                    map.next_value::<F32Val>()
                        .may_cancel_with(cancel.child_token())
                        .await?
                        .0,
                );
            }
            "d" => {
                d = Some(
                    map.next_value::<F64Val>()
                        .may_cancel_with(cancel.child_token())
                        .await?
                        .0,
                );
            }
            "c" => {
                c = Some(
                    map.next_value::<CharVal>()
                        .may_cancel_with(cancel.child_token())
                        .await?
                        .0,
                );
            }
            "s" => {
                s = Some(
                    map.next_value::<StrVal>()
                        .may_cancel_with(cancel.child_token())
                        .await?
                        .0,
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

    Ok(Scalars {
        b: b.ok_or_else(|| <A::Error as de::Error>::missing_field("b"))?,
        i: i.ok_or_else(|| <A::Error as de::Error>::missing_field("i"))?,
        u: u.ok_or_else(|| <A::Error as de::Error>::missing_field("u"))?,
        f: f.ok_or_else(|| <A::Error as de::Error>::missing_field("f"))?,
        d: d.ok_or_else(|| <A::Error as de::Error>::missing_field("d"))?,
        c: c.ok_or_else(|| <A::Error as de::Error>::missing_field("c"))?,
        s: s.ok_or_else(|| <A::Error as de::Error>::missing_field("s"))?,
    })
}

#[gen_may_cancel_future(VisitScalarsSeq)]
async fn visit_scalars_seq_async_<'de, 'a, A, C>(mut seq: A, cancel: C) -> Result<Scalars, A::Error>
where
    'de: 'a,
    A: SeqAccess<'de> + 'a,
    A::Error: 'a,
    C: TrCancellationToken,
{
    let b = seq
        .next_element::<BoolVal>()
        .may_cancel_with(cancel.child_token())
        .await?
        .ok_or_else(|| <A::Error as de::Error>::invalid_length(0, &"Scalars"))?
        .0;
    let i = seq
        .next_element::<I64Val>()
        .may_cancel_with(cancel.child_token())
        .await?
        .ok_or_else(|| <A::Error as de::Error>::invalid_length(1, &"Scalars"))?
        .0;
    let u = seq
        .next_element::<U64Val>()
        .may_cancel_with(cancel.child_token())
        .await?
        .ok_or_else(|| <A::Error as de::Error>::invalid_length(2, &"Scalars"))?
        .0;
    let f = seq
        .next_element::<F32Val>()
        .may_cancel_with(cancel.child_token())
        .await?
        .ok_or_else(|| <A::Error as de::Error>::invalid_length(3, &"Scalars"))?
        .0;
    let d = seq
        .next_element::<F64Val>()
        .may_cancel_with(cancel.child_token())
        .await?
        .ok_or_else(|| <A::Error as de::Error>::invalid_length(4, &"Scalars"))?
        .0;
    let c = seq
        .next_element::<CharVal>()
        .may_cancel_with(cancel.child_token())
        .await?
        .ok_or_else(|| <A::Error as de::Error>::invalid_length(5, &"Scalars"))?
        .0;
    let s = seq
        .next_element::<StrVal>()
        .may_cancel_with(cancel.child_token())
        .await?
        .ok_or_else(|| <A::Error as de::Error>::invalid_length(6, &"Scalars"))?
        .0;
    Ok(Scalars {
        b,
        i,
        u,
        f,
        d,
        c,
        s,
    })
}

/// `Scalars` 的访问者，同时接受命名映射与位置数组。
struct ScalarsVisitor;

impl<'de> Visitor<'de> for ScalarsVisitor {
    type Value = Scalars;

    fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("a Scalars map or sequence")
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
        VisitScalarsMapAsync::<'de, 'a, 'a, A>::new(map)
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
        VisitScalarsSeqAsync::<'de, 'a, 'a, A>::new(seq)
    }
}

#[gen_may_cancel_future(DecodeScalarsOp)]
async fn decode_scalars_async_<'de, 'a, D, C>(de: D, cancel: C) -> Result<Scalars, D::Error>
where
    'de: 'a,
    D: TrDecoder<'de> + 'a,
    D::Error: 'a,
    C: TrCancellationToken,
{
    de.decode_struct(
        "Scalars",
        &["b", "i", "u", "f", "d", "c", "s"],
        ScalarsVisitor,
    )
    .may_cancel_with(cancel)
    .await
}

impl<'de> TrDecodable<'de> for Scalars {
    type DecodeAsync<'a, D>
        = DecodeScalarsOpAsync<'de, 'a, 'a, D>
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
        DecodeScalarsOpAsync::new(de)
    }
}

/// 覆盖二进制字段的结构体。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
struct Blob {
    #[serde(with = "serde_bytes")]
    pub data: Vec<u8>,
}

#[gen_may_cancel_future(EncodeBlobOp)]
async fn encode_blob_async_<'a, S, C>(
    value: &'a Blob,
    encoder: S,
    cancel: C,
) -> Result<S::Ok, S::Error>
where
    S: TrEncoder + 'a,
    S::Ok: 'a,
    S::Error: 'a,
    C: TrCancellationToken,
{
    let mut st = encoder
        .encode_struct("Blob", 1)
        .may_cancel_with(cancel.child_token())
        .await?;
    st.encode_field("data", &BytesVal(value.data.clone()))
        .may_cancel_with(cancel.child_token())
        .await?;
    st.end().may_cancel_with(cancel).await
}

impl TrEncodable for Blob {
    type EncodeAsync<'a, S>
        = EncodeBlobOpAsync<'a, 'a, S>
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
        EncodeBlobOpAsync::<'a, 'a, S>::new(self, encoder)
    }
}

#[gen_may_cancel_future(VisitBlobMap)]
async fn visit_blob_map_async_<'de, 'a, A, C>(mut map: A, cancel: C) -> Result<Blob, A::Error>
where
    'de: 'a,
    A: MapAccess<'de> + 'a,
    A::Error: 'a,
    C: TrCancellationToken,
{
    let mut data = None;
    while let Some(key) = map
        .next_key::<StrVal>()
        .may_cancel_with(cancel.child_token())
        .await?
    {
        match key.0.as_str() {
            "data" => {
                data = Some(
                    map.next_value::<BytesVal>()
                        .may_cancel_with(cancel.child_token())
                        .await?
                        .0,
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
    Ok(Blob {
        data: data.ok_or_else(|| <A::Error as de::Error>::missing_field("data"))?,
    })
}

#[gen_may_cancel_future(VisitBlobSeq)]
async fn visit_blob_seq_async_<'de, 'a, A, C>(mut seq: A, cancel: C) -> Result<Blob, A::Error>
where
    'de: 'a,
    A: SeqAccess<'de> + 'a,
    A::Error: 'a,
    C: TrCancellationToken,
{
    let data = seq
        .next_element::<BytesVal>()
        .may_cancel_with(cancel.child_token())
        .await?
        .ok_or_else(|| <A::Error as de::Error>::invalid_length(0, &"Blob"))?
        .0;
    Ok(Blob { data })
}

/// `Blob` 的访问者。
struct BlobVisitor;

impl<'de> Visitor<'de> for BlobVisitor {
    type Value = Blob;

    fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("a Blob map or sequence")
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
        VisitBlobMapAsync::<'de, 'a, 'a, A>::new(map)
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
        VisitBlobSeqAsync::<'de, 'a, 'a, A>::new(seq)
    }
}

#[gen_may_cancel_future(DecodeBlobOp)]
async fn decode_blob_async_<'de, 'a, D, C>(de: D, cancel: C) -> Result<Blob, D::Error>
where
    'de: 'a,
    D: TrDecoder<'de> + 'a,
    D::Error: 'a,
    C: TrCancellationToken,
{
    de.decode_struct("Blob", &["data"], BlobVisitor)
        .may_cancel_with(cancel)
        .await
}

impl<'de> TrDecodable<'de> for Blob {
    type DecodeAsync<'a, D>
        = DecodeBlobOpAsync<'de, 'a, 'a, D>
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
        DecodeBlobOpAsync::new(de)
    }
}

/// 覆盖 128 位整数的结构体。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
struct BigInts {
    pub i: i128,
    pub u: u128,
}

#[gen_may_cancel_future(EncodeBigIntsOp)]
async fn encode_big_ints_async_<'a, S, C>(
    value: &'a BigInts,
    encoder: S,
    cancel: C,
) -> Result<S::Ok, S::Error>
where
    S: TrEncoder + 'a,
    S::Ok: 'a,
    S::Error: 'a,
    C: TrCancellationToken,
{
    let mut st = encoder
        .encode_struct("BigInts", 2)
        .may_cancel_with(cancel.child_token())
        .await?;
    st.encode_field("i", &I128Val(value.i))
        .may_cancel_with(cancel.child_token())
        .await?;
    st.encode_field("u", &U128Val(value.u))
        .may_cancel_with(cancel.child_token())
        .await?;
    st.end().may_cancel_with(cancel).await
}

impl TrEncodable for BigInts {
    type EncodeAsync<'a, S>
        = EncodeBigIntsOpAsync<'a, 'a, S>
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
        EncodeBigIntsOpAsync::<'a, 'a, S>::new(self, encoder)
    }
}

#[gen_may_cancel_future(VisitBigIntsMap)]
async fn visit_big_ints_map_async_<'de, 'a, A, C>(
    mut map: A,
    cancel: C,
) -> Result<BigInts, A::Error>
where
    'de: 'a,
    A: MapAccess<'de> + 'a,
    A::Error: 'a,
    C: TrCancellationToken,
{
    let mut i = None;
    let mut u = None;
    while let Some(key) = map
        .next_key::<StrVal>()
        .may_cancel_with(cancel.child_token())
        .await?
    {
        match key.0.as_str() {
            "i" => {
                i = Some(
                    map.next_value::<I128Val>()
                        .may_cancel_with(cancel.child_token())
                        .await?
                        .0,
                );
            }
            "u" => {
                u = Some(
                    map.next_value::<U128Val>()
                        .may_cancel_with(cancel.child_token())
                        .await?
                        .0,
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
    Ok(BigInts {
        i: i.ok_or_else(|| <A::Error as de::Error>::missing_field("i"))?,
        u: u.ok_or_else(|| <A::Error as de::Error>::missing_field("u"))?,
    })
}

#[gen_may_cancel_future(VisitBigIntsSeq)]
async fn visit_big_ints_seq_async_<'de, 'a, A, C>(
    mut seq: A,
    cancel: C,
) -> Result<BigInts, A::Error>
where
    'de: 'a,
    A: SeqAccess<'de> + 'a,
    A::Error: 'a,
    C: TrCancellationToken,
{
    let i = seq
        .next_element::<I128Val>()
        .may_cancel_with(cancel.child_token())
        .await?
        .ok_or_else(|| <A::Error as de::Error>::invalid_length(0, &"BigInts"))?
        .0;
    let u = seq
        .next_element::<U128Val>()
        .may_cancel_with(cancel.child_token())
        .await?
        .ok_or_else(|| <A::Error as de::Error>::invalid_length(1, &"BigInts"))?
        .0;
    Ok(BigInts { i, u })
}

/// `BigInts` 的访问者。
struct BigIntsVisitor;

impl<'de> Visitor<'de> for BigIntsVisitor {
    type Value = BigInts;

    fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("a BigInts map or sequence")
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
        VisitBigIntsMapAsync::<'de, 'a, 'a, A>::new(map)
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
        VisitBigIntsSeqAsync::<'de, 'a, 'a, A>::new(seq)
    }
}

#[gen_may_cancel_future(DecodeBigIntsOp)]
async fn decode_big_ints_async_<'de, 'a, D, C>(de: D, cancel: C) -> Result<BigInts, D::Error>
where
    'de: 'a,
    D: TrDecoder<'de> + 'a,
    D::Error: 'a,
    C: TrCancellationToken,
{
    de.decode_struct("BigInts", &["i", "u"], BigIntsVisitor)
        .may_cancel_with(cancel)
        .await
}

impl<'de> TrDecodable<'de> for BigInts {
    type DecodeAsync<'a, D>
        = DecodeBigIntsOpAsync<'de, 'a, 'a, D>
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
        DecodeBigIntsOpAsync::new(de)
    }
}

//-- ---- 序列、Option、单元结构体、newtype 与元组结构体 ----------------------

/// 一个 `Vec<u64>`。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(transparent)]
struct U64List(pub Vec<u64>);

#[gen_may_cancel_future(EncodeU64ListOp)]
async fn encode_u64_list_async_<'a, S, C>(
    value: &'a U64List,
    encoder: S,
    cancel: C,
) -> Result<S::Ok, S::Error>
where
    S: TrEncoder + 'a,
    S::Ok: 'a,
    S::Error: 'a,
    C: TrCancellationToken,
{
    let mut seq = encoder
        .encode_seq(Some(value.0.len()))
        .may_cancel_with(cancel.child_token())
        .await?;
    for item in &value.0 {
        seq.encode_element(&U64Val(*item))
            .may_cancel_with(cancel.child_token())
            .await?;
    }
    seq.end().may_cancel_with(cancel).await
}

impl TrEncodable for U64List {
    type EncodeAsync<'a, S>
        = EncodeU64ListOpAsync<'a, 'a, S>
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
        EncodeU64ListOpAsync::<'a, 'a, S>::new(self, encoder)
    }
}

#[gen_may_cancel_future(VisitU64List)]
async fn visit_u64_list_async_<'de, 'a, A, C>(mut seq: A, cancel: C) -> Result<U64List, A::Error>
where
    'de: 'a,
    A: SeqAccess<'de> + 'a,
    A::Error: 'a,
    C: TrCancellationToken,
{
    let mut values = Vec::new();
    while let Some(value) = seq
        .next_element::<U64Val>()
        .may_cancel_with(cancel.child_token())
        .await?
    {
        values.push(value.0);
    }
    Ok(U64List(values))
}

/// `U64List` 的访问者。
struct U64ListVisitor;

impl<'de> Visitor<'de> for U64ListVisitor {
    type Value = U64List;

    fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("an array of u64")
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
        VisitU64ListAsync::<'de, 'a, 'a, A>::new(seq)
    }
}

#[gen_may_cancel_future(DecodeU64ListOp)]
async fn decode_u64_list_async_<'de, 'a, D, C>(de: D, cancel: C) -> Result<U64List, D::Error>
where
    'de: 'a,
    D: TrDecoder<'de> + 'a,
    D::Error: 'a,
    C: TrCancellationToken,
{
    de.decode_seq(U64ListVisitor).may_cancel_with(cancel).await
}

impl<'de> TrDecodable<'de> for U64List {
    type DecodeAsync<'a, D>
        = DecodeU64ListOpAsync<'de, 'a, 'a, D>
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
        DecodeU64ListOpAsync::new(de)
    }
}

/// 一个 `Option<u64>`。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(transparent)]
struct MaybeU64(pub Option<u64>);

#[gen_may_cancel_future(EncodeMaybeU64Op)]
async fn encode_maybe_u64_async_<'a, S, C>(
    value: &'a MaybeU64,
    encoder: S,
    cancel: C,
) -> Result<S::Ok, S::Error>
where
    S: TrEncoder + 'a,
    S::Ok: 'a,
    S::Error: 'a,
    C: TrCancellationToken,
{
    match value.0 {
        Some(v) => {
            encoder
                .encode_some(&U64Val(v))
                .may_cancel_with(cancel)
                .await
        }
        None => encoder.encode_none().may_cancel_with(cancel).await,
    }
}

impl TrEncodable for MaybeU64 {
    type EncodeAsync<'a, S>
        = EncodeMaybeU64OpAsync<'a, 'a, S>
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
        EncodeMaybeU64OpAsync::<'a, 'a, S>::new(self, encoder)
    }
}

#[gen_may_cancel_future(VisitMaybeU64Some)]
async fn visit_maybe_u64_some_async_<'de, 'a, D, C>(de: D, cancel: C) -> Result<MaybeU64, D::Error>
where
    'de: 'a,
    D: TrDecoder<'de> + 'a,
    D::Error: 'a,
    C: TrCancellationToken,
{
    let value = U64Val::decode_async(de).may_cancel_with(cancel).await?;
    Ok(MaybeU64(Some(value.0)))
}

/// `MaybeU64` 的访问者。
struct MaybeU64Visitor;

impl<'de> Visitor<'de> for MaybeU64Visitor {
    type Value = MaybeU64;

    fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("an optional u64")
    }

    fn visit_none<'a, E>(self) -> impl TrMayCancel<'a, MayCancelOutput = Result<Self::Value, E>>
    where
        Self: 'a,
        Self::Value: 'a,
        E: de::Error + 'a,
    {
        future::ready(Ok(MaybeU64(None)))
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
        VisitMaybeU64SomeAsync::<'de, 'a, 'a, D>::new(de)
    }
}

#[gen_may_cancel_future(DecodeMaybeU64Op)]
async fn decode_maybe_u64_async_<'de, 'a, D, C>(de: D, cancel: C) -> Result<MaybeU64, D::Error>
where
    'de: 'a,
    D: TrDecoder<'de> + 'a,
    D::Error: 'a,
    C: TrCancellationToken,
{
    de.decode_option(MaybeU64Visitor)
        .may_cancel_with(cancel)
        .await
}

impl<'de> TrDecodable<'de> for MaybeU64 {
    type DecodeAsync<'a, D>
        = DecodeMaybeU64OpAsync<'de, 'a, 'a, D>
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
        DecodeMaybeU64OpAsync::new(de)
    }
}

/// 单元结构体。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
struct Unit;

impl TrEncodable for Unit {
    type EncodeAsync<'a, S>
        = S::EncodeUnitStructAsync<'a>
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
        encoder.encode_unit_struct("Unit")
    }
}

/// `Unit` 的访问者。
struct UnitVisitor;

impl<'de> Visitor<'de> for UnitVisitor {
    type Value = Unit;

    fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("a unit struct")
    }

    fn visit_unit<'a, E>(self) -> impl TrMayCancel<'a, MayCancelOutput = Result<Self::Value, E>>
    where
        Self: 'a,
        Self::Value: 'a,
        E: de::Error + 'a,
    {
        future::ready(Ok(Unit))
    }
}

#[gen_may_cancel_future(DecodeUnitOp)]
async fn decode_unit_async_<'de, 'a, D, C>(de: D, cancel: C) -> Result<Unit, D::Error>
where
    'de: 'a,
    D: TrDecoder<'de> + 'a,
    D::Error: 'a,
    C: TrCancellationToken,
{
    de.decode_unit_struct("Unit", UnitVisitor)
        .may_cancel_with(cancel)
        .await
}

impl<'de> TrDecodable<'de> for Unit {
    type DecodeAsync<'a, D>
        = DecodeUnitOpAsync<'de, 'a, 'a, D>
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
        DecodeUnitOpAsync::new(de)
    }
}

/// 只包含 `u64` 的 newtype 结构体。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
struct NewtypeU64(pub u64);

#[gen_may_cancel_future(EncodeNewtypeU64Op)]
async fn encode_newtype_u64_async_<'a, S, C>(
    value: &'a NewtypeU64,
    encoder: S,
    cancel: C,
) -> Result<S::Ok, S::Error>
where
    S: TrEncoder + 'a,
    S::Ok: 'a,
    S::Error: 'a,
    C: TrCancellationToken,
{
    encoder
        .encode_newtype_struct("NewtypeU64", &U64Val(value.0))
        .may_cancel_with(cancel)
        .await
}

impl TrEncodable for NewtypeU64 {
    type EncodeAsync<'a, S>
        = EncodeNewtypeU64OpAsync<'a, 'a, S>
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
        EncodeNewtypeU64OpAsync::<'a, 'a, S>::new(self, encoder)
    }
}

#[gen_may_cancel_future(VisitNewtypeU64)]
async fn visit_newtype_u64_async_<'de, 'a, D, C>(de: D, cancel: C) -> Result<NewtypeU64, D::Error>
where
    'de: 'a,
    D: TrDecoder<'de> + 'a,
    D::Error: 'a,
    C: TrCancellationToken,
{
    let value = U64Val::decode_async(de).may_cancel_with(cancel).await?;
    Ok(NewtypeU64(value.0))
}

/// `NewtypeU64` 的访问者。
struct NewtypeU64Visitor;

impl<'de> Visitor<'de> for NewtypeU64Visitor {
    type Value = NewtypeU64;

    fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("a u64 newtype struct")
    }

    fn visit_newtype_struct<'a, D>(
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
        VisitNewtypeU64Async::<'de, 'a, 'a, D>::new(de)
    }
}

#[gen_may_cancel_future(DecodeNewtypeU64Op)]
async fn decode_newtype_u64_async_<'de, 'a, D, C>(de: D, cancel: C) -> Result<NewtypeU64, D::Error>
where
    'de: 'a,
    D: TrDecoder<'de> + 'a,
    D::Error: 'a,
    C: TrCancellationToken,
{
    de.decode_newtype_struct("NewtypeU64", NewtypeU64Visitor)
        .may_cancel_with(cancel)
        .await
}

impl<'de> TrDecodable<'de> for NewtypeU64 {
    type DecodeAsync<'a, D>
        = DecodeNewtypeU64OpAsync<'de, 'a, 'a, D>
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
        DecodeNewtypeU64OpAsync::new(de)
    }
}

/// 元组结构体。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
struct PairTuple(pub u64, pub String);

#[gen_may_cancel_future(EncodePairTupleOp)]
async fn encode_pair_tuple_async_<'a, S, C>(
    value: &'a PairTuple,
    encoder: S,
    cancel: C,
) -> Result<S::Ok, S::Error>
where
    S: TrEncoder + 'a,
    S::Ok: 'a,
    S::Error: 'a,
    C: TrCancellationToken,
{
    let mut st = encoder
        .encode_tuple_struct("PairTuple", 2)
        .may_cancel_with(cancel.child_token())
        .await?;
    st.encode_field(&U64Val(value.0))
        .may_cancel_with(cancel.child_token())
        .await?;
    st.encode_field(&StrVal(value.1.clone()))
        .may_cancel_with(cancel.child_token())
        .await?;
    st.end().may_cancel_with(cancel).await
}

impl TrEncodable for PairTuple {
    type EncodeAsync<'a, S>
        = EncodePairTupleOpAsync<'a, 'a, S>
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
        EncodePairTupleOpAsync::<'a, 'a, S>::new(self, encoder)
    }
}

#[gen_may_cancel_future(VisitPairTuple)]
async fn visit_pair_tuple_async_<'de, 'a, A, C>(
    mut seq: A,
    cancel: C,
) -> Result<PairTuple, A::Error>
where
    'de: 'a,
    A: SeqAccess<'de> + 'a,
    A::Error: 'a,
    C: TrCancellationToken,
{
    let first = seq
        .next_element::<U64Val>()
        .may_cancel_with(cancel.child_token())
        .await?
        .ok_or_else(|| <A::Error as de::Error>::invalid_length(0, &"PairTuple"))?
        .0;
    let second = seq
        .next_element::<StrVal>()
        .may_cancel_with(cancel.child_token())
        .await?
        .ok_or_else(|| <A::Error as de::Error>::invalid_length(1, &"PairTuple"))?
        .0;
    Ok(PairTuple(first, second))
}

/// `PairTuple` 的访问者。
struct PairTupleVisitor;

impl<'de> Visitor<'de> for PairTupleVisitor {
    type Value = PairTuple;

    fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("a PairTuple sequence")
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
        VisitPairTupleAsync::<'de, 'a, 'a, A>::new(seq)
    }
}

#[gen_may_cancel_future(DecodePairTupleOp)]
async fn decode_pair_tuple_async_<'de, 'a, D, C>(de: D, cancel: C) -> Result<PairTuple, D::Error>
where
    'de: 'a,
    D: TrDecoder<'de> + 'a,
    D::Error: 'a,
    C: TrCancellationToken,
{
    de.decode_tuple_struct("PairTuple", 2, PairTupleVisitor)
        .may_cancel_with(cancel)
        .await
}

impl<'de> TrDecodable<'de> for PairTuple {
    type DecodeAsync<'a, D>
        = DecodePairTupleOpAsync<'de, 'a, 'a, D>
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
        DecodePairTupleOpAsync::new(de)
    }
}

//-- ---- 枚举 ---------------------------------------------------------------

/// 覆盖四种变体形态的枚举。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
enum Shape {
    Empty,
    Num(u64),
    Pair(u64, String),
    Named {
        #[serde(rename = "a")]
        a_: u64,
        #[serde(rename = "b")]
        b_: String,
    },
}

#[gen_may_cancel_future(EncodeShapeOp)]
async fn encode_shape_async_<'a, S, C>(
    value: &'a Shape,
    encoder: S,
    cancel: C,
) -> Result<S::Ok, S::Error>
where
    S: TrEncoder + 'a,
    S::Ok: 'a,
    S::Error: 'a,
    C: TrCancellationToken,
{
    match value {
        Shape::Empty => {
            encoder
                .encode_unit_variant("Shape", 0, "Empty")
                .may_cancel_with(cancel)
                .await
        }
        Shape::Num(v) => {
            encoder
                .encode_newtype_variant("Shape", 1, "Num", &U64Val(*v))
                .may_cancel_with(cancel)
                .await
        }
        Shape::Pair(n, s) => {
            let mut st = encoder
                .encode_tuple_variant("Shape", 2, "Pair", 2)
                .may_cancel_with(cancel.child_token())
                .await?;
            st.encode_field(&U64Val(*n))
                .may_cancel_with(cancel.child_token())
                .await?;
            st.encode_field(&StrVal(s.clone()))
                .may_cancel_with(cancel.child_token())
                .await?;
            st.end().may_cancel_with(cancel).await
        }
        Shape::Named { a_, b_ } => {
            let mut st = encoder
                .encode_struct_variant("Shape", 3, "Named", 2)
                .may_cancel_with(cancel.child_token())
                .await?;
            st.encode_field("a", &U64Val(*a_))
                .may_cancel_with(cancel.child_token())
                .await?;
            st.encode_field("b", &StrVal(b_.clone()))
                .may_cancel_with(cancel.child_token())
                .await?;
            st.end().may_cancel_with(cancel).await
        }
    }
}

impl TrEncodable for Shape {
    type EncodeAsync<'a, S>
        = EncodeShapeOpAsync<'a, 'a, S>
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
        EncodeShapeOpAsync::<'a, 'a, S>::new(self, encoder)
    }
}

#[gen_may_cancel_future(VisitPairVariant)]
async fn visit_pair_variant_async_<'de, 'a, A, C>(
    mut seq: A,
    cancel: C,
) -> Result<(u64, String), A::Error>
where
    'de: 'a,
    A: SeqAccess<'de> + 'a,
    A::Error: 'a,
    C: TrCancellationToken,
{
    let first = seq
        .next_element::<U64Val>()
        .may_cancel_with(cancel.child_token())
        .await?
        .ok_or_else(|| <A::Error as de::Error>::invalid_length(0, &"Pair variant"))?
        .0;
    let second = seq
        .next_element::<StrVal>()
        .may_cancel_with(cancel.child_token())
        .await?
        .ok_or_else(|| <A::Error as de::Error>::invalid_length(1, &"Pair variant"))?
        .0;
    Ok((first, second))
}

/// 元组变体的内容访问者。
struct PairVariantVisitor;

impl<'de> Visitor<'de> for PairVariantVisitor {
    type Value = (u64, String);

    fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("a Pair tuple variant")
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
        VisitPairVariantAsync::<'de, 'a, 'a, A>::new(seq)
    }
}

#[gen_may_cancel_future(VisitNamedVariantMap)]
async fn visit_named_variant_map_async_<'de, 'a, A, C>(
    mut map: A,
    cancel: C,
) -> Result<(u64, String), A::Error>
where
    'de: 'a,
    A: MapAccess<'de> + 'a,
    A::Error: 'a,
    C: TrCancellationToken,
{
    let mut a = None;
    let mut b = None;
    while let Some(key) = map
        .next_key::<StrVal>()
        .may_cancel_with(cancel.child_token())
        .await?
    {
        match key.0.as_str() {
            "a" => {
                a = Some(
                    map.next_value::<U64Val>()
                        .may_cancel_with(cancel.child_token())
                        .await?
                        .0,
                );
            }
            "b" => {
                b = Some(
                    map.next_value::<StrVal>()
                        .may_cancel_with(cancel.child_token())
                        .await?
                        .0,
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
    Ok((
        a.ok_or_else(|| <A::Error as de::Error>::missing_field("a"))?,
        b.ok_or_else(|| <A::Error as de::Error>::missing_field("b"))?,
    ))
}

#[gen_may_cancel_future(VisitNamedVariantSeq)]
async fn visit_named_variant_seq_async_<'de, 'a, A, C>(
    mut seq: A,
    cancel: C,
) -> Result<(u64, String), A::Error>
where
    'de: 'a,
    A: SeqAccess<'de> + 'a,
    A::Error: 'a,
    C: TrCancellationToken,
{
    let a = seq
        .next_element::<U64Val>()
        .may_cancel_with(cancel.child_token())
        .await?
        .ok_or_else(|| <A::Error as de::Error>::invalid_length(0, &"Named variant"))?
        .0;
    let b = seq
        .next_element::<StrVal>()
        .may_cancel_with(cancel.child_token())
        .await?
        .ok_or_else(|| <A::Error as de::Error>::invalid_length(1, &"Named variant"))?
        .0;
    Ok((a, b))
}

/// 结构体变体的内容访问者，同时接受命名映射与位置数组。
struct NamedVariantVisitor;

impl<'de> Visitor<'de> for NamedVariantVisitor {
    type Value = (u64, String);

    fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("a Named struct variant")
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
        VisitNamedVariantMapAsync::<'de, 'a, 'a, A>::new(map)
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
        VisitNamedVariantSeqAsync::<'de, 'a, 'a, A>::new(seq)
    }
}

#[gen_may_cancel_future(VisitShapeEnum)]
async fn visit_shape_enum_async_<'de, 'a, A, C>(data: A, cancel: C) -> Result<Shape, A::Error>
where
    'de: 'a,
    A: EnumAccess<'de> + 'a,
    A::Error: 'a,
    C: TrCancellationToken,
{
    let (variant_name, variant) = data
        .variant::<StrVal>()
        .may_cancel_with(cancel.child_token())
        .await?;
    match variant_name.0.as_str() {
        "Empty" => {
            variant.unit_variant().may_cancel_with(cancel).await?;
            Ok(Shape::Empty)
        }
        "Num" => {
            let value = variant
                .newtype_variant::<U64Val>()
                .may_cancel_with(cancel)
                .await?;
            Ok(Shape::Num(value.0))
        }
        "Pair" => {
            let (first, second) = variant
                .tuple_variant(2, PairVariantVisitor)
                .may_cancel_with(cancel)
                .await?;
            Ok(Shape::Pair(first, second))
        }
        "Named" => {
            let (a, b) = variant
                .struct_variant(&["a", "b"], NamedVariantVisitor)
                .may_cancel_with(cancel)
                .await?;
            Ok(Shape::Named { a_: a, b_: b })
        }
        other => Err(<A::Error as de::Error>::unknown_variant(
            other,
            &["Empty", "Num", "Pair", "Named"],
        )),
    }
}

/// `Shape` 的访问者。
struct ShapeVisitor;

impl<'de> Visitor<'de> for ShapeVisitor {
    type Value = Shape;

    fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("a Shape enum")
    }

    fn visit_enum<'a, A>(
        self,
        data: A,
    ) -> impl TrMayCancel<'a, MayCancelOutput = Result<Self::Value, A::Error>>
    where
        'de: 'a,
        Self: 'a,
        Self::Value: 'a,
        A: EnumAccess<'de> + 'a,
        A::Error: 'a,
    {
        VisitShapeEnumAsync::<'de, 'a, 'a, A>::new(data)
    }
}

#[gen_may_cancel_future(DecodeShapeOp)]
async fn decode_shape_async_<'de, 'a, D, C>(de: D, cancel: C) -> Result<Shape, D::Error>
where
    'de: 'a,
    D: TrDecoder<'de> + 'a,
    D::Error: 'a,
    C: TrCancellationToken,
{
    de.decode_enum("Shape", &["Empty", "Num", "Pair", "Named"], ShapeVisitor)
        .may_cancel_with(cancel)
        .await
}

impl<'de> TrDecodable<'de> for Shape {
    type DecodeAsync<'a, D>
        = DecodeShapeOpAsync<'de, 'a, 'a, D>
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
        DecodeShapeOpAsync::new(de)
    }
}

//-- ---- 测试 ---------------------------------------------------------------

/// 验证标量结构体的双向互操作。
/// Verify bidirectional interop for the scalar struct.
///
/// - 手段 / Method: `Scalars` 同时实现 serde 与 denco trait，交给
///   `check_cross_` 做命名/紧凑/交叉三条路径。
/// - 判断 / Assert: 命名字节相等，三条路径都解回原值。
#[test]
fn scalars_cross_() {
    check_cross_(&Scalars {
        b: true,
        i: -300,
        u: 70_000,
        f: 1.5,
        d: -2.25,
        c: '中',
        s: String::from("denco"),
    });
}

/// 验证二进制字段的双向互操作。
/// Verify bidirectional interop for a binary field.
///
/// - 手段 / Method: `Blob` 的 `data` 字段使用 `serde_bytes`，denco 侧调用
///   `encode_bytes` / `decode_bytes`。
/// - 判断 / Assert: 命名字节相等，rmp 与 mp 互相解码结果都等于原值。
#[test]
fn bytes_cross_() {
    check_cross_(&Blob {
        data: vec![0x00, 0x01, 0x02, 0xff],
    });
}

/// 验证 128 位整数的双向互操作。
/// Verify bidirectional interop for 128-bit integers.
///
/// - 手段 / Method: `BigInts` 使用 `i128` / `u128`，两边都应按 16 字节
///   大端 binary 编码。
/// - 判断 / Assert: 命名字节相等，交叉解码等于原值。
#[test]
fn big_ints_cross_() {
    check_cross_(&BigInts {
        i: -12_345_678_901_234_567_890,
        u: u128::MAX - 1,
    });
}

/// 验证数组的双向互操作。
/// Verify bidirectional interop for arrays.
///
/// - 手段 / Method: `U64List` 同时走 denco 的 `encode_seq` 与 rmp 的数组编码。
/// - 判断 / Assert: 命名字节相等，交叉解码等于原值。
#[test]
fn array_cross_() {
    check_cross_(&U64List(vec![0, 1, 300, u64::MAX]));
}

/// 验证 `Option::Some` 的双向互操作。
/// Verify bidirectional interop for `Option::Some`.
///
/// - 手段 / Method: `MaybeU64(Some(7))`。
/// - 判断 / Assert: 命名字节相等，交叉解码等于原值。
#[test]
fn option_some_cross_() {
    check_cross_(&MaybeU64(Some(7)));
}

/// 验证 `Option::None` 的双向互操作。
/// Verify bidirectional interop for `Option::None`.
///
/// - 手段 / Method: `MaybeU64(None)` 应编码为 nil。
/// - 判断 / Assert: 命名字节相等，交叉解码等于原值。
#[test]
fn option_none_cross_() {
    check_cross_(&MaybeU64(None));
}

/// 验证单元结构体的双向互操作。
/// Verify bidirectional interop for unit structs.
///
/// - 手段 / Method: `Unit` 在 rmp-serde 中编码为空数组 `0x90`。
/// - 判断 / Assert: 命名字节相等，交叉解码等于原值。
#[test]
fn unit_struct_cross_() {
    check_cross_(&Unit);
}

/// 验证 newtype 结构体的双向互操作。
/// Verify bidirectional interop for newtype structs.
///
/// - 手段 / Method: `NewtypeU64(300)` 应透明编码为内部 `u64`。
/// - 判断 / Assert: 命名字节相等，交叉解码等于原值。
#[test]
fn newtype_struct_cross_() {
    check_cross_(&NewtypeU64(300));
}

/// 验证元组结构体的双向互操作。
/// Verify bidirectional interop for tuple structs.
///
/// - 手段 / Method: `PairTuple(7, "denco")` 应编码为二元数组。
/// - 判断 / Assert: 命名字节相等，交叉解码等于原值。
#[test]
fn tuple_struct_cross_() {
    check_cross_(&PairTuple(7, String::from("denco")));
}

/// 验证枚举四种变体形态的双向互操作。
/// Verify bidirectional interop for all four enum variant shapes.
///
/// - 手段 / Method: 依次检查 unit、newtype、tuple、struct 变体。
/// - 判断 / Assert: 每个变体的命名字节相等，三条交叉路径都解回原值。
#[test]
fn enum_variants_cross_() {
    for shape in [
        Shape::Empty,
        Shape::Num(7),
        Shape::Pair(8, String::from("pair")),
        Shape::Named {
            a_: 9,
            b_: String::from("named"),
        },
    ] {
        check_cross_(&shape);
    }
}
