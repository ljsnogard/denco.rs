//! `alloc::collections` 容器的解码实现。
//!
//! 由 `alloc-collections` feature 控制。所有容器都复用 [`collect_`] 里的
//! 泛型 visitor；构造走 [`sink_`] 的「空容器 + 逐个插入」，因为非 `Global`
//! allocator 的容器没有 `FromIterator` 实现。
//!
//! allocator 实例由 [`TrAllocSource`] 从 decoder 的 arena 投影得到：
//! `Vec<T, &'alloc A>` 共享 decoder 的 arena，`Vec<T, Global>` 与 arena 无关。
//! `BTreeMap` / `BTreeSet` 额外要求 allocator 实现 `AllocatorClone`。

use alloc::collections::{BTreeMap, BTreeSet, BinaryHeap, LinkedList, VecDeque};
use core::alloc::{Allocator, AllocatorClone};

use super::{
    TrAllocSource, TrArena, TrDecodable, TrDecoder,
    collect_::{MapCollectVisitor, SeqCollectVisitor},
    sink_::{TrMapSink, TrSeqSink},
};

//-- ---- 构造器：空容器 + 逐个插入 --//

impl<T, CA> TrSeqSink<T> for VecDeque<T, CA>
where
    CA: Allocator,
{
    type Alloc = CA;

    fn empty_(alloc: CA) -> Self {
        VecDeque::new_in(alloc)
    }

    fn push_(&mut self, item: T) {
        self.push_back(item);
    }
}

impl<T, CA> TrSeqSink<T> for LinkedList<T, CA>
where
    CA: Allocator,
{
    type Alloc = CA;

    fn empty_(alloc: CA) -> Self {
        LinkedList::new_in(alloc)
    }

    fn push_(&mut self, item: T) {
        self.push_back(item);
    }
}

impl<T, CA> TrSeqSink<T> for BinaryHeap<T, CA>
where
    T: Ord,
    CA: Allocator,
{
    type Alloc = CA;

    fn empty_(alloc: CA) -> Self {
        BinaryHeap::new_in(alloc)
    }

    fn push_(&mut self, item: T) {
        BinaryHeap::push(self, item);
    }
}

impl<T, CA> TrSeqSink<T> for BTreeSet<T, CA>
where
    T: Ord,
    CA: AllocatorClone,
{
    type Alloc = CA;

    fn empty_(alloc: CA) -> Self {
        BTreeSet::new_in(alloc)
    }

    fn push_(&mut self, item: T) {
        self.insert(item);
    }
}

impl<K, V, CA> TrMapSink<K, V> for BTreeMap<K, V, CA>
where
    K: Ord,
    CA: AllocatorClone,
{
    type Alloc = CA;

    fn empty_(alloc: CA) -> Self {
        BTreeMap::new_in(alloc)
    }

    fn insert_(&mut self, key: K, value: V) {
        self.insert(key, value);
    }
}

//-- ---- 解码实现 --//

impl<'alloc, Alloc: TrArena<'alloc> + 'alloc, T, CA> TrDecodable<'alloc, Alloc> for VecDeque<T, CA>
where
    T: TrDecodable<'alloc, Alloc> + 'alloc,
    CA: TrAllocSource<'alloc, Alloc> + 'alloc,
{
    type DecodeAsync<'a, D>
        = D::DecodeSeqAsync<'a, SeqCollectVisitor<T, VecDeque<T, CA>>>
    where
        'alloc: 'a,
        D: TrDecoder<'alloc, Alloc> + 'a,
        D::Error: 'a,
        Self: 'a;

    fn decode_async<'a, D>(de: D) -> Self::DecodeAsync<'a, D>
    where
        'alloc: 'a,
        D: TrDecoder<'alloc, Alloc> + 'a,
        D::Error: 'a,
        Self: 'a,
    {
        let alloc = CA::alloc_from(de.allocator());
        de.decode_seq(SeqCollectVisitor::<T, Self>::new_(alloc))
    }
}

impl<'alloc, Alloc: TrArena<'alloc> + 'alloc, T, CA> TrDecodable<'alloc, Alloc>
    for LinkedList<T, CA>
where
    T: TrDecodable<'alloc, Alloc> + 'alloc,
    CA: TrAllocSource<'alloc, Alloc> + 'alloc,
{
    type DecodeAsync<'a, D>
        = D::DecodeSeqAsync<'a, SeqCollectVisitor<T, LinkedList<T, CA>>>
    where
        'alloc: 'a,
        D: TrDecoder<'alloc, Alloc> + 'a,
        D::Error: 'a,
        Self: 'a;

    fn decode_async<'a, D>(de: D) -> Self::DecodeAsync<'a, D>
    where
        'alloc: 'a,
        D: TrDecoder<'alloc, Alloc> + 'a,
        D::Error: 'a,
        Self: 'a,
    {
        let alloc = CA::alloc_from(de.allocator());
        de.decode_seq(SeqCollectVisitor::<T, Self>::new_(alloc))
    }
}

impl<'alloc, Alloc: TrArena<'alloc> + 'alloc, T, CA> TrDecodable<'alloc, Alloc>
    for BinaryHeap<T, CA>
where
    T: TrDecodable<'alloc, Alloc> + 'alloc + Ord,
    CA: TrAllocSource<'alloc, Alloc> + 'alloc,
{
    type DecodeAsync<'a, D>
        = D::DecodeSeqAsync<'a, SeqCollectVisitor<T, BinaryHeap<T, CA>>>
    where
        'alloc: 'a,
        D: TrDecoder<'alloc, Alloc> + 'a,
        D::Error: 'a,
        Self: 'a;

    fn decode_async<'a, D>(de: D) -> Self::DecodeAsync<'a, D>
    where
        'alloc: 'a,
        D: TrDecoder<'alloc, Alloc> + 'a,
        D::Error: 'a,
        Self: 'a,
    {
        let alloc = CA::alloc_from(de.allocator());
        de.decode_seq(SeqCollectVisitor::<T, Self>::new_(alloc))
    }
}

impl<'alloc, Alloc: TrArena<'alloc> + 'alloc, T, CA> TrDecodable<'alloc, Alloc> for BTreeSet<T, CA>
where
    T: TrDecodable<'alloc, Alloc> + 'alloc + Ord,
    CA: TrAllocSource<'alloc, Alloc> + AllocatorClone + 'alloc,
{
    type DecodeAsync<'a, D>
        = D::DecodeSeqAsync<'a, SeqCollectVisitor<T, BTreeSet<T, CA>>>
    where
        'alloc: 'a,
        D: TrDecoder<'alloc, Alloc> + 'a,
        D::Error: 'a,
        Self: 'a;

    fn decode_async<'a, D>(de: D) -> Self::DecodeAsync<'a, D>
    where
        'alloc: 'a,
        D: TrDecoder<'alloc, Alloc> + 'a,
        D::Error: 'a,
        Self: 'a,
    {
        let alloc = CA::alloc_from(de.allocator());
        de.decode_seq(SeqCollectVisitor::<T, Self>::new_(alloc))
    }
}

impl<'alloc, Alloc: TrArena<'alloc> + 'alloc, K, V, CA> TrDecodable<'alloc, Alloc>
    for BTreeMap<K, V, CA>
where
    K: TrDecodable<'alloc, Alloc> + 'alloc + Ord,
    V: TrDecodable<'alloc, Alloc> + 'alloc,
    CA: TrAllocSource<'alloc, Alloc> + AllocatorClone + 'alloc,
{
    type DecodeAsync<'a, D>
        = D::DecodeMapAsync<'a, MapCollectVisitor<K, V, BTreeMap<K, V, CA>>>
    where
        'alloc: 'a,
        D: TrDecoder<'alloc, Alloc> + 'a,
        D::Error: 'a,
        Self: 'a;

    fn decode_async<'a, D>(de: D) -> Self::DecodeAsync<'a, D>
    where
        'alloc: 'a,
        D: TrDecoder<'alloc, Alloc> + 'a,
        D::Error: 'a,
        Self: 'a,
    {
        let alloc = CA::alloc_from(de.allocator());
        de.decode_map(MapCollectVisitor::<K, V, Self>::new_(alloc))
    }
}
