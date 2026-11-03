//! 反序列化方向的组合便利方法。
//!
//! 这些方法大多直接复用 `*_seed` 方法，只有需要串联两个异步步骤的
//! `next_entry` 使用 `gen_mcf2` 生成的 helper future。

use core::marker::PhantomData;

use abs_cancel::{TrCancellationToken, TrMayCancel};
use gen_mcf2::gen_may_cancel_future;

use super::{EnumAccess, MapAccess, SeqAccess, TrArena, TrDecodable, VariantAccess};

/// [`SeqAccess`] 的便利方法。
pub trait SeqAccessExt<'alloc, Alloc: TrArena<'alloc> + 'alloc>:
    SeqAccess<'alloc, Alloc> + Sized
{
    /// 用 `T` 的 `TrDecodable` 实现读取下一个元素。
    ///
    /// # Errors
    ///
    /// 元素不合法或底层读取失败时返回 `Self::Error`。
    fn next_element<'a, T>(
        &'a mut self,
    ) -> impl TrMayCancel<'a, MayCancelOutput = Result<Option<T>, Self::Error>>
    where
        'alloc: 'a,
        T: TrDecodable<'alloc, Alloc> + 'a,
        Self: 'a,
        Self::Error: 'a,
    {
        self.next_element_seed(PhantomData::<T>)
    }
}

impl<'alloc, Alloc, A> SeqAccessExt<'alloc, Alloc> for A
where
    Alloc: TrArena<'alloc> + 'alloc,
    A: SeqAccess<'alloc, Alloc>,
{
}

#[gen_may_cancel_future(NextEntry)]
async fn next_entry_async<'alloc, 'a, Alloc, M, K, V, C>(
    map: &'a mut M,
    cancel: C,
) -> Result<Option<(K, V)>, M::Error>
where
    'alloc: 'a,
    Alloc: TrArena<'alloc> + 'alloc,
    M: MapAccess<'alloc, Alloc> + 'a,
    K: TrDecodable<'alloc, Alloc> + 'a,
    V: TrDecodable<'alloc, Alloc> + 'a,
    M::Error: 'a,
    C: TrCancellationToken,
{
    let Some(key) = map
        .next_key_seed(PhantomData::<K>)
        .may_cancel_with(cancel.child_token())
        .await?
    else {
        return Ok(None);
    };
    let value = map
        .next_value_seed(PhantomData::<V>)
        .may_cancel_with(cancel)
        .await?;
    Ok(Some((key, value)))
}

/// [`MapAccess`] 的便利方法。
pub trait MapAccessExt<'alloc, Alloc: TrArena<'alloc> + 'alloc>:
    MapAccess<'alloc, Alloc> + Sized
{
    /// 读取下一个键。
    ///
    /// # Errors
    ///
    /// 键不合法或底层读取失败时返回 `Self::Error`。
    fn next_key<'a, K>(
        &'a mut self,
    ) -> impl TrMayCancel<'a, MayCancelOutput = Result<Option<K>, Self::Error>>
    where
        'alloc: 'a,
        K: TrDecodable<'alloc, Alloc> + 'a,
        Self: 'a,
        Self::Error: 'a,
    {
        self.next_key_seed(PhantomData::<K>)
    }

    /// 读取当前键对应的值。
    ///
    /// # Errors
    ///
    /// 值不合法、没有对应键或底层读取失败时返回 `Self::Error`。
    fn next_value<'a, V>(
        &'a mut self,
    ) -> impl TrMayCancel<'a, MayCancelOutput = Result<V, Self::Error>>
    where
        'alloc: 'a,
        V: TrDecodable<'alloc, Alloc> + 'a,
        Self: 'a,
        Self::Error: 'a,
    {
        self.next_value_seed(PhantomData::<V>)
    }

    /// 读取下一个键值对。
    ///
    /// # Errors
    ///
    /// 键或值不合法、映射提前结束或底层读取失败时返回 `Self::Error`。
    fn next_entry<'a, K, V>(
        &'a mut self,
    ) -> impl TrMayCancel<'a, MayCancelOutput = Result<Option<(K, V)>, Self::Error>>
    where
        'alloc: 'a,
        K: TrDecodable<'alloc, Alloc> + 'a,
        V: TrDecodable<'alloc, Alloc> + 'a,
        Self: 'a,
        Self::Error: 'a,
    {
        NextEntryAsync::<'alloc, 'a, 'a, Alloc, Self, K, V>::new(self)
    }
}

impl<'alloc, Alloc, M> MapAccessExt<'alloc, Alloc> for M
where
    Alloc: TrArena<'alloc> + 'alloc,
    M: MapAccess<'alloc, Alloc>,
{
}

/// [`EnumAccess`] 的便利方法。
pub trait EnumAccessExt<'alloc, Alloc: TrArena<'alloc> + 'alloc>:
    EnumAccess<'alloc, Alloc> + Sized
{
    /// 用 `V` 的 `TrDecodable` 实现读取变体名。
    ///
    /// # Errors
    ///
    /// 变体名不合法或底层读取失败时返回 `Self::Error`。
    fn variant<'a, V>(
        self,
    ) -> impl TrMayCancel<'a, MayCancelOutput = Result<(V, Self::Variant), Self::Error>>
    where
        'alloc: 'a,
        V: TrDecodable<'alloc, Alloc> + 'a,
        Self: 'a,
        Self::Variant: 'a,
        Self::Error: 'a,
    {
        self.variant_seed(PhantomData::<V>)
    }
}

impl<'alloc, Alloc, E> EnumAccessExt<'alloc, Alloc> for E
where
    Alloc: TrArena<'alloc> + 'alloc,
    E: EnumAccess<'alloc, Alloc>,
{
}

/// [`VariantAccess`] 的便利方法。
pub trait VariantAccessExt<'alloc, Alloc: TrArena<'alloc> + 'alloc>:
    VariantAccess<'alloc, Alloc> + Sized
{
    /// 用 `T` 的 `TrDecodable` 实现读取 newtype 变体的内容。
    ///
    /// # Errors
    ///
    /// 变体形状不符或内容不合法时返回 `Self::Error`。
    fn newtype_variant<'a, T>(
        self,
    ) -> impl TrMayCancel<'a, MayCancelOutput = Result<T, Self::Error>>
    where
        'alloc: 'a,
        T: TrDecodable<'alloc, Alloc> + 'a,
        Self: 'a,
        Self::Error: 'a,
    {
        self.newtype_variant_seed(PhantomData::<T>)
    }
}

impl<'alloc, Alloc, V> VariantAccessExt<'alloc, Alloc> for V
where
    Alloc: TrArena<'alloc> + 'alloc,
    V: VariantAccess<'alloc, Alloc>,
{
}
