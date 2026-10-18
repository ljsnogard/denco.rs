//! 标准库容器的端到端测试。
//!
//! 这里同时核对两件事：容器能在 `MpEncoder` / `MpDecoder` 上往返，以及顺序确定的
//! 容器与 `rmp-serde` 的字节表示一致。`HashMap` / `HashSet` 的遍历顺序不确定，
//! 因此只做双向交叉解码，不断言字节。
//!
//! End-to-end tests for the standard-library container implementations. Containers
//! with deterministic order are checked byte-for-byte against `rmp-serde`; the hash
//! containers are cross-decoded in both directions instead.

#![feature(impl_trait_in_assoc_type)]

use core::{
    future::Future,
    task::{Context, Poll},
};
use std::{
    borrow::Cow,
    collections::{BTreeMap, BTreeSet, BinaryHeap, HashMap, HashSet, LinkedList, VecDeque},
    rc::Rc,
    task::Waker,
};

use abs_cancel::{NonCancellableToken, TrMayCancel};
use abs_denco::{de::TrDecodable, ser::TrEncodable};
use mp_denco::{MpDecoder, MpEncoder};

/// 带容器字段的派生类型：验证 `denco_macro` 不需要为容器做任何特殊处理。
///
/// A derived type with container fields: shows `denco_macro` needs no
/// container-specific handling.
#[derive(
    Clone,
    Debug,
    Eq,
    PartialEq,
    denco_macro::TrDecodable,
    denco_macro::TrEncodable,
    serde::Deserialize,
    serde::Serialize,
)]
struct Envelope {
    headers: BTreeMap<u16, String>,
    tags: BTreeSet<String>,
}

/// 立即完成的测试执行器。
fn block_on_<F: Future>(future: F) -> F::Output {
    let mut cx = Context::from_waker(Waker::noop());
    let mut future = core::pin::pin!(future);
    loop {
        match future.as_mut().poll(&mut cx) {
            Poll::Ready(output) => return output,
            Poll::Pending => std::thread::yield_now(),
        }
    }
}

/// 用 denco 把一个值编码成字节。
fn encode_<T>(value: &T) -> Vec<u8>
where
    T: ?Sized + TrEncodable,
{
    let mut storage = vec![0u8; 4096];
    let total = storage.len();
    let written = {
        let mut sink: &mut [u8] = storage.as_mut_slice();
        block_on_(async {
            value
                .encode_async(MpEncoder::new(&mut sink))
                .may_cancel_with(NonCancellableToken::new())
                .await
        })
        .expect("编码应当成功");
        total - sink.len()
    };
    storage.truncate(written);
    storage
}

/// 用 denco 从字节解码一个值。
fn decode_<'de, T>(bytes: &'de [u8]) -> T
where
    T: TrDecodable<'de>,
{
    let mut input: &'de [u8] = bytes;
    block_on_(async {
        T::decode_async(MpDecoder::new(&mut input))
            .may_cancel_with(NonCancellableToken::new())
            .await
    })
    .expect("解码应当成功")
}

/// 用 `rmp-serde` 编码同一个值，作为字节级参照。
fn serde_bytes_<T>(value: &T) -> Vec<u8>
where
    T: ?Sized + serde::Serialize,
{
    rmp_serde::to_vec_named(value).expect("serde 编码应当成功")
}

/// 测试 `BTreeMap` 的往返与字节一致性。
/// Test `BTreeMap` roundtrip and byte-for-byte agreement with `rmp-serde`.
///
/// - 手段：构造含数字键与文本键的 `BTreeMap`，分别用 denco 与 `rmp-serde` 编码。
/// - 判断：两份字节逐字节相等；denco 解码回来的值与写入值相等。数字键正是
///   MPTP 头表使用的形态，因此这条同时覆盖了实际协议场景。
///
/// - Means: build a `BTreeMap` with numeric and text keys and encode it with denco
///   and `rmp-serde`.
/// - Verdict: the two byte strings are identical and the denco roundtrip equals the
///   original map. Numeric keys are what the MPTP header table uses, so this also
///   covers the real protocol shape.
#[test]
fn btree_map_roundtrip_and_bytes_() {
    let mut value = BTreeMap::new();
    value.insert(1u16, "one".to_string());
    value.insert(42u16, "forty-two".to_string());

    let bytes = encode_(&value);
    assert_eq!(
        bytes,
        serde_bytes_(&value),
        "BTreeMap 的字节应与 serde 一致"
    );

    let got: BTreeMap<u16, String> = decode_(&bytes);
    assert_eq!(got, value);
}

/// 测试 `BTreeSet` 的往返与字节一致性。
/// Test `BTreeSet` roundtrip and byte-for-byte agreement with `rmp-serde`.
///
/// - 手段：构造一个 `BTreeSet<i64>`，比较 denco 与 serde 的编码结果。
/// - 判断：字节逐字节相等；解码值与原集合相等，且元素顺序按 `Ord` 排序。
///
/// - Means: build a `BTreeSet<i64>` and compare the denco and serde encodings.
/// - Verdict: the bytes match, the decoded set equals the original, and iteration
///   follows `Ord`.
#[test]
fn btree_set_roundtrip_and_bytes_() {
    let value: BTreeSet<i64> = [9i64, -3, 4].into_iter().collect();

    let bytes = encode_(&value);
    assert_eq!(
        bytes,
        serde_bytes_(&value),
        "BTreeSet 的字节应与 serde 一致"
    );

    let got: BTreeSet<i64> = decode_(&bytes);
    assert_eq!(got, value);
    assert_eq!(got.into_iter().collect::<Vec<_>>(), vec![-3, 4, 9]);
}

/// 测试 `VecDeque` 与 `LinkedList` 的往返与字节一致性。
/// Test `VecDeque` and `LinkedList` roundtrip plus byte agreement.
///
/// - 手段：构造元素相同的 `VecDeque` 与 `LinkedList`，各自与 serde 比对字节。
/// - 判断：两种容器的字节都与其 serde 编码相同；解码回来的元素序列与原来一致。
///
/// - Means: build a `VecDeque` and a `LinkedList` with the same elements and compare
///   each against its serde encoding.
/// - Verdict: both container kinds match serde byte-for-byte and decode back to the
///   same element sequence.
#[test]
fn seq_containers_roundtrip_and_bytes_() {
    let deque: VecDeque<u32> = [1u32, 2, 3].into_iter().collect();
    let list: LinkedList<u32> = [1u32, 2, 3].into_iter().collect();

    let deque_bytes = encode_(&deque);
    assert_eq!(
        deque_bytes,
        serde_bytes_(&deque),
        "VecDeque 的字节应与 serde 一致"
    );
    let got: VecDeque<u32> = decode_(&deque_bytes);
    assert_eq!(got, deque);

    let list_bytes = encode_(&list);
    assert_eq!(
        list_bytes,
        serde_bytes_(&list),
        "LinkedList 的字节应与 serde 一致"
    );
    let got: LinkedList<u32> = decode_(&list_bytes);
    assert_eq!(got, list);
}

/// 测试 `BinaryHeap` 的往返与字节一致性。
/// Test `BinaryHeap` roundtrip and byte-for-byte agreement.
///
/// - 手段：按同一顺序向两个堆插入同样的元素，比较 denco 与 serde 的编码。
/// - 判断：字节相等（两者都按内部数组顺序遍历）；解码后按值排序得到同一组元素。
///   堆序本身不是线格式语义，因此不断言遍历顺序。
///
/// - Means: push the same elements in the same order into two heaps and compare the
///   denco and serde encodings.
/// - Verdict: the bytes match (both iterate the internal array) and sorting the
///   decoded heap yields the same element set. Heap order is not part of the wire
///   contract, so it is not asserted.
#[test]
fn binary_heap_roundtrip_and_bytes_() {
    let mut value = BinaryHeap::new();
    for item in [5i32, 1, 9, 3] {
        value.push(item);
    }

    let bytes = encode_(&value);
    assert_eq!(
        bytes,
        serde_bytes_(&value),
        "BinaryHeap 的字节应与 serde 一致"
    );

    let got: BinaryHeap<i32> = decode_(&bytes);
    let mut sorted: Vec<i32> = got.into_iter().collect();
    sorted.sort_unstable();
    assert_eq!(sorted, vec![1, 3, 5, 9]);
}

/// 测试 `HashMap` / `HashSet` 的双向交叉解码。
/// Test `HashMap` / `HashSet` cross-decoding in both directions.
///
/// - 手段：denco 编码后交给 serde 解码；serde 编码后交给 denco 解码。
/// - 判断：两次都得到与原值相等的容器。哈希容器的遍历顺序不确定，因此**不**比较
///   字节；这一点与 serde 自身的性质一致。
///
/// - Means: encode with denco and decode with serde, then encode with serde and
///   decode with denco.
/// - Verdict: both directions reproduce the original container. Hash containers have
///   no deterministic order, so bytes are deliberately not compared — the same
///   property holds for serde itself.
#[test]
fn hash_containers_cross_decode_() {
    let map: HashMap<String, i64> = [("a".to_string(), 1i64), ("b".to_string(), 2)]
        .into_iter()
        .collect();
    let denco_bytes = encode_(&map);
    let via_serde: HashMap<String, i64> =
        rmp_serde::from_slice(&denco_bytes).expect("serde 应能解码 denco 的字节");
    assert_eq!(via_serde, map);

    let serde_bytes = serde_bytes_(&map);
    let via_denco: HashMap<String, i64> = decode_(&serde_bytes);
    assert_eq!(via_denco, map);

    let set: HashSet<u32> = [2u32, 4, 8].into_iter().collect();
    let denco_bytes = encode_(&set);
    let via_serde: HashSet<u32> =
        rmp_serde::from_slice(&denco_bytes).expect("serde 应能解码 denco 的字节");
    assert_eq!(via_serde, set);

    let serde_bytes = serde_bytes_(&set);
    let via_denco: HashSet<u32> = decode_(&serde_bytes);
    assert_eq!(via_denco, set);
}

/// 测试 `Rc` 的透明往返。
/// Test the transparent roundtrip of `Rc`.
///
/// - 手段：把 `Rc<String>` 分别用 denco 与 serde 编码。
/// - 判断：字节一致（`Rc` 不引入额外包装）；解码后内部字符串相同。
///
/// - Means: encode an `Rc<String>` with denco and serde.
/// - Verdict: the bytes match (`Rc` adds no wrapper) and the decoded inner string is
///   identical.
#[test]
fn rc_roundtrip_() {
    let value: Rc<String> = Rc::new("shared".to_string());

    let bytes = encode_(&value);
    assert_eq!(bytes, serde_bytes_(&value), "Rc 的字节应与 serde 一致");

    let got: Rc<String> = decode_(&bytes);
    assert_eq!(*got, *value);
}

/// 测试 `Cow<str>` / `Cow<[T]>` 的编码与 owned 解码。
/// Test `Cow<str>` / `Cow<[T]>` encoding and owned decoding.
///
/// - 手段：分别编码 `Borrowed` 与 `Owned` 的 `Cow<str>`，以及一个借用切片。
/// - 判断：借用与拥有两种形态的字节相同，且与 serde 一致；解码结果一律是
///   `Cow::Owned`，值与原值相等——当前格式层不提供零拷贝借用。
///
/// - Means: encode both `Borrowed` and `Owned` `Cow<str>` values plus a borrowed
///   slice.
/// - Verdict: borrowed and owned forms encode identically and match serde; decoding
///   always yields `Cow::Owned` with an equal value, because the current format
///   layer offers no zero-copy borrowing.
#[test]
fn cow_roundtrip_() {
    let borrowed: Cow<'static, str> = Cow::Borrowed("hello");
    let owned: Cow<'static, str> = Cow::Owned("hello".to_string());
    assert_eq!(encode_(&borrowed), encode_(&owned));
    assert_eq!(encode_(&borrowed), serde_bytes_(&borrowed));

    let got: Cow<'static, str> = decode_(&encode_(&borrowed));
    assert!(matches!(got, Cow::Owned(_)), "当前解码应一律得到 Owned");
    assert_eq!(got, "hello");

    let slice: &'static [u32] = &[1, 2, 3];
    let cow_slice: Cow<'static, [u32]> = Cow::Borrowed(slice);
    assert_eq!(encode_(&cow_slice), serde_bytes_(&cow_slice));

    let got: Cow<'static, [u32]> = decode_(&encode_(&cow_slice));
    assert_eq!(got.as_ref(), slice);
}

/// 测试嵌套容器与空容器的组合。
/// Test nested containers together with empty ones.
///
/// - 手段：编码一个 `BTreeMap<String, VecDeque<Option<u32>>>`，并逐个编码空容器。
/// - 判断：嵌套容器的字节与 serde 一致且往返相等；空容器也能往返，`BTreeMap`
///   与 `BTreeSet` 的空编码与 serde 相同。这条覆盖「容器元素本身又是容器」的组合。
///
/// - Means: encode a `BTreeMap<String, VecDeque<Option<u32>>>` and each empty
///   container.
/// - Verdict: the nested container matches serde byte-for-byte and roundtrips; the
///   empty containers roundtrip as well, and empty `BTreeMap` / `BTreeSet` encodings
///   match serde. This covers containers whose elements are containers.
#[test]
fn nested_and_empty_containers_() {
    let mut value: BTreeMap<String, VecDeque<Option<u32>>> = BTreeMap::new();
    value.insert(
        "a".to_string(),
        [Some(1u32), None, Some(3)].into_iter().collect(),
    );
    let bytes = encode_(&value);
    assert_eq!(bytes, serde_bytes_(&value), "嵌套容器的字节应与 serde 一致");
    let got: BTreeMap<String, VecDeque<Option<u32>>> = decode_(&bytes);
    assert_eq!(got, value);

    let empty_map = BTreeMap::<u8, u8>::new();
    assert_eq!(encode_(&empty_map), serde_bytes_(&empty_map));
    let empty_set = BTreeSet::<u8>::new();
    assert_eq!(encode_(&empty_set), serde_bytes_(&empty_set));

    let got: VecDeque<u8> = decode_(&encode_(&VecDeque::<u8>::new()));
    assert!(got.is_empty());
    let got: BinaryHeap<u8> = decode_(&encode_(&BinaryHeap::<u8>::new()));
    assert!(got.is_empty());
    let got: HashMap<u8, u8> = decode_(&encode_(&HashMap::<u8, u8>::new()));
    assert!(got.is_empty());
    let got: HashSet<u8> = decode_(&encode_(&HashSet::<u8>::new()));
    assert!(got.is_empty());
}

/// 测试派生结构体中的容器字段。
/// Test container fields inside a derived struct.
///
/// - 手段：给带 `BTreeMap` 与 `BTreeSet` 字段的 `Envelope` 同时派生 denco 与 serde，
///   分别编码并互相解码。
/// - 判断：派生出的 denco 编码与 `rmp_serde::to_vec_named` 字节一致，往返值相等。
///   这说明容器支持完全落在 `abs_denco`，派生宏无需任何改动。
///
/// - Means: derive both denco and serde for `Envelope` with `BTreeMap` and
///   `BTreeSet` fields, then encode and decode both ways.
/// - Verdict: the derived denco encoding matches `rmp_serde::to_vec_named`
///   byte-for-byte and roundtrips. Container support lives entirely in
///   `abs_denco`; the derive macro needs no changes.
#[test]
fn derive_with_containers_() {
    let mut value = Envelope {
        headers: BTreeMap::new(),
        tags: BTreeSet::new(),
    };
    value.headers.insert(7u16, "seven".to_string());
    value.tags.insert("a".to_string());
    value.tags.insert("b".to_string());

    let bytes = encode_(&value);
    assert_eq!(bytes, serde_bytes_(&value), "派生类型的字节应与 serde 一致");
    let got: Envelope = decode_(&bytes);
    assert_eq!(got, value);
}
