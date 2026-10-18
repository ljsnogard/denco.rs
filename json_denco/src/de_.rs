//! 流式 JSON 反序列化器。
//!
//! 解码器直接面向 [`abs_buff::TrBuffRead`] 工作：字节按需从读端借段读取，
//! 自身**不做预读缓冲**，只保留一个用于回推的字节。字符串、数字与数组内容
//! 一律解码成 owned 值，不借用读端内存。
//!
//! 长字符串等批量读取的效率由 [`TrBuffRead`] 的提供者负责：解码器只按语法
//! 需要向读端借段，不额外复制一层流缓冲。字符串内容会先用 `iter_slices()`
//! 预览段内普通字符并批量搬移，遇到引号、转义或控制字符再逐字节处理。
//!
//! 对同一个解码器的 `&mut` 引用实现了 [`TrDecoder`]，因此多消息流既可以复用
//! 同一个解码器，也可以在每个完整顶层值后新建解码器：
//!
//! ```ignore
//! let mut de = JsonDecoder::new(&mut reader);
//! let first = T::decode_async(&mut de).await?;
//! let second = T::decode_async(&mut de).await?;
//!
//! // 也可以在每个完整值后重新构造：
//! let mut de = JsonDecoder::new(&mut reader);
//! let second = T::decode_async(&mut de).await?;
//! ```

use alloc::{format, string::String, vec::Vec};
use core::str;

use abs_buff::{
    Demand, TrBuffRead,
    buffer::{TrBuffSegmRef, TrBuffSegmView},
};
use abs_cancel::{TrCancellationToken, TrMayCancel};
use abs_denco::{
    de::{EnumAccess, MapAccess, SeqAccess, TrDecodableSeed, TrDecoder, VariantAccess, Visitor},
    forward_to_decode_any,
};
use gen_mcf2::gen_may_cancel_future;

use crate::{JsonError, ser_::CancellableReady};

/// 一个在任意 [`TrBuffRead`] 上流式解析 JSON 的解码器。
///
/// `'r` 是读端的借用寿命。解码结果一律拥有所有权；解码器自身不做预读，
/// 只保留一个回推字节和读端结束状态。
pub struct JsonDecoder<'r, R> {
    reader_: &'r mut R,
    pending_: Option<u8>,
    ended_: bool,
    end_err_: Option<JsonError>,
}

impl<'r, R> JsonDecoder<'r, R> {
    /// 借用 `reader` 构造解码器。
    ///
    /// # Examples
    ///
    /// ```
    /// use json_denco::JsonDecoder;
    ///
    /// let bytes = [b'n', b'u', b'l', b'l'];
    /// let mut input: &[u8] = &bytes;
    /// let _decoder = JsonDecoder::new(&mut input);
    /// ```
    pub fn new(reader: &'r mut R) -> Self {
        Self {
            reader_: reader,
            pending_: None,
            ended_: false,
            end_err_: None,
        }
    }

    /// 返回解码器当前是否已经看到读端结束。
    pub fn is_ended(&self) -> bool {
        self.ended_
    }
}

fn unexpected_eof_() -> JsonError {
    JsonError::new("unexpected end of JSON input")
}

/// 从读端读取一个字节。
///
/// 解码器不保留读缓冲；`Demand::no_more_than(1)` 保证只消费一个字节，剩余
/// 字节由 [`TrBuffRead`] 的提供者保留。
///
/// 返回 `None` 表示读端已结束；底层错误保存在 `end_err_`，等真正需要字节时
/// 再交给调用方。
async fn read_one_async_<'r, R, C>(
    de: &mut JsonDecoder<'r, R>,
    cancel: C,
) -> Result<Option<u8>, JsonError>
where
    R: TrBuffRead<u8> + 'r,
    C: TrCancellationToken,
{
    if de.ended_ {
        return Ok(None);
    }
    if cancel.is_cancelled() {
        return Err(JsonError::new("operation cancelled"));
    }
    let demand = Demand::no_more_than(1);
    let mut some = de
        .reader_
        .read_async(&demand)
        .may_cancel_with(cancel.child_token())
        .await;
    let mut buf = [0u8; 1];
    let moved = match some.as_mut().pick_left() {
        Some(segm) => segm.move_items_to_as_buff(&mut buf[..]),
        None => 0,
    };
    if moved == 0 {
        de.ended_ = true;
        de.end_err_ = some.pick_right().map(JsonError::buff_read);
        return Ok(None);
    }
    Ok(Some(buf[0]))
}

/// 读一个字节；读端结束或出错时返回错误。
async fn read_byte_async_<'r, R, C>(de: &mut JsonDecoder<'r, R>, cancel: C) -> Result<u8, JsonError>
where
    R: TrBuffRead<u8> + 'r,
    C: TrCancellationToken,
{
    if let Some(byte) = de.pending_.take() {
        return Ok(byte);
    }
    match read_one_async_(de, cancel).await? {
        Some(byte) => Ok(byte),
        None => Err(de.end_err_.take().unwrap_or_else(unexpected_eof_)),
    }
}

/// 看下一个字节但不消费；读端结束时返回 `None`。
async fn peek_byte_async_<'r, R, C>(
    de: &mut JsonDecoder<'r, R>,
    cancel: C,
) -> Result<Option<u8>, JsonError>
where
    R: TrBuffRead<u8> + 'r,
    C: TrCancellationToken,
{
    if let Some(byte) = de.pending_ {
        return Ok(Some(byte));
    }
    match read_one_async_(de, cancel).await? {
        Some(byte) => {
            de.pending_ = Some(byte);
            Ok(Some(byte))
        }
        None => Ok(None),
    }
}

fn is_ws_(byte: u8) -> bool {
    matches!(byte, b' ' | b'\t' | b'\n' | b'\r')
}

async fn skip_ws_async_<'r, R, C>(de: &mut JsonDecoder<'r, R>, cancel: C) -> Result<(), JsonError>
where
    R: TrBuffRead<u8> + 'r,
    C: TrCancellationToken,
{
    loop {
        match peek_byte_async_(de, cancel.child_token()).await? {
            Some(byte) if is_ws_(byte) => {
                let _ = read_byte_async_(de, cancel.child_token()).await?;
            }
            _ => return Ok(()),
        }
    }
}

async fn expect_byte_async_<'r, R, C>(
    de: &mut JsonDecoder<'r, R>,
    expected: u8,
    cancel: C,
) -> Result<(), JsonError>
where
    R: TrBuffRead<u8> + 'r,
    C: TrCancellationToken,
{
    let byte = read_byte_async_(de, cancel).await?;
    if byte == expected {
        Ok(())
    } else {
        Err(JsonError::new(&format!(
            "unexpected byte in JSON input: expected {:?}, got {:?}",
            expected as char, byte as char
        )))
    }
}

async fn read_literal_async_<'r, R, C>(
    de: &mut JsonDecoder<'r, R>,
    literal: &'static [u8],
    cancel: C,
) -> Result<(), JsonError>
where
    R: TrBuffRead<u8> + 'r,
    C: TrCancellationToken,
{
    for &expected in literal {
        let byte = read_byte_async_(de, cancel.child_token()).await?;
        if byte != expected {
            return Err(JsonError::new("invalid JSON literal"));
        }
    }
    Ok(())
}

fn hex_value_(byte: u8) -> Option<u16> {
    match byte {
        b'0'..=b'9' => Some((byte - b'0') as u16),
        b'a'..=b'f' => Some((byte - b'a' + 10) as u16),
        b'A'..=b'F' => Some((byte - b'A' + 10) as u16),
        _ => None,
    }
}

async fn read_hex4_async_<'r, R, C>(
    de: &mut JsonDecoder<'r, R>,
    cancel: C,
) -> Result<u16, JsonError>
where
    R: TrBuffRead<u8> + 'r,
    C: TrCancellationToken,
{
    let mut value = 0u16;
    for _ in 0..4 {
        let byte = read_byte_async_(de, cancel.child_token()).await?;
        let digit = hex_value_(byte).ok_or_else(|| JsonError::new("invalid unicode escape"))?;
        value = (value << 4) | digit;
    }
    Ok(value)
}

/// 一次字符串扫描的结果。
enum StringRun {
    /// 当前段全部是普通字符，已经搬完，需要继续读下一段。
    More,
    /// 下一个字符是引号、反斜杠或控制字符，留给慢路径处理。
    Special,
}

/// 字符串快速路径每次最多向读端请求的字节数。
///
/// 这不是解码器缓冲：借到的段只用于预览和一次性搬移，段内未消费的字节会在
/// 段回收时留在读端。
const STRING_RUN_: usize = 4096;

/// 预览当前段，把引号/反斜杠/控制字符之前的普通字节一次性搬进 `out`。
///
/// 返回 [`StringRun::Special`] 表示段首（或搬移后的下一个字节）是特殊字符；
/// 返回 [`StringRun::More`] 表示当前段已全部搬完，需要继续借段。
async fn read_string_run_async_<'r, R, C>(
    de: &mut JsonDecoder<'r, R>,
    out: &mut Vec<u8>,
    cancel: C,
) -> Result<StringRun, JsonError>
where
    R: TrBuffRead<u8> + 'r,
    C: TrCancellationToken,
{
    if cancel.is_cancelled() {
        return Err(JsonError::new("operation cancelled"));
    }
    let demand = Demand::no_more_than(STRING_RUN_);
    let mut some = de
        .reader_
        .read_async(&demand)
        .may_cancel_with(cancel.child_token())
        .await;
    let segm = match some.as_mut().pick_left() {
        Some(segm) => segm,
        None => {
            de.ended_ = true;
            de.end_err_ = some.pick_right().map(JsonError::buff_read);
            return Err(de.end_err_.take().unwrap_or_else(unexpected_eof_));
        }
    };

    let mut total = 0usize;
    let mut safe = 0usize;
    let mut special = false;
    for slice in segm.iter_slices() {
        total += slice.len();
        if let Some(pos) = slice
            .iter()
            .position(|&byte| byte == b'"' || byte == b'\\' || byte < 0x20)
        {
            safe += pos;
            special = true;
            break;
        }
        safe += slice.len();
    }
    if total == 0 {
        return Err(JsonError::new("buff read returned an empty segment"));
    }

    if safe > 0 {
        let start = out.len();
        out.resize(start + safe, 0);
        let moved = segm.move_items_to_as_buff(&mut out[start..]);
        out.truncate(start + moved);
        if moved == 0 {
            return Err(JsonError::new("buff read returned an empty segment"));
        }
        if moved < safe {
            // 理论上游不会发生；保守地让外层继续从剩余字节读起。
            return Ok(StringRun::More);
        }
    }

    if special {
        Ok(StringRun::Special)
    } else {
        Ok(StringRun::More)
    }
}

/// 读取一个 JSON 字符串（调用方已经确认下一个非空白字符是 `"`）。
async fn read_string_value_async_<'r, R, C>(
    de: &mut JsonDecoder<'r, R>,
    cancel: C,
) -> Result<String, JsonError>
where
    R: TrBuffRead<u8> + 'r,
    C: TrCancellationToken,
{
    skip_ws_async_(de, cancel.child_token()).await?;
    expect_byte_async_(de, b'"', cancel.child_token()).await?;
    let mut out: Vec<u8> = Vec::new();
    loop {
        // 快速路径：一次预览并搬移一整段普通字符。若段首就是特殊字符，
        // 或回推字节还存在，则直接进入下面的单字节处理。
        if de.pending_.is_none() {
            match read_string_run_async_(de, &mut out, cancel.child_token()).await? {
                StringRun::More => continue,
                StringRun::Special => {}
            }
        }
        let byte = read_byte_async_(de, cancel.child_token()).await?;
        match byte {
            b'"' => break,
            b'\\' => {
                let escape = read_byte_async_(de, cancel.child_token()).await?;
                match escape {
                    b'"' | b'\\' | b'/' => out.push(escape),
                    b'b' => out.push(0x08),
                    b'f' => out.push(0x0c),
                    b'n' => out.push(b'\n'),
                    b'r' => out.push(b'\r'),
                    b't' => out.push(b'\t'),
                    b'u' => {
                        let first = read_hex4_async_(de, cancel.child_token()).await?;
                        let code = if (0xd800..=0xdbff).contains(&first) {
                            expect_byte_async_(de, b'\\', cancel.child_token()).await?;
                            expect_byte_async_(de, b'u', cancel.child_token()).await?;
                            let second = read_hex4_async_(de, cancel.child_token()).await?;
                            if !(0xdc00..=0xdfff).contains(&second) {
                                return Err(JsonError::new("invalid low surrogate"));
                            }
                            let high = (first as u32 - 0xd800) << 10;
                            let low = second as u32 - 0xdc00;
                            0x10000 + high + low
                        } else if (0xdc00..=0xdfff).contains(&first) {
                            return Err(JsonError::new("unexpected low surrogate"));
                        } else {
                            first as u32
                        };
                        let ch = char::from_u32(code)
                            .ok_or_else(|| JsonError::new("invalid unicode scalar value"))?;
                        let mut buf = [0u8; 4];
                        out.extend_from_slice(ch.encode_utf8(&mut buf).as_bytes());
                    }
                    _ => return Err(JsonError::new("invalid string escape")),
                }
            }
            0x00..=0x1f => {
                return Err(JsonError::new("unescaped control character in JSON string"));
            }
            _ => out.push(byte),
        }
    }
    String::from_utf8(out).map_err(|_| JsonError::new("invalid UTF-8 string"))
}

/// JSON 数字的解析结果。
#[derive(Clone, Copy, Debug)]
enum NumberToken {
    /// 非负整数。
    Pos(u128),
    /// 负整数（含 `-0`）。
    Neg(i128),
    /// 带小数或指数的浮点数。
    Float(f64),
}

/// 读取一个 JSON 数字并保留其文本，按语法选择整数或浮点解析。
async fn read_number_value_async_<'r, R, C>(
    de: &mut JsonDecoder<'r, R>,
    cancel: C,
) -> Result<NumberToken, JsonError>
where
    R: TrBuffRead<u8> + 'r,
    C: TrCancellationToken,
{
    skip_ws_async_(de, cancel.child_token()).await?;
    let mut text = String::new();
    let mut byte = read_byte_async_(de, cancel.child_token()).await?;
    if byte == b'-' {
        text.push('-');
        byte = read_byte_async_(de, cancel.child_token()).await?;
    }
    if byte == b'0' {
        text.push('0');
        if matches!(
            peek_byte_async_(de, cancel.child_token()).await?,
            Some(b'0'..=b'9')
        ) {
            return Err(JsonError::new("leading zero in JSON number"));
        }
    } else if byte.is_ascii_digit() {
        text.push(byte as char);
        while let Some(next) = peek_byte_async_(de, cancel.child_token()).await? {
            if next.is_ascii_digit() {
                text.push(read_byte_async_(de, cancel.child_token()).await? as char);
            } else {
                break;
            }
        }
    } else {
        return Err(JsonError::new("invalid JSON number"));
    }

    if matches!(
        peek_byte_async_(de, cancel.child_token()).await?,
        Some(b'.')
    ) {
        text.push(read_byte_async_(de, cancel.child_token()).await? as char);
        let digit = read_byte_async_(de, cancel.child_token()).await?;
        if !digit.is_ascii_digit() {
            return Err(JsonError::new("missing fraction digit in JSON number"));
        }
        text.push(digit as char);
        while let Some(next) = peek_byte_async_(de, cancel.child_token()).await? {
            if next.is_ascii_digit() {
                text.push(read_byte_async_(de, cancel.child_token()).await? as char);
            } else {
                break;
            }
        }
    }

    if matches!(
        peek_byte_async_(de, cancel.child_token()).await?,
        Some(b'e' | b'E')
    ) {
        text.push(read_byte_async_(de, cancel.child_token()).await? as char);
        if matches!(
            peek_byte_async_(de, cancel.child_token()).await?,
            Some(b'+' | b'-')
        ) {
            text.push(read_byte_async_(de, cancel.child_token()).await? as char);
        }
        let digit = read_byte_async_(de, cancel.child_token()).await?;
        if !digit.is_ascii_digit() {
            return Err(JsonError::new("missing exponent digit in JSON number"));
        }
        text.push(digit as char);
        while let Some(next) = peek_byte_async_(de, cancel.child_token()).await? {
            if next.is_ascii_digit() {
                text.push(read_byte_async_(de, cancel.child_token()).await? as char);
            } else {
                break;
            }
        }
    }

    if let Some(next) = peek_byte_async_(de, cancel.child_token()).await?
        && !is_ws_(next)
        && !matches!(next, b',' | b']' | b'}')
    {
        return Err(JsonError::new("invalid character after JSON number"));
    }

    if text.contains('.') || text.contains('e') || text.contains('E') {
        let value: f64 = text
            .parse()
            .map_err(|_| JsonError::new("invalid JSON floating point number"))?;
        if !value.is_finite() {
            return Err(JsonError::new("JSON number is out of range"));
        }
        return Ok(NumberToken::Float(value));
    }
    if text.starts_with('-') {
        let value: i128 = text
            .parse()
            .map_err(|_| JsonError::new("JSON integer is out of range"))?;
        Ok(NumberToken::Neg(value))
    } else {
        let value: u128 = text
            .parse()
            .map_err(|_| JsonError::new("JSON integer is out of range"))?;
        Ok(NumberToken::Pos(value))
    }
}

fn number_as_u128_(token: NumberToken) -> Result<u128, JsonError> {
    match token {
        NumberToken::Pos(value) => Ok(value),
        NumberToken::Neg(_) => Err(JsonError::new("expected an unsigned JSON integer")),
        NumberToken::Float(_) => Err(JsonError::new("expected a JSON integer, found a float")),
    }
}

fn number_as_i128_(token: NumberToken) -> Result<i128, JsonError> {
    match token {
        NumberToken::Pos(value) => {
            i128::try_from(value).map_err(|_| JsonError::new("JSON integer is out of range"))
        }
        NumberToken::Neg(value) => Ok(value),
        NumberToken::Float(_) => Err(JsonError::new("expected a JSON integer, found a float")),
    }
}

fn number_as_f64_(token: NumberToken) -> Result<f64, JsonError> {
    match token {
        NumberToken::Pos(value) => Ok(value as f64),
        NumberToken::Neg(value) => Ok(value as f64),
        NumberToken::Float(value) => Ok(value),
    }
}

/// 读取 JSON 字节数组（数字数组）为 `Vec<u8>`。
async fn read_byte_array_async_<'r, R, C>(
    de: &mut JsonDecoder<'r, R>,
    cancel: C,
) -> Result<Vec<u8>, JsonError>
where
    R: TrBuffRead<u8> + 'r,
    C: TrCancellationToken,
{
    skip_ws_async_(de, cancel.child_token()).await?;
    expect_byte_async_(de, b'[', cancel.child_token()).await?;
    let mut out = Vec::new();
    skip_ws_async_(de, cancel.child_token()).await?;
    if matches!(
        peek_byte_async_(de, cancel.child_token()).await?,
        Some(b']')
    ) {
        let _ = read_byte_async_(de, cancel.child_token()).await?;
        return Ok(out);
    }
    loop {
        let token = read_number_value_async_(de, cancel.child_token()).await?;
        let value = number_as_u128_(token)?;
        let byte = u8::try_from(value)
            .map_err(|_| JsonError::new("JSON byte array value is out of range"))?;
        out.push(byte);
        skip_ws_async_(de, cancel.child_token()).await?;
        match read_byte_async_(de, cancel.child_token()).await? {
            b',' => {
                skip_ws_async_(de, cancel.child_token()).await?;
            }
            b']' => break,
            _ => return Err(JsonError::new("expected `,` or `]` in JSON byte array")),
        }
    }
    Ok(out)
}

/// 解码模式：同一套解析逻辑按调用者期望的 Rust 类型选择 visitor 方法。
#[derive(Clone, Copy)]
enum ValueMode {
    Any,
    Bool,
    I8,
    I16,
    I32,
    I64,
    I128,
    U8,
    U16,
    U32,
    U64,
    U128,
    F32,
    F64,
    Char,
    Str,
    String,
    Bytes,
    ByteBuf,
    Option,
    Unit,
    UnitStruct,
    NewtypeStruct,
    Seq,
    Tuple,
    TupleStruct,
    Map,
    Struct,
    Enum,
    Identifier,
    IgnoredAny,
}

#[gen_may_cancel_future(DecodeValueOp, pub)]
async fn decode_value_async_<'r, 'de, 'a, R, V, C>(
    de: &'a mut JsonDecoder<'r, R>,
    mode: ValueMode,
    visitor: V,
    cancel: C,
) -> Result<V::Value, JsonError>
where
    'de: 'a,
    'r: 'a,
    R: TrBuffRead<u8> + 'r + 'a,
    V: Visitor<'de> + 'a,
    V::Value: 'a,
    C: TrCancellationToken,
{
    if cancel.is_cancelled() {
        return Err(JsonError::new("operation cancelled"));
    }
    match mode {
        ValueMode::Any | ValueMode::IgnoredAny => {
            decode_any_value_async_(de, visitor, cancel).await
        }
        ValueMode::Bool => {
            skip_ws_async_(de, cancel.child_token()).await?;
            let byte = read_byte_async_(de, cancel.child_token()).await?;
            let value = match byte {
                b't' => {
                    read_literal_async_(de, b"rue", cancel.child_token()).await?;
                    true
                }
                b'f' => {
                    read_literal_async_(de, b"alse", cancel.child_token()).await?;
                    false
                }
                _ => return Err(JsonError::new("expected a JSON boolean")),
            };
            visitor.visit_bool(value).may_cancel_with(cancel).await
        }
        ValueMode::I8 | ValueMode::I16 | ValueMode::I32 | ValueMode::I64 | ValueMode::I128 => {
            let token = read_number_value_async_(de, cancel.child_token()).await?;
            let value = number_as_i128_(token)?;
            if value >= i64::MIN as i128 && value <= i64::MAX as i128 {
                visitor
                    .visit_i64(value as i64)
                    .may_cancel_with(cancel)
                    .await
            } else {
                visitor.visit_i128(value).may_cancel_with(cancel).await
            }
        }
        ValueMode::U8 | ValueMode::U16 | ValueMode::U32 | ValueMode::U64 | ValueMode::U128 => {
            let token = read_number_value_async_(de, cancel.child_token()).await?;
            let value = number_as_u128_(token)?;
            if value <= u64::MAX as u128 {
                visitor
                    .visit_u64(value as u64)
                    .may_cancel_with(cancel)
                    .await
            } else {
                visitor.visit_u128(value).may_cancel_with(cancel).await
            }
        }
        ValueMode::F32 | ValueMode::F64 => {
            let token = read_number_value_async_(de, cancel.child_token()).await?;
            let value = number_as_f64_(token)?;
            visitor.visit_f64(value).may_cancel_with(cancel).await
        }
        ValueMode::Char | ValueMode::Str | ValueMode::String | ValueMode::Identifier => {
            let value = read_string_value_async_(de, cancel.child_token()).await?;
            match mode {
                ValueMode::String => visitor.visit_string(value).may_cancel_with(cancel).await,
                _ => visitor.visit_str(&value).may_cancel_with(cancel).await,
            }
        }
        ValueMode::Bytes | ValueMode::ByteBuf => {
            let value = read_byte_array_async_(de, cancel.child_token()).await?;
            match mode {
                ValueMode::Bytes => visitor.visit_bytes(&value).may_cancel_with(cancel).await,
                _ => visitor.visit_byte_buf(value).may_cancel_with(cancel).await,
            }
        }
        ValueMode::Option => {
            skip_ws_async_(de, cancel.child_token()).await?;
            if matches!(
                peek_byte_async_(de, cancel.child_token()).await?,
                Some(b'n')
            ) {
                read_literal_async_(de, b"null", cancel.child_token()).await?;
                visitor.visit_none().may_cancel_with(cancel).await
            } else {
                visitor.visit_some(&mut *de).may_cancel_with(cancel).await
            }
        }
        ValueMode::Unit | ValueMode::UnitStruct => {
            skip_ws_async_(de, cancel.child_token()).await?;
            read_literal_async_(de, b"null", cancel.child_token()).await?;
            visitor.visit_unit().may_cancel_with(cancel).await
        }
        ValueMode::NewtypeStruct => {
            skip_ws_async_(de, cancel.child_token()).await?;
            visitor
                .visit_newtype_struct(&mut *de)
                .may_cancel_with(cancel)
                .await
        }
        ValueMode::Seq | ValueMode::Tuple | ValueMode::TupleStruct => {
            visit_seq_async_(de, visitor, cancel).await
        }
        ValueMode::Map | ValueMode::Struct => visit_map_async_(de, visitor, cancel).await,
        ValueMode::Enum => decode_enum_value_async_(de, visitor, cancel).await,
    }
}

#[gen_may_cancel_future(DecodeAnyValueOp, pub)]
async fn decode_any_value_async_<'r, 'de, 'a, R, V, C>(
    de: &'a mut JsonDecoder<'r, R>,
    visitor: V,
    cancel: C,
) -> Result<V::Value, JsonError>
where
    'de: 'a,
    'r: 'a,
    R: TrBuffRead<u8> + 'r + 'a,
    V: Visitor<'de> + 'a,
    V::Value: 'a,
    C: TrCancellationToken,
{
    skip_ws_async_(de, cancel.child_token()).await?;
    let byte = peek_byte_async_(de, cancel.child_token()).await?;
    match byte {
        None => Err(unexpected_eof_()),
        Some(b'n') => {
            read_literal_async_(de, b"null", cancel.child_token()).await?;
            visitor.visit_unit().may_cancel_with(cancel).await
        }
        Some(b't') => {
            read_literal_async_(de, b"true", cancel.child_token()).await?;
            visitor.visit_bool(true).may_cancel_with(cancel).await
        }
        Some(b'f') => {
            read_literal_async_(de, b"false", cancel.child_token()).await?;
            visitor.visit_bool(false).may_cancel_with(cancel).await
        }
        Some(b'"') => {
            let value = read_string_value_async_(de, cancel.child_token()).await?;
            visitor.visit_string(value).may_cancel_with(cancel).await
        }
        Some(b'[') => visit_seq_async_(de, visitor, cancel).await,
        Some(b'{') => visit_map_async_(de, visitor, cancel).await,
        Some(next) if next == b'-' || next.is_ascii_digit() => {
            let token = read_number_value_async_(de, cancel.child_token()).await?;
            match token {
                NumberToken::Pos(value) if value <= u64::MAX as u128 => {
                    visitor
                        .visit_u64(value as u64)
                        .may_cancel_with(cancel)
                        .await
                }
                NumberToken::Pos(value) => visitor.visit_u128(value).may_cancel_with(cancel).await,
                NumberToken::Neg(value) if value >= i64::MIN as i128 => {
                    visitor
                        .visit_i64(value as i64)
                        .may_cancel_with(cancel)
                        .await
                }
                NumberToken::Neg(value) => visitor.visit_i128(value).may_cancel_with(cancel).await,
                NumberToken::Float(value) => visitor.visit_f64(value).may_cancel_with(cancel).await,
            }
        }
        Some(_) => Err(JsonError::new("unexpected character in JSON input")),
    }
}

#[gen_may_cancel_future(VisitSeqOp, pub)]
async fn visit_seq_async_<'r, 'de, 'a, R, V, C>(
    de: &'a mut JsonDecoder<'r, R>,
    visitor: V,
    cancel: C,
) -> Result<V::Value, JsonError>
where
    'de: 'a,
    'r: 'a,
    R: TrBuffRead<u8> + 'r + 'a,
    V: Visitor<'de> + 'a,
    V::Value: 'a,
    C: TrCancellationToken,
{
    skip_ws_async_(de, cancel.child_token()).await?;
    expect_byte_async_(de, b'[', cancel.child_token()).await?;
    let mut done = false;
    let access = JsonSeqAccess {
        de_: &mut *de,
        first_: true,
        done_: &mut done,
    };
    let value = visitor
        .visit_seq(access)
        .may_cancel_with(cancel.child_token())
        .await?;
    if !done {
        skip_ws_async_(de, cancel.child_token()).await?;
        expect_byte_async_(de, b']', cancel.child_token()).await?;
    }
    Ok(value)
}

#[gen_may_cancel_future(VisitMapOp, pub)]
async fn visit_map_async_<'r, 'de, 'a, R, V, C>(
    de: &'a mut JsonDecoder<'r, R>,
    visitor: V,
    cancel: C,
) -> Result<V::Value, JsonError>
where
    'de: 'a,
    'r: 'a,
    R: TrBuffRead<u8> + 'r + 'a,
    V: Visitor<'de> + 'a,
    V::Value: 'a,
    C: TrCancellationToken,
{
    skip_ws_async_(de, cancel.child_token()).await?;
    expect_byte_async_(de, b'{', cancel.child_token()).await?;
    let mut done = false;
    let access = JsonMapAccess {
        de_: &mut *de,
        first_: true,
        has_key_: false,
        done_: &mut done,
    };
    let value = visitor
        .visit_map(access)
        .may_cancel_with(cancel.child_token())
        .await?;
    if !done {
        skip_ws_async_(de, cancel.child_token()).await?;
        expect_byte_async_(de, b'}', cancel.child_token()).await?;
    }
    Ok(value)
}

/// JSON 数组的访问器。
struct JsonSeqAccess<'a, 'r, R> {
    de_: &'a mut JsonDecoder<'r, R>,
    first_: bool,
    done_: &'a mut bool,
}

impl<'de, 'r, R> SeqAccess<'de> for JsonSeqAccess<'_, 'r, R>
where
    R: TrBuffRead<u8> + 'r,
{
    type Error = JsonError;

    fn next_element_seed<'a, T>(
        &'a mut self,
        seed: T,
    ) -> impl TrMayCancel<'a, MayCancelOutput = Result<Option<T::Value>, Self::Error>>
    where
        'de: 'a,
        T: TrDecodableSeed<'de> + 'a,
        T::Value: 'a,
        Self: 'a,
        Self::Error: 'a,
    {
        DecodeElementOpAsync::<'r, 'de, 'a, 'a, R, T>::new(
            &mut *self.de_,
            &mut self.first_,
            &mut *self.done_,
            seed,
        )
    }

    fn size_hint(&self) -> Option<usize> {
        None
    }
}

/// JSON 对象的访问器。
struct JsonMapAccess<'a, 'r, R> {
    de_: &'a mut JsonDecoder<'r, R>,
    first_: bool,
    has_key_: bool,
    done_: &'a mut bool,
}

impl<'de, 'r, R> MapAccess<'de> for JsonMapAccess<'_, 'r, R>
where
    R: TrBuffRead<u8> + 'r,
{
    type Error = JsonError;

    fn next_key_seed<'a, K>(
        &'a mut self,
        seed: K,
    ) -> impl TrMayCancel<'a, MayCancelOutput = Result<Option<K::Value>, Self::Error>>
    where
        'de: 'a,
        K: TrDecodableSeed<'de> + 'a,
        K::Value: 'a,
        Self: 'a,
        Self::Error: 'a,
    {
        DecodeKeyOpAsync::<'r, 'de, 'a, 'a, R, K>::new(
            &mut *self.de_,
            &mut self.first_,
            &mut self.has_key_,
            &mut *self.done_,
            seed,
        )
    }

    fn next_value_seed<'a, V>(
        &'a mut self,
        seed: V,
    ) -> impl TrMayCancel<'a, MayCancelOutput = Result<V::Value, Self::Error>>
    where
        'de: 'a,
        V: TrDecodableSeed<'de> + 'a,
        V::Value: 'a,
        Self: 'a,
        Self::Error: 'a,
    {
        DecodeEntryValueOpAsync::<'r, 'de, 'a, 'a, R, V>::new(
            &mut *self.de_,
            &mut self.has_key_,
            seed,
        )
    }

    fn size_hint(&self) -> Option<usize> {
        None
    }
}

#[gen_may_cancel_future(DecodeElementOp, pub)]
async fn decode_element_async_<'r, 'de, 'a, R, T, C>(
    de: &'a mut JsonDecoder<'r, R>,
    first: &'a mut bool,
    done: &'a mut bool,
    seed: T,
    cancel: C,
) -> Result<Option<T::Value>, JsonError>
where
    'de: 'a,
    'r: 'a,
    R: TrBuffRead<u8> + 'r + 'a,
    T: TrDecodableSeed<'de> + 'a,
    T::Value: 'a,
    C: TrCancellationToken,
{
    skip_ws_async_(de, cancel.child_token()).await?;
    if *first {
        if matches!(
            peek_byte_async_(de, cancel.child_token()).await?,
            Some(b']')
        ) {
            let _ = read_byte_async_(de, cancel.child_token()).await?;
            *done = true;
            return Ok(None);
        }
        *first = false;
    } else {
        skip_ws_async_(de, cancel.child_token()).await?;
        match peek_byte_async_(de, cancel.child_token()).await? {
            Some(b']') => {
                let _ = read_byte_async_(de, cancel.child_token()).await?;
                *done = true;
                return Ok(None);
            }
            Some(b',') => {
                let _ = read_byte_async_(de, cancel.child_token()).await?;
                skip_ws_async_(de, cancel.child_token()).await?;
                if matches!(
                    peek_byte_async_(de, cancel.child_token()).await?,
                    Some(b']')
                ) {
                    return Err(JsonError::new("trailing comma in JSON array"));
                }
            }
            _ => return Err(JsonError::new("expected `,` or `]` in JSON array")),
        }
    }
    let value = seed.decode_async(&mut *de).may_cancel_with(cancel).await?;
    Ok(Some(value))
}

#[gen_may_cancel_future(DecodeKeyOp, pub)]
async fn decode_key_async_<'r, 'de, 'a, R, K, C>(
    de: &'a mut JsonDecoder<'r, R>,
    first: &'a mut bool,
    has_key: &'a mut bool,
    done: &'a mut bool,
    seed: K,
    cancel: C,
) -> Result<Option<K::Value>, JsonError>
where
    'de: 'a,
    'r: 'a,
    R: TrBuffRead<u8> + 'r + 'a,
    K: TrDecodableSeed<'de> + 'a,
    K::Value: 'a,
    C: TrCancellationToken,
{
    if *has_key {
        return Err(JsonError::new("JSON object value expected before next key"));
    }
    skip_ws_async_(de, cancel.child_token()).await?;
    if *first {
        if matches!(
            peek_byte_async_(de, cancel.child_token()).await?,
            Some(b'}')
        ) {
            let _ = read_byte_async_(de, cancel.child_token()).await?;
            *done = true;
            return Ok(None);
        }
        *first = false;
    } else {
        skip_ws_async_(de, cancel.child_token()).await?;
        match peek_byte_async_(de, cancel.child_token()).await? {
            Some(b'}') => {
                let _ = read_byte_async_(de, cancel.child_token()).await?;
                *done = true;
                return Ok(None);
            }
            Some(b',') => {
                let _ = read_byte_async_(de, cancel.child_token()).await?;
                skip_ws_async_(de, cancel.child_token()).await?;
                if matches!(
                    peek_byte_async_(de, cancel.child_token()).await?,
                    Some(b'}')
                ) {
                    return Err(JsonError::new("trailing comma in JSON object"));
                }
            }
            _ => return Err(JsonError::new("expected `,` or `}` in JSON object")),
        }
    }
    let name = read_string_value_async_(de, cancel.child_token()).await?;
    *has_key = true;
    let value = seed
        .decode_async(JsonNameDecoder { name_: name })
        .may_cancel_with(cancel)
        .await?;
    Ok(Some(value))
}

#[gen_may_cancel_future(DecodeEntryValueOp, pub)]
async fn decode_entry_value_async_<'r, 'de, 'a, R, V, C>(
    de: &'a mut JsonDecoder<'r, R>,
    has_key: &'a mut bool,
    seed: V,
    cancel: C,
) -> Result<V::Value, JsonError>
where
    'de: 'a,
    'r: 'a,
    R: TrBuffRead<u8> + 'r + 'a,
    V: TrDecodableSeed<'de> + 'a,
    V::Value: 'a,
    C: TrCancellationToken,
{
    if !*has_key {
        return Err(JsonError::new("JSON object value without a key"));
    }
    *has_key = false;
    expect_byte_async_(de, b':', cancel.child_token()).await?;
    seed.decode_async(&mut *de).may_cancel_with(cancel).await
}

/// JSON 枚举的访问器。
struct JsonEnumAccess<'a, 'r, R> {
    de_: &'a mut JsonDecoder<'r, R>,
    name_: String,
    unit_: bool,
}

impl<'de, 's, 'r, R> EnumAccess<'de> for JsonEnumAccess<'s, 'r, R>
where
    R: TrBuffRead<u8> + 'r,
{
    type Error = JsonError;
    type Variant = JsonVariantAccess<'s, 'r, R>;

    fn variant_seed<'a, V>(
        self,
        seed: V,
    ) -> impl TrMayCancel<'a, MayCancelOutput = Result<(V::Value, Self::Variant), Self::Error>>
    where
        'de: 'a,
        V: TrDecodableSeed<'de> + 'a,
        V::Value: 'a,
        Self: 'a,
        Self::Variant: 'a,
        Self::Error: 'a,
    {
        EnumVariantSeedOpAsync::<'r, 'de, 's, 'a, 'a, R, V>::new(
            self.de_, self.name_, self.unit_, seed,
        )
    }
}

/// JSON 枚举变体内容的访问器。
pub struct JsonVariantAccess<'a, 'r, R> {
    de_: &'a mut JsonDecoder<'r, R>,
    unit_: bool,
}

impl<'de, 's, 'r, R> VariantAccess<'de> for JsonVariantAccess<'s, 'r, R>
where
    R: TrBuffRead<u8> + 'r,
{
    type Error = JsonError;

    fn unit_variant<'a>(self) -> impl TrMayCancel<'a, MayCancelOutput = Result<(), Self::Error>>
    where
        'de: 'a,
        Self: 'a,
        Self::Error: 'a,
    {
        UnitVariantOpAsync::<'r, 's, 'a, 'a, R>::new(self.de_, self.unit_)
    }

    fn newtype_variant_seed<'a, T>(
        self,
        seed: T,
    ) -> impl TrMayCancel<'a, MayCancelOutput = Result<T::Value, Self::Error>>
    where
        'de: 'a,
        T: TrDecodableSeed<'de> + 'a,
        T::Value: 'a,
        Self: 'a,
        Self::Error: 'a,
    {
        NewtypeVariantOpAsync::<'r, 'de, 's, 'a, 'a, R, T>::new(self.de_, self.unit_, seed)
    }

    fn tuple_variant<'a, V>(
        self,
        len: usize,
        visitor: V,
    ) -> impl TrMayCancel<'a, MayCancelOutput = Result<V::Value, Self::Error>>
    where
        'de: 'a,
        V: Visitor<'de> + 'a,
        V::Value: 'a,
        Self: 'a,
        Self::Error: 'a,
    {
        TupleVariantOpAsync::<'r, 'de, 's, 'a, 'a, R, V>::new(self.de_, self.unit_, len, visitor)
    }

    fn struct_variant<'a, V>(
        self,
        fields: &'static [&'static str],
        visitor: V,
    ) -> impl TrMayCancel<'a, MayCancelOutput = Result<V::Value, Self::Error>>
    where
        'de: 'a,
        V: Visitor<'de> + 'a,
        V::Value: 'a,
        Self: 'a,
        Self::Error: 'a,
    {
        StructVariantOpAsync::<'r, 'de, 's, 'a, 'a, R, V>::new(
            self.de_, self.unit_, fields, visitor,
        )
    }
}

#[gen_may_cancel_future(DecodeEnumOp, pub)]
async fn decode_enum_value_async_<'r, 'de, 'a, R, V, C>(
    de: &'a mut JsonDecoder<'r, R>,
    visitor: V,
    cancel: C,
) -> Result<V::Value, JsonError>
where
    'de: 'a,
    'r: 'a,
    R: TrBuffRead<u8> + 'r + 'a,
    V: Visitor<'de> + 'a,
    V::Value: 'a,
    C: TrCancellationToken,
{
    skip_ws_async_(de, cancel.child_token()).await?;
    let byte = peek_byte_async_(de, cancel.child_token()).await?;
    let (name, unit) = match byte {
        Some(b'"') => (
            read_string_value_async_(de, cancel.child_token()).await?,
            true,
        ),
        Some(b'{') => {
            expect_byte_async_(de, b'{', cancel.child_token()).await?;
            skip_ws_async_(de, cancel.child_token()).await?;
            let name = read_string_value_async_(de, cancel.child_token()).await?;
            skip_ws_async_(de, cancel.child_token()).await?;
            expect_byte_async_(de, b':', cancel.child_token()).await?;
            (name, false)
        }
        _ => return Err(JsonError::new("expected a JSON enum variant")),
    };
    let access = JsonEnumAccess {
        de_: &mut *de,
        name_: name,
        unit_: unit,
    };
    visitor.visit_enum(access).may_cancel_with(cancel).await
}

#[gen_may_cancel_future(EnumVariantSeedOp, pub)]
async fn enum_variant_seed_async_<'r, 'de, 's, 'a, R, V, C>(
    de: &'s mut JsonDecoder<'r, R>,
    name: String,
    unit: bool,
    seed: V,
    cancel: C,
) -> Result<(V::Value, JsonVariantAccess<'s, 'r, R>), JsonError>
where
    'de: 'a,
    'r: 's + 'a,
    's: 'a,
    R: TrBuffRead<u8> + 'r,
    V: TrDecodableSeed<'de> + 'a,
    V::Value: 'a,
    C: TrCancellationToken,
{
    let value = seed
        .decode_async(JsonNameDecoder { name_: name })
        .may_cancel_with(cancel)
        .await?;
    Ok((
        value,
        JsonVariantAccess {
            de_: de,
            unit_: unit,
        },
    ))
}

#[gen_may_cancel_future(UnitVariantOp, pub)]
async fn unit_variant_async_<'r, 's, 'a, R, C>(
    de: &'s mut JsonDecoder<'r, R>,
    unit: bool,
    cancel: C,
) -> Result<(), JsonError>
where
    'r: 'a,
    's: 'a,
    R: TrBuffRead<u8> + 'r,
    C: TrCancellationToken,
{
    if unit {
        return Ok(());
    }
    skip_ws_async_(de, cancel.child_token()).await?;
    read_literal_async_(de, b"null", cancel.child_token()).await?;
    skip_ws_async_(de, cancel.child_token()).await?;
    expect_byte_async_(de, b'}', cancel).await
}

#[gen_may_cancel_future(NewtypeVariantOp, pub)]
async fn newtype_variant_async_<'r, 'de, 's, 'a, R, T, C>(
    de: &'s mut JsonDecoder<'r, R>,
    unit: bool,
    seed: T,
    cancel: C,
) -> Result<T::Value, JsonError>
where
    'de: 'a,
    'r: 'a,
    's: 'a,
    R: TrBuffRead<u8> + 'r,
    T: TrDecodableSeed<'de> + 'a,
    T::Value: 'a,
    C: TrCancellationToken,
{
    if unit {
        return Err(JsonError::new("unit variant has no newtype content"));
    }
    let value = seed
        .decode_async(&mut *de)
        .may_cancel_with(cancel.child_token())
        .await?;
    skip_ws_async_(de, cancel.child_token()).await?;
    expect_byte_async_(de, b'}', cancel.child_token()).await?;
    Ok(value)
}

#[gen_may_cancel_future(TupleVariantOp, pub)]
async fn tuple_variant_async_<'r, 'de, 's, 'a, R, V, C>(
    de: &'s mut JsonDecoder<'r, R>,
    unit: bool,
    len: usize,
    visitor: V,
    cancel: C,
) -> Result<V::Value, JsonError>
where
    'de: 'a,
    'r: 'a,
    's: 'a,
    R: TrBuffRead<u8> + 'r,
    V: Visitor<'de> + 'a,
    V::Value: 'a,
    C: TrCancellationToken,
{
    if unit {
        return Err(JsonError::new("unit variant has no tuple content"));
    }
    let value = (&mut *de)
        .decode_tuple(len, visitor)
        .may_cancel_with(cancel.child_token())
        .await?;
    skip_ws_async_(de, cancel.child_token()).await?;
    expect_byte_async_(de, b'}', cancel.child_token()).await?;
    Ok(value)
}

#[gen_may_cancel_future(StructVariantOp, pub)]
async fn struct_variant_async_<'r, 'de, 's, 'a, R, V, C>(
    de: &'s mut JsonDecoder<'r, R>,
    unit: bool,
    fields: &'static [&'static str],
    visitor: V,
    cancel: C,
) -> Result<V::Value, JsonError>
where
    'de: 'a,
    'r: 'a,
    's: 'a,
    R: TrBuffRead<u8> + 'r,
    V: Visitor<'de> + 'a,
    V::Value: 'a,
    C: TrCancellationToken,
{
    if unit {
        return Err(JsonError::new("unit variant has no struct content"));
    }
    let value = (&mut *de)
        .decode_struct("", fields, visitor)
        .may_cancel_with(cancel.child_token())
        .await?;
    skip_ws_async_(de, cancel.child_token()).await?;
    expect_byte_async_(de, b'}', cancel.child_token()).await?;
    Ok(value)
}

impl<'de, 'r, R> TrDecoder<'de> for &mut JsonDecoder<'r, R>
where
    R: TrBuffRead<u8> + 'r,
{
    type Error = JsonError;

    type DecodeAnyAsync<'a, V>
        = DecodeValueOpAsync<'r, 'de, 'a, 'a, R, V>
    where
        'de: 'a,
        'r: 'a,
        V: Visitor<'de> + 'a,
        V::Value: 'a,
        Self: 'a,
        Self::Error: 'a;

    fn decode_any<'a, V>(self, visitor: V) -> Self::DecodeAnyAsync<'a, V>
    where
        'de: 'a,
        V: Visitor<'de> + 'a,
        V::Value: 'a,
        Self: 'a,
        Self::Error: 'a,
    {
        DecodeValueOpAsync::<'r, 'de, 'a, 'a, R, V>::new(self, ValueMode::Any, visitor)
    }

    /// 所有 `decode_*` 方法共用同一个解析状态机，只是模式不同。
    type DecodeBoolAsync<'a, V>
        = DecodeValueOpAsync<'r, 'de, 'a, 'a, R, V>
    where
        'de: 'a,
        'r: 'a,
        V: Visitor<'de> + 'a,
        V::Value: 'a,
        Self: 'a,
        Self::Error: 'a;

    fn decode_bool<'a, V>(self, visitor: V) -> Self::DecodeBoolAsync<'a, V>
    where
        'de: 'a,
        V: Visitor<'de> + 'a,
        V::Value: 'a,
        Self: 'a,
        Self::Error: 'a,
    {
        DecodeValueOpAsync::<'r, 'de, 'a, 'a, R, V>::new(self, ValueMode::Bool, visitor)
    }

    type DecodeI8Async<'a, V>
        = DecodeValueOpAsync<'r, 'de, 'a, 'a, R, V>
    where
        'de: 'a,
        'r: 'a,
        V: Visitor<'de> + 'a,
        V::Value: 'a,
        Self: 'a,
        Self::Error: 'a;

    fn decode_i8<'a, V>(self, visitor: V) -> Self::DecodeI8Async<'a, V>
    where
        'de: 'a,
        V: Visitor<'de> + 'a,
        V::Value: 'a,
        Self: 'a,
        Self::Error: 'a,
    {
        DecodeValueOpAsync::<'r, 'de, 'a, 'a, R, V>::new(self, ValueMode::I8, visitor)
    }

    type DecodeI16Async<'a, V>
        = DecodeValueOpAsync<'r, 'de, 'a, 'a, R, V>
    where
        'de: 'a,
        'r: 'a,
        V: Visitor<'de> + 'a,
        V::Value: 'a,
        Self: 'a,
        Self::Error: 'a;

    fn decode_i16<'a, V>(self, visitor: V) -> Self::DecodeI16Async<'a, V>
    where
        'de: 'a,
        V: Visitor<'de> + 'a,
        V::Value: 'a,
        Self: 'a,
        Self::Error: 'a,
    {
        DecodeValueOpAsync::<'r, 'de, 'a, 'a, R, V>::new(self, ValueMode::I16, visitor)
    }

    type DecodeI32Async<'a, V>
        = DecodeValueOpAsync<'r, 'de, 'a, 'a, R, V>
    where
        'de: 'a,
        'r: 'a,
        V: Visitor<'de> + 'a,
        V::Value: 'a,
        Self: 'a,
        Self::Error: 'a;

    fn decode_i32<'a, V>(self, visitor: V) -> Self::DecodeI32Async<'a, V>
    where
        'de: 'a,
        V: Visitor<'de> + 'a,
        V::Value: 'a,
        Self: 'a,
        Self::Error: 'a,
    {
        DecodeValueOpAsync::<'r, 'de, 'a, 'a, R, V>::new(self, ValueMode::I32, visitor)
    }

    type DecodeI64Async<'a, V>
        = DecodeValueOpAsync<'r, 'de, 'a, 'a, R, V>
    where
        'de: 'a,
        'r: 'a,
        V: Visitor<'de> + 'a,
        V::Value: 'a,
        Self: 'a,
        Self::Error: 'a;

    fn decode_i64<'a, V>(self, visitor: V) -> Self::DecodeI64Async<'a, V>
    where
        'de: 'a,
        V: Visitor<'de> + 'a,
        V::Value: 'a,
        Self: 'a,
        Self::Error: 'a,
    {
        DecodeValueOpAsync::<'r, 'de, 'a, 'a, R, V>::new(self, ValueMode::I64, visitor)
    }

    type DecodeI128Async<'a, V>
        = DecodeValueOpAsync<'r, 'de, 'a, 'a, R, V>
    where
        'de: 'a,
        'r: 'a,
        V: Visitor<'de> + 'a,
        V::Value: 'a,
        Self: 'a,
        Self::Error: 'a;

    fn decode_i128<'a, V>(self, visitor: V) -> Self::DecodeI128Async<'a, V>
    where
        'de: 'a,
        V: Visitor<'de> + 'a,
        V::Value: 'a,
        Self: 'a,
        Self::Error: 'a,
    {
        DecodeValueOpAsync::<'r, 'de, 'a, 'a, R, V>::new(self, ValueMode::I128, visitor)
    }

    type DecodeU8Async<'a, V>
        = DecodeValueOpAsync<'r, 'de, 'a, 'a, R, V>
    where
        'de: 'a,
        'r: 'a,
        V: Visitor<'de> + 'a,
        V::Value: 'a,
        Self: 'a,
        Self::Error: 'a;

    fn decode_u8<'a, V>(self, visitor: V) -> Self::DecodeU8Async<'a, V>
    where
        'de: 'a,
        V: Visitor<'de> + 'a,
        V::Value: 'a,
        Self: 'a,
        Self::Error: 'a,
    {
        DecodeValueOpAsync::<'r, 'de, 'a, 'a, R, V>::new(self, ValueMode::U8, visitor)
    }

    type DecodeU16Async<'a, V>
        = DecodeValueOpAsync<'r, 'de, 'a, 'a, R, V>
    where
        'de: 'a,
        'r: 'a,
        V: Visitor<'de> + 'a,
        V::Value: 'a,
        Self: 'a,
        Self::Error: 'a;

    fn decode_u16<'a, V>(self, visitor: V) -> Self::DecodeU16Async<'a, V>
    where
        'de: 'a,
        V: Visitor<'de> + 'a,
        V::Value: 'a,
        Self: 'a,
        Self::Error: 'a,
    {
        DecodeValueOpAsync::<'r, 'de, 'a, 'a, R, V>::new(self, ValueMode::U16, visitor)
    }

    type DecodeU32Async<'a, V>
        = DecodeValueOpAsync<'r, 'de, 'a, 'a, R, V>
    where
        'de: 'a,
        'r: 'a,
        V: Visitor<'de> + 'a,
        V::Value: 'a,
        Self: 'a,
        Self::Error: 'a;

    fn decode_u32<'a, V>(self, visitor: V) -> Self::DecodeU32Async<'a, V>
    where
        'de: 'a,
        V: Visitor<'de> + 'a,
        V::Value: 'a,
        Self: 'a,
        Self::Error: 'a,
    {
        DecodeValueOpAsync::<'r, 'de, 'a, 'a, R, V>::new(self, ValueMode::U32, visitor)
    }

    type DecodeU64Async<'a, V>
        = DecodeValueOpAsync<'r, 'de, 'a, 'a, R, V>
    where
        'de: 'a,
        'r: 'a,
        V: Visitor<'de> + 'a,
        V::Value: 'a,
        Self: 'a,
        Self::Error: 'a;

    fn decode_u64<'a, V>(self, visitor: V) -> Self::DecodeU64Async<'a, V>
    where
        'de: 'a,
        V: Visitor<'de> + 'a,
        V::Value: 'a,
        Self: 'a,
        Self::Error: 'a,
    {
        DecodeValueOpAsync::<'r, 'de, 'a, 'a, R, V>::new(self, ValueMode::U64, visitor)
    }

    type DecodeU128Async<'a, V>
        = DecodeValueOpAsync<'r, 'de, 'a, 'a, R, V>
    where
        'de: 'a,
        'r: 'a,
        V: Visitor<'de> + 'a,
        V::Value: 'a,
        Self: 'a,
        Self::Error: 'a;

    fn decode_u128<'a, V>(self, visitor: V) -> Self::DecodeU128Async<'a, V>
    where
        'de: 'a,
        V: Visitor<'de> + 'a,
        V::Value: 'a,
        Self: 'a,
        Self::Error: 'a,
    {
        DecodeValueOpAsync::<'r, 'de, 'a, 'a, R, V>::new(self, ValueMode::U128, visitor)
    }

    type DecodeF32Async<'a, V>
        = DecodeValueOpAsync<'r, 'de, 'a, 'a, R, V>
    where
        'de: 'a,
        'r: 'a,
        V: Visitor<'de> + 'a,
        V::Value: 'a,
        Self: 'a,
        Self::Error: 'a;

    fn decode_f32<'a, V>(self, visitor: V) -> Self::DecodeF32Async<'a, V>
    where
        'de: 'a,
        V: Visitor<'de> + 'a,
        V::Value: 'a,
        Self: 'a,
        Self::Error: 'a,
    {
        DecodeValueOpAsync::<'r, 'de, 'a, 'a, R, V>::new(self, ValueMode::F32, visitor)
    }

    type DecodeF64Async<'a, V>
        = DecodeValueOpAsync<'r, 'de, 'a, 'a, R, V>
    where
        'de: 'a,
        'r: 'a,
        V: Visitor<'de> + 'a,
        V::Value: 'a,
        Self: 'a,
        Self::Error: 'a;

    fn decode_f64<'a, V>(self, visitor: V) -> Self::DecodeF64Async<'a, V>
    where
        'de: 'a,
        V: Visitor<'de> + 'a,
        V::Value: 'a,
        Self: 'a,
        Self::Error: 'a,
    {
        DecodeValueOpAsync::<'r, 'de, 'a, 'a, R, V>::new(self, ValueMode::F64, visitor)
    }

    type DecodeCharAsync<'a, V>
        = DecodeValueOpAsync<'r, 'de, 'a, 'a, R, V>
    where
        'de: 'a,
        'r: 'a,
        V: Visitor<'de> + 'a,
        V::Value: 'a,
        Self: 'a,
        Self::Error: 'a;

    fn decode_char<'a, V>(self, visitor: V) -> Self::DecodeCharAsync<'a, V>
    where
        'de: 'a,
        V: Visitor<'de> + 'a,
        V::Value: 'a,
        Self: 'a,
        Self::Error: 'a,
    {
        DecodeValueOpAsync::<'r, 'de, 'a, 'a, R, V>::new(self, ValueMode::Char, visitor)
    }

    type DecodeStrAsync<'a, V>
        = DecodeValueOpAsync<'r, 'de, 'a, 'a, R, V>
    where
        'de: 'a,
        'r: 'a,
        V: Visitor<'de> + 'a,
        V::Value: 'a,
        Self: 'a,
        Self::Error: 'a;

    fn decode_str<'a, V>(self, visitor: V) -> Self::DecodeStrAsync<'a, V>
    where
        'de: 'a,
        V: Visitor<'de> + 'a,
        V::Value: 'a,
        Self: 'a,
        Self::Error: 'a,
    {
        DecodeValueOpAsync::<'r, 'de, 'a, 'a, R, V>::new(self, ValueMode::Str, visitor)
    }

    type DecodeStringAsync<'a, V>
        = DecodeValueOpAsync<'r, 'de, 'a, 'a, R, V>
    where
        'de: 'a,
        'r: 'a,
        V: Visitor<'de> + 'a,
        V::Value: 'a,
        Self: 'a,
        Self::Error: 'a;

    fn decode_string<'a, V>(self, visitor: V) -> Self::DecodeStringAsync<'a, V>
    where
        'de: 'a,
        V: Visitor<'de> + 'a,
        V::Value: 'a,
        Self: 'a,
        Self::Error: 'a,
    {
        DecodeValueOpAsync::<'r, 'de, 'a, 'a, R, V>::new(self, ValueMode::String, visitor)
    }

    type DecodeBytesAsync<'a, V>
        = DecodeValueOpAsync<'r, 'de, 'a, 'a, R, V>
    where
        'de: 'a,
        'r: 'a,
        V: Visitor<'de> + 'a,
        V::Value: 'a,
        Self: 'a,
        Self::Error: 'a;

    fn decode_bytes<'a, V>(self, visitor: V) -> Self::DecodeBytesAsync<'a, V>
    where
        'de: 'a,
        V: Visitor<'de> + 'a,
        V::Value: 'a,
        Self: 'a,
        Self::Error: 'a,
    {
        DecodeValueOpAsync::<'r, 'de, 'a, 'a, R, V>::new(self, ValueMode::Bytes, visitor)
    }

    type DecodeByteBufAsync<'a, V>
        = DecodeValueOpAsync<'r, 'de, 'a, 'a, R, V>
    where
        'de: 'a,
        'r: 'a,
        V: Visitor<'de> + 'a,
        V::Value: 'a,
        Self: 'a,
        Self::Error: 'a;

    fn decode_byte_buf<'a, V>(self, visitor: V) -> Self::DecodeByteBufAsync<'a, V>
    where
        'de: 'a,
        V: Visitor<'de> + 'a,
        V::Value: 'a,
        Self: 'a,
        Self::Error: 'a,
    {
        DecodeValueOpAsync::<'r, 'de, 'a, 'a, R, V>::new(self, ValueMode::ByteBuf, visitor)
    }

    type DecodeOptionAsync<'a, V>
        = DecodeValueOpAsync<'r, 'de, 'a, 'a, R, V>
    where
        'de: 'a,
        'r: 'a,
        V: Visitor<'de> + 'a,
        V::Value: 'a,
        Self: 'a,
        Self::Error: 'a;

    fn decode_option<'a, V>(self, visitor: V) -> Self::DecodeOptionAsync<'a, V>
    where
        'de: 'a,
        V: Visitor<'de> + 'a,
        V::Value: 'a,
        Self: 'a,
        Self::Error: 'a,
    {
        DecodeValueOpAsync::<'r, 'de, 'a, 'a, R, V>::new(self, ValueMode::Option, visitor)
    }

    type DecodeUnitAsync<'a, V>
        = DecodeValueOpAsync<'r, 'de, 'a, 'a, R, V>
    where
        'de: 'a,
        'r: 'a,
        V: Visitor<'de> + 'a,
        V::Value: 'a,
        Self: 'a,
        Self::Error: 'a;

    fn decode_unit<'a, V>(self, visitor: V) -> Self::DecodeUnitAsync<'a, V>
    where
        'de: 'a,
        V: Visitor<'de> + 'a,
        V::Value: 'a,
        Self: 'a,
        Self::Error: 'a,
    {
        DecodeValueOpAsync::<'r, 'de, 'a, 'a, R, V>::new(self, ValueMode::Unit, visitor)
    }

    type DecodeUnitStructAsync<'a, V>
        = DecodeValueOpAsync<'r, 'de, 'a, 'a, R, V>
    where
        'de: 'a,
        'r: 'a,
        V: Visitor<'de> + 'a,
        V::Value: 'a,
        Self: 'a,
        Self::Error: 'a;

    fn decode_unit_struct<'a, V>(
        self,
        _name: &'static str,
        visitor: V,
    ) -> Self::DecodeUnitStructAsync<'a, V>
    where
        'de: 'a,
        V: Visitor<'de> + 'a,
        V::Value: 'a,
        Self: 'a,
        Self::Error: 'a,
    {
        DecodeValueOpAsync::<'r, 'de, 'a, 'a, R, V>::new(self, ValueMode::UnitStruct, visitor)
    }

    type DecodeNewtypeStructAsync<'a, V>
        = DecodeValueOpAsync<'r, 'de, 'a, 'a, R, V>
    where
        'de: 'a,
        'r: 'a,
        V: Visitor<'de> + 'a,
        V::Value: 'a,
        Self: 'a,
        Self::Error: 'a;

    fn decode_newtype_struct<'a, V>(
        self,
        _name: &'static str,
        visitor: V,
    ) -> Self::DecodeNewtypeStructAsync<'a, V>
    where
        'de: 'a,
        V: Visitor<'de> + 'a,
        V::Value: 'a,
        Self: 'a,
        Self::Error: 'a,
    {
        DecodeValueOpAsync::<'r, 'de, 'a, 'a, R, V>::new(self, ValueMode::NewtypeStruct, visitor)
    }

    type DecodeSeqAsync<'a, V>
        = DecodeValueOpAsync<'r, 'de, 'a, 'a, R, V>
    where
        'de: 'a,
        'r: 'a,
        V: Visitor<'de> + 'a,
        V::Value: 'a,
        Self: 'a,
        Self::Error: 'a;

    fn decode_seq<'a, V>(self, visitor: V) -> Self::DecodeSeqAsync<'a, V>
    where
        'de: 'a,
        V: Visitor<'de> + 'a,
        V::Value: 'a,
        Self: 'a,
        Self::Error: 'a,
    {
        DecodeValueOpAsync::<'r, 'de, 'a, 'a, R, V>::new(self, ValueMode::Seq, visitor)
    }

    type DecodeTupleAsync<'a, V>
        = DecodeValueOpAsync<'r, 'de, 'a, 'a, R, V>
    where
        'de: 'a,
        'r: 'a,
        V: Visitor<'de> + 'a,
        V::Value: 'a,
        Self: 'a,
        Self::Error: 'a;

    fn decode_tuple<'a, V>(self, _len: usize, visitor: V) -> Self::DecodeTupleAsync<'a, V>
    where
        'de: 'a,
        V: Visitor<'de> + 'a,
        V::Value: 'a,
        Self: 'a,
        Self::Error: 'a,
    {
        DecodeValueOpAsync::<'r, 'de, 'a, 'a, R, V>::new(self, ValueMode::Tuple, visitor)
    }

    type DecodeTupleStructAsync<'a, V>
        = DecodeValueOpAsync<'r, 'de, 'a, 'a, R, V>
    where
        'de: 'a,
        'r: 'a,
        V: Visitor<'de> + 'a,
        V::Value: 'a,
        Self: 'a,
        Self::Error: 'a;

    fn decode_tuple_struct<'a, V>(
        self,
        _name: &'static str,
        _len: usize,
        visitor: V,
    ) -> Self::DecodeTupleStructAsync<'a, V>
    where
        'de: 'a,
        V: Visitor<'de> + 'a,
        V::Value: 'a,
        Self: 'a,
        Self::Error: 'a,
    {
        DecodeValueOpAsync::<'r, 'de, 'a, 'a, R, V>::new(self, ValueMode::TupleStruct, visitor)
    }

    type DecodeMapAsync<'a, V>
        = DecodeValueOpAsync<'r, 'de, 'a, 'a, R, V>
    where
        'de: 'a,
        'r: 'a,
        V: Visitor<'de> + 'a,
        V::Value: 'a,
        Self: 'a,
        Self::Error: 'a;

    fn decode_map<'a, V>(self, visitor: V) -> Self::DecodeMapAsync<'a, V>
    where
        'de: 'a,
        V: Visitor<'de> + 'a,
        V::Value: 'a,
        Self: 'a,
        Self::Error: 'a,
    {
        DecodeValueOpAsync::<'r, 'de, 'a, 'a, R, V>::new(self, ValueMode::Map, visitor)
    }

    type DecodeStructAsync<'a, V>
        = DecodeValueOpAsync<'r, 'de, 'a, 'a, R, V>
    where
        'de: 'a,
        'r: 'a,
        V: Visitor<'de> + 'a,
        V::Value: 'a,
        Self: 'a,
        Self::Error: 'a;

    fn decode_struct<'a, V>(
        self,
        _name: &'static str,
        _fields: &'static [&'static str],
        visitor: V,
    ) -> Self::DecodeStructAsync<'a, V>
    where
        'de: 'a,
        V: Visitor<'de> + 'a,
        V::Value: 'a,
        Self: 'a,
        Self::Error: 'a,
    {
        DecodeValueOpAsync::<'r, 'de, 'a, 'a, R, V>::new(self, ValueMode::Struct, visitor)
    }

    type DecodeEnumAsync<'a, V>
        = DecodeValueOpAsync<'r, 'de, 'a, 'a, R, V>
    where
        'de: 'a,
        'r: 'a,
        V: Visitor<'de> + 'a,
        V::Value: 'a,
        Self: 'a,
        Self::Error: 'a;

    fn decode_enum<'a, V>(
        self,
        _name: &'static str,
        _variants: &'static [&'static str],
        visitor: V,
    ) -> Self::DecodeEnumAsync<'a, V>
    where
        'de: 'a,
        V: Visitor<'de> + 'a,
        V::Value: 'a,
        Self: 'a,
        Self::Error: 'a,
    {
        DecodeValueOpAsync::<'r, 'de, 'a, 'a, R, V>::new(self, ValueMode::Enum, visitor)
    }

    type DecodeIdentifierAsync<'a, V>
        = DecodeValueOpAsync<'r, 'de, 'a, 'a, R, V>
    where
        'de: 'a,
        'r: 'a,
        V: Visitor<'de> + 'a,
        V::Value: 'a,
        Self: 'a,
        Self::Error: 'a;

    fn decode_identifier<'a, V>(self, visitor: V) -> Self::DecodeIdentifierAsync<'a, V>
    where
        'de: 'a,
        V: Visitor<'de> + 'a,
        V::Value: 'a,
        Self: 'a,
        Self::Error: 'a,
    {
        DecodeValueOpAsync::<'r, 'de, 'a, 'a, R, V>::new(self, ValueMode::Identifier, visitor)
    }

    type DecodeIgnoredAnyAsync<'a, V>
        = DecodeValueOpAsync<'r, 'de, 'a, 'a, R, V>
    where
        'de: 'a,
        'r: 'a,
        V: Visitor<'de> + 'a,
        V::Value: 'a,
        Self: 'a,
        Self::Error: 'a;

    fn decode_ignored_any<'a, V>(self, visitor: V) -> Self::DecodeIgnoredAnyAsync<'a, V>
    where
        'de: 'a,
        V: Visitor<'de> + 'a,
        V::Value: 'a,
        Self: 'a,
        Self::Error: 'a,
    {
        DecodeValueOpAsync::<'r, 'de, 'a, 'a, R, V>::new(self, ValueMode::IgnoredAny, visitor)
    }
}

// ---------------------------------------------------------------------------
// 对象键的解码器
// ---------------------------------------------------------------------------

/// 已从 JSON 对象读出、但仍可按键类型解释的字符串。
struct JsonNameDecoder {
    name_: String,
}

/// 对象键按目标类型解释的模式。
#[derive(Clone, Copy)]
enum NameMode {
    Any,
    Bool,
    I128,
    U128,
    F64,
    Char,
    Str,
    String,
    Enum,
    Some,
    Newtype,
}

fn parse_bool_name_(name: &str) -> Result<bool, JsonError> {
    match name {
        "true" => Ok(true),
        "false" => Ok(false),
        _ => Err(JsonError::new("JSON object key is not a boolean")),
    }
}

fn parse_i128_name_(name: &str) -> Result<i128, JsonError> {
    name.parse()
        .map_err(|_| JsonError::new("JSON object key is not an integer"))
}

fn parse_u128_name_(name: &str) -> Result<u128, JsonError> {
    name.parse()
        .map_err(|_| JsonError::new("JSON object key is not an unsigned integer"))
}

fn parse_f64_name_(name: &str) -> Result<f64, JsonError> {
    let value: f64 = name
        .parse()
        .map_err(|_| JsonError::new("JSON object key is not a number"))?;
    if !value.is_finite() {
        return Err(JsonError::new("JSON object key is not a finite number"));
    }
    Ok(value)
}

#[gen_may_cancel_future(DecodeNameOp, pub)]
async fn decode_name_async_<'de, 'a, V, C>(
    name: String,
    mode: NameMode,
    visitor: V,
    cancel: C,
) -> Result<V::Value, JsonError>
where
    'de: 'a,
    V: Visitor<'de> + 'a,
    V::Value: 'a,
    C: TrCancellationToken,
{
    match mode {
        NameMode::Any => visitor.visit_string(name).may_cancel_with(cancel).await,
        NameMode::Bool => {
            let value = parse_bool_name_(&name)?;
            visitor.visit_bool(value).may_cancel_with(cancel).await
        }
        NameMode::I128 => {
            let value = parse_i128_name_(&name)?;
            if value >= i64::MIN as i128 && value <= i64::MAX as i128 {
                visitor
                    .visit_i64(value as i64)
                    .may_cancel_with(cancel)
                    .await
            } else {
                visitor.visit_i128(value).may_cancel_with(cancel).await
            }
        }
        NameMode::U128 => {
            let value = parse_u128_name_(&name)?;
            if value <= u64::MAX as u128 {
                visitor
                    .visit_u64(value as u64)
                    .may_cancel_with(cancel)
                    .await
            } else {
                visitor.visit_u128(value).may_cancel_with(cancel).await
            }
        }
        NameMode::F64 => {
            let value = parse_f64_name_(&name)?;
            visitor.visit_f64(value).may_cancel_with(cancel).await
        }
        NameMode::Char => {
            let mut chars = name.chars();
            let value = match (chars.next(), chars.next()) {
                (Some(ch), None) => ch,
                _ => return Err(JsonError::new("JSON object key is not a single character")),
            };
            visitor.visit_char(value).may_cancel_with(cancel).await
        }
        NameMode::Str => visitor.visit_str(&name).may_cancel_with(cancel).await,
        NameMode::String => visitor.visit_string(name).may_cancel_with(cancel).await,
        NameMode::Enum => {
            let access = JsonNameEnumAccess { name_: name };
            visitor.visit_enum(access).may_cancel_with(cancel).await
        }
        NameMode::Some => {
            visitor
                .visit_some(JsonNameDecoder { name_: name })
                .may_cancel_with(cancel)
                .await
        }
        NameMode::Newtype => {
            visitor
                .visit_newtype_struct(JsonNameDecoder { name_: name })
                .may_cancel_with(cancel)
                .await
        }
    }
}

impl<'de> TrDecoder<'de> for JsonNameDecoder {
    type Error = JsonError;

    type DecodeAnyAsync<'a, V>
        = DecodeNameOpAsync<'de, 'a, 'a, V>
    where
        'de: 'a,
        V: Visitor<'de> + 'a,
        V::Value: 'a,
        Self: 'a,
        Self::Error: 'a;

    fn decode_any<'a, V>(self, visitor: V) -> Self::DecodeAnyAsync<'a, V>
    where
        'de: 'a,
        V: Visitor<'de> + 'a,
        V::Value: 'a,
        Self: 'a,
        Self::Error: 'a,
    {
        DecodeNameOpAsync::<'de, 'a, 'a, V>::new(self.name_, NameMode::Any, visitor)
    }

    type DecodeBoolAsync<'a, V>
        = DecodeNameOpAsync<'de, 'a, 'a, V>
    where
        'de: 'a,
        V: Visitor<'de> + 'a,
        V::Value: 'a,
        Self: 'a,
        Self::Error: 'a;

    fn decode_bool<'a, V>(self, visitor: V) -> Self::DecodeBoolAsync<'a, V>
    where
        'de: 'a,
        V: Visitor<'de> + 'a,
        V::Value: 'a,
        Self: 'a,
        Self::Error: 'a,
    {
        DecodeNameOpAsync::<'de, 'a, 'a, V>::new(self.name_, NameMode::Bool, visitor)
    }

    type DecodeI8Async<'a, V>
        = DecodeNameOpAsync<'de, 'a, 'a, V>
    where
        'de: 'a,
        V: Visitor<'de> + 'a,
        V::Value: 'a,
        Self: 'a,
        Self::Error: 'a;

    fn decode_i8<'a, V>(self, visitor: V) -> Self::DecodeI8Async<'a, V>
    where
        'de: 'a,
        V: Visitor<'de> + 'a,
        V::Value: 'a,
        Self: 'a,
        Self::Error: 'a,
    {
        DecodeNameOpAsync::<'de, 'a, 'a, V>::new(self.name_, NameMode::I128, visitor)
    }

    type DecodeI16Async<'a, V>
        = DecodeNameOpAsync<'de, 'a, 'a, V>
    where
        'de: 'a,
        V: Visitor<'de> + 'a,
        V::Value: 'a,
        Self: 'a,
        Self::Error: 'a;

    fn decode_i16<'a, V>(self, visitor: V) -> Self::DecodeI16Async<'a, V>
    where
        'de: 'a,
        V: Visitor<'de> + 'a,
        V::Value: 'a,
        Self: 'a,
        Self::Error: 'a,
    {
        DecodeNameOpAsync::<'de, 'a, 'a, V>::new(self.name_, NameMode::I128, visitor)
    }

    type DecodeI32Async<'a, V>
        = DecodeNameOpAsync<'de, 'a, 'a, V>
    where
        'de: 'a,
        V: Visitor<'de> + 'a,
        V::Value: 'a,
        Self: 'a,
        Self::Error: 'a;

    fn decode_i32<'a, V>(self, visitor: V) -> Self::DecodeI32Async<'a, V>
    where
        'de: 'a,
        V: Visitor<'de> + 'a,
        V::Value: 'a,
        Self: 'a,
        Self::Error: 'a,
    {
        DecodeNameOpAsync::<'de, 'a, 'a, V>::new(self.name_, NameMode::I128, visitor)
    }

    type DecodeI64Async<'a, V>
        = DecodeNameOpAsync<'de, 'a, 'a, V>
    where
        'de: 'a,
        V: Visitor<'de> + 'a,
        V::Value: 'a,
        Self: 'a,
        Self::Error: 'a;

    fn decode_i64<'a, V>(self, visitor: V) -> Self::DecodeI64Async<'a, V>
    where
        'de: 'a,
        V: Visitor<'de> + 'a,
        V::Value: 'a,
        Self: 'a,
        Self::Error: 'a,
    {
        DecodeNameOpAsync::<'de, 'a, 'a, V>::new(self.name_, NameMode::I128, visitor)
    }

    type DecodeI128Async<'a, V>
        = DecodeNameOpAsync<'de, 'a, 'a, V>
    where
        'de: 'a,
        V: Visitor<'de> + 'a,
        V::Value: 'a,
        Self: 'a,
        Self::Error: 'a;

    fn decode_i128<'a, V>(self, visitor: V) -> Self::DecodeI128Async<'a, V>
    where
        'de: 'a,
        V: Visitor<'de> + 'a,
        V::Value: 'a,
        Self: 'a,
        Self::Error: 'a,
    {
        DecodeNameOpAsync::<'de, 'a, 'a, V>::new(self.name_, NameMode::I128, visitor)
    }

    type DecodeU8Async<'a, V>
        = DecodeNameOpAsync<'de, 'a, 'a, V>
    where
        'de: 'a,
        V: Visitor<'de> + 'a,
        V::Value: 'a,
        Self: 'a,
        Self::Error: 'a;

    fn decode_u8<'a, V>(self, visitor: V) -> Self::DecodeU8Async<'a, V>
    where
        'de: 'a,
        V: Visitor<'de> + 'a,
        V::Value: 'a,
        Self: 'a,
        Self::Error: 'a,
    {
        DecodeNameOpAsync::<'de, 'a, 'a, V>::new(self.name_, NameMode::U128, visitor)
    }

    type DecodeU16Async<'a, V>
        = DecodeNameOpAsync<'de, 'a, 'a, V>
    where
        'de: 'a,
        V: Visitor<'de> + 'a,
        V::Value: 'a,
        Self: 'a,
        Self::Error: 'a;

    fn decode_u16<'a, V>(self, visitor: V) -> Self::DecodeU16Async<'a, V>
    where
        'de: 'a,
        V: Visitor<'de> + 'a,
        V::Value: 'a,
        Self: 'a,
        Self::Error: 'a,
    {
        DecodeNameOpAsync::<'de, 'a, 'a, V>::new(self.name_, NameMode::U128, visitor)
    }

    type DecodeU32Async<'a, V>
        = DecodeNameOpAsync<'de, 'a, 'a, V>
    where
        'de: 'a,
        V: Visitor<'de> + 'a,
        V::Value: 'a,
        Self: 'a,
        Self::Error: 'a;

    fn decode_u32<'a, V>(self, visitor: V) -> Self::DecodeU32Async<'a, V>
    where
        'de: 'a,
        V: Visitor<'de> + 'a,
        V::Value: 'a,
        Self: 'a,
        Self::Error: 'a,
    {
        DecodeNameOpAsync::<'de, 'a, 'a, V>::new(self.name_, NameMode::U128, visitor)
    }

    type DecodeU64Async<'a, V>
        = DecodeNameOpAsync<'de, 'a, 'a, V>
    where
        'de: 'a,
        V: Visitor<'de> + 'a,
        V::Value: 'a,
        Self: 'a,
        Self::Error: 'a;

    fn decode_u64<'a, V>(self, visitor: V) -> Self::DecodeU64Async<'a, V>
    where
        'de: 'a,
        V: Visitor<'de> + 'a,
        V::Value: 'a,
        Self: 'a,
        Self::Error: 'a,
    {
        DecodeNameOpAsync::<'de, 'a, 'a, V>::new(self.name_, NameMode::U128, visitor)
    }

    type DecodeU128Async<'a, V>
        = DecodeNameOpAsync<'de, 'a, 'a, V>
    where
        'de: 'a,
        V: Visitor<'de> + 'a,
        V::Value: 'a,
        Self: 'a,
        Self::Error: 'a;

    fn decode_u128<'a, V>(self, visitor: V) -> Self::DecodeU128Async<'a, V>
    where
        'de: 'a,
        V: Visitor<'de> + 'a,
        V::Value: 'a,
        Self: 'a,
        Self::Error: 'a,
    {
        DecodeNameOpAsync::<'de, 'a, 'a, V>::new(self.name_, NameMode::U128, visitor)
    }

    type DecodeF32Async<'a, V>
        = DecodeNameOpAsync<'de, 'a, 'a, V>
    where
        'de: 'a,
        V: Visitor<'de> + 'a,
        V::Value: 'a,
        Self: 'a,
        Self::Error: 'a;

    fn decode_f32<'a, V>(self, visitor: V) -> Self::DecodeF32Async<'a, V>
    where
        'de: 'a,
        V: Visitor<'de> + 'a,
        V::Value: 'a,
        Self: 'a,
        Self::Error: 'a,
    {
        DecodeNameOpAsync::<'de, 'a, 'a, V>::new(self.name_, NameMode::F64, visitor)
    }

    type DecodeF64Async<'a, V>
        = DecodeNameOpAsync<'de, 'a, 'a, V>
    where
        'de: 'a,
        V: Visitor<'de> + 'a,
        V::Value: 'a,
        Self: 'a,
        Self::Error: 'a;

    fn decode_f64<'a, V>(self, visitor: V) -> Self::DecodeF64Async<'a, V>
    where
        'de: 'a,
        V: Visitor<'de> + 'a,
        V::Value: 'a,
        Self: 'a,
        Self::Error: 'a,
    {
        DecodeNameOpAsync::<'de, 'a, 'a, V>::new(self.name_, NameMode::F64, visitor)
    }

    type DecodeCharAsync<'a, V>
        = DecodeNameOpAsync<'de, 'a, 'a, V>
    where
        'de: 'a,
        V: Visitor<'de> + 'a,
        V::Value: 'a,
        Self: 'a,
        Self::Error: 'a;

    fn decode_char<'a, V>(self, visitor: V) -> Self::DecodeCharAsync<'a, V>
    where
        'de: 'a,
        V: Visitor<'de> + 'a,
        V::Value: 'a,
        Self: 'a,
        Self::Error: 'a,
    {
        DecodeNameOpAsync::<'de, 'a, 'a, V>::new(self.name_, NameMode::Char, visitor)
    }

    type DecodeStrAsync<'a, V>
        = DecodeNameOpAsync<'de, 'a, 'a, V>
    where
        'de: 'a,
        V: Visitor<'de> + 'a,
        V::Value: 'a,
        Self: 'a,
        Self::Error: 'a;

    fn decode_str<'a, V>(self, visitor: V) -> Self::DecodeStrAsync<'a, V>
    where
        'de: 'a,
        V: Visitor<'de> + 'a,
        V::Value: 'a,
        Self: 'a,
        Self::Error: 'a,
    {
        DecodeNameOpAsync::<'de, 'a, 'a, V>::new(self.name_, NameMode::Str, visitor)
    }

    type DecodeStringAsync<'a, V>
        = DecodeNameOpAsync<'de, 'a, 'a, V>
    where
        'de: 'a,
        V: Visitor<'de> + 'a,
        V::Value: 'a,
        Self: 'a,
        Self::Error: 'a;

    fn decode_string<'a, V>(self, visitor: V) -> Self::DecodeStringAsync<'a, V>
    where
        'de: 'a,
        V: Visitor<'de> + 'a,
        V::Value: 'a,
        Self: 'a,
        Self::Error: 'a,
    {
        DecodeNameOpAsync::<'de, 'a, 'a, V>::new(self.name_, NameMode::String, visitor)
    }

    type DecodeBytesAsync<'a, V>
        = DecodeNameOpAsync<'de, 'a, 'a, V>
    where
        'de: 'a,
        V: Visitor<'de> + 'a,
        V::Value: 'a,
        Self: 'a,
        Self::Error: 'a;

    fn decode_bytes<'a, V>(self, visitor: V) -> Self::DecodeBytesAsync<'a, V>
    where
        'de: 'a,
        V: Visitor<'de> + 'a,
        V::Value: 'a,
        Self: 'a,
        Self::Error: 'a,
    {
        DecodeNameOpAsync::<'de, 'a, 'a, V>::new(self.name_, NameMode::Any, visitor)
    }

    type DecodeByteBufAsync<'a, V>
        = DecodeNameOpAsync<'de, 'a, 'a, V>
    where
        'de: 'a,
        V: Visitor<'de> + 'a,
        V::Value: 'a,
        Self: 'a,
        Self::Error: 'a;

    fn decode_byte_buf<'a, V>(self, visitor: V) -> Self::DecodeByteBufAsync<'a, V>
    where
        'de: 'a,
        V: Visitor<'de> + 'a,
        V::Value: 'a,
        Self: 'a,
        Self::Error: 'a,
    {
        DecodeNameOpAsync::<'de, 'a, 'a, V>::new(self.name_, NameMode::Any, visitor)
    }

    type DecodeOptionAsync<'a, V>
        = DecodeNameOpAsync<'de, 'a, 'a, V>
    where
        'de: 'a,
        V: Visitor<'de> + 'a,
        V::Value: 'a,
        Self: 'a,
        Self::Error: 'a;

    fn decode_option<'a, V>(self, visitor: V) -> Self::DecodeOptionAsync<'a, V>
    where
        'de: 'a,
        V: Visitor<'de> + 'a,
        V::Value: 'a,
        Self: 'a,
        Self::Error: 'a,
    {
        DecodeNameOpAsync::<'de, 'a, 'a, V>::new(self.name_, NameMode::Some, visitor)
    }

    type DecodeUnitAsync<'a, V>
        = DecodeNameOpAsync<'de, 'a, 'a, V>
    where
        'de: 'a,
        V: Visitor<'de> + 'a,
        V::Value: 'a,
        Self: 'a,
        Self::Error: 'a;

    fn decode_unit<'a, V>(self, visitor: V) -> Self::DecodeUnitAsync<'a, V>
    where
        'de: 'a,
        V: Visitor<'de> + 'a,
        V::Value: 'a,
        Self: 'a,
        Self::Error: 'a,
    {
        DecodeNameOpAsync::<'de, 'a, 'a, V>::new(self.name_, NameMode::Any, visitor)
    }

    type DecodeUnitStructAsync<'a, V>
        = DecodeNameOpAsync<'de, 'a, 'a, V>
    where
        'de: 'a,
        V: Visitor<'de> + 'a,
        V::Value: 'a,
        Self: 'a,
        Self::Error: 'a;

    fn decode_unit_struct<'a, V>(
        self,
        _name: &'static str,
        visitor: V,
    ) -> Self::DecodeUnitStructAsync<'a, V>
    where
        'de: 'a,
        V: Visitor<'de> + 'a,
        V::Value: 'a,
        Self: 'a,
        Self::Error: 'a,
    {
        DecodeNameOpAsync::<'de, 'a, 'a, V>::new(self.name_, NameMode::Any, visitor)
    }

    type DecodeNewtypeStructAsync<'a, V>
        = DecodeNameOpAsync<'de, 'a, 'a, V>
    where
        'de: 'a,
        V: Visitor<'de> + 'a,
        V::Value: 'a,
        Self: 'a,
        Self::Error: 'a;

    fn decode_newtype_struct<'a, V>(
        self,
        _name: &'static str,
        visitor: V,
    ) -> Self::DecodeNewtypeStructAsync<'a, V>
    where
        'de: 'a,
        V: Visitor<'de> + 'a,
        V::Value: 'a,
        Self: 'a,
        Self::Error: 'a,
    {
        DecodeNameOpAsync::<'de, 'a, 'a, V>::new(self.name_, NameMode::Newtype, visitor)
    }

    forward_to_decode_any! {
        decode_seq => DecodeSeqAsync,
        decode_tuple => DecodeTupleAsync(len: usize),
        decode_tuple_struct => DecodeTupleStructAsync(name: &'static str, len: usize),
        decode_map => DecodeMapAsync,
        decode_struct => DecodeStructAsync(name: &'static str, fields: &'static [&'static str]),
        decode_identifier => DecodeIdentifierAsync,
        decode_ignored_any => DecodeIgnoredAnyAsync,
    }

    type DecodeEnumAsync<'a, V>
        = DecodeNameOpAsync<'de, 'a, 'a, V>
    where
        'de: 'a,
        V: Visitor<'de> + 'a,
        V::Value: 'a,
        Self: 'a,
        Self::Error: 'a;

    fn decode_enum<'a, V>(
        self,
        _name: &'static str,
        _variants: &'static [&'static str],
        visitor: V,
    ) -> Self::DecodeEnumAsync<'a, V>
    where
        'de: 'a,
        V: Visitor<'de> + 'a,
        V::Value: 'a,
        Self: 'a,
        Self::Error: 'a,
    {
        DecodeNameOpAsync::<'de, 'a, 'a, V>::new(self.name_, NameMode::Enum, visitor)
    }
}

/// 把对象键当作枚举变体名时的访问器。
struct JsonNameEnumAccess {
    name_: String,
}

impl<'de> EnumAccess<'de> for JsonNameEnumAccess {
    type Error = JsonError;
    type Variant = JsonNameVariantAccess;

    fn variant_seed<'a, V>(
        self,
        seed: V,
    ) -> impl TrMayCancel<'a, MayCancelOutput = Result<(V::Value, Self::Variant), Self::Error>>
    where
        'de: 'a,
        V: TrDecodableSeed<'de> + 'a,
        V::Value: 'a,
        Self: 'a,
        Self::Variant: 'a,
        Self::Error: 'a,
    {
        NameVariantSeedOpAsync::<'de, 'a, 'a, V>::new(self.name_, seed)
    }
}

#[gen_may_cancel_future(NameVariantSeedOp, pub)]
async fn name_variant_seed_async_<'de, 'a, V, C>(
    name: String,
    seed: V,
    cancel: C,
) -> Result<(V::Value, JsonNameVariantAccess), JsonError>
where
    'de: 'a,
    V: TrDecodableSeed<'de> + 'a,
    V::Value: 'a,
    C: TrCancellationToken,
{
    let value = seed
        .decode_async(JsonNameDecoder { name_: name })
        .may_cancel_with(cancel)
        .await?;
    Ok((value, JsonNameVariantAccess))
}

/// 对象键枚举只可能是单元变体。
pub struct JsonNameVariantAccess;

impl<'de> VariantAccess<'de> for JsonNameVariantAccess {
    type Error = JsonError;

    fn unit_variant<'a>(self) -> impl TrMayCancel<'a, MayCancelOutput = Result<(), Self::Error>>
    where
        'de: 'a,
        Self: 'a,
        Self::Error: 'a,
    {
        CancellableReady::new_(Ok(()))
    }

    fn newtype_variant_seed<'a, T>(
        self,
        _seed: T,
    ) -> impl TrMayCancel<'a, MayCancelOutput = Result<T::Value, Self::Error>>
    where
        'de: 'a,
        T: TrDecodableSeed<'de> + 'a,
        T::Value: 'a,
        Self: 'a,
        Self::Error: 'a,
    {
        NameVariantErrorOpAsync::<'de, 'a, 'a, T>::new(_seed)
    }

    fn tuple_variant<'a, V>(
        self,
        _len: usize,
        _visitor: V,
    ) -> impl TrMayCancel<'a, MayCancelOutput = Result<V::Value, Self::Error>>
    where
        'de: 'a,
        V: Visitor<'de> + 'a,
        V::Value: 'a,
        Self: 'a,
        Self::Error: 'a,
    {
        NameVariantVisitorErrorOpAsync::<'de, 'a, 'a, V>::new(_visitor)
    }

    fn struct_variant<'a, V>(
        self,
        _fields: &'static [&'static str],
        _visitor: V,
    ) -> impl TrMayCancel<'a, MayCancelOutput = Result<V::Value, Self::Error>>
    where
        'de: 'a,
        V: Visitor<'de> + 'a,
        V::Value: 'a,
        Self: 'a,
        Self::Error: 'a,
    {
        NameVariantVisitorErrorOpAsync::<'de, 'a, 'a, V>::new(_visitor)
    }
}

#[gen_may_cancel_future(NameVariantErrorOp, pub)]
async fn name_variant_error_async_<'de, 'a, T, C>(
    _seed: T,
    cancel: C,
) -> Result<T::Value, JsonError>
where
    'de: 'a,
    T: TrDecodableSeed<'de> + 'a,
    T::Value: 'a,
    C: TrCancellationToken,
{
    let _ = cancel;
    Err(JsonError::new(
        "JSON object key can only encode a unit enum variant",
    ))
}

#[gen_may_cancel_future(NameVariantVisitorErrorOp, pub)]
async fn name_variant_visitor_error_async_<'de, 'a, V, C>(
    _visitor: V,
    cancel: C,
) -> Result<V::Value, JsonError>
where
    'de: 'a,
    V: Visitor<'de> + 'a,
    V::Value: 'a,
    C: TrCancellationToken,
{
    let _ = cancel;
    Err(JsonError::new(
        "JSON object key can only encode a unit enum variant",
    ))
}
