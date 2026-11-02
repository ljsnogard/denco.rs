//! `std::collections` 映射与集合的解码实现。
//!
//! 由 `std` feature 控制。解码用 [`sink_`] 的「空容器 + 逐个插入」构造：
//! `K: Eq + Hash`、`S: BuildHasher + Default`，allocator 由 [`TrAllocSource`]
//! 从 decoder 的 arena 投影得到。

use core::{
    alloc::Allocator,
    hash::{BuildHasher, Hash},
};
use std::collections::{HashMap, HashSet};

use super::{
    TrAllocSource, TrArena, TrDecodable, TrDecoder,
    collect_::{MapCollectVisitor, SeqCollectVisitor},
    sink_::{TrMapSink, TrSeqSink},
};

impl<K, V, TyHasher, CA> TrMapSink<K, V> for HashMap<K, V, TyHasher, CA>
where
    K: Eq + Hash,
    TyHasher: BuildHasher + Default,
    CA: Allocator,
{
    type Alloc = CA;

    fn empty_(alloc: CA) -> Self {
        HashMap::with_hasher_in(TyHasher::default(), alloc)
    }

    fn insert_(&mut self, key: K, value: V) {
        self.insert(key, value);
    }
}

impl<T, TyHasher, CA> TrSeqSink<T> for HashSet<T, TyHasher, CA>
where
    T: Eq + Hash,
    TyHasher: BuildHasher + Default,
    CA: Allocator,
{
    type Alloc = CA;

    fn empty_(alloc: CA) -> Self {
        HashSet::with_hasher_in(TyHasher::default(), alloc)
    }

    fn push_(&mut self, item: T) {
        self.insert(item);
    }
}

impl<'de, 'alloc, Alloc: TrArena<'alloc> + 'alloc, K, V, TyHasher, CA>
    TrDecodable<'de, 'alloc, Alloc> for HashMap<K, V, TyHasher, CA>
where
    K: TrDecodable<'de, 'alloc, Alloc> + 'de + Eq + Hash,
    V: TrDecodable<'de, 'alloc, Alloc> + 'de,
    TyHasher: BuildHasher + Default,
    CA: TrAllocSource<'alloc, Alloc> + 'alloc,
{
    type DecodeAsync<'a, D>
        = D::DecodeMapAsync<'a, MapCollectVisitor<K, V, HashMap<K, V, TyHasher, CA>>>
    where
        'de: 'a,
        'alloc: 'a,
        D: TrDecoder<'de, 'alloc, Alloc> + 'a,
        D::Error: 'a,
        Self: 'a;

    fn decode_async<'a, D>(de: D) -> Self::DecodeAsync<'a, D>
    where
        'de: 'a,
        'alloc: 'a,
        D: TrDecoder<'de, 'alloc, Alloc> + 'a,
        D::Error: 'a,
        Self: 'a,
    {
        let alloc = CA::alloc_from(de.allocator());
        de.decode_map(MapCollectVisitor::<K, V, Self>::new_(alloc))
    }
}

impl<'de, 'alloc, Alloc: TrArena<'alloc> + 'alloc, T, TyHasher, CA> TrDecodable<'de, 'alloc, Alloc>
    for HashSet<T, TyHasher, CA>
where
    T: TrDecodable<'de, 'alloc, Alloc> + 'de + Eq + Hash,
    TyHasher: BuildHasher + Default,
    CA: TrAllocSource<'alloc, Alloc> + 'alloc,
{
    type DecodeAsync<'a, D>
        = D::DecodeSeqAsync<'a, SeqCollectVisitor<T, HashSet<T, TyHasher, CA>>>
    where
        'de: 'a,
        'alloc: 'a,
        D: TrDecoder<'de, 'alloc, Alloc> + 'a,
        D::Error: 'a,
        Self: 'a;

    fn decode_async<'a, D>(de: D) -> Self::DecodeAsync<'a, D>
    where
        'de: 'a,
        'alloc: 'a,
        D: TrDecoder<'de, 'alloc, Alloc> + 'a,
        D::Error: 'a,
        Self: 'a,
    {
        let alloc = CA::alloc_from(de.allocator());
        de.decode_seq(SeqCollectVisitor::<T, Self>::new_(alloc))
    }
}
