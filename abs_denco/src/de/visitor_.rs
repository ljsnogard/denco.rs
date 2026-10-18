//! 反序列化访问者。

use core::future;

use alloc::{string::String, vec::Vec};

use abs_cancel::{TrCancellationToken, TrMayCancel};
use gen_mcf2::gen_may_cancel_future;

use super::{EnumAccess, Error, MapAccess, SeqAccess, TrDecoder, Unexpected};

#[gen_may_cancel_future(VisitString)]
async fn visit_string_async<'de, 'a, V, E, C>(
    visitor: V,
    v: String,
    cancel: C,
) -> Result<V::Value, E>
where
    'de: 'a,
    V: Visitor<'de> + 'a,
    V::Value: 'a,
    E: Error + 'a,
    C: TrCancellationToken,
{
    visitor.visit_str(v.as_str()).may_cancel_with(cancel).await
}

#[gen_may_cancel_future(VisitByteBuf)]
async fn visit_byte_buf_async<'de, 'a, V, E, C>(
    visitor: V,
    v: Vec<u8>,
    cancel: C,
) -> Result<V::Value, E>
where
    'de: 'a,
    V: Visitor<'de> + 'a,
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
async fn visit_char_async<'de, 'a, V, E, C>(visitor: V, v: char, cancel: C) -> Result<V::Value, E>
where
    'de: 'a,
    V: Visitor<'de> + 'a,
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
/// 与同步版 `serde::de::Visitor` 相比，默认实现尽量保持相同的语义：
/// 窄整数会转发到宽整数，借用的字符串/字节会转发到对应的非借用方法，
/// `visit_string` / `visit_byte_buf` / `visit_char` 也通过 `gen_mcf2`
/// 生成的 helper future 转发到对应的借用方法。
///
/// # Errors
///
/// 每个 `visit_*` 方法在输入类型不符时返回 `E`。默认实现总是返回
/// [`Unexpected`] 描述的类型错误。
pub trait Visitor<'de>: Sized {
    /// 访问者最终产生的值。
    type Value;

    /// 描述本访问者期望接收什么。
    fn expecting(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result;

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
        future::ready(Err(<E as Error>::invalid_type(Unexpected::Bool(v), &self)))
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
            &self,
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
            &self,
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
            &self,
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
            &self,
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
        future::ready(Err(<E as Error>::invalid_type(Unexpected::Float(v), &self)))
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
        'de: 'a,
    {
        VisitCharAsync::<'de, 'a, 'a, Self, E>::new(self, v)
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
        future::ready(Err(<E as Error>::invalid_type(Unexpected::Str(v), &self)))
    }

    /// 输入包含一个与 `'de` 同寿命的字符串；默认转发到
    /// [`Visitor::visit_str`]。
    fn visit_borrowed_str<'a, E>(
        self,
        v: &'de str,
    ) -> impl TrMayCancel<'a, MayCancelOutput = Result<Self::Value, E>>
    where
        Self: 'a,
        Self::Value: 'a,
        E: Error + 'a,
        'de: 'a,
    {
        self.visit_str(v)
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
        'de: 'a,
    {
        VisitStringAsync::<'de, 'a, 'a, Self, E>::new(self, v)
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
        future::ready(Err(<E as Error>::invalid_type(Unexpected::Bytes(v), &self)))
    }

    /// 输入包含一段与 `'de` 同寿命的字节；默认转发到
    /// [`Visitor::visit_bytes`]。
    fn visit_borrowed_bytes<'a, E>(
        self,
        v: &'de [u8],
    ) -> impl TrMayCancel<'a, MayCancelOutput = Result<Self::Value, E>>
    where
        Self: 'a,
        Self::Value: 'a,
        E: Error + 'a,
        'de: 'a,
    {
        self.visit_bytes(v)
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
        'de: 'a,
    {
        VisitByteBufAsync::<'de, 'a, 'a, Self, E>::new(self, v)
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
        future::ready(Err(<E as Error>::invalid_type(Unexpected::Option, &self)))
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
        'de: 'a,
        Self::Value: 'a,
        D: TrDecoder<'de> + 'a,
        D::Error: 'a,
    {
        let _ = deserializer;
        future::ready(Err(<D::Error as Error>::invalid_type(
            Unexpected::Option,
            &self,
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
        future::ready(Err(<E as Error>::invalid_type(Unexpected::Unit, &self)))
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
        'de: 'a,
        Self::Value: 'a,
        D: TrDecoder<'de> + 'a,
        D::Error: 'a,
    {
        let _ = deserializer;
        future::ready(Err(<D::Error as Error>::invalid_type(
            Unexpected::NewtypeStruct,
            &self,
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
        'de: 'a,
        Self::Value: 'a,
        A: SeqAccess<'de> + 'a,
        A::Error: 'a,
    {
        let _ = seq;
        future::ready(Err(<A::Error as Error>::invalid_type(
            Unexpected::Seq,
            &self,
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
        'de: 'a,
        Self::Value: 'a,
        A: MapAccess<'de> + 'a,
        A::Error: 'a,
    {
        let _ = map;
        future::ready(Err(<A::Error as Error>::invalid_type(
            Unexpected::Map,
            &self,
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
        'de: 'a,
        Self::Value: 'a,
        A: EnumAccess<'de> + 'a,
        A::Error: 'a,
    {
        let _ = data;
        future::ready(Err(<A::Error as Error>::invalid_type(
            Unexpected::Enum,
            &self,
        )))
    }
}
