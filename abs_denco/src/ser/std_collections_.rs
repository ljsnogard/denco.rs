//! `std::collections` 中的映射与集合的编码实现。
//!
//! 由 `std` feature 控制。`HashMap` / `HashSet` 的遍历顺序不确定，因此写出的
//! 字节顺序也不确定；这与 serde 的行为一致。
//!
//! 实现按 `A: Allocator` 泛化，自定义 hasher（`S`）与 allocator（`A`）都可用。

use core::alloc::Allocator;
use std::collections::{HashMap, HashSet, hash_map, hash_set};

use super::{
    TrEncodable, TrEncoder,
    ext_::{CollectMapOpAsync, CollectSeqOpAsync},
};

impl<K, V, TyHasher, A> TrEncodable for HashMap<K, V, TyHasher, A>
where
    K: TrEncodable,
    V: TrEncodable,
    A: Allocator,
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

impl<T, TyHasher, A> TrEncodable for HashSet<T, TyHasher, A>
where
    T: TrEncodable,
    A: Allocator,
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
