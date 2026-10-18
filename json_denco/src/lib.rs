//! # json_denco
//!
//! 基于 [`abs_denco`] 的流式 JSON 格式实现，直接面向 [`abs_buff`] 的
//! `TrBuffWrite` / `TrBuffRead` 工作。
//!
//! ## 读写模型
//!
//! - [`JsonEncoder`] 借用任意 `TrBuffWrite<u8>`，标点、数字与转义后的字符串
//!   分片都通过 `write_async` 借出的写段写入；不把整段输出预攒进 `Vec<u8>`。
//! - [`JsonDecoder`] 借用任意 `TrBuffRead<u8>`，通过 `read_async` 按需借段
//!   读取。解码器自身**不做预读缓冲**，只保留一个回推字节；长字符串等批量
//!   读取的效率由 `TrBuffRead` 的提供者负责。
//! - 字符串、数字与字节数组一律解码为 owned 结果，不借用读端内存。
//!
//! 连续编码多个顶层值时，调用方应在值之间写入空白（尤其是数字，否则相邻数字
//! 会被当成一个数）。一个完整顶层值结束后，解码器最多只可能留下空白字节；
//! 因此既可以复用同一个 [`JsonDecoder`]，也可以在每个值后新建解码器。
//!
//! ## 当前覆盖的子集
//!
//! - 编码：`null`、布尔、整数（含 i128/u128）、有限浮点数、字符串、字节数组、
//!   序列/元组、映射/结构体、newtype 结构体、单元结构体、四种枚举变体；
//! - 解码：`null`、布尔、整数、浮点数、字符串（含 `\uXXXX` 与代理对）、
//!   字节数组、数组、对象、`Option`、单元/newtype 结构体、四种枚举变体；
//! - 外部标签枚举与 `serde_json` 的默认表示一致；
//! - 映射键按 JSON 规则写成字符串，解码时可按字符串、整数、布尔、浮点或
//!   单元枚举变体解释；浮点与复合值不能作为编码键。
//!
//! ## 与 `serde_json` 的差异
//!
//! - 浮点数只接受有限值；`NaN` 与无穷大在编码时返回错误。
//! - 编码器会为整数值的浮点数补 `.0`，保证解码时仍按浮点路径处理。
//! - 不提供 `serde_json` 的任意精度数字与原始 JSON 片段能力。
//!
//! ## 用法
//!
//! ```
//! use abs_cancel::{NonCancellableToken, TrMayCancel};
//! use abs_denco::ser::TrEncodable;
//! use json_denco::JsonEncoder;
//!
//! # async fn demo() -> Result<(), json_denco::JsonError> {
//! let mut storage = [0u8; 32];
//! let mut sink: &mut [u8] = &mut storage;
//! 7u64.encode_async(JsonEncoder::new(&mut sink))
//!     .may_cancel_with(NonCancellableToken::new())
//!     .await?;
//! # Ok(())
//! # }
//! ```

#![no_std]
#![feature(impl_trait_in_assoc_type)]

extern crate alloc;

mod de_;
mod error_;
mod ser_;

pub use de_::JsonDecoder;
pub use error_::JsonError;
pub use ser_::{JsonEncoder, JsonMapEncoder, JsonSeqEncoder};
