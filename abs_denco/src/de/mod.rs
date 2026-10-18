//! 解码方向的 trait。
//!
//! 本模块对应同步版 `serde::de`，但把所有执行异步工作的操作改写为返回
//! [`abs_cancel::TrMayCancel`] 实现的异步方法。

mod access_;
#[cfg(feature = "alloc-collections")]
mod alloc_collections_;
mod boxed_;
mod collect_;
#[cfg(feature = "cow")]
mod cow_;
mod decodable_;
mod decoder_;
mod error_;
mod ext_;
mod forward_;
mod ignored_any_;
mod impls_;
#[cfg(feature = "rc")]
mod rc_;
mod sink_;
#[cfg(feature = "std")]
mod std_collections_;
mod visitor_;

pub use access_::{EnumAccess, MapAccess, SeqAccess, VariantAccess};
pub use boxed_::BoxedMayCancel;
pub use decodable_::{TrDecodable, TrDecodableOwned, TrDecodableSeed};
pub use decoder_::TrDecoder;
pub use error_::{Error, Expected, Unexpected};
pub use ext_::{EnumAccessExt, MapAccessExt, SeqAccessExt, VariantAccessExt};
pub use ignored_any_::IgnoredAny;
pub use visitor_::Visitor;
