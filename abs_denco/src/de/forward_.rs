//! 把 `decode_*` 转发到 `decode_any` 的宏。

/// 在 [`crate::de::TrDecoder`] 的 `impl` 中，把一组
/// `decode_*` 方法连同它们的 GAT 关联类型一起转发到
/// `decode_any`。
///
/// 宏的参数是「方法名 => future 关联类型名」，可选地在后面用括号给出该方法
/// 除 `visitor` 之外的参数：
///
/// ```ignore
/// impl<'de> TrDecoder<'de> for MyDecoder {
///     type Error = MyError;
///     type DecodeAnyAsync<'a, V> = MyAnyFuture<'a, V> where ...;
///
///     fn decode_any<'a, V>(self, visitor: V) -> Self::DecodeAnyAsync<'a, V>
///     where ... { ... }
///
///     forward_to_decode_any! {
///         decode_bool => DecodeBoolAsync,
///         decode_i8 => DecodeI8Async,
///         decode_unit_struct => DecodeUnitStructAsync(name: &'static str),
///         decode_tuple => DecodeTupleAsync(len: usize),
///     }
/// }
/// ```
///
/// 宏假定 `impl` 的 trait 泛型生命周期名为 `'de`，并且该 `impl` 已经定义了
/// `DecodeAnyAsync`。
///
/// # Panics
///
/// 宏本身不会 panic；转发的方法也不会 panic。
///
/// # Errors
///
/// 转发后的方法把错误原样来自 `decode_any`。
#[macro_export]
macro_rules! forward_to_decode_any {
    ($($method:ident => $future:ident $( ( $($arg:ident : $ty:ty),* ) )?),* $(,)?) => {
        $(
            $crate::forward_to_decode_any!(
                @impl $method => $future $( ( $($arg : $ty),* ) )?
            );
        )*
    };
    (@impl $method:ident => $future:ident) => {
        type $future<'a, V> = Self::DecodeAnyAsync<'a, V>
        where
            'de: 'a,
            V: $crate::de::Visitor<'de> + 'a,
            V::Value: 'a,
            Self: 'a,
            Self::Error: 'a;

        fn $method<'a, V>(self, visitor: V) -> Self::$future<'a, V>
        where
            'de: 'a,
            V: $crate::de::Visitor<'de> + 'a,
            V::Value: 'a,
            Self: 'a,
            Self::Error: 'a,
        {
            self.decode_any(visitor)
        }
    };
    (@impl $method:ident => $future:ident ( $($arg:ident : $ty:ty),* )) => {
        type $future<'a, V> = Self::DecodeAnyAsync<'a, V>
        where
            'de: 'a,
            V: $crate::de::Visitor<'de> + 'a,
            V::Value: 'a,
            Self: 'a,
            Self::Error: 'a;

        fn $method<'a, V>(self, $($arg: $ty,)* visitor: V) -> Self::$future<'a, V>
        where
            'de: 'a,
            V: $crate::de::Visitor<'de> + 'a,
            V::Value: 'a,
            Self: 'a,
            Self::Error: 'a,
        {
            let _ = ($($arg),*);
            self.decode_any(visitor)
        }
    };
}
