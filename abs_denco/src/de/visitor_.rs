//! 反序列化访问者。

use core::future;

use alloc::{string::String, vec::Vec};

use abs_cancel::{TrCancellationToken, TrMayCancel};
use gen_mcf2::gen_may_cancel_future;

use super::{EnumAccess, Error, Expected, MapAccess, SeqAccess, TrArena, TrDecoder, Unexpected};

#[gen_may_cancel_future(VisitString)]
async fn visit_string_async<'alloc, 'a, Alloc, V, E, C>(
    visitor: V,
    v: String,
    cancel: C,
) -> Result<V::Value, E>
where
    'alloc: 'a,
    Alloc: TrArena<'alloc> + 'alloc,
    V: Visitor<'alloc, Alloc> + 'a,
    V::Value: 'a,
    E: Error + 'a,
    C: TrCancellationToken,
{
    visitor.visit_str(v.as_str()).may_cancel_with(cancel).await
}

#[gen_may_cancel_future(VisitByteBuf)]
async fn visit_byte_buf_async<'alloc, 'a, Alloc, V, E, C>(
    visitor: V,
    v: Vec<u8>,
    cancel: C,
) -> Result<V::Value, E>
where
    'alloc: 'a,
    Alloc: TrArena<'alloc> + 'alloc,
    V: Visitor<'alloc, Alloc> + 'a,
    V::Value: 'a,
    E: Error + 'a,
    C: TrCancellationToken,
{
    visitor
        .visit_bytes(v.as_slice())
        .may_cancel_with(cancel)
        .await
}

#[gen_may_cancel_future(VisitChar)]
async fn visit_char_async<'alloc, 'a, Alloc, V, E, C>(
    visitor: V,
    v: char,
    cancel: C,
) -> Result<V::Value, E>
where
    'alloc: 'a,
    Alloc: TrArena<'alloc> + 'alloc,
    V: Visitor<'alloc, Alloc> + 'a,
    V::Value: 'a,
    E: Error + 'a,
    C: TrCancellationToken,
{
    let mut buf = [0u8; 4];
    let s = v.encode_utf8(&mut buf);
    visitor.visit_str(s).may_cancel_with(cancel).await
}

/// 由数据格式驱动、负责把输入转换成某个值的数据结构。
///
/// `Visitor` 是反序列化方向的核心扩展点：`TrDecoder` 决定输入是什么
/// 类型，然后调用对应的 `visit_*` 方法；每个 `visit_*` 方法返回一个
/// [`TrMayCancel`] 的实现。
///
/// 与同步版 `serde::de::Visitor` 相比，默认实现尽量保持相同的转发语义：
/// 窄整数会转发到宽整数，`visit_string` / `visit_byte_buf` / `visit_char`
/// 也通过 `gen_mcf2` 生成的 helper future 转发到 `visit_str` / `visit_bytes`。
/// serde 的 `visit_borrowed_str` / `visit_borrowed_bytes`（输入借用入口）在
/// denco 中不存在：`visit_str` / `visit_bytes` 的 `'a` 只是本次 visit 的临时
/// 视图寿命，解出的借用值只能来自 decoder 的 arena。
///
/// # Errors
///
/// 每个 `visit_*` 方法在输入类型不符时返回 `E`。默认实现总是返回
/// [`Unexpected`] 描述的类型错误。
pub trait Visitor<'alloc, Alloc: TrArena<'alloc> + 'alloc>: Sized {
    /// 访问者最终产生的值。
    type Value;

    /// 描述本访问者期望接收什么。
    fn expecting(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result;

    /// 返回一个可传给 [`Error::invalid_type`] 等方法的 [`Expected`] 视图。
    ///
    /// serde 用户请注意：serde 的 `Visitor` 只有 `'de`，因此 serde 能提供
    /// `impl<V: Visitor<'de>> Expected for V`，手写 visitor 里直接写 `&self`
    /// 就能当 `&dyn Expected`。denco 的 `Visitor<'alloc, Alloc>` 没有输入生命周期，
    /// 但多了 `Alloc` 类型参数；同样的 blanket impl 会触发 **E0207**（类型参数
    /// 未被约束），所以必须显式适配。
    ///
    /// 本方法适合**参数已确定**的泛型 helper（`V: Visitor<'alloc, Alloc>` 在作用域内）。visitor impl 的 `visit_*` 方法体里，`self` 的类型通常不
    /// 携带 `Alloc`，此时 `self.expected()` 会因参数推断歧义报 E0282；请改用
    /// 自由函数 `&expected::<Alloc, _>(&self)`（把 `Alloc` 换成 impl 的参数名）
    /// 或全限定 `<Self as Visitor<'alloc, Alloc>>::expected(self)`，也可以
    /// 直接传静态描述字符串。详见 [`Expected`] 文档。
    ///
    /// ```
    /// use abs_denco::de::{Error, Expected, TrArena, Unexpected, Visitor, expected};
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
    ///     // 泛型 helper 中两种写法都能推断
    ///     takes(&v.expected());
    ///     takes(&expected(v));
    /// }
    /// ```
    fn expected(&self) -> impl Expected + '_ {
        let format: fn(&Self, &mut core::fmt::Formatter<'_>) -> core::fmt::Result =
            |v, f| v.expecting(f);
        super::error_::VisitorExpected::new_(self, format)
    }

    /// 输入包含一个 `bool`。
    ///
    /// # Errors
    ///
    /// 默认实现返回类型错误。
    fn visit_bool<'a, E>(
        self,
        v: bool,
    ) -> impl TrMayCancel<'a, MayCancelOutput = Result<Self::Value, E>>
    where
        Self: 'a,
        Self::Value: 'a,
        E: Error + 'a,
    {
        let _ = v;
        future::ready(Err(<E as Error>::invalid_type(
            Unexpected::Bool(v),
            &super::error_::expected::<Alloc, _>(&self),
        )))
    }

    /// 输入包含一个 `i8`；默认转发到 [`Visitor::visit_i64`]。
    fn visit_i8<'a, E>(
        self,
        v: i8,
    ) -> impl TrMayCancel<'a, MayCancelOutput = Result<Self::Value, E>>
    where
        Self: 'a,
        Self::Value: 'a,
        E: Error + 'a,
    {
        self.visit_i64(v as i64)
    }

    /// 输入包含一个 `i16`；默认转发到 [`Visitor::visit_i64`]。
    fn visit_i16<'a, E>(
        self,
        v: i16,
    ) -> impl TrMayCancel<'a, MayCancelOutput = Result<Self::Value, E>>
    where
        Self: 'a,
        Self::Value: 'a,
        E: Error + 'a,
    {
        self.visit_i64(v as i64)
    }

    /// 输入包含一个 `i32`；默认转发到 [`Visitor::visit_i64`]。
    fn visit_i32<'a, E>(
        self,
        v: i32,
    ) -> impl TrMayCancel<'a, MayCancelOutput = Result<Self::Value, E>>
    where
        Self: 'a,
        Self::Value: 'a,
        E: Error + 'a,
    {
        self.visit_i64(v as i64)
    }

    /// 输入包含一个 `i64`。
    ///
    /// # Errors
    ///
    /// 默认实现返回类型错误。
    fn visit_i64<'a, E>(
        self,
        v: i64,
    ) -> impl TrMayCancel<'a, MayCancelOutput = Result<Self::Value, E>>
    where
        Self: 'a,
        Self::Value: 'a,
        E: Error + 'a,
    {
        let _ = v;
        future::ready(Err(<E as Error>::invalid_type(
            Unexpected::Signed(v),
            &super::error_::expected::<Alloc, _>(&self),
        )))
    }

    /// 输入包含一个 `i128`。
    ///
    /// # Errors
    ///
    /// 默认实现返回类型错误。
    fn visit_i128<'a, E>(
        self,
        v: i128,
    ) -> impl TrMayCancel<'a, MayCancelOutput = Result<Self::Value, E>>
    where
        Self: 'a,
        Self::Value: 'a,
        E: Error + 'a,
    {
        let _ = v;
        future::ready(Err(<E as Error>::invalid_type(
            Unexpected::Other("i128"),
            &super::error_::expected::<Alloc, _>(&self),
        )))
    }

    /// 输入包含一个 `u8`；默认转发到 [`Visitor::visit_u64`]。
    fn visit_u8<'a, E>(
        self,
        v: u8,
    ) -> impl TrMayCancel<'a, MayCancelOutput = Result<Self::Value, E>>
    where
        Self: 'a,
        Self::Value: 'a,
        E: Error + 'a,
    {
        self.visit_u64(v as u64)
    }

    /// 输入包含一个 `u16`；默认转发到 [`Visitor::visit_u64`]。
    fn visit_u16<'a, E>(
        self,
        v: u16,
    ) -> impl TrMayCancel<'a, MayCancelOutput = Result<Self::Value, E>>
    where
        Self: 'a,
        Self::Value: 'a,
        E: Error + 'a,
    {
        self.visit_u64(v as u64)
    }

    /// 输入包含一个 `u32`；默认转发到 [`Visitor::visit_u64`]。
    fn visit_u32<'a, E>(
        self,
        v: u32,
    ) -> impl TrMayCancel<'a, MayCancelOutput = Result<Self::Value, E>>
    where
        Self: 'a,
        Self::Value: 'a,
        E: Error + 'a,
    {
        self.visit_u64(v as u64)
    }

    /// 输入包含一个 `u64`。
    ///
    /// # Errors
    ///
    /// 默认实现返回类型错误。
    fn visit_u64<'a, E>(
        self,
        v: u64,
    ) -> impl TrMayCancel<'a, MayCancelOutput = Result<Self::Value, E>>
    where
        Self: 'a,
        Self::Value: 'a,
        E: Error + 'a,
    {
        let _ = v;
        future::ready(Err(<E as Error>::invalid_type(
            Unexpected::Unsigned(v),
            &super::error_::expected::<Alloc, _>(&self),
        )))
    }

    /// 输入包含一个 `u128`。
    ///
    /// # Errors
    ///
    /// 默认实现返回类型错误。
    fn visit_u128<'a, E>(
        self,
        v: u128,
    ) -> impl TrMayCancel<'a, MayCancelOutput = Result<Self::Value, E>>
    where
        Self: 'a,
        Self::Value: 'a,
        E: Error + 'a,
    {
        let _ = v;
        future::ready(Err(<E as Error>::invalid_type(
            Unexpected::Other("u128"),
            &super::error_::expected::<Alloc, _>(&self),
        )))
    }

    /// 输入包含一个 `f32`；默认转发到 [`Visitor::visit_f64`]。
    fn visit_f32<'a, E>(
        self,
        v: f32,
    ) -> impl TrMayCancel<'a, MayCancelOutput = Result<Self::Value, E>>
    where
        Self: 'a,
        Self::Value: 'a,
        E: Error + 'a,
    {
        self.visit_f64(v as f64)
    }

    /// 输入包含一个 `f64`。
    ///
    /// # Errors
    ///
    /// 默认实现返回类型错误。
    fn visit_f64<'a, E>(
        self,
        v: f64,
    ) -> impl TrMayCancel<'a, MayCancelOutput = Result<Self::Value, E>>
    where
        Self: 'a,
        Self::Value: 'a,
        E: Error + 'a,
    {
        let _ = v;
        future::ready(Err(<E as Error>::invalid_type(
            Unexpected::Float(v),
            &super::error_::expected::<Alloc, _>(&self),
        )))
    }

    /// 输入包含一个 `char`。
    ///
    /// # Errors
    ///
    /// 默认实现返回类型错误。
    fn visit_char<'a, E>(
        self,
        v: char,
    ) -> impl TrMayCancel<'a, MayCancelOutput = Result<Self::Value, E>>
    where
        Self: 'a,
        Self::Value: 'a,
        E: Error + 'a,
        'alloc: 'a,
    {
        VisitCharAsync::<'alloc, 'a, 'a, Alloc, Self, E>::new(self, v)
    }

    /// 输入包含一个字符串切片。
    ///
    /// # Errors
    ///
    /// 默认实现返回类型错误。
    fn visit_str<'a, E>(
        self,
        v: &'a str,
    ) -> impl TrMayCancel<'a, MayCancelOutput = Result<Self::Value, E>>
    where
        Self: 'a,
        Self::Value: 'a,
        E: Error + 'a,
    {
        let _ = v;
        future::ready(Err(<E as Error>::invalid_type(
            Unexpected::Str(v),
            &super::error_::expected::<Alloc, _>(&self),
        )))
    }

    /// 输入包含一个拥有所有权的字符串。
    ///
    /// # Errors
    ///
    /// 默认实现返回类型错误；需要接受 `String` 的访问者应覆盖本方法。
    fn visit_string<'a, E>(
        self,
        v: String,
    ) -> impl TrMayCancel<'a, MayCancelOutput = Result<Self::Value, E>>
    where
        Self: 'a,
        Self::Value: 'a,
        E: Error + 'a,
        'alloc: 'a,
    {
        VisitStringAsync::<'alloc, 'a, 'a, Alloc, Self, E>::new(self, v)
    }

    /// 输入包含一段字节切片。
    ///
    /// # Errors
    ///
    /// 默认实现返回类型错误。
    fn visit_bytes<'a, E>(
        self,
        v: &'a [u8],
    ) -> impl TrMayCancel<'a, MayCancelOutput = Result<Self::Value, E>>
    where
        Self: 'a,
        Self::Value: 'a,
        E: Error + 'a,
    {
        let _ = v;
        future::ready(Err(<E as Error>::invalid_type(
            Unexpected::Bytes(v),
            &super::error_::expected::<Alloc, _>(&self),
        )))
    }

    /// 输入包含一段拥有所有权的字节。
    ///
    /// # Errors
    ///
    /// 默认实现返回类型错误；需要接受 `Vec<u8>` 的访问者应覆盖本方法。
    fn visit_byte_buf<'a, E>(
        self,
        v: Vec<u8>,
    ) -> impl TrMayCancel<'a, MayCancelOutput = Result<Self::Value, E>>
    where
        Self: 'a,
        Self::Value: 'a,
        E: Error + 'a,
        'alloc: 'a,
    {
        VisitByteBufAsync::<'alloc, 'a, 'a, Alloc, Self, E>::new(self, v)
    }

    /// 输入包含 `None`。
    ///
    /// # Errors
    ///
    /// 默认实现返回类型错误。
    fn visit_none<'a, E>(self) -> impl TrMayCancel<'a, MayCancelOutput = Result<Self::Value, E>>
    where
        Self: 'a,
        Self::Value: 'a,
        E: Error + 'a,
    {
        future::ready(Err(<E as Error>::invalid_type(
            Unexpected::Option,
            &super::error_::expected::<Alloc, _>(&self),
        )))
    }

    /// 输入包含 `Some(deserializer)`。
    ///
    /// # Errors
    ///
    /// 默认实现返回类型错误。
    fn visit_some<'a, D>(
        self,
        deserializer: D,
    ) -> impl TrMayCancel<'a, MayCancelOutput = Result<Self::Value, D::Error>>
    where
        Self: 'a,
        'alloc: 'a,
        Self::Value: 'a,
        D: TrDecoder<'alloc, Alloc> + 'a,
        D::Error: 'a,
    {
        let _ = deserializer;
        future::ready(Err(<D::Error as Error>::invalid_type(
            Unexpected::Option,
            &super::error_::expected::<Alloc, _>(&self),
        )))
    }

    /// 输入包含 `()`。
    ///
    /// # Errors
    ///
    /// 默认实现返回类型错误。
    fn visit_unit<'a, E>(self) -> impl TrMayCancel<'a, MayCancelOutput = Result<Self::Value, E>>
    where
        Self: 'a,
        Self::Value: 'a,
        E: Error + 'a,
    {
        future::ready(Err(<E as Error>::invalid_type(
            Unexpected::Unit,
            &super::error_::expected::<Alloc, _>(&self),
        )))
    }

    /// 输入包含一个 newtype 结构体。
    ///
    /// # Errors
    ///
    /// 默认实现返回类型错误。
    fn visit_newtype_struct<'a, D>(
        self,
        deserializer: D,
    ) -> impl TrMayCancel<'a, MayCancelOutput = Result<Self::Value, D::Error>>
    where
        Self: 'a,
        'alloc: 'a,
        Self::Value: 'a,
        D: TrDecoder<'alloc, Alloc> + 'a,
        D::Error: 'a,
    {
        let _ = deserializer;
        future::ready(Err(<D::Error as Error>::invalid_type(
            Unexpected::NewtypeStruct,
            &super::error_::expected::<Alloc, _>(&self),
        )))
    }

    /// 输入包含一个序列。
    ///
    /// # Errors
    ///
    /// 默认实现返回类型错误。
    fn visit_seq<'a, A>(
        self,
        seq: A,
    ) -> impl TrMayCancel<'a, MayCancelOutput = Result<Self::Value, A::Error>>
    where
        Self: 'a,
        'alloc: 'a,
        Self::Value: 'a,
        A: SeqAccess<'alloc, Alloc> + 'a,
        A::Error: 'a,
    {
        let _ = seq;
        future::ready(Err(<A::Error as Error>::invalid_type(
            Unexpected::Seq,
            &super::error_::expected::<Alloc, _>(&self),
        )))
    }

    /// 输入包含一个映射。
    ///
    /// # Errors
    ///
    /// 默认实现返回类型错误。
    fn visit_map<'a, A>(
        self,
        map: A,
    ) -> impl TrMayCancel<'a, MayCancelOutput = Result<Self::Value, A::Error>>
    where
        Self: 'a,
        'alloc: 'a,
        Self::Value: 'a,
        A: MapAccess<'alloc, Alloc> + 'a,
        A::Error: 'a,
    {
        let _ = map;
        future::ready(Err(<A::Error as Error>::invalid_type(
            Unexpected::Map,
            &super::error_::expected::<Alloc, _>(&self),
        )))
    }

    /// 输入包含一个枚举。
    ///
    /// # Errors
    ///
    /// 默认实现返回类型错误。
    fn visit_enum<'a, A>(
        self,
        data: A,
    ) -> impl TrMayCancel<'a, MayCancelOutput = Result<Self::Value, A::Error>>
    where
        Self: 'a,
        'alloc: 'a,
        Self::Value: 'a,
        A: EnumAccess<'alloc, Alloc> + 'a,
        A::Error: 'a,
    {
        let _ = data;
        future::ready(Err(<A::Error as Error>::invalid_type(
            Unexpected::Enum,
            &super::error_::expected::<Alloc, _>(&self),
        )))
    }
}
