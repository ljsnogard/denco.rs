//! denco 的 derive 宏。
//!
//! 提供 [`TrEncodable`] 与 [`TrDecodable`] 两个派生宏，用法与 serde 的
//! `Serialize` / `Deserialize` 类似：
//!
//! ```ignore
//! use denco_macro::{TrDecodable, TrEncodable};
//!
//! #[derive(TrEncodable, TrDecodable)]
//! struct Pair {
//!     left: u64,
//!     right: String,
//! }
//! ```
//!
//! 支持的结构体形状：unit、newtype、tuple、named；支持的枚举变体形状：
//! unit、newtype、tuple、struct。命名字段解码时同时接受映射与位置数组。
//!
//! # 辅助属性
//!
//! - 类型级：`#[denco(rename = "...")]`、`#[denco(rename_all = "...")]`、
//!   `#[denco(transparent)]`；
//! - 字段级：`rename`、`alias`、`skip`、`skip_encoding` /
//!   `skip_serializing`、`skip_decoding` / `skip_deserializing`、`default`；
//! - 变体级：`rename`、`alias`。
//!
//! # 依赖
//!
//! 生成代码会引用 `abs_denco`、`abs_cancel` 与 `gen_mcf2`，使用者的 crate
//! 需要把这三者都作为直接依赖。完整说明、示例与限制见 crate 根目录的
//! `README.md`。
//!
//! # 生命周期参数
//!
//! 类型可以带生命周期参数（例如 `struct Foo<'alloc> { name: &'alloc str }`）。
//! 解码侧把用户的全部生命周期参数**绑定到 decoder 的 arena 寿命 `'alloc`**，
//! 生成 `impl<'de, 'alloc, ...> TrDecodable<..., __DencoAlloc> for Foo<'alloc>`；
//! 编码侧保留用户生命周期（`impl<'a> TrEncodable for Foo<'a>`）。
//!
//! 限制：
//!
//! - 多个生命周期参数都会绑定到同一个 `'alloc`；
//! - 含生命周期的递归类型（如 `Node<'alloc> { next: Option<Box<Node<'alloc>>> }`）
//!   可能因字段上的 `TrDecodable` 约束触发 trait 求解循环，此时建议手写实现；
//! - 借用输入缓冲的 `&'de str` 仍不支持，借用只针对 arena。
//!
//! # 已知限制
//!
//! 暂不支持 union、跳过元组字段、自定义 `default` 函数以及 `flatten` / `with`
//! 等 serde 高级属性。

use zyn::syn;

mod attrs_;
mod decode_;
mod encode_;
mod model_;
mod names_;

/// 为结构体或枚举派生 `abs_denco::ser::TrEncodable`。
///
/// 支持 unit / newtype / tuple / named 结构体，以及 unit / newtype /
/// tuple / struct 枚举变体。字段与变体可以通过 `#[denco(...)]` 重命名或跳过。
#[zyn::derive("TrEncodable", attributes(denco))]
fn tr_encodable(#[zyn(input)] input: zyn::Extract<syn::DeriveInput>) -> zyn::TokenStream {
    match model_::parse_(&input) {
        Ok(model) => encode_::expand_(&model),
        Err(err) => err.to_compile_error(),
    }
}

/// 为结构体或枚举派生 `abs_denco::de::TrDecodable`。
///
/// 解码同时接受命名字段映射与位置数组两种表示；未知字段会报错，缺失字段
/// 可通过 `Option<T>` 或 `#[denco(default)]` 补齐。
#[zyn::derive("TrDecodable", attributes(denco))]
fn tr_decodable(#[zyn(input)] input: zyn::Extract<syn::DeriveInput>) -> zyn::TokenStream {
    match model_::parse_(&input) {
        Ok(model) => decode_::expand_(&model),
        Err(err) => err.to_compile_error(),
    }
}
