//! bumpalo 的独占句柄 `BumpBox`，以及 `TrDecodable` 的引用版实现。

use core::ops::{Deref, DerefMut};

use abs_cancel::{TrCancellationToken, TrMayCancel};
use abs_denco::{
    TrBoxed, TrUnique,
    de::{Error, TrArena, TrDecodable, TrDecoder},
};
use bumpalo::boxed::Box as BumpaloBox;
use gen_mcf2::gen_may_cancel_future;

use crate::arena_::BumpArena;

/// bumpalo 特化版的独占智能指针，作为 [`BumpArena`] 的 `Ref<T>`。
///
/// 内部是 [`bumpalo::boxed::Box`]：内存属于 arena，句柄被 drop 时运行 `T` 的
/// 析构函数但不归还内存。它实现 [`Deref`] / [`DerefMut`]，并适配
/// [`TrBoxed`] / [`TrUnique`]。
pub struct BumpBox<'alloc, T: ?Sized>(BumpaloBox<'alloc, T>);

impl<'alloc, T: ?Sized> BumpBox<'alloc, T> {
    /// 从 arena 分配出的裸指针接管所有权。
    pub(crate) fn from_raw_(raw: *mut T) -> Self {
        // SAFETY: 指针来自 `BumpArena` 的 `allocate`，按 `T` 的对齐与大小划定且
        // 只写入一次；之后由 `BumpBox` 独占。释放由 arena 整体负责，
        // `bumpalo::boxed::Box` 的 Drop 只运行 `T` 的析构。
        Self(unsafe { BumpaloBox::from_raw(raw) })
    }

    /// 取出裸指针，调用方负责之后不再使用本句柄。
    pub fn into_raw(self) -> *mut T {
        BumpaloBox::into_raw(self.0)
    }
}

impl<T: ?Sized> Deref for BumpBox<'_, T> {
    type Target = T;

    fn deref(&self) -> &T {
        &self.0
    }
}

impl<T: ?Sized> DerefMut for BumpBox<'_, T> {
    fn deref_mut(&mut self) -> &mut T {
        &mut self.0
    }
}

impl<T: ?Sized> TrBoxed for BumpBox<'_, T> {
    type Item = T;
}

impl<T: ?Sized> TrUnique for BumpBox<'_, T> {}

/// 把值版解码结果放进 bump arena，返回 [`BumpBox`] 句柄。
#[gen_may_cancel_future(DecodeBumpBoxOp, pub, new(pub))]
async fn decode_bump_box_async_<'alloc, 'a, T, D, C>(
    de: D,
    cancel: C,
) -> Result<BumpBox<'alloc, T>, D::Error>
where
    'alloc: 'a,
    T: TrDecodable<'alloc, BumpArena> + 'alloc,
    D: TrDecoder<'alloc, BumpArena> + 'a,
    D::Error: Error + 'a,
    C: TrCancellationToken,
{
    let arena = de.allocator();
    let value = T::decode_async(de)
        .may_cancel_with(cancel.child_token())
        .await?;
    arena
        .alloc_value_async(value)
        .may_cancel_with(cancel)
        .await
        .map_err(<D::Error as Error>::custom)
}

impl<'alloc, T> TrDecodable<'alloc, BumpArena> for BumpBox<'alloc, T>
where
    T: TrDecodable<'alloc, BumpArena> + 'alloc,
{
    type DecodeAsync<'a, D>
        = DecodeBumpBoxOpAsync<'alloc, 'a, 'a, T, D>
    where
        'alloc: 'a,
        D: TrDecoder<'alloc, BumpArena> + 'a,
        D::Error: 'a,
        Self: 'a;

    fn decode_async<'a, D>(de: D) -> Self::DecodeAsync<'a, D>
    where
        'alloc: 'a,
        D: TrDecoder<'alloc, BumpArena> + 'a,
        D::Error: 'a,
        Self: 'a,
    {
        DecodeBumpBoxOpAsync::new(de)
    }
}
