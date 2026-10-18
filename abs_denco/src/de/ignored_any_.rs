//! 丢弃任意输入的 `IgnoredAny`。

#![allow(missing_docs)]

use core::{fmt, future};

use abs_cancel::{NonCancellableToken, TrCancellationToken, TrMayCancel};
use gen_mcf2::gen_may_cancel_future;

use super::{
    BoxedMayCancel, EnumAccess, EnumAccessExt, Error, MapAccess, MapAccessExt, SeqAccess,
    SeqAccessExt, TrDecodable, TrDecoder, VariantAccessExt, Visitor,
};

/// 一个可以接收任意输入、但不保存任何信息的访问者。
///
/// 它适合跳过未知字段或未知序列元素。实现方式与同步版 serde 的
/// `IgnoredAny` 对齐：只覆盖必要的 `visit_*` 方法，其余方法通过默认转发
/// 落入这些方法。
#[derive(Copy, Clone, Debug, Default, PartialEq)]
pub struct IgnoredAny;

macro_rules! ignored_value {
    ($name:ident, $ty:ty) => {
        fn $name<'a, E>(
            self,
            _v: $ty,
        ) -> impl TrMayCancel<'a, MayCancelOutput = Result<Self::Value, E>>
        where
            Self: 'a,
            Self::Value: 'a,
            E: Error + 'a,
        {
            future::ready(Ok(IgnoredAny))
        }
    };
}

#[gen_may_cancel_future(VisitIgnoredSeq)]
async fn visit_ignored_seq_async<'de, 'a, A, C>(
    mut seq: A,
    cancel: C,
) -> Result<IgnoredAny, A::Error>
where
    'de: 'a,
    A: SeqAccess<'de> + 'a,
    A::Error: 'a,
    C: TrCancellationToken,
{
    while seq
        .next_element::<IgnoredAny>()
        .may_cancel_with(cancel.child_token())
        .await?
        .is_some()
    {}
    Ok(IgnoredAny)
}

#[gen_may_cancel_future(VisitIgnoredMap)]
async fn visit_ignored_map_async<'de, 'a, A, C>(
    mut map: A,
    cancel: C,
) -> Result<IgnoredAny, A::Error>
where
    'de: 'a,
    A: MapAccess<'de> + 'a,
    A::Error: 'a,
    C: TrCancellationToken,
{
    while map
        .next_entry::<IgnoredAny, IgnoredAny>()
        .may_cancel_with(cancel.child_token())
        .await?
        .is_some()
    {}
    Ok(IgnoredAny)
}

#[gen_may_cancel_future(VisitIgnoredEnum)]
async fn visit_ignored_enum_async<'de, 'a, A, C>(data: A, cancel: C) -> Result<IgnoredAny, A::Error>
where
    'de: 'a,
    A: EnumAccess<'de> + 'a,
    A::Error: 'a,
    C: TrCancellationToken,
{
    let (_, variant) = data
        .variant::<IgnoredAny>()
        .may_cancel_with(cancel.child_token())
        .await?;
    variant
        .newtype_variant::<IgnoredAny>()
        .may_cancel_with(cancel)
        .await
}

impl<'de> Visitor<'de> for IgnoredAny {
    type Value = IgnoredAny;

    fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("anything at all")
    }

    ignored_value!(visit_bool, bool);
    ignored_value!(visit_i64, i64);
    ignored_value!(visit_i128, i128);
    ignored_value!(visit_u64, u64);
    ignored_value!(visit_u128, u128);
    ignored_value!(visit_f64, f64);
    ignored_value!(visit_str, &'a str);
    ignored_value!(visit_bytes, &'a [u8]);

    fn visit_none<'a, E>(self) -> impl TrMayCancel<'a, MayCancelOutput = Result<Self::Value, E>>
    where
        Self: 'a,
        Self::Value: 'a,
        E: Error + 'a,
    {
        future::ready(Ok(IgnoredAny))
    }

    fn visit_unit<'a, E>(self) -> impl TrMayCancel<'a, MayCancelOutput = Result<Self::Value, E>>
    where
        Self: 'a,
        Self::Value: 'a,
        E: Error + 'a,
    {
        future::ready(Ok(IgnoredAny))
    }

    fn visit_some<'a, D>(
        self,
        deserializer: D,
    ) -> impl TrMayCancel<'a, MayCancelOutput = Result<Self::Value, D::Error>>
    where
        'de: 'a,
        Self: 'a,
        Self::Value: 'a,
        D: TrDecoder<'de> + 'a,
        D::Error: 'a,
    {
        IgnoredAny::decode_async(deserializer)
    }

    fn visit_newtype_struct<'a, D>(
        self,
        deserializer: D,
    ) -> impl TrMayCancel<'a, MayCancelOutput = Result<Self::Value, D::Error>>
    where
        'de: 'a,
        Self: 'a,
        Self::Value: 'a,
        D: TrDecoder<'de> + 'a,
        D::Error: 'a,
    {
        IgnoredAny::decode_async(deserializer)
    }

    fn visit_seq<'a, A>(
        self,
        seq: A,
    ) -> impl TrMayCancel<'a, MayCancelOutput = Result<Self::Value, A::Error>>
    where
        'de: 'a,
        Self: 'a,
        Self::Value: 'a,
        A: SeqAccess<'de> + 'a,
        A::Error: 'a,
    {
        VisitIgnoredSeqAsync::<'de, 'a, 'a, A>::new(seq)
    }

    fn visit_map<'a, A>(
        self,
        map: A,
    ) -> impl TrMayCancel<'a, MayCancelOutput = Result<Self::Value, A::Error>>
    where
        'de: 'a,
        Self: 'a,
        Self::Value: 'a,
        A: MapAccess<'de> + 'a,
        A::Error: 'a,
    {
        VisitIgnoredMapAsync::<'de, 'a, 'a, A>::new(map)
    }

    fn visit_enum<'a, A>(
        self,
        data: A,
    ) -> impl TrMayCancel<'a, MayCancelOutput = Result<Self::Value, A::Error>>
    where
        'de: 'a,
        Self: 'a,
        Self::Value: 'a,
        A: EnumAccess<'de> + 'a,
        A::Error: 'a,
    {
        VisitIgnoredEnumAsync::<'de, 'a, 'a, A>::new(data)
    }
}

impl<'de> TrDecodable<'de> for IgnoredAny {
    type DecodeAsync<'a, D>
        = BoxedMayCancel<'a, Result<IgnoredAny, D::Error>>
    where
        'de: 'a,
        D: TrDecoder<'de> + 'a,
        D::Error: 'a,
        Self: 'a;

    fn decode_async<'a, D>(deserializer: D) -> Self::DecodeAsync<'a, D>
    where
        'de: 'a,
        D: TrDecoder<'de> + 'a,
        D::Error: 'a,
        Self: 'a,
    {
        BoxedMayCancel::new(async move {
            deserializer
                .decode_ignored_any(IgnoredAny)
                .may_cancel_with(NonCancellableToken::new())
                .await
        })
    }
}
