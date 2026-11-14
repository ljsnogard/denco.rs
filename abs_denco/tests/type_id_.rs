//! `type-id` feature 的公开语义测试。
//!
//! 只在 `--features type-id` 下编译；默认构建不包含 `type_id` 模块。
//!
//! Public type-id semantics, compiled only with the feature enabled.

#![cfg(feature = "type-id")]

use abs_denco::denco_type_ids;
use abs_denco::type_id::{
    HASH_ALGORITHM, TYPE_ID_VERSION, TrHasTypeId, TrTypeId, TypeIdConflict, TypeIdEntry,
    check_unique, path_id_32, path_id_64, path_id_128,
};

fn id_128_<T: TrHasTypeId + ?Sized>() -> u128 {
    T::TYPE_ID_128
}

fn assert_golden_<T: TrHasTypeId>(id_128: u128, id_64: u64, id_32: u32) {
    assert_eq!(T::TYPE_ID_128, id_128);
    assert_eq!(T::TYPE_ID_64, id_64);
    assert_eq!(T::TYPE_ID_32, id_32);
}

/// 测试三种宽度 trait 与非零约定。
/// Test the three width traits and the nonzero contract.
///
/// - 手段：读取 `BITS`、`from_u128` / `to_u128`，并检查 `u8` 的三个宽度。
/// - 判定：BITS 为 32/64/128；`u8` 的三个宽度值都非零且互不为对方的强制关系。
/// - Means: read `BITS` / conversions and inspect the three widths of `u8`.
/// - Verdict: BITS are 32/64/128; all three `u8` IDs are nonzero.
#[test]
fn widths_and_nonzero_() {
    assert_eq!(<u32 as TrTypeId>::BITS, 32);
    assert_eq!(<u64 as TrTypeId>::BITS, 64);
    assert_eq!(<u128 as TrTypeId>::BITS, 128);

    assert_eq!(<u64 as TrTypeId>::from_u128(u128::MAX), u64::MAX);
    assert_eq!(<u32 as TrTypeId>::from_u128(0x1_0000_0001), 1);
    assert_eq!(<u64 as TrTypeId>::to_u128(7), 7);

    assert_ne!(<u8 as TrHasTypeId>::TYPE_ID_128, 0);
    assert_ne!(<u8 as TrHasTypeId>::TYPE_ID_64, 0);
    assert_ne!(<u8 as TrHasTypeId>::TYPE_ID_32, 0);
}

/// 测试线上等价类型的三个宽度 ID 都相同、语义不同的类型 ID 不同。
/// Test that wire-equivalent types share all three IDs and distinct types do not.
///
/// - 手段：比较 `str`/`String`/`&str`、序列、指针包装、`Option`、数组的三个宽度。
/// - 判定：等价类的三个宽度都相等；`Option<T>`、定长数组、不同长度数组不同。
/// - Means: compare all three widths of strings, sequences, smart pointers, options and arrays.
/// - Verdict: equivalence classes match at every width; option/array/width variants differ.
#[test]
fn equivalence_classes_() {
    assert_eq!(id_128_::<str>(), id_128_::<String>());
    assert_eq!(
        <str as TrHasTypeId>::TYPE_ID_64,
        <String as TrHasTypeId>::TYPE_ID_64
    );
    assert_eq!(
        <str as TrHasTypeId>::TYPE_ID_32,
        <String as TrHasTypeId>::TYPE_ID_32
    );
    assert_eq!(id_128_::<&'static str>(), id_128_::<String>());

    assert_eq!(id_128_::<[u8]>(), id_128_::<Vec<u8>>());
    assert_eq!(
        <[u8] as TrHasTypeId>::TYPE_ID_64,
        <Vec<u8> as TrHasTypeId>::TYPE_ID_64
    );
    assert_eq!(
        <[u8] as TrHasTypeId>::TYPE_ID_32,
        <Vec<u8> as TrHasTypeId>::TYPE_ID_32
    );
    assert_eq!(id_128_::<&'static [u8]>(), id_128_::<Vec<u8>>());
    assert_eq!(id_128_::<Box<[u8]>>(), id_128_::<Vec<u8>>());

    assert_eq!(id_128_::<u8>(), id_128_::<Box<u8>>());
    assert_eq!(id_128_::<u8>(), id_128_::<std::sync::Arc<u8>>());

    assert_ne!(id_128_::<Option<u8>>(), id_128_::<u8>());
    assert_ne!(id_128_::<[u8; 4]>(), id_128_::<Vec<u8>>());
    assert_ne!(id_128_::<[u8; 4]>(), id_128_::<[u8; 5]>());
}

/// 测试算法稳定性 golden：固定类型的三个宽度值。
/// Golden test: the three width values of fixed types.
///
/// - 手段：按 `HASH_ALGORITHM` 选择硬编码的期望值（由独立参考实现算出）。
/// - 判定：`()`、`u8`、`String`、`Vec<u8>`、`Option<u64>`、`[u8; 4]` 的 128/64/32 全部相等。
/// - Means: pick hard-coded expectations by `HASH_ALGORITHM` (computed by an independent reference).
/// - Verdict: all three widths of the six fixed types match exactly.
#[test]
fn golden_ids_() {
    match HASH_ALGORITHM {
        "fnv1a" => {
            assert_golden_::<()>(
                0xd2ed_f9b4_08ca_4ac7_701d_ec81_a1ed_cba9,
                0x2758_3694_ecaa_020c,
                0x3a5c_9e61,
            );
            assert_golden_::<u8>(
                0xd2ed_f9b4_09ca_4ac7_701d_ec81_a1ed_cce4,
                0x2758_3594_ecaa_0059,
                0x3b5c_9ff4,
            );
            assert_golden_::<String>(
                0xd2ed_f9b4_18ca_4ac7_701d_ec81_a1ed_df59,
                0x2758_4694_ecaa_1d3c,
                0x2a5c_8531,
            );
            assert_golden_::<Vec<u8>>(
                0xdecf_2fc2_f8df_1f1a_e068_fe56_5a2e_76af,
                0x560f_8346_ffd2_ede7,
                0x7f63_0ecd,
            );
            assert_golden_::<Option<u64>>(
                0x0a73_45a3_92ce_7077_18ae_2493_ada7_94ed,
                0x0db9_cc0f_beba_5f68,
                0x0770_a7fa,
            );
            assert_golden_::<[u8; 4]>(
                0xbd4b_a4d8_da59_5f30_2c67_b585_1c27_7cbc,
                0xc7bc_ea20_05e3_a640,
                0xf50b_dc82,
            );
        }
        "murmur" => {
            assert_golden_::<()>(
                0xb90c_7616_a726_784b_7e67_f10c_b416_b076,
                0x04d2_5d4f_5e0a_89f3,
                0x5b34_5f1d,
            );
            assert_golden_::<u8>(
                0x9c79_e4e6_1bc7_fad2_46ab_5c21_47b2_1e57,
                0x45bb_41c8_c99f_ab6d,
                0x0662_58f4,
            );
            assert_golden_::<String>(
                0x554c_9536_204d_9e30_045a_1039_c838_65fa,
                0x9db1_a321_f357_e24f,
                0x2862_3597,
            );
            assert_golden_::<Vec<u8>>(
                0xea3d_5ad4_c6a9_1ec1_cbc9_ebfe_3277_56f8,
                0x1a9b_d39a_9e79_31e9,
                0x4eb5_3829,
            );
            assert_golden_::<Option<u64>>(
                0x3f5e_1f3e_770c_1499_62c3_2a64_5043_9eca,
                0x7ed7_0ef3_9eb1_0bbf,
                0x7f84_4a85,
            );
            assert_golden_::<[u8; 4]>(
                0x398a_9a7e_6db3_dbef_32f7_d5b5_b3e2_d335,
                0x6a05_3f6f_56f9_549e,
                0x994d_215d,
            );
        }
        other => panic!("未知 HASH_ALGORITHM: {other}"),
    }
}

/// 测试手写路径 ID 的稳定性与区分性。
/// Test the stability and distinctness of manual path IDs.
///
/// - 手段：用相同与不同的 (module_path, ident) 调三个 `path_id_*`。
/// - 判定：相同输入相等且非零；不同模块或类型名得到不同值。
/// - Means: call all three `path_id_*` with equal and differing (module_path, ident) pairs.
/// - Verdict: equal inputs match and are nonzero; different paths/names differ.
#[test]
fn path_ids_are_stable_() {
    let id_128 = path_id_128("crate::module", "Node");
    let id_64 = path_id_64("crate::module", "Node");
    let id_32 = path_id_32("crate::module", "Node");
    assert_eq!(id_128, path_id_128("crate::module", "Node"));
    assert_eq!(id_64, path_id_64("crate::module", "Node"));
    assert_eq!(id_32, path_id_32("crate::module", "Node"));
    assert_ne!(id_128, 0);
    assert_ne!(id_64, 0);
    assert_ne!(id_32, 0);
    assert_ne!(id_128, path_id_128("crate::other", "Node"));
    assert_ne!(id_64, path_id_64("crate::module", "Other"));
    assert_ne!(id_32, path_id_32("crate::other", "Other"));
}

/// 测试注册表按 128/64/32 宽度检测重复 ID。
/// Test that the registry detects duplicate IDs at 128/64/32 widths.
///
/// - 手段：用 `denco_type_ids!` 构造无重复与重复两组条目，分别调用 `check_unique`。
/// - 判定：无重复组返回 `Ok`；重复组返回 `Err`，`width` 与错误文本包含类型名。
/// - Means: build unique/duplicate entry lists with `denco_type_ids!` and call `check_unique`.
/// - Verdict: the unique list is `Ok`; the duplicate list errors with the width and type name.
#[test]
fn registry_checks_requested_width_() {
    fn assert_error_<E: std::error::Error>() {}
    assert_error_::<TypeIdConflict>();

    let entries: &[TypeIdEntry] = denco_type_ids![u8, u16, Vec<u8>];
    assert_eq!(entries.len(), 3);
    assert_eq!(check_unique(entries, 128), Ok(()));
    assert_eq!(check_unique(entries, 64), Ok(()));
    assert_eq!(check_unique(entries, 32), Ok(()));

    let duplicates: &[TypeIdEntry] = denco_type_ids![u8, u8];
    for width in [128, 64, 32] {
        let error = check_unique(duplicates, width).expect_err("重复 ID 必须报错");
        assert_eq!(error.width, width);
        assert!(format!("{error}").contains("u8"));
    }
}

/// 测试版本常量可用且取值固定。
/// Test that the version constants exist and are fixed.
///
/// - 手段：直接读取 `TYPE_ID_VERSION` 与 `HASH_ALGORITHM`。
/// - 判定：版本为 1，算法名属于当前支持的两个值。
/// - Means: read `TYPE_ID_VERSION` and `HASH_ALGORITHM`.
/// - Verdict: version is 1 and the algorithm is one of the two supported names.
#[test]
fn version_constants_() {
    assert_eq!(TYPE_ID_VERSION, 1);
    assert!(matches!(HASH_ALGORITHM, "fnv1a" | "murmur"));
}

/// 测试 `alloc::collections` 序列容器都归一到同一个 seq schema。
/// Test that `alloc::collections` sequence containers share the seq schema.
///
/// - 手段：在 `alloc-collections` feature 下比较 `Vec` / `VecDeque` / `LinkedList` / `BinaryHeap`。
/// - 判定：四者的 128 位 ID 全部相等。
/// - Means: compare `Vec` / `VecDeque` / `LinkedList` / `BinaryHeap` under the feature.
/// - Verdict: all four 128-bit IDs are equal.
#[cfg(feature = "alloc-collections")]
#[test]
fn sequence_containers_are_canonical_() {
    use std::collections::{BinaryHeap, LinkedList, VecDeque};

    let sequence = id_128_::<Vec<u8>>();
    assert_eq!(id_128_::<VecDeque<u8>>(), sequence);
    assert_eq!(id_128_::<LinkedList<u8>>(), sequence);
    assert_eq!(id_128_::<BinaryHeap<u8>>(), sequence);
}

/// 测试映射/集合容器按线上形态归一。
/// Test that map/set containers normalize to their wire shape.
///
/// - 手段：在 `std` feature 下比较 `BTreeMap` / `HashMap` 与 `BTreeSet` / `HashSet`。
/// - 判定：两种映射的 128 位 ID 相等，两种集合的 128 位 ID 相等。
/// - Means: compare BTree/Hash map and set variants under the `std` feature.
/// - Verdict: the two maps share an ID and the two sets share an ID.
#[cfg(feature = "std")]
#[test]
fn map_and_set_containers_are_canonical_() {
    use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};

    assert_eq!(
        id_128_::<BTreeMap<u8, u16>>(),
        id_128_::<HashMap<u8, u16>>()
    );
    assert_eq!(id_128_::<BTreeSet<u8>>(), id_128_::<HashSet<u8>>());
}

/// 测试 `Cow` 转发到内部借用类型的 schema。
/// Test that `Cow` forwards to its borrowed type's schema.
///
/// - 手段：在 `cow` feature 下比较 `Cow<str>` / `Cow<[u8]>` 与 `str` / `Vec<u8>`。
/// - 判定：128 位 ID 分别相等。
/// - Means: compare `Cow<str>` / `Cow<[u8]>` with `str` / `Vec<u8>` under the feature.
/// - Verdict: the 128-bit IDs match respectively.
#[cfg(feature = "cow")]
#[test]
fn cow_forwards_() {
    use std::borrow::Cow;

    assert_eq!(id_128_::<Cow<'static, str>>(), id_128_::<str>());
    assert_eq!(id_128_::<Cow<'static, [u8]>>(), id_128_::<Vec<u8>>());
}
