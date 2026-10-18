//! 反序列化方向的组合便利方法。
//!
//! 这些方法大多直接复用 `*_seed` 方法，只有需要串联两个异步步骤的
//! `next_entry` 使用 `gen_mcf2` 生成的 helper future。

use core::marker::PhantomData;

use abs_cancel::{TrCancellationToken, TrMayCancel};
use gen_mcf2::gen_may_cancel_future;

use super::{EnumAccess, MapAccess, SeqAccess, TrDecodable, VariantAccess};

/// [`SeqAccess`] 的便利方法。
pub trait SeqAccessExt<'de>: SeqAccess<'de> + Sized {
    /// 用 `T` 的 `TrDecodable` 实现读取下一个元素。
    ///
    /// # Errors
    ///
    /// 元素不合法或底层读取失败时返回 `Self::Error`。
    fn next_element<'a, T>(
        &'a mut self,
    ) -> impl TrMayCancel<'a, MayCancelOutput = Result<Option<T>, Self::Error>>
    where
        'de: 'a,
        T: TrDecodable<'de> + 'a,
        Self: 'a,
        Self::Error: 'a,
    {
        self.next_element_seed(PhantomData::<T>)
    }
}

impl<'de, A> SeqAccessExt<'de> for A where A: SeqAccess<'de> {}

#[gen_may_cancel_future(NextEntry)]
async fn next_entry_async<'de, 'a, M, K, V, C>(
    map: &'a mut M,
    cancel: C,
) -> Result<Option<(K, V)>, M::Error>
where
    'de: 'a,
    M: MapAccess<'de> + 'a,
    K: TrDecodable<'de> + 'a,
    V: TrDecodable<'de> + 'a,
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
pub trait MapAccessExt<'de>: MapAccess<'de> + Sized {
    /// 读取下一个键。
    ///
    /// # Errors
    ///
    /// 键不合法或底层读取失败时返回 `Self::Error`。
    fn next_key<'a, K>(
        &'a mut self,
    ) -> impl TrMayCancel<'a, MayCancelOutput = Result<Option<K>, Self::Error>>
    where
        'de: 'a,
        K: TrDecodable<'de> + 'a,
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
        'de: 'a,
        V: TrDecodable<'de> + 'a,
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
        'de: 'a,
        K: TrDecodable<'de> + 'a,
        V: TrDecodable<'de> + 'a,
        Self: 'a,
        Self::Error: 'a,
    {
        NextEntryAsync::<'de, 'a, 'a, Self, K, V>::new(self)
    }
}

impl<'de, M> MapAccessExt<'de> for M where M: MapAccess<'de> {}

/// [`EnumAccess`] 的便利方法。
pub trait EnumAccessExt<'de>: EnumAccess<'de> + Sized {
    /// 用 `V` 的 `TrDecodable` 实现读取变体名。
    ///
    /// # Errors
    ///
    /// 变体名不合法或底层读取失败时返回 `Self::Error`。
    fn variant<'a, V>(
        self,
    ) -> impl TrMayCancel<'a, MayCancelOutput = Result<(V, Self::Variant), Self::Error>>
    where
        'de: 'a,
        V: TrDecodable<'de> + 'a,
        Self: 'a,
        Self::Variant: 'a,
        Self::Error: 'a,
    {
        self.variant_seed(PhantomData::<V>)
    }
}

impl<'de, E> EnumAccessExt<'de> for E where E: EnumAccess<'de> {}

/// [`VariantAccess`] 的便利方法。
pub trait VariantAccessExt<'de>: VariantAccess<'de> + Sized {
    /// 用 `T` 的 `TrDecodable` 实现读取 newtype 变体的内容。
    ///
    /// # Errors
    ///
    /// 变体形状不符或内容不合法时返回 `Self::Error`。
    fn newtype_variant<'a, T>(
        self,
    ) -> impl TrMayCancel<'a, MayCancelOutput = Result<T, Self::Error>>
    where
        'de: 'a,
        T: TrDecodable<'de> + 'a,
        Self: 'a,
        Self::Error: 'a,
    {
        self.newtype_variant_seed(PhantomData::<T>)
    }
}

impl<'de, V> VariantAccessExt<'de> for V where V: VariantAccess<'de> {}
