# denco derive 宏

**当前结论**：`denco_macro` 用 `zyn` 的 `#[zyn::derive]` 提供
`TrEncodable` / `TrDecodable` 两个派生宏；生成代码复用 `gen_mcf2` 的
`#[gen_may_cancel_future]` 来命名可取消 future。支持结构体与枚举的
unit/newtype/tuple/named 形状，以及常用的 `denco(...)` 重命名、跳过与默认值属性。

## 1. 目标

让库用户像使用 serde 的 `Serialize` / `Deserialize` 一样，通过
`#[derive(TrEncodable, TrDecodable)]` 直接得到异步编解码实现，不必为每个
结构体/枚举手写 Visitor、seed 与 `gen_mcf2` helper future。

## 2. 实现结构

- `src/lib.rs`：两个 `#[zyn::derive]` 入口，带 `attributes(denco)`；
- `src/attrs_.rs`：解析 `#[denco(...)]`；
- `src/model_.rs`：把 `DeriveInput` 转成字段/变体模型，并拒绝 union 与
  生命周期参数；
- `src/names_.rs`：实现 `rename_all` 的大小写转换；
- `src/encode_.rs` / `src/decode_.rs`：生成 helper future、Visitor、seed 与
  `TrEncodable` / `TrDecodable` 实现。

生成代码直接展开在被派生类型所在的模块，所有私有 helper 使用
`__DencoEncode{类型名}` / `__DencoDecode{类型名}` 前缀，避免同一模块内多个
派生互相冲突。

## 3. 关键取舍

### 3.1 为什么用 `gen_mcf2` 而不是自己写 future 类型

`TrEncodable::EncodeAsync` / `TrDecodable::DecodeAsync` 是 GAT，必须给出
具体类型。`gen_mcf2` 已经把「async fn → 可取消 future 类型」的规则与取消
令牌转发封装好，派生宏只需生成 `async fn` 并加属性。保持与手写实现完全
一致的取消语义。

代价是使用者的 crate 需要直接依赖 `gen_mcf2`（生成代码里写的是
`#[gen_mcf2::gen_may_cancel_future]`）以及 `abs_cancel`。

### 3.2 为什么 helper 直接放在父模块，而不是私有子模块/const 块

最初尝试把 helper 放进私有模块：

- `const _: () = { ... }` 中不能展开依赖 `self` 路径的 `gen_mcf2` 属性；
- 私有子模块里的 helper future 若出现在公开类型的 GAT 上，会触发
  `E0446: private type in public interface`；把 helper 改成 `pub` 又会因为
  它的 `Future::Output` 含私有目标类型而继续触发 E0446。

最终改为在父模块用唯一前缀命名 helper。这样公开类型与私有 helper 位于同一
模块，Rust 允许私有 future 出现在其 GAT 中；同时不会污染模块的公开命名
空间（helper 名称以 `__Denco...` 开头）。

### 3.3 名称与可见性

- helper future 的可见性跟随被派生类型的可见性：`pub` 类型生成 `pub`
  helper，私有类型生成私有 helper；
- 入口 helper 加 `#[doc(hidden)]`，避免进入文档；
- 同一类型同时派生两个宏时，编码与解码的 trait 别名分别使用
  `__DencoEnc...` / `__DencoDec...` 前缀，避免重复导入。

## 3.4 标准类型实现

派生宏要求字段类型自身实现 `TrEncodable` / `TrDecodable`。为使
`String`、数组、`Arc` 等经典字段可用，`abs_denco` 新增：

- `src/ser/impls_.rs`：标量、`str`/`String`、`[T]`/`[T; N]`/`Vec<T>`、
  `Option<T>`、`Box<T>`、`Arc<T>` 的编码实现；
- `src/de/impls_.rs`：对应的解码实现，数字 visitor 同时接受更宽的整数
  方法（MessagePack 的 positive fixint 会走 `visit_u64`）；
- `Vec` 按序列编码，`[T; N]` 用序列编码、以 `decode_tuple` 解码；
- `Arc` / `Box` 透明转发内部值，与 serde 的 `rc` feature 行为一致。

不实现借用型 `&str` / `&[u8]` 解码；需要借用结果时仍应手写实现。

## 4. 属性支持

- 类型：`rename`、`rename_all`、`transparent`；
- 字段：`rename`、`alias`、`skip`、`skip_encoding` / `skip_serializing`、
  `skip_decoding` / `skip_deserializing`、`default`；
- 变体：`rename`、`alias`。

默认行为与 serde 的外部标签表示一致：unit 变体编码为变体名，newtype 变体
编码为单键映射，tuple/struct 变体分别编码为 `{variant: [...]}` 与
`{variant: {...}}`。命名字段解码同时接受映射与位置数组；未知字段报错，
`Option` 与 `default` 字段可缺失。

## 5. 已知限制

- 不支持带生命周期参数的类型（借用型解码需手写）；
- 不支持 union、自定义 `default` 函数、`flatten`、`with`；
- 不支持跳过元组结构体/元组变体的单个字段；
- 枚举 struct 变体的字段暂不继承容器级 `rename_all`。

## 6. 验证

`denco_macro/tests/derive_.rs` 覆盖：

- unit / newtype / tuple / named 结构体；
- 四种枚举变体与 `rename_all`；
- 泛型与 const 泛型；
- `skip`、`default`、`alias`、`transparent`、空命名结构体；
- 与 `rmp_serde::to_vec_named` 的字节级一致性；
- `mp_denco/tests/derive_interop_.rs`：包含 `String`、数组、`Vec`、
  `Option`、`Arc`、标量与枚举变体的双向交叉编解码。

`cargo test --workspace`、`cargo clippy --workspace --all-targets -- -D warnings`
与 `format_use_imports.py --check` 均通过。
