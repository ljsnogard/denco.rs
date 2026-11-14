//! 可选的类型指纹（结构化 TypeId）。
//!
//! 本模块只在 `type-id` feature 开启时存在；默认构建不引入任何相关代码。
//!
//! # 设计要点
//!
//! - 类型侧 trait 是 [`TrHasTypeId`]；ID 的数值类型由 [`TrTypeId`] 表达
//!   （`u128` / `u64` / `u32`）。
//! - 128 / 64 / 32 三个宽度各自使用原生宽度哈希，互不为截断；每个值都非零。
//! - 哈希输入由定义处 `module_path!()`、类型名、线上名与内部结构组成；具体
//!   语法与稳定性契约见 `dev-notes/2026-11-14-type-id-feature.md`。
//! - 哈希算法由 `abs_denco/type-id-murmur3` 选择；默认 FNV-1a 32/64/128，
//!   Murmur 时使用 x86_32 / MurmurHash64A / x64_128。
//!
//! # 与 `core::any::TypeId` 的区别
//!
//! 本模块的 ID 是**线上数据类型的稳定指纹**，不是 Rust 内存类型身份：
//! `#[denco(transparent)]`、`Box<T>`、`String` 与 `str` 等按线上等价共享同一个
//! ID；它也不能用于 `Any` 下转型。

#[allow(missing_docs)]
mod hasher;
mod impls_;
mod type_id_;

pub use hasher::HASH_ALGORITHM;
pub use type_id_::{
    TYPE_ID_VERSION, TrHasTypeId, TrTypeId, TypeIdConflict, TypeIdEntry, check_unique, path_id_32,
    path_id_64, path_id_128,
};

/// 仅供 derive 生成的代码使用；不属于稳定 API。
#[doc(hidden)]
#[allow(missing_docs)]
pub mod __private {
    pub use super::hasher::{Hasher32, Hasher64, Hasher128};
    pub use super::type_id_::tags;
}
