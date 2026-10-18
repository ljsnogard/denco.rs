//! `std::collections` 映射与集合的解码实现。
//!
//! 由 `std` feature 控制。解码需要容器的 `FromIterator` 约束：`K: Eq + Hash`，
//! 且 hasher 可默认构造（`BuildHasher + Default`）。

use core::hash::{BuildHasher, Hash};
use std::collections::{HashMap, HashSet};

use super::{
    TrDecodable, TrDecoder,
    collect_::{MapCollectVisitor, SeqCollectVisitor},
};

impl<'de, K, V, TyHasher> TrDecodable<'de> for HashMap<K, V, TyHasher>
where
    K: TrDecodable<'de> + 'de + Eq + Hash,
    V: TrDecodable<'de> + 'de,
    TyHasher: BuildHasher + Default,
{
    type DecodeAsync<'a, D>
        = D::DecodeMapAsync<'a, MapCollectVisitor<K, V, HashMap<K, V, TyHasher>>>
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

impl<'de, T, TyHasher> TrDecodable<'de> for HashSet<T, TyHasher>
where
    T: TrDecodable<'de> + 'de + Eq + Hash,
    TyHasher: BuildHasher + Default,
{
    type DecodeAsync<'a, D>
        = D::DecodeSeqAsync<'a, SeqCollectVisitor<T, HashSet<T, TyHasher>>>
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
