//! # mp_denco
//!
//! 基于 [`abs_denco`] 的最小 MessagePack 格式实现，直接面向
//! [`abs_buff`] 的 `TrBuffWrite` / `TrBuffRead` 工作。
//!
//! ## 读写模型
//!
//! - [`MpEncoder`] 借用任意 `TrBuffWrite<u8>`，每个头部与标量先在栈上编码，
//!   再通过 `write_async` 借出的写段写入；不把整段输出预攒进 `Vec`。
//! - [`MpDecoder`] 借用任意 `TrBuffRead<u8>`，按需通过 `read_async` 读取标签、
//!   长度与负载；不再预扫描整个字节切片。字符串与二进制一律解码为 owned
//!   结果：短值用栈数组，长值边读边构造 `String` / `Vec<u8>`。
//!
//! ## 当前覆盖的子集
//!
//! - 编码：nil、布尔、整数（含 i128/u128 的 16 字节大端 binary）、浮点数、
//!   字符串、二进制、序列/元组、映射/结构体、newtype 结构体、单元结构体、
//!   四种枚举变体；
//! - 解码：nil、布尔、整数、浮点数、字符串、二进制、数组、映射、`Option`、
//!   单元/newtype 结构体、四种枚举变体；
//! - 结构体默认编码为位置数组，不写字段名，与 `rmp_serde::to_vec` 一致；
//!   用 [`MpEncoder::with_struct_map`] 或 [`MpEncoder::with_config`] 配合
//!   [`StructMapConfig`] 可改为命名映射，与 `rmp_serde::to_vec_named` 一致；
//!   解码同时接受位置数组与命名映射；
//! - 取消语义：入口处的预取消检查，以及 `gen_mcf2` helper 路径上的取消传播。
//!
//! 具体格式约定与 `rmp-serde` 的对照见 `mp_denco/tests/interop_.rs`。

#![no_std]
#![feature(impl_trait_in_assoc_type)]

extern crate alloc;

mod config_;
mod de_;
mod error_;
mod ser_;

pub use config_::{DefaultConfig, SerializerConfig, StructMapConfig, StructTupleConfig};
pub use de_::MpDecoder;
pub use error_::MpError;
pub use ser_::{CancellableReady, MpEncoder, MpMapEncoder, MpSeqEncoder};
