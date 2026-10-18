//! 最小 MessagePack 反序列化器。
//!
//! 解码器直接面向 [`abs_buff::TrBuffRead`] 工作：标签、长度与定长负载都通过
//! `read_async` 按需借段读取。字符串与二进制一律解码为 owned 结果：短值放
//! 栈数组，长值边读边构造 `String` / `Vec<u8>`。

use alloc::{string::String, vec::Vec};
use core::str;

use abs_buff::{
    Demand, TrBuffRead,
    buffer::TrBuffSegmRef,
};
use abs_cancel::{TrCancellationToken, TrMayCancel};
use abs_denco::{
    de::{EnumAccess, MapAccess, SeqAccess, TrDecodableSeed, TrDecoder, VariantAccess, Visitor},
    forward_to_decode_any,
};
use gen_mcf2::gen_may_cancel_future;

use crate::MpError;

/// 短字符串/二进制在栈上的读取缓冲上限。
const STACK_BUF_: usize = 256;

/// 枚举变体名在栈上的缓冲上限。
const NAME_CAP_: usize = 128;

/// 从一段 buff 读取 MessagePack 的解码器。
///
/// `'r` 是读端的借用寿命；解码结果一律拥有所有权，不借用读端内存。
pub struct MpDecoder<'r, R> {
    reader_: &'r mut R,
    pending_: Option<u8>,
}

impl<'r, R> MpDecoder<'r, R> {
    /// 从 `reader` 构造解码器。
    ///
    /// # Examples
    ///
    /// ```
    /// use mp_denco::MpDecoder;
    ///
    /// let bytes = [0xc0u8];
    /// let mut input: &[u8] = &bytes;
    /// let _decoder = MpDecoder::new(&mut input);
    /// ```
    pub fn new(reader: &'r mut R) -> Self {
        Self {
            reader_: reader,
            pending_: None,
        }
    }
}

fn is_str_marker_(tag: u8) -> bool {
    matches!(tag, 0xa0..=0xbf | 0xd9..=0xdb)
}

fn is_map_marker_(tag: u8) -> bool {
    matches!(tag, 0x80..=0x8f | 0xde | 0xdf)
}

/// 按 `rmp-serde` 的约定读取 128 位整数：既接受普通整数标记，
/// 也接受 16 字节大端 binary。
async fn read_i128_marker_async_<'r, R, C>(
    de: &mut MpDecoder<'r, R>,
    cancel: C,
) -> Result<i128, MpError>
where
    R: TrBuffRead<u8> + 'r,
    C: TrCancellationToken,
{
    let tag = read_tag_async_(de, cancel.child_token()).await?;
    match tag {
        0x00..=0x7f => Ok(tag as i128),
        0xe0..=0xff => Ok((tag as i8) as i128),
        0xcc => Ok(read_u8_async_(de, cancel).await? as i128),
        0xcd => Ok(read_u16_async_(de, cancel).await? as i128),
        0xce => Ok(read_u32_async_(de, cancel).await? as i128),
        0xcf => Ok(read_u64_async_(de, cancel).await? as i128),
        0xd0 => Ok((read_u8_async_(de, cancel).await? as i8) as i128),
        0xd1 => Ok((read_u16_async_(de, cancel).await? as i16) as i128),
        0xd2 => Ok((read_u32_async_(de, cancel).await? as i32) as i128),
        0xd3 => Ok((read_u64_async_(de, cancel).await? as i64) as i128),
        0xc4 => {
            let len = read_u8_async_(de, cancel.child_token()).await? as usize;
            read_128_buf_async_(de, len, cancel).await
        }
        0xc5 => {
            let len = read_u16_async_(de, cancel.child_token()).await? as usize;
            read_128_buf_async_(de, len, cancel).await
        }
        0xc6 => {
            let len = read_u32_async_(de, cancel.child_token()).await? as usize;
            read_128_buf_async_(de, len, cancel).await
        }
        _ => Err(MpError::new("invalid 128-bit integer encoding")),
    }
}

async fn read_128_buf_async_<'r, R, C>(
    de: &mut MpDecoder<'r, R>,
    len: usize,
    cancel: C,
) -> Result<i128, MpError>
where
    R: TrBuffRead<u8> + 'r,
    C: TrCancellationToken,
{
    if len != 16 {
        return Err(MpError::new("128-bit integer must be 16 bytes"));
    }
    let mut buf = [0u8; 16];
    read_exact_async_(de.reader_, &mut buf, cancel).await?;
    Ok(i128::from_be_bytes(buf))
}

async fn read_tag_async_<'r, R, C>(
    de: &mut MpDecoder<'r, R>,
    cancel: C,
) -> Result<u8, MpError>
where
    R: TrBuffRead<u8> + 'r,
    C: TrCancellationToken,
{
    if cancel.is_cancelled() {
        return Err(MpError::new("operation cancelled"));
    }
    if let Some(byte) = de.pending_.take() {
        return Ok(byte);
    }
    read_byte_async_(de.reader_, cancel).await
}

async fn read_byte_async_<R, C>(reader: &mut R, cancel: C) -> Result<u8, MpError>
where
    R: TrBuffRead<u8>,
    C: TrCancellationToken,
{
    let mut buf = [0u8; 1];
    read_exact_async_(reader, &mut buf, cancel).await?;
    Ok(buf[0])
}

async fn read_exact_async_<R, C>(
    reader: &mut R,
    dst: &mut [u8],
    cancel: C,
) -> Result<(), MpError>
where
    R: TrBuffRead<u8>,
    C: TrCancellationToken,
{
    let total = dst.len();
    let mut filled = 0usize;
    while filled < total {
        if cancel.is_cancelled() {
            return Err(MpError::new("operation cancelled"));
        }
        let demand = Demand::no_more_than(total - filled);
        let mut some = reader
            .read_async(&demand)
            .may_cancel_with(cancel.child_token())
            .await;
        let moved = match some.as_mut().pick_left() {
            Some(segm) => segm.move_items_to_as_buff(&mut dst[filled..]),
            None => 0,
        };
        if moved == 0 {
            let err = some
                .pick_right()
                .map(MpError::buff_read)
                .unwrap_or_else(|| MpError::new("unexpected end of input"));
            return Err(err);
        }
        filled += moved;
    }
    Ok(())
}

async fn read_u8_async_<'r, R, C>(
    de: &mut MpDecoder<'r, R>,
    cancel: C,
) -> Result<u8, MpError>
where
    R: TrBuffRead<u8> + 'r,
    C: TrCancellationToken,
{
    read_byte_async_(de.reader_, cancel).await
}

async fn read_u16_async_<'r, R, C>(
    de: &mut MpDecoder<'r, R>,
    cancel: C,
) -> Result<u16, MpError>
where
    R: TrBuffRead<u8> + 'r,
    C: TrCancellationToken,
{
    let mut buf = [0u8; 2];
    read_exact_async_(de.reader_, &mut buf, cancel).await?;
    Ok(u16::from_be_bytes(buf))
}

async fn read_u32_async_<'r, R, C>(
    de: &mut MpDecoder<'r, R>,
    cancel: C,
) -> Result<u32, MpError>
where
    R: TrBuffRead<u8> + 'r,
    C: TrCancellationToken,
{
    let mut buf = [0u8; 4];
    read_exact_async_(de.reader_, &mut buf, cancel).await?;
    Ok(u32::from_be_bytes(buf))
}

async fn read_u64_async_<'r, R, C>(
    de: &mut MpDecoder<'r, R>,
    cancel: C,
) -> Result<u64, MpError>
where
    R: TrBuffRead<u8> + 'r,
    C: TrCancellationToken,
{
    let mut buf = [0u8; 8];
    read_exact_async_(de.reader_, &mut buf, cancel).await?;
    Ok(u64::from_be_bytes(buf))
}

async fn read_str_len_from_tag_async_<'r, R, C>(
    de: &mut MpDecoder<'r, R>,
    tag: u8,
    cancel: C,
) -> Result<usize, MpError>
where
    R: TrBuffRead<u8> + 'r,
    C: TrCancellationToken,
{
    match tag {
        0xa0..=0xbf => Ok((tag & 0x1f) as usize),
        0xd9 => Ok(read_u8_async_(de, cancel).await? as usize),
        0xda => Ok(read_u16_async_(de, cancel).await? as usize),
        0xdb => Ok(read_u32_async_(de, cancel).await? as usize),
        _ => Err(MpError::new("expected a MessagePack string")),
    }
}

async fn read_bin_len_from_tag_async_<'r, R, C>(
    de: &mut MpDecoder<'r, R>,
    tag: u8,
    cancel: C,
) -> Result<usize, MpError>
where
    R: TrBuffRead<u8> + 'r,
    C: TrCancellationToken,
{
    match tag {
        0xc4 => Ok(read_u8_async_(de, cancel).await? as usize),
        0xc5 => Ok(read_u16_async_(de, cancel).await? as usize),
        0xc6 => Ok(read_u32_async_(de, cancel).await? as usize),
        _ => Err(MpError::new("expected a MessagePack binary")),
    }
}

async fn read_array_len_from_tag_async_<'r, R, C>(
    de: &mut MpDecoder<'r, R>,
    tag: u8,
    cancel: C,
) -> Result<usize, MpError>
where
    R: TrBuffRead<u8> + 'r,
    C: TrCancellationToken,
{
    match tag {
        0x90..=0x9f => Ok((tag & 0x0f) as usize),
        0xdc => Ok(read_u16_async_(de, cancel).await? as usize),
        0xdd => Ok(read_u32_async_(de, cancel).await? as usize),
        _ => Err(MpError::new("expected a MessagePack array")),
    }
}

async fn read_map_len_from_tag_async_<'r, R, C>(
    de: &mut MpDecoder<'r, R>,
    tag: u8,
    cancel: C,
) -> Result<usize, MpError>
where
    R: TrBuffRead<u8> + 'r,
    C: TrCancellationToken,
{
    match tag {
        0x80..=0x8f => Ok((tag & 0x0f) as usize),
        0xde => Ok(read_u16_async_(de, cancel).await? as usize),
        0xdf => Ok(read_u32_async_(de, cancel).await? as usize),
        _ => Err(MpError::new("expected a MessagePack map")),
    }
}

/// 读取字符串负载并驱动 visitor。
///
/// 短值读入栈数组，长值边读边构造结果 `String`，不预拷整个 buff。
async fn visit_str_async_<'de, R, V, C>(
    reader: &mut R,
    len: usize,
    visitor: V,
    cancel: C,
) -> Result<V::Value, MpError>
where
    R: TrBuffRead<u8>,
    V: Visitor<'de>,
    C: TrCancellationToken,
{
    if cancel.is_cancelled() {
        return Err(MpError::new("operation cancelled"));
    }
    if len == 0 {
        return visitor.visit_str("").may_cancel_with(cancel).await;
    }
    if len <= STACK_BUF_ {
        let mut buf = [0u8; STACK_BUF_];
        read_exact_async_(reader, &mut buf[..len], cancel.child_token()).await?;
        let value =
            str::from_utf8(&buf[..len]).map_err(|_| MpError::new("invalid UTF-8 string"))?;
        return visitor.visit_str(value).may_cancel_with(cancel).await;
    }

    let mut bytes = Vec::with_capacity(len);
    let mut remaining = len;
    while remaining > 0 {
        let chunk = core::cmp::min(remaining, STACK_BUF_);
        let mut buf = [0u8; STACK_BUF_];
        read_exact_async_(reader, &mut buf[..chunk], cancel.child_token()).await?;
        bytes.extend_from_slice(&buf[..chunk]);
        remaining -= chunk;
    }
    let value = String::from_utf8(bytes).map_err(|_| MpError::new("invalid UTF-8 string"))?;
    visitor.visit_string(value).may_cancel_with(cancel).await
}

/// 读取二进制负载并驱动 visitor。
async fn visit_bytes_async_<'de, R, V, C>(
    reader: &mut R,
    len: usize,
    visitor: V,
    cancel: C,
) -> Result<V::Value, MpError>
where
    R: TrBuffRead<u8>,
    V: Visitor<'de>,
    C: TrCancellationToken,
{
    if cancel.is_cancelled() {
        return Err(MpError::new("operation cancelled"));
    }
    if len == 0 {
        return visitor.visit_bytes(&[]).may_cancel_with(cancel).await;
    }
    if len <= STACK_BUF_ {
        let mut buf = [0u8; STACK_BUF_];
        read_exact_async_(reader, &mut buf[..len], cancel.child_token()).await?;
        return visitor
            .visit_bytes(&buf[..len])
            .may_cancel_with(cancel)
            .await;
    }

    let mut bytes = Vec::with_capacity(len);
    let mut remaining = len;
    while remaining > 0 {
        let chunk = core::cmp::min(remaining, STACK_BUF_);
        let mut buf = [0u8; STACK_BUF_];
        read_exact_async_(reader, &mut buf[..chunk], cancel.child_token()).await?;
        bytes.extend_from_slice(&buf[..chunk]);
        remaining -= chunk;
    }
    visitor.visit_byte_buf(bytes).may_cancel_with(cancel).await
}

/// 枚举变体名的持有形态。
enum NameRef {
    Stack { data_: [u8; NAME_CAP_], len_: usize },
    Heap(String),
}

async fn read_name_async_<R, C>(reader: &mut R, len: usize, cancel: C) -> Result<NameRef, MpError>
where
    R: TrBuffRead<u8>,
    C: TrCancellationToken,
{
    if cancel.is_cancelled() {
        return Err(MpError::new("operation cancelled"));
    }
    if len == 0 {
        return Ok(NameRef::Stack {
            data_: [0u8; NAME_CAP_],
            len_: 0,
        });
    }
    if len <= NAME_CAP_ {
        let mut data_ = [0u8; NAME_CAP_];
        read_exact_async_(reader, &mut data_[..len], cancel.child_token()).await?;
        str::from_utf8(&data_[..len]).map_err(|_| MpError::new("invalid UTF-8 variant name"))?;
        return Ok(NameRef::Stack { data_, len_: len });
    }

    let mut bytes = Vec::with_capacity(len);
    let mut remaining = len;
    while remaining > 0 {
        let chunk = core::cmp::min(remaining, STACK_BUF_);
        let mut buf = [0u8; STACK_BUF_];
        read_exact_async_(reader, &mut buf[..chunk], cancel.child_token()).await?;
        bytes.extend_from_slice(&buf[..chunk]);
        remaining -= chunk;
    }
    let value = String::from_utf8(bytes).map_err(|_| MpError::new("invalid UTF-8 variant name"))?;
    Ok(NameRef::Heap(value))
}

#[gen_may_cancel_future(DecodeNameOp)]
async fn decode_name_async_<'de, 'a, V, C>(
    name: NameRef,
    visitor: V,
    cancel: C,
) -> Result<V::Value, MpError>
where
    'de: 'a,
    V: Visitor<'de> + 'a,
    V::Value: 'a,
    C: TrCancellationToken,
{
    match name {
        NameRef::Stack { data_, len_ } => {
            let value = str::from_utf8(&data_[..len_])
                .map_err(|_| MpError::new("invalid UTF-8 variant name"))?;
            visitor.visit_str(value).may_cancel_with(cancel).await
        }
        NameRef::Heap(value) => visitor.visit_str(&value).may_cancel_with(cancel).await,
    }
}

/// 只把一个已经读出的变体名当作「字符串值」交给 visitor 的解码器。
///
/// 变体名在读取时已经消费了 MessagePack 字符串头；若再让普通的
/// `MpDecoder` 解析，会把名字首字节误当成新的 tag。本类型直接驱动
/// `visit_str`，供 `EnumAccess::variant_seed` 的标识符 seed 使用。
struct MpNameDecoder {
    name_: NameRef,
}

impl<'de> TrDecoder<'de> for MpNameDecoder {
    type Error = MpError;

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
        DecodeNameOpAsync::<'de, 'a, 'a, V>::new(self.name_, visitor)
    }

    forward_to_decode_any! {
        decode_bool => DecodeBoolAsync,
        decode_i8 => DecodeI8Async,
        decode_i16 => DecodeI16Async,
        decode_i32 => DecodeI32Async,
        decode_i64 => DecodeI64Async,
        decode_i128 => DecodeI128Async,
        decode_u8 => DecodeU8Async,
        decode_u16 => DecodeU16Async,
        decode_u32 => DecodeU32Async,
        decode_u64 => DecodeU64Async,
        decode_u128 => DecodeU128Async,
        decode_f32 => DecodeF32Async,
        decode_f64 => DecodeF64Async,
        decode_char => DecodeCharAsync,
        decode_str => DecodeStrAsync,
        decode_string => DecodeStringAsync,
        decode_bytes => DecodeBytesAsync,
        decode_byte_buf => DecodeByteBufAsync,
        decode_option => DecodeOptionAsync,
        decode_unit => DecodeUnitAsync,
        decode_unit_struct => DecodeUnitStructAsync(name: &'static str),
        decode_newtype_struct => DecodeNewtypeStructAsync(name: &'static str),
        decode_seq => DecodeSeqAsync,
        decode_tuple => DecodeTupleAsync(len: usize),
        decode_tuple_struct => DecodeTupleStructAsync(name: &'static str, len: usize),
        decode_map => DecodeMapAsync,
        decode_struct => DecodeStructAsync(name: &'static str, fields: &'static [&'static str]),
        decode_enum => DecodeEnumAsync(name: &'static str, variants: &'static [&'static str]),
        decode_identifier => DecodeIdentifierAsync,
        decode_ignored_any => DecodeIgnoredAnyAsync,
    }
}

#[gen_may_cancel_future(DecodeElementOp, pub)]
async fn decode_element_async_<'r, 'de, 'a, R, T, C>(
    de: Option<MpDecoder<'r, R>>,
    seed: T,
    cancel: C,
) -> Result<Option<T::Value>, MpError>
where
    'de: 'a,
    'r: 'a,
    R: TrBuffRead<u8> + 'r + 'a,
    T: TrDecodableSeed<'de> + 'a,
    T::Value: 'a,
    C: TrCancellationToken,
{
    let Some(de) = de else {
        return Ok(None);
    };
    let value = seed.decode_async(de).may_cancel_with(cancel).await?;
    Ok(Some(value))
}

#[gen_may_cancel_future(DecodeKeyOp, pub)]
async fn decode_key_async_<'r, 'de, 'a, R, K, C>(
    de: Option<MpDecoder<'r, R>>,
    seed: K,
    err: Option<MpError>,
    cancel: C,
) -> Result<Option<K::Value>, MpError>
where
    'de: 'a,
    'r: 'a,
    R: TrBuffRead<u8> + 'r + 'a,
    K: TrDecodableSeed<'de> + 'a,
    K::Value: 'a,
    C: TrCancellationToken,
{
    if let Some(err) = err {
        return Err(err);
    }
    let Some(de) = de else {
        return Ok(None);
    };
    let value = seed.decode_async(de).may_cancel_with(cancel).await?;
    Ok(Some(value))
}

#[gen_may_cancel_future(DecodeMapValueOp, pub)]
async fn decode_map_value_async_<'r, 'de, 'a, R, V, C>(
    de: Option<MpDecoder<'r, R>>,
    seed: V,
    err: Option<MpError>,
    cancel: C,
) -> Result<V::Value, MpError>
where
    'de: 'a,
    'r: 'a,
    R: TrBuffRead<u8> + 'r + 'a,
    V: TrDecodableSeed<'de> + 'a,
    V::Value: 'a,
    C: TrCancellationToken,
{
    if let Some(err) = err {
        return Err(err);
    }
    let Some(de) = de else {
        return Err(MpError::new("map value without a key"));
    };
    seed.decode_async(de).may_cancel_with(cancel).await
}

#[gen_may_cancel_future(UnitVariantOp, pub)]
async fn unit_variant_async_<'r, 'de, 'a, R, C>(
    de: Option<MpDecoder<'r, R>>,
    cancel: C,
) -> Result<(), MpError>
where
    'de: 'a,
    'r: 'a,
    R: TrBuffRead<u8> + 'r + 'a,
    C: TrCancellationToken,
{
    let Some(mut de) = de else {
        return Ok(());
    };
    let tag = read_tag_async_(&mut de, cancel).await?;
    if tag == 0xc0 {
        Ok(())
    } else {
        Err(MpError::new("expected a unit variant"))
    }
}

#[gen_may_cancel_future(DecodeVariantValueOp, pub)]
async fn decode_variant_value_async_<'r, 'de, 'a, R, T, C>(
    de: Option<MpDecoder<'r, R>>,
    seed: T,
    cancel: C,
) -> Result<T::Value, MpError>
where
    'de: 'a,
    'r: 'a,
    R: TrBuffRead<u8> + 'r + 'a,
    T: TrDecodableSeed<'de> + 'a,
    T::Value: 'a,
    C: TrCancellationToken,
{
    let Some(de) = de else {
        return Err(MpError::new("expected a newtype variant"));
    };
    seed.decode_async(de).may_cancel_with(cancel).await
}

#[gen_may_cancel_future(DecodeVariantTupleOp, pub)]
async fn decode_variant_tuple_async_<'r, 'de, 'a, R, V, C>(
    de: Option<MpDecoder<'r, R>>,
    len: usize,
    visitor: V,
    cancel: C,
) -> Result<V::Value, MpError>
where
    'de: 'a,
    'r: 'a,
    R: TrBuffRead<u8> + 'r + 'a,
    V: Visitor<'de> + 'a,
    V::Value: 'a,
    C: TrCancellationToken,
{
    let Some(de) = de else {
        return Err(MpError::new("expected a tuple variant"));
    };
    de.decode_tuple(len, visitor).may_cancel_with(cancel).await
}

#[gen_may_cancel_future(DecodeVariantStructOp, pub)]
async fn decode_variant_struct_async_<'r, 'de, 'a, R, V, C>(
    de: Option<MpDecoder<'r, R>>,
    fields: &'static [&'static str],
    visitor: V,
    cancel: C,
) -> Result<V::Value, MpError>
where
    'de: 'a,
    'r: 'a,
    R: TrBuffRead<u8> + 'r + 'a,
    V: Visitor<'de> + 'a,
    V::Value: 'a,
    C: TrCancellationToken,
{
    let Some(de) = de else {
        return Err(MpError::new("expected a struct variant"));
    };
    de.decode_struct("", fields, visitor)
        .may_cancel_with(cancel)
        .await
}

#[gen_may_cancel_future(EnumVariantSeedOp)]
async fn enum_variant_seed_async_<'r, 'de, 'a, R, V, C>(
    name: NameRef,
    reader: &'r mut R,
    unit_: bool,
    seed: V,
    cancel: C,
) -> Result<(V::Value, MpVariantAccess<'r, R>), MpError>
where
    'de: 'a,
    'r: 'a,
    R: TrBuffRead<u8> + 'r + 'a,
    V: TrDecodableSeed<'de> + 'a,
    V::Value: 'a,
    C: TrCancellationToken,
{
    let value = seed
        .decode_async(MpNameDecoder { name_: name })
        .may_cancel_with(cancel)
        .await?;
    Ok((
        value,
        MpVariantAccess {
            reader_: reader,
            unit_,
        },
    ))
}

/// MessagePack 数组的访问器。
struct MpSeqAccess<'r, R> {
    reader_: &'r mut R,
    remaining_: usize,
}

impl<'de, 'r, R> SeqAccess<'de> for MpSeqAccess<'r, R>
where
    R: TrBuffRead<u8> + 'r,
{
    type Error = MpError;

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
        let de = if self.remaining_ == 0 {
            None
        } else {
            self.remaining_ -= 1;
            Some(MpDecoder::<'a, R> {
                reader_: &mut *self.reader_,
                pending_: None,
            })
        };
        DecodeElementOpAsync::<'a, 'de, 'a, 'a, R, T>::new(de, seed)
    }

    fn size_hint(&self) -> Option<usize> {
        Some(self.remaining_)
    }
}

/// MessagePack 映射的访问器。
struct MpMapAccess<'r, R> {
    reader_: &'r mut R,
    remaining_: usize,
    has_key_: bool,
}

impl<'de, 'r, R> MapAccess<'de> for MpMapAccess<'r, R>
where
    R: TrBuffRead<u8> + 'r,
{
    type Error = MpError;

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
        let err = if self.has_key_ {
            Some(MpError::new("map value expected before next key"))
        } else {
            None
        };
        let de = if self.remaining_ == 0 || err.is_some() {
            None
        } else {
            self.remaining_ -= 1;
            self.has_key_ = true;
            Some(MpDecoder::<'a, R> {
                reader_: &mut *self.reader_,
                pending_: None,
            })
        };
        DecodeKeyOpAsync::<'a, 'de, 'a, 'a, R, K>::new(de, seed, err)
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
        let err = if self.has_key_ {
            None
        } else {
            Some(MpError::new("map value without a key"))
        };
        let de = if err.is_some() {
            None
        } else {
            self.has_key_ = false;
            Some(MpDecoder::<'a, R> {
                reader_: &mut *self.reader_,
                pending_: None,
            })
        };
        DecodeMapValueOpAsync::<'a, 'de, 'a, 'a, R, V>::new(de, seed, err)
    }

    fn size_hint(&self) -> Option<usize> {
        Some(self.remaining_)
    }
}

/// 枚举访问器：保存变体名与主读端。
struct MpEnumAccess<'r, R> {
    name_: NameRef,
    reader_: &'r mut R,
    unit_: bool,
}

impl<'de, 'r, R> EnumAccess<'de> for MpEnumAccess<'r, R>
where
    R: TrBuffRead<u8> + 'r,
{
    type Error = MpError;
    type Variant = MpVariantAccess<'r, R>;

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
        EnumVariantSeedOpAsync::<'r, 'de, 'a, 'a, R, V>::new(
            self.name_,
            self.reader_,
            self.unit_,
            seed,
        )
    }
}

/// 变体内容访问器。
struct MpVariantAccess<'r, R> {
    reader_: &'r mut R,
    unit_: bool,
}

impl<'de, 'r, R> VariantAccess<'de> for MpVariantAccess<'r, R>
where
    R: TrBuffRead<u8> + 'r,
{
    type Error = MpError;

    fn unit_variant<'a>(self) -> impl TrMayCancel<'a, MayCancelOutput = Result<(), Self::Error>>
    where
        'de: 'a,
        Self: 'a,
        Self::Error: 'a,
    {
        let de = if self.unit_ {
            None
        } else {
            Some(MpDecoder::<'a, R> {
                reader_: &mut *self.reader_,
                pending_: None,
            })
        };
        UnitVariantOpAsync::<'a, 'de, 'a, 'a, R>::new(de)
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
        let de = if self.unit_ {
            None
        } else {
            Some(MpDecoder::<'a, R> {
                reader_: &mut *self.reader_,
                pending_: None,
            })
        };
        DecodeVariantValueOpAsync::<'a, 'de, 'a, 'a, R, T>::new(de, seed)
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
        let de = if self.unit_ {
            None
        } else {
            Some(MpDecoder::<'a, R> {
                reader_: &mut *self.reader_,
                pending_: None,
            })
        };
        DecodeVariantTupleOpAsync::<'a, 'de, 'a, 'a, R, V>::new(de, len, visitor)
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
        let de = if self.unit_ {
            None
        } else {
            Some(MpDecoder::<'a, R> {
                reader_: &mut *self.reader_,
                pending_: None,
            })
        };
        DecodeVariantStructOpAsync::<'a, 'de, 'a, 'a, R, V>::new(de, fields, visitor)
    }
}

#[gen_may_cancel_future(DecodeOptionOp, pub)]
async fn decode_option_async_<'r, 'de, 'a, R, V, C>(
    mut de: MpDecoder<'r, R>,
    visitor: V,
    cancel: C,
) -> Result<V::Value, MpError>
where
    'de: 'a,
    'r: 'a,
    R: TrBuffRead<u8> + 'r + 'a,
    V: Visitor<'de> + 'a,
    V::Value: 'a,
    C: TrCancellationToken,
{
    let tag = read_tag_async_(&mut de, cancel.child_token()).await?;
    if tag == 0xc0 {
        return visitor.visit_none().may_cancel_with(cancel).await;
    }
    de.pending_ = Some(tag);
    visitor.visit_some(de).may_cancel_with(cancel).await
}

#[gen_may_cancel_future(DecodeUnitStructOp, pub)]
async fn decode_unit_struct_async_<'r, 'de, 'a, R, V, C>(
    mut de: MpDecoder<'r, R>,
    visitor: V,
    cancel: C,
) -> Result<V::Value, MpError>
where
    'de: 'a,
    'r: 'a,
    R: TrBuffRead<u8> + 'r + 'a,
    V: Visitor<'de> + 'a,
    V::Value: 'a,
    C: TrCancellationToken,
{
    let tag = read_tag_async_(&mut de, cancel.child_token()).await?;
    match tag {
        0xc0 | 0x90 => visitor.visit_unit().may_cancel_with(cancel).await,
        _ => {
            de.pending_ = Some(tag);
            de.decode_any(visitor).may_cancel_with(cancel).await
        }
    }
}

#[gen_may_cancel_future(DecodeNewtypeStructOp, pub)]
async fn decode_newtype_struct_async_<'r, 'de, 'a, R, V, C>(
    de: MpDecoder<'r, R>,
    visitor: V,
    cancel: C,
) -> Result<V::Value, MpError>
where
    'de: 'a,
    'r: 'a,
    R: TrBuffRead<u8> + 'r + 'a,
    V: Visitor<'de> + 'a,
    V::Value: 'a,
    C: TrCancellationToken,
{
    visitor
        .visit_newtype_struct(de)
        .may_cancel_with(cancel)
        .await
}

#[gen_may_cancel_future(DecodeI128Op, pub)]
async fn decode_i128_async_<'r, 'de, 'a, R, V, C>(
    mut de: MpDecoder<'r, R>,
    visitor: V,
    cancel: C,
) -> Result<V::Value, MpError>
where
    'de: 'a,
    'r: 'a,
    R: TrBuffRead<u8> + 'r + 'a,
    V: Visitor<'de> + 'a,
    V::Value: 'a,
    C: TrCancellationToken,
{
    let value = read_i128_marker_async_(&mut de, cancel.child_token()).await?;
    visitor.visit_i128(value).may_cancel_with(cancel).await
}

#[gen_may_cancel_future(DecodeU128Op, pub)]
async fn decode_u128_async_<'r, 'de, 'a, R, V, C>(
    mut de: MpDecoder<'r, R>,
    visitor: V,
    cancel: C,
) -> Result<V::Value, MpError>
where
    'de: 'a,
    'r: 'a,
    R: TrBuffRead<u8> + 'r + 'a,
    V: Visitor<'de> + 'a,
    V::Value: 'a,
    C: TrCancellationToken,
{
    let value = read_i128_marker_async_(&mut de, cancel.child_token()).await? as u128;
    visitor.visit_u128(value).may_cancel_with(cancel).await
}

#[gen_may_cancel_future(DecodeEnumOp, pub)]
async fn decode_enum_async_<'r, 'de, 'a, R, V, C>(
    mut de: MpDecoder<'r, R>,
    _name: &'static str,
    _variants: &'static [&'static str],
    visitor: V,
    cancel: C,
) -> Result<V::Value, MpError>
where
    'de: 'a,
    'r: 'a,
    R: TrBuffRead<u8> + 'r + 'a,
    V: Visitor<'de> + 'a,
    V::Value: 'a,
    C: TrCancellationToken,
{
    let tag = read_tag_async_(&mut de, cancel.child_token()).await?;
    let (name, unit_) = if is_str_marker_(tag) {
        let len = read_str_len_from_tag_async_(&mut de, tag, cancel.child_token()).await?;
        let name = read_name_async_(de.reader_, len, cancel.child_token()).await?;
        (name, true)
    } else if is_map_marker_(tag) {
        let pairs = read_map_len_from_tag_async_(&mut de, tag, cancel.child_token()).await?;
        if pairs != 1 {
            return Err(MpError::new("enum map must contain exactly one variant"));
        }
        let name_tag = read_tag_async_(&mut de, cancel.child_token()).await?;
        if !is_str_marker_(name_tag) {
            return Err(MpError::new("enum variant name must be a string"));
        }
        let len = read_str_len_from_tag_async_(&mut de, name_tag, cancel.child_token()).await?;
        let name = read_name_async_(de.reader_, len, cancel.child_token()).await?;
        (name, false)
    } else {
        return Err(MpError::new("expected an enum variant string or map"));
    };
    let access = MpEnumAccess {
        name_: name,
        reader_: de.reader_,
        unit_,
    };
    visitor.visit_enum(access).may_cancel_with(cancel).await
}

#[gen_may_cancel_future(DecodeAnyOp, pub)]
async fn decode_any_async_<'r, 'de, 'a, R, V, C>(
    mut de: MpDecoder<'r, R>,
    visitor: V,
    cancel: C,
) -> Result<V::Value, MpError>
where
    'de: 'a,
    'r: 'a,
    R: TrBuffRead<u8> + 'r + 'a,
    V: Visitor<'de> + 'a,
    V::Value: 'a,
    C: TrCancellationToken,
{
    if cancel.is_cancelled() {
        return Err(MpError::new("operation cancelled"));
    }
    let tag = read_tag_async_(&mut de, cancel.child_token()).await?;
    match tag {
        0xc0 => visitor.visit_unit().may_cancel_with(cancel).await,
        0xc2 => visitor.visit_bool(false).may_cancel_with(cancel).await,
        0xc3 => visitor.visit_bool(true).may_cancel_with(cancel).await,
        0x00..=0x7f => visitor.visit_u64(tag as u64).may_cancel_with(cancel).await,
        0xe0..=0xff => {
            visitor
                .visit_i64((tag as i8) as i64)
                .may_cancel_with(cancel)
                .await
        }
        0xcc => {
            let value = read_u8_async_(&mut de, cancel.child_token()).await? as u64;
            visitor.visit_u64(value).may_cancel_with(cancel).await
        }
        0xcd => {
            let value = read_u16_async_(&mut de, cancel.child_token()).await? as u64;
            visitor.visit_u64(value).may_cancel_with(cancel).await
        }
        0xce => {
            let value = read_u32_async_(&mut de, cancel.child_token()).await? as u64;
            visitor.visit_u64(value).may_cancel_with(cancel).await
        }
        0xcf => {
            let value = read_u64_async_(&mut de, cancel.child_token()).await?;
            visitor.visit_u64(value).may_cancel_with(cancel).await
        }
        0xd0 => {
            let value = read_u8_async_(&mut de, cancel.child_token()).await? as i8 as i64;
            visitor.visit_i64(value).may_cancel_with(cancel).await
        }
        0xd1 => {
            let value = read_u16_async_(&mut de, cancel.child_token()).await? as i16 as i64;
            visitor.visit_i64(value).may_cancel_with(cancel).await
        }
        0xd2 => {
            let value = read_u32_async_(&mut de, cancel.child_token()).await? as i32 as i64;
            visitor.visit_i64(value).may_cancel_with(cancel).await
        }
        0xd3 => {
            let value = read_u64_async_(&mut de, cancel.child_token()).await? as i64;
            visitor.visit_i64(value).may_cancel_with(cancel).await
        }
        0xca => {
            let value = f32::from_bits(read_u32_async_(&mut de, cancel.child_token()).await?);
            visitor.visit_f32(value).may_cancel_with(cancel).await
        }
        0xcb => {
            let value = f64::from_bits(read_u64_async_(&mut de, cancel.child_token()).await?);
            visitor.visit_f64(value).may_cancel_with(cancel).await
        }
        0xa0..=0xbf | 0xd9..=0xdb => {
            let len = read_str_len_from_tag_async_(&mut de, tag, cancel.child_token()).await?;
            visit_str_async_(de.reader_, len, visitor, cancel).await
        }
        0xc4..=0xc6 => {
            let len = read_bin_len_from_tag_async_(&mut de, tag, cancel.child_token()).await?;
            visit_bytes_async_(de.reader_, len, visitor, cancel).await
        }
        0x90..=0x9f | 0xdc | 0xdd => {
            let len = read_array_len_from_tag_async_(&mut de, tag, cancel.child_token()).await?;
            let access = MpSeqAccess {
                reader_: de.reader_,
                remaining_: len,
            };
            visitor.visit_seq(access).may_cancel_with(cancel).await
        }
        0x80..=0x8f | 0xde | 0xdf => {
            let len = read_map_len_from_tag_async_(&mut de, tag, cancel.child_token()).await?;
            let access = MpMapAccess {
                reader_: de.reader_,
                remaining_: len,
                has_key_: false,
            };
            visitor.visit_map(access).may_cancel_with(cancel).await
        }
        _ => Err(MpError::new("unsupported MessagePack tag")),
    }
}

impl<'de, 'r, R> TrDecoder<'de> for MpDecoder<'r, R>
where
    R: TrBuffRead<u8> + 'r,
{
    type Error = MpError;

    type DecodeAnyAsync<'a, V>
        = DecodeAnyOpAsync<'r, 'de, 'a, 'a, R, V>
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
        DecodeAnyOpAsync::<'r, 'de, 'a, 'a, R, V>::new(self, visitor)
    }

    type DecodeOptionAsync<'a, V>
        = DecodeOptionOpAsync<'r, 'de, 'a, 'a, R, V>
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
        DecodeOptionOpAsync::<'r, 'de, 'a, 'a, R, V>::new(self, visitor)
    }

    type DecodeUnitStructAsync<'a, V>
        = DecodeUnitStructOpAsync<'r, 'de, 'a, 'a, R, V>
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
        DecodeUnitStructOpAsync::<'r, 'de, 'a, 'a, R, V>::new(self, visitor)
    }

    type DecodeNewtypeStructAsync<'a, V>
        = DecodeNewtypeStructOpAsync<'r, 'de, 'a, 'a, R, V>
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
        DecodeNewtypeStructOpAsync::<'r, 'de, 'a, 'a, R, V>::new(self, visitor)
    }

    type DecodeI128Async<'a, V>
        = DecodeI128OpAsync<'r, 'de, 'a, 'a, R, V>
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
        DecodeI128OpAsync::<'r, 'de, 'a, 'a, R, V>::new(self, visitor)
    }

    type DecodeU128Async<'a, V>
        = DecodeU128OpAsync<'r, 'de, 'a, 'a, R, V>
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
        DecodeU128OpAsync::<'r, 'de, 'a, 'a, R, V>::new(self, visitor)
    }

    type DecodeEnumAsync<'a, V>
        = DecodeEnumOpAsync<'r, 'de, 'a, 'a, R, V>
    where
        'de: 'a,
        'r: 'a,
        V: Visitor<'de> + 'a,
        V::Value: 'a,
        Self: 'a,
        Self::Error: 'a;

    fn decode_enum<'a, V>(
        self,
        name: &'static str,
        variants: &'static [&'static str],
        visitor: V,
    ) -> Self::DecodeEnumAsync<'a, V>
    where
        'de: 'a,
        V: Visitor<'de> + 'a,
        V::Value: 'a,
        Self: 'a,
        Self::Error: 'a,
    {
        DecodeEnumOpAsync::<'r, 'de, 'a, 'a, R, V>::new(self, name, variants, visitor)
    }

    forward_to_decode_any! {
        decode_bool => DecodeBoolAsync,
        decode_i8 => DecodeI8Async,
        decode_i16 => DecodeI16Async,
        decode_i32 => DecodeI32Async,
        decode_i64 => DecodeI64Async,
        decode_u8 => DecodeU8Async,
        decode_u16 => DecodeU16Async,
        decode_u32 => DecodeU32Async,
        decode_u64 => DecodeU64Async,
        decode_f32 => DecodeF32Async,
        decode_f64 => DecodeF64Async,
        decode_char => DecodeCharAsync,
        decode_str => DecodeStrAsync,
        decode_string => DecodeStringAsync,
        decode_bytes => DecodeBytesAsync,
        decode_byte_buf => DecodeByteBufAsync,
        decode_unit => DecodeUnitAsync,
        decode_seq => DecodeSeqAsync,
        decode_tuple => DecodeTupleAsync(len: usize),
        decode_tuple_struct => DecodeTupleStructAsync(name: &'static str, len: usize),
        decode_map => DecodeMapAsync,
        decode_struct => DecodeStructAsync(name: &'static str, fields: &'static [&'static str]),
        decode_identifier => DecodeIdentifierAsync,
        decode_ignored_any => DecodeIgnoredAnyAsync,
    }
}
