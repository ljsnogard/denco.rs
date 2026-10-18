//! 可序列化数据结构的抽象。

use abs_cancel::TrMayCancel;

use super::TrEncoder;

/// 可以被任意数据格式序列化的数据结构。
///
/// 本 trait 是同步版 `serde::Serialize` 的异步版本：`encode_async` 返回一个
/// GAT 关联类型 [`TrEncodable::EncodeAsync`]，它实现 [`TrMayCancel`]。
/// 调用者可以先决定取消令牌，再 `.await` 取得 `Result<S::Ok, S::Error>`。
///
/// # 生命周期
///
/// 返回 future 的取消生命周期 `'a` 由 `&'a self` 绑定。实现者应保证
/// `Self: 'a`，并在内部把取消令牌继续传给嵌套的序列化操作。
pub trait TrEncodable {
    /// 序列化本值时产生的可取消 future。
    ///
    /// 具体类型由实现者决定；实现者可以在该关联类型上附加 `Send` 等约束。
    type EncodeAsync<'a, S>: TrMayCancel<'a, MayCancelOutput = Result<S::Ok, S::Error>>
    where
        Self: 'a,
        S: TrEncoder + 'a,
        S::Ok: 'a,
        S::Error: 'a;

    /// 把本值序列化到给定的 `serializer`。
    ///
    /// # Errors
    ///
    /// 当数据格式拒绝该值、底层 I/O 失败，或实现者主动响应取消信号时，
    /// 返回 `S::Error`。
    fn encode_async<'a, S>(&'a self, encoder: S) -> Self::EncodeAsync<'a, S>
    where
        S: TrEncoder + 'a,
        S::Ok: 'a,
        S::Error: 'a,
        Self: 'a;
}
