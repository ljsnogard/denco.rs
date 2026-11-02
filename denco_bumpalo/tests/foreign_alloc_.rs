//! 自定义 allocator 字段的跨格式测试：`Vec<T, A>` / `Arc<T, A>`，以及
//! allocator 本身是泛型参数的情形。
//!
//! Cross-format tests for fields whose allocator is a custom allocator or a
//! generic parameter.

#![feature(allocator_ext)]
#![feature(impl_trait_in_assoc_type)]

use core::{
    alloc::{AllocError, Allocator},
    future::IntoFuture,
    ptr::NonNull,
    task::{Context, Poll, Waker},
};
use std::{
    alloc::{Global, Layout},
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
};

use abs_cancel::{NonCancellableToken, TrMayCancel};
use abs_denco::{
    de::{NoArena, TrAllocSource, TrArena, TrDecodable},
    ser::TrEncodable,
};
use denco_bumpalo::BumpArena;
use json_denco::{JsonDecoder, JsonEncoder};
use mp_denco::{MpDecoder, MpEncoder};

/// 自定义 allocator 的分配次数计数。
static CUSTOM_ALLOCS_: AtomicUsize = AtomicUsize::new(0);

/// 一个与 arena 无关、只做计数并转发给 [`Global`] 的 allocator。
///
/// An arena-independent counting allocator forwarding to [`Global`].
#[derive(Clone, Default, Debug, PartialEq, Eq)]
struct CountingAlloc;

// SAFETY: 所有分配都转发给 `Global`，`deallocate` 收到的指针与布局来自同一个
// `Global` 分配；没有跨 allocator 释放。
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

// 解码侧：allocator 与 decoder 的 arena 无关，因此对任意 arena 都新建实例。
impl<'alloc, Alloc> TrAllocSource<'alloc, Alloc> for CountingAlloc
where
    Alloc: TrArena<'alloc> + 'alloc,
{
    fn alloc_from(_arena: &'alloc Alloc) -> Self {
        CountingAlloc
    }
}

/// 字段使用具体自定义 allocator 的结构体。
#[derive(Debug, PartialEq, denco_macro::TrEncodable, denco_macro::TrDecodable)]
struct ConcreteAllocs {
    values_: Vec<u32, CountingAlloc>,
    shared_: Arc<u32, CountingAlloc>,
}

/// 字段使用泛型 allocator 参数的结构体。
///
/// A struct whose fields use a generic allocator parameter.
#[derive(Debug, PartialEq, denco_macro::TrEncodable, denco_macro::TrDecodable)]
struct GenericAllocs<A: Allocator> {
    values_: Vec<u32, A>,
    shared_: Arc<u32, A>,
}

/// 同时包含值类型参数与 allocator 参数的结构体。
#[derive(Debug, PartialEq, denco_macro::TrEncodable, denco_macro::TrDecodable)]
struct MixedAllocs<T, A: Allocator> {
    label_: T,
    values_: Vec<T, A>,
    shared_: Arc<T, A>,
}

/// 立即完成的测试执行器。
fn block_on_<F: IntoFuture>(future: F) -> F::Output {
    let mut future = core::pin::pin!(future.into_future());
    let waker = Waker::noop();
    let mut context = Context::from_waker(waker);
    loop {
        match future.as_mut().poll(&mut context) {
            Poll::Ready(value) => return value,
            Poll::Pending => core::hint::spin_loop(),
        }
    }
}

fn encode_mp_<T>(value: &T) -> Vec<u8>
where
    T: ?Sized + TrEncodable,
{
    let mut storage = vec![0u8; 4096];
    let total = storage.len();
    let written = {
        let mut sink: &mut [u8] = storage.as_mut_slice();
        block_on_(
            value
                .encode_async(MpEncoder::new(&mut sink))
                .may_cancel_with(NonCancellableToken::new()),
        )
        .expect("MessagePack 编码应当成功");
        total - sink.len()
    };
    storage.truncate(written);
    storage
}

fn encode_json_<T>(value: &T) -> Vec<u8>
where
    T: ?Sized + TrEncodable,
{
    let mut storage = vec![0u8; 4096];
    let total = storage.len();
    let written = {
        let mut sink: &mut [u8] = storage.as_mut_slice();
        block_on_(
            value
                .encode_async(JsonEncoder::new(&mut sink))
                .may_cancel_with(NonCancellableToken::new()),
        )
        .expect("JSON 编码应当成功");
        total - sink.len()
    };
    storage.truncate(written);
    storage
}

fn decode_mp_<'de, 'alloc, T>(bytes: &'de [u8]) -> T
where
    T: TrDecodable<'de, 'alloc, NoArena>,
{
    let mut input: &'de [u8] = bytes;
    block_on_(
        T::decode_async(MpDecoder::new(&mut input)).may_cancel_with(NonCancellableToken::new()),
    )
    .expect("MessagePack 解码应当成功")
}

fn decode_json_<'de, 'alloc, T>(bytes: &'de [u8]) -> T
where
    T: TrDecodable<'de, 'alloc, NoArena>,
{
    let mut input: &'de [u8] = bytes;
    let mut decoder = JsonDecoder::new(&mut input);
    block_on_(T::decode_async(&mut decoder).may_cancel_with(NonCancellableToken::new()))
        .expect("JSON 解码应当成功")
}

fn sample_concrete_() -> ConcreteAllocs {
    let mut values = Vec::new_in(CountingAlloc);
    values.extend([1u32, 2, 3]);
    ConcreteAllocs {
        values_: values,
        shared_: Arc::new_in(7u32, CountingAlloc),
    }
}

fn sample_mixed_() -> MixedAllocs<String, CountingAlloc> {
    let mut values = Vec::new_in(CountingAlloc);
    values.push(String::from("a"));
    values.push(String::from("b"));
    MixedAllocs {
        label_: String::from("mixed"),
        values_: values,
        shared_: Arc::new_in(String::from("shared"), CountingAlloc),
    }
}

fn sample_generic_() -> GenericAllocs<CountingAlloc> {
    let mut values = Vec::new_in(CountingAlloc);
    values.extend([4u32, 5]);
    GenericAllocs {
        values_: values,
        shared_: Arc::new_in(9u32, CountingAlloc),
    }
}

/// 测试目标：具体自定义 allocator 的字段在 MessagePack 上可往返。
/// Test goal: fields with a concrete custom allocator roundtrip on MessagePack.
///
/// - 手段：编码 `ConcreteAllocs`，分别用 `MpDecoder::new`（`NoArena`）与
///   `MpDecoder::new_in(&bump)`（`BumpArena`）解码。
///   / How: encode `ConcreteAllocs` and decode it with `MpDecoder::new`
///   (`NoArena`) and `MpDecoder::new_in(&bump)` (`BumpArena`).
/// - 判断：两次解码结果都等于原值；`CountingAlloc` 的计数增长，说明容器确实
///   用了自定义 allocator。
///   / What: both decode results equal the original value and the counting
///   allocator's counter grows, proving the container uses it.
#[test]
fn concrete_alloc_fields_mp() {
    let value = sample_concrete_();
    let bytes = encode_mp_(&value);

    let from_no_arena: ConcreteAllocs = decode_mp_(&bytes);
    assert_eq!(from_no_arena, value);

    let bump = BumpArena::new();
    let mut input: &[u8] = &bytes;
    let from_bump: ConcreteAllocs = block_on_(
        ConcreteAllocs::decode_async(MpDecoder::new_in(&mut input, &bump))
            .may_cancel_with(NonCancellableToken::new()),
    )
    .expect("MessagePack + BumpArena 解码应当成功");
    assert_eq!(from_bump, value);
    assert!(CUSTOM_ALLOCS_.load(Ordering::Relaxed) > 0);
}

/// 测试目标：具体自定义 allocator 的字段在 JSON 上可往返。
/// Test goal: fields with a concrete custom allocator roundtrip on JSON.
///
/// - 手段：编码 `ConcreteAllocs`，分别用 `JsonDecoder::new` 与
///   `JsonDecoder::new_in(&bump)` 解码。
///   / How: encode `ConcreteAllocs` and decode it with `JsonDecoder::new` and
///   `JsonDecoder::new_in(&bump)`.
/// - 判断：两次解码结果都等于原值。
///   / What: both decode results equal the original value.
#[test]
fn concrete_alloc_fields_json() {
    let value = sample_concrete_();
    let bytes = encode_json_(&value);

    let from_no_arena: ConcreteAllocs = decode_json_(&bytes);
    assert_eq!(from_no_arena, value);

    let bump = BumpArena::new();
    let mut input: &[u8] = &bytes;
    let mut decoder = JsonDecoder::new_in(&mut input, &bump);
    let from_bump: ConcreteAllocs = block_on_(
        ConcreteAllocs::decode_async(&mut decoder).may_cancel_with(NonCancellableToken::new()),
    )
    .expect("JSON + BumpArena 解码应当成功");
    assert_eq!(from_bump, value);
}

/// 测试目标：allocator 是泛型参数时，MessagePack 上可往返。
/// Test goal: a generic allocator parameter roundtrips on MessagePack.
///
/// - 手段：编码 `GenericAllocs<CountingAlloc>`，分别用 NoArena 与 BumpArena
///   decoder 解码。
///   / How: encode `GenericAllocs<CountingAlloc>` and decode it with NoArena and
///   BumpArena decoders.
/// - 判断：两次解码结果都等于原值。
///   / What: both decode results equal the original value.
#[test]
fn generic_alloc_fields_mp() {
    let value = sample_generic_();
    let bytes = encode_mp_(&value);

    let from_no_arena: GenericAllocs<CountingAlloc> = decode_mp_(&bytes);
    assert_eq!(from_no_arena, value);

    let bump = BumpArena::new();
    let mut input: &[u8] = &bytes;
    let from_bump: GenericAllocs<CountingAlloc> = block_on_(
        GenericAllocs::<CountingAlloc>::decode_async(MpDecoder::new_in(&mut input, &bump))
            .may_cancel_with(NonCancellableToken::new()),
    )
    .expect("MessagePack + 泛型 allocator 解码应当成功");
    assert_eq!(from_bump, value);
}

/// 测试目标：allocator 是泛型参数时，JSON 上可往返。
/// Test goal: a generic allocator parameter roundtrips on JSON.
///
/// - 手段：编码 `GenericAllocs<CountingAlloc>`，分别用 NoArena 与 BumpArena
///   decoder 解码。
///   / How: encode `GenericAllocs<CountingAlloc>` and decode it with NoArena and
///   BumpArena decoders.
/// - 判断：两次解码结果都等于原值。
///   / What: both decode results equal the original value.
#[test]
fn generic_alloc_fields_json() {
    let value = sample_generic_();
    let bytes = encode_json_(&value);

    let from_no_arena: GenericAllocs<CountingAlloc> = decode_json_(&bytes);
    assert_eq!(from_no_arena, value);

    let bump = BumpArena::new();
    let mut input: &[u8] = &bytes;
    let mut decoder = JsonDecoder::new_in(&mut input, &bump);
    let from_bump: GenericAllocs<CountingAlloc> = block_on_(
        GenericAllocs::<CountingAlloc>::decode_async(&mut decoder)
            .may_cancel_with(NonCancellableToken::new()),
    )
    .expect("JSON + 泛型 allocator 解码应当成功");
    assert_eq!(from_bump, value);
}

/// 测试目标：值参数与 allocator 参数混用时，两种格式都能往返。
/// Test goal: value and allocator parameters mixed in one struct roundtrip on
/// both formats.
///
/// - 手段：编码 `MixedAllocs<String, CountingAlloc>`，分别用 MessagePack 与 JSON
///   解码。
///   / How: encode `MixedAllocs<String, CountingAlloc>` and decode it with
///   MessagePack and JSON.
/// - 判断：两个解码结果都等于原值。
///   / What: both decode results equal the original value.
#[test]
fn mixed_value_and_alloc_params() {
    let value = sample_mixed_();

    let mp_bytes = encode_mp_(&value);
    let from_mp: MixedAllocs<String, CountingAlloc> = decode_mp_(&mp_bytes);
    assert_eq!(from_mp, value);

    let json_bytes = encode_json_(&value);
    let from_json: MixedAllocs<String, CountingAlloc> = decode_json_(&json_bytes);
    assert_eq!(from_json, value);
}
