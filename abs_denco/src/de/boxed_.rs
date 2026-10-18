//! 装箱的可取消 future。

use alloc::boxed::Box;
use core::{
    future::Future,
    pin::Pin,
    task::{Context, Poll},
};

use abs_cancel::{TrCancellationToken, TrMayCancel};

/// 一个装箱的 future，用于打破递归的 future 类型。
///
/// 典型场景是 `IgnoredAny`：它丢弃任意嵌套值，递归的 `TrDecodable`
/// future 类型无法直接在类型系统中展开，必须装箱。
///
/// 当前实现**忽略取消令牌**：调用 [`TrMayCancel::may_cancel_with`] 会原样
/// 返回自身。`IgnoredAny` 只用于丢弃数据，这个折中是明确的限制。
pub struct BoxedMayCancel<'a, T> {
    future_: Pin<Box<dyn Future<Output = T> + 'a>>,
}

impl<'a, T> BoxedMayCancel<'a, T> {
    /// 把 `future` 装箱。
    pub fn new<F>(future: F) -> Self
    where
        F: Future<Output = T> + 'a,
    {
        Self {
            future_: Box::pin(future),
        }
    }
}

impl<T> Future for BoxedMayCancel<'_, T> {
    type Output = T;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        self.get_mut().future_.as_mut().poll(cx)
    }
}

impl<'a, T: 'a> TrMayCancel<'a> for BoxedMayCancel<'a, T> {
    type MayCancelFuture<'f, C>
        = Self
    where
        'f: 'a,
        Self: 'f,
        C: 'f + TrCancellationToken;

    type MayCancelOutput = T;

    fn may_cancel_with<C>(self, _cancel: C) -> Self::MayCancelFuture<'a, C>
    where
        C: 'a + TrCancellationToken,
    {
        self
    }
}
