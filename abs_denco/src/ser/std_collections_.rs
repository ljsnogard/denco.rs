//! `std::collections` 中的映射与集合的编码实现。
//!
//! 由 `std` feature 控制。`HashMap` / `HashSet` 的遍历顺序不确定，因此写出的
//! 字节顺序也不确定；这与 serde 的行为一致。

use std::collections::{HashMap, HashSet, hash_map, hash_set};

use super::{
    TrEncodable, TrEncoder,
    ext_::{CollectMapOpAsync, CollectSeqOpAsync},
};

impl<K, V, TyHasher> TrEncodable for HashMap<K, V, TyHasher>
where
    K: TrEncodable,
    V: TrEncodable,
{
    type EncodeAsync<'a, S>
        = CollectMapOpAsync<'a, 'a, S, &'a K, &'a V, hash_map::Iter<'a, K, V>>
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

impl<T, TyHasher> TrEncodable for HashSet<T, TyHasher>
where
    T: TrEncodable,
{
    type EncodeAsync<'a, S>
        = CollectSeqOpAsync<'a, 'a, S, hash_set::Iter<'a, T>>
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
