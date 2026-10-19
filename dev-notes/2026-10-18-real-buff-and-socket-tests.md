# 真实 buff / socket 场景测试计划

**当前结论**：计划内的 `buffex::Ring`、TCP、UDS 集成测试已落地到
`mp_denco/tests/{ring_,socket_}.rs`，共享类型与超时辅助在
`mp_denco/tests/support/mod.rs`；`buffex_tokio_adapt` 的离线依赖可用。
实施中发现 tokio 读适配器在设备 park 时不响应取消令牌，随后在 `buffex.rs`
仓库用 `abs_cancel::ok_or::XtOkOr` 让设备 I/O 与取消信号竞速修复，UDS 部分
头用例已改为依赖真实取消（见第 9.2 节）。2026-10-18 追加决策：取消
`mp_denco` 的借用内存反序列化，`abs_buff` 中为它新增的借用读 trait 与
`OwnedRead` 一并移除（见第 10 节）。

## 1. 当前缺口

已有覆盖：

- `mp_denco/tests/roundtrip_.rs`：`&mut [u8]` 写端、`&mut &[u8]` 读端；
- `mp_denco/tests/interop_.rs`：同一批类型与 `rmp_serde` 双向交叉；
- ~~`borrowed_str_zero_copy_`~~、~~`async_borrowed_zero_copy_`~~、
  ~~`owned_read_fallback_`~~、~~`partial_lend_long_string_`~~：已随借用
  反序列化一并移除；字符串/二进制统一走 owned 路径，长值覆盖见
  `ring_long_string_owned_path_` 与 `tcp_large_payload_beyond_ring_capacity_`。

未覆盖：

- `buffex::ring::{Ring, RingReader, RingWriter}` 的段读写、环绕、跨段、
  容量压力与 park/wake；
- `buffex_tokio_adapt` / `buffex_smol_adapt` 的 `BuffRead` / `BuffWrite`；
- 真实 TCP / Unix domain socket 上的分片、并发双向、半关闭、超时；
- 真实缓冲/网络设备上的取消传播与无死锁验证。

## 2. 测试目标

1. 证明 `MpEncoder` / `MpDecoder` 能在真实 `TrBuffWrite` / `TrBuffRead`
   实现上工作，而不只是在切片和自制读端上工作。
2. 覆盖段被拆成多片、容量不足需要 park、数据分片到达等真实形态。
3. 确认可复用缓冲（ring、socket 适配器）只通过 `read_async` 读取，解码
   结果一律为 owned 值。
4. 覆盖取消令牌在真实 park 点上的传播，避免测试挂起。
5. 为后续 `buffex::Ring` 的稳定借用/lease 设计提供回归基线（当前已明确
   不做零拷贝解码，见第 10 节）。

## 3. 依赖与测试文件

建议新增：

- `mp_denco/tests/ring_.rs`
- `mp_denco/tests/socket_.rs`
- 如需复用，可加 `mp_denco/tests/support/mod.rs`（按项目规范用 `mod.rs`，
  逐项导出，不使用 `pub use *`）。

`mp_denco/Cargo.toml` 的 `[dev-dependencies]` 预计需要：

```toml
buffex = { path = "../../buffex.rs/buffex" }
buffex_tokio_adapt = { path = "../../buffex.rs/buffex_tokio_adapt" }
tokio = { version = "1", features = [
    "macros", "rt-multi-thread", "net", "io-util", "time",
] }
# 可选第二阶段：smol 运行时
# buffex_smol_adapt = { path = "../../buffex.rs/buffex_smol_adapt" }
# smol = "2"
# async-net = "2"
```

注意：

- `buffex_tokio_adapt` 自身依赖 `abs_art` / `mm_ptr` 等 git 依赖，需先确认
  离线缓存可用；必要时在 denco 根 `Cargo.toml` 增加对应 `[patch]`。
- tokio 适配器示例使用 `#![feature(allocator_api)]`；集成测试若直接构造
  `DefaultAllocConfig`，按实际编译错误决定是否需要在测试 crate 顶部开启。
- 套接字测试统一加 5 秒超时包装，避免 park 导致测试挂死。

## 4. `buffex::Ring` 测试计划（ring_.rs）

ring 的缓冲内存会在段回收后复用，因此解码器只使用 `read_async`，不借用
段内存；字符串与二进制一律产出 owned 值。

### 4.1 基础往返

- `ring_roundtrip_small_`
  - 手段：`Ring::try_new(Box::<[u8]>::new_uninit_slice(4096))`，`split()`
    得到 `(writer, reader)`；用 `MpEncoder::new(&mut writer)` 编码
    `U64Val`，drop 写端后用 `MpDecoder` 直接解码 `reader`。
  - 判断：解码值等于原值；与 `rmp_serde::to_vec` 字节一致。

### 4.2 跨段与环绕

- `ring_wrap_around_two_segments_`
  - 手段：容量取 64（需在 ring 允许区间内）；先写入并消费若干填充字节，
    让读写位置跨过末端，再编码一个会跨越环末端的值。
  - 判断：`RingSegmRef::iter_slices()` 至少两片；解码结果正确；
    `ring_state` 最终消费完毕。
- `ring_multi_slice_read_exact_`
  - 手段：让定长头/负载跨两片，覆盖 `read_exact_async_` 的
    `move_items_to_as_buff` 多片循环。
  - 判断：无短读错误，字节与预期一致。

### 4.3 容量与长值

- `ring_long_string_owned_path_`
  - 手段：容量 8 KiB，编码 > `STACK_BUF_`（256 字节）的字符串/二进制；
    解码走 owned 长值路径。
  - 判断：内容相等；不出现堆预拷贝导致的额外解析路径（通过行为断言，
    不做内存计数）。
- `ring_large_message_concurrent_pump_`
  - 手段：环容量 128、消息 4 KiB；生产者线程/任务编码，消费者线程/任务
    解码，`RingWriter::write_async` 在满时 park，`RingReader::read_async`
    在空时 park。
  - 判断：在 5 秒内完成；无死锁；值相等；任务正常退出。

### 4.4 多消息流与取消

- `ring_multi_message_stream_`
  - 手段：连续编码 3 个消息到同一环；复用同一个 `RingReader` 逐值构造
    解码器。
  - 判断：3 个值按顺序解出；环最终为空。
- `ring_cancel_while_reader_parked_`
  - 手段：环空时启动解码，等待 `read_async` park；用 `cantok_rs` 在
    20ms 后取消。
  - 判断：解码 future 以 `Err` 结束，测试不挂起。
- `ring_cancel_while_writer_full_`
  - 手段：环满时编码，等待 `write_async` park；取消令牌触发。
  - 判断：编码 future 以 `Err` 结束。

## 5. TCP / UDS socket 测试计划（socket_.rs）

优先用 `buffex_tokio_adapt`；smol 版本作为第二阶段。

### 5.1 TCP

- `tcp_roundtrip_`
  - 手段：`tokio::net::TcpListener` 接受回环连接；服务端用
    `BuffWrite::<_, DefaultAllocConfig>::try_new(stream, 4096, DefaultAllocConfig)`
    编码 `U64Val`，`close_async` 收尾；客户端用
    `BuffRead::try_new(stream, 4096, DefaultAllocConfig)` 直接解码。
  - 判断：解码值等于原值；与 `to_vec` 的紧凑字节一致。
- `tcp_bidirectional_concurrent_`
  - 手段：客户端与服务端同时各发一个值（`tokio::join!`），双向
    `BuffRead` / `BuffWrite`。
  - 判断：两个方向都解出正确值；无死锁。
- `tcp_fragmented_write_`
  - 手段：服务端分多次 `write_all` 小块并 `yield_now`/短暂 sleep，或直接
    用较小的适配器环，让客户端解码时多次 park/唤醒。
  - 判断：客户端按需读满，值正确。
- `tcp_large_payload_beyond_ring_capacity_`
  - 手段：适配器环 4 KiB，发送 64 KiB 字符串；`close_async` 驱动泵。
  - 判断：完整解码；不丢字节；在超时内完成。
- `tcp_multi_message_stream_`
  - 手段：同一连接连续发送 3 个值；客户端逐值解码。
  - 判断：顺序与内容正确；连接关闭后读到 EOF 错误。

### 5.2 Unix domain socket（`#[cfg(unix)]`）

- `uds_roundtrip_`
  - 手段：`tokio::net::UnixListener` + 临时路径（`std::env::temp_dir` 下
    唯一文件名；测试结束清理）；其余同 TCP。
  - 判断：值正确；socket 文件清理成功。
- `uds_cancel_partial_header_`
  - 手段：服务端只发送部分消息头后暂停；客户端解码并在 20ms 后取消。
  - 判断：返回 `Err`，不挂起；连接可安全关闭。

### 5.3 收尾语义

- `socket_flush_close_delivers_tail_`
  - 手段：编码后只调用 `flush_async` / `close_async`，不额外 sleep。
  - 判断：对端能解出完整值；close 后读端最终得到结束错误。
- `socket_write_error_maps_to_mp_error_`
  - 手段：关闭对端后再编码/收尾。
  - 判断：返回 `MpError`，消息包含底层错误信息，不 panic。

## 6. 第二阶段（可选）

- 用 `buffex_smol_adapt` + `async-net::unix::UnixStream` 复跑 UDS 往返，
  验证不绑定 tokio 运行时。
- 用 `buffex_compio_adapt` 跑最小往返，确认 `MpEncoder` / `MpDecoder`
  对 `TrBuffWrite` / `TrBuffRead` 的抽象没有隐含的 runtime 假设。
- 将 ring/socket 的公共断言抽到 `tests/support/mod.rs`，避免复制。

## 7. 验收标准

- `cargo test -p mp_denco --test ring_ --offline` 通过；
- `cargo test -p mp_denco --test socket_ --offline` 通过（Unix-only 用例
  在非 Unix 平台自动跳过）；
- 所有 park/取消用例有 5 秒超时，失败时给出明确阶段信息；
- `cargo clippy --workspace --all-targets` 无警告；
- 测试函数与关键 helper 使用中英双语注释，写明目标、手段、判断标准；
- 不引入 `unsafe` 借用扩展；ring/socket 一律走 `read_async` 的 owned 路径。

## 8. 风险与开放问题

1. **ring 容量限制**：`Ring::check_buffer_size` 有最小/最大容量，测试需
   选择合法容量并处理 `try_new` 的 `Err(cap)`。
2. **适配器收尾**：`BuffWrite` 的段 drop 只提交进环，不等于写入设备；
   编码后必须 `flush_async` / `close_async`，否则对端可能收不到尾部。
3. **多消息复用**：`MpDecoder` 每次消费一个 `&mut reader`，多消息测试
   直接复用同一个 reader 即可，不需要包装/解包。
4. ~~**borrowed 零拷贝与 ring**~~（已废弃）：借用读 trait、
   `OwnedRead` 与 `SegmRef::into_borrowed_slice` 已从 `abs_buff` 移除；
   `mp_denco` 不再尝试零拷贝借用，统一走 owned 解码。
5. **tokio 适配器依赖**：`buffex_tokio_adapt` 依赖 `abs_art` / `mm_ptr`
   等 git 仓库，需确认离线缓存与 `[patch]` 配置；若不可用，第一阶段可
   先用 `std::net` + 自制最小 `TrBuffRead`/`TrBuffWrite` 适配器，后续再
   接 tokio。
6. **平台差异**：UDS 测试必须 `#[cfg(unix)]`；Windows 上跳过并在测试输出
   中明确说明。

## 9. 实施记录（2026-10-18）

### 9.1 落地内容

- `mp_denco/tests/ring_.rs`（8 个用例）：
  - 基础往返与 rmp-serde 命名字节一致；
  - 容量 64 时跨环末端的两片段、`move_items_to_as_buff` 多片搬移；
  - 450 字节字符串与 300 字节二进制的 owned 长值路径；
  - 容量 128、消息 4 KiB 的生产者/消费者并发 pump；
  - 3 消息流复用同一个 `RingReader` 逐值解码；
  - 空环读 park、满环写 park 的取消，均以 `Err` 在 5 秒内结束。
- `mp_denco/tests/socket_.rs`（8 个用例）：
  - TCP 基础往返（`flush_async` + `close_async` 收尾）、双向并发、
    64 字节小环多次 park、64 KiB 超环容量负载、多消息流 + EOF；
  - 对端 RST 后写错误映射为 `MpError`（`SO_LINGER=0` 由 `socket2` 设置）；
  - UDS（`#[cfg(unix)]`）往返与部分头取消，临时 socket 文件由
    `Drop` 守卫清理；所有用例都包在 5 秒 `tokio::time::timeout` 里。
- `mp_denco/tests/support/mod.rs`：共享 `U64Val` / `StrVal` / `BytesVal`、
  `block_on_`、`with_timeout_`、ring 编解码与排空辅助。
- dev-dependencies：`buffex`、`buffex_tokio_adapt`、`tokio`、`socket2`；
  在离线缓存下 `cargo metadata` / 全量测试 / clippy 均可运行。

### 9.2 已修复：各运行时设备 I/O 与取消信号竞速

最初发现 `abs_buff_tokio_adapt::ReadAsInput::read_async` 的
`input_read_impl_async_` 把取消令牌参数命名为 `_token` 并完全忽略；
`BuffRead::wait_input_` 虽然把令牌传给 `move_items_from_input_async`，
但后者只在两次 `.await` 之间检查 `is_cancelled()`，设备 park 点不会被
取消唤醒。

在 `buffex.rs` 仓库的修复：

- `abs_buff_tokio_adapt` 的读、写设备适配器改用
  `abs_cancel::ok_or::XtOkOr`：`设备 I/O.ok_or(token.cancellation())`。
  `OkOr` 先 poll 设备 I/O，已完成的读写优先如实回报；只有设备仍
  `Pending` 时才让取消胜出，避免把「已经完成的写」误报成取消。
- 取消时分别返回 `ReadErrTag::Cancelled` / `WriteErrTag::Cancelled`。
- `buffex_tokio_adapt::BuffRead::record_end_` 不再把
  `ReadErrTag::Cancelled` 记成粘性设备错误，而是交给 `read_async` 循环
  返回 `ConsumerError::Cancelled`。
- `buffex_smol_adapt` / `buffex_compio_adapt` 的设备适配器（含 compio 的
  owned 变体）也统一改为 `设备 I/O.ok_or(token.cancellation())`。
- `buffex_tokio_adapt` / `buffex_smol_adapt` 的按需写泵把调用方令牌继续传给
  设备写：`write_async` / `flush_async` 用子令牌，`try_*` 与 `close_async`
  用 `NonCancellableToken`（关闭必须完成）；`WriteErrTag::Cancelled` 不再
  记成粘性设备错误。
- `abs_buff/tests/segm_async_io.rs` 的两个真实文件设备假实现也改为
  同样的竞速写法。
- 新增 `abs_buff_tokio_adapt` 的设备读写取消用例、`buffex_smol_adapt` 的
  设备单元用例、`buffex_compio_adapt` 的 `device_io_honours_cancellation_`
  集成用例，以及 `buffex_tokio_adapt` 的
  `buff_read_honours_cancellation_while_device_pending_` 回归用例。

`mp_denco` 的 `uds_cancel_partial_header_` 因此去掉了「取消后补 1 字节
唤醒」的规避，只发部分头并保持连接，客户端取消后立即以 `Err` 结束。

仍保留的已知约定：`close_async` 类入口继续忽略取消令牌，因为「关闭必须
完成」的原则不变（见 `buffex_tokio_adapt/dev-notes/write-20261002-0317.md`
第 6.3 条）；`buffex_iroh` 的 try_new/close 不 await 任何设备操作，也不在
竞速改造范围内。

## 10. 决策补充：取消借用内存反序列化（2026-10-18）

**结论**：`mp_denco` 不再支持借用内存反序列化；`abs_buff` 中为支持该能力
新增的 `TrBuffTryReadBorrowed`、`TrBuffReadBorrowed`、`OwnedRead`、
`NoopReclaim`、`SegmRef::into_borrowed_slice`、`&'de [u8]` 借用实现及其测试
已移除。`MpDecoder<'r, R>` 去掉 `'de` 标记，只要求 `R: TrBuffRead<u8>`。

### 10.1 背景

原设计让 `MpDecoder` 先尝试 `try_read_borrowed` / `read_borrowed`，成功时把
`&'de str` / `&'de [u8]` 交给 visitor 的 `visit_borrowed_str` /
`visit_borrowed_bytes`；读端不支持借用（如 ring、socket 适配器）时必须由
调用者包一层 `OwnedRead`，否则无法满足 `TrBuffReadBorrowed<'de>` 约束。

### 10.2 决策与理由

- **反直觉且危险**：工作目录下绝大多数项目使用可复用缓冲，调用者必须记得
  包装 `OwnedRead` 才能解码；一旦遗漏或选错 `'de`，借用路径的约束很难从
  API 上直观判断。
- **收益有限**：只有稳定切片读端能真正零拷贝，而 ring/socket 场景本来就要
  复制成 owned 值；为少数场景保留一套 unsafe 借用 trait 不划算。
- **保留抽象层**：`abs_denco` 的 `'de` 与 `visit_borrowed_*` 仍保留，供将来
  自定义稳定读端直接实现 `TrDecoder` 时使用；`mp_denco` 不再调用它们。

### 10.3 实施内容

- `mp_denco/src/de_.rs`：
  - 删除 `read_borrowed_segm_async_` 与 `NameRef::Borrowed`；
  - `MpDecoder`、各访问器不再携带 `'de` 标记，边界统一为
    `R: TrBuffRead<u8>`；
  - 字符串/二进制统一走 `visit_str` / `visit_string` / `visit_bytes` /
    `visit_byte_buf`：短值栈数组，长值 `String` / `Vec<u8>`。
- `mp_denco/tests/`：删除 borrowed/`OwnedRead` 用例与自制定制借用读端；
  ring/socket 直接解码 `RingReader` / `BuffRead`。
- `buffex.rs/abs_buff`：回退提交 `c33efd6` 的借用读能力，README 同步删除
  “Stable borrowed reads” 一节。

### 10.4 后果

- `MpDecoder` 的公开类型少一个生命周期参数，调用者不再需要构造 `'de`，
  也不需要 `OwnedRead`。
- 解码字符串/二进制总是分配或复制；这是为 API 简单性与安全性做的取舍。
- `abs_denco` 的 `'de` 抽象保持不变，因此本次不涉及 `abs_denco` 的公开 API
  变更。
