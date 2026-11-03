//! 标准容器解码共用的收集 visitor。
//!
//! 目标容器用 [`TrSeqSink`] / [`TrMapSink`] 参数化：非 `Global` allocator 的
//! 容器没有 `FromIterator` 实现（标准库只给默认 allocator 提供），所以这里
//! 改成「先建空容器、再逐个插入」。allocator 实例由容器 `TrDecodable` 实现
//! 从 decoder 的 arena 投影后传进来，不再使用 `A::default()`。

use core::marker::PhantomData;

use abs_cancel::{TrCancellationToken, TrMayCancel};

#[cfg(any(feature = "alloc-collections", feature = "std"))]
use super::{MapAccess, MapAccessExt, sink_::TrMapSink};
use super::{SeqAccess, TrArena, TrDecodable, Visitor, sink_::TrSeqSink};

/// 序列访问者：把元素收进任意实现 [`TrSeqSink`] 的容器。
///
/// 它携带目标容器的 allocator 实例，因此同一个 visitor 构造出的容器与
/// decoder 共享 arena（当 allocator 是 `&'alloc A` 时）。
///
/// 它出现在容器 `TrDecodable` 实现的 GAT 关联类型里，因此必须是 `pub`；
/// 所在模块不对外导出，使用者无法直接命名它。
pub struct SeqCollectVisitor<T, C: TrSeqSink<T>> {
    alloc_: C::Alloc,
    marker_: PhantomData<(T, C)>,
}

impl<T, C: TrSeqSink<T>> SeqCollectVisitor<T, C> {
    /// 构造一个携带目标 allocator 的访问者。
    pub(crate) fn new_(alloc: C::Alloc) -> Self {
        Self {
            alloc_: alloc,
            marker_: PhantomData,
        }
    }
}

impl<'alloc, Alloc: TrArena<'alloc> + 'alloc, T, C> Visitor<'alloc, Alloc>
    for SeqCollectVisitor<T, C>
where
    T: TrDecodable<'alloc, Alloc> + 'alloc,
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
        'alloc: 'a,
        Self: 'a,
        Self::Value: 'a,
        A: SeqAccess<'alloc, Alloc> + 'a,
        A::Error: 'a,
    {
        DecodeSeqCollectOpAsync::<'alloc, 'a, 'a, Alloc, T, C, A>::new(seq, self.alloc_)
    }
}

#[gen_mcf2::gen_may_cancel_future(DecodeSeqCollectOp, pub)]
async fn decode_seq_collect_async_<'alloc, 'a, Arena, T, C, A, TyTok>(
    mut seq: A,
    alloc: C::Alloc,
    cancel: TyTok,
) -> Result<C, A::Error>
where
    Arena: TrArena<'alloc> + 'alloc,
    'alloc: 'a,
    T: TrDecodable<'alloc, Arena> + 'alloc,
    C: TrSeqSink<T> + 'a,
    C::Alloc: 'a,
    A: SeqAccess<'alloc, Arena> + 'a,
    A::Error: 'a,
    TyTok: TrCancellationToken,
{
    let mut out = C::empty_(alloc);
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
/// 与 [`SeqCollectVisitor`] 相同，它需要出现在公开的 GAT 关联类型里，并携带
/// 目标容器的 allocator 实例。
#[cfg(any(feature = "alloc-collections", feature = "std"))]
pub struct MapCollectVisitor<K, V, C: TrMapSink<K, V>> {
    alloc_: C::Alloc,
    marker_: PhantomData<(K, V, C)>,
}

#[cfg(any(feature = "alloc-collections", feature = "std"))]
impl<K, V, C: TrMapSink<K, V>> MapCollectVisitor<K, V, C> {
    /// 构造一个携带目标 allocator 的访问者。
    pub(crate) fn new_(alloc: C::Alloc) -> Self {
        Self {
            alloc_: alloc,
            marker_: PhantomData,
        }
    }
}

#[cfg(any(feature = "alloc-collections", feature = "std"))]
impl<'alloc, Alloc: TrArena<'alloc> + 'alloc, K, V, C> Visitor<'alloc, Alloc>
    for MapCollectVisitor<K, V, C>
where
    K: TrDecodable<'alloc, Alloc> + 'alloc,
    V: TrDecodable<'alloc, Alloc> + 'alloc,
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
        'alloc: 'a,
        Self: 'a,
        Self::Value: 'a,
        A: MapAccess<'alloc, Alloc> + 'a,
        A::Error: 'a,
    {
        DecodeMapCollectOpAsync::<'alloc, 'a, 'a, Alloc, K, V, C, A>::new(map, self.alloc_)
    }
}

#[cfg(any(feature = "alloc-collections", feature = "std"))]
#[gen_mcf2::gen_may_cancel_future(DecodeMapCollectOp, pub)]
async fn decode_map_collect_async_<'alloc, 'a, Arena, K, V, C, A, TyTok>(
    mut map: A,
    alloc: C::Alloc,
    cancel: TyTok,
) -> Result<C, A::Error>
where
    Arena: TrArena<'alloc> + 'alloc,
    'alloc: 'a,
    K: TrDecodable<'alloc, Arena> + 'alloc,
    V: TrDecodable<'alloc, Arena> + 'alloc,
    C: TrMapSink<K, V> + 'a,
    C::Alloc: 'a,
    A: MapAccess<'alloc, Arena> + 'a,
    A::Error: 'a,
    TyTok: TrCancellationToken,
{
    let mut out = C::empty_(alloc);
    while let Option::Some((key, value)) = map
        .next_entry::<K, V>()
        .may_cancel_with(cancel.child_token())
        .await?
    {
        out.insert_(key, value);
    }
    Result::Ok(out)
}
