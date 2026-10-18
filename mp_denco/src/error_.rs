//! MessagePack 格式的错误类型。

use alloc::{
    format,
    string::{String, ToString},
};
use core::fmt;

/// 最小 MessagePack 格式使用的错误。
///
/// 错误消息使用 `String` 保存，便于把底层格式错误和上层 `custom` 消息
/// 统一呈现。
#[derive(Debug, PartialEq, Eq)]
pub struct MpError {
    msg_: String,
}

impl MpError {
    /// 用一条静态消息构造错误。
    pub fn new(msg: &str) -> Self {
        Self {
            msg_: msg.to_string(),
        }
    }

    /// 把底层缓冲写错误包装成格式错误。
    ///
    /// # Examples
    ///
    /// ```
    /// use mp_denco::MpError;
    ///
    /// let err = MpError::buff_write("closed");
    /// assert!(err.to_string().contains("closed"));
    /// ```
    pub fn buff_write<T>(err: T) -> Self
    where
        T: fmt::Display,
    {
        Self {
            msg_: format!("buff write failed: {err}"),
        }
    }

    /// 把底层缓冲读错误包装成格式错误。
    ///
    /// # Examples
    ///
    /// ```
    /// use mp_denco::MpError;
    ///
    /// let err = MpError::buff_read("drained");
    /// assert!(err.to_string().contains("drained"));
    /// ```
    pub fn buff_read<T>(err: T) -> Self
    where
        T: fmt::Display,
    {
        Self {
            msg_: format!("buff read failed: {err}"),
        }
    }
}

impl fmt::Display for MpError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.msg_)
    }
}

impl core::error::Error for MpError {}

impl abs_denco::ser::Error for MpError {
    fn custom<T>(msg: T) -> Self
    where
        T: fmt::Display,
    {
        Self {
            msg_: format!("{}", msg),
        }
    }
}

impl abs_denco::de::Error for MpError {
    fn custom<T>(msg: T) -> Self
    where
        T: fmt::Display,
    {
        Self {
            msg_: format!("{}", msg),
        }
    }
}
