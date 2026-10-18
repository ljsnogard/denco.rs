//! `alloc::collections` 容器的解码实现。
//!
//! 由 `alloc-collections` feature 控制。所有容器都复用 [`collect_`] 里的
//! 泛型 visitor；构造走 [`sink_`] 的「空容器 + 逐个插入」，因为非 `Global`
//! allocator 的容器没有 `FromIterator` 实现。
//!
//! `A: Default` 用来提供解码时的 allocator 实例；`BTreeMap` / `BTreeSet`
//! 额外要求 `A: AllocatorClone`。

use alloc::collections::{BTreeMap, BTreeSet, BinaryHeap, LinkedList, VecDeque};
use core::alloc::{Allocator, AllocatorClone};

use super::{
    TrDecodable, TrDecoder,
    collect_::{MapCollectVisitor, SeqCollectVisitor},
    sink_::{TrMapSink, TrSeqSink},
};

//-- ---- 构造器：空容器 + 逐个插入 --//

impl<T, A> TrSeqSink<T> for VecDeque<T, A>
where
    A: Allocator + Default,
{
    fn empty_() -> Self {
        VecDeque::new_in(A::default())
    }

    fn push_(&mut self, item: T) {
        self.push_back(item);
    }
}

impl<T, A> TrSeqSink<T> for LinkedList<T, A>
where
    A: Allocator + Default,
{
    fn empty_() -> Self {
        LinkedList::new_in(A::default())
    }

    fn push_(&mut self, item: T) {
        self.push_back(item);
    }
}

impl<T, A> TrSeqSink<T> for BinaryHeap<T, A>
where
    T: Ord,
    A: Allocator + Default,
{
    fn empty_() -> Self {
        BinaryHeap::new_in(A::default())
    }

    fn push_(&mut self, item: T) {
        BinaryHeap::push(self, item);
    }
}

impl<T, A> TrSeqSink<T> for BTreeSet<T, A>
where
    T: Ord,
    A: AllocatorClone + Default,
{
    fn empty_() -> Self {
        BTreeSet::new_in(A::default())
    }

    fn push_(&mut self, item: T) {
        self.insert(item);
    }
}

impl<K, V, A> TrMapSink<K, V> for BTreeMap<K, V, A>
where
    K: Ord,
    A: AllocatorClone + Default,
{
    fn empty_() -> Self {
        BTreeMap::new_in(A::default())
    }

    fn insert_(&mut self, key: K, value: V) {
        self.insert(key, value);
    }
}

//-- ---- 解码实现 --//

impl<'de, T, A> TrDecodable<'de> for VecDeque<T, A>
where
    T: TrDecodable<'de> + 'de,
    A: Allocator + Default,
{
    type DecodeAsync<'a, D>
        = D::DecodeSeqAsync<'a, SeqCollectVisitor<T, VecDeque<T, A>>>
    where
        'de: 'a,
        D: TrDecoder<'de> + 'a,
        D::Error: 'a,
        Self: 'a;

    fn decode_async<'a, D>(de: D) -> Self::DecodeAsync<'a, D>
    where
        'de: 'a,
        D: TrDecoder<'de> + 'a,
        D::Error: 'a,
        Self: 'a,
    {
        de.decode_seq(SeqCollectVisitor::<T, Self>::new_())
    }
}

impl<'de, T, A> TrDecodable<'de> for LinkedList<T, A>
where
    T: TrDecodable<'de> + 'de,
    A: Allocator + Default,
{
    type DecodeAsync<'a, D>
        = D::DecodeSeqAsync<'a, SeqCollectVisitor<T, LinkedList<T, A>>>
    where
        'de: 'a,
        D: TrDecoder<'de> + 'a,
        D::Error: 'a,
        Self: 'a;

    fn decode_async<'a, D>(de: D) -> Self::DecodeAsync<'a, D>
    where
        'de: 'a,
        D: TrDecoder<'de> + 'a,
        D::Error: 'a,
        Self: 'a,
    {
        de.decode_seq(SeqCollectVisitor::<T, Self>::new_())
    }
}

impl<'de, T, A> TrDecodable<'de> for BinaryHeap<T, A>
where
    T: TrDecodable<'de> + 'de + Ord,
    A: Allocator + Default,
{
    type DecodeAsync<'a, D>
        = D::DecodeSeqAsync<'a, SeqCollectVisitor<T, BinaryHeap<T, A>>>
    where
        'de: 'a,
        D: TrDecoder<'de> + 'a,
        D::Error: 'a,
        Self: 'a;

    fn decode_async<'a, D>(de: D) -> Self::DecodeAsync<'a, D>
    where
        'de: 'a,
        D: TrDecoder<'de> + 'a,
        D::Error: 'a,
        Self: 'a,
    {
        de.decode_seq(SeqCollectVisitor::<T, Self>::new_())
    }
}

impl<'de, T, A> TrDecodable<'de> for BTreeSet<T, A>
where
    T: TrDecodable<'de> + 'de + Ord,
    A: AllocatorClone + Default,
{
    type DecodeAsync<'a, D>
        = D::DecodeSeqAsync<'a, SeqCollectVisitor<T, BTreeSet<T, A>>>
    where
        'de: 'a,
        D: TrDecoder<'de> + 'a,
        D::Error: 'a,
        Self: 'a;

    fn decode_async<'a, D>(de: D) -> Self::DecodeAsync<'a, D>
    where
        'de: 'a,
        D: TrDecoder<'de> + 'a,
        D::Error: 'a,
        Self: 'a,
    {
        de.decode_seq(SeqCollectVisitor::<T, Self>::new_())
    }
}

impl<'de, K, V, A> TrDecodable<'de> for BTreeMap<K, V, A>
where
    K: TrDecodable<'de> + 'de + Ord,
    V: TrDecodable<'de> + 'de,
    A: AllocatorClone + Default,
{
    type DecodeAsync<'a, D>
        = D::DecodeMapAsync<'a, MapCollectVisitor<K, V, BTreeMap<K, V, A>>>
    where
        'de: 'a,
        D: TrDecoder<'de> + 'a,
        D::Error: 'a,
        Self: 'a;

    fn decode_async<'a, D>(de: D) -> Self::DecodeAsync<'a, D>
    where
        'de: 'a,
        D: TrDecoder<'de> + 'a,
        D::Error: 'a,
        Self: 'a,
    {
        de.decode_map(MapCollectVisitor::<K, V, Self>::new_())
    }
}
