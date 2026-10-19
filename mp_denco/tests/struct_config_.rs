//! 结构体序列化配置的集成测试。
//!
//! 默认配置把结构体写成位置数组（不写字段名），[`MpEncoder::with_struct_map`]
//! 改为命名映射；配置还会传播到嵌套在序列、映射、`Option` 与枚举变体中的
//! 结构体。本文件同时验证两种字节表示都能被解码。
//!
//! Integration tests for the struct serialization configuration: the default
//! representation is a positional array, `with_struct_map` switches to named
//! fields, and the configuration propagates through nested values.

#![feature(impl_trait_in_assoc_type)]

use core::{
    future::Future,
    task::{Context, Poll},
};
use std::{collections::BTreeMap, task::Waker};

use abs_denco::{de::TrDecodable, ser::TrEncodable};
use denco_macro::{TrDecodable, TrEncodable};
use mp_denco::{MpDecoder, MpEncoder};
use serde::{Deserialize, Serialize};

/// 立即完成的测试执行器。
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

/// 用默认配置（结构体为位置数组）编码一个值。
fn encode_<T>(value: &T) -> Vec<u8>
where
    T: ?Sized + TrEncodable,
{
    let mut storage = vec![0u8; 4096];
    let total = storage.len();
    let written = {
        let mut sink: &mut [u8] = storage.as_mut_slice();
        block_on_(async { value.encode_async(MpEncoder::new(&mut sink)).await })
            .expect("默认编码应当成功");
        total - sink.len()
    };
    storage.truncate(written);
    storage
}

/// 用命名映射配置编码一个值。
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
                .await
        })
        .expect("命名编码应当成功");
        total - sink.len()
    };
    storage.truncate(written);
    storage
}

/// 用默认解码器解码一个值。
fn decode_<'de, T>(bytes: &'de [u8]) -> T
where
    T: TrDecodable<'de>,
{
    let mut input: &'de [u8] = bytes;
    block_on_(async { T::decode_async(MpDecoder::new(&mut input)).await }).expect("解码应当成功")
}

//-- ---- 测试类型 -------------------------------------------------------------

/// 两字段的嵌套结构体。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TrEncodable, TrDecodable)]
struct Inner {
    id_: u64,
    label_: String,
}

/// 覆盖序列、映射与 `Option` 嵌套的结构体。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TrEncodable, TrDecodable)]
struct Outer {
    inner_: Inner,
    items_: Vec<Inner>,
    maybe_: Option<Inner>,
    table_: BTreeMap<String, Inner>,
}

/// 覆盖四种变体、且部分变体内部含结构体的枚举。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TrEncodable, TrDecodable)]
enum SampleEnum {
    Empty,
    Newtype(Inner),
    Tuple(u8, Inner),
    Struct { first_: Inner, count_: u32 },
}

/// 构造一个固定的两层嵌套值。
fn sample_outer_() -> Outer {
    Outer {
        inner_: Inner {
            id_: 7,
            label_: String::from("outer"),
        },
        items_: vec![
            Inner {
                id_: 8,
                label_: String::from("first"),
            },
            Inner {
                id_: 9,
                label_: String::from("second"),
            },
        ],
        maybe_: Some(Inner {
            id_: 10,
            label_: String::from("maybe"),
        }),
        table_: BTreeMap::from([(
            String::from("key"),
            Inner {
                id_: 11,
                label_: String::from("mapped"),
            },
        )]),
    }
}

//-- ---- 测试 ----------------------------------------------------------------

/// 验证默认配置把结构体编码为不写字段名的位置数组。
/// Verify the default configuration encodes structs as positional arrays.
///
/// - 测试目标 / Goal：`MpEncoder::new` 默认不写字段名，字节与
///   `rmp_serde::to_vec` 一致。
/// - 手段 / Method：编码 `Inner`，检查首个字节是两元素 fixarray 头，并用
///   `rmp_serde` 交叉解码。
/// - 判断 / Assert：首字节为 `0x92`；字节与 `to_vec` 完全相等；两条解码
///   路径都得到原值。
#[test]
fn default_struct_is_tuple_() {
    let value = Inner {
        id_: 300,
        label_: String::from("denco"),
    };

    let bytes = encode_(&value);
    assert_eq!(bytes[0], 0x92, "两字段结构体应使用 fixarray 头");
    let reference = rmp_serde::to_vec(&value).expect("rmp_serde compact 编码应当成功");
    assert_eq!(bytes, reference, "默认编码应与 rmp_serde::to_vec 字节一致");

    let from_mp: Inner = rmp_serde::from_slice(&bytes).expect("rmp 应能解码默认字节");
    assert_eq!(from_mp, value);

    let from_rmp: Inner = decode_(&reference);
    assert_eq!(from_rmp, value);
}

/// 验证 `with_struct_map` 把字段名一并写入字节流。
/// Verify `with_struct_map` writes field names into the byte stream.
///
/// - 测试目标 / Goal：命名映射配置与 `rmp_serde::to_vec_named` 字节一致。
/// - 手段 / Method：用 `with_struct_map` 编码 `Inner`，检查首个字节是两键
///   fixmap 头，并用 `rmp_serde` 交叉解码。
/// - 判断 / Assert：首字节为 `0x82`；字节与 `to_vec_named` 完全相等；两条
///   解码路径都得到原值。
#[test]
fn struct_map_config_writes_names_() {
    let value = Inner {
        id_: 300,
        label_: String::from("denco"),
    };

    let bytes = encode_named_(&value);
    assert_eq!(bytes[0], 0x82, "两字段结构体应使用 fixmap 头");
    let reference = rmp_serde::to_vec_named(&value).expect("rmp_serde named 编码应当成功");
    assert_eq!(
        bytes, reference,
        "命名编码应与 rmp_serde::to_vec_named 字节一致"
    );

    let from_mp: Inner = decode_(&bytes);
    assert_eq!(from_mp, value);

    let from_rmp: Inner = rmp_serde::from_slice(&reference).expect("rmp 应能解码命名字节");
    assert_eq!(from_rmp, value);
}

/// 验证配置传播到序列、映射与 `Option` 中的嵌套结构体。
/// Verify the configuration propagates into structs nested in sequences,
/// maps, and `Option`.
///
/// - 测试目标 / Goal：嵌套结构体也遵循顶层选择，不会退回默认表示。
/// - 手段 / Method：分别用默认配置与命名映射编码 `Outer`，与
///   `rmp_serde::to_vec` / `to_vec_named` 比较字节，并解码两种字节。
/// - 判断 / Assert：两种表示字节分别一致；两种字节都解回原值。
#[test]
fn struct_map_config_propagates_to_nested_() {
    let value = sample_outer_();

    let default_bytes = encode_(&value);
    let default_reference = rmp_serde::to_vec(&value).expect("rmp_serde compact 编码应当成功");
    assert_eq!(
        default_bytes, default_reference,
        "嵌套结构体的默认编码应与 to_vec 一致"
    );

    let named_bytes = encode_named_(&value);
    let named_reference = rmp_serde::to_vec_named(&value).expect("rmp_serde named 编码应当成功");
    assert_eq!(
        named_bytes, named_reference,
        "嵌套结构体的命名编码应与 to_vec_named 一致"
    );

    let from_default: Outer = decode_(&default_bytes);
    assert_eq!(from_default, value);

    let from_named: Outer = decode_(&named_bytes);
    assert_eq!(from_named, value);
}

/// 验证结构体枚举变体的内层字段也遵循配置。
/// Verify struct enum variants follow the configuration for their inner fields.
///
/// - 测试目标 / Goal：`newtype`、`tuple` 与 `struct` 变体中的 `Inner` 都随
///   顶层配置切换表示。
/// - 手段 / Method：对四种变体分别用两种配置编码，与 `rmp_serde` 的紧凑/
///   命名表示比较，并交叉解码。
/// - 判断 / Assert：两种表示字节分别一致；mp 与 rmp 的解码结果都等于原值。
#[test]
fn struct_variant_follows_config_() {
    let values = [
        SampleEnum::Empty,
        SampleEnum::Newtype(Inner {
            id_: 1,
            label_: String::from("newtype"),
        }),
        SampleEnum::Tuple(
            2,
            Inner {
                id_: 3,
                label_: String::from("tuple"),
            },
        ),
        SampleEnum::Struct {
            first_: Inner {
                id_: 4,
                label_: String::from("struct"),
            },
            count_: 5,
        },
    ];

    for value in values {
        let default_bytes = encode_(&value);
        let default_reference = rmp_serde::to_vec(&value).expect("rmp_serde compact 编码应当成功");
        assert_eq!(
            default_bytes, default_reference,
            "枚举变体的默认编码应与 to_vec 一致"
        );

        let named_bytes = encode_named_(&value);
        let named_reference =
            rmp_serde::to_vec_named(&value).expect("rmp_serde named 编码应当成功");
        assert_eq!(
            named_bytes, named_reference,
            "枚举变体的命名编码应与 to_vec_named 一致"
        );

        let from_mp_default: SampleEnum = decode_(&default_bytes);
        assert_eq!(from_mp_default, value);

        let from_mp_named: SampleEnum = decode_(&named_bytes);
        assert_eq!(from_mp_named, value);

        let from_rmp_default: SampleEnum =
            rmp_serde::from_slice(&default_bytes).expect("rmp 应能解码默认枚举字节");
        assert_eq!(from_rmp_default, value);

        let from_rmp_named: SampleEnum =
            rmp_serde::from_slice(&named_bytes).expect("rmp 应能解码命名枚举字节");
        assert_eq!(from_rmp_named, value);
    }
}

/// 验证解码器同时接受 rmp-serde 的紧凑与命名两种结构体表示。
/// Verify the decoder accepts both compact and named struct layouts.
///
/// - 测试目标 / Goal：解码不依赖编码时的配置选择。
/// - 手段 / Method：用 `rmp_serde::to_vec` 与 `to_vec_named` 分别编码
///   `Outer`，再全部交给 `MpDecoder`。
/// - 判断 / Assert：两条解码结果都等于原值。
#[test]
fn decoder_accepts_both_layouts_() {
    let value = sample_outer_();

    let compact = rmp_serde::to_vec(&value).expect("rmp_serde compact 编码应当成功");
    let named = rmp_serde::to_vec_named(&value).expect("rmp_serde named 编码应当成功");

    let from_compact: Outer = decode_(&compact);
    assert_eq!(from_compact, value, "紧凑表示的解码结果应等于原值");

    let from_named: Outer = decode_(&named);
    assert_eq!(from_named, value, "命名表示的解码结果应等于原值");
}
