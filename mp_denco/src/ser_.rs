//! 最小 MessagePack 序列化器。
//!
//! 编码器直接面向 [`abs_buff::TrBuffWrite`] 工作：所有定长写入都通过
//! `write_async` 借出写段，再按实际可写量推进游标，不把数据先攒进
//! `Vec<u8>`。数字与长度头在栈上编码，字符串与二进制直接引用调用方数据。

use core::{
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

use crate::MpError;

/// 栈上编码缓冲的最大长度。
///
/// 最长的是 128 位整数的 `bin8` 头 + 16 字节负载（18 字节）；其余标量、
/// 数组/映射/字符串/二进制头都更短。
const SCALAR_CAP_: usize = 18;

/// 一个立即完成、但会检查预取消状态的 future。
///
/// MessagePack 的内存序列化在单个写入步骤内是同步的；这个包装器负责在
/// 调用 `may_cancel_with` 时检查令牌是否已经取消。若已取消，它会用错误
/// 替换原本的结果。
pub struct CancellableReady<R> {
    result_: Option<Result<R, MpError>>,
}

impl<R> CancellableReady<R> {
    /// 用已经完成的结果构造。
    pub fn new(result: Result<R, MpError>) -> Self {
        Self {
            result_: Some(result),
        }
    }
}

impl<R> Future for CancellableReady<R>
where
    R: Unpin,
{
    type Output = Result<R, MpError>;

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

    type MayCancelOutput = Result<R, MpError>;

    fn may_cancel_with<C>(mut self, cancel: C) -> Self::MayCancelFuture<'a, C>
    where
        C: 'a + TrCancellationToken,
    {
        if cancel.is_cancelled() {
            self.result_ = Some(Err(MpError::new("operation cancelled")));
        }
        self
    }
}

/// 栈上的短字节序列。
///
/// 所有头部与标量都先编码到这里，再通过 `write_async` 写入写段；不产生堆分配。
#[derive(Clone, Copy)]
struct ScalarBytes {
    data_: [u8; SCALAR_CAP_],
    len_: usize,
}

impl ScalarBytes {
    const fn new_() -> Self {
        Self {
            data_: [0u8; SCALAR_CAP_],
            len_: 0,
        }
    }

    fn push_(&mut self, byte: u8) {
        self.data_[self.len_] = byte;
        self.len_ += 1;
    }

    fn extend_(&mut self, bytes: &[u8]) {
        let end = self.len_ + bytes.len();
        self.data_[self.len_..end].copy_from_slice(bytes);
        self.len_ = end;
    }

    fn as_slice_(&self) -> &[u8] {
        &self.data_[..self.len_]
    }
}

fn u64_bytes_(v: u64) -> ScalarBytes {
    let mut out = ScalarBytes::new_();
    if v <= 0x7f {
        out.push_(v as u8);
    } else if v <= u8::MAX as u64 {
        out.push_(0xcc);
        out.push_(v as u8);
    } else if v <= u16::MAX as u64 {
        out.push_(0xcd);
        out.extend_(&(v as u16).to_be_bytes());
    } else if v <= u32::MAX as u64 {
        out.push_(0xce);
        out.extend_(&(v as u32).to_be_bytes());
    } else {
        out.push_(0xcf);
        out.extend_(&v.to_be_bytes());
    }
    out
}

fn i64_bytes_(v: i64) -> ScalarBytes {
    let mut out = ScalarBytes::new_();
    if v >= 0 {
        return u64_bytes_(v as u64);
    }
    if v >= -32 {
        out.push_(v as i8 as u8);
    } else if v >= i8::MIN as i64 {
        out.push_(0xd0);
        out.push_(v as i8 as u8);
    } else if v >= i16::MIN as i64 {
        out.push_(0xd1);
        out.extend_(&(v as i16).to_be_bytes());
    } else if v >= i32::MIN as i64 {
        out.push_(0xd2);
        out.extend_(&(v as i32).to_be_bytes());
    } else {
        out.push_(0xd3);
        out.extend_(&v.to_be_bytes());
    }
    out
}

fn f32_bytes_(v: f32) -> ScalarBytes {
    let mut out = ScalarBytes::new_();
    out.push_(0xca);
    out.extend_(&v.to_be_bytes());
    out
}

fn f64_bytes_(v: f64) -> ScalarBytes {
    let mut out = ScalarBytes::new_();
    out.push_(0xcb);
    out.extend_(&v.to_be_bytes());
    out
}

fn i128_bytes_(v: i128) -> ScalarBytes {
    let mut out = ScalarBytes::new_();
    out.push_(0xc4);
    out.push_(0x10);
    out.extend_(&v.to_be_bytes());
    out
}

fn u128_bytes_(v: u128) -> ScalarBytes {
    let mut out = ScalarBytes::new_();
    out.push_(0xc4);
    out.push_(0x10);
    out.extend_(&v.to_be_bytes());
    out
}

fn str_header_(len: usize) -> Result<ScalarBytes, MpError> {
    let mut out = ScalarBytes::new_();
    if len <= 31 {
        out.push_(0xa0 | len as u8);
    } else if len <= u8::MAX as usize {
        out.push_(0xd9);
        out.push_(len as u8);
    } else if len <= u16::MAX as usize {
        out.push_(0xda);
        out.extend_(&(len as u16).to_be_bytes());
    } else if len <= u32::MAX as usize {
        out.push_(0xdb);
        out.extend_(&(len as u32).to_be_bytes());
    } else {
        return Err(MpError::new("string too long for MessagePack"));
    }
    Ok(out)
}

fn bin_header_(len: usize) -> Result<ScalarBytes, MpError> {
    let mut out = ScalarBytes::new_();
    if len <= u8::MAX as usize {
        out.push_(0xc4);
        out.push_(len as u8);
    } else if len <= u16::MAX as usize {
        out.push_(0xc5);
        out.extend_(&(len as u16).to_be_bytes());
    } else if len <= u32::MAX as usize {
        out.push_(0xc6);
        out.extend_(&(len as u32).to_be_bytes());
    } else {
        return Err(MpError::new("binary too long for MessagePack"));
    }
    Ok(out)
}

fn array_header_(len: usize) -> Result<ScalarBytes, MpError> {
    let mut out = ScalarBytes::new_();
    if len <= 15 {
        out.push_(0x90 | len as u8);
    } else if len <= u16::MAX as usize {
        out.push_(0xdc);
        out.extend_(&(len as u16).to_be_bytes());
    } else if len <= u32::MAX as usize {
        out.push_(0xdd);
        out.extend_(&(len as u32).to_be_bytes());
    } else {
        return Err(MpError::new("array too long for MessagePack"));
    }
    Ok(out)
}

fn map_header_(len: usize) -> Result<ScalarBytes, MpError> {
    let mut out = ScalarBytes::new_();
    if len <= 15 {
        out.push_(0x80 | len as u8);
    } else if len <= u16::MAX as usize {
        out.push_(0xde);
        out.extend_(&(len as u16).to_be_bytes());
    } else if len <= u32::MAX as usize {
        out.push_(0xdf);
        out.extend_(&(len as u32).to_be_bytes());
    } else {
        return Err(MpError::new("map too long for MessagePack"));
    }
    Ok(out)
}

/// 把 `bytes` 全部写入写端，按写段实际可写量循环推进。
async fn write_all_async_<W, C>(writer: &mut W, mut bytes: &[u8], cancel: C) -> Result<(), MpError>
where
    W: TrBuffWrite<u8>,
    C: TrCancellationToken,
{
    if cancel.is_cancelled() {
        return Err(MpError::new("operation cancelled"));
    }
    while !bytes.is_empty() {
        if cancel.is_cancelled() {
            return Err(MpError::new("operation cancelled"));
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
                .map(MpError::buff_write)
                .unwrap_or_else(|| MpError::new("buff write returned no segment"));
            return Err(err);
        }
        bytes = &bytes[moved..];
    }
    Ok(())
}

async fn write_str_async_<W, C>(writer: &mut W, value: &str, cancel: C) -> Result<(), MpError>
where
    W: TrBuffWrite<u8>,
    C: TrCancellationToken,
{
    let header = str_header_(value.len())?;
    write_all_async_(writer, header.as_slice_(), cancel.child_token()).await?;
    write_all_async_(writer, value.as_bytes(), cancel).await
}

async fn write_bin_async_<W, C>(writer: &mut W, value: &[u8], cancel: C) -> Result<(), MpError>
where
    W: TrBuffWrite<u8>,
    C: TrCancellationToken,
{
    let header = bin_header_(value.len())?;
    write_all_async_(writer, header.as_slice_(), cancel.child_token()).await?;
    write_all_async_(writer, value, cancel).await
}

#[gen_may_cancel_future(EncodeFixedOp, pub)]
async fn encode_fixed_async_<'w, W, C>(
    writer: &'w mut W,
    bytes: ScalarBytes,
    cancel: C,
) -> Result<(), MpError>
where
    W: TrBuffWrite<u8> + 'w,
    C: TrCancellationToken,
{
    write_all_async_(writer, bytes.as_slice_(), cancel).await
}

#[gen_may_cancel_future(EncodeCharOp, pub)]
async fn encode_char_async_<'w, W, C>(
    writer: &'w mut W,
    value: char,
    cancel: C,
) -> Result<(), MpError>
where
    W: TrBuffWrite<u8> + 'w,
    C: TrCancellationToken,
{
    let mut buf = [0u8; 4];
    write_str_async_(writer, value.encode_utf8(&mut buf), cancel).await
}

#[gen_may_cancel_future(EncodeStrOp, pub)]
async fn encode_str_async_<'w, 'b, W, C>(
    writer: &'w mut W,
    value: &'b str,
    cancel: C,
) -> Result<(), MpError>
where
    W: TrBuffWrite<u8> + 'w,
    C: TrCancellationToken,
{
    write_str_async_(writer, value, cancel).await
}

#[gen_may_cancel_future(EncodeBinOp, pub)]
async fn encode_bin_async_<'w, 'b, W, C>(
    writer: &'w mut W,
    value: &'b [u8],
    cancel: C,
) -> Result<(), MpError>
where
    W: TrBuffWrite<u8> + 'w,
    C: TrCancellationToken,
{
    write_bin_async_(writer, value, cancel).await
}

#[gen_may_cancel_future(EncodeSeqHeaderOp, pub)]
async fn encode_seq_header_async_<'w, W, C>(
    writer: &'w mut W,
    header: Result<ScalarBytes, MpError>,
    remaining: usize,
    cancel: C,
) -> Result<MpSeqEncoder<'w, W>, MpError>
where
    W: TrBuffWrite<u8> + 'w,
    C: TrCancellationToken,
{
    let header = header?;
    write_all_async_(writer, header.as_slice_(), cancel).await?;
    Ok(MpSeqEncoder {
        writer_: writer,
        remaining_: remaining,
    })
}

#[gen_may_cancel_future(EncodeMapHeaderOp, pub)]
async fn encode_map_header_async_<'w, W, C>(
    writer: &'w mut W,
    header: Result<ScalarBytes, MpError>,
    remaining: usize,
    cancel: C,
) -> Result<MpMapEncoder<'w, W>, MpError>
where
    W: TrBuffWrite<u8> + 'w,
    C: TrCancellationToken,
{
    let header = header?;
    write_all_async_(writer, header.as_slice_(), cancel).await?;
    Ok(MpMapEncoder {
        writer_: writer,
        remaining_: remaining,
    })
}

#[gen_may_cancel_future(EncodeNewtypeVariantOp, pub)]
async fn encode_newtype_variant_async_<'w, 'b, W, T, C>(
    writer: &'w mut W,
    variant: &'static str,
    value: &'b T,
    cancel: C,
) -> Result<(), MpError>
where
    W: TrBuffWrite<u8> + 'w,
    T: ?Sized + TrEncodable + 'b,
    C: TrCancellationToken,
{
    let header = map_header_(1)?;
    write_all_async_(writer, header.as_slice_(), cancel.child_token()).await?;
    write_str_async_(writer, variant, cancel.child_token()).await?;
    value
        .encode_async(MpEncoder::new(writer))
        .may_cancel_with(cancel)
        .await
}

#[gen_may_cancel_future(EncodeTupleVariantOp, pub)]
async fn encode_tuple_variant_async_<'w, W, C>(
    writer: &'w mut W,
    variant: &'static str,
    len: usize,
    cancel: C,
) -> Result<MpSeqEncoder<'w, W>, MpError>
where
    W: TrBuffWrite<u8> + 'w,
    C: TrCancellationToken,
{
    let map = map_header_(1)?;
    write_all_async_(writer, map.as_slice_(), cancel.child_token()).await?;
    write_str_async_(writer, variant, cancel.child_token()).await?;
    let array = array_header_(len)?;
    write_all_async_(writer, array.as_slice_(), cancel).await?;
    Ok(MpSeqEncoder {
        writer_: writer,
        remaining_: len,
    })
}

#[gen_may_cancel_future(EncodeStructVariantOp, pub)]
async fn encode_struct_variant_async_<'w, W, C>(
    writer: &'w mut W,
    variant: &'static str,
    len: usize,
    cancel: C,
) -> Result<MpMapEncoder<'w, W>, MpError>
where
    W: TrBuffWrite<u8> + 'w,
    C: TrCancellationToken,
{
    let map = map_header_(1)?;
    write_all_async_(writer, map.as_slice_(), cancel.child_token()).await?;
    write_str_async_(writer, variant, cancel.child_token()).await?;
    let fields = map_header_(len)?;
    write_all_async_(writer, fields.as_slice_(), cancel).await?;
    Ok(MpMapEncoder {
        writer_: writer,
        remaining_: len,
    })
}

#[gen_may_cancel_future(EncodeElementOp, pub)]
async fn encode_element_async_<'w, 'b, W, T, C>(
    writer: &'w mut W,
    value: &'b T,
    cancel: C,
) -> Result<(), MpError>
where
    W: TrBuffWrite<u8> + 'w,
    T: ?Sized + TrEncodable + 'b,
    C: TrCancellationToken,
{
    value
        .encode_async(MpEncoder::new(writer))
        .may_cancel_with(cancel)
        .await
}

#[gen_may_cancel_future(EncodeStructFieldOp, pub)]
async fn encode_struct_field_async_<'w, 'b, W, T, C>(
    writer: &'w mut W,
    key: &'static str,
    value: &'b T,
    cancel: C,
) -> Result<(), MpError>
where
    W: TrBuffWrite<u8> + 'w,
    T: ?Sized + TrEncodable + 'b,
    C: TrCancellationToken,
{
    write_str_async_(writer, key, cancel.child_token()).await?;
    value
        .encode_async(MpEncoder::new(writer))
        .may_cancel_with(cancel)
        .await
}

/// 把数据结构写入一个 [`TrBuffWrite`] 的 MessagePack 编码器。
///
/// 构造时借用写端；所有标量方法都会先借出写段，再把栈上编码好的字节写入。
pub struct MpEncoder<'w, W> {
    writer_: &'w mut W,
}

impl<'w, W> MpEncoder<'w, W> {
    /// 借用 `writer` 构造编码器。
    ///
    /// # Examples
    ///
    /// ```
    /// use mp_denco::MpEncoder;
    ///
    /// let mut storage = [0u8; 8];
    /// let mut sink: &mut [u8] = &mut storage;
    /// let _encoder = MpEncoder::new(&mut sink);
    /// ```
    pub fn new(writer: &'w mut W) -> Self {
        Self { writer_: writer }
    }
}

impl<'w, W> TrEncoder for MpEncoder<'w, W>
where
    W: TrBuffWrite<u8> + 'w,
{
    type Ok = ();
    type Error = MpError;

    type EncodeSeq = MpSeqEncoder<'w, W>;
    type EncodeTuple = MpSeqEncoder<'w, W>;
    type EncodeTupleStruct = MpSeqEncoder<'w, W>;
    type EncodeTupleVariant = MpSeqEncoder<'w, W>;
    type EncodeMap = MpMapEncoder<'w, W>;
    type EncodeStruct = MpMapEncoder<'w, W>;
    type EncodeStructVariant = MpMapEncoder<'w, W>;

    type EncodeBoolAsync<'a>
        = EncodeFixedOpAsync<'w, 'a, W>
    where
        Self: 'a,
        Self::Ok: 'a,
        Self::Error: 'a,
        'w: 'a;

    type EncodeI8Async<'a>
        = EncodeFixedOpAsync<'w, 'a, W>
    where
        Self: 'a,
        Self::Ok: 'a,
        Self::Error: 'a,
        'w: 'a;

    type EncodeI16Async<'a>
        = EncodeFixedOpAsync<'w, 'a, W>
    where
        Self: 'a,
        Self::Ok: 'a,
        Self::Error: 'a,
        'w: 'a;

    type EncodeI32Async<'a>
        = EncodeFixedOpAsync<'w, 'a, W>
    where
        Self: 'a,
        Self::Ok: 'a,
        Self::Error: 'a,
        'w: 'a;

    type EncodeI64Async<'a>
        = EncodeFixedOpAsync<'w, 'a, W>
    where
        Self: 'a,
        Self::Ok: 'a,
        Self::Error: 'a,
        'w: 'a;

    type EncodeI128Async<'a>
        = EncodeFixedOpAsync<'w, 'a, W>
    where
        Self: 'a,
        Self::Ok: 'a,
        Self::Error: 'a,
        'w: 'a;

    type EncodeU8Async<'a>
        = EncodeFixedOpAsync<'w, 'a, W>
    where
        Self: 'a,
        Self::Ok: 'a,
        Self::Error: 'a,
        'w: 'a;

    type EncodeU16Async<'a>
        = EncodeFixedOpAsync<'w, 'a, W>
    where
        Self: 'a,
        Self::Ok: 'a,
        Self::Error: 'a,
        'w: 'a;

    type EncodeU32Async<'a>
        = EncodeFixedOpAsync<'w, 'a, W>
    where
        Self: 'a,
        Self::Ok: 'a,
        Self::Error: 'a,
        'w: 'a;

    type EncodeU64Async<'a>
        = EncodeFixedOpAsync<'w, 'a, W>
    where
        Self: 'a,
        Self::Ok: 'a,
        Self::Error: 'a,
        'w: 'a;

    type EncodeU128Async<'a>
        = EncodeFixedOpAsync<'w, 'a, W>
    where
        Self: 'a,
        Self::Ok: 'a,
        Self::Error: 'a,
        'w: 'a;

    type EncodeF32Async<'a>
        = EncodeFixedOpAsync<'w, 'a, W>
    where
        Self: 'a,
        Self::Ok: 'a,
        Self::Error: 'a,
        'w: 'a;

    type EncodeF64Async<'a>
        = EncodeFixedOpAsync<'w, 'a, W>
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
        = EncodeStrOpAsync<'w, 'b, 'a, W>
    where
        Self: 'a,
        Self::Error: 'a,
        'b: 'a,
        Self::Ok: 'a,
        'w: 'a;

    type EncodeBytesAsync<'a, 'b>
        = EncodeBinOpAsync<'w, 'b, 'a, W>
    where
        Self: 'a,
        Self::Error: 'a,
        'b: 'a,
        Self::Ok: 'a,
        'w: 'a;

    type EncodeNoneAsync<'a>
        = EncodeFixedOpAsync<'w, 'a, W>
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
        = EncodeFixedOpAsync<'w, 'a, W>
    where
        Self: 'a,
        Self::Ok: 'a,
        Self::Error: 'a,
        'w: 'a;

    type EncodeUnitStructAsync<'a>
        = EncodeFixedOpAsync<'w, 'a, W>
    where
        Self: 'a,
        Self::Ok: 'a,
        Self::Error: 'a,
        'w: 'a;

    type EncodeUnitVariantAsync<'a>
        = EncodeStrOpAsync<'w, 'static, 'a, W>
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
        let byte = if v { 0xc3 } else { 0xc2 };
        let mut bytes = ScalarBytes::new_();
        bytes.push_(byte);
        EncodeFixedOpAsync::<'w, 'a, W>::new(self.writer_, bytes)
    }

    fn encode_i8<'a>(self, v: i8) -> Self::EncodeI8Async<'a>
    where
        Self: 'a,
        Self::Ok: 'a,
        Self::Error: 'a,
    {
        EncodeFixedOpAsync::<'w, 'a, W>::new(self.writer_, i64_bytes_(v as i64))
    }

    fn encode_i16<'a>(self, v: i16) -> Self::EncodeI16Async<'a>
    where
        Self: 'a,
        Self::Ok: 'a,
        Self::Error: 'a,
    {
        EncodeFixedOpAsync::<'w, 'a, W>::new(self.writer_, i64_bytes_(v as i64))
    }

    fn encode_i32<'a>(self, v: i32) -> Self::EncodeI32Async<'a>
    where
        Self: 'a,
        Self::Ok: 'a,
        Self::Error: 'a,
    {
        EncodeFixedOpAsync::<'w, 'a, W>::new(self.writer_, i64_bytes_(v as i64))
    }

    fn encode_i64<'a>(self, v: i64) -> Self::EncodeI64Async<'a>
    where
        Self: 'a,
        Self::Ok: 'a,
        Self::Error: 'a,
    {
        EncodeFixedOpAsync::<'w, 'a, W>::new(self.writer_, i64_bytes_(v))
    }

    fn encode_i128<'a>(self, v: i128) -> Self::EncodeI128Async<'a>
    where
        Self: 'a,
        Self::Ok: 'a,
        Self::Error: 'a,
    {
        EncodeFixedOpAsync::<'w, 'a, W>::new(self.writer_, i128_bytes_(v))
    }

    fn encode_u8<'a>(self, v: u8) -> Self::EncodeU8Async<'a>
    where
        Self: 'a,
        Self::Ok: 'a,
        Self::Error: 'a,
    {
        EncodeFixedOpAsync::<'w, 'a, W>::new(self.writer_, u64_bytes_(v as u64))
    }

    fn encode_u16<'a>(self, v: u16) -> Self::EncodeU16Async<'a>
    where
        Self: 'a,
        Self::Ok: 'a,
        Self::Error: 'a,
    {
        EncodeFixedOpAsync::<'w, 'a, W>::new(self.writer_, u64_bytes_(v as u64))
    }

    fn encode_u32<'a>(self, v: u32) -> Self::EncodeU32Async<'a>
    where
        Self: 'a,
        Self::Ok: 'a,
        Self::Error: 'a,
    {
        EncodeFixedOpAsync::<'w, 'a, W>::new(self.writer_, u64_bytes_(v as u64))
    }

    fn encode_u64<'a>(self, v: u64) -> Self::EncodeU64Async<'a>
    where
        Self: 'a,
        Self::Ok: 'a,
        Self::Error: 'a,
    {
        EncodeFixedOpAsync::<'w, 'a, W>::new(self.writer_, u64_bytes_(v))
    }

    fn encode_u128<'a>(self, v: u128) -> Self::EncodeU128Async<'a>
    where
        Self: 'a,
        Self::Ok: 'a,
        Self::Error: 'a,
    {
        EncodeFixedOpAsync::<'w, 'a, W>::new(self.writer_, u128_bytes_(v))
    }

    fn encode_f32<'a>(self, v: f32) -> Self::EncodeF32Async<'a>
    where
        Self: 'a,
        Self::Ok: 'a,
        Self::Error: 'a,
    {
        EncodeFixedOpAsync::<'w, 'a, W>::new(self.writer_, f32_bytes_(v))
    }

    fn encode_f64<'a>(self, v: f64) -> Self::EncodeF64Async<'a>
    where
        Self: 'a,
        Self::Ok: 'a,
        Self::Error: 'a,
    {
        EncodeFixedOpAsync::<'w, 'a, W>::new(self.writer_, f64_bytes_(v))
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
        EncodeStrOpAsync::<'w, 'b, 'a, W>::new(self.writer_, v)
    }

    fn encode_bytes<'a, 'b>(self, v: &'b [u8]) -> Self::EncodeBytesAsync<'a, 'b>
    where
        Self: 'a,
        Self::Ok: 'a,
        Self::Error: 'a,
        'b: 'a,
    {
        EncodeBinOpAsync::<'w, 'b, 'a, W>::new(self.writer_, v)
    }

    fn encode_none<'a>(self) -> Self::EncodeNoneAsync<'a>
    where
        Self: 'a,
        Self::Ok: 'a,
        Self::Error: 'a,
    {
        let mut bytes = ScalarBytes::new_();
        bytes.push_(0xc0);
        EncodeFixedOpAsync::<'w, 'a, W>::new(self.writer_, bytes)
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
        let mut bytes = ScalarBytes::new_();
        bytes.push_(0xc0);
        EncodeFixedOpAsync::<'w, 'a, W>::new(self.writer_, bytes)
    }

    fn encode_unit_struct<'a>(self, _name: &'static str) -> Self::EncodeUnitStructAsync<'a>
    where
        Self: 'a,
        Self::Ok: 'a,
        Self::Error: 'a,
    {
        // `rmp-serde` 把单元结构体写成空数组而不是 nil。
        let mut bytes = ScalarBytes::new_();
        bytes.push_(0x90);
        EncodeFixedOpAsync::<'w, 'a, W>::new(self.writer_, bytes)
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
        EncodeStrOpAsync::<'w, 'static, 'a, W>::new(self.writer_, variant)
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
        EncodeNewtypeVariantOpAsync::<'w, 'b, 'a, W, T>::new(self.writer_, variant, value)
    }

    fn encode_seq<'a>(self, len: Option<usize>) -> Self::EncodeSeqAsync<'a>
    where
        Self: 'a,
        Self::EncodeSeq: 'a,
        Self::Error: 'a,
    {
        let (header, remaining) = match len {
            Some(len) => (array_header_(len), len),
            None => (
                Err(MpError::new("mp_denco requires a known sequence length")),
                0,
            ),
        };
        EncodeSeqHeaderOpAsync::<'w, 'a, W>::new(self.writer_, header, remaining)
    }

    fn encode_tuple<'a>(self, len: usize) -> Self::EncodeTupleAsync<'a>
    where
        Self: 'a,
        Self::EncodeTuple: 'a,
        Self::Error: 'a,
    {
        EncodeSeqHeaderOpAsync::<'w, 'a, W>::new(self.writer_, array_header_(len), len)
    }

    fn encode_tuple_struct<'a>(
        self,
        _name: &'static str,
        len: usize,
    ) -> Self::EncodeTupleStructAsync<'a>
    where
        Self: 'a,
        Self::EncodeTupleStruct: 'a,
        Self::Error: 'a,
    {
        EncodeSeqHeaderOpAsync::<'w, 'a, W>::new(self.writer_, array_header_(len), len)
    }

    fn encode_tuple_variant<'a>(
        self,
        _name: &'static str,
        _variant_index: u32,
        variant: &'static str,
        len: usize,
    ) -> Self::EncodeTupleVariantAsync<'a>
    where
        Self: 'a,
        Self::EncodeTupleVariant: 'a,
        Self::Error: 'a,
    {
        EncodeTupleVariantOpAsync::<'w, 'a, W>::new(self.writer_, variant, len)
    }

    fn encode_map<'a>(self, len: Option<usize>) -> Self::EncodeMapAsync<'a>
    where
        Self: 'a,
        Self::EncodeMap: 'a,
        Self::Error: 'a,
    {
        let (header, remaining) = match len {
            Some(len) => (map_header_(len), len),
            None => (Err(MpError::new("mp_denco requires a known map length")), 0),
        };
        EncodeMapHeaderOpAsync::<'w, 'a, W>::new(self.writer_, header, remaining)
    }

    fn encode_struct<'a>(self, _name: &'static str, len: usize) -> Self::EncodeStructAsync<'a>
    where
        Self: 'a,
        Self::EncodeStruct: 'a,
        Self::Error: 'a,
    {
        EncodeMapHeaderOpAsync::<'w, 'a, W>::new(self.writer_, map_header_(len), len)
    }

    fn encode_struct_variant<'a>(
        self,
        _name: &'static str,
        _variant_index: u32,
        variant: &'static str,
        len: usize,
    ) -> Self::EncodeStructVariantAsync<'a>
    where
        Self: 'a,
        Self::EncodeStructVariant: 'a,
        Self::Error: 'a,
    {
        EncodeStructVariantOpAsync::<'w, 'a, W>::new(self.writer_, variant, len)
    }

    fn is_human_readable(&self) -> bool {
        false
    }
}

/// 序列化序列/元组时的状态机。
pub struct MpSeqEncoder<'w, W> {
    writer_: &'w mut W,
    #[allow(dead_code)]
    remaining_: usize,
}

impl<'w, W> TrEncodeSeq for MpSeqEncoder<'w, W>
where
    W: TrBuffWrite<u8> + 'w,
{
    type Ok = ();
    type Error = MpError;

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
        EncodeElementOpAsync::<'a, 'b, 'a, W, T>::new(&mut *self.writer_, value)
    }

    fn end<'a>(self) -> impl TrMayCancel<'a, MayCancelOutput = Result<Self::Ok, Self::Error>>
    where
        Self: 'a,
        Self::Ok: 'a,
        Self::Error: 'a,
    {
        CancellableReady::new(Ok(()))
    }
}

impl<'w, W> TrEncodeTuple for MpSeqEncoder<'w, W>
where
    W: TrBuffWrite<u8> + 'w,
{
    type Ok = ();
    type Error = MpError;

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
        EncodeElementOpAsync::<'a, 'b, 'a, W, T>::new(&mut *self.writer_, value)
    }

    fn end<'a>(self) -> impl TrMayCancel<'a, MayCancelOutput = Result<Self::Ok, Self::Error>>
    where
        Self: 'a,
        Self::Ok: 'a,
        Self::Error: 'a,
    {
        CancellableReady::new(Ok(()))
    }
}

impl<'w, W> TrEncodeTupleStruct for MpSeqEncoder<'w, W>
where
    W: TrBuffWrite<u8> + 'w,
{
    type Ok = ();
    type Error = MpError;

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
        EncodeElementOpAsync::<'a, 'b, 'a, W, T>::new(&mut *self.writer_, value)
    }

    fn end<'a>(self) -> impl TrMayCancel<'a, MayCancelOutput = Result<Self::Ok, Self::Error>>
    where
        Self: 'a,
        Self::Ok: 'a,
        Self::Error: 'a,
    {
        CancellableReady::new(Ok(()))
    }
}

impl<'w, W> TrEncodeTupleVariant for MpSeqEncoder<'w, W>
where
    W: TrBuffWrite<u8> + 'w,
{
    type Ok = ();
    type Error = MpError;

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
        EncodeElementOpAsync::<'a, 'b, 'a, W, T>::new(&mut *self.writer_, value)
    }

    fn end<'a>(self) -> impl TrMayCancel<'a, MayCancelOutput = Result<Self::Ok, Self::Error>>
    where
        Self: 'a,
        Self::Ok: 'a,
        Self::Error: 'a,
    {
        CancellableReady::new(Ok(()))
    }
}

/// 序列化映射/结构体时的状态机。
pub struct MpMapEncoder<'w, W> {
    writer_: &'w mut W,
    #[allow(dead_code)]
    remaining_: usize,
}

impl<'w, W> TrEncodeMap for MpMapEncoder<'w, W>
where
    W: TrBuffWrite<u8> + 'w,
{
    type Ok = ();
    type Error = MpError;

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
        EncodeElementOpAsync::<'a, 'b, 'a, W, T>::new(&mut *self.writer_, key)
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
        EncodeElementOpAsync::<'a, 'b, 'a, W, T>::new(&mut *self.writer_, value)
    }

    fn end<'a>(self) -> impl TrMayCancel<'a, MayCancelOutput = Result<Self::Ok, Self::Error>>
    where
        Self: 'a,
        Self::Ok: 'a,
        Self::Error: 'a,
    {
        CancellableReady::new(Ok(()))
    }
}

impl<'w, W> TrEncodeStruct for MpMapEncoder<'w, W>
where
    W: TrBuffWrite<u8> + 'w,
{
    type Ok = ();
    type Error = MpError;

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
        EncodeStructFieldOpAsync::<'a, 'b, 'a, W, T>::new(&mut *self.writer_, key, value)
    }

    fn end<'a>(self) -> impl TrMayCancel<'a, MayCancelOutput = Result<Self::Ok, Self::Error>>
    where
        Self: 'a,
        Self::Ok: 'a,
        Self::Error: 'a,
    {
        CancellableReady::new(Ok(()))
    }
}

impl<'w, W> TrEncodeStructVariant for MpMapEncoder<'w, W>
where
    W: TrBuffWrite<u8> + 'w,
{
    type Ok = ();
    type Error = MpError;

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
        EncodeStructFieldOpAsync::<'a, 'b, 'a, W, T>::new(&mut *self.writer_, key, value)
    }

    fn end<'a>(self) -> impl TrMayCancel<'a, MayCancelOutput = Result<Self::Ok, Self::Error>>
    where
        Self: 'a,
        Self::Ok: 'a,
        Self::Error: 'a,
    {
        CancellableReady::new(Ok(()))
    }
}
