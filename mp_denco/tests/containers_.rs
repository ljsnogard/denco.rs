//! 标准库容器的端到端测试。
//!
//! 这里同时核对两件事：容器能在 `MpEncoder` / `MpDecoder` 上往返，以及顺序确定的
//! 容器与 `rmp-serde` 的字节表示一致。`HashMap` / `HashSet` 的遍历顺序不确定，
//! 因此只做双向交叉解码，不断言字节。
//!
//! End-to-end tests for the standard-library container implementations. Containers
//! with deterministic order are checked byte-for-byte against `rmp-serde`; the hash
//! containers are cross-decoded in both directions instead.

#![feature(allocator_ext)]
#![feature(btreemap_alloc)]
#![feature(impl_trait_in_assoc_type)]

use core::{
    alloc::{AllocError, Allocator, AllocatorClone, Layout},
    future::Future,
    ptr::NonNull,
    sync::atomic::{AtomicUsize, Ordering},
    task::{Context, Poll},
};
use std::{
    alloc::Global,
    borrow::Cow,
    collections::{
        BTreeMap, BTreeSet, BinaryHeap, HashMap, HashSet, LinkedList, VecDeque,
        hash_map::RandomState,
    },
    rc::Rc,
    sync::Arc,
    task::Waker,
};

use abs_cancel::{NonCancellableToken, TrMayCancel};
use abs_denco::{de::TrDecodable, ser::TrEncodable};
use mp_denco::{MpDecoder, MpEncoder};

/// A1 / A2 两个 allocator 的分配次数。
///
/// 下标 0 是 A1，下标 1 是 A2；下标只用于观察，不参与编码。
static A1_A2_ALLOCS_: [AtomicUsize; 2] = [AtomicUsize::new(0), AtomicUsize::new(0)];

/// 带类型级编号的计数 allocator。
///
/// `TaggedAlloc<0>` 与 `TaggedAlloc<1>` 是**两个不同的 allocator 类型**，
/// 但它们的分配都转发给 `Global`；区别只体现在计数表的下标上。
#[derive(Clone, Default)]
struct TaggedAlloc<const ID: usize>;

// SAFETY: 分配全部转发给 `Global`，释放的指针与布局也来自同一个 `Global`
// 分配，没有跨 allocator 释放。
unsafe impl<const ID: usize> Allocator for TaggedAlloc<ID> {
    fn allocate(&self, layout: Layout) -> Result<NonNull<[u8]>, AllocError> {
        A1_A2_ALLOCS_[ID].fetch_add(1, Ordering::Relaxed);
        Global.allocate(layout)
    }

    unsafe fn deallocate(&self, ptr: NonNull<u8>, layout: Layout) {
        // SAFETY: 由调用方保证该块来自本 allocator（即 `Global`）且只释放一次。
        unsafe { Global.deallocate(ptr, layout) }
    }
}

/// 自定义 allocator 的分配次数计数。
static CUSTOM_ALLOCS_: AtomicUsize = AtomicUsize::new(0);

/// 一个只做计数、实际转发给 [`Global`] 的 allocator。
///
/// A counting allocator that forwards to [`Global`].
#[derive(Clone, Default)]
struct CountingAlloc;

// SAFETY: 所有分配都转发给 `Global`，`deallocate` 收到的指针与布局来自同一个
// `Global` 分配；没有跨 allocator 释放，也没有提前失效。
unsafe impl Allocator for CountingAlloc {
    fn allocate(&self, layout: Layout) -> Result<NonNull<[u8]>, AllocError> {
        CUSTOM_ALLOCS_.fetch_add(1, Ordering::Relaxed);
        Global.allocate(layout)
    }

    unsafe fn deallocate(&self, ptr: NonNull<u8>, layout: Layout) {
        // SAFETY: 由调用方保证该块来自本 allocator（即 `Global`）且只释放一次。
        unsafe { Global.deallocate(ptr, layout) }
    }
}

// SAFETY: `CountingAlloc` 是零大小、无 Drop、无别名的分配器，克隆出的实例与
// 原实例等价；计数用的是全局静态量。
unsafe impl AllocatorClone for CountingAlloc {}

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

/// 测试非 `Global` allocator 的容器编解码。
/// Test container encode/decode with a non-`Global` allocator.
///
/// - 手段：实现一个只转发给 `Global` 的计数 allocator，用它构造 `Vec` / `VecDeque` /
///   `LinkedList` / `BinaryHeap` / `BTreeMap` / `BTreeSet` / `HashMap` / `HashSet` /
///   `Box` / `Rc` / `Arc`，分别做 denco 编码与解码。
/// - 判断：所有值往返相等；顺序确定的容器与同内容的 `Global` 版本字节一致；
///   分配计数在编解码后增长，证明分配确实经过自定义 allocator，而不是被悄悄换回
///   `Global`。
///
/// - Means: implement a counting allocator that forwards to `Global` and use it to
///   build `Vec` / `VecDeque` / `LinkedList` / `BinaryHeap` / `BTreeMap` /
///   `BTreeSet` / `HashMap` / `HashSet` / `Box` / `Rc` / `Arc`, then encode and
///   decode each with denco.
/// - Verdict: every value roundtrips; deterministic containers encode to the same
///   bytes as their `Global` counterparts; the allocation counter grows, proving the
///   custom allocator is really used rather than silently replaced by `Global`.
#[test]
fn custom_allocator_containers_() {
    let before = CUSTOM_ALLOCS_.load(Ordering::Relaxed);

    // Vec
    let mut vec: Vec<u32, CountingAlloc> = Vec::new_in(CountingAlloc);
    vec.push(1u32);
    vec.push(2u32);
    assert_eq!(encode_(&vec), encode_(&vec![1u32, 2u32]));
    let got: Vec<u32, CountingAlloc> = decode_(&encode_(&vec));
    assert_eq!(got, vec);

    // VecDeque
    let mut deque: VecDeque<u32, CountingAlloc> = VecDeque::new_in(CountingAlloc);
    deque.push_back(3u32);
    deque.push_back(4u32);
    assert_eq!(encode_(&deque), encode_(&VecDeque::from([3u32, 4u32])));
    let got: VecDeque<u32, CountingAlloc> = decode_(&encode_(&deque));
    assert_eq!(got, deque);

    // LinkedList
    let mut list: LinkedList<u32, CountingAlloc> = LinkedList::new_in(CountingAlloc);
    list.push_back(5u32);
    list.push_back(6u32);
    assert_eq!(encode_(&list), encode_(&LinkedList::from([5u32, 6u32])));
    let got: LinkedList<u32, CountingAlloc> = decode_(&encode_(&list));
    assert_eq!(got, list);

    // BinaryHeap
    let mut heap: BinaryHeap<u32, CountingAlloc> = BinaryHeap::new_in(CountingAlloc);
    heap.push(7u32);
    heap.push(8u32);
    assert_eq!(encode_(&heap), encode_(&BinaryHeap::from([7u32, 8u32])));
    let got: BinaryHeap<u32, CountingAlloc> = decode_(&encode_(&heap));
    let mut sorted: Vec<u32> = got.into_iter().collect();
    sorted.sort_unstable();
    assert_eq!(sorted, vec![7u32, 8u32]);

    // BTreeMap
    let mut map: BTreeMap<u16, String, CountingAlloc> = BTreeMap::new_in(CountingAlloc);
    map.insert(1u16, "one".to_string());
    map.insert(2u16, "two".to_string());
    let mut global_map = BTreeMap::new();
    global_map.insert(1u16, "one".to_string());
    global_map.insert(2u16, "two".to_string());
    assert_eq!(encode_(&map), encode_(&global_map));
    let got: BTreeMap<u16, String, CountingAlloc> = decode_(&encode_(&map));
    assert_eq!(got, map);

    // BTreeSet
    let mut set: BTreeSet<u32, CountingAlloc> = BTreeSet::new_in(CountingAlloc);
    set.insert(9u32);
    set.insert(10u32);
    assert_eq!(encode_(&set), encode_(&BTreeSet::from([9u32, 10u32])));
    let got: BTreeSet<u32, CountingAlloc> = decode_(&encode_(&set));
    assert_eq!(got, set);

    // HashMap / HashSet：顺序不确定，只做往返与交叉解码。
    let mut hash_map: HashMap<String, i64, RandomState, CountingAlloc> =
        HashMap::with_hasher_in(RandomState::default(), CountingAlloc);
    hash_map.insert("a".to_string(), 1i64);
    hash_map.insert("b".to_string(), 2i64);
    let hash_map_bytes = encode_(&hash_map);
    let got: HashMap<String, i64, RandomState, CountingAlloc> = decode_(&hash_map_bytes);
    assert_eq!(got, hash_map);
    let via_global: HashMap<String, i64> =
        rmp_serde::from_slice(&hash_map_bytes).expect("serde 应能解码自定义 allocator 的字节");
    let expected: HashMap<String, i64> = [("a".to_string(), 1i64), ("b".to_string(), 2)]
        .into_iter()
        .collect();
    assert_eq!(via_global, expected);

    let mut hash_set: HashSet<u32, RandomState, CountingAlloc> =
        HashSet::with_hasher_in(RandomState::default(), CountingAlloc);
    hash_set.insert(11u32);
    hash_set.insert(12u32);
    let hash_set_bytes = encode_(&hash_set);
    let got: HashSet<u32, RandomState, CountingAlloc> = decode_(&hash_set_bytes);
    assert_eq!(got, hash_set);

    // Box / Rc / Arc
    let boxed: Box<u32, CountingAlloc> = Box::new_in(13u32, CountingAlloc);
    assert_eq!(encode_(&boxed), encode_(&Box::new(13u32)));
    let got: Box<u32, CountingAlloc> = decode_(&encode_(&boxed));
    assert_eq!(*got, 13u32);

    let rc: Rc<u32, CountingAlloc> = Rc::new_in(14u32, CountingAlloc);
    assert_eq!(encode_(&rc), encode_(&Rc::new(14u32)));
    let got: Rc<u32, CountingAlloc> = decode_(&encode_(&rc));
    assert_eq!(*got, 14u32);

    let arc: Arc<u32, CountingAlloc> = Arc::new_in(15u32, CountingAlloc);
    assert_eq!(encode_(&arc), encode_(&Arc::new(15u32)));
    let got: Arc<u32, CountingAlloc> = decode_(&encode_(&arc));
    assert_eq!(*got, 15u32);

    let after = CUSTOM_ALLOCS_.load(Ordering::Relaxed);
    assert!(
        after > before,
        "自定义 allocator 应当被实际使用：before={before}, after={after}"
    );
}

/// 测试「A1 编码、A2 解码」是否可行。
/// Test whether a container encoded with allocator A1 can be decoded into A2.
///
/// - 手段：`TaggedAlloc<0>` 记作 A1、`TaggedAlloc<1>` 记作 A2。用 A1 构造
///   `Vec<u32>` 并编码；再把同一段字节解码成 `Vec<u32, TaggedAlloc<1>>`。
/// - 判断：解码值与源值相同；字节与 `Global` 版本逐字节一致（allocator 身份
///   不进入线格式）；解码后 A2 的分配计数增长、A1 的计数保持不变，说明解码
///   确实是用 A2 重新分配，而不是复用 A1。这条测试证明不同 allocator 之间的
///   编码/解码是可行的。
///
/// - Means: `TaggedAlloc<0>` is A1 and `TaggedAlloc<1>` is A2. Build a
///   `Vec<u32>` with A1 and encode it, then decode the same bytes into
///   `Vec<u32, TaggedAlloc<1>>`.
/// - Verdict: the decoded value equals the source; the bytes match the `Global`
///   version exactly (allocator identity is not on the wire); after decoding,
///   A2's allocation counter grew while A1's stayed put, proving the decode
///   really re-allocated with A2. This demonstrates A1-to-A2 encode/decode is
///   feasible.
#[test]
fn allocator_a1_encode_a2_decode_() {
    let a1_before = A1_A2_ALLOCS_[0].load(Ordering::Relaxed);

    // 用 A1 构造源容器。
    let mut source: Vec<u32, TaggedAlloc<0>> = Vec::new_in(TaggedAlloc::<0>);
    source.push(1u32);
    source.push(2u32);
    source.push(3u32);
    assert!(
        A1_A2_ALLOCS_[0].load(Ordering::Relaxed) > a1_before,
        "A1 应当参与源容器的构造"
    );

    // 编码：allocator 身份不进入字节。
    let bytes = encode_(&source);
    assert_eq!(
        bytes,
        encode_(&vec![1u32, 2, 3]),
        "字节应与 Global 版本一致"
    );

    // 解码到 A2；解码期间 A1 不应再分配。
    let a1_before_decode = A1_A2_ALLOCS_[0].load(Ordering::Relaxed);
    let a2_before_decode = A1_A2_ALLOCS_[1].load(Ordering::Relaxed);
    let got: Vec<u32, TaggedAlloc<1>> = decode_(&bytes);

    assert_eq!(got.as_slice(), &[1u32, 2, 3], "A2 容器应装回同样的值");
    assert!(
        A1_A2_ALLOCS_[1].load(Ordering::Relaxed) > a2_before_decode,
        "A2 应当参与解码构造"
    );
    assert_eq!(
        A1_A2_ALLOCS_[0].load(Ordering::Relaxed),
        a1_before_decode,
        "解码不应当再用 A1 分配"
    );

    // 同一段字节也能解回默认 allocator 的容器。
    let global: Vec<u32> = decode_(&bytes);
    assert_eq!(global, vec![1u32, 2, 3]);
}
