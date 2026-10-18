//! 序列化方向的组合便利方法。
//!
//! 这些方法依赖多个子 future，并且需要把取消令牌分发给每个子操作。为了
//! 不把额外的 GAT 关联类型塞进 [`TrEncoder`](super::TrEncoder)，这里用
//! 扩展 trait 提供默认实现；调用者需要 `use` 对应的扩展 trait。

use alloc::string::ToString;
use core::fmt::Display;

use abs_cancel::{TrCancellationToken, TrMayCancel};
use gen_mcf2::gen_may_cancel_future;

use super::{TrEncodable, TrEncodeMap, TrEncodeSeq, TrEncoder};

/// 计算迭代器是否提供了精确长度。
fn iterator_len_hint<I>(iter: &I) -> Option<usize>
where
    I: Iterator,
{
    let (lower, upper) = iter.size_hint();
    if upper == Some(lower) {
        Some(lower)
    } else {
        None
    }
}

// 生成的 future 会出现在容器实现（公开类型的 GAT）里，因此类型本身必须是 `pub`；
// 构造函数只在 crate 内可见。
#[gen_may_cancel_future(CollectSeqOp, pub, new(pub(crate)))]
async fn collect_seq_async<'a, S, I, C>(
    serializer: S,
    iter: I,
    cancel: C,
) -> Result<S::Ok, S::Error>
where
    S: TrEncoder + 'a,
    I: IntoIterator + 'a,
    I::Item: TrEncodable + 'a,
    S::Ok: 'a,
    S::Error: 'a,
    C: TrCancellationToken,
{
    let iter = iter.into_iter();
    let len = iterator_len_hint(&iter);
    let mut seq = serializer
        .encode_seq(len)
        .may_cancel_with(cancel.child_token())
        .await?;
    for item in iter {
        seq.encode_element(&item)
            .may_cancel_with(cancel.child_token())
            .await?;
    }
    seq.end().may_cancel_with(cancel).await
}

#[gen_may_cancel_future(CollectMapOp, pub, new(pub(crate)))]
async fn collect_map_async<'a, S, K, V, I, C>(
    serializer: S,
    iter: I,
    cancel: C,
) -> Result<S::Ok, S::Error>
where
    S: TrEncoder + 'a,
    I: IntoIterator<Item = (K, V)> + 'a,
    K: TrEncodable + 'a,
    V: TrEncodable + 'a,
    S::Ok: 'a,
    S::Error: 'a,
    C: TrCancellationToken,
{
    let iter = iter.into_iter();
    let mut map = serializer
        .encode_map(iterator_len_hint(&iter))
        .may_cancel_with(cancel.child_token())
        .await?;
    for (key, value) in iter {
        map.encode_key(&key)
            .may_cancel_with(cancel.child_token())
            .await?;
        map.encode_value(&value)
            .may_cancel_with(cancel.child_token())
            .await?;
    }
    map.end().may_cancel_with(cancel).await
}

#[gen_may_cancel_future(CollectStrOp)]
async fn collect_str_async<'a, S, T, C>(
    serializer: S,
    value: &'a T,
    cancel: C,
) -> Result<S::Ok, S::Error>
where
    S: TrEncoder + 'a,
    T: ?Sized + Display + 'a,
    S::Ok: 'a,
    S::Error: 'a,
    C: TrCancellationToken,
{
    let text = value.to_string();
    serializer.encode_str(&text).may_cancel_with(cancel).await
}

/// [`TrEncoder`](super::TrEncoder) 的组合便利方法。
pub trait TrEncoderExt: TrEncoder + Sized {
    /// 把一个迭代器序列化为序列。
    ///
    /// # Errors
    ///
    /// 序列化任一元素失败、格式不支持该序列，或取消令牌触发时返回
    /// `Self::Error`。
    fn collect_seq<'a, I>(
        self,
        iter: I,
    ) -> impl TrMayCancel<'a, MayCancelOutput = Result<Self::Ok, Self::Error>>
    where
        I: IntoIterator + 'a,
        I::Item: TrEncodable + 'a,
        Self: 'a,
        Self::Ok: 'a,
        Self::Error: 'a,
    {
        CollectSeqOpAsync::<'a, 'a, Self, I>::new(self, iter)
    }

    /// 把一个键值对迭代器序列化为映射。
    ///
    /// # Errors
    ///
    /// 序列化任一键或值失败、格式不支持该映射，或取消令牌触发时返回
    /// `Self::Error`。
    fn collect_map<'a, K, V, I>(
        self,
        iter: I,
    ) -> impl TrMayCancel<'a, MayCancelOutput = Result<Self::Ok, Self::Error>>
    where
        I: IntoIterator<Item = (K, V)> + 'a,
        K: TrEncodable + 'a,
        V: TrEncodable + 'a,
        Self: 'a,
        Self::Ok: 'a,
        Self::Error: 'a,
    {
        CollectMapOpAsync::<'a, 'a, Self, K, V, I>::new(self, iter)
    }

    /// 用一个 `Display` 值构造字符串后序列化。
    ///
    /// # Errors
    ///
    /// 序列化字符串失败或取消令牌触发时返回 `Self::Error`。
    fn collect_str<'a, T>(
        self,
        value: &'a T,
    ) -> impl TrMayCancel<'a, MayCancelOutput = Result<Self::Ok, Self::Error>>
    where
        T: ?Sized + Display + 'a,
        Self: 'a,
        Self::Ok: 'a,
        Self::Error: 'a,
    {
        CollectStrOpAsync::<'a, 'a, Self, T>::new(self, value)
    }
}

impl<S> TrEncoderExt for S where S: TrEncoder {}

#[gen_may_cancel_future(EncodeEntryOp)]
async fn encode_entry_async<'a, 'b, 'c, M, K, V, C>(
    map: &'a mut M,
    key: &'b K,
    value: &'c V,
    cancel: C,
) -> Result<(), M::Error>
where
    M: TrEncodeMap + 'a,
    K: ?Sized + TrEncodable + 'b,
    V: ?Sized + TrEncodable + 'c,
    M::Error: 'a,
    C: TrCancellationToken,
{
    map.encode_key(key)
        .may_cancel_with(cancel.child_token())
        .await?;
    map.encode_value(value).may_cancel_with(cancel).await
}

/// [`TrEncodeMap`] 的组合便利方法。
pub trait TrEncodeMapExt: TrEncodeMap + Sized {
    /// 依次序列化一个键和一个值。
    ///
    /// # Errors
    ///
    /// 序列化键或值失败，或取消令牌触发时返回 `Self::Error`。
    fn encode_entry<'a, 'b, 'c, K, V>(
        &'a mut self,
        key: &'b K,
        value: &'c V,
    ) -> impl TrMayCancel<'a, MayCancelOutput = Result<(), Self::Error>>
    where
        K: ?Sized + TrEncodable + 'b,
        V: ?Sized + TrEncodable + 'c,
        Self: 'a,
        Self::Error: 'a,
        'b: 'a,
        'c: 'a,
    {
        EncodeEntryOpAsync::<'a, 'b, 'c, 'a, Self, K, V>::new(self, key, value)
    }
}

impl<M> TrEncodeMapExt for M where M: TrEncodeMap {}
