//! 标准容器解码共用的收集 visitor。
//!
//! 目标容器用 [`FromIterator`] 参数化：`alloc::collections` 与 `std::collections`
//! 的容器都满足它，因此每个容器只需要一个薄壳，不必重复写 visitor 与异步循环。

use alloc::vec::Vec;
use core::marker::PhantomData;

use abs_cancel::{TrCancellationToken, TrMayCancel};

use super::{MapAccess, MapAccessExt, SeqAccess, TrDecodable, Visitor};

/// 序列访问者：把元素收进任意实现 `FromIterator<T>` 的容器。
///
/// 它出现在容器 `TrDecodable` 实现的 GAT 关联类型里，因此必须是 `pub`；
/// 所在模块不对外导出，使用者无法直接命名它。
pub struct SeqCollectVisitor<T, C>(PhantomData<(T, C)>);

impl<T, C> SeqCollectVisitor<T, C> {
    /// 构造一个只携带类型参数的访问者。
    pub(crate) const fn new_() -> Self {
        Self(PhantomData)
    }
}

impl<'de, T, C> Visitor<'de> for SeqCollectVisitor<T, C>
where
    T: TrDecodable<'de> + 'de,
    C: FromIterator<T>,
{
    type Value = C;

    fn expecting(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        formatter.write_str("a sequence")
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
        DecodeSeqCollectOpAsync::<'de, 'a, 'a, T, C, A>::new(seq)
    }
}

#[gen_mcf2::gen_may_cancel_future(DecodeSeqCollectOp, pub)]
async fn decode_seq_collect_async_<'de, 'a, T, C, A, TyTok>(
    mut seq: A,
    cancel: TyTok,
) -> Result<C, A::Error>
where
    'de: 'a,
    T: TrDecodable<'de> + 'de,
    C: FromIterator<T> + 'a,
    A: SeqAccess<'de> + 'a,
    A::Error: 'a,
    TyTok: TrCancellationToken,
{
    let mut out = Vec::new();
    while let Option::Some(value) = seq
        .next_element_seed(PhantomData::<T>)
        .may_cancel_with(cancel.child_token())
        .await?
    {
        out.push(value);
    }
    Result::Ok(out.into_iter().collect())
}

/// 映射访问者：把键值对收进任意实现 `FromIterator<(K, V)>` 的容器。
///
/// 与 [`SeqCollectVisitor`] 相同，它需要出现在公开的 GAT 关联类型里。
pub struct MapCollectVisitor<K, V, C>(PhantomData<(K, V, C)>);

impl<K, V, C> MapCollectVisitor<K, V, C> {
    /// 构造一个只携带类型参数的访问者。
    pub(crate) const fn new_() -> Self {
        Self(PhantomData)
    }
}

impl<'de, K, V, C> Visitor<'de> for MapCollectVisitor<K, V, C>
where
    K: TrDecodable<'de> + 'de,
    V: TrDecodable<'de> + 'de,
    C: FromIterator<(K, V)>,
{
    type Value = C;

    fn expecting(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        formatter.write_str("a map")
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
        DecodeMapCollectOpAsync::<'de, 'a, 'a, K, V, C, A>::new(map)
    }
}

#[gen_mcf2::gen_may_cancel_future(DecodeMapCollectOp, pub)]
async fn decode_map_collect_async_<'de, 'a, K, V, C, A, TyTok>(
    mut map: A,
    cancel: TyTok,
) -> Result<C, A::Error>
where
    'de: 'a,
    K: TrDecodable<'de> + 'de,
    V: TrDecodable<'de> + 'de,
    C: FromIterator<(K, V)> + 'a,
    A: MapAccess<'de> + 'a,
    A::Error: 'a,
    TyTok: TrCancellationToken,
{
    let mut out = Vec::new();
    while let Option::Some((key, value)) = map
        .next_entry::<K, V>()
        .may_cancel_with(cancel.child_token())
        .await?
    {
        out.push((key, value));
    }
    Result::Ok(out.into_iter().collect())
}
