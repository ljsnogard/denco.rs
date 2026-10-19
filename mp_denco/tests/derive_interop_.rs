//! 派生宏生成的 `TrEncodable` / `TrDecodable` 与 `serde` 的互操作测试。
//!
//! 这些类型同时派生 serde 的 `Serialize` / `Deserialize` 与 denco 的
//! `TrEncodable` / `TrDecodable`，覆盖 `String`、数组、`Vec`、`Option` 与
//! `Arc` 等常见字段类型。

#![feature(impl_trait_in_assoc_type)]

use core::{
    future::Future,
    task::{Context, Poll},
};
use std::{sync::Arc, task::Waker};

use abs_cancel::TrMayCancel;
use abs_denco::{de::TrDecodable, ser::TrEncodable};
use denco_macro::{TrDecodable, TrEncodable};
use mp_denco::{MpDecoder, MpEncoder};
use serde::{Deserialize, Serialize};

/// 立即完成、对 `Pending` 让出 CPU 的测试执行器。
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

/// 用 `mp_denco` 编码一个值。
fn mp_encode_<T>(value: &T) -> Vec<u8>
where
    T: ?Sized + TrEncodable,
{
    let mut storage = vec![0u8; 4096];
    let total = storage.len();
    let written = {
        let mut sink: &mut [u8] = storage.as_mut_slice();
        block_on_(async { value.encode_async(MpEncoder::new(&mut sink)).await })
            .expect("mp_denco 编码应当成功");
        total - sink.len()
    };
    storage.truncate(written);
    storage
}

/// 用 `mp_denco` 的命名映射配置编码一个值。
///
/// 结构体字段会连同字段名一起写入，对应 `rmp_serde::to_vec_named` 的表示。
fn mp_encode_named_<T>(value: &T) -> Vec<u8>
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
                .await
        })
        .expect("mp_denco 命名编码应当成功");
        total - sink.len()
    };
    storage.truncate(written);
    storage
}

/// 用 `mp_denco` 解码一个值。
fn mp_decode_<'de, T>(bytes: &'de [u8]) -> T
where
    T: TrDecodable<'de>,
{
    let mut input: &'de [u8] = bytes;
    block_on_(async { T::decode_async(MpDecoder::new(&mut input)).await })
        .expect("mp_denco 解码应当成功")
}

//-- ---- 测试类型 -------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TrEncodable, TrDecodable)]
struct Inner {
    id: u64,
    label: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TrEncodable, TrDecodable)]
struct Classic {
    name: String,
    tags: Vec<String>,
    bytes: [u8; 4],
    numbers: [u32; 3],
    shared: Arc<u64>,
    maybe: Option<u32>,
    nested: Arc<Inner>,
    flag: bool,
    letter: char,
    ratio: f32,
    score: f64,
    small: i8,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TrEncodable, TrDecodable)]
enum ClassicEnum {
    Empty,
    Text(String),
    Bytes([u8; 3]),
    Shared(Arc<u64>),
    Struct { name: String, nums: [u16; 2] },
}

//-- ---- 测试 ----------------------------------------------------------------

/// 验证包含 String/数组/Vec/Option/Arc 的结构体双向互通。
///
/// - 手段：同一个 `Classic` 分别用默认配置（位置数组）与 `with_struct_map`
///   编码，再与 `rmp_serde::to_vec` / `to_vec_named` 交叉解码。
/// - 判断：两种表示的字节分别一致；各解码路径都等于原值。
#[test]
fn classic_struct_interop_() {
    let value = Classic {
        name: String::from("denco"),
        tags: vec![String::from("a"), String::from("b")],
        bytes: [0xde, 0xad, 0xbe, 0xef],
        numbers: [1, 2, u32::MAX],
        shared: Arc::new(42),
        maybe: Some(7),
        nested: Arc::new(Inner {
            id: 300,
            label: String::from("inner"),
        }),
        flag: true,
        letter: '中',
        ratio: 1.5,
        score: -2.25,
        small: -7,
    };

    let mp_bytes = mp_encode_(&value);
    let rmp_compact = rmp_serde::to_vec(&value).expect("rmp_serde compact 编码应当成功");
    assert_eq!(mp_bytes, rmp_compact, "默认位置数组编码应当一致");

    let mp_named = mp_encode_named_(&value);
    let rmp_named = rmp_serde::to_vec_named(&value).expect("rmp_serde named 编码应当成功");
    assert_eq!(mp_named, rmp_named, "命名映射编码应当一致");

    let from_mp: Classic = rmp_serde::from_slice(&mp_bytes).expect("rmp 应能解码 mp 字节");
    assert_eq!(from_mp, value);

    let from_mp_named: Classic =
        rmp_serde::from_slice(&mp_named).expect("rmp 应能解码 mp 命名字节");
    assert_eq!(from_mp_named, value);

    let from_rmp_named: Classic = mp_decode_(&rmp_named);
    assert_eq!(from_rmp_named, value);

    let from_rmp_compact: Classic = mp_decode_(&rmp_compact);
    assert_eq!(from_rmp_compact, value);
}

/// 验证包含 String/数组/Arc 的枚举各变体双向互通。
///
/// - 手段：对 `ClassicEnum` 的 unit/newtype/tuple/struct 变体分别用默认配置
///   与命名映射配置编码，再与 `rmp_serde` 的紧凑/命名表示交叉编解码。
/// - 判断：两种表示字节分别一致，且各解码路径都得到原值。
#[test]
fn classic_enum_interop_() {
    for value in [
        ClassicEnum::Empty,
        ClassicEnum::Text(String::from("hello")),
        ClassicEnum::Bytes([1, 2, 3]),
        ClassicEnum::Shared(Arc::new(9)),
        ClassicEnum::Struct {
            name: String::from("variant"),
            nums: [10, 20],
        },
    ] {
        let mp_bytes = mp_encode_(&value);
        let rmp_compact = rmp_serde::to_vec(&value).expect("rmp_serde compact 编码应当成功");
        assert_eq!(mp_bytes, rmp_compact, "枚举默认位置数组编码应当一致");

        let mp_named = mp_encode_named_(&value);
        let rmp_named = rmp_serde::to_vec_named(&value).expect("rmp_serde named 编码应当成功");
        assert_eq!(mp_named, rmp_named, "枚举命名映射编码应当一致");

        let from_mp: ClassicEnum =
            rmp_serde::from_slice(&mp_bytes).expect("rmp 应能解码 mp 枚举字节");
        assert_eq!(from_mp, value);

        let from_mp_named: ClassicEnum =
            rmp_serde::from_slice(&mp_named).expect("rmp 应能解码 mp 命名枚举字节");
        assert_eq!(from_mp_named, value);

        let from_rmp_named: ClassicEnum = mp_decode_(&rmp_named);
        assert_eq!(from_rmp_named, value);

        let from_rmp_compact: ClassicEnum = mp_decode_(&rmp_compact);
        assert_eq!(from_rmp_compact, value);
    }
}
