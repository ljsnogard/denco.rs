//! 结构体序列化方式的可选配置。
//!
//! 配置形态参考 `rmp-serde` 的 `SerializerConfig`：由密封 trait 与若干
//! 包装类型组成，通过 [`MpEncoder`](crate::MpEncoder) 的类型参数在编译期
//! 选择结构体的字节表示。默认表示是位置数组（不写字段名）；需要字段名时
//! 用 [`StructMapConfig`] 包装当前配置。

/// 控制结构体序列化方式的配置 trait。
///
/// 本 trait 是密封的：外部 crate 不能实现它，只能使用本 crate 提供的
/// [`DefaultConfig`]、[`StructMapConfig`] 与 [`StructTupleConfig`]。
/// 这样后续增加配置项时不会破坏已有的外部代码。
///
/// 默认配置 [`DefaultConfig`] 把结构体编码为位置数组，不写字段名；
/// 用 [`StructMapConfig`] 包装后改为带字段名的映射。
///
/// # Examples
///
/// ```
/// use mp_denco::{DefaultConfig, SerializerConfig};
///
/// fn assert_config<C: SerializerConfig>(_: C) {}
/// assert_config(DefaultConfig);
/// ```
pub trait SerializerConfig: sealed::SerializerConfig {}

impl<T: sealed::SerializerConfig> SerializerConfig for T {}

pub(crate) mod sealed {
    /// 密封的配置行为；只在 crate 内部实现。
    pub trait SerializerConfig: Copy {
        /// 结构体是否编码为带字段名的映射。
        fn is_named(&self) -> bool;
    }
}

/// 默认配置：结构体编码为位置数组，不写字段名。
///
/// 这对应 `rmp_serde::to_vec` 的紧凑表示，也是 `mp_denco` 的默认行为。
/// 值与字段按声明顺序依次写入数组，因此要求编解码双方使用相同的字段顺序。
///
/// # Examples
///
/// ```
/// use mp_denco::{DefaultConfig, MpEncoder};
///
/// let mut storage = [0u8; 8];
/// let mut sink: &mut [u8] = &mut storage;
/// let _encoder = MpEncoder::with_config(&mut sink, DefaultConfig);
/// ```
#[derive(Copy, Clone, Debug, Default)]
pub struct DefaultConfig;

impl sealed::SerializerConfig for DefaultConfig {
    #[inline(always)]
    fn is_named(&self) -> bool {
        false
    }
}

/// 配置包装：把结构体编码为带字段名的映射。
///
/// 包装后保留内部配置的其余行为，只覆盖结构体的表示方式。对应
/// `rmp_serde::to_vec_named` 的命名表示。
///
/// # Examples
///
/// ```
/// use mp_denco::{DefaultConfig, MpEncoder, StructMapConfig};
///
/// let mut storage = [0u8; 8];
/// let mut sink: &mut [u8] = &mut storage;
/// let _encoder = MpEncoder::with_config(&mut sink, StructMapConfig::new(DefaultConfig));
/// ```
#[derive(Copy, Clone, Debug)]
pub struct StructMapConfig<C>(C);

impl<C> StructMapConfig<C> {
    /// 用 `inner` 作为底层配置构造命名映射配置。
    ///
    /// # Examples
    ///
    /// ```
    /// use mp_denco::{DefaultConfig, StructMapConfig};
    ///
    /// let _config = StructMapConfig::new(DefaultConfig);
    /// ```
    pub const fn new(inner: C) -> Self {
        Self(inner)
    }
}

impl<C> sealed::SerializerConfig for StructMapConfig<C>
where
    C: sealed::SerializerConfig,
{
    #[inline(always)]
    fn is_named(&self) -> bool {
        true
    }
}

/// 配置包装：把结构体编码为不写字段名的位置数组。
///
/// 用于在 [`StructMapConfig`] 之上显式恢复紧凑表示；默认配置本身已经
/// 是这个行为。
///
/// # Examples
///
/// ```
/// use mp_denco::{DefaultConfig, MpEncoder, StructMapConfig, StructTupleConfig};
///
/// let config = StructTupleConfig::new(StructMapConfig::new(DefaultConfig));
/// let mut storage = [0u8; 8];
/// let mut sink: &mut [u8] = &mut storage;
/// let _encoder = MpEncoder::with_config(&mut sink, config);
/// ```
#[derive(Copy, Clone, Debug)]
pub struct StructTupleConfig<C>(C);

impl<C> StructTupleConfig<C> {
    /// 用 `inner` 作为底层配置构造位置数组配置。
    ///
    /// # Examples
    ///
    /// ```
    /// use mp_denco::{DefaultConfig, StructTupleConfig};
    ///
    /// let _config = StructTupleConfig::new(DefaultConfig);
    /// ```
    pub const fn new(inner: C) -> Self {
        Self(inner)
    }
}

impl<C> sealed::SerializerConfig for StructTupleConfig<C>
where
    C: sealed::SerializerConfig,
{
    #[inline(always)]
    fn is_named(&self) -> bool {
        false
    }
}
