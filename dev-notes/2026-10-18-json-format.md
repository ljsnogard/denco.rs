# JSON 格式实现与三运行时 socket 测试

**当前结论**：`json_denco` 已按 `mp_denco` 的结构落地：`JsonEncoder` 直接向
`TrBuffWrite<u8>` 借段写出，`JsonDecoder` 直接向 `TrBuffRead<u8>` 借段读入。
解码器**不做预读缓冲**，只保留一个回推字节；完整顶层值结束后最多留下空白，
因此既可以复用解码器，也可以逐消息新建解码器。长字符串的读取效率由
`TrBuffRead` 的提供者负责。格式与 `serde_json` 的默认表示一致。
tokio、smol、compio 三个后端的 TCP 测试分别在
`json_denco/tests/socket_{tokio,smol,compio}_.rs`，全部通过。

> 2026-10-18 修正：最初实现使用 512 字节解码器内缓冲，导致多消息流必须复用
> 同一个 `JsonDecoder`，否则预读的后续消息会随解码器丢弃。该缓冲已移除，
> 见第 7 节。

## 1. 背景

`mp_denco` 验证了异步编解码抽象能直接对接 `abs_buff` 的段接口，但 MessagePack
是自描述二进制格式，标签、长度与定界都不需要额外语法层。JSON 是文本格式，
必须在同一个段读写接口之上补一层流式词法/语法解析：

- 不能预读整个文档，否则大 payload 与 `read_async` 的 park 语义都失去意义；
- 数字、字符串与对象键的结束位置需要回推一个字节；
- 不同数组/对象访问器可能提前结束（例如定长元组只消费固定个元素），
  解码器必须负责消费剩余的 `]` / `}`；
- 数字解析需要一个字节的回推；字符串与容器不需要回推。

## 2. 编码器决策

- 所有标点、数字文本与转义后的字符串分片都通过 `write_async` 借出的写段写出，
  不预攒整个输出；单个片段过长时按写段可写量循环推进。
- 数字先在 64 字节栈缓冲上格式化：整数直接 `Display`，浮点数要求有限，
  且当 `Display` 输出没有小数点/指数时补 `.0`，保证解码时仍走浮点路径。
- 字符串按 JSON 规则转义 `"`、`\`、控制字符与常见转义；非 ASCII 直接按 UTF-8
  写出，不转成 `\uXXXX`。
- 字节串编码为数字数组（与 `serde_json` 对 `serialize_bytes` 的默认表示一致）。
- 外部标签枚举：unit 变体是字符串，newtype 变体是 `{"Variant": value}`，
  tuple 变体是 `{"Variant": [...]}`，struct 变体是 `{"Variant": {...}}`。
- 映射键按 JSON 要求写成字符串。键编码器是同一个 `JsonEncoder` 的 `key_` 模式：
  字符串、char、布尔、整数与单元变体受支持；浮点、字节串与复合值返回错误。
- 复用 `mp_denco` 的 `CancellableReady` 思路，为 `end()` 这类立即完成的操作
  保留预取消检查。

## 3. 解码器决策

- **无预读缓冲**：解码器不持有任何流缓冲；`read_one_async_` 用
  `Demand::no_more_than(1)` 借段，只消费一个字节，剩余字节留在 `TrBuffRead`
  内部。长字符串逐字节读取的效率由读端提供者负责，解码器不额外复制一层缓冲。
- **单字节回推**：`peek_byte_async_` 把读出的字节放进 `pending_`，
  `read_byte_async_` 优先消费 `pending_`。这样数字可以在不消费定界符的情况下
  结束；字符串读到闭合引号、容器读到闭合符即结束，不需要回推。
- **状态可复用但非强制**：`JsonDecoder` 只持有读端、`pending_` 与结束状态；对
  `&mut JsonDecoder` 实现 `TrDecoder<'de>`。完整顶层值结束后，`pending_` 只可能
  是数字后的空白（丢失无害）或已由容器访问器消费的结构定界符，因此逐消息新建
  解码器不会丢数据。
- **统一模式机**：所有 `decode_*` 方法都通过一个 `ValueMode` 分派到
  `decode_value_async_`，避免为四十多个方法复制解析逻辑；对象键另有一个
  `JsonNameDecoder`，把已读出的字符串按键类型解释为字符串/整数/布尔/浮点/
  char/单元枚举变体。
- **提前结束的访问器**：`JsonSeqAccess` / `JsonMapAccess` 用共享的 `done_` 标记
  记录闭合符是否已消费；`visit_seq_async_` / `visit_map_async_` 在 visitor
  提前返回时补消费 `]` / `}`。这修复了定长元组只消费固定元素后把 `]` 留给
  tuple/struct 变体闭括号的问题。
- **字符串**：支持 `\" \\ \/ \b \f \n \r \t`、`\uXXXX` 与 UTF-16 代理对；
  未转义控制字符报错，UTF-8 在构造 `String` 时校验。
- **字符串快速路径**：`read_string_run_async_` 每次最多借 4096 字节，用
  `iter_slices()` 预览，搜索引号、反斜杠与控制字符；只把特殊字符之前的普通
  字节一次性搬进目标 `Vec`，特殊字符留给逐字节慢路径。未消费的字节由段回收
  留在读端，因此仍然没有解码器级预读缓冲。
- **数字**：按 JSON 语法校验前导零、小数与指数；整数用 `u128` / `i128` 解析，
  能放进 `u64` / `i64` 时优先调用 `visit_u64` / `visit_i64`，否则用 128 位
  visitor 方法；浮点数统一 `visit_f64`。
- **字节数组**：解析数字数组后走 `visit_byte_buf` / `visit_bytes`。
- **对象键**：`decode_any` 对键调用 `visit_string`，数值/布尔键由
  `JsonNameDecoder` 转换；键的 newtype/tuple/struct 内容不受支持（JSON 键
  只能是标量）。

## 4. 已知限制

- 不提供 `serde_json` 的任意精度数字与原始 JSON 片段能力。
- 浮点只接受有限值；`NaN` / 无穷大编码报错。
- 顶层连续值之间需要调用方写分隔空白；编码器不会自动追加分隔符。
- 映射键编码只支持标量；`HashMap<Struct, _>` 这类键会被拒绝。
- 解码器按 owned 路径工作，字符串与字节不借用读端内存。

## 5. 测试覆盖

- `tests/roundtrip_.rs`（8 个用例）：复合结构体与四种枚举变体同
  `serde_json` 双向交叉；标量与 `i128::MIN` / `u128::MAX`；`encode_bytes` /
  `decode_bytes`；字符串转义、`\uXXXX` 与代理对；同一解码器连续解码多个
 空白分隔数值；每个消息新建解码器仍能按序解码；约 10 KiB、跨多个段预览的长转义字符串。
- `tests/socket_tokio_.rs`（5 个用例）：复合值往返、双向并发、多消息流、
  64 KiB 超环容量负载、部分 JSON 的取消。
- `tests/socket_smol_.rs`（3 个用例）：`async-net` TCP 上的往返、逐消息新建
  解码器的多消息流、64 KiB 负载；`futures_lite::future::block_on` 驱动，
  5 秒超时。
- `tests/socket_compio_.rs`（3 个用例）：compio TCP 上的往返、多消息流、
  64 KiB 负载；服务端任务用 `spawn` 与客户端解码并发。

socket 用例统一使用 `Wire` 结构体而非字段更多的 `Payload`：后者的嵌套枚举
会让测试 async block 的类型布局过深，`cargo check` 需要提高递归上限且明显
变慢；`Payload` 的复杂组合已由切片测试覆盖。

## 6. 后果

- `json_denco` 与 `mp_denco` 共享同一套 `abs_denco` trait 与 `abs_buff` 段接口，
  三个运行时后端无需任何格式侧改动。
- 解码器只保留 `pending_` 与结束状态，不引入固定流缓冲；只有长字符串/字节数组
  会按需分配目标容器。
- 多消息流不再强制复用 `JsonDecoder`；复用与逐消息新建两种用法都有测试覆盖。
- 64 KiB 字符串用例在 5 秒超时内完成。字符串快速路径落地后，debug 下三个
  socket 测试文件（含全部用例）分别约 0.3s / 0.01s / 0.02s；优化前约为
  0.7s / 0.9s / 0.1s。

## 7. 决策修正：移除解码器内 512 字节流缓冲

### 7.1 原设计

最初 `JsonDecoder` 持有 512 字节 `buf_`，`fill_async_` 每次最多借 512 字节；
解析器从该缓冲逐字节读取。好处是每 512 字节才调用一次 `read_async`，但代价是
解码器变成“流游标”：一旦调用者按值新建解码器，预读进缓冲的后续消息就会丢失。

### 7.2 修正理由

- JSON 语法只需要**一个字节**的 lookahead；字符串闭合引号与容器闭合符都在
  当次顶层解码内消费，不需要跨值保留。
- 512 字节缓冲带来的收益只是减少 `read_async` 次数，却引入了“必须复用解码器”
  的隐式 API 约束，属于设计缺陷。
- `TrBuffRead` 本身就是缓冲区；解码器不应再叠一层持久缓冲，长字符串的读取
  效率应由读端提供者负责。
- 段接口支持部分消费：`read_async(no_more_than(1))` 只消费一个字节，剩余字节
  留在读端，不会被解码器丢弃。

### 7.3 修正后的不变量

- `JsonDecoder` 不持有读缓冲，只持有 `pending_`、`ended_` 与 `end_err_`。
- 完整顶层值结束后，`pending_` 最多是空白；空白丢失不影响下一个值。
- 顶层数字之间必须有空白分隔；没有分隔符的相邻数字本身无法区分。
- 逐消息新建解码器与复用解码器都受测试保护。
