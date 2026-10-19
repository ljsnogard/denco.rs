# denco 命名、GAT 与 MessagePack

**当前结论**：`abs_denco` 的格式 trait 使用 GAT 关联类型，并采用 denco 的
encode/decode 命名：数据结构 trait 是 `TrEncodable` / `TrDecodable`，格式
trait 是 `TrEncoder` / `TrDecoder`，编码状态机是 `TrEncodeSeq` 等。
`Tr` 前缀只用于 trait 名；GAT 与关联类型不带 `Tr`，其中的
Serialize/Deserialize 词义统一改为 Encode/Decode。取消语义由 `cantok_rs`
驱动，并通过最小 MessagePack 做端到端验证。

## 1. 命名对照表

### 1.1 数据结构 trait

| denco | serde | 说明 |
| --- | --- | --- |
| `ser::TrEncodable` | `serde::Serialize` | 可被编码的数据结构 |
| `TrEncodable::encode_async` | `Serialize::serialize` | 方法名改为 `encode_async` |
| `TrEncodable::EncodeAsync` | 无对应 GAT | serde 同步返回 `Result`；denco 用 GAT 返回可取消 future |
| `de::TrDecodable` | `serde::Deserialize` | 可被解码的数据结构 |
| `TrDecodable::decode_async` | `Deserialize::deserialize` | 方法名改为 `decode_async` |
| `TrDecodable::DecodeAsync` | 无对应 GAT | 同上 |
| `de::TrDecodableSeed` | `serde::de::DeserializeSeed` | 带状态的解码 seed |
| `TrDecodableSeed::decode_async` | `DeserializeSeed::deserialize` | 方法名改为 `decode_async` |
| `TrDecodableSeed::DecodeAsync` | 无对应 GAT | 同上 |
| `de::TrDecodableOwned` | `serde::de::DeserializeOwned` | 不借用输入数据的解码结果 |

### 1.2 格式 trait

| denco | serde | 说明 |
| --- | --- | --- |
| `ser::TrEncoder` | `serde::Serializer` | 编码器 |
| `TrEncoder::encode_bool` / `encode_str` / `encode_seq` / … | `Serializer::serialize_bool` / `serialize_str` / `serialize_seq` / … | 方法名改为 `encode_*` |
| `TrEncoder::EncodeBoolAsync` / `EncodeStrAsync` / `EncodeSeqAsync` / … | 无对应 GAT | 对应各 `encode_*` 方法的可取消 future |
| `TrEncoder::Ok` / `Error` | `Serializer::Ok` / `Error` | 不变 |
| `de::TrDecoder` | `serde::de::Deserializer` | 解码器 |
| `TrDecoder::decode_any` / `decode_bool` / `decode_seq` / … | `Deserializer::deserialize_any` / `deserialize_bool` / `deserialize_seq` / … | 方法名改为 `decode_*` |
| `TrDecoder::DecodeAnyAsync` / `DecodeBoolAsync` / `DecodeSeqAsync` / … | 无对应 GAT | 对应各 `decode_*` 方法的可取消 future |
| `TrDecoder::Error` | `Deserializer::Error` | 不变 |
| `TrDecoder::is_human_readable` | `Deserializer::is_human_readable` | 同步方法，不变 |

### 1.3 编码状态机

| denco trait | serde trait | denco 关联类型 | serde 关联类型 |
| --- | --- | --- | --- |
| `TrEncodeSeq` | `SerializeSeq` | `TrEncoder::EncodeSeq` | `Serializer::SerializeSeq` |
| `TrEncodeTuple` | `SerializeTuple` | `TrEncoder::EncodeTuple` | `Serializer::SerializeTuple` |
| `TrEncodeTupleStruct` | `SerializeTupleStruct` | `TrEncoder::EncodeTupleStruct` | `Serializer::SerializeTupleStruct` |
| `TrEncodeTupleVariant` | `SerializeTupleVariant` | `TrEncoder::EncodeTupleVariant` | `Serializer::SerializeTupleVariant` |
| `TrEncodeMap` | `SerializeMap` | `TrEncoder::EncodeMap` | `Serializer::SerializeMap` |
| `TrEncodeStruct` | `SerializeStruct` | `TrEncoder::EncodeStruct` | `Serializer::SerializeStruct` |
| `TrEncodeStructVariant` | `SerializeStructVariant` | `TrEncoder::EncodeStructVariant` | `Serializer::SerializeStructVariant` |

状态机内部方法名：

- `TrEncodeSeq::encode_element` / `end` ↔ `SerializeSeq::serialize_element` / `end`
- `TrEncodeTuple::encode_element` / `end` ↔ `SerializeTuple::serialize_element` / `end`
- `TrEncodeTupleStruct::encode_field` / `end` ↔ `SerializeTupleStruct::serialize_field` / `end`
- `TrEncodeTupleVariant::encode_field` / `end` ↔ `SerializeTupleVariant::serialize_field` / `end`
- `TrEncodeMap::encode_key` / `encode_value` / `end` ↔ `SerializeMap::serialize_key` / `serialize_value` / `end`
- `TrEncodeStruct::encode_field` / `skip_field` / `end` ↔ `SerializeStruct::serialize_field` / `skip_field` / `end`
- `TrEncodeStructVariant::encode_field` / `skip_field` / `end` ↔ `SerializeStructVariant::serialize_field` / `skip_field` / `end`

### 1.4 便利方法与访问器

| denco | serde | 说明 |
| --- | --- | --- |
| `TrEncoderExt::collect_seq` | `Serializer::collect_seq` | denco 放在扩展 trait，避免给 `TrEncoder` 增加 GAT |
| `TrEncoderExt::collect_map` | `Serializer::collect_map` | 同上 |
| `TrEncoderExt::collect_str` | `Serializer::collect_str` | 同上 |
| `TrEncodeMapExt::encode_entry` | `SerializeMap::serialize_entry` | denco 放在扩展 trait |
| `SeqAccessExt::next_element` | `SeqAccess::next_element` | denco 放在扩展 trait |
| `MapAccessExt::next_key` / `next_value` / `next_entry` | `MapAccess::next_key` / `next_value` / `next_entry` | 同上 |
| `EnumAccessExt::variant` | `EnumAccess::variant` | 同上 |
| `VariantAccessExt::newtype_variant` | `VariantAccess::newtype_variant` | 同上 |
| `forward_to_decode_any!` | `forward_to_deserialize_any!` | 转发宏 |
| `Visitor` | `Visitor` | 未改名 |
| `SeqAccess` / `MapAccess` / `EnumAccess` / `VariantAccess` | 同名 | 未改名 |
| `IgnoredAny` | `IgnoredAny` | 未改名 |
| `Impossible` | `Impossible` | 未改名 |
| `Error` / `Expected` / `Unexpected` | 同名 | 未改名 |
| `BoxedMayCancel` | 无 | denco 新增，用于打破递归 future 类型 |
| `MpEncoder` / `MpDecoder` | 无 | MessagePack 的具体格式类型，原名 `MpSerializer` / `MpDeserializer` |
| `MpSeqEncoder` / `MpMapEncoder` | 无 | MessagePack 的具体状态机类型 |

### 1.5 GAT 与 `Tr` 前缀规则

`Tr` 前缀只加在 **trait 名** 上：

- `TrEncodable`、`TrDecodable`、`TrDecodableSeed`、`TrDecodableOwned`
- `TrEncoder`、`TrDecoder`
- `TrEncodeSeq`、`TrEncodeTuple`、`TrEncodeMap`、`TrEncodeStruct` 等
- `TrEncoderExt`、`TrEncodeMapExt`

GAT、关联类型和 helper 类型不是 trait，不加 `Tr` 前缀：

- `TrEncodable::EncodeAsync`
- `TrDecodable::DecodeAsync`
- `TrEncoder::EncodeBoolAsync` / `EncodeStrAsync` / `EncodeSeqAsync` / …
- `TrDecoder::DecodeAnyAsync` / `DecodeBoolAsync` / `DecodeSeqAsync` / …
- `TrEncoder::EncodeSeq` / `EncodeMap` / `EncodeStruct` / …
- helper：`DecodeAnyOpAsync`、`EncodeBoolOpAsync` 等

GAT 名中原来的 Serialize/Deserialize 词义统一改为 Encode/Decode，例如：

- `Serializer::serialize_bool` 的 GAT → `TrEncoder::EncodeBoolAsync`
- `Deserializer::deserialize_any` 的 GAT → `TrDecoder::DecodeAnyAsync`
- `Serializer::SerializeSeq` 关联类型 → `TrEncoder::EncodeSeq`

## 2. GAT 与 RPITIT 的边界

### 2.1 使用 GAT 的 trait

这些 trait 直接承载格式与数据结构的交互，future 类型需要被实现者命名：

- `TrEncodable`
- `TrEncoder`
- `TrDecodable`
- `TrDecodableSeed`
- `TrDecoder`

### 2.2 保留 RPITIT 的 trait

- `Visitor`
- `TrEncodeSeq` / `TrEncodeTuple` / `TrEncodeTupleStruct` / `TrEncodeTupleVariant`
- `TrEncodeMap` / `TrEncodeStruct` / `TrEncodeStructVariant`
- `SeqAccess` / `MapAccess` / `EnumAccess` / `VariantAccess`

原因：

- 它们有大量默认实现（尤其是 `Visitor` 的默认类型错误）；Rust 的关联类型
  默认值仍不稳定，GAT 下无法保留这些默认实现；
- 它们很少被最终用户直接命名 future 类型；
- 保留 RPITIT 可以继续用默认实现降低 Visitor 与状态机的实现负担。

### 2.3 `TrDecoder` 与 Visitor 的衔接

`TrDecoder` 的 GAT 具体类型不能直接写成 `V::VisitXxxFuture`（Visitor 是
RPITIT，返回匿名类型）。格式实现用 `gen_mcf2` 为每个 `decode_*` 生成命名
helper future；helper 内部 `await` visitor 的 RPITIT future 并传递取消令牌。
最小 MessagePack 先用 `decode_any` 一个 helper，其余方法用
`forward_to_decode_any!` 转发并复用该 GAT。

### 2.4 反序列化方法显式化

`TrDecoder` 不提供默认转发。每个 `decode_*` 在 trait 中声明独立 GAT 与
函数；格式实现可以用 `forward_to_decode_any!` 生成转发用的 GAT 别名与
方法体。

### 2.5 helper 的 `Op` 约定

`gen_mcf2` 为前缀 `Foo` 生成 `FooAsync`（实现 `TrMayCancel`）与
`FooFuture`（实现 `Future`）。如果 GAT 叫 `EncodeBoolAsync`，helper 前缀
不能也叫 `EncodeBool`，否则生成类型重名。约定是给 helper 前缀加 `Op`：

```rust
#[gen_may_cancel_future(DecodeAnyOp)]
async fn decode_any_async<...>() -> Result<...> { ... }
```

生成 `DecodeAnyOpAsync` / `DecodeAnyOpFuture`，而 trait 中的 GAT 仍是
`DecodeAnyAsync`。`Visitor` 的 helper（`VisitString` 等）不与 GAT 撞名，
因此不需要 `Op`。

## 3. 取消语义

- 接入 `cantok_rs` 的 `CancellationSource` / `CancellationToken`；
- `MpDecoder` 的解码入口 `decode_any_async` 在读取 tag 前检查
  `cancel.is_cancelled()`，已取消时直接返回错误；
- `MpEncoder` 的同步方法改用 `CancellableReady`，在 `may_cancel_with` 时
  检查预取消状态；
- `mp_denco/tests/cancel_.rs`：
  - 慢速 `SlowDecoder` 经过 `forward_to_decode_any!` 转发到会等待
    `cancellation()` 的 `decode_any`，另一个线程触发取消，验证信号能穿过
    转发宏到达最内层；
  - `SlowEncodable` 的 `encode_async` helper 等待取消，验证顶层编码路径；
- `roundtrip_.rs` 增加已取消令牌的编码/解码测试。

## 4. MessagePack 现状

`mp_denco` 已支持：

- 编码：nil、bool、整数（i128/u128 编码为 16 字节大端 binary）、浮点、
  字符串、二进制、序列/元组、映射/结构体、单元结构体、newtype 结构体、
  四种枚举变体；
- 解码：nil、bool、整数、浮点、字符串、二进制、数组、映射、`Option`、
  单元/newtype 结构体、四种枚举变体；
- 结构体默认编码为位置数组（不写字段名），与 `rmp_serde::to_vec` 一致；
  通过 `MpEncoder::with_struct_map` 可切换为命名映射；解码同时接受位置数组
  与命名映射，因此既能读取 `rmp_serde::to_vec` 的输出，也能读取
  `rmp_serde::to_vec_named` 的输出。

与 `rmp-serde` 对齐的具体约定：

| 数据形态 | MessagePack 表示 |
| --- | --- |
| 单元结构体 | 空数组 `0x90`（解码也兼容旧的 nil） |
| newtype 结构体 | 透明编码内部值 |
| 单元枚举变体 | 变体名字符串 |
| newtype 变体 | 单键映射 `{variant: value}` |
| 元组变体 | 单键映射 `{variant: [fields...]}` |
| 结构体变体 | 默认单键映射 `{variant: [values...]}`；命名映射配置为 `{variant: {field: value, ...}}` |
| i128 / u128 | 16 字节大端 binary `0xc4 0x10 ...` |
| 字符串 / 二进制 | 短值栈数组，长值 `String` / `Vec<u8>`（owned 解码） |

默认编码与 `rmp_serde::to_vec` 的字节在测试覆盖的类型上完全一致；
`with_struct_map()` 之后的字节与 `rmp_serde::to_vec_named` 一致。详见
[`2026-10-19-struct-serialization-config.md`](2026-10-19-struct-serialization-config.md)。

MessagePack 是同步内存格式，无法在操作中途被打断；当前只保证入口处的
预取消检查与 helper 路径的取消传播。

## 5. 依赖说明

上游 git 版本的 `cantok_rs` 目前还是空壳，因此 `denco/Cargo.toml` 暂时使用
本地 patch：

```toml
[patch."https://gitee.com/lino_snsalias/cancel_rs.git"]
abs_cancel = { path = "../cancel_rs/abs_cancel-rs" }
gen_mcf2 = { path = "../cancel_rs/gen_mcf2" }
```

并把 `cantok_rs` 指向同级 `cancel_rs/cantok_rs`。这样保证
`abs_cancel` / `gen_mcf2` / `cantok_rs` 使用同一份 `abs_cancel`。上游推送
`cantok_rs` 后应删除该 patch 并改回 git 依赖。

## 6. 验证

- `cargo test --workspace`：通过（含 `abs_denco`、`denco_macro`、`mp_denco`
  的单元/集成/文档测试）；
- `cargo clippy --workspace --all-targets`：无警告；
- `format_use_imports.py --check`：通过。

## 7. 仍未完成

- `de::value` 模块（`BorrowedStrDeserializer`、`SeqDeserializer` 等）；
- 未知长度的 `encode_seq(None)` / `encode_map(None)` 仍返回错误；
  `rmp-serde` 会用临时缓冲补齐长度，denco 尚未引入该策略；
- MessagePack ext 类型（fixext / ext8 / ext16 / ext32）未实现；
- `BoxedMayCancel` 仍忽略取消令牌（`IgnoredAny` 递归场景的折中）；
- `Visitor`、`SeqAccess` / `MapAccess` / `EnumAccess` / `VariantAccess`
  仍是 serde 名，可按需继续重命名；
- 具体测试类型 `SlowDecoder` 只是测试用名称，不属于公共 API。
- 真实 `buffex::Ring`、tokio socket 场景已由
  [`2026-10-18-real-buff-and-socket-tests.md`](2026-10-18-real-buff-and-socket-tests.md)
  覆盖；smol / compio 运行时的 ring/socket 组合仍未覆盖。

## 8. 与 rmp-serde 的互操作验证

`mp_denco/tests/interop_.rs` 用同一批类型同时实现 serde 与 denco trait，
对每个值做四条检查：

1. `mp_denco` 默认编码字节 == `rmp_serde::to_vec` 编码字节；
2. `rmp_serde::from_slice` 能解码 `mp_denco` 的默认字节；
3. `mp_denco` 能解码 `rmp_serde::to_vec` 的紧凑数组字节；
4. `mp_denco` 能解码 `rmp_serde::to_vec_named` 的命名字节。

覆盖的类型：

- 标量结构体（bool / i64 / u64 / f32 / f64 / char / String）；
- 二进制字段（`serde_bytes`）；
- i128 / u128；
- 数组、`Option::Some` / `Option::None`；
- 单元结构体、newtype 结构体、元组结构体；
- 枚举的 unit / newtype / tuple / struct 四种变体。

为了通过第 1、3、4 条，`mp_denco` 修正了三处与 serde 数据模型不一致的
设计：单元结构体改用空数组；枚举变体改为 `rmp-serde` 的单键映射约定；
i128/u128 改为 16 字节大端 binary。解码侧则把 unit struct、newtype struct、
i128/u128、enum 从 `decode_any` 转发改为定向实现。字符串/二进制路径在
2026-10-18 的借用反序列化移除后统一走 `visit_str` / `visit_string` /
`visit_bytes` / `visit_byte_buf`（见 `2026-10-18-real-buff-and-socket-tests.md`
第 10 节）。

## 9. Buff 化改造

`mp_denco` 的 encoder / decoder 已从「切片 + 同步读写」改为直接面向
`abs_buff`：

- `MpEncoder<'w, W: TrBuffWrite<u8>>`：所有头部与标量先在栈数组
  `ScalarBytes` 中编码，再通过 `write_async` 借出的写段写入；写循环按实际
  可写量推进，不把输出预攒进 `Vec`。
- `MpDecoder<'r, R: TrBuffRead<u8>>`：标签、长度与定长负载都通过
  `read_async` 按需读取；不再预扫描输入长度。
- 解码字符串/二进制时短值读入栈数组，长值边读边构造结果 `String` /
  `Vec<u8>`。异步方法不会为了解析而把整个 buff 预拷到堆。
- 枚举变体名先读成 `NameRef`（栈数组 / 长值 String），再用
  `MpNameDecoder` 直接驱动标识符 seed 的 `visit_str`，避免重复解析头。

> 2026-10-18 更新：`TrBuffTryReadBorrowed` / `TrBuffReadBorrowed` /
> `OwnedRead` 与相关测试已移除，解码统一走 owned 路径。原因与影响见
> `2026-10-18-real-buff-and-socket-tests.md` 第 10 节。

依赖上，`mp_denco` 暂以 path 引用 `../../buffex.rs/abs_buff`；`abs_buff`
自身仍通过 git 引用 `abs_cancel` / `gen_mcf2`，由 denco 根工作区的
`[patch]` 统一到本地检出。

## 10. 决策历史（简）

1. 第一版用 RPITIT；调用者无法命名 future，也无法在 trait 上统一加
   `Send` 等约束，因此核心格式 trait 改为 GAT。
2. GAT 关联类型不能有默认值，因此 `Visitor`、状态机与访问器保留 RPITIT。
3. GAT 名采用 `Encode*Async` / `Decode*Async`；helper 前缀加 `Op` 以避免
   与 `gen_mcf2` 生成的 `*Async` 类型冲突。
4. 数据结构 trait 改名为 `TrEncodable` / `TrDecodable`，格式 trait 改名为
   `TrEncoder` / `TrDecoder`，状态机改名为 `TrEncodeSeq` 等；方法名统一
   改为 `encode_*` / `decode_*`，`MpSerializer` / `MpDeserializer` 改为
   `MpEncoder` / `MpDecoder`。
5. 取消语义接入 `cantok_rs`，并保留本地 path patch 直到上游推送完成。
6. 与 `rmp-serde` 互操作时发现 unit struct、枚举、i128/u128 与 serde
   数据模型不一致，按 `rmp-serde` 的表示修正；结构体默认编码为位置数组，
   命名映射作为可配置选项，解码兼容两种表示。
7. 面向真正的 async 数据源，encoder/decoder 改为直接使用 `abs_buff`。
8. 2026-10-18 取消借用内存反序列化：`MpDecoder` 去掉 `'de`，只要求
   `TrBuffRead<u8>`；`abs_buff` 的借用读 trait 与 `OwnedRead` 回退一并
   移除。原因是 `OwnedRead` 回退对可复用缓冲反直觉且危险，而零拷贝收益只
   覆盖稳定切片读端。
