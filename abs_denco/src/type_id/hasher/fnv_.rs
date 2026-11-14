//! FNV-1a 32 / 64 / 128 的 const 流式实现。
//!
//! 常量取自 IETF FNV 草案 §5；三个宽度使用各自的原生状态和常量，不做截断。

use super::{KIND_ID, KIND_STR, KIND_U8, KIND_U32, KIND_U64};

/// 生成一个原生宽度的 FNV-1a 流式哈希器。
macro_rules! impl_fnv_hasher_ {
    ($name:ident, $int:ty, $prefix:expr, $offset:expr, $prime:expr) => {
        /// 隐藏的原生宽度 FNV-1a 流式哈希器。
        pub struct $name {
            hash_: $int,
        }

        impl $name {
            /// 创建哈希器并写入版本/宽度前缀；`segment_len` 仅用于统一接口。
            pub const fn new(_segment_len: usize) -> Self {
                let hasher = Self { hash_: $offset };
                hasher.bytes($prefix)
            }

            /// 写入一个 schema 标签/方向位/变体形状字节。
            pub const fn u8(self, value: u8) -> Self {
                self.bytes(&[KIND_U8, value])
            }

            /// 写入一个 `u32`（字段数、变体数、字符串长度之外的计数）。
            pub const fn u32(self, value: u32) -> Self {
                let bytes = value.to_le_bytes();
                self.bytes(&[KIND_U32]).bytes(&bytes)
            }

            /// 写入一个 `u64`（const 泛型实参）。
            pub const fn u64(self, value: u64) -> Self {
                let bytes = value.to_le_bytes();
                self.bytes(&[KIND_U64]).bytes(&bytes)
            }

            /// 写入一个长度前缀字符串。
            pub const fn str(self, value: &str) -> Self {
                let len = value.len() as u32;
                let len_bytes = len.to_le_bytes();
                self.bytes(&[KIND_STR])
                    .bytes(&len_bytes)
                    .bytes(value.as_bytes())
            }

            /// 写入一个字段类型的同宽度 ID。
            pub const fn id(self, value: $int) -> Self {
                let bytes = value.to_le_bytes();
                self.bytes(&[KIND_ID]).bytes(&bytes)
            }

            /// 结束并返回非零 ID。
            pub const fn finish(self) -> $int {
                if self.hash_ == 0 { 1 } else { self.hash_ }
            }

            /// 逐字节写入原始输入。
            const fn bytes(mut self, bytes: &[u8]) -> Self {
                let mut index = 0;
                while index < bytes.len() {
                    self.hash_ ^= bytes[index] as $int;
                    self.hash_ = self.hash_.wrapping_mul($prime);
                    index += 1;
                }
                self
            }
        }

        impl Default for $name {
            fn default() -> Self {
                Self::new(0)
            }
        }
    };
}

impl_fnv_hasher_!(
    Hasher32,
    u32,
    b"denco/type-id/1/fnv1a/u32\0",
    0x811c_9dc5,
    0x0100_0193
);
impl_fnv_hasher_!(
    Hasher64,
    u64,
    b"denco/type-id/1/fnv1a/u64\0",
    0xcbf2_9ce4_8422_2325,
    0x0000_0100_0000_01b3
);
impl_fnv_hasher_!(
    Hasher128,
    u128,
    b"denco/type-id/1/fnv1a/u128\0",
    0x6c62_272e_07bb_0142_62b8_2175_6295_c58d,
    0x0000_0000_0100_0000_0000_0000_0000_013b
);
