# 结构体序列化：默认位置数组，可选命名映射

**结论**：`mp_denco` 默认把结构体编码为位置数组（与 `rmp_serde::to_vec` 的紧凑约定一致），并通过密封的 `SerializerConfig` 类型参数让调用方切换为带字段名的映射；配置随序列、映射、`Option` 与枚举变体的嵌套编码自动传播。

## 背景

在此前的实现中，`MpEncoder::encode_struct` 固定写 `map_header_(len)`，每个字段再写 `key + value`，因此默认字节与 `rmp_serde::to_vec_named` 一致。解码端已经同时接受映射与位置数组两种表示，但编码端没有选择权。

新的需求是：调用方决定序列化结构体时是否写入 `field_name`，还是按字段顺序写成 tuple；默认不写 key，同时允许显式选择 map。这个行为与 `rmp-serde` 的 `SerializerConfig` / `StructMapConfig` / `with_struct_map` 对应。

## 候选方案

### A. 运行时布尔配置字段

在 `MpEncoder` 中放一个 `bool`，`encode_struct` 与 `encode_field` 读取它。实现简单，但每层嵌套都要手动复制该字段；类型上也无法表达“这个编码器一定是命名映射”。

### B. 类型级密封配置（采用）

仿照 `rmp-serde/src/config.rs`：

- 公开密封 trait `SerializerConfig`，内部方法 `is_named(&self) -> bool`；
- `DefaultConfig` 返回 `false`，`StructMapConfig<C>` 返回 `true`，`StructTupleConfig<C>` 返回 `false`；
- encoder 与状态机都增加配置类型参数 `C = DefaultConfig`。

优点是选择发生在编译期、零运行时分支成本，且嵌套状态机可以直接携带同一个 `C`，不会在递归编码时丢失配置。代价是公开类型签名多一个带默认值的类型参数。

### C. Cargo feature 全局切换

无法让同一个进程内的不同调用点选择不同表示，且默认值一旦发布难以调整，因此不采用。

## 决策

采用 B。公开 API 为：

```rust
MpEncoder::new(&mut sink)                       // 默认：结构体为位置数组
MpEncoder::new(&mut sink).with_struct_map()     // 结构体为命名映射
MpEncoder::with_config(&mut sink, config)       // 显式配置
MpEncoder::new(&mut sink).with_struct_map().with_struct_tuple() // 显式恢复
```

`with_*` 方法消耗当前 encoder 并转交写端借用；`config()` 返回 `Copy` 的配置副本。配置类型是密封的，外部 crate 只能组合本 crate 提供的配置。

## 实现要点

- `MpEncoder<'w, W, C = DefaultConfig>` 持有 `config_: C`；`TrEncoder` 的 `EncodeSeq` / `EncodeMap` / `EncodeStruct` 等关联类型都携带 `C`。
- `MpSeqEncoder` 与 `MpMapEncoder` 也增加 `C`，构造子编码器时用 `MpEncoder::with_config(writer, self.config_)`，从而把配置传进序列元素、映射键值、结构体字段与枚举变体内部。
- 结构体继续复用 `MpMapEncoder`：`encode_struct` 按 `is_named` 选择 `map_header_` 或 `array_header_`，`encode_field` 只在 `is_named` 为真时写 key。这样不需要新增一套结构体状态机。
- 结构体枚举变体保持外层单键映射 `{variant: ...}` 不变，内层字段按同一配置选择 `{field: value}` 或 `[values...]`。这与 `rmp-serde` 对 `SerializeStructVariant` 的处理一致。
- 解码器不增加配置参数：`decode_struct` 的 visitor 本身同时实现 `visit_map` 与 `visit_seq`，因此两种表示都能读。

## 后果

- **行为变更**：默认结构体字节从命名映射变为位置数组，原来依赖 `to_vec_named` 字节的调用方需要显式调用 `with_struct_map()`。本次已同步更新 `interop_`、`derive_interop_`、`containers_`、`ring_`、`roundtrip_` 的字节断言。
- **增量 API**：新增 `config_` 模块与四个公开项 `SerializerConfig`、`DefaultConfig`、`StructMapConfig`、`StructTupleConfig`，以及 `MpEncoder::{with_config, config, with_struct_map, with_struct_tuple}`。`MpEncoder::new` 的默认行为改变，但签名不变。
- 内部 `gen_mcf2` helper 增加配置类型参数，属于实现细节；公开的 `MpEncoder` / `MpSeqEncoder` / `MpMapEncoder` 通过默认类型参数保持原有写法可用。
- `tests/struct_config_.rs` 覆盖：默认数组头、命名映射头、`Vec` / `BTreeMap` / `Option` 嵌套传播、四种枚举变体，以及两种字节都能解码。
- 密封 trait 为后续增加配置项（例如 human-readable、bytes 模式）留下扩展空间，而不破坏外部实现。
