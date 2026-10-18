//! 编码方向的 trait。
//!
//! 本模块对应同步版 `serde::ser`，但把所有执行异步工作的操作改写为返回
//! [`abs_cancel::TrMayCancel`] 实现的异步方法。

#[cfg(feature = "alloc-collections")]
mod alloc_collections_;
#[cfg(feature = "cow")]
mod cow_;
mod encodable_;
mod encoder_;
mod error_;
mod ext_;
mod impls_;
mod impossible_;
#[cfg(feature = "rc")]
mod rc_;
#[cfg(feature = "std")]
mod std_collections_;

pub use encodable_::TrEncodable;
pub use encoder_::{
    TrEncodeMap, TrEncodeSeq, TrEncodeStruct, TrEncodeStructVariant, TrEncodeTuple,
    TrEncodeTupleStruct, TrEncodeTupleVariant, TrEncoder,
};
pub use error_::Error;
pub use ext_::{TrEncodeMapExt, TrEncoderExt};
pub use impossible_::Impossible;
