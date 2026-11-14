//! `denco_macro` 的 `TrHasTypeId` derive 测试。
//!
//! 只在 `--features type-id`（并同时启用 `abs_denco/type-id` 的 dev-dependency）下编译。
//!
//! Tests for the explicit `#[derive(TrHasTypeId)]` implementation.

#![cfg(feature = "type-id")]
#![feature(impl_trait_in_assoc_type)]

use abs_denco::type_id::{HASH_ALGORITHM, TrHasTypeId, path_id_32, path_id_64, path_id_128};
use denco_macro::{TrDecodable, TrEncodable};

#[derive(TrEncodable, TrDecodable, denco_macro::TrHasTypeId)]
struct Plain {
    id: u64,
    name: String,
}

#[derive(TrEncodable, TrDecodable, denco_macro::TrHasTypeId)]
struct Both {
    value: u8,
}

#[derive(TrEncodable, TrDecodable, denco_macro::TrHasTypeId)]
struct Tuple(u8, u16);

#[derive(TrEncodable, TrDecodable, denco_macro::TrHasTypeId)]
enum Event {
    Ping,
    Text(String),
    Move { x: i32, y: i32 },
}

#[derive(TrEncodable, TrDecodable, denco_macro::TrHasTypeId)]
#[denco(transparent)]
struct Transparent(Vec<u8>);

#[derive(TrEncodable, TrDecodable, denco_macro::TrHasTypeId)]
struct Generic<T> {
    value: T,
}

#[derive(TrEncodable, TrDecodable, denco_macro::TrHasTypeId)]
struct ConstGeneric<const N: usize> {
    bytes: [u8; N],
}

#[derive(TrEncodable, TrDecodable, denco_macro::TrHasTypeId)]
#[denco(rename_all = "snake_case")]
struct Renamed {
    first_name: String,
    #[denco(rename = "x")]
    renamed: u8,
}

#[allow(dead_code)]
#[derive(TrEncodable, TrDecodable, denco_macro::TrHasTypeId)]
struct Skipped {
    a: u8,
    #[denco(skip)]
    b: u16,
    #[denco(skip_encoding)]
    c: u32,
    #[denco(skip_decoding)]
    d: u64,
    #[denco(default)]
    e: String,
    #[denco(alias = "f_alias")]
    f: bool,
}

/// 手写固定 ID 的类型：不派生 `TrHasTypeId`，直接实现 trait。
/// A hand-written fixed ID: no `TrHasTypeId` derive, the trait is implemented manually.
#[derive(TrEncodable, TrDecodable)]
struct Legacy {
    a: u8,
}

impl TrHasTypeId for Legacy {
    const TYPE_ID_128: u128 = 0x1001;
    const TYPE_ID_64: u64 = 0x1001;
    const TYPE_ID_32: u32 = 0x1001;
    const TYPE_NAME: &'static str = "Legacy";
}

/// 递归类型：不派生 `TrHasTypeId`，手写路径 ID。
/// A recursive type: no `TrHasTypeId` derive, manual path IDs.
#[derive(TrEncodable)]
struct Node {
    next: Option<Box<Node>>,
}

impl TrHasTypeId for Node {
    const TYPE_ID_128: u128 = path_id_128(module_path!(), "Node");
    const TYPE_ID_64: u64 = path_id_64(module_path!(), "Node");
    const TYPE_ID_32: u32 = path_id_32(module_path!(), "Node");
    const TYPE_NAME: &'static str = concat!(module_path!(), "::", "Node");
}

/// 引用递归类型的外层类型：验证手写路径 ID 可以被正常嵌套引用。
/// An outer type containing the recursive type: its manual path IDs nest normally.
#[derive(TrEncodable, denco_macro::TrHasTypeId)]
struct Holder {
    node: Node,
}

/// 测试 derive 生成的三个宽度都存在、非零且结构不同的类型互不相同。
/// Test that all three derived widths exist, are nonzero and differ across structures.
///
/// - 手段：读取多个派生类型的 128 位 ID，并检查三个宽度非零。
/// - 判定：全部非零；两两不相等；`TYPE_NAME` 带类型名。
/// - Means: read the 128-bit IDs of several derived types and check all widths.
/// - Verdict: all nonzero, pairwise distinct, and `TYPE_NAME` carries the type name.
#[test]
fn derived_ids_are_nonzero_and_distinct_() {
    let ids = [
        Plain::TYPE_ID_128,
        Both::TYPE_ID_128,
        Tuple::TYPE_ID_128,
        Event::TYPE_ID_128,
        Transparent::TYPE_ID_128,
        Generic::<u8>::TYPE_ID_128,
        ConstGeneric::<4>::TYPE_ID_128,
        Renamed::TYPE_ID_128,
        Skipped::TYPE_ID_128,
        Legacy::TYPE_ID_128,
        Node::TYPE_ID_128,
        Holder::TYPE_ID_128,
    ];
    for id in ids {
        assert_ne!(id, 0);
    }
    for (index, left) in ids.iter().enumerate() {
        for right in &ids[index + 1..] {
            assert_ne!(left, right);
        }
    }

    assert_ne!(Plain::TYPE_ID_64, 0);
    assert_ne!(Plain::TYPE_ID_32, 0);
    assert!(Plain::TYPE_NAME.ends_with("::Plain"));
    assert!(Node::TYPE_NAME.ends_with("::Node"));
}

/// 测试泛型与 const 泛型实参参与三个宽度的 ID。
/// Test that generic and const-generic arguments take part in all three widths.
///
/// - 手段：比较 `Generic<u8>` / `Generic<u16>` 与 `ConstGeneric<4>` / `ConstGeneric<5>`。
/// - 判定：不同类型实参或 const 实参得到不同的 128/64/32 位 ID。
/// - Means: compare `Generic<u8>` / `Generic<u16>` and `ConstGeneric<4>` / `ConstGeneric<5>`.
/// - Verdict: different type/const arguments produce different IDs at every width.
#[test]
fn generic_arguments_change_id_() {
    assert_ne!(Generic::<u8>::TYPE_ID_128, Generic::<u16>::TYPE_ID_128);
    assert_ne!(Generic::<u8>::TYPE_ID_64, Generic::<u16>::TYPE_ID_64);
    assert_ne!(Generic::<u8>::TYPE_ID_32, Generic::<u16>::TYPE_ID_32);
    assert_eq!(Generic::<u8>::TYPE_ID_128, Generic::<u8>::TYPE_ID_128);
    assert_ne!(
        ConstGeneric::<4>::TYPE_ID_128,
        ConstGeneric::<5>::TYPE_ID_128
    );
    assert_ne!(ConstGeneric::<4>::TYPE_ID_64, ConstGeneric::<5>::TYPE_ID_64);
    assert_ne!(ConstGeneric::<4>::TYPE_ID_32, ConstGeneric::<5>::TYPE_ID_32);
}

/// 测试 transparent、手写固定 ID 与手写路径 ID 的语义。
/// Test transparent, hand-written fixed IDs and manual path IDs.
///
/// - 手段：比较 `Transparent` 与 `Vec<u8>`；读取 `Legacy` 与 `Node` 的三个宽度。
/// - 判定：transparent 三宽度等于内层；`Legacy` 等于 `0x1001`；`Node` 等于手写路径 ID。
/// - Means: compare `Transparent` with `Vec<u8>`; read `Legacy` and `Node` at all widths.
/// - Verdict: transparent matches the inner type; `Legacy` is `0x1001`; `Node` matches paths.
#[test]
fn transparent_and_manual_ids_() {
    assert_eq!(
        Transparent::TYPE_ID_128,
        <Vec<u8> as TrHasTypeId>::TYPE_ID_128
    );
    assert_eq!(
        Transparent::TYPE_ID_64,
        <Vec<u8> as TrHasTypeId>::TYPE_ID_64
    );
    assert_eq!(
        Transparent::TYPE_ID_32,
        <Vec<u8> as TrHasTypeId>::TYPE_ID_32
    );

    assert_eq!(Legacy::TYPE_ID_128, 0x1001);
    assert_eq!(Legacy::TYPE_ID_64, 0x1001);
    assert_eq!(Legacy::TYPE_ID_32, 0x1001);

    assert_eq!(Node::TYPE_ID_128, path_id_128(module_path!(), "Node"));
    assert_eq!(Node::TYPE_ID_64, path_id_64(module_path!(), "Node"));
    assert_eq!(Node::TYPE_ID_32, path_id_32(module_path!(), "Node"));
    assert_ne!(Holder::TYPE_ID_128, 0);
    assert_ne!(Holder::TYPE_ID_128, Node::TYPE_ID_128);
}

/// 测试派生类型的三个宽度 golden：覆盖 schema 标签、路径、字段名与字段 ID。
/// Golden test for a derived type at all three widths.
///
/// - 手段：按 `HASH_ALGORITHM` 选择 `Plain` 的硬编码三宽度值。
/// - 判定：128/64/32 位都与独立参考实现算出的值一致。
/// - Means: pick hard-coded three-width values for `Plain` by `HASH_ALGORITHM`.
/// - Verdict: all three widths match the independent reference implementation.
#[test]
fn derived_golden_() {
    let (id_128, id_64, id_32) = match HASH_ALGORITHM {
        "fnv1a" => (
            0xdce9_3d27_c599_f6ef_3276_74fe_ced2_89f7,
            0x90af_b4d3_5c4b_0b8b,
            0xb6b3_7b41,
        ),
        "murmur" => (
            0x6290_0bc1_3bac_d0d7_b115_9cb1_96b6_70ce,
            0xdecb_6ffc_9e4e_36de,
            0xc945_1095,
        ),
        other => panic!("未知 HASH_ALGORITHM: {other}"),
    };
    assert_eq!(Plain::TYPE_ID_128, id_128);
    assert_eq!(Plain::TYPE_ID_64, id_64);
    assert_eq!(Plain::TYPE_ID_32, id_32);
}
