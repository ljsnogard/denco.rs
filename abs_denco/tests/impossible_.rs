//! `Impossible` 序列化器的测试。

use core::{
    fmt,
    future::Future,
    task::{Context, Poll},
};

use std::{pin::pin, task::Waker};

use abs_cancel::{NonCancellableToken, TrMayCancel};
use abs_denco::ser::{self, Impossible, TrEncoder};

/// 测试用错误类型。
#[derive(Debug)]
struct TestError;

impl fmt::Display for TestError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("test error")
    }
}

impl core::error::Error for TestError {}

impl ser::Error for TestError {
    fn custom<T>(_msg: T) -> Self
    where
        T: fmt::Display,
    {
        TestError
    }
}

/// 立即完成的测试执行器。
fn block_on<F: Future>(future: F) -> F::Output {
    let mut cx = Context::from_waker(Waker::noop());
    let mut future = pin!(future);
    loop {
        match future.as_mut().poll(&mut cx) {
            Poll::Ready(output) => return output,
            Poll::Pending => std::thread::yield_now(),
        }
    }
}

/// 验证 `Impossible` 的标量序列化总是失败。
/// Verify that scalar serialization on `Impossible` always fails.
///
/// - 手段 / Method: 调用 `encode_bool(true)` 并驱动 future。
/// - 判断 / Assert: 结果为 `Err`。
#[test]
fn impossible_scalar_is_error() {
    let out: Result<(), TestError> = block_on(async {
        Impossible::<(), TestError>::new()
            .encode_bool(true)
            .may_cancel_with(NonCancellableToken::new())
            .await
    });
    assert!(out.is_err());
}

/// 验证 `Impossible` 的复合值序列化总是失败。
/// Verify that composite serialization on `Impossible` always fails.
///
/// - 手段 / Method: 调用 `encode_seq(Some(0))` 并驱动 future。
/// - 判断 / Assert: 结果为 `Err`。
#[test]
fn impossible_composite_is_error() {
    let out: Result<Impossible<(), TestError>, TestError> = block_on(async {
        Impossible::<(), TestError>::new()
            .encode_seq(Some(0))
            .may_cancel_with(NonCancellableToken::new())
            .await
    });
    assert!(out.is_err());
}

/// 验证 `Impossible` 的状态机 `end` 也返回错误。
/// Verify that `end` on an `Impossible` state machine also returns an error.
///
/// - 手段 / Method: 直接调用 `TrEncodeSeq::end`。
/// - 判断 / Assert: 结果为 `Err`。
#[test]
fn impossible_end_is_error() {
    use abs_denco::ser::TrEncodeSeq;

    let out: Result<(), TestError> = block_on(async {
        Impossible::<(), TestError>::new()
            .end()
            .may_cancel_with(NonCancellableToken::new())
            .await
    });
    assert!(out.is_err());
}
