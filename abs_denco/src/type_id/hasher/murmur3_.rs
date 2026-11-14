//! Murmur 家族的 const 流式实现。
//!
//! - 32 位：MurmurHash3 x86_32；
//! - 64 位：MurmurHash64A（MurmurHash2 64 位；Murmur3 没有原生 64 位输出）；
//! - 128 位：MurmurHash3 x64_128。
//!
//! 所有多字节读取固定为小端，保证跨目标端稳定。

use super::{KIND_ID, KIND_STR, KIND_U8, KIND_U32, KIND_U64};

const PREFIX32: &[u8] = b"denco/type-id/1/murmur/u32\0";
const PREFIX64: &[u8] = b"denco/type-id/1/murmur/u64\0";
const PREFIX128: &[u8] = b"denco/type-id/1/murmur/u128\0";

//-- ---- 32 位：MurmurHash3 x86_32 -------------------------------------------

const C1_32: u32 = 0xcc9e_2d51;
const C2_32: u32 = 0x1b87_3593;

/// 隐藏的原生 32 位 Murmur3 流式哈希器。
pub struct Hasher32 {
    hash_: u32,
    buf_: [u8; 4],
    len_: usize,
    total_: usize,
}

impl Hasher32 {
    /// 创建哈希器并写入版本/宽度前缀；`segment_len` 仅用于统一接口。
    pub const fn new(_segment_len: usize) -> Self {
        let hasher = Hasher32 {
            hash_: 0,
            buf_: [0u8; 4],
            len_: 0,
            total_: 0,
        };
        hasher.bytes(PREFIX32)
    }

    /// 写入一个 schema 标签/方向位/变体形状字节。
    pub const fn u8(self, value: u8) -> Self {
        self.bytes(&[KIND_U8, value])
    }

    /// 写入一个 `u32`。
    pub const fn u32(self, value: u32) -> Self {
        let bytes = value.to_le_bytes();
        self.bytes(&[KIND_U32]).bytes(&bytes)
    }

    /// 写入一个 `u64`。
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

    /// 写入一个字段类型的 32 位 ID。
    pub const fn id(self, value: u32) -> Self {
        let bytes = value.to_le_bytes();
        self.bytes(&[KIND_ID]).bytes(&bytes)
    }

    /// 结束并返回非零 32 位 ID。
    pub const fn finish(self) -> u32 {
        let mut hash = self.hash_;
        if self.len_ > 0 {
            let mut k = 0u32;
            let mut index = 0;
            while index < self.len_ {
                k |= (self.buf_[index] as u32) << (8 * index);
                index += 1;
            }
            k = k.wrapping_mul(C1_32);
            k = k.rotate_left(15);
            k = k.wrapping_mul(C2_32);
            hash ^= k;
        }
        hash ^= self.total_ as u32;
        let mixed = fmix32_(hash);
        if mixed == 0 { 1 } else { mixed }
    }

    /// 按 4 字节缓冲写入原始输入。
    const fn bytes(mut self, bytes: &[u8]) -> Self {
        self.total_ += bytes.len();
        let mut index = 0;
        while index < bytes.len() {
            self.buf_[self.len_] = bytes[index];
            self.len_ += 1;
            if self.len_ == 4 {
                let block = [self.buf_[0], self.buf_[1], self.buf_[2], self.buf_[3]];
                self.hash_ = mix_block32_(self.hash_, &block);
                self.len_ = 0;
            }
            index += 1;
        }
        self
    }
}

impl Default for Hasher32 {
    fn default() -> Self {
        Self::new(0)
    }
}

/// 处理一个完整的 4 字节块。
const fn mix_block32_(mut hash: u32, block: &[u8; 4]) -> u32 {
    let mut k = read_u32_le_(block);
    k = k.wrapping_mul(C1_32);
    k = k.rotate_left(15);
    k = k.wrapping_mul(C2_32);
    hash ^= k;
    hash = hash.rotate_left(13);
    hash.wrapping_mul(5).wrapping_add(0xe654_6b64)
}

/// MurmurHash3 的 32 位 finalizer。
const fn fmix32_(mut value: u32) -> u32 {
    value ^= value >> 16;
    value = value.wrapping_mul(0x85eb_ca6b);
    value ^= value >> 13;
    value = value.wrapping_mul(0xc2b2_ae35);
    value ^= value >> 16;
    value
}

//-- ---- 64 位：MurmurHash64A ------------------------------------------------

const M64: u64 = 0xc6a4_a793_5bd1_e995;
const R64: u32 = 47;

/// 隐藏的原生 64 位 MurmurHash64A 流式哈希器。
///
/// MurmurHash64A 需要预先知道总长度，因此 [`Hasher64::new`] 接收
/// **不含前缀**的 segment 字节数，内部再加上前缀长度作为总长度。
pub struct Hasher64 {
    hash_: u64,
    buf_: [u8; 8],
    len_: usize,
}

impl Hasher64 {
    /// 创建哈希器并写入版本/宽度前缀。
    pub const fn new(segment_len: usize) -> Self {
        let total = (PREFIX64.len() + segment_len) as u64;
        let hasher = Hasher64 {
            hash_: total.wrapping_mul(M64),
            buf_: [0u8; 8],
            len_: 0,
        };
        hasher.bytes(PREFIX64)
    }

    /// 写入一个 schema 标签/方向位/变体形状字节。
    pub const fn u8(self, value: u8) -> Self {
        self.bytes(&[KIND_U8, value])
    }

    /// 写入一个 `u32`。
    pub const fn u32(self, value: u32) -> Self {
        let bytes = value.to_le_bytes();
        self.bytes(&[KIND_U32]).bytes(&bytes)
    }

    /// 写入一个 `u64`。
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

    /// 写入一个字段类型的 64 位 ID。
    pub const fn id(self, value: u64) -> Self {
        let bytes = value.to_le_bytes();
        self.bytes(&[KIND_ID]).bytes(&bytes)
    }

    /// 结束并返回非零 64 位 ID。
    pub const fn finish(self) -> u64 {
        let mut hash = self.hash_;
        if self.len_ > 0 {
            let mut k = 0u64;
            let mut index = 0;
            while index < self.len_ {
                k |= (self.buf_[index] as u64) << (8 * index);
                index += 1;
            }
            hash ^= k;
            hash = hash.wrapping_mul(M64);
        }
        hash ^= hash >> R64;
        hash = hash.wrapping_mul(M64);
        hash ^= hash >> R64;
        if hash == 0 { 1 } else { hash }
    }

    /// 按 8 字节缓冲写入原始输入。
    const fn bytes(mut self, bytes: &[u8]) -> Self {
        let mut index = 0;
        while index < bytes.len() {
            self.buf_[self.len_] = bytes[index];
            self.len_ += 1;
            if self.len_ == 8 {
                let block = [
                    self.buf_[0],
                    self.buf_[1],
                    self.buf_[2],
                    self.buf_[3],
                    self.buf_[4],
                    self.buf_[5],
                    self.buf_[6],
                    self.buf_[7],
                ];
                let mut k = read_u64_le_(&block, 0);
                k = k.wrapping_mul(M64);
                k ^= k >> R64;
                k = k.wrapping_mul(M64);
                self.hash_ ^= k;
                self.hash_ = self.hash_.wrapping_mul(M64);
                self.len_ = 0;
            }
            index += 1;
        }
        self
    }
}

impl Default for Hasher64 {
    fn default() -> Self {
        Self::new(0)
    }
}

//-- ---- 128 位：MurmurHash3 x64_128 -----------------------------------------

const C1_128: u64 = 0x87c3_7b91_1142_53d5;
const C2_128: u64 = 0x4cf5_ad43_2745_937f;

/// 隐藏的原生 128 位 Murmur3 流式哈希器。
pub struct Hasher128 {
    h1_: u64,
    h2_: u64,
    buf_: [u8; 16],
    len_: usize,
    total_: usize,
}

impl Hasher128 {
    /// 创建哈希器并写入版本/宽度前缀；`segment_len` 仅用于统一接口。
    pub const fn new(_segment_len: usize) -> Self {
        let hasher = Hasher128 {
            h1_: 0,
            h2_: 0,
            buf_: [0u8; 16],
            len_: 0,
            total_: 0,
        };
        hasher.bytes(PREFIX128)
    }

    /// 写入一个 schema 标签/方向位/变体形状字节。
    pub const fn u8(self, value: u8) -> Self {
        self.bytes(&[KIND_U8, value])
    }

    /// 写入一个 `u32`。
    pub const fn u32(self, value: u32) -> Self {
        let bytes = value.to_le_bytes();
        self.bytes(&[KIND_U32]).bytes(&bytes)
    }

    /// 写入一个 `u64`。
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

    /// 写入一个字段类型的 128 位 ID。
    pub const fn id(self, value: u128) -> Self {
        let bytes = value.to_le_bytes();
        self.bytes(&[KIND_ID]).bytes(&bytes)
    }

    /// 结束并返回非零 128 位 ID。
    pub const fn finish(self) -> u128 {
        let mut h1 = self.h1_;
        let mut h2 = self.h2_;
        let remaining = self.len_;
        let total = self.total_ as u64;

        let mut k1 = 0u64;
        let mut k2 = 0u64;

        if remaining >= 9 {
            let mut index = 8;
            while index < remaining {
                k2 |= (self.buf_[index] as u64) << (8 * (index - 8));
                index += 1;
            }
            k2 = k2.wrapping_mul(C2_128);
            k2 = k2.rotate_left(33);
            k2 = k2.wrapping_mul(C1_128);
            h2 ^= k2;
        }

        if remaining >= 1 {
            let limit = if remaining < 8 { remaining } else { 8 };
            let mut index = 0;
            while index < limit {
                k1 |= (self.buf_[index] as u64) << (8 * index);
                index += 1;
            }
            k1 = k1.wrapping_mul(C1_128);
            k1 = k1.rotate_left(31);
            k1 = k1.wrapping_mul(C2_128);
            h1 ^= k1;
        }

        h1 ^= total;
        h2 ^= total;
        h1 = h1.wrapping_add(h2);
        h2 = h2.wrapping_add(h1);
        h1 = fmix64_(h1);
        h2 = fmix64_(h2);
        h1 = h1.wrapping_add(h2);
        h2 = h2.wrapping_add(h1);

        let value = (h1 as u128) | ((h2 as u128) << 64);
        if value == 0 { 1 } else { value }
    }

    /// 按 16 字节缓冲写入原始输入。
    const fn bytes(mut self, bytes: &[u8]) -> Self {
        self.total_ += bytes.len();
        let mut index = 0;
        while index < bytes.len() {
            self.buf_[self.len_] = bytes[index];
            self.len_ += 1;
            if self.len_ == 16 {
                let (h1, h2) = mix_block_128_(self.h1_, self.h2_, &self.buf_);
                self.h1_ = h1;
                self.h2_ = h2;
                self.len_ = 0;
            }
            index += 1;
        }
        self
    }
}

impl Default for Hasher128 {
    fn default() -> Self {
        Self::new(0)
    }
}

/// 处理一个完整的 16 字节块。
const fn mix_block_128_(mut h1: u64, mut h2: u64, block: &[u8; 16]) -> (u64, u64) {
    let mut k1 = read_u64_le_(block, 0);
    let mut k2 = read_u64_le_(block, 8);

    k1 = k1.wrapping_mul(C1_128);
    k1 = k1.rotate_left(31);
    k1 = k1.wrapping_mul(C2_128);
    h1 ^= k1;

    h1 = h1.rotate_left(27);
    h1 = h1.wrapping_add(h2);
    h1 = h1.wrapping_mul(5).wrapping_add(0x52dc_e729);

    k2 = k2.wrapping_mul(C2_128);
    k2 = k2.rotate_left(33);
    k2 = k2.wrapping_mul(C1_128);
    h2 ^= k2;

    h2 = h2.rotate_left(31);
    h2 = h2.wrapping_add(h1);
    h2 = h2.wrapping_mul(5).wrapping_add(0x3849_5ab5);

    (h1, h2)
}

/// 从字节切片读取一个小端 `u64`。
const fn read_u64_le_(bytes: &[u8], at: usize) -> u64 {
    let mut value = 0u64;
    let mut index = 0;
    while index < 8 {
        value |= (bytes[at + index] as u64) << (8 * index);
        index += 1;
    }
    value
}

/// 从字节切片读取一个小端 `u32`。
const fn read_u32_le_(bytes: &[u8]) -> u32 {
    let mut value = 0u32;
    let mut index = 0;
    while index < 4 {
        value |= (bytes[index] as u32) << (8 * index);
        index += 1;
    }
    value
}

/// MurmurHash3 的 64 位 finalizer。
const fn fmix64_(mut value: u64) -> u64 {
    value ^= value >> 33;
    value = value.wrapping_mul(0xff51_afd7_ed55_8ccd);
    value ^= value >> 33;
    value = value.wrapping_mul(0xc4ce_b9fe_1a85_ec53);
    value ^= value >> 33;
    value
}
