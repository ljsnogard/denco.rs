//! 解码结果的 arena 分配器抽象。

use core::{convert::Infallible, future};

use abs_cancel::TrMayCancel;

/// 解码器可选的 arena 分配器。
///
/// `abs_denco` 不绑定具体分配器：bumpalo、sw-scope 或「不分配」都由实现方
/// 提供。`'alloc` 是 arena 的借用寿命，[`TrArena::Ref`] 决定分配后的形态。
///
/// 放入操作是**异步且可失败**的：sw-scope 的域锁是协作式异步锁，只有
/// `put_async` / `emplace_*_async` 这类异步入口；同步分配器（bumpalo、
/// [`NoArena`]）用 `core::future::ready` 返回立即完成的 future。
///
/// # 形态差异
///
/// | 实现 | `Ref<T>` |
/// | --- | --- |
/// | bumpalo | `&'alloc mut T` |
/// | sw-scope | `Owning<'alloc, T>` |
/// | [`NoArena`] | `T` |
///
/// 引用形态不同是两类 arena 的关键差异；`'alloc` 只描述寿命，不区分类别。
///
/// # Examples
///
/// ```ignore
/// impl<'alloc> TrArena<'alloc> for MyArena {
///     type Error = MyArenaError;
///     type Ref<T: 'alloc> = &'alloc mut T;
///     type AllocValueAsync<'a, T>
///         = core::future::Ready<Result<&'alloc mut T, MyArenaError>>
///     where
///         'alloc: 'a,
///         T: 'alloc + 'a,
///         Self: 'a;
///
///     fn alloc_value_async<'a, T>(&'alloc self, value: T) -> Self::AllocValueAsync<'a, T>
///     where
///         'alloc: 'a,
///         T: 'alloc + 'a,
///         Self: 'a,
///     {
///         // 具体 arena 的同步分配逻辑
///         # todo!()
///     }
/// }
/// ```
pub trait TrArena<'alloc> {
    /// 分配失败时产生的错误类型。
    type Error: core::error::Error;

    /// 值 `T` 分配后的引用/句柄形态。
    type Ref<T: 'alloc>: 'alloc
    where
        Self: 'alloc;

    /// 把值放入 arena 时产生的可取消 future。
    type AllocValueAsync<'a, T>: TrMayCancel<'a, MayCancelOutput = Result<Self::Ref<T>, Self::Error>>
    where
        'alloc: 'a,
        T: 'alloc + 'a,
        Self: 'alloc + 'a;

    /// 把 `value` 放进 arena，返回借用 arena 的引用/句柄。
    fn alloc_value_async<'a, T>(&'alloc self, value: T) -> Self::AllocValueAsync<'a, T>
    where
        'alloc: 'a,
        T: 'alloc + 'a,
        Self: 'alloc + 'a;
}

/// 不使用 arena 的解码器所报告的分配器。
///
/// 它不保存任何状态，[`TrArena::Ref`] 就是 `T` 本身；`alloc_value_async`
/// 立即完成。值版 [`TrDecodable`](crate::de::TrDecodable) 实现在没有 arena 的
/// 格式下使用它；`&'alloc mut T` 的引用版要求 `Ref<T> = &'alloc mut T`，
/// 因此不会匹配本类型。
///
/// # Examples
///
/// ```
/// use abs_cancel::{NonCancellableToken, TrMayCancel};
/// use abs_denco::de::{NoArena, TrArena};
///
/// let alloc = NoArena;
/// let value = alloc
///     .alloc_value_async(42_u32)
///     .may_cancel_with(NonCancellableToken::new());
/// ```
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct NoArena;

impl<'alloc> TrArena<'alloc> for NoArena {
    type Error = Infallible;

    type Ref<T: 'alloc>
        = T
    where
        Self: 'alloc;

    type AllocValueAsync<'a, T>
        = future::Ready<Result<T, Infallible>>
    where
        'alloc: 'a,
        T: 'alloc + 'a,
        Self: 'alloc + 'a;

    fn alloc_value_async<'a, T>(&'alloc self, value: T) -> Self::AllocValueAsync<'a, T>
    where
        'alloc: 'a,
        T: 'alloc + 'a,
        Self: 'a,
    {
        future::ready(Ok(value))
    }
}
