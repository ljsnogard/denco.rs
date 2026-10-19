//! `buffex::Ring` 上的真实缓冲集成测试。
//!
//! `Ring` 的段在回收后会被复用，因此解码器只通过 `read_async` 读取，字符串与
//! 二进制一律解码为 owned 结果；本文件同时覆盖段跨环末端、容量不足 park、
//! 长值与取消传播。
//!
//! Integration tests for `MpEncoder` / `MpDecoder` on a real `buffex::Ring`.
//! Reusable ring memory is always consumed through `read_async`, and decoded
//! strings/binaries are always owned.

#![feature(impl_trait_in_assoc_type)]

use core::fmt;
use std::time::Duration;

use abs_buff::{
    Demand,
    buffer::{TrBuffSegmMut, TrBuffSegmRef},
};
use abs_cancel::{NonCancellableToken, TrMayCancel};
use abs_denco::{de::TrDecodable, ser::TrEncodable};
use cantok_rs::CancellationSource;
use mp_denco::{MpDecoder, MpEncoder, MpError};
use serde::Serialize;

mod support;
use support::{
    BytesVal, StrVal, TestRing, U64Val, block_on_, decode_ring_, drain_ring_, encode_ring_,
    new_ring_, with_timeout_,
};

/// 断言同一个值在 ring 上完成「编码 → 排空字节 → 解码」闭环。
///
/// 断言目标 / Goal：
/// - ring 写出的字节与 `rmp_serde::to_vec` 完全一致；
/// - 通过解码器直接解码出的值与原值相等。
///
/// 手段 / Method：先用一个 ring 编码并排空字节，再用另一个 ring 编码后解码，
/// 两次编码使用同样的 `MpEncoder`，避免排空消费掉待解码的数据。
///
/// 判断 / Assert：字节逐字节相等，解码值 `==` 原值。
fn assert_ring_roundtrip_<T>(value: &T, cap: usize)
where
    T: TrEncodable + for<'de> TrDecodable<'de> + Serialize + PartialEq + fmt::Debug,
{
    let mut byte_ring = new_ring_(cap);
    encode_ring_(&mut byte_ring, value);
    let bytes = drain_ring_(&mut byte_ring);
    let compact = rmp_serde::to_vec(value).expect("rmp_serde compact 编码应当成功");
    assert_eq!(bytes, compact, "ring 编码字节应与 rmp_serde 紧凑表示一致");

    let mut decode_ring = new_ring_(cap);
    encode_ring_(&mut decode_ring, value);
    let got: T = decode_ring_(&mut decode_ring);
    assert_eq!(&got, value, "ring 解码值应与原值相等");
}

/// 把 ring 的读写位置向前推进 `amount` 字节：先写满再消费，使位置跨过环内偏移。
///
/// Advance both ring positions by writing and consuming `amount` filler bytes.
fn shift_ring_(ring: &mut TestRing, amount: usize) {
    let (mut writer, mut reader) = TestRing::split(ring);
    let filler = vec![0xa5u8; amount];

    let write_demand = Demand::exactly(amount);
    let mut write_segm = writer
        .try_write(&write_demand)
        .pick_left()
        .expect("空 ring 应当能借出写入段");
    assert_eq!(
        write_segm.move_items_from_as_buff(filler.as_slice()),
        amount
    );
    drop(write_segm);

    let mut sink = vec![0u8; amount];
    let read_demand = Demand::exactly(amount);
    let mut read_segm = reader
        .try_read(&read_demand)
        .pick_left()
        .expect("刚写入的填充数据应当可读");
    assert_eq!(read_segm.move_items_to_as_buff(sink.as_mut_slice()), amount);
    assert_eq!(sink, filler, "填充字节应原样读回");
}

/// 验证小消息在 ring 上的基础往返与字节一致性。
/// Verify the basic ring roundtrip and byte-for-byte agreement with rmp-serde.
///
/// - 测试目标 / Goal：`MpEncoder` / `MpDecoder` 能在 `buffex::Ring` 上工作。
/// - 手段 / Method：容量 4096 的 ring，编码小整数后分别排空字节与解码。
/// - 判断 / Assert：字节等于 `rmp_serde::to_vec`；解码值等于原值。
#[test]
fn ring_roundtrip_small_() {
    assert_ring_roundtrip_(&U64Val(300), 4096);
    assert_ring_roundtrip_(&U64Val(0x0102_0304_0506_0708), 4096);
}

/// 验证消息跨环末端时仍能正确编解码。
/// Verify a message that wraps around the ring end still roundtrips.
///
/// - 测试目标 / Goal：段在环末端被拆成两片时，编码与解码都不丢字节。
/// - 手段 / Method：容量 64，先写入并消费 60 字节填充，再编码一个 9 字节
///   的 `u64`；编码前先窥视读段确认它由两片组成，然后完整解码。
/// - 判断 / Assert：读段 `iter_slices()` 得到 2 片；解码值等于原值；环最终为空。
#[test]
fn ring_wrap_around_two_segments_() {
    let value = U64Val(0x0102_0304_0506_0708);
    let mut ring = new_ring_(64);
    shift_ring_(&mut ring, 60);

    let (mut writer, mut reader) = TestRing::split(&mut ring);
    block_on_(async {
        value
            .encode_async(MpEncoder::new(&mut writer))
            .may_cancel_with(NonCancellableToken::new())
            .await
    })
    .expect("跨环末端的编码应当成功");

    // 窥视整条消息：9 字节从偏移 60 开始，前 4 字节到环末，后 5 字节回到环首。
    let peek_demand = Demand::no_more_than(9);
    let peek = reader
        .try_read(&peek_demand)
        .pick_left()
        .expect("ring 中应当有 9 字节消息");
    let piece_count = peek.iter_slices().count();
    assert_eq!(piece_count, 2, "跨环末端的读段应拆成两片");
    drop(peek); // 只窥视不消费，读位置保持不动

    let got: U64Val = block_on_(async {
        U64Val::decode_async(MpDecoder::new(&mut reader))
            .may_cancel_with(NonCancellableToken::new())
            .await
    })
    .expect("跨环末端的解码应当成功");
    assert_eq!(got, value);
    assert_eq!(
        reader.ring_state().data_size(),
        0,
        "解码完成后 ring 应当被取空"
    );
}

/// 验证定长头与负载跨片时 `move_items_to_as_buff` 的多片搬移。
/// Verify multi-slice moves when a fixed header and payload cross the ring end.
///
/// - 测试目标 / Goal：`read_exact` 依赖的段搬移能跨两片正确工作。
/// - 手段 / Method：容量 64，位置推进到 60 后编码 9 字节 `u64`；先消费 1
///   字节标签，再借出 8 字节负载段，断言它是两片并一次性搬出。
/// - 判断 / Assert：标签为 `0xcf`；负载段 2 片；搬出 8 字节且内容为大端原值。
#[test]
fn ring_multi_slice_read_exact_() {
    let raw = 0x1122_3344_5566_7788u64;
    let mut ring = new_ring_(64);
    shift_ring_(&mut ring, 60);

    let (mut writer, mut reader) = TestRing::split(&mut ring);
    block_on_(async {
        U64Val(raw)
            .encode_async(MpEncoder::new(&mut writer))
            .may_cancel_with(NonCancellableToken::new())
            .await
    })
    .expect("编码应当成功");

    let mut header = [0u8; 1];
    let header_demand = Demand::exactly(1);
    let mut header_segm = reader
        .try_read(&header_demand)
        .pick_left()
        .expect("标签应当可读");
    assert_eq!(header_segm.move_items_to_as_buff(&mut header[..]), 1);
    drop(header_segm);
    assert_eq!(header[0], 0xcf, "0xcf 表示 8 字节无符号整数");

    let mut payload = [0u8; 8];
    let payload_demand = Demand::no_more_than(8);
    let mut payload_segm = reader
        .try_read(&payload_demand)
        .pick_left()
        .expect("负载应当可读");
    let piece_count = payload_segm.iter_slices().count();
    assert_eq!(piece_count, 2, "跨环末端的负载段应拆成两片");
    assert_eq!(
        payload_segm.move_items_to_as_buff(&mut payload[..]),
        8,
        "多片搬移应当一次取满 8 字节"
    );
    assert_eq!(payload, raw.to_be_bytes());
}

/// 验证超过栈缓冲上限的长字符串与长二进制走 owned 路径。
/// Verify long strings and binaries above the stack-buffer limit use the owned path.
///
/// - 测试目标 / Goal：长度大于 256 字节的值不依赖零拷贝借用也能正确解码。
/// - 手段 / Method：在容量 4096 的 ring 上分别编码 300 字节字符串与 300 字节
///   二进制，由解码器直接走 owned 路径。
/// - 判断 / Assert：字节与 rmp-serde 紧凑表示一致；解码内容与原值完全相等。
#[test]
fn ring_long_string_owned_path_() {
    let text = StrVal("环".repeat(150)); // 450 字节 UTF-8，明显超过 256
    assert_ring_roundtrip_(&text, 4096);

    let bytes = BytesVal((0u8..=255).cycle().take(300).collect());
    assert_ring_roundtrip_(&bytes, 4096);
}

/// 验证小容量 ring 上 4 KiB 消息的生产者/消费者并发 pump 不会死锁。
/// Verify a 4 KiB message pumped concurrently through a 128-byte ring cannot deadlock.
///
/// - 测试目标 / Goal：写端满时 park、读端空时 park 的互相唤醒路径可用。
/// - 手段 / Method：容量 128 的 ring，生产线程编码 4 KiB 字符串，消费线程
///   直接解码；整个场景包在 5 秒超时里。
/// - 判断 / Assert：5 秒内完成，解码字符串与原值相等，两个线程正常退出。
#[test]
fn ring_large_message_concurrent_pump_() {
    with_timeout_("ring_large_message_concurrent_pump_", || {
        let value = StrVal("p".repeat(4096));
        let expected = value.clone();
        let mut ring = new_ring_(128);
        let (mut writer, mut reader) = TestRing::split(&mut ring);

        std::thread::scope(|scope| {
            let producer = scope.spawn(move || {
                block_on_(async {
                    value
                        .encode_async(MpEncoder::new(&mut writer))
                        .may_cancel_with(NonCancellableToken::new())
                        .await
                })
                .expect("生产端编码应当成功");
            });

            let got: StrVal = block_on_(async {
                StrVal::decode_async(MpDecoder::new(&mut reader))
                    .may_cancel_with(NonCancellableToken::new())
                    .await
            })
            .expect("消费端解码应当成功");
            assert_eq!(got, expected, "并发 pump 后内容应当完整");
            producer.join().expect("生产线程不应 panic");
        });
    });
}

/// 验证同一 ring 上连续编码 3 个消息后可以逐值解码。
/// Verify a 3-message stream on one ring can be decoded value by value.
///
/// - 测试目标 / Goal：多消息流不需要复制缓冲，也不会多读下一个消息的字节。
/// - 手段 / Method：容量 4096，连续编码 3 个 `U64Val`；每次解码都复用同一个
///   `RingReader`。
/// - 判断 / Assert：3 个值按顺序相等；全部解码后环内数据量为 0。
#[test]
fn ring_multi_message_stream_() {
    let values = [U64Val(1), U64Val(300), U64Val(u64::MAX)];
    let mut ring = new_ring_(4096);
    for value in &values {
        encode_ring_(&mut ring, value);
    }

    let (_writer, mut reader) = TestRing::split(&mut ring);
    for expected in &values {
        let got: U64Val = block_on_(async {
            U64Val::decode_async(MpDecoder::new(&mut reader))
                .may_cancel_with(NonCancellableToken::new())
                .await
        })
        .expect("多消息流中的每个值都应当解码成功");
        assert_eq!(&got, expected, "消息顺序应当保持不变");
    }
    assert_eq!(
        reader.ring_state().data_size(),
        0,
        "3 个消息解码后 ring 应当为空"
    );
}

/// 验证读端在空 ring 上 park 时取消令牌能终止解码。
/// Verify cancellation terminates a decoder parked on an empty ring.
///
/// - 测试目标 / Goal：`RingReader::read_async` 的 park 点响应取消，不会挂死。
/// - 手段 / Method：空 ring 上开始解码 `U64Val`，另一线程在 20ms 后取消；
///   整体包在 5 秒超时里。
/// - 判断 / Assert：解码 future 以 `Err` 结束。
#[test]
fn ring_cancel_while_reader_parked_() {
    with_timeout_("ring_cancel_while_reader_parked_", || {
        let mut ring = new_ring_(64);
        let (_writer, mut reader) = TestRing::split(&mut ring);

        let source = CancellationSource::new();
        let token = source.child_token();
        std::thread::scope(|scope| {
            scope.spawn(|| {
                std::thread::sleep(Duration::from_millis(20));
                source.cancel();
            });
            let out: Result<U64Val, MpError> = block_on_(async {
                U64Val::decode_async(MpDecoder::new(&mut reader))
                    .may_cancel_with(token)
                    .await
            });
            assert!(out.is_err(), "读端 park 被取消后应返回 Err");
        });
    });
}

/// 验证写端在满 ring 上 park 时取消令牌能终止编码。
/// Verify cancellation terminates an encoder parked on a full ring.
///
/// - 测试目标 / Goal：`RingWriter::write_async` 的 park 点响应取消，不会挂死。
/// - 手段 / Method：容量 64 的 ring 先写满，再编码 `U64Val`；另一线程在
///   20ms 后取消，整体包在 5 秒超时里。
/// - 判断 / Assert：编码 future 以 `Err` 结束。
#[test]
fn ring_cancel_while_writer_full_() {
    with_timeout_("ring_cancel_while_writer_full_", || {
        let mut ring = new_ring_(64);
        let (mut writer, _reader) = TestRing::split(&mut ring);

        let filler = [0xffu8; 64];
        let fill_demand = Demand::exactly(64);
        let mut segm = writer
            .try_write(&fill_demand)
            .pick_left()
            .expect("空 ring 应当能借出 64 字节写入段");
        assert_eq!(segm.move_items_from_as_buff(&filler[..]), 64);
        drop(segm);

        let source = CancellationSource::new();
        let token = source.child_token();
        std::thread::scope(|scope| {
            scope.spawn(|| {
                std::thread::sleep(Duration::from_millis(20));
                source.cancel();
            });
            let out: Result<(), MpError> = block_on_(async {
                U64Val(7)
                    .encode_async(MpEncoder::new(&mut writer))
                    .may_cancel_with(token)
                    .await
            });
            assert!(out.is_err(), "写端 park 被取消后应返回 Err");
        });
    });
}
