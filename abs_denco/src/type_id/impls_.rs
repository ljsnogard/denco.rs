//! `abs_denco` 内置编解码类型的 `TrHasTypeId` 实现。
//!
//! 容器按**线上等价**归一：`String` 与 `str` 同 ID，`Vec`/切片/队列同 ID，
//! `BTreeMap` 与 `HashMap` 同 ID，`Box`/`Rc`/`Arc`/`Cow`/引用转发到内层。
//!
//! 三个宽度使用各自的原生哈希与字段 ID：32 位 schema 只写字段的
//! `TYPE_ID_32`，64 位只写 `TYPE_ID_64`，128 位只写 `TYPE_ID_128`。

use core::alloc::Allocator;

use alloc::{boxed::Box, string::String, vec::Vec};
#[cfg(feature = "std")]
use std::collections::{HashMap, HashSet};

use super::TrHasTypeId;
use super::hasher::{Hasher32, Hasher64, Hasher128};
use super::type_id_::tags;

//-- ---- 32 位辅助 ------------------------------------------------------------

/// 无载荷 schema 的 32 位 ID。
const fn scalar_id_32_(tag: u8) -> u32 {
    let hasher = Hasher32::new(2);
    let hasher = hasher.u8(tag);
    hasher.finish()
}

/// 一元容器 schema 的 32 位 ID。
const fn unary_id_32_(tag: u8, inner: u32) -> u32 {
    let hasher = Hasher32::new(2 + 5);
    let hasher = hasher.u8(tag);
    let hasher = hasher.id(inner);
    hasher.finish()
}

/// 二元容器 schema 的 32 位 ID。
#[cfg(feature = "alloc-collections")]
const fn binary_id_32_(tag: u8, first: u32, second: u32) -> u32 {
    let hasher = Hasher32::new(2 + 5 + 5);
    let hasher = hasher.u8(tag);
    let hasher = hasher.id(first);
    let hasher = hasher.id(second);
    hasher.finish()
}

/// 定长数组 schema 的 32 位 ID。
const fn array_id_32_(tag: u8, inner: u32, len: u64) -> u32 {
    let hasher = Hasher32::new(2 + 5 + 9);
    let hasher = hasher.u8(tag);
    let hasher = hasher.id(inner);
    let hasher = hasher.u64(len);
    hasher.finish()
}

//-- ---- 64 位辅助 ------------------------------------------------------------

/// 无载荷 schema 的 64 位 ID。
const fn scalar_id_64_(tag: u8) -> u64 {
    let hasher = Hasher64::new(2);
    let hasher = hasher.u8(tag);
    hasher.finish()
}

/// 一元容器 schema 的 64 位 ID。
const fn unary_id_64_(tag: u8, inner: u64) -> u64 {
    let hasher = Hasher64::new(2 + 9);
    let hasher = hasher.u8(tag);
    let hasher = hasher.id(inner);
    hasher.finish()
}

/// 二元容器 schema 的 64 位 ID。
#[cfg(feature = "alloc-collections")]
const fn binary_id_64_(tag: u8, first: u64, second: u64) -> u64 {
    let hasher = Hasher64::new(2 + 9 + 9);
    let hasher = hasher.u8(tag);
    let hasher = hasher.id(first);
    let hasher = hasher.id(second);
    hasher.finish()
}

/// 定长数组 schema 的 64 位 ID。
const fn array_id_64_(tag: u8, inner: u64, len: u64) -> u64 {
    let hasher = Hasher64::new(2 + 9 + 9);
    let hasher = hasher.u8(tag);
    let hasher = hasher.id(inner);
    let hasher = hasher.u64(len);
    hasher.finish()
}

//-- ---- 128 位辅助 -----------------------------------------------------------

/// 无载荷 schema 的 128 位 ID。
const fn scalar_id_128_(tag: u8) -> u128 {
    let hasher = Hasher128::new(2);
    let hasher = hasher.u8(tag);
    hasher.finish()
}

/// 一元容器 schema 的 128 位 ID。
const fn unary_id_128_(tag: u8, inner: u128) -> u128 {
    let hasher = Hasher128::new(2 + 17);
    let hasher = hasher.u8(tag);
    let hasher = hasher.id(inner);
    hasher.finish()
}

/// 二元容器 schema 的 128 位 ID。
#[cfg(feature = "alloc-collections")]
const fn binary_id_128_(tag: u8, first: u128, second: u128) -> u128 {
    let hasher = Hasher128::new(2 + 17 + 17);
    let hasher = hasher.u8(tag);
    let hasher = hasher.id(first);
    let hasher = hasher.id(second);
    hasher.finish()
}

/// 定长数组 schema 的 128 位 ID。
const fn array_id_128_(tag: u8, inner: u128, len: u64) -> u128 {
    let hasher = Hasher128::new(2 + 17 + 9);
    let hasher = hasher.u8(tag);
    let hasher = hasher.id(inner);
    let hasher = hasher.u64(len);
    hasher.finish()
}

//-- ---- 标量与内置类型 -------------------------------------------------------

macro_rules! impl_scalar_ {
    ($ty:ty, $tag:ident) => {
        impl TrHasTypeId for $ty {
            const TYPE_ID_128: u128 = scalar_id_128_(tags::$tag);
            const TYPE_ID_64: u64 = scalar_id_64_(tags::$tag);
            const TYPE_ID_32: u32 = scalar_id_32_(tags::$tag);
            const TYPE_NAME: &'static str = stringify!($ty);
        }
    };
}

impl_scalar_!(bool, BOOL);
impl_scalar_!(i8, I8);
impl_scalar_!(i16, I16);
impl_scalar_!(i32, I32);
impl_scalar_!(i64, I64);
impl_scalar_!(i128, I128);
impl_scalar_!(u8, U8);
impl_scalar_!(u16, U16);
impl_scalar_!(u32, U32);
impl_scalar_!(u64, U64);
impl_scalar_!(u128, U128);
impl_scalar_!(f32, F32);
impl_scalar_!(f64, F64);
impl_scalar_!(char, CHAR);

impl TrHasTypeId for () {
    const TYPE_ID_128: u128 = scalar_id_128_(tags::UNIT);
    const TYPE_ID_64: u64 = scalar_id_64_(tags::UNIT);
    const TYPE_ID_32: u32 = scalar_id_32_(tags::UNIT);
    const TYPE_NAME: &'static str = "()";
}

impl TrHasTypeId for str {
    const TYPE_ID_128: u128 = scalar_id_128_(tags::STR);
    const TYPE_ID_64: u64 = scalar_id_64_(tags::STR);
    const TYPE_ID_32: u32 = scalar_id_32_(tags::STR);
    const TYPE_NAME: &'static str = "str";
}

impl TrHasTypeId for String {
    const TYPE_ID_128: u128 = <str as TrHasTypeId>::TYPE_ID_128;
    const TYPE_ID_64: u64 = <str as TrHasTypeId>::TYPE_ID_64;
    const TYPE_ID_32: u32 = <str as TrHasTypeId>::TYPE_ID_32;
    const TYPE_NAME: &'static str = "String";
}

#[cfg(feature = "str_alloc")]
impl<A: Allocator> TrHasTypeId for abs_str::str_alloc::String<A> {
    const TYPE_ID_128: u128 = <str as TrHasTypeId>::TYPE_ID_128;
    const TYPE_ID_64: u64 = <str as TrHasTypeId>::TYPE_ID_64;
    const TYPE_ID_32: u32 = <str as TrHasTypeId>::TYPE_ID_32;
    const TYPE_NAME: &'static str = "abs_str::String";
}

impl<T: ?Sized + TrHasTypeId> TrHasTypeId for &T {
    const TYPE_ID_128: u128 = T::TYPE_ID_128;
    const TYPE_ID_64: u64 = T::TYPE_ID_64;
    const TYPE_ID_32: u32 = T::TYPE_ID_32;
    const TYPE_NAME: &'static str = T::TYPE_NAME;
}

impl<T: ?Sized + TrHasTypeId> TrHasTypeId for &mut T {
    const TYPE_ID_128: u128 = T::TYPE_ID_128;
    const TYPE_ID_64: u64 = T::TYPE_ID_64;
    const TYPE_ID_32: u32 = T::TYPE_ID_32;
    const TYPE_NAME: &'static str = T::TYPE_NAME;
}

impl<T: ?Sized + TrHasTypeId, A: Allocator> TrHasTypeId for Box<T, A> {
    const TYPE_ID_128: u128 = T::TYPE_ID_128;
    const TYPE_ID_64: u64 = T::TYPE_ID_64;
    const TYPE_ID_32: u32 = T::TYPE_ID_32;
    const TYPE_NAME: &'static str = T::TYPE_NAME;
}

#[cfg(feature = "rc")]
impl<T: ?Sized + TrHasTypeId, A: Allocator> TrHasTypeId for alloc::rc::Rc<T, A> {
    const TYPE_ID_128: u128 = T::TYPE_ID_128;
    const TYPE_ID_64: u64 = T::TYPE_ID_64;
    const TYPE_ID_32: u32 = T::TYPE_ID_32;
    const TYPE_NAME: &'static str = T::TYPE_NAME;
}

#[cfg(target_has_atomic = "ptr")]
impl<T: ?Sized + TrHasTypeId, A: Allocator> TrHasTypeId for alloc::sync::Arc<T, A> {
    const TYPE_ID_128: u128 = T::TYPE_ID_128;
    const TYPE_ID_64: u64 = T::TYPE_ID_64;
    const TYPE_ID_32: u32 = T::TYPE_ID_32;
    const TYPE_NAME: &'static str = T::TYPE_NAME;
}

#[cfg(feature = "cow")]
impl<'a, T: ?Sized + alloc::borrow::ToOwned + TrHasTypeId> TrHasTypeId
    for alloc::borrow::Cow<'a, T>
{
    const TYPE_ID_128: u128 = T::TYPE_ID_128;
    const TYPE_ID_64: u64 = T::TYPE_ID_64;
    const TYPE_ID_32: u32 = T::TYPE_ID_32;
    const TYPE_NAME: &'static str = T::TYPE_NAME;
}

//-- ---- 容器 ----------------------------------------------------------------

impl<T: TrHasTypeId> TrHasTypeId for Option<T> {
    const TYPE_ID_128: u128 = unary_id_128_(tags::OPTION, T::TYPE_ID_128);
    const TYPE_ID_64: u64 = unary_id_64_(tags::OPTION, T::TYPE_ID_64);
    const TYPE_ID_32: u32 = unary_id_32_(tags::OPTION, T::TYPE_ID_32);
    const TYPE_NAME: &'static str = "Option";
}

impl<T: TrHasTypeId, A: Allocator> TrHasTypeId for Vec<T, A> {
    const TYPE_ID_128: u128 = unary_id_128_(tags::SEQ, T::TYPE_ID_128);
    const TYPE_ID_64: u64 = unary_id_64_(tags::SEQ, T::TYPE_ID_64);
    const TYPE_ID_32: u32 = unary_id_32_(tags::SEQ, T::TYPE_ID_32);
    const TYPE_NAME: &'static str = "Vec";
}

impl<T: TrHasTypeId> TrHasTypeId for [T] {
    const TYPE_ID_128: u128 = <Vec<T> as TrHasTypeId>::TYPE_ID_128;
    const TYPE_ID_64: u64 = <Vec<T> as TrHasTypeId>::TYPE_ID_64;
    const TYPE_ID_32: u32 = <Vec<T> as TrHasTypeId>::TYPE_ID_32;
    const TYPE_NAME: &'static str = "Vec";
}

impl<T: TrHasTypeId, const N: usize> TrHasTypeId for [T; N] {
    const TYPE_ID_128: u128 = array_id_128_(tags::ARRAY, T::TYPE_ID_128, N as u64);
    const TYPE_ID_64: u64 = array_id_64_(tags::ARRAY, T::TYPE_ID_64, N as u64);
    const TYPE_ID_32: u32 = array_id_32_(tags::ARRAY, T::TYPE_ID_32, N as u64);
    const TYPE_NAME: &'static str = "[T; N]";
}

#[cfg(feature = "alloc-collections")]
macro_rules! impl_seq_container_ {
    ($container:ty, $name:expr) => {
        impl<T: TrHasTypeId, A: Allocator> TrHasTypeId for $container {
            const TYPE_ID_128: u128 = unary_id_128_(tags::SEQ, T::TYPE_ID_128);
            const TYPE_ID_64: u64 = unary_id_64_(tags::SEQ, T::TYPE_ID_64);
            const TYPE_ID_32: u32 = unary_id_32_(tags::SEQ, T::TYPE_ID_32);
            const TYPE_NAME: &'static str = $name;
        }
    };
}

#[cfg(feature = "alloc-collections")]
impl_seq_container_!(alloc::collections::VecDeque<T, A>, "VecDeque");
#[cfg(feature = "alloc-collections")]
impl_seq_container_!(alloc::collections::LinkedList<T, A>, "LinkedList");
#[cfg(feature = "alloc-collections")]
impl_seq_container_!(alloc::collections::BinaryHeap<T, A>, "BinaryHeap");

#[cfg(feature = "alloc-collections")]
impl<T: TrHasTypeId, A: Allocator + core::alloc::AllocatorClone> TrHasTypeId
    for alloc::collections::BTreeSet<T, A>
{
    const TYPE_ID_128: u128 = unary_id_128_(tags::SET, T::TYPE_ID_128);
    const TYPE_ID_64: u64 = unary_id_64_(tags::SET, T::TYPE_ID_64);
    const TYPE_ID_32: u32 = unary_id_32_(tags::SET, T::TYPE_ID_32);
    const TYPE_NAME: &'static str = "BTreeSet";
}

#[cfg(feature = "alloc-collections")]
impl<K: TrHasTypeId, V: TrHasTypeId, A: Allocator + core::alloc::AllocatorClone> TrHasTypeId
    for alloc::collections::BTreeMap<K, V, A>
{
    const TYPE_ID_128: u128 = binary_id_128_(tags::MAP, K::TYPE_ID_128, V::TYPE_ID_128);
    const TYPE_ID_64: u64 = binary_id_64_(tags::MAP, K::TYPE_ID_64, V::TYPE_ID_64);
    const TYPE_ID_32: u32 = binary_id_32_(tags::MAP, K::TYPE_ID_32, V::TYPE_ID_32);
    const TYPE_NAME: &'static str = "BTreeMap";
}

#[cfg(feature = "std")]
impl<T: TrHasTypeId, TyHasher, A: Allocator> TrHasTypeId for HashSet<T, TyHasher, A> {
    const TYPE_ID_128: u128 = unary_id_128_(tags::SET, T::TYPE_ID_128);
    const TYPE_ID_64: u64 = unary_id_64_(tags::SET, T::TYPE_ID_64);
    const TYPE_ID_32: u32 = unary_id_32_(tags::SET, T::TYPE_ID_32);
    const TYPE_NAME: &'static str = "HashSet";
}

#[cfg(feature = "std")]
impl<K: TrHasTypeId, V: TrHasTypeId, TyHasher, A: Allocator> TrHasTypeId
    for HashMap<K, V, TyHasher, A>
{
    const TYPE_ID_128: u128 = binary_id_128_(tags::MAP, K::TYPE_ID_128, V::TYPE_ID_128);
    const TYPE_ID_64: u64 = binary_id_64_(tags::MAP, K::TYPE_ID_64, V::TYPE_ID_64);
    const TYPE_ID_32: u32 = binary_id_32_(tags::MAP, K::TYPE_ID_32, V::TYPE_ID_32);
    const TYPE_NAME: &'static str = "HashMap";
}
