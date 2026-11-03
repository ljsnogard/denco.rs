//! 反序列化方向的错误抽象、`Unexpected` 与 `Expected`。

use core::{
    error::Error as CoreError,
    fmt::{self, Display},
};

use super::{TrArena, Visitor};

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
/// # 与 serde 的差异（手写 visitor 必读）
///
/// serde 的 `Visitor<'de>` 只有生命周期参数，所以 serde 能提供
/// `impl<V: Visitor<'de>> Expected for V`：手写 visitor 里把 `&self` 传给
/// [`Error::invalid_type`] / [`Error::invalid_value`] / [`Error::invalid_length`]
/// 就会自动转成 `&dyn Expected`。
///
/// denco 的 [`Visitor<'alloc, Alloc>`] 没有输入生命周期（decoder 不再借用输入，见
/// [`crate::de::TrDecoder`] 文档），但多了 `Alloc` 类型参数。同样的 blanket impl 会让
/// `Alloc` 只出现在 where 谓词里，触发 **E0207**（类型参数未被约束），Rust
/// 不允许，因此不能复刻 serde 的写法。手写 visitor 需要显式
/// 适配，提供两个入口：[`Visitor::expected`] 与自由函数 [`expected`]。
///
/// 两者都返回只借用 visitor 的轻量视图，不分配内存。但 visitor 类型通常不携带
/// `Alloc`，所以**调用点必须能确定 `'alloc / Alloc`**：
///
/// - 在泛型 helper 里（`V: Visitor<'alloc, Alloc>` 就在作用域内）：
///   `&v.expected()` 与 `&expected(&v)` 都能自动推断；
/// - 在 visitor impl 的 `visit_*` 方法体内：`self.expected()` 会因参数推断歧义
///   报 E0282；请用自由函数并显式写出 allocator 参数
///   `&expected::<Alloc, _>(&self)`（把 `Alloc` 换成 impl 里的参数名），
///   或全限定 `<Self as Visitor<'alloc, Alloc>>::expected(&self)`；
/// - 描述是固定字符串时，直接 `&"a string"` 最简单（[`Expected`] 对 `&str`
///   有实现）。
///
/// # Examples
///
/// ```
/// use abs_denco::de::{Expected, TrArena, Visitor, expected};
///
/// struct MyVisitor;
///
/// impl<'alloc, Alloc: TrArena<'alloc> + 'alloc> Visitor<'alloc, Alloc> for MyVisitor
/// {
///     type Value = ();
///
///     fn expecting(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
///         f.write_str("a number")
///     }
/// }
///
/// fn takes(_: &dyn Expected) {}
///
/// fn demo<'alloc, Alloc, V>(v: &V)
/// where
///     V: Visitor<'alloc, Alloc>,
///     Alloc: TrArena<'alloc> + 'alloc,
/// {
///     // 方法版
///     takes(&v.expected());
///     // 自由函数版
///     takes(&expected(v));
/// }
/// ```
pub trait Expected {
    /// 把「期望什么」写入 `formatter`。
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result;
}

/// 把一个 [`Visitor`] 适配成可传给 [`Error::invalid_type`] 等方法的 [`Expected`]。
///
/// 这是 [`Visitor::expected`] 的自由函数版本。两者功能相同；自由函数的好处是
/// 可以用 turbofish 显式钉住 `Alloc`：
///
/// - 在泛型 helper 里（`V: Visitor<'alloc, Alloc>` 在作用域内）：
///   `&expected(&v)` 即可；
/// - 在 visitor impl 的 `visit_*` 方法体内（visitor 类型通常不携带 `Alloc`）：
///   写 `&expected::<Alloc, _>(&self)`，把 `Alloc` 换成 impl 的参数名；
/// - 描述是固定字符串时，直接 `&"a string"` 也可以。
///
/// 为什么需要显式适配（而不是 serde 那样直接 `&self`），见 [`Expected`] 的
/// 「与 serde 的差异」一节。
///
/// # Examples
///
/// ```
/// use abs_denco::de::{Expected, TrArena, Visitor, expected};
///
/// struct MyVisitor;
///
/// impl<'alloc, Alloc: TrArena<'alloc> + 'alloc> Visitor<'alloc, Alloc> for MyVisitor
/// {
///     type Value = ();
///
///     fn expecting(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
///         f.write_str("a number")
///     }
/// }
///
/// fn takes(_: &dyn Expected) {}
///
/// fn demo<'alloc, Alloc, V>(v: &V)
/// where
///     V: Visitor<'alloc, Alloc>,
///     Alloc: TrArena<'alloc> + 'alloc,
/// {
///     takes(&expected(v));
/// }
/// ```
pub fn expected<'a, 'alloc, Alloc, V>(visitor: &'a V) -> impl Expected + 'a
where
    V: Visitor<'alloc, Alloc> + 'a,
    Alloc: TrArena<'alloc> + 'alloc,
{
    let format: fn(&V, &mut fmt::Formatter<'_>) -> fmt::Result = |v, f| v.expecting(f);
    VisitorExpected::new_(visitor, format)
}

/// [`Visitor`] 的 [`Expected`] 适配器。
///
/// 它**不携带 `Alloc`**：`Expected` 的 impl 只依赖 `V`，因此不会出现 E0207；
/// 「如何格式化」在构造时以函数指针保存，避免把 `Alloc` 带入适配器类型，也避免
/// 额外的生命周期约束（否则 `Alloc` 可能需要 `'a`）。
pub(crate) struct VisitorExpected<'a, V: ?Sized> {
    visitor_: &'a V,
    format_: fn(&V, &mut fmt::Formatter<'_>) -> fmt::Result,
}

impl<'a, V: ?Sized> VisitorExpected<'a, V> {
    pub(crate) fn new_(
        visitor: &'a V,
        format: fn(&V, &mut fmt::Formatter<'_>) -> fmt::Result,
    ) -> Self {
        Self {
            visitor_: visitor,
            format_: format,
        }
    }
}

impl<V: ?Sized> Expected for VisitorExpected<'_, V> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        (self.format_)(self.visitor_, formatter)
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
