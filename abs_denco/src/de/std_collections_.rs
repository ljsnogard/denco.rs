//! `std::collections` 映射与集合的解码实现。
//!
//! 由 `std` feature 控制。解码用 [`sink_`] 的「空容器 + 逐个插入」构造：
//! `K: Eq + Hash`、`S: BuildHasher + Default`，allocator 由 `A: Default` 提供。

use core::{
    alloc::Allocator,
    hash::{BuildHasher, Hash},
};
use std::collections::{HashMap, HashSet};

use super::{
    TrDecodable, TrDecoder,
    collect_::{MapCollectVisitor, SeqCollectVisitor},
    sink_::{TrMapSink, TrSeqSink},
};

impl<K, V, TyHasher, A> TrMapSink<K, V> for HashMap<K, V, TyHasher, A>
where
    K: Eq + Hash,
    TyHasher: BuildHasher + Default,
    A: Allocator + Default,
{
    fn empty_() -> Self {
        HashMap::with_hasher_in(TyHasher::default(), A::default())
    }

    fn insert_(&mut self, key: K, value: V) {
        self.insert(key, value);
    }
}

impl<T, TyHasher, A> TrSeqSink<T> for HashSet<T, TyHasher, A>
where
    T: Eq + Hash,
    TyHasher: BuildHasher + Default,
    A: Allocator + Default,
{
    fn empty_() -> Self {
        HashSet::with_hasher_in(TyHasher::default(), A::default())
    }

    fn push_(&mut self, item: T) {
        self.insert(item);
    }
}

impl<'de, K, V, TyHasher, A> TrDecodable<'de> for HashMap<K, V, TyHasher, A>
where
    K: TrDecodable<'de> + 'de + Eq + Hash,
    V: TrDecodable<'de> + 'de,
    TyHasher: BuildHasher + Default,
    A: Allocator + Default,
{
    type DecodeAsync<'a, D>
        = D::DecodeMapAsync<'a, MapCollectVisitor<K, V, HashMap<K, V, TyHasher, A>>>
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

impl<'de, T, TyHasher, A> TrDecodable<'de> for HashSet<T, TyHasher, A>
where
    T: TrDecodable<'de> + 'de + Eq + Hash,
    TyHasher: BuildHasher + Default,
    A: Allocator + Default,
{
    type DecodeAsync<'a, D>
        = D::DecodeSeqAsync<'a, SeqCollectVisitor<T, HashSet<T, TyHasher, A>>>
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
