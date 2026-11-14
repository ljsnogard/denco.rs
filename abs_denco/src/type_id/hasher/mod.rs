//! `type-id` 的哈希实现选择。
//!
//! `denco_macro` 只调用隐藏的 `Hasher32` / `Hasher64` / `Hasher128`
//! 方法；具体算法完全由本 crate 的 feature 决定。多个 hash feature 同时开启
//! 时按固定优先级选择（Murmur 覆盖 FNV），以保证 `--all-features` 可编译；
//! 生产环境应只开启一个。
//!
//! 三个宽度使用**原生宽度哈希**，不是 128 位结果的截断：
//!
//! - FNV：FNV-1a 32 / 64 / 128；
//! - Murmur：MurmurHash3 x86_32 / MurmurHash64A / MurmurHash3 x64_128。

#[cfg(not(feature = "type-id-murmur3"))]
mod fnv_;
#[cfg(feature = "type-id-murmur3")]
mod murmur3_;

#[cfg(not(feature = "type-id-murmur3"))]
pub use fnv_::{Hasher32, Hasher64, Hasher128};
#[cfg(feature = "type-id-murmur3")]
pub use murmur3_::{Hasher32, Hasher64, Hasher128};

/// 当前编译选中的哈希家族名；它也是版本前缀的一部分。
#[cfg(feature = "type-id-murmur3")]
pub const HASH_ALGORITHM: &str = "murmur";

/// 当前编译选中的哈希家族名；它也是版本前缀的一部分。
#[cfg(not(feature = "type-id-murmur3"))]
pub const HASH_ALGORITHM: &str = "fnv1a";

/// 字符串片段的 framing 标签。
pub(crate) const KIND_STR: u8 = 0x01;

/// `u8` 片段的 framing 标签。
pub(crate) const KIND_U8: u8 = 0x02;

/// `u32` 片段的 framing 标签。
pub(crate) const KIND_U32: u8 = 0x03;

/// `u64` 片段的 framing 标签。
pub(crate) const KIND_U64: u8 = 0x04;

/// 字段类型 ID 的 framing 标签；具体字节数由宽度决定。
pub(crate) const KIND_ID: u8 = 0x05;
