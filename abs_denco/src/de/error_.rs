//! 反序列化方向的错误抽象、`Unexpected` 与 `Expected`。

use core::{
    error::Error as CoreError,
    fmt::{self, Display},
};

use super::Visitor;

/// 反序列化过程中由数据格式定义的错误。
///
/// 与同步版 `serde::de::Error` 一样，数据格式通常只需要实现
/// [`Error::custom`]，其余方法都有可用的默认实现。错误构造是同步的，
/// 不需要取消令牌。
pub trait Error: CoreError + Sized {
    /// 用一条自定义消息构造错误。
    ///
    /// # Errors
    ///
    /// 本方法总是返回一个错误值。
    fn custom<T>(msg: T) -> Self
    where
        T: Display;

    /// 构造「输入类型与期望类型不符」的错误。
    ///
    /// # Errors
    ///
    /// 本方法总是返回一个错误值。
    #[cold]
    fn invalid_type(unexp: Unexpected<'_>, exp: &dyn Expected) -> Self {
        Self::custom(format_args!("invalid type: {}, expected {}", unexp, exp))
    }

    /// 构造「输入值不合法」的错误。
    ///
    /// # Errors
    ///
    /// 本方法总是返回一个错误值。
    #[cold]
    fn invalid_value(unexp: Unexpected<'_>, exp: &dyn Expected) -> Self {
        Self::custom(format_args!("invalid value: {}, expected {}", unexp, exp))
    }

    /// 构造「序列或映射长度不符」的错误。
    ///
    /// # Errors
    ///
    /// 本方法总是返回一个错误值。
    #[cold]
    fn invalid_length(len: usize, exp: &dyn Expected) -> Self {
        Self::custom(format_args!("invalid length {}, expected {}", len, exp))
    }

    /// 构造「未知枚举变体」的错误。
    ///
    /// # Errors
    ///
    /// 本方法总是返回一个错误值。
    #[cold]
    fn unknown_variant(variant: &str, expected: &'static [&'static str]) -> Self {
        if expected.is_empty() {
            Self::custom(format_args!(
                "unknown variant `{}`, there are no variants",
                variant
            ))
        } else {
            Self::custom(format_args!(
                "unknown variant `{}`, expected {}",
                variant,
                OneOf { names: expected }
            ))
        }
    }

    /// 构造「未知结构体字段」的错误。
    ///
    /// # Errors
    ///
    /// 本方法总是返回一个错误值。
    #[cold]
    fn unknown_field(field: &str, expected: &'static [&'static str]) -> Self {
        if expected.is_empty() {
            Self::custom(format_args!(
                "unknown field `{}`, there are no fields",
                field
            ))
        } else {
            Self::custom(format_args!(
                "unknown field `{}`, expected {}",
                field,
                OneOf { names: expected }
            ))
        }
    }

    /// 构造「缺少必需字段」的错误。
    ///
    /// # Errors
    ///
    /// 本方法总是返回一个错误值。
    #[cold]
    fn missing_field(field: &'static str) -> Self {
        Self::custom(format_args!("missing field `{}`", field))
    }

    /// 构造「字段重复出现」的错误。
    ///
    /// # Errors
    ///
    /// 本方法总是返回一个错误值。
    #[cold]
    fn duplicate_field(field: &'static str) -> Self {
        Self::custom(format_args!("duplicate field `{}`", field))
    }
}

/// 描述「输入中出现了什么类型」的辅助枚举。
///
/// 它被 [`Error::invalid_type`] 等错误构造方法用作参数，以便格式生成
/// 可读的错误消息。
#[derive(Copy, Clone, PartialEq, Debug)]
pub enum Unexpected<'a> {
    /// 输入中出现了一个未被预期的布尔值。
    Bool(bool),
    /// 输入中出现了一个未被预期的无符号整数。
    Unsigned(u64),
    /// 输入中出现了一个未被预期的有符号整数。
    Signed(i64),
    /// 输入中出现了一个未被预期的浮点数。
    Float(f64),
    /// 输入中出现了一个未被预期的字符。
    Char(char),
    /// 输入中出现了一个未被预期的字符串。
    Str(&'a str),
    /// 输入中出现了一段未被预期的字节。
    Bytes(&'a [u8]),
    /// 输入中出现了一个未被预期的 `()`。
    Unit,
    /// 输入中出现了一个未被预期的 `Option`。
    Option,
    /// 输入中出现了一个未被预期的 newtype 结构体。
    NewtypeStruct,
    /// 输入中出现了一个未被预期的序列。
    Seq,
    /// 输入中出现了一个未被预期的映射。
    Map,
    /// 输入中出现了一个未被预期的枚举。
    Enum,
    /// 输入中出现了一个未被预期的单元变体。
    UnitVariant,
    /// 输入中出现了一个未被预期的 newtype 变体。
    NewtypeVariant,
    /// 输入中出现了一个未被预期的元组变体。
    TupleVariant,
    /// 输入中出现了一个未被预期的结构体变体。
    StructVariant,
    /// 输入中出现了其他未被预期的内容。
    Other(&'a str),
}

impl fmt::Display for Unexpected<'_> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match *self {
            Unexpected::Bool(b) => write!(formatter, "boolean `{}`", b),
            Unexpected::Unsigned(i) => write!(formatter, "integer `{}`", i),
            Unexpected::Signed(i) => write!(formatter, "integer `{}`", i),
            Unexpected::Float(f) => write!(formatter, "floating point `{}`", f),
            Unexpected::Char(c) => write!(formatter, "character `{}`", c),
            Unexpected::Str(s) => write!(formatter, "string {:?}", s),
            Unexpected::Bytes(_) => formatter.write_str("byte array"),
            Unexpected::Unit => formatter.write_str("unit value"),
            Unexpected::Option => formatter.write_str("Option value"),
            Unexpected::NewtypeStruct => formatter.write_str("newtype struct"),
            Unexpected::Seq => formatter.write_str("sequence"),
            Unexpected::Map => formatter.write_str("map"),
            Unexpected::Enum => formatter.write_str("enum"),
            Unexpected::UnitVariant => formatter.write_str("unit variant"),
            Unexpected::NewtypeVariant => formatter.write_str("newtype variant"),
            Unexpected::TupleVariant => formatter.write_str("tuple variant"),
            Unexpected::StructVariant => formatter.write_str("struct variant"),
            Unexpected::Other(other) => formatter.write_str(other),
        }
    }
}

/// 描述「调用者期望收到什么」的抽象。
///
/// 它通常由 [`Visitor`] 自身实现：`&visitor` 可以直接作为
/// [`Error::invalid_type`] 的 `exp` 参数。
pub trait Expected {
    /// 把「期望什么」写入 `formatter`。
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result;
}

impl<'de, T> Expected for T
where
    T: Visitor<'de>,
{
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.expecting(formatter)
    }
}

impl Expected for &str {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self)
    }
}

impl Display for dyn Expected + '_ {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        Expected::fmt(self, formatter)
    }
}

/// 把一组候选名字格式化成 `` `a`、`b` 或 `c` `` 的形式。
struct OneOf {
    names: &'static [&'static str],
}

impl Display for OneOf {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.names.len() {
            // 空切片由调用方单独处理，不会走到这里。
            0 => unreachable!("OneOf 不用于空名字列表"),
            1 => write!(formatter, "`{}`", self.names[0]),
            2 => write!(formatter, "`{}` or `{}`", self.names[0], self.names[1]),
            _ => {
                formatter.write_str("one of ")?;
                for (i, alt) in self.names.iter().enumerate() {
                    if i > 0 {
                        formatter.write_str(", ")?;
                    }
                    write!(formatter, "`{}`", alt)?;
                }
                Ok(())
            }
        }
    }
}
