//! `Visitor` 字符串/字符转发默认实现的测试。
//!
//! 这些测试验证 `visit_string`、`visit_byte_buf`、`visit_char` 的默认实现
//! 会通过 `gen_mcf2` 生成的 helper future 转发到 `visit_str` / `visit_bytes`。

use core::{
    fmt,
    future::{self, Future},
    task::{Context, Poll},
};

use std::{pin::pin, task::Waker};

use abs_cancel::{NonCancellableToken, TrMayCancel};
use abs_denco::de::{self, Visitor};

/// 测试用错误类型。
#[derive(Debug, PartialEq)]
struct TestError;

impl fmt::Display for TestError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("test error")
    }
}

impl core::error::Error for TestError {}

impl de::Error for TestError {
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

/// 只实现借用字符串/字节方法的访问者，用来验证默认转发。
struct LenVisitor;

impl<'de> Visitor<'de> for LenVisitor {
    type Value = usize;

    fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("a string or byte slice")
    }

    fn visit_str<'a, E>(
        self,
        v: &'a str,
    ) -> impl TrMayCancel<'a, MayCancelOutput = Result<Self::Value, E>>
    where
        Self: 'a,
        Self::Value: 'a,
        E: de::Error + 'a,
    {
        future::ready(Ok(v.len()))
    }

    fn visit_bytes<'a, E>(
        self,
        v: &'a [u8],
    ) -> impl TrMayCancel<'a, MayCancelOutput = Result<Self::Value, E>>
    where
        Self: 'a,
        Self::Value: 'a,
        E: de::Error + 'a,
    {
        future::ready(Ok(v.len()))
    }
}

/// 验证 `visit_string` 默认转发到 `visit_str`。
/// Verify that `visit_string` forwards to `visit_str` by default.
///
/// - 手段 / Method: 只实现 `visit_str` 的 `LenVisitor` 接收一个 `String`。
/// - 判断 / Assert: 返回字符串长度 4。
#[test]
fn visit_string_forwards_to_visit_str() {
    let out: Result<usize, TestError> = block_on(async {
        LenVisitor
            .visit_string(String::from("abcd"))
            .may_cancel_with(NonCancellableToken::new())
            .await
    });
    assert_eq!(out, Ok(4));
}

/// 验证 `visit_byte_buf` 默认转发到 `visit_bytes`。
/// Verify that `visit_byte_buf` forwards to `visit_bytes` by default.
///
/// - 手段 / Method: 只实现 `visit_bytes` 的 `LenVisitor` 接收 `Vec<u8>`。
/// - 判断 / Assert: 返回字节长度 3。
#[test]
fn visit_byte_buf_forwards_to_visit_bytes() {
    let out: Result<usize, TestError> = block_on(async {
        LenVisitor
            .visit_byte_buf(vec![1, 2, 3])
            .may_cancel_with(NonCancellableToken::new())
            .await
    });
    assert_eq!(out, Ok(3));
}

/// 验证 `visit_char` 默认转发到 `visit_str`。
/// Verify that `visit_char` forwards to `visit_str` by default.
///
/// - 手段 / Method: 只实现 `visit_str` 的 `LenVisitor` 接收 `'é'`。
/// - 判断 / Assert: 返回 UTF-8 字节长度 2。
#[test]
fn visit_char_forwards_to_visit_str() {
    let out: Result<usize, TestError> = block_on(async {
        LenVisitor
            .visit_char('é')
            .may_cancel_with(NonCancellableToken::new())
            .await
    });
    assert_eq!(out, Ok(2));
}
