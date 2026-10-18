//! `alloc::collections` 容器的编码实现。
//!
//! 由 `alloc-collections` feature 控制。映射写成 MessagePack 映射，其余容器写成序列；
//! 元素只借用容器本身，因此编码依赖 [`TrEncodable`] 对 `&T` 的转发实现。

use alloc::collections::{
    BTreeMap, BTreeSet, BinaryHeap, LinkedList, VecDeque, binary_heap::Iter as BinaryHeapIter,
    btree_map, btree_set::Iter as BTreeSetIter, linked_list::Iter as LinkedListIter,
    vec_deque::Iter as VecDequeIter,
};

use super::{
    TrEncodable, TrEncoder,
    ext_::{CollectMapOpAsync, CollectSeqOpAsync},
};

/// 为「元素序列」型容器生成编码实现。
///
/// `$iter` 是容器 `iter()` 的返回类型（单标识符，便于在宏里追加生命周期与元素参数）。
macro_rules! impl_encode_seq_container_ {
    ($container:ty, $iter:ident) => {
        impl<T> TrEncodable for $container
        where
            T: TrEncodable,
        {
            type EncodeAsync<'a, S>
                = CollectSeqOpAsync<'a, 'a, S, $iter<'a, T>>
            where
                S: TrEncoder + 'a,
                S::Ok: 'a,
                S::Error: 'a,
                Self: 'a;

            fn encode_async<'a, S>(&'a self, encoder: S) -> Self::EncodeAsync<'a, S>
            where
                S: TrEncoder + 'a,
                S::Ok: 'a,
                S::Error: 'a,
                Self: 'a,
            {
                CollectSeqOpAsync::new(encoder, self.iter())
            }
        }
    };
}

impl_encode_seq_container_!(BTreeSet<T>, BTreeSetIter);
impl_encode_seq_container_!(VecDeque<T>, VecDequeIter);
impl_encode_seq_container_!(LinkedList<T>, LinkedListIter);
impl_encode_seq_container_!(BinaryHeap<T>, BinaryHeapIter);

impl<K, V> TrEncodable for BTreeMap<K, V>
where
    K: TrEncodable,
    V: TrEncodable,
{
    type EncodeAsync<'a, S>
        = CollectMapOpAsync<'a, 'a, S, &'a K, &'a V, btree_map::Iter<'a, K, V>>
    where
        S: TrEncoder + 'a,
        S::Ok: 'a,
        S::Error: 'a,
        Self: 'a;

    fn encode_async<'a, S>(&'a self, encoder: S) -> Self::EncodeAsync<'a, S>
    where
        S: TrEncoder + 'a,
        S::Ok: 'a,
        S::Error: 'a,
        Self: 'a,
    {
        CollectMapOpAsync::new(encoder, self.iter())
    }
}
