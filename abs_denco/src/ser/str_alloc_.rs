//! `abs_str::str_alloc::String` 的编码实现（`str_alloc` feature）。
//!
//! 与标准库 `String` 一样按 `str` 编码；allocator 只影响内存来源，不影响
//! 线上格式。

use core::{alloc::Allocator, ops::Deref};

use abs_str::str_alloc::String as AllocString;

use super::{TrEncodable, TrEncoder};

impl<A: Allocator> TrEncodable for AllocString<A> {
    type EncodeAsync<'a, S>
        = S::EncodeStrAsync<'a, 'a>
    where
        Self: 'a,
        S: TrEncoder + 'a,
        S::Ok: 'a,
        S::Error: 'a;

    fn encode_async<'a, S>(&'a self, encoder: S) -> Self::EncodeAsync<'a, S>
    where
        Self: 'a,
        S: TrEncoder + 'a,
        S::Ok: 'a,
        S::Error: 'a,
    {
        encoder.encode_str(self.deref())
    }
}
