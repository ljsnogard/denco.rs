//! `alloc::collections` 容器的解码实现。
//!
//! 由 `alloc-collections` feature 控制。所有容器都复用 [`collect_`] 里的
//! 泛型 visitor，这里只负责把「容器自己的约束」（`Ord` 等）写清楚。

use alloc::collections::{BTreeMap, BTreeSet, BinaryHeap, LinkedList, VecDeque};

use super::{
    TrDecodable, TrDecoder,
    collect_::{MapCollectVisitor, SeqCollectVisitor},
};

impl<'de, T> TrDecodable<'de> for VecDeque<T>
where
    T: TrDecodable<'de> + 'de,
{
    type DecodeAsync<'a, D>
        = D::DecodeSeqAsync<'a, SeqCollectVisitor<T, VecDeque<T>>>
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

impl<'de, T> TrDecodable<'de> for LinkedList<T>
where
    T: TrDecodable<'de> + 'de,
{
    type DecodeAsync<'a, D>
        = D::DecodeSeqAsync<'a, SeqCollectVisitor<T, LinkedList<T>>>
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

impl<'de, T> TrDecodable<'de> for BTreeSet<T>
where
    T: TrDecodable<'de> + 'de + Ord,
{
    type DecodeAsync<'a, D>
        = D::DecodeSeqAsync<'a, SeqCollectVisitor<T, BTreeSet<T>>>
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

impl<'de, T> TrDecodable<'de> for BinaryHeap<T>
where
    T: TrDecodable<'de> + 'de + Ord,
{
    type DecodeAsync<'a, D>
        = D::DecodeSeqAsync<'a, SeqCollectVisitor<T, BinaryHeap<T>>>
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

impl<'de, K, V> TrDecodable<'de> for BTreeMap<K, V>
where
    K: TrDecodable<'de> + 'de + Ord,
    V: TrDecodable<'de> + 'de,
{
    type DecodeAsync<'a, D>
        = D::DecodeMapAsync<'a, MapCollectVisitor<K, V, BTreeMap<K, V>>>
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
