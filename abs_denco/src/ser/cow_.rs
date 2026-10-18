//! `alloc::borrow::Cow` 的编码实现。
//!
//! 由 `cow` feature 控制。编码与解码都不区分 `Borrowed` / `Owned`：
//! 编码按内部借用值转发，解码一律产出 `Owned`。

use alloc::borrow::{Cow, ToOwned};

use super::{TrEncodable, TrEncoder};

impl<'a, T> TrEncodable for Cow<'a, T>
where
    T: ?Sized + ToOwned + TrEncodable,
{
    type EncodeAsync<'x, S>
        = T::EncodeAsync<'x, S>
    where
        Self: 'x,
        T: 'x,
        S: TrEncoder + 'x,
        S::Ok: 'x,
        S::Error: 'x;

    fn encode_async<'x, S>(&'x self, encoder: S) -> Self::EncodeAsync<'x, S>
    where
        Self: 'x,
        T: 'x,
        S: TrEncoder + 'x,
        S::Ok: 'x,
        S::Error: 'x,
    {
        T::encode_async(&**self, encoder)
    }
}
