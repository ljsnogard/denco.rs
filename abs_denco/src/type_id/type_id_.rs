//! `type-id` 的类型侧 trait、数值宽度 trait、注册表与手写路径入口。

use core::fmt;

use super::hasher::{Hasher32, Hasher64, Hasher128};

/// 当前 TypeId 输入语法与版本前缀的版本号。
///
/// 任何会改变 `TYPE_ID_128` 的语法/前缀变更都必须递增本常量，并更新 golden
/// 测试；这等价于一次线上不兼容变更。
pub const TYPE_ID_VERSION: u32 = 1;

/// 规范输入使用的 schema 标签、方向位与变体形状。
///
/// 这些值是 `dev-notes/2026-11-14-type-id-feature.md` §8.5 的字节级契约，供
/// `denco_macro` 生成的代码使用；不属于稳定 API。
#[doc(hidden)]
#[allow(missing_docs)]
pub mod tags {
    pub const UNIT: u8 = 0x00;
    pub const BOOL: u8 = 0x01;
    pub const I8: u8 = 0x02;
    pub const I16: u8 = 0x03;
    pub const I32: u8 = 0x04;
    pub const I64: u8 = 0x05;
    pub const I128: u8 = 0x06;
    pub const U8: u8 = 0x07;
    pub const U16: u8 = 0x08;
    pub const U32: u8 = 0x09;
    pub const U64: u8 = 0x0a;
    pub const U128: u8 = 0x0b;
    pub const F32: u8 = 0x0c;
    pub const F64: u8 = 0x0d;
    pub const CHAR: u8 = 0x0e;
    pub const STR: u8 = 0x10;
    pub const SEQ: u8 = 0x11;
    pub const ARRAY: u8 = 0x12;
    pub const OPTION: u8 = 0x13;
    pub const MAP: u8 = 0x14;
    pub const SET: u8 = 0x15;
    pub const STRUCT_NAMED: u8 = 0x20;
    pub const STRUCT_TUPLE: u8 = 0x21;
    pub const STRUCT_NEWTYPE: u8 = 0x22;
    pub const STRUCT_UNIT: u8 = 0x23;
    pub const ENUM: u8 = 0x24;
    pub const PATH: u8 = 0x40;

    /// 字段同时参与编码与解码。
    pub const DIR_BOTH: u8 = 0b11;
    /// 字段只参与编码（`skip_decoding`）。
    pub const DIR_ENCODE_ONLY: u8 = 0b01;
    /// 字段只参与解码（`skip_encoding`）。
    pub const DIR_DECODE_ONLY: u8 = 0b10;

    pub const VARIANT_UNIT: u8 = 0;
    pub const VARIANT_NEWTYPE: u8 = 1;
    pub const VARIANT_TUPLE: u8 = 2;
    pub const VARIANT_NAMED: u8 = 3;
}

/// TypeId 的数值宽度。
///
/// 本 trait 只由 `u32`、`u64`、`u128` 实现；它让消费方可以在泛型代码里表达
/// “某个宽度的 TypeId”，而 [`TrHasTypeId`] 负责给出类型的三种宽度值。
///
/// # Examples
///
/// ```
/// use abs_denco::type_id::TrTypeId;
///
/// assert_eq!(<u32 as TrTypeId>::BITS, 32);
/// assert_eq!(<u32 as TrTypeId>::from_u128(0x1_0000_0001), 1);
/// assert_eq!(<u128 as TrTypeId>::to_u128(7), 7);
/// ```
pub trait TrTypeId: Copy + Eq + Ord + core::hash::Hash + fmt::Debug + fmt::Display {
    /// 本宽度的位数：32、64 或 128。
    const BITS: u32;

    /// 把任意 128 位数值低位截断到本宽度；纯数值转换，不参与 TypeId 生成。
    fn from_u128(value: u128) -> Self;

    /// 零扩展到 128 位。
    fn to_u128(self) -> u128;
}

impl TrTypeId for u32 {
    const BITS: u32 = 32;

    fn from_u128(value: u128) -> Self {
        value as u32
    }

    fn to_u128(self) -> u128 {
        self as u128
    }
}

impl TrTypeId for u64 {
    const BITS: u32 = 64;

    fn from_u128(value: u128) -> Self {
        value as u64
    }

    fn to_u128(self) -> u128 {
        self as u128
    }
}

impl TrTypeId for u128 {
    const BITS: u32 = 128;

    fn from_u128(value: u128) -> Self {
        value
    }

    fn to_u128(self) -> u128 {
        self
    }
}

/// 拥有稳定线上类型指纹的类型。
///
/// 三种宽度同时提供，且**互相独立**：每个宽度使用自己的原生宽度哈希与
/// domain，不是 128 位值的截断。
///
/// `denco_macro/type-id` 开启时，`#[derive(TrHasTypeId)]` 会生成实现；手写
/// codec 类型、递归类型或需要固定线上 ID 的类型直接手写实现，递归可配合
/// [`path_id_32`] / [`path_id_64`] / [`path_id_128`]。
///
/// # Examples
///
/// ```
/// use abs_denco::type_id::TrHasTypeId;
///
/// struct Manual;
///
/// impl TrHasTypeId for Manual {
///     const TYPE_ID_128: u128 = 0x1234;
///     const TYPE_ID_64: u64 = 0x1234;
///     const TYPE_ID_32: u32 = 0x1234;
///     const TYPE_NAME: &'static str = "Manual";
/// }
///
/// assert_eq!(Manual::TYPE_ID_128, 0x1234);
/// assert_eq!(Manual::TYPE_ID_64, 0x1234);
/// assert_eq!(Manual::TYPE_ID_32, 0x1234);
/// ```
#[diagnostic::on_unimplemented(
    message = "类型 `{Self}` 没有 `TrHasTypeId` 实现",
    label = "需要同时开启 `abs_denco/type-id` 与 `denco_macro/type-id`",
    note = "手写/递归/固定 ID 请直接实现 `TrHasTypeId`；递归可用 `path_id_32` / `path_id_64` / `path_id_128`"
)]
pub trait TrHasTypeId {
    /// 原生 128 位类型指纹；必须非零。
    const TYPE_ID_128: u128;

    /// 原生 64 位类型指纹；必须非零。
    const TYPE_ID_64: u64;

    /// 原生 32 位类型指纹；必须非零。
    const TYPE_ID_32: u32;

    /// 便于注册表与错误信息的人类可读名字，通常是 `module_path!()::Type`。
    const TYPE_NAME: &'static str;
}

/// 用规范输入流计算一个手写/递归类型的路径级 32 位 ID。
///
/// 输入只包含 `module_path!()` 与 Rust 类型名，不含字段结构；因此它不会把
/// 嵌套结构变化传播到外层，但可以安全地打断递归。
///
/// # Examples
///
/// ```
/// use abs_denco::type_id::path_id_32;
///
/// let id = path_id_32("my_crate::model", "Node");
/// assert_ne!(id, 0);
/// assert_eq!(id, path_id_32("my_crate::model", "Node"));
/// assert_ne!(id, path_id_32("my_crate::model", "Other"));
/// ```
pub const fn path_id_32(module_path: &str, ident: &str) -> u32 {
    let segment_len = path_segment_len_(module_path, ident);
    let hasher = Hasher32::new(segment_len);
    let hasher = hasher.u8(tags::PATH);
    let hasher = hasher.str(module_path);
    let hasher = hasher.str(ident);
    hasher.finish()
}

/// 用规范输入流计算一个手写/递归类型的路径级 64 位 ID。
///
/// 输入只包含 `module_path!()` 与 Rust 类型名，不含字段结构；因此它不会把
/// 嵌套结构变化传播到外层，但可以安全地打断递归。
///
/// # Examples
///
/// ```
/// use abs_denco::type_id::path_id_64;
///
/// let id = path_id_64("my_crate::model", "Node");
/// assert_ne!(id, 0);
/// assert_eq!(id, path_id_64("my_crate::model", "Node"));
/// assert_ne!(id, path_id_64("my_crate::model", "Other"));
/// ```
pub const fn path_id_64(module_path: &str, ident: &str) -> u64 {
    let segment_len = path_segment_len_(module_path, ident);
    let hasher = Hasher64::new(segment_len);
    let hasher = hasher.u8(tags::PATH);
    let hasher = hasher.str(module_path);
    let hasher = hasher.str(ident);
    hasher.finish()
}

/// 用规范输入流计算一个手写/递归类型的路径级 128 位 ID。
///
/// 输入只包含 `module_path!()` 与 Rust 类型名，不含字段结构；因此它不会把
/// 嵌套结构变化传播到外层，但可以安全地打断递归。
///
/// # Examples
///
/// ```
/// use abs_denco::type_id::path_id_128;
///
/// let id = path_id_128("my_crate::model", "Node");
/// assert_ne!(id, 0);
/// assert_eq!(id, path_id_128("my_crate::model", "Node"));
/// assert_ne!(id, path_id_128("my_crate::model", "Other"));
/// ```
pub const fn path_id_128(module_path: &str, ident: &str) -> u128 {
    let segment_len = path_segment_len_(module_path, ident);
    let hasher = Hasher128::new(segment_len);
    let hasher = hasher.u8(tags::PATH);
    let hasher = hasher.str(module_path);
    let hasher = hasher.str(ident);
    hasher.finish()
}

/// 路径级 schema 的 segment 字节数（不含 hasher 内部前缀）。
const fn path_segment_len_(module_path: &str, ident: &str) -> usize {
    2 + (1 + 4 + module_path.len()) + (1 + 4 + ident.len())
}

/// 注册表条目：一个类型的三个宽度 ID 与可读名字。
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct TypeIdEntry {
    /// 128 位 ID。
    pub id_128: u128,
    /// 低 64 位 ID（非零）。
    pub id_64: u64,
    /// 低 32 位 ID（非零）。
    pub id_32: u32,
    /// 人类可读类型名。
    pub name: &'static str,
}

/// 注册表检测到的同宽 ID 冲突。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TypeIdConflict {
    /// 发生冲突的宽度：32、64 或 128。
    pub width: u32,
    /// 冲突条目中第一个类型的规范 128 位 ID。
    pub id_128: u128,
    /// 第一个类型名。
    pub first: &'static str,
    /// 第二个类型名。
    pub second: &'static str,
}

impl fmt::Display for TypeIdConflict {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "TypeId 冲突（{} 位）：0x{:032x} 同时属于 `{}` 与 `{}`",
            self.width, self.id_128, self.first, self.second
        )
    }
}

impl core::error::Error for TypeIdConflict {}

/// 按指定宽度检查注册表条目是否有重复 ID。
///
/// 复杂度是 O(n²)，不分配；适合测试/装配期的小规模清单。跨 crate 的完整清单
/// 需要各 crate 用 [`denco_type_ids`](crate::denco_type_ids) 导出后汇总。
///
/// # Panics
///
/// `width` 不是 32、64、128 时 panic。
///
/// # Examples
///
/// ```
/// use abs_denco::denco_type_ids;
/// use abs_denco::type_id::check_unique;
///
/// let entries = denco_type_ids![u8, u16];
/// assert!(check_unique(entries, 128).is_ok());
/// ```
pub fn check_unique(entries: &[TypeIdEntry], width: u32) -> Result<(), TypeIdConflict> {
    for (index, left) in entries.iter().enumerate() {
        for right in &entries[index + 1..] {
            let duplicate = match width {
                32 => left.id_32 == right.id_32,
                64 => left.id_64 == right.id_64,
                128 => left.id_128 == right.id_128,
                _ => panic!("TypeId 宽度必须是 32、64 或 128"),
            };
            if duplicate {
                return Err(TypeIdConflict {
                    width,
                    id_128: left.id_128,
                    first: left.name,
                    second: right.name,
                });
            }
        }
    }
    Ok(())
}

/// 把一组类型展开为 [`TypeIdEntry`] 切片。
///
/// 用法：
///
/// ```
/// use abs_denco::denco_type_ids;
///
/// let entries = denco_type_ids![u8, u16, String];
/// assert_eq!(entries.len(), 3);
/// assert_eq!(entries[2].name, "String");
/// ```
///
/// 每个 crate 可以导出一份清单，workspace 测试再汇总；本宏不做跨 crate 自动
/// 收集。
#[macro_export]
macro_rules! denco_type_ids {
    ($($ty:ty),* $(,)?) => {
        &[
            $(
                $crate::type_id::TypeIdEntry {
                    id_128: <$ty as $crate::type_id::TrHasTypeId>::TYPE_ID_128,
                    id_64: <$ty as $crate::type_id::TrHasTypeId>::TYPE_ID_64,
                    id_32: <$ty as $crate::type_id::TrHasTypeId>::TYPE_ID_32,
                    name: <$ty as $crate::type_id::TrHasTypeId>::TYPE_NAME,
                }
            ),*
        ]
    };
}
