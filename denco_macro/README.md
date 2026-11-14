# denco_macro

`denco_macro` 为 `abs_denco` 提供两个派生宏：

- `#[derive(TrEncodable)]`：生成 `abs_denco::ser::TrEncodable` 实现；
- `#[derive(TrDecodable)]`：生成 `abs_denco::de::TrDecodable` 实现。

开启 `denco_macro/type-id` 后还提供显式的 `#[derive(TrHasTypeId)]`，生成
`abs_denco::type_id::TrHasTypeId`（见后文「类型 ID」）。

宏内部使用 [`zyn`](https://crates.io/crates/zyn) 的 `#[zyn::derive]`、模板与元素设施，
生成代码的 future 类型复用 `gen_mcf2::gen_may_cancel_future`，因此与手写实现保持
相同的取消语义。

## 依赖

派生代码会引用以下 crate，使用者的 `Cargo.toml` 中需要有它们：

```toml
[dependencies]
abs_denco = { path = "../abs_denco" }
abs_cancel = { git = "https://gitee.com/lino_snsalias/cancel_rs.git" }
gen_mcf2 = { git = "https://gitee.com/lino_snsalias/cancel_rs.git" }
denco_macro = { path = "../denco_macro" }
```

`gen_mcf2` 会生成以 `abs_cancel::...` 书写的代码，`abs_cancel` 也必须作为直接依赖
出现在使用者 crate 中。

## 基本用法

```rust
use denco_macro::{TrDecodable, TrEncodable};

#[derive(Debug, PartialEq, TrEncodable, TrDecodable)]
struct Record {
    id: u64,
    name: String,
}

#[derive(Debug, PartialEq, TrEncodable, TrDecodable)]
enum Shape {
    Empty,
    Number(u64),
    Pair(u64, String),
    Named { left: u64, right: String },
}
```

生成结果与手写实现一致：

- 命名字段结构体编码为结构体（MessagePack 下是命名映射）；
- 元组结构体编码为 tuple struct；
- newtype 结构体编码为 newtype struct；
- 枚举采用外部标签表示：unit 变体是变体名，newtype 变体是 `{variant: value}`，
  tuple 变体是 `{variant: [fields...]}`，struct 变体是
  `{variant: {field: value, ...}}`；
- 解码命名字段结构体时同时接受映射与位置数组；
- 未知字段返回 `Error::unknown_field`，缺失字段返回 `Error::missing_field`。

字段类型只需要自己实现 `TrEncodable` / `TrDecodable`；派生宏不会为基本类型
自动生成实现。

## 辅助属性

### 类型级

| 属性 | 说明 |
| --- | --- |
| `#[denco(rename = "...")]` | 修改结构体/枚举的编码名 |
| `#[denco(rename_all = "snake_case")]` | 批量重命名字段或枚举变体；支持 `lowercase`、`UPPERCASE`、`PascalCase`、`camelCase`、`snake_case`、`SCREAMING_SNAKE_CASE`、`kebab-case`、`SCREAMING-KEBAB-CASE` |
| `#[denco(transparent)]` | 仅用于单字段 newtype 结构体：直接编码/解码内部值 |

### 字段级

| 属性 | 说明 |
| --- | --- |
| `#[denco(rename = "...")]` | 修改字段的编码名 |
| `#[denco(alias = "...")]` | 解码时额外接受的字段名；可重复 |
| `#[denco(skip)]` | 编码和解码都跳过；解码时使用 `Default::default()` |
| `#[denco(skip_encoding)]` / `#[denco(skip_serializing)]` | 只跳过编码 |
| `#[denco(skip_decoding)]` / `#[denco(skip_deserializing)]` | 只跳过解码；解码时使用默认值 |
| `#[denco(default)]` | 映射缺字段时使用 `Default::default()` |

`Option<T>` 字段在映射中缺失时默认得到 `None`（若字段类型本身有
`TrDecodable` 实现）。

### 变体级

| 属性 | 说明 |
| --- | --- |
| `#[denco(rename = "...")]` | 修改变体名 |
| `#[denco(alias = "...")]` | 解码时额外接受的变体名；可重复 |

## 类型 ID（`type-id` feature）

默认关闭。开启时需要**同时**打开两个 feature：

```toml
abs_denco = { path = "../abs_denco", features = ["type-id"] }
denco_macro = { path = "../denco_macro", features = ["type-id"] }
```

`TrHasTypeId` 是**显式派生**，不会由 `TrEncodable` 附带生成：

```rust
use abs_denco::type_id::TrHasTypeId;
use denco_macro::{TrDecodable, TrEncodable, TrHasTypeId};

#[derive(TrEncodable, TrDecodable, TrHasTypeId)]
struct User {
    id: u64,
    name: String,
}

let wire_id = User::TYPE_ID_64;
let full = User::TYPE_ID_128;
```

- 三个宽度同时提供且**互相独立**：32/64/128 位各用原生宽度哈希与专属
  domain，不是 128 位结果的截断。
- 默认哈希是 FNV-1a 32/64/128；把 `abs_denco` 的 feature 换成
  `type-id-murmur3` 可切换为 Murmur3 x86_32 / MurmurHash64A / x64_128
  （Murmur3 没有原生 64 位输出，64 位使用 MurmurHash64A）。
- 哈希算法是线上兼容性契约的一部分，同一 workspace 只能选一种。
- 递归类型、手写 codec 类型或需要固定线上 ID 的类型**不要派生**
  `TrHasTypeId`，直接手写 `impl TrHasTypeId`（三个 const）；递归可调用
  `path_id_32` / `path_id_64` / `path_id_128`。
- 派生 `TrHasTypeId` 的类型，其参与线上的字段类型都必须实现 `TrHasTypeId`；
  手写 codec 类型需要自己补 impl。

## 示例：重命名与默认值

```rust
use denco_macro::{TrDecodable, TrEncodable};

#[derive(Debug, PartialEq, TrEncodable, TrDecodable)]
#[denco(rename_all = "snake_case")]
struct Config {
    #[denco(rename = "identifier")]
    user_id: u64,
    #[denco(skip)]
    cache: u64,
    #[denco(default)]
    retries: u64,
}
```

编码后的键是 `identifier` 与 `retries`；`cache` 不会被编码，解码时取
`Default::default()`；映射缺少 `retries` 时同样取默认值。

## 生成代码的组织

派生宏会在当前模块生成一组私有的辅助类型与函数，名称以
`__DencoEncode{类型名}` / `__DencoDecode{类型名}` 为前缀。它们不会进入公开 API；
关联类型里引用的是这些私有 future，因此使用者无需手动命名它们。

## 已知限制

- 多个生命周期参数都会绑定到同一个 `'alloc`；含生命周期的递归类型可能触发 trait
  求解循环，建议手写实现。
- 递归类型（直接或互相递归）在 `type-id` 下不要派生 `TrHasTypeId`，手写 impl
  并使用 `path_id_32` / `path_id_64` / `path_id_128`；否则 const 求值会报 `E0391`。
- 不支持 union。
- 暂不支持跳过元组结构体或元组变体的单个字段；这类字段的位置语义会改变。
- 暂不支持自定义 `#[denco(default = "path")]` 函数、`flatten`、`with` 等 serde
  高级属性。
- 辅助属性名固定为 `denco`；同一个类型同时派生两个宏时复用同一组属性。
- 生成代码依赖 `gen_mcf2` 的 `#[gen_may_cancel_future]`，使用者需要让
  `gen_mcf2` 与 `abs_cancel` 在作用域中可用。

## 测试

`denco_macro/tests/derive_.rs` 覆盖 unit/newtype/tuple/named 结构体、四种枚举
变体、泛型、`rename_all`、`skip`、`default`、`alias`、`transparent`、空结构体，
以及派生编码与 `rmp-serde` 命名字节的一致性；`denco_macro/tests/type_id_.rs`
覆盖显式 `TrHasTypeId` 的生成、泛型、transparent、手写固定 ID 与递归路径 ID。
