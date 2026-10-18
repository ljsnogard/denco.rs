//! 序列化方向的错误抽象。

use core::{error::Error as CoreError, fmt::Display};

/// 序列化过程中由数据格式定义的错误。
///
/// 与同步版 `serde::ser::Error` 一样，本 trait 只要求数据格式提供
/// [`Error::custom`]；其余错误语义由格式自行解释。错误构造本身是同步的，
/// 不需要取消令牌，因此不由 [`abs_cancel::TrMayCancel`] 包装。
pub trait Error: CoreError + Sized {
    /// 用一条自定义消息构造错误。
    ///
    /// # Errors
    ///
    /// 本方法总是返回一个错误值；是否把它继续向上传播由调用者决定。
    fn custom<T>(msg: T) -> Self
    where
        T: Display;
}
