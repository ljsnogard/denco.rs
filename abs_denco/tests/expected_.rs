//! `Expected` 适配入口（`expected` / `Visitor::expected`）的集成测试。
//!
//! Integration tests for the `Expected` adapters.

use core::fmt;

use abs_denco::de::{Error, Expected, NoArena, TrArena, Unexpected, Visitor, expected};

/// 一个描述「期望数字」的 visitor。
struct NumberVisitor;

impl<'de, 'alloc, Alloc> Visitor<'de, 'alloc, Alloc> for NumberVisitor
where
    Alloc: TrArena<'alloc> + 'alloc,
{
    type Value = ();

    fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("a number")
    }
}

/// 只用于断言错误文本的错误类型。
#[derive(Debug)]
struct TestError(String);

impl fmt::Display for TestError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for TestError {}

impl Error for TestError {
    fn custom<T: fmt::Display>(msg: T) -> Self {
        Self(msg.to_string())
    }
}

/// 通过 `Display for dyn Expected` 读出适配器的描述。
fn render(exp: &dyn Expected) -> String {
    format!("{exp}")
}

/// 模拟 visitor impl 内部（`Alloc` 是 impl 参数）的调用点。
trait Probe<'de, 'alloc, Alloc> {
    fn probe(&self) -> String;
}

impl<'de, 'alloc, Alloc> Probe<'de, 'alloc, Alloc> for NumberVisitor
where
    Alloc: TrArena<'alloc> + 'alloc,
{
    fn probe(&self) -> String {
        // `self.expected()` 在 impl 内无法推断 Alloc，因此用 turbofish。
        render(&expected::<Alloc, _>(self))
    }
}

fn via_free<'de, 'alloc, Alloc>(visitor: &NumberVisitor) -> String
where
    Alloc: TrArena<'alloc> + 'alloc,
    NumberVisitor: Visitor<'de, 'alloc, Alloc>,
{
    render(&expected(visitor))
}

fn via_method<'de, 'alloc, Alloc>(visitor: &NumberVisitor) -> String
where
    Alloc: TrArena<'alloc> + 'alloc,
    NumberVisitor: Visitor<'de, 'alloc, Alloc>,
{
    render(&visitor.expected())
}

fn invalid_via_free<'de, 'alloc, Alloc>() -> String
where
    Alloc: TrArena<'alloc> + 'alloc,
    NumberVisitor: Visitor<'de, 'alloc, Alloc>,
{
    TestError::invalid_type(Unexpected::Bool(true), &expected(&NumberVisitor)).to_string()
}

fn invalid_via_method<'de, 'alloc, Alloc>() -> String
where
    Alloc: TrArena<'alloc> + 'alloc,
    NumberVisitor: Visitor<'de, 'alloc, Alloc>,
{
    TestError::invalid_type(Unexpected::Bool(true), &NumberVisitor.expected()).to_string()
}

/// 测试目标：自由函数版在泛型 helper 中可推断并复用 visitor 的 `expecting`。
/// Test goal: the free-function adapter infers the visitor parameters in a
/// generic helper and reuses the visitor's `expecting` text.
///
/// - 手段：在带 `Visitor` 约束的 helper 里调用 `expected(&v)`，通过
///   `Display for dyn Expected` 读出描述。
///   / How: call `expected(&v)` inside a helper with a `Visitor` bound and read
///   the description through `Display for dyn Expected`.
/// - 判断：输出与 `expecting` 写下的文本一致。
///   / What: the output equals the text written by `expecting`.
#[test]
fn expected_free_fn_in_generic_helper_() {
    assert_eq!(via_free::<NoArena>(&NumberVisitor), "a number");
}

/// 测试目标：`Visitor::expected` 在泛型 helper 中可推断。
/// Test goal: `Visitor::expected` infers parameters in a generic helper.
///
/// - 手段：在带 `Visitor` 约束的 helper 里调用 `v.expected()`。
///   / How: call `v.expected()` inside a helper with a `Visitor` bound.
/// - 判断：输出与 `expecting` 写下的文本一致。
///   / What: the output equals the text written by `expecting`.
#[test]
fn expected_method_in_generic_helper_() {
    assert_eq!(via_method::<NoArena>(&NumberVisitor), "a number");
}

/// 测试目标：visitor impl 内部可用 turbofish 指定 `Alloc`。
/// Test goal: inside a visitor-impl-like context, a turbofish can pin `Alloc`.
///
/// - 手段：在 `Probe` 的泛型 impl 里调用 `expected::<Alloc, _>(&self)`。
///   / How: call `expected::<Alloc, _>(&self)` inside a generic `Probe` impl.
/// - 判断：输出与 `expecting` 写下的文本一致，证明文档里的 impl 内写法可用。
///   / What: the output equals the `expecting` text, proving the documented
///   in-impl form works.
#[test]
fn expected_with_turbofish_in_impl_() {
    let value = <NumberVisitor as Probe<'_, '_, NoArena>>::probe(&NumberVisitor);
    assert_eq!(value, "a number");
}

/// 测试目标：两种适配器都能作为 `Error::invalid_type` 的 `Expected` 参数。
/// Test goal: both adapters work as the `Expected` argument of
/// `Error::invalid_type`.
///
/// - 手段：分别用 `expected(&v)` 与 `v.expected()` 构造 `TestError`。
///   / How: build `TestError` with `expected(&v)` and `v.expected()`.
/// - 判断：两条错误文本都包含 visitor 的 `expecting` 描述。
///   / What: both messages contain the visitor's `expecting` description.
#[test]
fn invalid_type_accepts_adapters_() {
    let from_free = invalid_via_free::<NoArena>();
    let from_method = invalid_via_method::<NoArena>();
    assert_eq!(from_free, "invalid type: boolean `true`, expected a number");
    assert_eq!(from_free, from_method);
}
