//! 标准容器解码共用的收集 visitor。
//!
//! 目标容器用 [`TrSeqSink`] / [`TrMapSink`] 参数化：非 `Global` allocator 的
//! 容器没有 `FromIterator` 实现（标准库只给默认 allocator 提供），所以这里
//! 改成「先建空容器、再逐个插入」。allocator 实例由 `A: Default` 提供。

use core::marker::PhantomData;

use abs_cancel::{TrCancellationToken, TrMayCancel};

#[cfg(any(feature = "alloc-collections", feature = "std"))]
use super::{MapAccess, MapAccessExt, sink_::TrMapSink};
use super::{SeqAccess, TrArena, TrDecodable, Visitor, sink_::TrSeqSink};

/// 序列访问者：把元素收进任意实现 [`TrSeqSink`] 的容器。
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

impl<'de, 'alloc, Alloc: TrArena<'alloc> + 'alloc, T, C> Visitor<'de, 'alloc, Alloc>
    for SeqCollectVisitor<T, C>
where
    T: TrDecodable<'de, 'alloc, Alloc> + 'de,
    C: TrSeqSink<T>,
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
        'alloc: 'a,
        Self: 'a,
        Self::Value: 'a,
        A: SeqAccess<'de, 'alloc, Alloc> + 'a,
        A::Error: 'a,
    {
        DecodeSeqCollectOpAsync::<'de, 'alloc, 'a, 'a, Alloc, T, C, A>::new(seq)
    }
}

#[gen_mcf2::gen_may_cancel_future(DecodeSeqCollectOp, pub)]
async fn decode_seq_collect_async_<'de, 'alloc, 'a, Alloc, T, C, A, TyTok>(
    mut seq: A,
    cancel: TyTok,
) -> Result<C, A::Error>
where
    Alloc: TrArena<'alloc> + 'alloc,
    'de: 'a,
    T: TrDecodable<'de, 'alloc, Alloc> + 'de,
    C: TrSeqSink<T> + 'a,
    A: SeqAccess<'de, 'alloc, Alloc> + 'a,
    A::Error: 'a,
    TyTok: TrCancellationToken,
{
    let mut out = C::empty_();
    while let Option::Some(value) = seq
        .next_element_seed(PhantomData::<T>)
        .may_cancel_with(cancel.child_token())
        .await?
    {
        out.push_(value);
    }
    Result::Ok(out)
}

/// 映射访问者：把键值对收进任意实现 [`TrMapSink`] 的容器。
///
/// 与 [`SeqCollectVisitor`] 相同，它需要出现在公开的 GAT 关联类型里。
#[cfg(any(feature = "alloc-collections", feature = "std"))]
pub struct MapCollectVisitor<K, V, C>(PhantomData<(K, V, C)>);

#[cfg(any(feature = "alloc-collections", feature = "std"))]
impl<K, V, C> MapCollectVisitor<K, V, C> {
    /// 构造一个只携带类型参数的访问者。
    pub(crate) const fn new_() -> Self {
        Self(PhantomData)
    }
}

#[cfg(any(feature = "alloc-collections", feature = "std"))]
impl<'de, 'alloc, Alloc: TrArena<'alloc> + 'alloc, K, V, C> Visitor<'de, 'alloc, Alloc>
    for MapCollectVisitor<K, V, C>
where
    K: TrDecodable<'de, 'alloc, Alloc> + 'de,
    V: TrDecodable<'de, 'alloc, Alloc> + 'de,
    C: TrMapSink<K, V>,
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
        'alloc: 'a,
        Self: 'a,
        Self::Value: 'a,
        A: MapAccess<'de, 'alloc, Alloc> + 'a,
        A::Error: 'a,
    {
        DecodeMapCollectOpAsync::<'de, 'alloc, 'a, 'a, Alloc, K, V, C, A>::new(map)
    }
}

#[cfg(any(feature = "alloc-collections", feature = "std"))]
#[gen_mcf2::gen_may_cancel_future(DecodeMapCollectOp, pub)]
async fn decode_map_collect_async_<'de, 'alloc, 'a, Alloc, K, V, C, A, TyTok>(
    mut map: A,
    cancel: TyTok,
) -> Result<C, A::Error>
where
    Alloc: TrArena<'alloc> + 'alloc,
    'de: 'a,
    K: TrDecodable<'de, 'alloc, Alloc> + 'de,
    V: TrDecodable<'de, 'alloc, Alloc> + 'de,
    C: TrMapSink<K, V> + 'a,
    A: MapAccess<'de, 'alloc, Alloc> + 'a,
    A::Error: 'a,
    TyTok: TrCancellationToken,
{
    let mut out = C::empty_();
    while let Option::Some((key, value)) = map
        .next_entry::<K, V>()
        .may_cancel_with(cancel.child_token())
        .await?
    {
        out.insert_(key, value);
    }
    Result::Ok(out)
}
