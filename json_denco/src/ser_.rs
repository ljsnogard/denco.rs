//! 流式 JSON 序列化器。
//!
//! 编码器直接面向 [`abs_buff::TrBuffWrite`] 工作：所有定长片段（标点、数字、
//! 转义后的字符串分片）都通过 `write_async` 借出写段后写入，不把整段输出
//! 预攒进 `Vec<u8>`。
//!
//! 格式约定与 `serde_json` 的外部标签表示一致：
//!
//! - 单元值 / `None` 编码为 `null`；
//! - 单元结构体编码为 `null`，newtype 结构体直接编码内部值；
//! - 字节串编码为数字数组；
//! - 单元变体编码为字符串，newtype 变体编码为 `{"Variant": value}`，
//!   tuple 变体编码为 `{"Variant": [...]}`，struct 变体编码为
//!   `{"Variant": {...}}`；
//! - 映射键按 JSON 要求写成字符串；字符串、布尔与整数键都受支持，
//!   浮点与复合值不能作为键。

use core::{
    fmt::{self, Write as FmtWrite},
    future::Future,
    pin::Pin,
    task::{Context, Poll},
};

use abs_buff::{Demand, TrBuffWrite, buffer::TrBuffSegmMut};
use abs_cancel::{TrCancellationToken, TrMayCancel};
use abs_denco::ser::{
    TrEncodable, TrEncodeMap, TrEncodeSeq, TrEncodeStruct, TrEncodeStructVariant, TrEncodeTuple,
    TrEncodeTupleStruct, TrEncodeTupleVariant, TrEncoder,
};
use gen_mcf2::gen_may_cancel_future;

use crate::JsonError;

/// 栈上文本缓冲的最大长度。
///
/// 最长的是 `i128::MIN` 的十进制（40 字符）与浮点数的 `Display` 结果；
/// 64 字节足够覆盖常见输出，超出时返回错误而不是截断。
const TEXT_CAP_: usize = 64;

/// 一个立即完成、但会检查预取消状态的 future。
///
/// JSON 的单个片段写入在借到写段后是同步的；这个包装器负责在调用
/// `may_cancel_with` 时检查令牌是否已经取消。若已取消，它会用错误替换
/// 原本的结果。
pub(crate) struct CancellableReady<R> {
    result_: Option<Result<R, JsonError>>,
}

impl<R> CancellableReady<R> {
    pub(crate) fn new_(result: Result<R, JsonError>) -> Self {
        Self {
            result_: Some(result),
        }
    }
}

impl<R> Future for CancellableReady<R>
where
    R: Unpin,
{
    type Output = Result<R, JsonError>;

    fn poll(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<Self::Output> {
        Poll::Ready(
            self.get_mut()
                .result_
                .take()
                .expect("CancellableReady polled after completion"),
        )
    }
}

impl<'a, R> TrMayCancel<'a> for CancellableReady<R>
where
    R: Unpin + 'a,
{
    type MayCancelFuture<'f, C>
        = Self
    where
        'f: 'a,
        Self: 'f,
        C: 'f + TrCancellationToken;

    type MayCancelOutput = Result<R, JsonError>;

    fn may_cancel_with<C>(mut self, cancel: C) -> Self::MayCancelFuture<'a, C>
    where
        C: 'a + TrCancellationToken,
    {
        if cancel.is_cancelled() {
            self.result_ = Some(Err(JsonError::new("operation cancelled")));
        }
        self
    }
}

/// 栈上的短文本。
#[derive(Clone, Copy)]
struct TextBuf {
    data_: [u8; TEXT_CAP_],
    len_: usize,
}

impl TextBuf {
    const fn new_() -> Self {
        Self {
            data_: [0u8; TEXT_CAP_],
            len_: 0,
        }
    }

    fn as_slice_(&self) -> &[u8] {
        &self.data_[..self.len_]
    }
}

impl FmtWrite for TextBuf {
    fn write_str(&mut self, s: &str) -> fmt::Result {
        let end = self.len_ + s.len();
        if end > TEXT_CAP_ {
            return Err(fmt::Error);
        }
        self.data_[self.len_..end].copy_from_slice(s.as_bytes());
        self.len_ = end;
        Ok(())
    }
}

fn text_of_<T>(value: T) -> Result<TextBuf, JsonError>
where
    T: fmt::Display,
{
    let mut out = TextBuf::new_();
    core::write!(&mut out, "{value}").map_err(|_| JsonError::new("number too long for JSON"))?;
    Ok(out)
}

fn f64_text_(value: f64) -> Result<TextBuf, JsonError> {
    if !value.is_finite() {
        return Err(JsonError::new("JSON cannot represent NaN or infinity"));
    }
    let mut out = text_of_(value)?;
    if !out
        .as_slice_()
        .iter()
        .any(|byte| matches!(byte, b'.' | b'e' | b'E'))
    {
        core::write!(&mut out, ".0").map_err(|_| JsonError::new("number too long for JSON"))?;
    }
    Ok(out)
}

fn f32_text_(value: f32) -> Result<TextBuf, JsonError> {
    if !value.is_finite() {
        return Err(JsonError::new("JSON cannot represent NaN or infinity"));
    }
    let mut out = text_of_(value)?;
    if !out
        .as_slice_()
        .iter()
        .any(|byte| matches!(byte, b'.' | b'e' | b'E'))
    {
        core::write!(&mut out, ".0").map_err(|_| JsonError::new("number too long for JSON"))?;
    }
    Ok(out)
}

/// 把 `bytes` 全部写入写端，按写段实际可写量循环推进。
async fn write_all_async_<W, C>(
    writer: &mut W,
    mut bytes: &[u8],
    cancel: C,
) -> Result<(), JsonError>
where
    W: TrBuffWrite<u8>,
    C: TrCancellationToken,
{
    if cancel.is_cancelled() {
        return Err(JsonError::new("operation cancelled"));
    }
    while !bytes.is_empty() {
        if cancel.is_cancelled() {
            return Err(JsonError::new("operation cancelled"));
        }
        let demand = Demand::no_more_than(bytes.len());
        let mut some = writer
            .write_async(&demand)
            .may_cancel_with(cancel.child_token())
            .await;
        let moved = match some.as_mut().pick_left() {
            Some(segm) => segm.move_items_from_as_buff(bytes),
            None => 0,
        };
        if moved == 0 {
            let err = some
                .pick_right()
                .map(JsonError::buff_write)
                .unwrap_or_else(|| JsonError::new("buff write returned no segment"));
            return Err(err);
        }
        bytes = &bytes[moved..];
    }
    Ok(())
}

fn control_escape_(out: &mut [u8; 6], byte: u8) -> usize {
    const HEX_: &[u8; 16] = b"0123456789abcdef";
    out[0] = b'\\';
    out[1] = b'u';
    out[2] = b'0';
    out[3] = b'0';
    out[4] = HEX_[(byte >> 4) as usize];
    out[5] = HEX_[(byte & 0x0f) as usize];
    6
}

/// 写一个带引号、按 JSON 规则转义的字符串。
async fn write_quoted_async_<W, C>(writer: &mut W, value: &str, cancel: C) -> Result<(), JsonError>
where
    W: TrBuffWrite<u8>,
    C: TrCancellationToken,
{
    write_all_async_(writer, b"\"", cancel.child_token()).await?;
    let bytes = value.as_bytes();
    let mut start = 0usize;
    for (index, &byte) in bytes.iter().enumerate() {
        let mut escape_buf = [0u8; 6];
        let escape: Option<&[u8]> = match byte {
            b'"' => Some(b"\\\""),
            b'\\' => Some(b"\\\\"),
            0x08 => Some(b"\\b"),
            0x0c => Some(b"\\f"),
            b'\n' => Some(b"\\n"),
            b'\r' => Some(b"\\r"),
            b'\t' => Some(b"\\t"),
            _ if byte < 0x20 => {
                let len = control_escape_(&mut escape_buf, byte);
                Some(&escape_buf[..len])
            }
            _ => None,
        };
        if let Some(escape) = escape {
            if start < index {
                write_all_async_(writer, &bytes[start..index], cancel.child_token()).await?;
            }
            write_all_async_(writer, escape, cancel.child_token()).await?;
            start = index + 1;
        }
    }
    if start < bytes.len() {
        write_all_async_(writer, &bytes[start..], cancel.child_token()).await?;
    }
    write_all_async_(writer, b"\"", cancel).await
}

/// 写一段数字文本；`key` 为真时按 JSON 对象键的规则加引号。
async fn write_text_async_<W, C>(
    writer: &mut W,
    text: TextBuf,
    key: bool,
    cancel: C,
) -> Result<(), JsonError>
where
    W: TrBuffWrite<u8>,
    C: TrCancellationToken,
{
    if key {
        write_all_async_(writer, b"\"", cancel.child_token()).await?;
        write_all_async_(writer, text.as_slice_(), cancel.child_token()).await?;
        write_all_async_(writer, b"\"", cancel).await
    } else {
        write_all_async_(writer, text.as_slice_(), cancel).await
    }
}

/// 把字节串写成 JSON 数字数组。
async fn write_binary_async_<W, C>(writer: &mut W, value: &[u8], cancel: C) -> Result<(), JsonError>
where
    W: TrBuffWrite<u8>,
    C: TrCancellationToken,
{
    write_all_async_(writer, b"[", cancel.child_token()).await?;
    for (index, &byte) in value.iter().enumerate() {
        if index > 0 {
            write_all_async_(writer, b",", cancel.child_token()).await?;
        }
        let text = text_of_(byte)?;
        write_all_async_(writer, text.as_slice_(), cancel.child_token()).await?;
    }
    write_all_async_(writer, b"]", cancel).await
}

#[gen_may_cancel_future(EncodeTextOp, pub)]
async fn encode_text_async_<'w, W, C>(
    writer: &'w mut W,
    text: Result<TextBuf, JsonError>,
    key: bool,
    cancel: C,
) -> Result<(), JsonError>
where
    W: TrBuffWrite<u8> + 'w,
    C: TrCancellationToken,
{
    write_text_async_(writer, text?, key, cancel).await
}

#[gen_may_cancel_future(EncodeQuotedOp, pub)]
async fn encode_quoted_async_<'w, 'b, W, C>(
    writer: &'w mut W,
    value: &'b str,
    cancel: C,
) -> Result<(), JsonError>
where
    W: TrBuffWrite<u8> + 'w,
    C: TrCancellationToken,
{
    write_quoted_async_(writer, value, cancel).await
}

#[gen_may_cancel_future(EncodeCharOp, pub)]
async fn encode_char_async_<'w, W, C>(
    writer: &'w mut W,
    value: char,
    cancel: C,
) -> Result<(), JsonError>
where
    W: TrBuffWrite<u8> + 'w,
    C: TrCancellationToken,
{
    let mut buf = [0u8; 4];
    write_quoted_async_(writer, value.encode_utf8(&mut buf), cancel).await
}

#[gen_may_cancel_future(EncodeBinaryOp, pub)]
async fn encode_binary_async_<'w, 'b, W, C>(
    writer: &'w mut W,
    value: &'b [u8],
    key: bool,
    cancel: C,
) -> Result<(), JsonError>
where
    W: TrBuffWrite<u8> + 'w,
    C: TrCancellationToken,
{
    if key {
        return Err(JsonError::new("JSON object key must be a string"));
    }
    write_binary_async_(writer, value, cancel).await
}

#[gen_may_cancel_future(EncodeLiteralOp, pub)]
async fn encode_literal_async_<'w, W, C>(
    writer: &'w mut W,
    literal: &'static [u8],
    key: bool,
    cancel: C,
) -> Result<(), JsonError>
where
    W: TrBuffWrite<u8> + 'w,
    C: TrCancellationToken,
{
    if key {
        return Err(JsonError::new("JSON object key must be a string"));
    }
    write_all_async_(writer, literal, cancel).await
}

#[gen_may_cancel_future(EncodeSeqHeaderOp, pub)]
async fn encode_seq_header_async_<'w, W, C>(
    writer: &'w mut W,
    key: bool,
    cancel: C,
) -> Result<JsonSeqEncoder<'w, W>, JsonError>
where
    W: TrBuffWrite<u8> + 'w,
    C: TrCancellationToken,
{
    if key {
        return Err(JsonError::new("JSON object key must be a string"));
    }
    write_all_async_(writer, b"[", cancel).await?;
    Ok(JsonSeqEncoder {
        writer_: writer,
        first_: true,
        close_: b"]",
    })
}

#[gen_may_cancel_future(EncodeTupleVariantOp, pub)]
async fn encode_tuple_variant_async_<'w, W, C>(
    writer: &'w mut W,
    variant: &'static str,
    key: bool,
    cancel: C,
) -> Result<JsonSeqEncoder<'w, W>, JsonError>
where
    W: TrBuffWrite<u8> + 'w,
    C: TrCancellationToken,
{
    if key {
        return Err(JsonError::new("JSON object key must be a string"));
    }
    write_all_async_(writer, b"{", cancel.child_token()).await?;
    write_quoted_async_(writer, variant, cancel.child_token()).await?;
    write_all_async_(writer, b":[", cancel).await?;
    Ok(JsonSeqEncoder {
        writer_: writer,
        first_: true,
        close_: b"]}",
    })
}

#[gen_may_cancel_future(EncodeMapHeaderOp, pub)]
async fn encode_map_header_async_<'w, W, C>(
    writer: &'w mut W,
    key: bool,
    cancel: C,
) -> Result<JsonMapEncoder<'w, W>, JsonError>
where
    W: TrBuffWrite<u8> + 'w,
    C: TrCancellationToken,
{
    if key {
        return Err(JsonError::new("JSON object key must be a string"));
    }
    write_all_async_(writer, b"{", cancel).await?;
    Ok(JsonMapEncoder {
        writer_: writer,
        first_: true,
        close_: b"}",
    })
}

#[gen_may_cancel_future(EncodeStructVariantOp, pub)]
async fn encode_struct_variant_async_<'w, W, C>(
    writer: &'w mut W,
    variant: &'static str,
    key: bool,
    cancel: C,
) -> Result<JsonMapEncoder<'w, W>, JsonError>
where
    W: TrBuffWrite<u8> + 'w,
    C: TrCancellationToken,
{
    if key {
        return Err(JsonError::new("JSON object key must be a string"));
    }
    write_all_async_(writer, b"{", cancel.child_token()).await?;
    write_quoted_async_(writer, variant, cancel.child_token()).await?;
    write_all_async_(writer, b":{", cancel).await?;
    Ok(JsonMapEncoder {
        writer_: writer,
        first_: true,
        close_: b"}}",
    })
}

#[gen_may_cancel_future(EncodeNewtypeVariantOp, pub)]
async fn encode_newtype_variant_async_<'w, 'b, W, T, C>(
    writer: &'w mut W,
    variant: &'static str,
    value: &'b T,
    key: bool,
    cancel: C,
) -> Result<(), JsonError>
where
    W: TrBuffWrite<u8> + 'w,
    T: ?Sized + TrEncodable + 'b,
    C: TrCancellationToken,
{
    if key {
        return Err(JsonError::new("JSON object key must be a string"));
    }
    write_all_async_(writer, b"{", cancel.child_token()).await?;
    write_quoted_async_(writer, variant, cancel.child_token()).await?;
    write_all_async_(writer, b":", cancel.child_token()).await?;
    value
        .encode_async(JsonEncoder::new(writer))
        .may_cancel_with(cancel.child_token())
        .await?;
    write_all_async_(writer, b"}", cancel).await
}

#[gen_may_cancel_future(EncodeElementOp, pub)]
async fn encode_element_async_<'w, 'b, W, T, C>(
    writer: &'w mut W,
    first: &'w mut bool,
    value: &'b T,
    cancel: C,
) -> Result<(), JsonError>
where
    W: TrBuffWrite<u8> + 'w,
    T: ?Sized + TrEncodable + 'b,
    C: TrCancellationToken,
{
    if !*first {
        write_all_async_(writer, b",", cancel.child_token()).await?;
    }
    *first = false;
    value
        .encode_async(JsonEncoder::new(writer))
        .may_cancel_with(cancel)
        .await
}

#[gen_may_cancel_future(EncodeMapKeyOp, pub)]
async fn encode_map_key_async_<'w, 'b, W, T, C>(
    writer: &'w mut W,
    first: &'w mut bool,
    key_value: &'b T,
    cancel: C,
) -> Result<(), JsonError>
where
    W: TrBuffWrite<u8> + 'w,
    T: ?Sized + TrEncodable + 'b,
    C: TrCancellationToken,
{
    if !*first {
        write_all_async_(writer, b",", cancel.child_token()).await?;
    }
    *first = false;
    key_value
        .encode_async(JsonEncoder::new_key_(writer))
        .may_cancel_with(cancel)
        .await
}

#[gen_may_cancel_future(EncodeMapValueOp, pub)]
async fn encode_map_value_async_<'w, 'b, W, T, C>(
    writer: &'w mut W,
    value: &'b T,
    cancel: C,
) -> Result<(), JsonError>
where
    W: TrBuffWrite<u8> + 'w,
    T: ?Sized + TrEncodable + 'b,
    C: TrCancellationToken,
{
    write_all_async_(writer, b":", cancel.child_token()).await?;
    value
        .encode_async(JsonEncoder::new(writer))
        .may_cancel_with(cancel)
        .await
}

#[gen_may_cancel_future(EncodeStructFieldOp, pub)]
async fn encode_struct_field_async_<'w, 'b, W, T, C>(
    writer: &'w mut W,
    first: &'w mut bool,
    key: &'static str,
    value: &'b T,
    cancel: C,
) -> Result<(), JsonError>
where
    W: TrBuffWrite<u8> + 'w,
    T: ?Sized + TrEncodable + 'b,
    C: TrCancellationToken,
{
    if !*first {
        write_all_async_(writer, b",", cancel.child_token()).await?;
    }
    *first = false;
    write_quoted_async_(writer, key, cancel.child_token()).await?;
    write_all_async_(writer, b":", cancel.child_token()).await?;
    value
        .encode_async(JsonEncoder::new(writer))
        .may_cancel_with(cancel)
        .await
}

#[gen_may_cancel_future(EncodeCloseOp, pub)]
async fn encode_close_async_<'w, W, C>(
    writer: &'w mut W,
    close: &'static [u8],
    cancel: C,
) -> Result<(), JsonError>
where
    W: TrBuffWrite<u8> + 'w,
    C: TrCancellationToken,
{
    write_all_async_(writer, close, cancel).await
}

/// 把数据结构写成 JSON 的编码器。
///
/// 构造时借用写端；所有片段都会先借出写段，再把栈上准备好的字节写入。
pub struct JsonEncoder<'w, W> {
    writer_: &'w mut W,
    /// 是否正在编码一个 JSON 对象键。
    key_: bool,
}

impl<'w, W> JsonEncoder<'w, W> {
    /// 借用 `writer` 构造编码器。
    ///
    /// # Examples
    ///
    /// ```
    /// use json_denco::JsonEncoder;
    ///
    /// let mut storage = [0u8; 8];
    /// let mut sink: &mut [u8] = &mut storage;
    /// let _encoder = JsonEncoder::new(&mut sink);
    /// ```
    pub fn new(writer: &'w mut W) -> Self {
        Self {
            writer_: writer,
            key_: false,
        }
    }

    fn new_key_(writer: &'w mut W) -> Self {
        Self {
            writer_: writer,
            key_: true,
        }
    }
}

impl<'w, W> TrEncoder for JsonEncoder<'w, W>
where
    W: TrBuffWrite<u8> + 'w,
{
    type Ok = ();
    type Error = JsonError;

    type EncodeSeq = JsonSeqEncoder<'w, W>;
    type EncodeTuple = JsonSeqEncoder<'w, W>;
    type EncodeTupleStruct = JsonSeqEncoder<'w, W>;
    type EncodeTupleVariant = JsonSeqEncoder<'w, W>;
    type EncodeMap = JsonMapEncoder<'w, W>;
    type EncodeStruct = JsonMapEncoder<'w, W>;
    type EncodeStructVariant = JsonMapEncoder<'w, W>;

    type EncodeBoolAsync<'a>
        = EncodeTextOpAsync<'w, 'a, W>
    where
        Self: 'a,
        Self::Ok: 'a,
        Self::Error: 'a,
        'w: 'a;

    type EncodeI8Async<'a>
        = EncodeTextOpAsync<'w, 'a, W>
    where
        Self: 'a,
        Self::Ok: 'a,
        Self::Error: 'a,
        'w: 'a;

    type EncodeI16Async<'a>
        = EncodeTextOpAsync<'w, 'a, W>
    where
        Self: 'a,
        Self::Ok: 'a,
        Self::Error: 'a,
        'w: 'a;

    type EncodeI32Async<'a>
        = EncodeTextOpAsync<'w, 'a, W>
    where
        Self: 'a,
        Self::Ok: 'a,
        Self::Error: 'a,
        'w: 'a;

    type EncodeI64Async<'a>
        = EncodeTextOpAsync<'w, 'a, W>
    where
        Self: 'a,
        Self::Ok: 'a,
        Self::Error: 'a,
        'w: 'a;

    type EncodeI128Async<'a>
        = EncodeTextOpAsync<'w, 'a, W>
    where
        Self: 'a,
        Self::Ok: 'a,
        Self::Error: 'a,
        'w: 'a;

    type EncodeU8Async<'a>
        = EncodeTextOpAsync<'w, 'a, W>
    where
        Self: 'a,
        Self::Ok: 'a,
        Self::Error: 'a,
        'w: 'a;

    type EncodeU16Async<'a>
        = EncodeTextOpAsync<'w, 'a, W>
    where
        Self: 'a,
        Self::Ok: 'a,
        Self::Error: 'a,
        'w: 'a;

    type EncodeU32Async<'a>
        = EncodeTextOpAsync<'w, 'a, W>
    where
        Self: 'a,
        Self::Ok: 'a,
        Self::Error: 'a,
        'w: 'a;

    type EncodeU64Async<'a>
        = EncodeTextOpAsync<'w, 'a, W>
    where
        Self: 'a,
        Self::Ok: 'a,
        Self::Error: 'a,
        'w: 'a;

    type EncodeU128Async<'a>
        = EncodeTextOpAsync<'w, 'a, W>
    where
        Self: 'a,
        Self::Ok: 'a,
        Self::Error: 'a,
        'w: 'a;

    type EncodeF32Async<'a>
        = EncodeTextOpAsync<'w, 'a, W>
    where
        Self: 'a,
        Self::Ok: 'a,
        Self::Error: 'a,
        'w: 'a;

    type EncodeF64Async<'a>
        = EncodeTextOpAsync<'w, 'a, W>
    where
        Self: 'a,
        Self::Ok: 'a,
        Self::Error: 'a,
        'w: 'a;

    type EncodeCharAsync<'a>
        = EncodeCharOpAsync<'w, 'a, W>
    where
        Self: 'a,
        Self::Ok: 'a,
        Self::Error: 'a,
        'w: 'a;

    type EncodeStrAsync<'a, 'b>
        = EncodeQuotedOpAsync<'w, 'b, 'a, W>
    where
        Self: 'a,
        Self::Error: 'a,
        'b: 'a,
        Self::Ok: 'a,
        'w: 'a;

    type EncodeBytesAsync<'a, 'b>
        = EncodeBinaryOpAsync<'w, 'b, 'a, W>
    where
        Self: 'a,
        Self::Error: 'a,
        'b: 'a,
        Self::Ok: 'a,
        'w: 'a;

    type EncodeNoneAsync<'a>
        = EncodeLiteralOpAsync<'w, 'a, W>
    where
        Self: 'a,
        Self::Ok: 'a,
        Self::Error: 'a,
        'w: 'a;

    type EncodeSomeAsync<'a, 'b, T>
        = <T as TrEncodable>::EncodeAsync<'a, Self>
    where
        T: ?Sized + TrEncodable,
        T: 'a,
        T: 'b,
        Self: 'a,
        Self::Error: 'a,
        'b: 'a;

    type EncodeUnitAsync<'a>
        = EncodeLiteralOpAsync<'w, 'a, W>
    where
        Self: 'a,
        Self::Ok: 'a,
        Self::Error: 'a,
        'w: 'a;

    type EncodeUnitStructAsync<'a>
        = EncodeLiteralOpAsync<'w, 'a, W>
    where
        Self: 'a,
        Self::Ok: 'a,
        Self::Error: 'a,
        'w: 'a;

    type EncodeUnitVariantAsync<'a>
        = EncodeQuotedOpAsync<'w, 'static, 'a, W>
    where
        Self: 'a,
        Self::Ok: 'a,
        Self::Error: 'a,
        'w: 'a;

    type EncodeNewtypeStructAsync<'a, 'b, T>
        = <T as TrEncodable>::EncodeAsync<'a, Self>
    where
        T: ?Sized + TrEncodable,
        T: 'a,
        T: 'b,
        Self: 'a,
        Self::Error: 'a,
        'b: 'a;

    type EncodeNewtypeVariantAsync<'a, 'b, T>
        = EncodeNewtypeVariantOpAsync<'w, 'b, 'a, W, T>
    where
        T: ?Sized + TrEncodable + 'a + 'b,
        Self: 'a,
        Self::Ok: 'a,
        Self::Error: 'a,
        'b: 'a,
        'w: 'a;

    type EncodeSeqAsync<'a>
        = EncodeSeqHeaderOpAsync<'w, 'a, W>
    where
        Self: 'a,
        Self::Error: 'a,
        Self::EncodeSeq: 'a,
        'w: 'a;

    type EncodeTupleAsync<'a>
        = EncodeSeqHeaderOpAsync<'w, 'a, W>
    where
        Self: 'a,
        Self::Error: 'a,
        Self::EncodeTuple: 'a,
        'w: 'a;

    type EncodeTupleStructAsync<'a>
        = EncodeSeqHeaderOpAsync<'w, 'a, W>
    where
        Self: 'a,
        Self::Error: 'a,
        Self::EncodeTupleStruct: 'a,
        'w: 'a;

    type EncodeTupleVariantAsync<'a>
        = EncodeTupleVariantOpAsync<'w, 'a, W>
    where
        Self: 'a,
        Self::Error: 'a,
        Self::EncodeTupleVariant: 'a,
        'w: 'a;

    type EncodeMapAsync<'a>
        = EncodeMapHeaderOpAsync<'w, 'a, W>
    where
        Self: 'a,
        Self::Error: 'a,
        Self::EncodeMap: 'a,
        'w: 'a;

    type EncodeStructAsync<'a>
        = EncodeMapHeaderOpAsync<'w, 'a, W>
    where
        Self: 'a,
        Self::Error: 'a,
        Self::EncodeStruct: 'a,
        'w: 'a;

    type EncodeStructVariantAsync<'a>
        = EncodeStructVariantOpAsync<'w, 'a, W>
    where
        Self: 'a,
        Self::Error: 'a,
        Self::EncodeStructVariant: 'a,
        'w: 'a;

    fn encode_bool<'a>(self, v: bool) -> Self::EncodeBoolAsync<'a>
    where
        Self: 'a,
        Self::Ok: 'a,
        Self::Error: 'a,
    {
        let text = if v {
            Ok(text_of_("true").expect("布尔文本不会超长"))
        } else {
            Ok(text_of_("false").expect("布尔文本不会超长"))
        };
        EncodeTextOpAsync::<'w, 'a, W>::new(self.writer_, text, self.key_)
    }

    fn encode_i8<'a>(self, v: i8) -> Self::EncodeI8Async<'a>
    where
        Self: 'a,
        Self::Ok: 'a,
        Self::Error: 'a,
    {
        let text = Ok(text_of_(v).expect("i8 文本不会超长"));
        EncodeTextOpAsync::<'w, 'a, W>::new(self.writer_, text, self.key_)
    }

    fn encode_i16<'a>(self, v: i16) -> Self::EncodeI16Async<'a>
    where
        Self: 'a,
        Self::Ok: 'a,
        Self::Error: 'a,
    {
        let text = Ok(text_of_(v).expect("i16 文本不会超长"));
        EncodeTextOpAsync::<'w, 'a, W>::new(self.writer_, text, self.key_)
    }

    fn encode_i32<'a>(self, v: i32) -> Self::EncodeI32Async<'a>
    where
        Self: 'a,
        Self::Ok: 'a,
        Self::Error: 'a,
    {
        let text = Ok(text_of_(v).expect("i32 文本不会超长"));
        EncodeTextOpAsync::<'w, 'a, W>::new(self.writer_, text, self.key_)
    }

    fn encode_i64<'a>(self, v: i64) -> Self::EncodeI64Async<'a>
    where
        Self: 'a,
        Self::Ok: 'a,
        Self::Error: 'a,
    {
        let text = Ok(text_of_(v).expect("i64 文本不会超长"));
        EncodeTextOpAsync::<'w, 'a, W>::new(self.writer_, text, self.key_)
    }

    fn encode_i128<'a>(self, v: i128) -> Self::EncodeI128Async<'a>
    where
        Self: 'a,
        Self::Ok: 'a,
        Self::Error: 'a,
    {
        let text = Ok(text_of_(v).expect("i128 文本不会超长"));
        EncodeTextOpAsync::<'w, 'a, W>::new(self.writer_, text, self.key_)
    }

    fn encode_u8<'a>(self, v: u8) -> Self::EncodeU8Async<'a>
    where
        Self: 'a,
        Self::Ok: 'a,
        Self::Error: 'a,
    {
        let text = Ok(text_of_(v).expect("u8 文本不会超长"));
        EncodeTextOpAsync::<'w, 'a, W>::new(self.writer_, text, self.key_)
    }

    fn encode_u16<'a>(self, v: u16) -> Self::EncodeU16Async<'a>
    where
        Self: 'a,
        Self::Ok: 'a,
        Self::Error: 'a,
    {
        let text = Ok(text_of_(v).expect("u16 文本不会超长"));
        EncodeTextOpAsync::<'w, 'a, W>::new(self.writer_, text, self.key_)
    }

    fn encode_u32<'a>(self, v: u32) -> Self::EncodeU32Async<'a>
    where
        Self: 'a,
        Self::Ok: 'a,
        Self::Error: 'a,
    {
        let text = Ok(text_of_(v).expect("u32 文本不会超长"));
        EncodeTextOpAsync::<'w, 'a, W>::new(self.writer_, text, self.key_)
    }

    fn encode_u64<'a>(self, v: u64) -> Self::EncodeU64Async<'a>
    where
        Self: 'a,
        Self::Ok: 'a,
        Self::Error: 'a,
    {
        let text = Ok(text_of_(v).expect("u64 文本不会超长"));
        EncodeTextOpAsync::<'w, 'a, W>::new(self.writer_, text, self.key_)
    }

    fn encode_u128<'a>(self, v: u128) -> Self::EncodeU128Async<'a>
    where
        Self: 'a,
        Self::Ok: 'a,
        Self::Error: 'a,
    {
        let text = Ok(text_of_(v).expect("u128 文本不会超长"));
        EncodeTextOpAsync::<'w, 'a, W>::new(self.writer_, text, self.key_)
    }

    fn encode_f32<'a>(self, v: f32) -> Self::EncodeF32Async<'a>
    where
        Self: 'a,
        Self::Ok: 'a,
        Self::Error: 'a,
    {
        EncodeTextOpAsync::<'w, 'a, W>::new(self.writer_, f32_text_(v), self.key_)
    }

    fn encode_f64<'a>(self, v: f64) -> Self::EncodeF64Async<'a>
    where
        Self: 'a,
        Self::Ok: 'a,
        Self::Error: 'a,
    {
        EncodeTextOpAsync::<'w, 'a, W>::new(self.writer_, f64_text_(v), self.key_)
    }

    fn encode_char<'a>(self, v: char) -> Self::EncodeCharAsync<'a>
    where
        Self: 'a,
        Self::Ok: 'a,
        Self::Error: 'a,
    {
        EncodeCharOpAsync::<'w, 'a, W>::new(self.writer_, v)
    }

    fn encode_str<'a, 'b>(self, v: &'b str) -> Self::EncodeStrAsync<'a, 'b>
    where
        Self: 'a,
        Self::Ok: 'a,
        Self::Error: 'a,
        'b: 'a,
    {
        EncodeQuotedOpAsync::<'w, 'b, 'a, W>::new(self.writer_, v)
    }

    fn encode_bytes<'a, 'b>(self, v: &'b [u8]) -> Self::EncodeBytesAsync<'a, 'b>
    where
        Self: 'a,
        Self::Ok: 'a,
        Self::Error: 'a,
        'b: 'a,
    {
        EncodeBinaryOpAsync::<'w, 'b, 'a, W>::new(self.writer_, v, self.key_)
    }

    fn encode_none<'a>(self) -> Self::EncodeNoneAsync<'a>
    where
        Self: 'a,
        Self::Ok: 'a,
        Self::Error: 'a,
    {
        EncodeLiteralOpAsync::<'w, 'a, W>::new(self.writer_, b"null", self.key_)
    }

    fn encode_some<'a, 'b, T>(self, value: &'b T) -> Self::EncodeSomeAsync<'a, 'b, T>
    where
        T: ?Sized + TrEncodable + 'a + 'b,
        Self: 'a,
        Self::Ok: 'a,
        Self::Error: 'a,
        'b: 'a,
    {
        value.encode_async(self)
    }

    fn encode_unit<'a>(self) -> Self::EncodeUnitAsync<'a>
    where
        Self: 'a,
        Self::Ok: 'a,
        Self::Error: 'a,
    {
        EncodeLiteralOpAsync::<'w, 'a, W>::new(self.writer_, b"null", self.key_)
    }

    fn encode_unit_struct<'a>(self, _name: &'static str) -> Self::EncodeUnitStructAsync<'a>
    where
        Self: 'a,
        Self::Ok: 'a,
        Self::Error: 'a,
    {
        EncodeLiteralOpAsync::<'w, 'a, W>::new(self.writer_, b"null", self.key_)
    }

    fn encode_unit_variant<'a>(
        self,
        _name: &'static str,
        _variant_index: u32,
        variant: &'static str,
    ) -> Self::EncodeUnitVariantAsync<'a>
    where
        Self: 'a,
        Self::Ok: 'a,
        Self::Error: 'a,
    {
        EncodeQuotedOpAsync::<'w, 'static, 'a, W>::new(self.writer_, variant)
    }

    fn encode_newtype_struct<'a, 'b, T>(
        self,
        _name: &'static str,
        value: &'b T,
    ) -> Self::EncodeNewtypeStructAsync<'a, 'b, T>
    where
        T: ?Sized + TrEncodable + 'a + 'b,
        Self: 'a,
        Self::Ok: 'a,
        Self::Error: 'a,
        'b: 'a,
    {
        value.encode_async(self)
    }

    fn encode_newtype_variant<'a, 'b, T>(
        self,
        _name: &'static str,
        _variant_index: u32,
        variant: &'static str,
        value: &'b T,
    ) -> Self::EncodeNewtypeVariantAsync<'a, 'b, T>
    where
        T: ?Sized + TrEncodable + 'a + 'b,
        Self: 'a,
        Self::Ok: 'a,
        Self::Error: 'a,
        'b: 'a,
    {
        EncodeNewtypeVariantOpAsync::<'w, 'b, 'a, W, T>::new(
            self.writer_,
            variant,
            value,
            self.key_,
        )
    }

    fn encode_seq<'a>(self, _len: Option<usize>) -> Self::EncodeSeqAsync<'a>
    where
        Self: 'a,
        Self::EncodeSeq: 'a,
        Self::Error: 'a,
    {
        EncodeSeqHeaderOpAsync::<'w, 'a, W>::new(self.writer_, self.key_)
    }

    fn encode_tuple<'a>(self, _len: usize) -> Self::EncodeTupleAsync<'a>
    where
        Self: 'a,
        Self::EncodeTuple: 'a,
        Self::Error: 'a,
    {
        EncodeSeqHeaderOpAsync::<'w, 'a, W>::new(self.writer_, self.key_)
    }

    fn encode_tuple_struct<'a>(
        self,
        _name: &'static str,
        _len: usize,
    ) -> Self::EncodeTupleStructAsync<'a>
    where
        Self: 'a,
        Self::EncodeTupleStruct: 'a,
        Self::Error: 'a,
    {
        EncodeSeqHeaderOpAsync::<'w, 'a, W>::new(self.writer_, self.key_)
    }

    fn encode_tuple_variant<'a>(
        self,
        _name: &'static str,
        _variant_index: u32,
        variant: &'static str,
        _len: usize,
    ) -> Self::EncodeTupleVariantAsync<'a>
    where
        Self: 'a,
        Self::EncodeTupleVariant: 'a,
        Self::Error: 'a,
    {
        EncodeTupleVariantOpAsync::<'w, 'a, W>::new(self.writer_, variant, self.key_)
    }

    fn encode_map<'a>(self, _len: Option<usize>) -> Self::EncodeMapAsync<'a>
    where
        Self: 'a,
        Self::EncodeMap: 'a,
        Self::Error: 'a,
    {
        EncodeMapHeaderOpAsync::<'w, 'a, W>::new(self.writer_, self.key_)
    }

    fn encode_struct<'a>(self, _name: &'static str, _len: usize) -> Self::EncodeStructAsync<'a>
    where
        Self: 'a,
        Self::EncodeStruct: 'a,
        Self::Error: 'a,
    {
        EncodeMapHeaderOpAsync::<'w, 'a, W>::new(self.writer_, self.key_)
    }

    fn encode_struct_variant<'a>(
        self,
        _name: &'static str,
        _variant_index: u32,
        variant: &'static str,
        _len: usize,
    ) -> Self::EncodeStructVariantAsync<'a>
    where
        Self: 'a,
        Self::EncodeStructVariant: 'a,
        Self::Error: 'a,
    {
        EncodeStructVariantOpAsync::<'w, 'a, W>::new(self.writer_, variant, self.key_)
    }

    fn is_human_readable(&self) -> bool {
        true
    }
}

/// 序列化 JSON 数组/元组时的状态机。
pub struct JsonSeqEncoder<'w, W> {
    writer_: &'w mut W,
    first_: bool,
    close_: &'static [u8],
}

impl<'w, W> TrEncodeSeq for JsonSeqEncoder<'w, W>
where
    W: TrBuffWrite<u8> + 'w,
{
    type Ok = ();
    type Error = JsonError;

    fn encode_element<'a, 'b, T>(
        &'a mut self,
        value: &'b T,
    ) -> impl TrMayCancel<'a, MayCancelOutput = Result<(), Self::Error>>
    where
        T: ?Sized + TrEncodable,
        Self: 'a,
        Self::Error: 'a,
        'b: 'a,
    {
        EncodeElementOpAsync::<'a, 'b, 'a, W, T>::new(&mut *self.writer_, &mut self.first_, value)
    }

    fn end<'a>(self) -> impl TrMayCancel<'a, MayCancelOutput = Result<Self::Ok, Self::Error>>
    where
        Self: 'a,
        Self::Ok: 'a,
        Self::Error: 'a,
    {
        EncodeCloseOpAsync::<'a, 'a, W>::new(self.writer_, self.close_)
    }
}

impl<'w, W> TrEncodeTuple for JsonSeqEncoder<'w, W>
where
    W: TrBuffWrite<u8> + 'w,
{
    type Ok = ();
    type Error = JsonError;

    fn encode_element<'a, 'b, T>(
        &'a mut self,
        value: &'b T,
    ) -> impl TrMayCancel<'a, MayCancelOutput = Result<(), Self::Error>>
    where
        T: ?Sized + TrEncodable,
        Self: 'a,
        Self::Error: 'a,
        'b: 'a,
    {
        EncodeElementOpAsync::<'a, 'b, 'a, W, T>::new(&mut *self.writer_, &mut self.first_, value)
    }

    fn end<'a>(self) -> impl TrMayCancel<'a, MayCancelOutput = Result<Self::Ok, Self::Error>>
    where
        Self: 'a,
        Self::Ok: 'a,
        Self::Error: 'a,
    {
        EncodeCloseOpAsync::<'a, 'a, W>::new(self.writer_, self.close_)
    }
}

impl<'w, W> TrEncodeTupleStruct for JsonSeqEncoder<'w, W>
where
    W: TrBuffWrite<u8> + 'w,
{
    type Ok = ();
    type Error = JsonError;

    fn encode_field<'a, 'b, T>(
        &'a mut self,
        value: &'b T,
    ) -> impl TrMayCancel<'a, MayCancelOutput = Result<(), Self::Error>>
    where
        T: ?Sized + TrEncodable,
        Self: 'a,
        Self::Error: 'a,
        'b: 'a,
    {
        EncodeElementOpAsync::<'a, 'b, 'a, W, T>::new(&mut *self.writer_, &mut self.first_, value)
    }

    fn end<'a>(self) -> impl TrMayCancel<'a, MayCancelOutput = Result<Self::Ok, Self::Error>>
    where
        Self: 'a,
        Self::Ok: 'a,
        Self::Error: 'a,
    {
        EncodeCloseOpAsync::<'a, 'a, W>::new(self.writer_, self.close_)
    }
}

impl<'w, W> TrEncodeTupleVariant for JsonSeqEncoder<'w, W>
where
    W: TrBuffWrite<u8> + 'w,
{
    type Ok = ();
    type Error = JsonError;

    fn encode_field<'a, 'b, T>(
        &'a mut self,
        value: &'b T,
    ) -> impl TrMayCancel<'a, MayCancelOutput = Result<(), Self::Error>>
    where
        T: ?Sized + TrEncodable,
        Self: 'a,
        Self::Error: 'a,
        'b: 'a,
    {
        EncodeElementOpAsync::<'a, 'b, 'a, W, T>::new(&mut *self.writer_, &mut self.first_, value)
    }

    fn end<'a>(self) -> impl TrMayCancel<'a, MayCancelOutput = Result<Self::Ok, Self::Error>>
    where
        Self: 'a,
        Self::Ok: 'a,
        Self::Error: 'a,
    {
        EncodeCloseOpAsync::<'a, 'a, W>::new(self.writer_, self.close_)
    }
}

/// 序列化 JSON 对象/结构体时的状态机。
pub struct JsonMapEncoder<'w, W> {
    writer_: &'w mut W,
    first_: bool,
    close_: &'static [u8],
}

impl<'w, W> TrEncodeMap for JsonMapEncoder<'w, W>
where
    W: TrBuffWrite<u8> + 'w,
{
    type Ok = ();
    type Error = JsonError;

    fn encode_key<'a, 'b, T>(
        &'a mut self,
        key: &'b T,
    ) -> impl TrMayCancel<'a, MayCancelOutput = Result<(), Self::Error>>
    where
        T: ?Sized + TrEncodable,
        Self: 'a,
        Self::Error: 'a,
        'b: 'a,
    {
        EncodeMapKeyOpAsync::<'a, 'b, 'a, W, T>::new(&mut *self.writer_, &mut self.first_, key)
    }

    fn encode_value<'a, 'b, T>(
        &'a mut self,
        value: &'b T,
    ) -> impl TrMayCancel<'a, MayCancelOutput = Result<(), Self::Error>>
    where
        T: ?Sized + TrEncodable,
        Self: 'a,
        Self::Error: 'a,
        'b: 'a,
    {
        EncodeMapValueOpAsync::<'a, 'b, 'a, W, T>::new(&mut *self.writer_, value)
    }

    fn end<'a>(self) -> impl TrMayCancel<'a, MayCancelOutput = Result<Self::Ok, Self::Error>>
    where
        Self: 'a,
        Self::Ok: 'a,
        Self::Error: 'a,
    {
        EncodeCloseOpAsync::<'a, 'a, W>::new(self.writer_, self.close_)
    }
}

impl<'w, W> TrEncodeStruct for JsonMapEncoder<'w, W>
where
    W: TrBuffWrite<u8> + 'w,
{
    type Ok = ();
    type Error = JsonError;

    fn encode_field<'a, 'b, T>(
        &'a mut self,
        key: &'static str,
        value: &'b T,
    ) -> impl TrMayCancel<'a, MayCancelOutput = Result<(), Self::Error>>
    where
        T: ?Sized + TrEncodable,
        Self: 'a,
        Self::Error: 'a,
        'b: 'a,
    {
        EncodeStructFieldOpAsync::<'a, 'b, 'a, W, T>::new(
            &mut *self.writer_,
            &mut self.first_,
            key,
            value,
        )
    }

    fn end<'a>(self) -> impl TrMayCancel<'a, MayCancelOutput = Result<Self::Ok, Self::Error>>
    where
        Self: 'a,
        Self::Ok: 'a,
        Self::Error: 'a,
    {
        EncodeCloseOpAsync::<'a, 'a, W>::new(self.writer_, self.close_)
    }
}

impl<'w, W> TrEncodeStructVariant for JsonMapEncoder<'w, W>
where
    W: TrBuffWrite<u8> + 'w,
{
    type Ok = ();
    type Error = JsonError;

    fn encode_field<'a, 'b, T>(
        &'a mut self,
        key: &'static str,
        value: &'b T,
    ) -> impl TrMayCancel<'a, MayCancelOutput = Result<(), Self::Error>>
    where
        T: ?Sized + TrEncodable,
        Self: 'a,
        Self::Error: 'a,
        'b: 'a,
    {
        EncodeStructFieldOpAsync::<'a, 'b, 'a, W, T>::new(
            &mut *self.writer_,
            &mut self.first_,
            key,
            value,
        )
    }

    fn end<'a>(self) -> impl TrMayCancel<'a, MayCancelOutput = Result<Self::Ok, Self::Error>>
    where
        Self: 'a,
        Self::Ok: 'a,
        Self::Error: 'a,
    {
        EncodeCloseOpAsync::<'a, 'a, W>::new(self.writer_, self.close_)
    }
}
