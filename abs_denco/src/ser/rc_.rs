//! `alloc::rc::Rc` 的透明转发编码实现。
//!
//! 由 `rc` feature 控制；`Arc` 的同类实现保持无条件提供，见 `impls_.rs`。

use alloc::rc::Rc;

use super::{TrEncodable, TrEncoder};

impl<T> TrEncodable for Rc<T>
where
    T: ?Sized + TrEncodable,
{
    type EncodeAsync<'a, S>
        = T::EncodeAsync<'a, S>
    where
        Self: 'a,
        T: 'a,
        S: TrEncoder + 'a,
        S::Ok: 'a,
        S::Error: 'a;

    fn encode_async<'a, S>(&'a self, encoder: S) -> Self::EncodeAsync<'a, S>
    where
        Self: 'a,
        T: 'a,
        S: TrEncoder + 'a,
        S::Ok: 'a,
        S::Error: 'a,
    {
        T::encode_async(self.as_ref(), encoder)
    }
}
