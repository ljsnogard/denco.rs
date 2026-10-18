//! `json_denco` 的切片往返与 `serde_json` 互操作测试。
//!
//! 这些用例验证 JSON 表示与 `serde_json` 一致，并覆盖转义、Unicode、
//! 128 位整数、二进制数组与多值流。

#![feature(impl_trait_in_assoc_type)]
#![recursion_limit = "512"]

use abs_cancel::{NonCancellableToken, TrMayCancel};
use abs_denco::de::TrDecodable;
use json_denco::JsonDecoder;

mod support;
use support::{
    BytesVal, Inner, Payload, Shape, StrVal, U64Val, block_on_, decode_from_slice_, encode_to_vec_,
    sample_payload_,
};

/// 验证复合结构体与 `serde_json` 双向互通。
/// Verify a composite struct interoperates with `serde_json` in both directions.
///
/// - 测试目标 / Goal：`JsonEncoder` 的输出能被 `serde_json` 解析，反过来
///   `serde_json` 的输出也能被 `JsonDecoder` 解码。
/// - 手段 / Method：构造包含转义字符串、数组、Vec、Option、char、浮点与枚举
///   的 `Payload`，两种格式交叉编解码。
/// - 判断 / Assert：两条解码路径都得到与原值完全相等的结构体。
#[test]
fn payload_interop_() {
    for value in [
        sample_payload_(),
        Payload {
            name: String::new(),
            tags: Vec::new(),
            numbers: [0, 0, 0],
            bytes: Vec::new(),
            maybe: None,
            flag: false,
            letter: '\0',
            ratio: -0.5,
            score: 0.0,
            nested: Inner {
                id: 0,
                label: String::new(),
            },
            shape: Shape::Empty,
        },
    ] {
        let json_bytes = encode_to_vec_(&value);
        let from_json: Payload =
            serde_json::from_slice(&json_bytes).expect("serde_json 应能解析 json_denco 输出");
        assert_eq!(from_json, value, "serde_json 解码结果应与原值一致");

        let serde_bytes = serde_json::to_vec(&value).expect("serde_json 编码应当成功");
        let from_denco: Payload = decode_from_slice_(&serde_bytes);
        assert_eq!(from_denco, value, "json_denco 应能解码 serde_json 输出");
    }
}

/// 验证四种外部标签枚举变体与 `serde_json` 双向互通。
/// Verify all four externally tagged enum variants interoperate with `serde_json`.
///
/// - 测试目标 / Goal：unit/newtype/tuple/struct 变体的 JSON 表示与 serde 一致。
/// - 手段 / Method：对每个变体分别用 `json_denco` 与 `serde_json` 编码并交叉解码。
/// - 判断 / Assert：两个方向的解码结果都等于原变体。
#[test]
fn enum_interop_() {
    for value in [
        Shape::Empty,
        Shape::Text(String::from("hello \"world\"")),
        Shape::Pair(7, 9),
        Shape::Named {
            left: 11,
            right: String::from("中文"),
        },
    ] {
        let json_bytes = encode_to_vec_(&value);
        let from_json: Shape =
            serde_json::from_slice(&json_bytes).expect("serde_json 应能解析枚举输出");
        assert_eq!(from_json, value, "serde_json 枚举解码结果应一致");

        let serde_bytes = serde_json::to_vec(&value).expect("serde_json 枚举编码应当成功");
        let from_denco: Shape = decode_from_slice_(&serde_bytes);
        assert_eq!(from_denco, value, "json_denco 应能解码 serde_json 枚举输出");
    }
}

/// 验证标量、128 位整数与二进制数组的往返。
/// Verify scalars, 128-bit integers and byte arrays round-trip.
///
/// - 测试目标 / Goal：数字文本格式、`i128::MIN` / `u128::MAX` 与
///   `encode_bytes` / `decode_bytes` 都能正确往返。
/// - 手段 / Method：分别对每种值编码后再用同一个格式解码。
/// - 判断 / Assert：解码值与原值相等；二进制数组的 JSON 文本是数字数组。
#[test]
fn scalar_roundtrip_() {
    let u64_bytes = encode_to_vec_(&U64Val(u64::MAX));
    assert_eq!(u64_bytes, b"18446744073709551615");

    let i128_bytes = encode_to_vec_(&i128::MIN);
    let i128_back: i128 = decode_from_slice_(&i128_bytes);
    assert_eq!(i128_back, i128::MIN);

    let u128_bytes = encode_to_vec_(&u128::MAX);
    let u128_back: u128 = decode_from_slice_(&u128_bytes);
    assert_eq!(u128_back, u128::MAX);

    let float_bytes = encode_to_vec_(&1.0f64);
    assert_eq!(float_bytes, b"1.0", "整数值浮点应补 `.0`");
    let float_back: f64 = decode_from_slice_(&float_bytes);
    assert_eq!(float_back, 1.0f64);

    let bytes_value = BytesVal(vec![0, 1, 127, 255]);
    let bytes_json = encode_to_vec_(&bytes_value);
    assert_eq!(bytes_json, b"[0,1,127,255]");
    let bytes_back: BytesVal = decode_from_slice_(&bytes_json);
    assert_eq!(bytes_back, bytes_value);

    let str_value = StrVal(String::from("hello"));
    let str_json = encode_to_vec_(&str_value);
    assert_eq!(str_json, b"\"hello\"");
    let str_back: StrVal = decode_from_slice_(&str_json);
    assert_eq!(str_back, str_value);
}

/// 验证字符串转义与多字节字符往返。
/// Verify string escaping and multi-byte characters round-trip.
///
/// - 测试目标 / Goal：引号、反斜杠、控制字符、中文与 emoji 都按 JSON 规则转义。
/// - 手段 / Method：编码一个包含全部特殊字符的字符串，再用 `serde_json`
///   与 `json_denco` 分别解码。
/// - 判断 / Assert：两种解码路径都得到原字符串；输出是合法 JSON。
#[test]
fn string_escape_roundtrip_() {
    let value = String::from("quote=\" backslash=\\ newline=\n tab=\t ctrl=\u{1} 中文 😀");
    let json_bytes = encode_to_vec_(&value);

    let from_json: String =
        serde_json::from_slice(&json_bytes).expect("serde_json 应能解析转义字符串");
    assert_eq!(from_json, value);

    let from_denco: String = decode_from_slice_(&json_bytes);
    assert_eq!(from_denco, value);

    let _: serde_json::Value = serde_json::from_slice(&json_bytes).expect("输出必须是合法 JSON");
}

/// 验证 `\uXXXX` 与 UTF-16 代理对解码。
/// Verify `\uXXXX` escapes and UTF-16 surrogate pairs decode correctly.
///
/// - 测试目标 / Goal：解码器支持 `\u4e2d` 与 `\ud83d\ude00` 这类转义。
/// - 手段 / Method：直接构造包含转义序列的 JSON 字节串并解码为 `String`。
/// - 判断 / Assert：结果等于对应的中文与 emoji 字符。
#[test]
fn unicode_escape_decode_() {
    let json_bytes = br#""\u4e2d\u6587 \ud83d\ude00""#;
    let value: String = decode_from_slice_(json_bytes);
    assert_eq!(value, "中文 😀");
}

/// 验证同一解码器可以连续解码多个空白分隔的值。
/// Verify one decoder can decode several whitespace-separated values.
///
/// - 测试目标 / Goal：数字解析的单字节回推在多个值之间保持。
/// - 手段 / Method：把三个 `U64Val` 依次写进同一个切片（每个值后加空格），
///   再用同一个 `JsonDecoder` 逐个解码。
/// - 判断 / Assert：三个值按顺序解出且与输入一致。
#[test]
fn multi_value_stream_slice_() {
    let values = [U64Val(1), U64Val(300), U64Val(u64::MAX)];
    let mut storage = Vec::new();
    for value in &values {
        storage.extend_from_slice(&encode_to_vec_(value));
        storage.push(b' ');
    }
    let mut input: &[u8] = &storage[..];
    let mut decoder = JsonDecoder::new(&mut input);
    for expected in &values {
        let got: U64Val = block_on_(async {
            U64Val::decode_async(&mut decoder)
                .may_cancel_with(NonCancellableToken::new())
                .await
        })
        .expect("多值解码应当成功");
        assert_eq!(&got, expected);
    }
}

/// 验证每个消息新建解码器也不会丢失后续消息。
/// Verify a fresh decoder per message does not lose later messages.
///
/// - 测试目标 / Goal：解码器不再持有预读缓冲，完整顶层值结束后只可能留下
///   空白字节；因此逐消息新建解码器不会吞掉下一条消息。
/// - 手段 / Method：把三个 `U64Val` 用空格分隔写进切片，每解一个值都新建
///   `JsonDecoder`。
/// - 判断 / Assert：三个值按顺序解出且与输入一致。
#[test]
fn per_value_decoder_stream_() {
    let values = [U64Val(4), U64Val(900), U64Val(u64::MAX)];
    let mut storage = Vec::new();
    for value in &values {
        storage.extend_from_slice(&encode_to_vec_(value));
        storage.push(b' ');
    }
    let mut input: &[u8] = &storage[..];
    for expected in &values {
        let got: U64Val = block_on_(async {
            let mut decoder = JsonDecoder::new(&mut input);
            U64Val::decode_async(&mut decoder)
                .may_cancel_with(NonCancellableToken::new())
                .await
        })
        .expect("每个消息新建解码器时也应当解码成功");
        assert_eq!(&got, expected);
    }
}
