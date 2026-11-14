# 给 denco 增加可选的 TypeId（结构化类型指纹）

**结论（2026-11-14 第二轮修订）**：在 `abs_denco` + `denco_macro` 增加**默认关闭**的
`type-id` 特性。类型侧 trait 为 `TrHasTypeId`，ID 数值类型为 `TrTypeId`（u128 / u64 /
u32 三宽度）。**三个宽度互相独立**：32/64/128 位各用原生宽度哈希（FNV-1a 32/64/128；
`type-id-murmur3` 时 Murmur3 x86_32 / MurmurHash64A / x64_128），不是 128 位值的截断。
`TrHasTypeId` 是**显式 derive**；`TrEncodable` 不再附带生成。递归/手写/固定 ID 直接
手写 `impl TrHasTypeId`（可配合 `path_id_32/64/128`），没有 `skip_type_id` 或
`type_id = ...` 属性。**本轮只做 `denco`**；`mptp_rpc` 的线上 ID 破坏已确认接受，
迁移放到下一轮。

- 仓库：`denco`（本轮）、`mptp_rpc`（下一轮集成）、`mptp_im_demo`（验收）
- 基线：`denco` `60a2736`；`mptp_rpc` `7bb64f0`（或其后）；`smux_v1` `90d9634`
- 状态：**denco 部分已按第二轮方案实现（`feat/gen-typeid`）；mptp 迁移待下一轮**
- 关联：`mptp_im_demo/DESIGN.md` §4.4.1、`mptp_im_demo/ACCEPTANCE.md` **G-16**、
  `mptp_im_demo/crates/im_protocol/src/ids.rs`（当前手写表）
- 先读：本目录 `README.md`、`2026-10-18-derive-macros.md`、`2026-10-19-struct-serialization-config.md`、
  `2026-11-04-remove-de.md`、`abs_denco/src/{ser,de}` 的 trait 与 impls、`denco_macro/src/`

---

## 0. 任务性质与硬约束

1. **先提案、后写码**：本任务涉及新公开 trait / 新公开类型 / 新 feature / 可能的
   线上 ID 破坏。按 `old-cantare/AGENTS.md` 第 1 条，先写提案（放本目录）并等用户
   确认；确认前只允许写**测试**与不改公开面的原型。
2. **特性默认关闭**：`denco` 不开该特性时，公开 API、生成代码、编译时间不得变化。
3. **`no_std` + 编译期**：TypeId 必须是 `const`，不依赖堆、运行时、`std::hash`
   （`DefaultHasher` 不保证跨版本稳定，禁止）；不引入 `unsafe`（除非有完整论证）。
4. **算法一旦发布就是兼容性契约**：散列输入、顺序、分隔、版本前缀的任何改动都会改变
   所有 ID；必须定义版本策略与 golden 测试。
5. **`mptp_rpc` 的线上兼容性必须显式决策**：把 `Data_Type_Id` 从手写 `0x1001…` 换成
   生成值会改变线上头；是否接受由用户确认。若不能破坏，需要显式 override 机制。
6. **测试先行**：先写会失败的测试（feature 关闭 / 开启、稳定性、结构差异、跨 crate），
   再实现；测试与实现分开提交。
7. **产出物位置**：设计提案与实施记录都追加到本文（denco dev-notes 约定：同一天同一
   主题追加，不另起互相矛盾的文件）；`mptp_rpc` 侧的迁移说明放
   `mptp_rpc/dev-notes/`。

---

## 1. 背景与问题（为什么做）

`mptp_rpc` 的 `rpc_core_v1::stream::TrDataTypeId` 现在要求每个线上 DTO 手写：

```rust
impl TrDataTypeId for User {
    const DATA_TYPE_ID: DataTypeId = DataTypeId::new(0x1001).unwrap();
}
```

`mptp_im_demo` 因此有一张手写表（`0x1001`–`0x1009`）。这有两个问题（ACCEPTANCE G-16）：

- 手写 ID 跨 crate 无法检测冲突；
- ID 没有单一事实来源，编译单元 / feature 组合变化时容易漂移。

目标：ID 由**类型的定义本身**决定——全路径 + 内部结构——这样：

- 同一类型在任何 crate / 任何编译顺序下得到同一个 ID；
- 结构变化（加字段、改字段类型、改变体）自动改变 ID；
- 冲突概率由散列宽度控制，并可在测试期用注册表检测。

---

## 2. 必须先定的设计问题（提案里逐项回答）

### 2.1 trait 形状：独立 trait 还是 `TrEncodable` 的 supertrait？

候选：

- **A（低风险备选）**：独立 `pub trait TrTypeId { const TYPE_ID: TypeId; }`；
  `type-id` 特性开启时，derive 额外生成 `impl TrTypeId`。
  - 优点：不破坏现有手写 impl；`TrEncodable`/`TrDecodable` 的形状不变；
  - 缺点：不是「每个 `TrEncodable` 都自动有 ID」，手写 impl 需要自己补。
- **B（最贴合本任务表述）**：`type-id` 开启时让 `TrEncodable: TrTypeId`
  （`TrDecodable` 同理）——「每一个 TrEncodable/TrDecodable 的类型」都自动有 ID。
  - 优点：语义上无法漏；
  - 缺点：特性开启后所有手写 impl（含下游）都必须补 `TrTypeId`，是**破坏性**的
    （仅在该 feature 下）；`TrDecodable` 带 `'alloc`/`Alloc` 参数，super trait 关系
    要确认不会引入额外约束。**选 B 必须在提案里写清破坏面与迁移方式。**
- **C**：不碰现有 derive，新增 `#[derive(TrTypeId)]`，由用户显式选择。
  - 优点：最可控、最容易 feature gate；
  - 缺点：用户可能忘了给某类型加，`Data_Type_Id` 覆盖不完整。

提案必须选一个，并说明「如何保证 mptp_rpc 的每个线上类型都有 ID」。

### 2.2 `TypeId` 的宽度与类型

- 候选：`u64`、`[u8; 16]`、`[u8; 32]`。
- 约束：
  - `mptp_rpc` 的 `DataTypeId` 是**非零 `u64`**、直接进线上头；若 TypeId 更宽，
    需要定义映射（截断？派生第二个 64 位？还是改 `DataTypeId` 宽度——后者是公开 API
    变更，必须用户确认）。
  - 冲突概率与宽度：128 位更稳；64 位在几万类型内也够用，但要写清 birthday 估算。
- 提案需回答：TypeId 的规范类型、与 `DataTypeId` 的映射、0 值处理。

### 2.3 散列输入的**精确语法**（本任务最核心）

必须给出一个**无歧义、可规范化**的字节串语法，至少覆盖：

- 定义处全路径：`module_path!()` + `::` + 类型名（注意 `module_path!()` 在哪个 crate
  展开；derive 生成在定义处，不能用调用处的路径）；
- 结构体：种类标签 + 字段数 + 每个字段的**线上名字**（`#[denco(rename)]` 之后）+
  字段类型 ID；字段顺序参与；
- 枚举：种类标签 + 变体数 + 每个变体的**线上名字** + 载荷形状；变体**顺序不参与**
  （线上按名字），但名字 / 载荷变化参与；
- 泛型：类型参数用其实参的 TypeId；生命周期与 allocator 参数是否参与（建议不参与，
  因为线上等价）；`const` 泛型（如 `[T; N]` 的 `N`）参与；
- `#[denco(transparent)]`：建议**等于内层类型 ID**（线上等价），还是保留包装路径？
  需拍板；
- 字段属性：`skip` / `skip_encoding` / `skip_decoding` / `default` / `alias` 是否参与；
  注意方向不对称的 skip 会让编码 / 解码 schema 不同，一个 TypeId 是否足够？
- 引用 / 容器等价：`&str` 与 `String`、`&[T]` 与 `Vec<T>`、`Box<T>` 与 `T`、`Rc/Arc<T>`
  与 `T` 是否应得到相同 ID（按线上等价建议相同）；`Option<T>` 与 `T` 必须不同；
- 定长数组 `[T; N]` vs `Vec<T>`：不同；
- 结构体命名映射配置（`with_struct_map`）是**编码期**配置，不是类型结构；TypeId 不应
  随它变化，但解码同时接受两种表示，提案要说明这个选择；
- 每个段的长度前缀 / 标签，防止 `"ab"+"c"` 与 `"a"+"bc"` 碰撞；
- 版本前缀（如 `b"denco-type-id-v1"`），算法变更时递增。

提案里请直接给出**伪代码 / 语法 BNF**，并配 4–6 个具体类型的期望输入（例如
`Option<Vec<u64>>`、带 `rename` 的结构体、带 `skip` 的枚举、`Foo<u8>` / `Foo<u16>`）。

### 2.4 外部选择 hash 的方式

要求：默认 FNV，外部可在**编译期**选择。候选：

- **A**：cargo feature（如 `type-id` = FNV-1a 128；`type-id-fx` / `type-id-crc64` 互斥）；
  简单，但换 hash 是全局破坏性变更，必须文档化。
- **B**：`#[denco(type_id_hash = "fnv" | "fx")]` 类型级属性，宏据此选择 `abs_denco` 里
  对应的 `const fn`；粒度更细，但同一 workspace 混用会让排序 / 注册表复杂。
- **C**：trait 泛型参数 `TrTypeId<H = Fnv1a>`；最灵活，但会让「每个类型都有唯一
  TypeId」的写法（`T::TYPE_ID`）出现歧义，且 supertrait 方案会变复杂。

提案必须选一个；若无强需求，建议 **A**（feature，默认 FNV-1a），并说明为什么不用 B/C。
所有候选 hash 必须能用 `const fn` 实现、与目标位宽 / 字节序无关。

### 2.5 feature 的 Cargo 契约（容易被忽略）

proc-macro 看不到下游依赖的 feature。若 derive 生成 `TrTypeId`，而 `abs_denco` 没开
`type-id`，生成代码会找不到 trait。请给出可执行的方案，候选：

- `abs_denco/type-id` + `denco_macro/type-id` **必须同时开启**，并在 README / 生成的
  代码里给出清晰编译错误（例如生成一个只在 feature 开启时存在的 helper 类型）；
- 或者把 trait 无条件放进 `abs_denco`，feature 只控制 hash 实现 / 注册表 / impl 生成；
- 或者独立 `#[derive(TrTypeId)]`（`denco_macro/type-id`），由用户显式加；
- 评估 `proc-macro-crate` 能找到 crate 名但**找不到 feature** 的限制，不要假装能自动
  检测。

### 2.6 冲突检测与注册表

- 128 位散列碰撞概率极低，但 G-16 明确要求「检测冲突」。
- 候选：
  - 测试期注册表：每个 crate 用宏生成 `pub const __DENCO_TYPE_IDS: &[(TypeId, &str)]`，
    workspace 的测试把它们汇总，重复即失败；
  - 可选 feature `type-id-registry`，提供 `TypeIdRegistry`（`no_std` + alloc 或 `std`）；
  - 编译期不可行（跨 crate 收集），不要承诺 link-time 注册。
- 提案要说明：注册表放哪、谁调用、mptp_rpc / demo 如何用它替代现有
  `type_ids_are_unique_` 测试。

### 2.7 稳定性与版本

- 同一输入必须跨 32/64 位、跨编译器版本、跨目标端得到同一 ID；
- 禁止依赖 `usize` / 本机端序 / `DefaultHasher` / `RandomState` / 时间 / 地址；
- ID 算法版本（前缀）与 golden 测试；任何输入语法变更都必须是新版本 + 全量 golden
  更新 + semver 说明。
- 文档里写清：**修改字段名 / 类型 / 顺序 / 变体名 = 新 TypeId**；重排枚举变体（名字
  不变）= 同一 TypeId（若按名字哈希）。

---

## 3. 实施地图（供提案引用；不要直接开写）

### 3.1 `denco`（主仓库）

| 位置 | 预计改动 |
| --- | --- |
| `abs_denco/Cargo.toml` | 新增 `type-id`（及可能的 hash 子 feature）；默认关闭 |
| `abs_denco/src/lib.rs` | `pub mod type_id`（feature-gated）；文档 / feature 表更新 |
| `abs_denco/src/type_id/`（新） | `TypeId`、`TrTypeId`、`const fn` hash、规范输入构造器、基础 type 的 impl |
| `abs_denco/src/ser/impls_.rs`、`de/impls_.rs`、`*/alloc_collections_`、`std_collections_`、`rc_`、`cow_`、`str_alloc_` | 为已有编解码类型补 `TrTypeId`（feature-gated） |
| `denco_macro/Cargo.toml` | 对应的 `type-id` feature（与 abs_denco 同步开启的契约） |
| `denco_macro/src/lib.rs` / `model_.rs` / `attrs_.rs` / 新 `type_id_.rs` | 解析结构 / 属性，生成 `impl TrTypeId`（或 supertrait 所需代码） |
| `denco_macro/tests/`、`mp_denco/tests/`、`abs_denco` 单测 | 见 §4 测试矩阵 |
| `dev-notes/` | 本提案 + 实施记录；README 无需索引表（仅约定） |

### 3.2 `mptp_rpc`

- `rpc_core_v1/Cargo.toml`：`abs_denco` 打开 `type-id`（`denco_macro` 同步）；
- `rpc_core_v1/src/stream/types_.rs`：`DataTypeId` / `TrDataTypeId` 如何对接
  `TrTypeId`（提案决定：blanket impl / 显式 From / 保留手写）；
- `mp_denco`（若在 rpc 侧有派生类型）：确保 feature 打开；
- 相关测试：ID 唯一性 / golden / 类型头 400/406 不受影响；
- **线上兼容**：现有 `0x1001`–`0x1009` 是否保留 override？若保留，新增
  `#[denco(type_id = 0x...)]`（或等价）并只对旧类型使用；若接受破坏，更新
  `rpc_core_v1` / `mptp_serving` / demo 的测试与文档，并在 ACCEPTANCE 里写明。

### 3.3 `mptp_im_demo`

- 删除或改造 `im_protocol/src/ids.rs` 的手写表（取决于 §3.2 的兼容决策）；
- 更新 G-16 / DESIGN §4.4.1 / README；
- 全量场景（13 个）必须继续通过；类型头 406 / 大 item / Pull/Push 不受 ID 变更影响
  的部分要明确回归。

---

## 4. 必须先写的测试（feature 开启 / 关闭各一套）

### 4.1 基础语义（`abs_denco` 单测，feature-gated）

1. **同类型同 ID**：同一类型在两次编译 / 两个模块引用得到同一常量。
2. **结构敏感性**：
   - 加字段 / 删字段 / 改字段类型 → ID 变；
   - 字段顺序交换 → ID 变（线上位置变了）；
   - `#[denco(rename = ...)]` 改线上名 → ID 变；
   - 仅改 Rust 字段名（无 `rename`，位置编码）→ 按提案决定（建议：不变，因为线上
     位置未变；若决定把 Rust 名也纳入，需说明这是「严格指纹」而非「线上兼容」）。
3. **枚举**：重排变体 → ID 不变（按名字）；改变体名 / 载荷类型 / 增加变体 → ID 变。
4. **泛型**：`Foo<u8>` ≠ `Foo<u16>`；`Foo<Vec<u8>>` ≠ `Foo<Vec<u16>>`；
   lifetime / allocator 参数不影响 ID。
5. **等价性**：`&str` = `String`、`&[T]` = `Vec<T>`、`Box<T>` = `Rc<T>` = `T`
   （按线上等价，若提案选择如此）；`Option<T>` ≠ `T`。
6. **属性**：`skip` / `skip_encoding` / `skip_decoding` / `default` / `alias` /
   `transparent` 的行为与提案一致（每个属性至少一个用例）。
7. **规范输入无歧义**：构造两个「拼接后相同」的字段名 / 类型名，断言 ID 不同
   （长度前缀 / 标签生效）。
8. **golden**：4–6 个固定类型 + 硬编码 ID（或 ID 的十六进制），作为算法稳定性契约。

### 4.2 feature / 编译面

9. feature 关闭：`cargo check -p abs_denco --no-default-features --features str_alloc`
   以及 `mp_denco` / `denco_macro` 的默认构建不受影响；不生成任何 `TrTypeId` 代码。
10. feature 开启：`cargo test -p abs_denco --features type-id`、`cargo test -p mp_denco ...`
    全绿；`no_std` 构建通过。
11. feature 契约：只开一半（abs_denco 开、denco_macro 没开，或反之）时给出**可读的
    编译错误**（最好有 compile-fail / trybuild 用例），而不是一堆「找不到 trait」。

### 4.3 跨 crate / 冲突

12. 类型定义在 crate A、在 crate B 读取 `TYPE_ID` → 相同（需要 test-support crate 或
    现有 crate 间依赖；提案里说明用哪个）。
13. 注册表：汇总所有测试类型的 `(TypeId, name)`，断言无重复；mptp_rpc 用它替代
    `type_ids_are_unique_`。
14. 不同 hash feature 的 ID 不同（若实现多个 hash）；同一类型在同一 hash 下稳定。

### 4.4 `mptp_rpc` / demo 集成

15. `cargo test -p rpc_core_v1 -p mptp_serving -p rpc_svc_macro`（feature 开启）。
16. `cargo test -p im_testkit --test scenarios`：13 个场景全绿。
17. 类型头测试：`e13_push_type_mismatch_is_406_` / `pull_wrong_accept_is_406_` 仍通过；
    若 ID 改变，更新测试里硬编码的 `0x9999` 之类（只改测试值，不改语义）。
18. 若保留旧 ID：断言 `User::DATA_TYPE_ID == 0x1001` 等 9 个旧值不变（golden）。

测试注释按 `old-cantare/AGENTS.md` 第 2 条：中英双语，写清目标 / 手段 / 判定。

---

## 5. 验收标准

1. 提案已确认；提案包含 §2 的每个决策、输入语法、feature 契约、兼容性结论。
2. §4 的测试全部通过；feature 关闭时 `denco` 的现有测试与公开 API 无变化。
3. `cargo test --workspace`（denco）、`cargo clippy --workspace --all-targets -- -D warnings`、
   `python3 ../../format_use_imports.py --check`（按仓库位置）全绿。
4. `mptp_rpc` 打开 feature 后：`rpc_core_v1` / `mptp_serving` / `rpc_svc_macro` /
   `mptp_smux_v1_demo` 测试全绿；`mptp_im_demo` 13 场景全绿。
5. 公开接口变更清单（denco / mptp_rpc）逐项列出并经确认；`mptp_rpc` 的线上 ID 变更
   有明确的迁移说明或 override 证据。
6. 无 `DefaultHasher`、无运行时注册、无 `unsafe`（除论证过的投影）、无 `Vec` 暂存。
7. 实施记录回填 `denco/dev-notes/`，并更新 `mptp_im_demo/ACCEPTANCE.md` G-16 状态。

---

## 6. 不要做

1. **不要**直接用 `std::any::type_name` / `DefaultHasher` / `RandomState` / 地址 /
   时间来生成 ID——它们跨编译 / 跨版本不稳定，且 `no_std` 不可用。
2. **不要**在 `const` 里用 `Vec` / `String` / 堆分配；用固定大小的 builder 与
   `const fn`。
3. **不要**把 `module_path!()` 放在调用处展开；必须在**定义处**（derive 生成的
   impl 内）展开。
4. **不要**让 ID 依赖 `usize`、本机端序或 `#[cfg(target_pointer_width)]`。
5. **不要**在 feature 关闭时留下公开 trait / 生成代码 / 编译开销。
6. **不要**在没有用户确认的情况下改变 `mptp_rpc` 的公开 API 或线上 `Data_Type_Id`
   语义；需要时先写提案。
7. **不要**只测「同类型同 ID」；必须覆盖结构差异、泛型、属性、等价类与 golden。
8. **不要**把 `mptp_im_demo` 的手写 ID 直接换成生成值而不更新 golden / 兼容性说明。

---

## 7. 参考

- `mptp_im_demo/DESIGN.md` §4.4.1（ID 生成原则与提案草案）
- `mptp_im_demo/ACCEPTANCE.md` G-16 / A-06
- `denco/dev-notes/2026-10-18-derive-macros.md`、`2026-10-19-struct-serialization-config.md`、
  `2026-11-04-remove-de.md`
- `abs_denco/src/{ser/encodable_.rs,de/decodable_.rs}`、`abs_denco/src/ser/impls_.rs`、
  `abs_denco/src/de/impls_.rs`
- `denco_macro/src/{lib.rs,model_.rs,attrs_.rs,encode_.rs,decode_.rs}`
- `rpc_core_v1/src/stream/types_.rs`（`DataTypeId` / `TrDataTypeId` / 类型头校验）
- `mptp_im_demo/crates/im_protocol/src/ids.rs`（当前手写表）

---

## 8. 设计决策（2026-11-14 第一轮讨论）——部分已被 §10 取代

> 本节记录第一轮讨论。§10 修订了 derive 方式、三宽度生成、hash 集合与属性；
> 与 §10 冲突的部分以 §10 为准（§8.2、§8.3、§8.4、§8.5、§8.7）。

### 8.1 总体结论

- 新增两个 **默认关闭** 的 feature：`abs_denco/type-id`、`denco_macro/type-id`；
  哈希算法再由 `abs_denco/type-id-murmur3` 选择（默认 FNV-1a 128）。
- 类型侧 trait 是 `TrHasTypeId`；ID 数值类型由 `TrTypeId`（u128 / u64 / u32）表达。
- `#[derive(TrEncodable)]` 在 `denco_macro/type-id` 开启时额外生成
  `impl TrHasTypeId`；`TrDecodable` 的 derive 不生成（避免两个 derive 冲突）。
- 哈希输入 = 定义处 `module_path!()` + Rust 类型名 + 线上名 + 字段/变体结构。
- 递归类型用 `#[denco(skip_type_id)]` + 手写 `path_id_128`；显式线上 ID 用
  `#[denco(type_id = ...)]`。
- 本轮只改 `denco`；`mptp_rpc` 的 `Data_Type_Id` 迁移是下一轮，已确认接受破坏。

### 8.2 为什么是独立 `TrHasTypeId`，不是 supertrait

- `TrEncodable: TrHasTypeId` 需要在 `cfg` 下复制 trait 定义；开启 feature 后所有手写
  impl（含下游）都必须补 ID，破坏面不可控。
- `TrDecodable` 带 `'alloc` / `Alloc` 参数，supertrait 还会把 decode-only 类型
  （`&'alloc mut T`、`IgnoredAny` 等）拖进来。
- 两个 codec derive 都会被迫生成 `impl TrHasTypeId`，同时派生时冲突；最终仍要
  “只让 `TrEncodable` 生成”，B 的“无法漏”优势不成立。
- 选独立 trait 后，手写 codec 类型（mptp 里只有 `Headers`、`U16OrStr`）按需补
  `impl TrHasTypeId` 即可，不开启 feature 时零变化。

### 8.3 宽度与零值

```rust
pub trait TrTypeId: Copy + Eq + Ord + core::hash::Hash + core::fmt::Debug + core::fmt::Display {
    const BITS: u32;
    fn from_u128(value: u128) -> Self; // 纯低位截断
    fn to_u128(self) -> u128;          // 零扩展
}

pub trait TrHasTypeId {
    const TYPE_ID_128: u128; // 规范值；实现者保证非零
    const TYPE_ID_64: u64 = nonzero_64(Self::TYPE_ID_128 as u64);
    const TYPE_ID_32: u32 = nonzero_32(Self::TYPE_ID_128 as u32);
    const TYPE_NAME: &'static str;
}
```

- 128 位是唯一规范值；64/32 是低位截断后各自做 `0 → 1` 映射。这样每个宽度都满足
  `DataTypeId` 之类“非零”消费方的要求。
- `TYPE_ID_64` / `TYPE_ID_32` 有默认值，derive 只写 `TYPE_ID_128` 与 `TYPE_NAME`；
  透明类型、引用、容器转发时显式转发三个宽度，避免内层自定义 64/32 时丢失。
- 32 位只适合字段本来就是 u32 的协议：1 万类型时生日碰撞约 1.2%，registry 必须按
  实际使用的宽度检查。
- `TrTypeId` 不 seal；u32/u64/u128 之外允许用户为自己的 width newtype 实现。

### 8.4 哈希算法

| feature | 算法 | 规范输出 | 说明 |
| --- | --- | --- | --- |
| `type-id`（默认） | FNV-1a 128 | 128 位 | 常量来自 [draft-eastlake-fnv-10 §5](https://datatracker.ietf.org/doc/html/draft-eastlake-fnv-10)；const 实现最短；低位雪崩偏弱 |
| `type-id-murmur3` | MurmurHash3 x64_128 | 128 位 | 参考 [MurmurHash3.cpp](https://github.com/aappleby/smhasher/blob/master/src/MurmurHash3.cpp)（public domain）；固定小端读取，输出 `h1 \| (h2 << 64)` |

- **优先级**：同时开启时 `type-id-murmur3` 覆盖 FNV（保证 `--all-features` 可编译）；
  生产环境必须只开一个 hash feature。v1 不引入 Fx（rustc 的 Fx 用 `usize`，跨
  32/64 位不稳定，低位质量弱）与 XXH3（const 实现过大）。
- 版本前缀把算法名纳入：`denco/type-id/1/fnv1a-128\0`、
  `denco/type-id/1/murmur3-x64-128-le\0`。算法和输入语法一样是兼容性契约。
- 公开 `TYPE_ID_VERSION: u32 = 1` 与 `HASH_ALGORITHM: &'static str`，测试/诊断可按
  算法分支。
- **`denco_macro` 不需要知道 hash feature**：宏只调用隐藏的 `Hasher`，算法选择全在
  `abs_denco` 内；只保留 `type-id` 代码生成这一重 feature 契约。

### 8.5 输入语法 v1（字节级）

所有片段都经过 `Hasher` 的统一 framing，避免 `"ab"+"c"` 与 `"a"+"bc"` 之类的拼接歧义：

```text
input   = b"denco/type-id/1/" || algorithm || b"\0" || segment*
segment = 0x01 || u32le(len) || utf8        // str
        | 0x02 || byte                      // u8：schema tag / 方向位 / 变体形状
        | 0x03 || u32le(value)              // u32：字段数、变体数
        | 0x04 || u64le(value)              // u64：const 泛型 N
        | 0x05 || u128le(value)             // 字段类型的 TYPE_ID_128

schema  = 0x00 unit
        | 0x01 bool | 0x02 i8 | 0x03 i16 | 0x04 i32 | 0x05 i64 | 0x06 i128
        | 0x07 u8   | 0x08 u16 | 0x09 u32 | 0x0a u64 | 0x0b u128
        | 0x0c f32  | 0x0d f64 | 0x0e char
        | 0x10 str
        | 0x11 seq(id(T))
        | 0x12 array(id(T), u64(N))
        | 0x13 option(id(T))
        | 0x14 map(id(K), id(V))
        | 0x15 set(id(T))
        | 0x20 named_struct(path, wire_name, u32(count), field*)
        | 0x21 tuple_struct(path, wire_name, u32(count), field*)
        | 0x22 newtype_struct(path, wire_name, dir, id(inner))
        | 0x23 unit_struct(path, wire_name)
        | 0x24 enum(path, wire_name, u32(count), variant*)   // 按 wire_name 排序
        | 0x40 path_only(path)

path       = str(module_path!()) || str(rust_ident)
wire_name  = str(rename 或 rust_ident)
field      = [str(wire_name) 仅命名字段] || u8(dir) || id(field_ty)
dir        = 0b11 双向 | 0b10 仅解码(skip_encoding) | 0b01 仅编码(skip_decoding)
id(T)      = u128le(<T as TrHasTypeId>::TYPE_ID_128)

variant    = str(wire_name)
           || u8(shape: 0 unit / 1 newtype / 2 tuple / 3 named)
           || newtype: u8(dir) || id(ty)
           || tuple:   u32(count) || (u8(dir) || id(ty))*
           || named:   u32(count) || (str(name) || u8(dir) || id(ty))*
           || unit:    （无载荷）
```

- `dir` 的 bit0 = 参与编码，bit1 = 参与解码；`skip` 双向字段直接不出现。
- 枚举变体先按线上名字节序排序再写；结构体字段保持声明顺序。
- `#[denco(transparent)]` 只写 `id(inner)`，不写包装层；`TYPE_NAME` 仍取包装类型路径。
- `path_id_128(module_path, ident)` 写 `0x40` 段 + 两个字符串，是递归/手写类型的入口；
  它只含路径，不含结构，因此会失去嵌套结构敏感性。
- `Hasher::finish` 返回 `0 → 1` 映射后的非零 128 位值。

**边界**：递归类型（直接或互相递归）会在 const 求值时触发 `E0391`；必须用
`#[denco(skip_type_id)]` 手写。深哈希意味着嵌套类型结构变化会传播到外层 ID。

### 8.6 规范化取舍

| 项 | 决策 |
| --- | --- |
| `str` / `String` / `abs_str::String<A>` | 同一 `str` schema |
| `&[T]` / `Vec<T>` / `VecDeque` / `LinkedList` / `BinaryHeap` | 同一 `seq(T)` |
| `BTreeMap` / `HashMap` | 同一 `map(K,V)`；`BTreeSet` / `HashSet` 同一 `set(T)` |
| `Box<T>` / `Rc<T>` / `Arc<T>` / `&T` / `&mut T` / `Cow<T>` | 转发到 `T` |
| `[T; N]` vs `Vec<T>` | 不同（`array` vs `seq`） |
| `Option<T>` vs `T` | 不同 |
| `transparent` | 等于内层 |
| 结构体字段顺序 | 参与 |
| 枚举变体顺序 | 不参与（按名字排序） |
| `rename` / `rename_all` | 参与（生效后的线上名） |
| 仅改 Rust 字段名 | **参与**（严格指纹；位置编码下也会改 ID） |
| `alias` / `default` | 不参与 |
| `skip` / `skip_encoding` / `skip_decoding` | 见 §8.5 的方向位 |
| `with_struct_map` | 不参与；ID 是类型级 schema |

### 8.7 新增属性（`denco_macro/type-id` 开启时）

```rust
#[denco(skip_type_id)]          // 抑制自动 impl，手写 TrHasTypeId
#[denco(type_id = 0x1001)]      // 显式覆盖 TYPE_ID_128；非零；与 skip_type_id 互斥
```

- `skip_type_id` 只影响 ID 生成，不影响编解码。
- `type_id = ...` 接受 `syn::LitInt`（十进制/十六进制/下划线）；值即
  `TYPE_ID_128`，64/32 由默认截断给出；0 直接编译错误。
- 两个属性都只在 `denco_macro/type-id` 开启时被接受；关闭时报“未知属性”，避免静默
  无效。

### 8.8 Feature 契约

- `abs_denco/type-id` 开启 `pub mod type_id`；`denco_macro/type-id` 开启代码生成。
- 两边必须同时开启。只开 abs：`TrHasTypeId` 上的
  `#[diagnostic::on_unimplemented]` 提示去开 `denco_macro/type-id`。只开 macro：
  rustc 会指出 `abs_denco::type_id` 被 cfg 掉，并提示 `type-id` feature。
- mptp 等下游用 facade feature 一次性转发，例如
  `rpc_core_v1/type-id = ["abs_denco/type-id", "denco_macro/type-id"]`。
- 只有 abs_denco 的 hash feature 影响 ID；`denco_macro` 不需要对应的 hash feature。

### 8.9 冲突注册表

```rust
pub struct TypeIdEntry {
    pub id_128: u128,
    pub id_64: u64,
    pub id_32: u32,
    pub name: &'static str,
}
pub struct TypeIdConflict { pub width: u32, pub id_128: u128, pub first: &'static str, pub second: &'static str }
pub fn check_unique(entries: &[TypeIdEntry], width: u32) -> Result<(), TypeIdConflict>;
```

- `denco_type_ids! { A, B, ... }` 生成 `&[TypeIdEntry]`；每个 crate 暴露自己的清单，
  workspace 测试汇总（不做 link-time/自动跨 crate 收集）。
- `check_unique` 按调用方实际使用的 32/64/128 宽度做 O(n²) 比较；同 ID 即冲突。
  等价类型（`Box<T>` 与 `T`、transparent 包装）只登记一个。
- mptp 用 `check_unique(entries, 64)` 替换现有 `type_ids_are_unique_`。

### 8.10 与 mptp 的兼容性

- 用户已拍板：**接受线上 `Data_Type_Id` 破坏**，删除 `im_protocol` 手写表与 12 处
  手写 `TrDataTypeId`（9 个 DTO + 3 个 example）。
- 下一轮在 `rpc_core_v1` 加 `type-id` facade feature，并用
  `impl<T: TrHasTypeId> TrDataTypeId for T { const DATA_TYPE_ID = ...TYPE_ID_64... }`
  替换手写 impl；golden / README / ACCEPTANCE(G-16) 全量更新。
- mptp_rpc 通过 git 依赖 denco；联调前需要推送 `feat/gen-typeid` 或临时 `[patch]`。
  本轮只做 `denco`。

### 8.11 公开 API 变更清单（待随实现提交）

| crate | 新增 |
| --- | --- |
| `abs_denco` | feature `type-id`、`type-id-murmur3`；`pub mod type_id`：`TrTypeId`、`TrHasTypeId`、`path_id_128`、`TypeIdEntry`、`TypeIdConflict`、`check_unique`、`TYPE_ID_VERSION`、`HASH_ALGORITHM`、`denco_type_ids!`；`#[doc(hidden)] __private::Hasher` |
| `denco_macro` | feature `type-id`；`TrEncodable` derive 额外生成 `impl TrHasTypeId`；类型属性 `skip_type_id`、`type_id = ...` |
| 下游（下一轮） | `rpc_core_v1/type-id` facade feature；`TrDataTypeId` blanket impl |

### 8.12 测试与实施顺序

1. 提案（本节）提交。
2. **测试先行**（单独提交）：`abs_denco` 的 hash 向量、语义/等价类/golden（FNV 与
   Murmur3 各一套）、registry；`denco_macro` 的 derive/属性/递归用例。
3. `abs_denco` 实现：`type_id` 模块、`Hasher`（FNV/Murmur3 流式）、基础类型 impls、
   registry。
4. `denco_macro` 实现：`type_id_` 生成器、属性解析、model 字段。
5. 文档 / feature 表 / README。
6. 验证：
   - `timeout 60 cargo test -p abs_denco --features type-id`
   - `timeout 60 cargo test -p abs_denco --features type-id-murmur3`
   - `timeout 60 cargo test -p denco_macro --features type-id`
   - `timeout 300 cargo test --workspace`
   - `cargo clippy --workspace --all-targets --all-features -- -D warnings`
   - `cargo test --workspace --all-features`（murmur3 优先）
   - `python3 ../format_use_imports.py --check`
   - golden 用独立 Python/C++ 参考实现从 §8.5 语法算出后冻结。

### 8.13 已知风险

- 递归类型必须手写；忘记 `skip_type_id` 时是 `E0391`，错误可读但发生在 const 求值。
- 严格字段名意味着重构改名会改 ID；这是有意选择（严格指纹）。
- 32 位截断碰撞概率高，不适合大类型集；mptp 继续用 64 位。
- 同一 workspace 中不同 crate 开不同 hash feature 会被 Cargo 统一为优先级最高的
  算法；生产环境必须显式只开一个。
- `--all-features` 下 murmur3 覆盖 FNV；golden 测试按 `HASH_ALGORITHM` 分支。

---

## 9. 实施记录（第一轮，已被 §10 取代）

> 本节记录第一轮实现；derive 方式、宽度截断与属性已在 §10 修订。

### 9.1 落地内容

- `abs_denco`：
  - `type-id` / `type-id-murmur3` feature（默认关闭）；
  - `src/type_id/{mod,type_id_,impls_}.rs` 与 `src/type_id/hasher/{mod,fnv_,murmur3_}.rs`；
  - `TrTypeId`（u32/u64/u128）、`TrHasTypeId`（三宽度 + 名字）、`path_id_128`、
    `TypeIdEntry` / `TypeIdConflict` / `check_unique`、`denco_type_ids!`、
    `HASH_ALGORITHM` / `TYPE_ID_VERSION`；
  - 内置标量、`str`/`String`/`abs_str::String`、引用/`Box`/`Rc`/`Arc`/`Cow`、
    `Option`/`Vec`/切片/数组/队列/映射/集合的 `TrHasTypeId`。
- `denco_macro`：
  - `type-id` feature；`TrEncodable` derive 额外生成 `impl TrHasTypeId`；
  - 类型属性 `#[denco(skip_type_id)]` 与 `#[denco(type_id = 0x...)]`（互斥、0 拒绝）；
  - `src/type_id_.rs` 负责把 `TypeModel` 展开为隐藏 `Hasher` 调用序列；
  - `attrs_` / `model_` / `lib.rs` 增加 feature-gated 字段与分派。
- 文档：`abs_denco/README.md`、`denco_macro/README.md`、两个 crate 的 feature 表。

### 9.2 与提案的关键实现细节

- 哈希器是**流式 const 实现**（无固定缓冲上限）：`Hasher::new()` 写入
  `denco/type-id/1/<algorithm>\0`，随后每个片段写一个 framing 标签；字段类型写
  16 字节小端 `TYPE_ID_128`。
- `TrHasTypeId` 的 64/32 默认值在 trait 里由规范 128 位值截断 + 零值映射得到；
  derive 只生成 `TYPE_ID_128` 与 `TYPE_NAME`；transparent / 指针转发显式转发三个宽度。
- `#[denco(transparent)]` 的整个 ID **直接等于内层**（不是把内层 ID 当作字段片段再哈希）。
- 哈希 feature 只在 `abs_denco` 侧选择；`denco_macro` 不感知 hash feature，只保留
  `type-id` 代码生成这一重契约。
- `--all-features` 时 Murmur3 覆盖 FNV，因此 `HASH_ALGORITHM` 是测试分支依据。

### 9.3 验证结果

以下命令均通过（timeout 设置符合 AGENTS §12）：

- `cargo test -p abs_denco --features type-id`
- `cargo test -p abs_denco --features type-id-murmur3`
- `cargo test -p abs_denco --no-default-features --features type-id`
- `cargo test -p abs_denco --all-features`
- `cargo test -p denco_macro --features type-id`
- `cargo test --workspace`
- `cargo test --workspace --all-features`
- `cargo clippy --workspace --all-targets --all-features -- -D warnings`
- `cargo doc --workspace --all-features --no-deps`
- `python3 ../format_use_imports.py --check abs_denco denco_macro`

golden 值先用独立 Python 参考实现按 §8.5 语法算出；Murmur3 实现另与官方 C++
参考实现做过逐字节对照。

### 9.4 已知问题 / 下一轮

- `TrDecodable` 的 derive 不生成 ID：只派生解码的类型需要手写 `TrHasTypeId`。
- 递归类型必须 `skip_type_id`；否则 const 求值 `E0391`。
- 32 位截断只适合小类型集；registry 必须按实际使用宽度检查。
- 顺手修复了 `abs_denco/src/de/decodable_.rs` 中 `TrDecodableOwned` 的文档示例：
  `Cow` 行改为 `#[cfg(feature = "cow")]`，否则 `cargo test -p abs_denco --features type-id` 会因该 feature 未开启而失败（基线已有的问题）。
- mptp_rpc 迁移：`rpc_core_v1/type-id` facade、blanket `TrDataTypeId`、删除
  `im_protocol::ids` 与 12 处手写 impl、更新 golden / G-16。需要先推送
  `feat/gen-typeid` 或临时 `[patch]`；不在本轮。

---

## 10. 第二轮修订（2026-11-14）：显式 derive + 原生三宽度

### 10.1 为什么修订

第一轮实现有两个不符合最终意图的点：

1. `#[derive(TrEncodable)]` 会自动附带 `TrHasTypeId`，用户无法显式选择；
2. 64/32 位 ID 是 128 位值的低位截断，不是独立生成的值。

修订后，`TrHasTypeId` 只由显式 `#[derive(TrHasTypeId)]` 生成；三个宽度分别用
原生 32/64/128 位哈希计算，互不为截断。

### 10.2 显式 derive

```rust
use abs_denco::type_id::TrHasTypeId;
use denco_macro::{TrDecodable, TrEncodable};

#[derive(TrEncodable, TrDecodable, denco_macro::TrHasTypeId)]
struct User {
    id: u64,
    name: String,
}
```

- `denco_macro/type-id` 开启时才有 `TrHasTypeId` derive；必须与
  `abs_denco/type-id` 同时开启。
- `TrEncodable` / `TrDecodable` 不再生成任何 TypeId 代码。
- **删除** `#[denco(skip_type_id)]` 与 `#[denco(type_id = ...)]`：
  - 递归类型/手写 codec：不要派生 `TrHasTypeId`，直接手写 impl；
  - 固定线上 ID：同上，手写三个 const；
  - 递归可用 `path_id_32` / `path_id_64` / `path_id_128`。

### 10.3 三宽度独立生成

- FNV 家族：原生 FNV-1a 32 / 64 / 128。
- Murmur 家族：MurmurHash3 x86_32（32）、MurmurHash64A（64）、MurmurHash3
  x64_128（128）。Murmur3 没有原生 64 位输出，因此 64 位使用 Murmur64A。
- 每个宽度有独立的 domain 前缀：
  `denco/type-id/1/<fnv1a|murmur>/u32|u64|u128\0`。
- **字段 ID 也按宽度独立**：32 位 schema 写字段的 `TYPE_ID_32`（4 字节），64 位写
  `TYPE_ID_64`（8 字节），128 位写 `TYPE_ID_128`（16 字节）。三个 schema 流只在
  ID 段落长度/值上不同，其余标签/路径/名字相同。
- `TrHasTypeId` 三个 const 都是必需项，没有默认值：

```rust
pub trait TrHasTypeId {
    const TYPE_ID_128: u128; // 非零
    const TYPE_ID_64: u64;   // 非零
    const TYPE_ID_32: u32;   // 非零
    const TYPE_NAME: &'static str;
}
```

- `path_id_32` / `path_id_64` / `path_id_128` 分别是三个宽度的路径级入口。
- 第二轮修订发生在发布前，`TYPE_ID_VERSION` 仍为 1；发布后任何输入/前缀变化才需要
  递增版本。
- Murmur64A 需要预先知道总长度；`Hasher64::new(segment_len)` 接收不含前缀的
  segment 字节数，内部再加上前缀长度。FNV/32/128 的 `new(segment_len)` 忽略该参数，
  仅用于统一生成代码。

### 10.4 与 §8 / §9 的差异

- §8.2：不再由 `TrEncodable` 生成；改为显式 derive。
- §8.3：删除 64/32 默认值与非零截断；三个 const 必需且独立。
- §8.4：hash 家族仍是 FNV-1a / Murmur，但按原生宽度实现；`HASH_ALGORITHM`
  值为 `"fnv1a"` / `"murmur"`。
- §8.5：字段 ID 段落按宽度使用 4/8/16 字节；前缀带宽度（`/u32` 等）。
- §8.7：删除两个属性；递归/固定 ID 一律手写 impl。
- §9：第一轮实现记录已过时；goldens 全部重算。

### 10.5 验证（第二轮）

以下均通过：

- `cargo test -p abs_denco --features type-id`
- `cargo test -p abs_denco --features type-id-murmur3`
- `cargo test -p abs_denco --no-default-features --features type-id`
- `cargo test -p abs_denco --all-features`
- `cargo test -p denco_macro --features type-id`
- `cargo test --workspace`
- `cargo test --workspace --all-features`
- `cargo clippy --workspace --all-targets --all-features -- -D warnings`
- `cargo doc --workspace --all-features --no-deps`
- `python3 ../format_use_imports.py --check abs_denco denco_macro`
- 改动文件逐个 `rustfmt --check`

goldens 由独立的 Python 参考实现按 10.3 的 framing 算出；FNV 32/64/128、
Murmur3 x86_32/x64_128 与 Murmur64A 分别实现并与 Rust 侧结果对齐。

### 10.6 下一轮

- `mptp_rpc`：`rpc_core_v1/type-id` facade、blanket `TrDataTypeId`、删除
  `im_protocol::ids` 与 12 处手写 impl、更新 golden / G-16。
- 需要先推送 `feat/gen-typeid` 或临时 `[patch]`。
