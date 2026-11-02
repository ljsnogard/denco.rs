# allocator 生命周期重构：`TrDecoder<'de, 'alloc>` 与引用版 `TrDecodable`

**当前结论**：现有 `TrDecoder<'de>` / `TrDecodable<'de>` 的单生命周期无法同时描述
「结果为值」与「结果为 `&'alloc mut T` / `Owning<'alloc, T>`」。推荐让
`TrDecoder` 增加 `'alloc` 与 allocator 访问，让 `TrDecodable` 增加 `'alloc`（以及
很可能还需要 allocator 类型参数），引用版复用值版解码；**不需要重写
`decode_*` / visitor 的读取逻辑**。bumpalo 与 sw-scope 的差异不在「寿命」，而在
「allocator 返回的引用形态」：前者是 `&'alloc mut T`，后者是 `Owning<'alloc, T>`。

**进展（2026-11-02）**：阶段 A 已完成并全绿，实施记录见 §9；`TrArena` /
`NoArena` / `TrDecoder::Alloc` / `allocator()` 已提前落地，引用版实现仍未开始。
B1 方向已确认；`TrArena` 的异步化与「就地构造」取舍见 §11（结论：`TrArena`
必须异步、可失败；就地构造不纳入阶段 B）。**B1 已于 2026-11-02 晚完成**，
实施记录见 §12；bumpalo / sw-scope 的具体 arena 与引用版验收是阶段 B 的剩余工作。

## 1. 背景与问题

- `TrDecodable<'de>` 只有一个生命周期；`Self = &'alloc mut Foo` 的 `'alloc` 无法
  与 decoder 的 allocator 寿命建立约束。
- `TrDecodable::decode_async<D>` 的 `D: TrDecoder<'de>` 看不到 `'alloc`，也无法
  从 decoder 取 allocator 实例。
- 目标：同一套 trait 同时支持三种结果：
  1. 值：`Foo`；
  2. bumpalo 引用：`&'alloc mut Foo`；
  3. sw-scope 保活指针：`Owning<'alloc, Foo>`。
- `String` 本次保持默认 `Global` 的 `String`，不做 allocator 化。

## 2. 已尝试但放弃的方案（已废弃，勿重复）

1. **`TrDecoder<'de, 'alloc>` + 给 `decode_str` 等 4 个方法加 `V: 'alloc`**
   - `MpBumpDecoder` 的 future 类型包含 `'alloc`，trait 的 GAT 需要 `V: 'alloc`；
   - 但一旦统一加 `V: 'alloc`，`SeqCollectVisitor<T, Vec<T, A>>` 等容器 visitor
     会要求 `T: 'alloc, A: 'alloc`，与容器 `TrDecodable` 的 GAT 连锁；
   - `BorrowedStrSeed` 也因 `TrDecodableSeed::DecodeAsync` 的 GAT 看不到 seed 的
     `'alloc` 而失败（E0309/E0478）。
2. **把 `BumpAlloc` 放进 `abs_denco`**：违反 `abs_denco` 的抽象层定位，已回滚
   （commit `6d6b87f`）。
3. **把 `BumpAlloc` / `BumpString` 留在 `mp_denco`**：这是当前可用形态
   （commit `71dc435`），但只能走 `A: Allocator + Default`（每个容器/字符串一个
   arena），不能把解码结果借用 decoder 的 allocator。

具体错误记录见 [`2026-11-02-bump-alloc.md`](2026-11-02-bump-alloc.md)。

## 3. 推荐方案

### 3.1 `TrDecoder<'de, 'alloc>` + allocator 访问

```rust
pub trait TrDecoder<'de, 'alloc>: Sized {
    type Error: Error;
    type Alloc: TrArena<'alloc>;

    /// 借用 decoder 的 allocator；`'alloc` 是 arena 的借用寿命。
    fn allocator(&self) -> &'alloc Self::Alloc;

    // 30 个 decode_* 的 GAT / 方法只加 `'alloc: 'a`；方法体不变。
}
```

### 3.2 `TrArena`：不绑定具体 allocator 的抽象（放在 `abs_denco`）

```rust
pub trait TrArena<'alloc> {
    /// 引用/句柄形态：bumpalo 是 `&'alloc mut T`，sw-scope 是 `Owning<'alloc, T>`。
    type Ref<T: 'alloc>: 'alloc;

    fn alloc_value<T: 'alloc>(&'alloc self, value: T) -> Self::Ref<T>;
}
```

- `BumpAlloc`：`type Ref<T> = &'alloc mut T`；
- `ScopeArena`：`type Ref<T> = Owning<'alloc, T>`（内部 `Scope::put` + `Retain`
  保活表 + `try_owning`）。

### 3.3 `TrDecodable`：`'alloc` 参数（很可能还需要 allocator 类型参数）

```rust
pub trait TrDecodable<'de, 'alloc>: Sized {
    type DecodeAsync<'a, D>: TrMayCancel<'a, MayCancelOutput = Result<Self, D::Error>>
    where
        'de: 'a,
        'alloc: 'a,
        D: TrDecoder<'de, 'alloc> + 'a,
        D::Error: 'a,
        Self: 'a;

    fn decode_async<'a, D>(de: D) -> Self::DecodeAsync<'a, D>
    where
        'de: 'a,
        'alloc: 'a,
        D: TrDecoder<'de, 'alloc> + 'a,
        D::Error: 'a,
        Self: 'a;
}
```

### 3.4 三种结果的实现

- 值版：`impl<'de, 'alloc> TrDecodable<'de, 'alloc> for Foo`；
- bumpalo 引用版：`impl<'de, 'alloc> TrDecodable<'de, 'alloc> for &'alloc mut Foo`：
  ```rust
  let alloc = de.allocator();
  let value = Foo::decode_async(de).await?;
  Ok(alloc.alloc_value(value))
  ```
- sw-scope 版：`impl<'de, 'alloc> TrDecodable<'de, 'alloc> for Owning<'alloc, Foo>`，
  同样先值版解码，再 `ScopeArena::keep(value)`。

**关键洞察**：引用版是值版的薄包装，`decode_*` / visitor / derive 生成的 visitor
都不需要改；`'alloc` 只出现在 trait 参数与 `allocator()` 上，不侵入 visitor 的
`Value` 类型。

## 4. 必须提前决定的类型细节：`Ref<T>` 形态

`'alloc` 只解决「寿命」，没有解决「allocator 返回的引用形态」。`&'alloc mut Foo`
与 `Owning<'alloc, Foo>` 是两个不同的 `Self`，它们的 `decode_async` 需要
`D::Alloc::Ref<Foo>` 分别等于这两个类型。两种可行设计：

### 方案 B1：allocator 类型参与 trait（推荐）

```rust
pub trait TrDecoder<'de, 'alloc, A: TrArena<'alloc>>: Sized { ... }
pub trait TrDecodable<'de, 'alloc, A: TrArena<'alloc>>: Sized { ... }

impl<'de, 'alloc> TrDecodable<'de, 'alloc, BumpAlloc> for &'alloc mut Foo { ... }
impl<'de, 'alloc> TrDecodable<'de, 'alloc, ScopeArena> for Owning<'alloc, Foo> { ... }
```

`A` 精确固定 `Ref<Foo>` 的形态，impl 不需要额外关联类型等式约束；代价是
`TrDecodable` 的约束与 derive 生成代码多一个类型参数。

### 方案 B2：只加 `'alloc`，引用版由 allocator 侧实现

`TrDecodable for &'alloc mut Foo` 只对 `BumpAlloc` 的 `TrDecoder` 实现，
`Owning` 版本由 sw-scope 侧实现（sw-scope 依赖 abs_denco 的 trait，类型本地，
孤儿规则允许）。问题是 `TrDecodable` 的 trait 定义里 `D: TrDecoder<'de, 'alloc>`
无法在 impl 中收窄到具体的 `D::Alloc`（E0276），需要 `TrDecodable` 的 trait
定义带 `A` 或 `D::Alloc: TrArena<'alloc, Ref<Foo> = Self>` 的约束；后者又因为
`Self` 的 `Foo` 类型参数无法在 trait 定义中表达而不可行。因此 B2 通常退化为 B1。

**结论**：要同时支持 bumpalo 与 sw-scope，`TrDecoder` / `TrDecodable` 很可能需要
第三个 allocator 类型参数（B1），而不是只加 `'alloc`。这是本次重构最需要先确认的
接口决策。

## 5. 分阶段实施

### 阶段 A：加 `'alloc` 与 `allocator()`，值版全量通过（✅ 已完成，见 §9）

实际范围比最初估计的大：除 `TrDecoder` / `TrDecodable` / `TrDecodableSeed` 外，
`Visitor`、`SeqAccess` / `MapAccess` / `EnumAccess` / `VariantAccess` 及其
`*Ext` 都必须带 `'alloc`，否则嵌套解码的 visitor 无法给 `T: TrDecodable<'de,
'alloc>` 选择 allocator 寿命（实测 E0478）。所有 helper future 也增加 `'alloc`。

- `abs_denco`：`decoder_` / `decodable_` / `visitor_` / `access_` / `ext_` /
  `error_` / `forward_` / `impls.rs` / `collect_` / `cow_` / `rc_` /
  `ignored_any_` / `alloc_collections_` / `std_collections_` 的 GAT/where 加
  `'alloc`；新增 `TrArena` / `NoArena`；
- `mp_denco` / `json_denco`：decoder / access 的 trait impl 加 `'alloc`，
  `type Alloc = NoArena`，并实现 `allocator()`；
- `denco_macro`：生成的 Visitor / Seed / Access 约束、helper 泛型、`DecodeAsync`
  GAT 全部加 `'alloc`；
- 验证：`cargo check/test/clippy --workspace --all-features` 全绿。
- 与最初计划的差异：为了实现 `allocator()`，`TrArena` 及其 `Ref` 已提前定义；
  但引用版 `TrDecodable`（`&'alloc mut T` / `Owning`）仍未实现。

### 阶段 B：引用版 `TrDecodable`（`TrArena` / `Alloc` / `allocator()` 已在 A 落地）

- `abs_denco`：先定 B1 还是 B2（§4、§10）；
- bumpalo：`mp_denco` 的 `BumpAlloc` 实现 `TrArena`（`Ref<T> = &'alloc mut T`），
  `MpBumpDecoder` 携带 `&'alloc BumpAlloc`（共享引用，见 §11.2.3）；
- sw-scope：`ScopeArena`（Scope + Retain 保活表）实现 `TrArena`
  （`Ref<T> = Owning<'alloc, T>`），由 sw-scope 侧实现
  `TrDecodable for Owning<'alloc, Foo>`；
- 验证：`Foo<'alloc> { name: &'alloc str }`、`Owning<'alloc, Foo>`。

### 阶段 C：容器与嵌套

- `Vec<Foo<'alloc>>` / `Option<Foo<'alloc>>` 的 `TrDecodable`：容器元素需要同一
  `'alloc`；如果 `TrDecodable` 带 `A`，容器的 `impl` 也要带 `A`；
- 嵌套 `decode_seq` / `decode_map` 的 visitor 与 seed 传递；
- 验证 `Drop` 语义（bump 不运行 arena 内 `T::drop`；sw-scope 的 `Retain`/`close`）。

## 6. bumpalo 与 sw-scope 的差异清单

| 维度 | bumpalo | sw-scope |
| --- | --- | --- |
| 引用形态 | `&'alloc mut T` | `Owning<'alloc, T>`（`DerefMut`，不可 Clone） |
| 保活 | arena 整体释放 | `Retain<T>`；`Owning` 借用 `&'alloc Retain` |
| 保活表 | 不需要 | 必须由 `ScopeArena` 持有到 `'alloc`，否则 `Retain` 一 drop 数据就没了 |
| `close` | `Bump::reset(&mut self)`，借用检查阻止提前 reset | `Scope::close(self)` 跳过 `Owning`/`Sharing` 视图；`Owning` drop 后延迟清盘 |
| 共享 | `&mut` 不能共享 | `Sharing<'alloc, T>`（`Arc` 形态，可 Clone） |
| `Drop` | arena 内 `alloc` 的值不自动 `Drop` | `Retain` 最后一个句柄消失或 `close` 时析构 |
| `Default` | `BumpAlloc` 可以 `Default`（每个容器一个 arena） | `Scope` 需要 root/arena 实例，通常不能 `Default` |

**对 `TrArena::Ref<T>` 的要求**：它必须能覆盖 `&'alloc mut T` 与 `Owning<'alloc, T>`；
如果只定义 `&'alloc mut T`，sw-scope 的 `Owning` 就无法表达。因此
`TrArena`/`Ref<T>` 是同时兼容两类 arena 的关键抽象。

## 7. 当前代码状态

- 分支：`feat/bump-alloc`；
- `abs_denco`：已回滚 bumpalo 集成（commit `6d6b87f`），保持抽象层；
  阶段 A 之后 `TrDecoder` / `TrDecodable` / `TrDecodableSeed` / `Visitor` /
  `*Access` / `*AccessExt` 都带 `'alloc`，并新增 `de::TrArena` / `de::NoArena`；
- `mp_denco`：`MpDecoder<'r, 'alloc, R, A = NoArena>` 已对 arena 泛型化，
  access 链携带 `&'alloc A`；`BumpAlloc` / `BumpString` 已迁出到 `denco_bumpalo`，
  `bump` feature 与 bumpalo 依赖已删除；
- `denco_bumpalo`：新增 workspace crate，`BumpArena` 同时实现
  `core::alloc::Allocator` 与 `TrArena`；`Ref<T> = BumpBox<'alloc, T>`
  （bumpalo 特化 Box，适配 `abs_mm::TrBoxed` / `TrUnique`），引用版解码已在
  MessagePack 与 JSON 上端到端验证；
- `json_denco`：`JsonDecoder<'r, 'alloc, R, A = NoArena>` 已泛型化，
  `&mut JsonDecoder` 的 `TrDecoder` 实现携带 `&'alloc A`；
- `BumpBox<'alloc, T>` 的引用版由 `denco_bumpalo` 实现；`abs_denco` 仍保留
  `&'alloc mut T` 的 blanket impl 供裸引用形态使用；`Owning<'alloc, T>` 版本
  随 `denco_sw_scope` 暂缓；
- 上一轮单生命周期下的失败记录见 `2026-11-02-bump-alloc.md`。

## 8. 下一会话提示词

把下面这段整段交给下一个会话：

```text
你在 denco 仓库工作（/root/projects/me.noli/old-cantare/denco）。当前状态：
abs_denco 的 TrArena / allocator 参数、MpDecoder / JsonDecoder 的 arena 泛型化、
denco_bumpalo（BumpArena + BumpBox + abs_mm 适配）均已完成，测试/clippy 全绿；
sw-scope 按团队决定暂缓。本轮目标是阶段 C，按 §15 的现状差距分三块推进。

开始前按顺序读：
1. dev-notes/2026-11-02-alloc-lifetime-refactor.md —— 总纲与 §9~§15 的实施记录/现状核对。
2. dev-notes/2026-11-02-bump-alloc.md —— 单生命周期下的失败记录，避免重复踩坑。
3. dev-notes/2026-10-18-gat-and-messagepack.md —— TrDecoder/TrDecodable 的 GAT 基线。
4. dev-notes/2026-10-18-derive-macros.md —— derive 生成代码的泛型/约束命名规则。

目标 A：容器借用 decoder 的 arena（§15.2）
- 现状：Vec<T,A>/Box<T,A> 等要求 A: Allocator + Default，BumpArena 会为每个容器
  各自 Default 出一个独立 arena，并没有使用 decoder 传入的 &BumpArena；
  bumpalo 的 Allocator 只对 &Bump 实现且需要 allocator_api/allocator-api2 feature，
  对 &mut Bump 没有实现，且 &Bump 不是 Default。
- 目标：容器的 allocator 由 TrDecoder::allocator() 提供（例如 Vec<T, &'alloc A>，
  或经 TrArena 的容器 allocator 投影），使容器与引用版结果共享同一个 arena。
- 先确认 allocator 投影的 trait 形状，再改 impls_ / collect_ / alloc_collections_ /
  std_collections_；不要用 Default 作为共享 arena 的实现方式。

目标 B：带生命周期的类型（§15.3）
- 在 abs_denco 新增一个 trait（建议名 TrStrFromUtf8），用于「用 decoder 的 arena
  从 UTF-8 构造任意类似 std String 的类型」；String、BumpString、&'alloc str、
  Cow 等都可以实现，反序列化路径只依赖该 trait。
- 先定 trait 形状再实现，至少确认：
  1. 同步还是异步（sw-scope 的 put 是异步；若希望未来支持 ScopeStr，建议返回
     TrMayCancel future，bump 用 ready）；
  2. 整段 bytes 一次构造，还是 new_in + push_str 增量构造（长字符串流式读取）；
  3. UTF-8 校验由谁负责（decoder 已校验 vs trait 再校验）；
  4. 错误类型（A::Error 还是自定义，decoder 映射为 D::Error::custom）。
- 衔接方式也需确认：`impl<S: TrStrFromUtf8> TrDecodable for S` 这类 blanket impl
  容易与 derive/其它 impl 冲突；可考虑让 visitor 携带目标字符串类型，或新增
  decode_string_as 之类的显式入口。
- derive 允许生命周期参数，并把用户类型的生命周期绑定到 'alloc；
- 实现 &'alloc str / &'alloc [u8] 的 TrDecodable（或经由 TrStrFromUtf8/字节 trait）；
- 验收：Foo<'alloc> { name: &'alloc str, data: &'alloc [u8] } 在 MessagePack 与
  JSON 上解码为 BumpBox<'alloc, Foo<'alloc>>。

目标 C：保持既有接口
- 不要回退 §9~§15 的 TrArena / allocator 参数 / BumpBox / abs_mm 适配；
- sw-scope 继续暂缓（日后用 ScopeArena + ScopeOwning + 稳定地址 Retain 保活表）。

工作方法：
- 先 git status；有未提交改动先报告，不要覆盖。
- 公开 API 变更先说明理由并等待确认；先写设计结论到 dev-notes，再改代码。
- 顺序：abs_denco → denco_bumpalo → mp_denco → json_denco → denco_macro，
  每步 cargo check。
- 结束跑 cargo test/clippy --workspace --all-features；用 .tmp 放临时验证。
- 依赖现状：denco/Cargo.toml 有 mm_ptr/abs_mm 的本地 patch（离线开发用），
  上游同步后移除。
```

## 9. 阶段 A 实施记录（2026-11-02）

**结论**：`'alloc` 已贯穿解码方向的全部 trait、格式实现与 derive 生成代码，
值版解码行为不变，`cargo test/clippy --workspace --all-features` 全绿。

### 9.1 实际改动

- `abs_denco`
  - `TrDecoder<'de, 'alloc>` 增加 `type Alloc: TrArena<'alloc>` 与
    `fn allocator(&self) -> &'alloc Self::Alloc`；30 个 `decode_*` 的 GAT 与
    方法只增加 `'alloc: 'a` / `V: Visitor<'de, 'alloc>`，读取逻辑未改；
  - `TrDecodable` / `TrDecodableSeed` / `TrDecodableOwned` 增加 `'alloc`；
  - `Visitor` 变成 `Visitor<'de, 'alloc>`；`SeqAccess` / `MapAccess` /
    `EnumAccess` / `VariantAccess` 及四个 `*Ext` 同样增加 `'alloc`；
  - 新增 `de::alloc_`：`TrArena<'alloc>`（`type Ref<T>: 'alloc` +
    `alloc_value`）与 `NoArena`（`Ref<T> = T`，供无 arena 的格式使用）。
- `mp_denco` / `json_denco`
  - `TrDecoder` / access trait 的 impl 增加 `'alloc`；
  - 所有 decoder 的 `type Alloc = NoArena`、`allocator()` 返回 `&NoArena`；
  - 解码 helper future 增加 `'alloc` 泛型（因为 `V: Visitor<'de, 'alloc>`），
    GAT 类型以 `'alloc` 作为第一个实参；
  - `BumpString` 的解码实现同步加 `'alloc`。
- `denco_macro`
  - 生成的 `TrDecodable` / `Visitor` / `TrDecodableSeed` impl 增加 `'alloc`；
  - 所有 helper（`*OpFn`）增加 `'alloc`，GAT 类型以 `'alloc` 作为第一个实参；
  - `visit_newtype_struct` / `visit_seq` / `visit_map` / `visit_enum` 的
    where 增加 `'alloc: 'a`；
  - 测试中的手写 impl 同步更新。

### 9.2 关键发现（与总纲原估计的差异）

1. **`'alloc` 必须贯穿访问器链，不能只加在三个顶层 trait 上。**
   嵌套解码在 visitor 内部发生（`visit_some` / `visit_seq` / `visit_map` /
   `visit_enum` 会拿到 `D` 或 `A` 并调用 `T::decode_async`），
   `T: TrDecodable<'de, 'alloc>` 的 `'alloc` 必须由 trait 参数提供。若只在
   方法上写 `D: TrDecoder<'de, 'alloc>`，`Visitor` 看不到它，实测 E0478
   （trait 方法声明的是 `D: TrDecoder<'de, 'a>`，impl 收窄成 `'alloc`）。
   因此 `Visitor<'de, 'alloc>` + `*Access<'de, 'alloc>` + `*AccessExt` 是必需项。
2. **helper future 必须把 `'alloc` 放进泛型列表，并作为生成类型的第一个参数。**
   否则 `V: Visitor<'de, 'alloc>` 无法写在 helper 的 where 里。
3. **`allocator()` 需要一个返回类型，所以 `TrArena` / `NoArena` 在阶段 A 就落地了。**
   这不等于提前决定 B1：`TrDecoder` 的关联类型 `Alloc` 与 B1/B2 都兼容；
   真正的分叉是 `TrDecodable` 是否带 allocator 类型参数。
4. **`NoArena` 的 `Ref<T> = T`** 让「值版解码不经过 arena」有明确类型；
   `allocator()` 对 ZST 返回 `&NoArena`（const promotion，不借用 decoder）。
   阶段 B 给 `MpBumpDecoder` 换成 `Alloc = BumpAlloc` 时不需要改调用方。

### 9.3 验证

- `cargo check --workspace --all-targets --all-features`：通过；
- `cargo test --workspace --all-features`：通过（abs_denco / denco_macro /
  json_denco / mp_denco 全部单元、集成与文档测试）；
- `cargo clippy --workspace --all-targets --all-features -- -D warnings`：通过；
- 已对改动文件单独执行 `rustfmt --edition 2024`；未运行仓库级 `cargo fmt`。

## 10. 阶段 B 的 B1/B2 建议

**建议选 B1**：`TrDecoder<'de, 'alloc, A: TrArena<'alloc>>` 与
`TrDecodable<'de, 'alloc, A: TrArena<'alloc>>` 都带 allocator 类型参数，
并让 `TrDecoder::Alloc = A`。

理由：

- 引用版 `TrDecodable for &'alloc mut Foo` 需要把 `D::Alloc::Ref<Foo>` 钉死为
  `Self`；B2 想在 impl 上把 `D: TrDecoder<'de, 'alloc>` 收窄为
  `D: TrDecoder<'de, 'alloc, Alloc = BumpAlloc>`，与 trait 声明不一致（E0276），
  除非 trait 定义里就写 `D::Alloc: TrArena<'alloc, Ref<Foo> = Self>`，而 `Foo`
  无法在 trait 定义里表达。
- B1 的代价是 `TrDecodable` / derive 多一个类型参数；阶段 A 已经把 `'alloc`
  贯通，B1 只是把 `A` 加到同一批位置，改动模式与阶段 A 相同。
- 若团队希望避免 `TrDecodable` 多参数，可以退一步：引用版不做成
  `TrDecodable`，而是提供独立入口（例如 `ArenaDecodable` / 手写
  `decode_owned_async`），但那会偏离「同一套 trait 支持三种结果」的目标。

B1 的实施清单（供下一轮）：

1. `abs_denco`：`TrDecoder<'de, 'alloc, A>` 的 `type Alloc = A` 由关联类型改为
   参数约束（或保留关联类型 + `A: TrArena` 的等价形式）；`TrDecodable<'de,
   'alloc, A>` 的 GAT 写 `D: TrDecoder<'de, 'alloc, A>`；`TrDecodableSeed` 与
   access/ext trait 同步加 `A`；
2. `impls_` / `collect_` / 容器：值版 impl 对任意 `A: TrArena<'alloc>` 成立；
3. `mp_denco`：`BumpAlloc: TrArena`（`Ref<T> = &'alloc mut T`），
   `MpBumpDecoder<'r, 'alloc, R>` 持有 `&'alloc BumpAlloc`（`alloc_value` 只需
   `&self`），`impl TrDecodable for &'alloc mut Foo` 采用「值版解码 + alloc」；
4. `json_denco`：保持 `NoArena`；
5. `denco_macro`：生成的 impl 增加 allocator 参数（对值版类型可以是
   `NoArena` 默认，或让 derive 生成 `impl<A>`）；
6. sw-scope：`ScopeArena`（Scope + `Retain` 保活表）实现 `TrArena`
   （`Ref<T> = Owning<'alloc, T>`），由 sw-scope 侧实现引用版 `TrDecodable`。

## 11. `TrArena` API 的两个约束（2026-11-02 晚讨论）

**结论**：`TrArena` 必须**异步且可失败**（sw-scope 的放入路径只有异步入口）；
但**不建议**把「先分配、就地构造」纳入阶段 B。就地构造不是换一个分配函数，
而是给解码增加一套 place-aware 的构造模式，成本远大于它省下的那一次 memcpy。

### 11.1 就地构造（placement）对 denco 的影响

#### 11.1.1 现状为什么是「先解码、后 move」

- `Visitor::visit_*` 的契约是**返回 `Self::Value`**：构造点在 visitor 内部，
  解码器只能拿到一个已经构造好的值；
- bumpalo 的 `Bump::alloc(value)` 就是 `alloc_with(|| value)`，其源码注释写明
  「栈上构造、再从栈 memcpy 到 arena」；
- sw-scope 的 `put_async(value)` 同样先拿到 `T`，再在域锁内 `ptr::write`。
  它的 `emplace_with_predrop_async` 虽然能就地构造，但 `TrEmplace::emplace` 是
  **同步闭包**，并且在持有 `CooperativeRwLock` 写锁期间执行。

所以当前 `alloc_value(value)` 的语义准确说是「**搬入** arena」，不是「在 arena
里构造」。

#### 11.1.2 真正的就地构造需要什么

对 `&'alloc mut Foo` 而言，就地构造要求「分配 `Foo` 的空间 → 解码过程直接把
每个字段写进这块内存」。落到 denco 的接口上，至少需要：

1. **place-aware 的 visitor 契约**：`visit_seq` / `visit_map` / `visit_struct`
   不能再「返回 `Foo`」，而要接收 `&'alloc mut MaybeUninit<Foo>`，把字段逐个
   `ptr::write` 进去。`Visitor` 要么整体改签名，要么新增一套
   `TrVisitorInPlace` / `TrDecodableInPlace`，20 多个 `visit_*` 方法全部重复一遍。
2. **derive 生成代码分叉**：现在 `decode_entry_` / `visitor_impl_` 生成的是值版
   visitor；就地版需要生成「字段地址 + 初始化位图 + 失败/取消时按逆序 drop 已初始化
   字段」的代码。等于第二套解码生成器。
3. **部分初始化的 drop 语义**：
   - bumpalo 不运行 arena 内 `T::drop`，部分初始化最多泄漏 arena 空间；
   - sw-scope 在 `Retain` 消失或 `close` 时运行 `T::drop`。若 `Retain<T>` 在
     `T` 只初始化了一半时就存在，析构即 UB。因此 sw-scope 场景必须引入
     `InitGuard`（记录哪些字段已初始化）并在 `Retain` 发布前完成全部初始化。
4. **取消安全**：解码 future 会在任意 await 点被 drop。place-aware 版本必须在
   `InitGuard::drop` 里清理已写字段，否则 sw-scope 下泄漏资源、bumpalo 下泄漏
   内存；这要求 derive 生成正确的清理路径。
5. **sw-scope 需要新的两阶段 API**：
   - 现有 `emplace_*_async` 是「持锁 + 同步闭包」，不能把异步解码塞进去；
     若在持锁期间 await，嵌套的 `Owning` 字段再分配会重入同一把域锁，直接死锁；
   - 可行形态只能是「`alloc_uninit_async::<T>()` 先不发布（或发布为
     `Retain<MaybeUninit<T>>`）→ 解码写入 → `commit` 把槽位的析构元数据重定型为
     `T`」。sw-scope 目前只有 `alloc_uninit_slice_async`，没有单值 uninit +
     commit/retype，需要新增。

#### 11.1.3 收益评估

- 省下的只是**每个 arena 值一次 memcpy**（bumpalo 与 sw-scope 的 put 都是
  memcpy）；解析本身（buff 读取、UTF-8 校验、内部分配）通常远大于这次拷贝；
- 容器方向已经有另一条分配轴：`Vec<T, A>` / `Box<T, A>` 用
  `A: Allocator + Default` 直接在 arena 里构造缓冲，元素本来就不经过
  `TrArena`，不受此问题影响；
- 只有「特别大的顶层记录」或「必须自引用」的类型才会从中获得明显收益；而 Rust
  安全代码本身不支持自引用结构，后者仍需 `unsafe`。

#### 11.1.4 建议

- **阶段 B 不做就地构造**，保留「值版解码 + `alloc_value_async` 搬入」的薄包装；
- 为将来留出扩展位：`TrArena` 只承诺「值 → `Ref<T>`」，不要假设「必须先有值」；
  真正 placement 时另加 `TrArenaPlace`（或 `TrDecodableInPlace`）与
  `alloc_uninit_async` + `commit`，作为 opt-in 的第二种模式，而不是改动
  `alloc_value_async`；
- 是否值得做，交给基准测试：拿大记录对比「搬入」与「就地」的端到端耗时，再决定。

若将来要做，接口大致是：

```rust
pub trait TrDecodableInPlace<'de, 'alloc, A: TrArena<'alloc>>: Sized {
    type DecodeInPlaceAsync<'a, D>: TrMayCancel<'a, MayCancelOutput = Result<A::Ref<Self>, D::Error>>
    where
        'de: 'a,
        'alloc: 'a,
        D: TrDecoder<'de, 'alloc, A> + 'a,
        D::Error: 'a,
        Self: 'a,
        A::Ref<Self>: 'a;

    /// 在 arena 预先划出的 `place` 上直接构造；失败/取消时必须清理已写字段。
    fn decode_in_place_async<'a, D>(
        de: D,
        place: &'alloc mut core::mem::MaybeUninit<Self>,
    ) -> Self::DecodeInPlaceAsync<'a, D>
    where
        'de: 'a,
        'alloc: 'a,
        D: TrDecoder<'de, 'alloc, A> + 'a,
        D::Error: 'a,
        Self: 'a,
        A::Ref<Self>: 'a;
}
```

注意它**不能再复用**现有 `TrDecodable` 的薄包装：`impl TrDecodable for &'alloc
mut T` 必须先有 place 才能解码，因此要么加 `T: TrDecodableInPlace` 约束，要么
把 `Ref<T>` 的构造拆成两步。

### 11.2 `TrArena` 应该异步

#### 11.2.1 事实依据

- sw-scope 的公开放入入口全部是异步的：`put_async`、
  `emplace_with_predrop_async`、`put_with_factory_predrop_async`、
  `put_str_with_predrop_async`、`alloc_uninit_slice_async`；
- 它们内部统一走 `CooperativeRwLock::write_async()`，这是协作式异步锁；同步
  `try_write` 只用于清盘路径，拿不到锁时会直接失败，不能用于解码分配；
- 因此同步版 `TrArena` 无法为 `Owning<'alloc, T>` 实现，除非在异步上下文里
  `block_on`（不可接受）。

#### 11.2.2 建议的 trait 形状

```rust
pub trait TrArena<'alloc> {
    /// 分配失败的错误类型；由引用版解码映射为 `D::Error::custom`。
    type Error: core::error::Error;

    /// 引用/句柄形态：bumpalo 是 `&'alloc mut T`，sw-scope 是 `Owning<'alloc, T>`。
    type Ref<T: 'alloc>: 'alloc
    where
        Self: 'alloc;

    /// 把值放入 arena；同步分配器返回 ready future。
    fn alloc_value_async<'a, T>(
        &'alloc self,
        value: T,
    ) -> impl TrMayCancel<'a, MayCancelOutput = Result<Self::Ref<T>, Self::Error>> + 'a
    where
        'alloc: 'a,
        T: 'alloc + 'a,
        Self: 'a;
}
```

- 用 RPITIT 而不是 GAT：`TrArena` 属于 `Visitor` / `*Access` 一类的内部管线
  trait，调用方只 await、不命名 future；denco 的 GAT 只留给需要被命名的
  `TrDecoder` / `TrDecodable` / `TrEncoder` / `TrEncodable`；
- `TrMayCancel` 而不是裸 `Future`：与 denco 其它异步点一致，调用方用
  `may_cancel_with(cancel.child_token())` 传播取消；sw-scope 在等锁期间被取消
  时不会发布任何对象；
- `TrArena::Error` 用 `core::error::Error`，引用版解码里
  `map_err(D::Error::custom)`；`NoArena` 可用 `Infallible`；
  bumpalo 的 `AllocErr` 只实现了 `Display`（未实现 `Error`），sw-scope 的
  `ScopeError` 只有 `Debug`，因此两个 arena 侧都需要一个小的适配错误类型
  （`impl Error`），并优先用 `Bump::try_alloc_layout` 避免 `Bump::alloc` 的
  OOM panic。

#### 11.2.3 借用与顺序（已用探针验证）

引用版解码必须是「先取 allocator、再 move decoder、最后 await 分配」：

```rust
let alloc = de.allocator();                 // &'alloc A，与局部借用解耦
let value = Foo::decode_async(de).await?;   // 消耗 de
alloc.alloc_value_async(value).await       // 分配失败 → D::Error::custom
```

这要求 `allocator(&self) -> &'alloc Self::Alloc` 返回的引用**不依赖 `&self`
的局部借用**。最小探针验证了三种形态：

- `struct MyDe<'alloc> { arena_: &'alloc BumpArena }` + `allocator()` 返回
  `self.arena_`：编译通过，`&'alloc mut Foo` 与 `Owning<'alloc, Foo>` 两种引用版
  都能按上述顺序实现；
- `struct OwningDe { arena_: BumpArena }`（自己拥有 arena）：`&self.arena_`
  报 `lifetime may not live long enough`（borrowed data escapes）；
- `struct MutDe<'alloc> { arena_: &'alloc mut BumpArena }`：`&*self.arena_`
  同样报 `lifetime may not live long enough`——通过 `&self` 无法把
  `&'alloc mut A` 变成 `&'alloc A`（`&mut` 不 Copy，不能在共享借用下移出）。

**结论**：decoder 必须持有 arena 的**共享**引用 `&'alloc A`，不能拥有 `A`，
也不能持有 `&'alloc mut A`。这对两个 arena 都适用：`BumpAlloc` 内部是
`SpinningMutexOwned`，`Scope` 的 put 也只取 `&self`；需要 `&mut` 的 reset / close
由调用方在解码结束后、在 decoder 之外完成。

顺序上应保持**先解码、后分配**：这样解码期间不持有域锁，嵌套的
`Owning` / `&'alloc` 字段逐个解码、逐个放入，不会因为父对象持锁而死锁。

#### 11.2.4 对 B1 的额外影响

1. **`A` 还要穿过 access/ext 链**：`next_element_seed` / `next_key_seed` 会用
   同一个 arena 构造子 decoder，因此 `SeqAccess` / `MapAccess` / `EnumAccess` /
   `VariantAccess` 与四个 `*Ext` 都要带 `A`，`TrDecodableSeed<'de, 'alloc, A>`
   同步；
2. **`ScopeArena` 的保活表**：`Owning<'alloc, T>` 借用 `&'alloc Retain<T>`，
   arena 必须以**稳定地址**持有异构的 `Retain<T>`。可用
   `Vec<Box<dyn ErasedDrop + 'alloc>>`（`Box` 地址稳定，drop 时运行类型化
   析构），不要用会重分配的 `Vec<Retain<T>>`，否则已返回的 `Owning` 会悬空；
3. **`NoArena` 保持同步等价**：`alloc_value_async` 直接返回
   `future::ready(Ok(value))`，零成本；bumpalo 同理用 ready future；
4. **错误映射**：引用版 impl 内部 `map_err(D::Error::custom)`，不需要给
   `TrDecodable` 增加 `From<A::Error>` 约束（GAT 的 where 不能比 trait 声明更强）。

### 11.3 对阶段 B 清单的修订

- `TrArena`：异步 + 可失败（`Error`），`alloc_value_async` 取代
  `alloc_value`；`Ref<T>` 形态不变；
- `TrDecoder<'de, 'alloc, A>` / `TrDecodable<'de, 'alloc, A>`：沿用 B1；
  `A` 同时进入 `TrDecodableSeed` 与 access/ext；
- bumpalo：`BumpAlloc: TrArena`（`Ref<T> = &'alloc mut T`，ready future），
  `MpBumpDecoder<'r, 'alloc, R>` 持有 `&'alloc BumpAlloc`（共享引用即可，见 §11.2.3）；
- sw-scope：`ScopeArena<'alloc>` 持有 `&'alloc Scope` + 稳定地址保活表，
  `Ref<T> = Owning<'alloc, T>`；
- 就地构造：不在阶段 B；作为阶段 D 的 opt-in 设计，先基准测试再决定。

## 12. B1 实施记录（2026-11-02 晚）

**结论**：已按「异步 GAT + 保持搬运」完成 B1 接口重构；`cargo check/test/clippy
--workspace --all-features` 全绿。Bumpalo 这一类 arena 已在 §13 接入
（`denco_bumpalo` + `MpDecoder` 泛型化）；sw-scope 按团队决定暂缓。

### 12.1 `TrArena` 的最终形状（异步 GAT）

```rust
pub trait TrArena<'alloc> {
    type Error: core::error::Error;
    type Ref<T: 'alloc>: 'alloc where Self: 'alloc;
    type AllocValueAsync<'a, T>: TrMayCancel<
        'a,
        MayCancelOutput = Result<Self::Ref<T>, Self::Error>,
    > where
        'alloc: 'a,
        T: 'alloc + 'a,
        Self: 'alloc + 'a;
    fn alloc_value_async<'a, T>(&'alloc self, value: T) -> Self::AllocValueAsync<'a, T>
    where
        'alloc: 'a,
        T: 'alloc + 'a,
        Self: 'alloc + 'a;
}
```

- 同步分配器用 `core::future::Ready` 实现；`NoArena` 的 `Error = Infallible`、
  `Ref<T> = T`；
- 引用版解码是「先取 allocator → 移交给值版解码 → await 分配」的薄包装；
- `TrArena::Error` 由引用版映射为 `D::Error::custom`；bumpalo 的 `AllocErr` 与
  sw-scope 的 `ScopeError` 都不满足 `core::error::Error`，需要各自的适配错误类型。

### 12.2 allocator 类型参数如何贯穿

- `TrDecoder<'de, 'alloc, Alloc: TrArena<'alloc>>`：`allocator()` 返回
  `&'alloc Alloc`，不再有 `type Alloc` 关联类型；
- `TrDecodable<'de, 'alloc, Alloc>` / `TrDecodableSeed<'de, 'alloc, Alloc>`；
- `Visitor<'de, 'alloc, Alloc>`、四个 `*Access` 与四个 `*AccessExt` 同样带
  `Alloc`（嵌套解码要靠它构造同一 arena 的子 decoder）；
- 值版 impl 全部对 `Alloc` 泛化；当前的 `MpDecoder` / `JsonDecoder` 固定为
  `NoArena`（它们还没有 arena），`TrDecodableOwned` 定义为
  `for<'de, 'alloc> TrDecodable<'de, 'alloc, NoArena>`。

### 12.3 关键发现：引用版可以是 abs_denco 的泛型 blanket impl

原计划以为引用版要由 derive 为每个类型单独生成。实际上 `abs_denco` 可以直接写
（trait 是本地 trait，孤儿规则允许）：

```rust
impl<'de, 'alloc, Alloc, T> TrDecodable<'de, 'alloc, Alloc> for &'alloc mut T
where
    Alloc: TrArena<'alloc, Ref<T> = &'alloc mut T> + 'alloc,
    T: TrDecodable<'de, 'alloc, Alloc> + 'alloc,
{ /* 取 allocator、值版解码、await 分配 */ }
```

`A::Ref<T> = &'alloc mut T` 这个 GAT 绑定把引用形态钉死为 `Self`，因此任何
「返回 `&mut T`」的 arena（bumpalo）都自动可用；返回 `Owning` 的 allocator 不匹配
本 impl。**derive 只需要生成值版 impl**，不用生成引用版。sw-scope 侧则在自己
crate 内写 `impl TrDecodable<..., ScopeArena> for Owning<'alloc, T>`
（`Owning` 与 `ScopeArena` 都是本地类型，孤儿规则允许）。

### 12.4 实施中遇到的三个约束

1. **`Expected` 的 blanket impl 触发 E0207**：`Alloc` 只出现在
   `T: Visitor<..., Alloc>` 谓词里，不构成对 impl 类型参数的约束。解决办法是新增
   `error_::VisitorExpected<'a, V, Alloc>` 适配器（把 `Alloc` 放进 self 类型），
   `Expected` / `Error` 的公开 API 完全不变。
2. **decoder 只能持有 `&'alloc A`（共享）**：不能自己拥有 `A`，也不能持有
   `&'alloc mut A`。后者无法通过 `&self` 变出 `&'alloc A`（`&mut` 不 Copy），
   前者的借用会让 `allocator()` 的返回值绑定在 `&self` 上，无法先取 allocator
   再 move decoder。探针三种形态均已验证。
3. **`gen_mcf2` 禁止泛型参数上的内联约束**：derive 生成的 `__DencoAlloc` 必须是
   裸类型参数，`TrArena` 约束必须写进 where 子句；`forward_to_decode_any!` 宏也
   改成显式接收 allocator：`alloc = NoArena;`。

### 12.5 尚未完成（阶段 B）

1. `mp_denco`：让 `MpDecoder` 对 allocator 泛化（保留 `MpDecoder<'r, R>` 作为
   `NoArena` 别名），新增 `MpBumpDecoder<'r, 'alloc, R>` 持有
   `&'alloc BumpAlloc`；`BumpAlloc` 实现 `TrArena`（`Ref<T> = &'alloc mut T`，
   `try_alloc_layout` + `ptr::write`，新增适配错误类型）；access 结构同样要携带
   `&'alloc A` 才能给子 decoder 传递 arena；
2. sw-scope：`ScopeArena<'alloc>` 持有 `&'alloc Scope` + 稳定地址的异构
   `Retain<T>` 保活表（`Vec<Box<dyn ErasedDrop + 'alloc>>`），
   `Ref<T> = Owning<'alloc, T>`；用 `put_async` 实现 `alloc_value_async`；
3. 引用版验收：`Foo<'alloc> { name: &'alloc str }` 解码为 `&'alloc mut Foo`
   与 `Owning<'alloc, Foo>`；容器/嵌套（阶段 C）。

## 13. Arena 接入：`denco_bumpalo` 与格式泛型化（2026-11-02 深夜）

**结论**：按「外部包装 crate」的方向完成了 Bumpalo 这一类 arena 的接入：
`abs_denco` 的 `TrArena` + 泛型格式 decoder + 新 crate `denco_bumpalo`，
引用版解码（`&'alloc mut T`）已端到端跑通。sw-scope 按团队决定暂缓。

### 13.1 命名调整

- `TrAlloc` → [`TrArena`](../abs_denco/src/de/arena_.rs)；
- `NoAlloc` → `NoArena`；
- `MpDecoder` **不改名**：加默认类型参数 `A = NoArena`，并新增 `'alloc`
  生命周期；`MpDecoder<'r, 'alloc, R, A>` 携带 `&'alloc A`；
- 不再需要 `MpBumpDecoder`（旧单生命周期设计的产物，代码里已不存在）。

### 13.2 格式侧泛型化（mp_denco）

- `MpDecoder<'r, 'alloc, R, A: TrArena<'alloc> + 'alloc = NoArena>` 持有
  `arena_: &'alloc A`；`MpDecoder::new` 用于 `NoArena`，
  `MpDecoder::new_in(reader, &arena)` 用于任意 arena；
- `MpSeqAccess` / `MpMapAccess` / `MpEnumAccess` / `MpVariantAccess` 都携带
  `&'alloc A`，构造子 decoder 时透传（`next_element_seed` 等嵌套路径因此天然
  使用同一个 arena）；
- `MpNameDecoder`（只驱动 `visit_str` 的辅助 decoder）同样携带 `&'alloc A`；
- 所有 decode helper future 增加 `A` 类型参数与 `TrArena` 约束；
- 读端 helper（`read_tag_async_` 等）也随 `MpDecoder` 泛型化。
- `json_denco` 尚未泛型化，仍固定 `NoArena`。

### 13.3 新 crate：`denco_bumpalo`

- `BumpAlloc`（后改名 `BumpArena`）/ `BumpString` 从 `mp_denco` 迁出；
  `mp_denco` 删除 `bump` feature 与 `bumpalo` / `atomic_sync` 依赖；
- `BumpArena` 同时实现 `core::alloc::Allocator`（容器用）与
  [`TrArena`](../abs_denco/src/de/arena_.rs)（引用版解码用）；
- `TrArena::Ref<T>` 最初是 `&'alloc mut T`，后来按团队建议改为 bumpalo 特化
  Box：[`BumpBox<'alloc, T>`](../denco_bumpalo/src/lib.rs)（见 §14）；
- bumpalo 的 `AllocErr` 未实现 `core::error::Error`，新增薄适配
  `BumpArenaError`；
- `denco_bumpalo` 不依赖 `mp_denco` / `json_denco`（只有 dev-dependencies
  用于集成测试），格式 crate 也不依赖 `denco_bumpalo`，两侧完全解耦。

### 13.4 `abs_mm` 的评估（已被 §14 取代）

`abs_mm` dev/0.3.0 之前的 `TrBoxed` 要求 `Deref` + 一个
`core::alloc::Allocator` 句柄（`try_get_mem_alloc`），而 `&'alloc mut T` 与
sw-scope 的 `Owning` 都不携带 `core::alloc::Allocator`，当时判断不适用。
dev/0.3.0 更新后 `TrBoxed` 只剩 `Deref`，适配方案见 §14。

### 13.5 sw-scope 暂缓

按团队决定先满足 bumpalo 这一类 arena。`denco_sw_scope` 需要：

- `ScopeArena<'alloc>`：持有 `&'alloc Scope` + 稳定地址的异构 `Retain<T>`
  保活表（`Vec<Box<dyn RetainHolder + 'alloc>>`）；
- 本地 newtype `ScopeOwning<'alloc, T>(Owning<'alloc, T>)`：`Owning` 是
  sw-scope 的类型、`TrDecodable` 是 abs_denco 的 trait，包装 crate 不能直接为
  `Owning` 实现（孤儿规则），newtype 同时解决这一点；
- `TrArena::Ref<T> = ScopeOwning<'alloc, T>`，`alloc_value_async` 基于
  `Scope::put_async`，错误类型包装没有 `Display`/`Error` 的 `ScopeError`。

### 13.6 验证

- `cargo check/test/clippy --workspace --all-features` 全绿；
- `denco_bumpalo/tests/bump_alloc_.rs`：
  - `Vec<u8, BumpAlloc>`（`Allocator` 容器路径）；
  - `BumpString<BumpAlloc>`；
  - `MpDecoder::new_in(&mut input, &bump)` 把 `Point` 解码为 `&'alloc mut Point`，
    并断言 `bump.allocated_bytes() > 0`。

### 13.7 剩余（json 已在 §14 完成）

1. `json_denco` 泛型化 —— 已在 §14 完成；
2. `denco_sw_scope`；
3. 阶段 C：容器/嵌套与 Drop 语义验证。

## 14. abs_mm 适配、BumpBox 与 json 泛型化（2026-11-03 凌晨）

**结论**：`abs_mm` dev/0.3.0 的 `TrBoxed` 已去掉 allocator GAT，只要求
`Deref` + `Item`；据此把 Bump arena 的 `Ref<T>` 改成 bumpalo 特化 Box
（`BumpBox`），并适配 `TrBoxed` / `TrUnique`。`json_denco` 也完成了 allocator
泛型化，BumpArena 的引用版解码同时支持 MessagePack 与 JSON。

### 14.1 abs_mm dev/0.3.0 的变化

```rust
pub trait TrBoxed: Deref {
    type Item: ?Sized;
}
pub trait TrUnique: TrBoxed + DerefMut<Target = Self::Item> {}
```

不再有 `Alloc` 关联类型或 `try_get_mem_alloc`，因此 arena 句柄只要能
`Deref`/`DerefMut` 就能适配。`Owning` 这类类型（sw-scope）也可以按同样方式
适配，但按团队决定随 `denco_sw_scope` 暂缓。

### 14.2 `BumpAlloc` → `BumpArena`，`Ref<T>` 改为 `BumpBox`

- 类型改名 `BumpAlloc` → `BumpArena`（crate 名仍是 `denco_bumpalo`）；
- 新增 [`BumpBox<'alloc, T>`](../denco_bumpalo/src/lib.rs)：包装
  `bumpalo::boxed::Box`，实现 `Deref` / `DerefMut`，并实现
  `abs_mm::res_man::TrBoxed` / `TrUnique`；
- `abs_denco` 统一 re-export `abs_mm::res_man::{TrBoxed, TrUnique}`
  （`abs_denco/Cargo.toml` 因此依赖 `abs_mm`），`denco_bumpalo` 只从
  `abs_denco` 取这两个 trait，不再直接依赖 `abs_mm`；
- `TrArena for BumpArena` 的 `Ref<T>` 从 `&'alloc mut T` 改为
  `BumpBox<'alloc, T>`；`alloc_value_async` 用 `Allocator::allocate` 划块、
  `ptr::write` 写入后以 `BumpaloBox::from_raw` 接管：
  - 内存仍由 arena 整体释放；
  - `BumpBox` drop 时运行 `T` 的析构（与 `&mut T` 不同）；
  - `BumpBox::into_raw` 可取出裸指针。
- 引用版 `TrDecodable` 现在由 `denco_bumpalo` 为本地类型 `BumpBox` 实现
  （值版解码 + `alloc_value_async`），与 sw-scope 的 `ScopeOwning` 形态对称；
  `abs_denco` 里 `&'alloc mut T` 的 blanket impl 保留，供将来「裸引用」形态的
  arena 使用；
- 测试：`Vec<u8, BumpArena>`、`BumpString<BumpArena>`、MessagePack/JSON 两种格式
  把 `Point` 解码为 `BumpBox<Point>`，并用 `assert_unique` 静态验证
  `abs_denco::TrUnique<Item = Point>`。

### 14.3 json_denco 泛型化

- `JsonDecoder<'r, 'alloc, R, A: TrArena<'alloc> + 'alloc = NoArena>`，新增
  `arena_: &'alloc A`；`JsonDecoder::new`（NoArena）与
  `JsonDecoder::new_in(reader, &arena)`；
- `impl TrDecoder<'de, 'alloc, A> for &mut JsonDecoder<'r, 'alloc, R, A>`；
  `allocator()` 返回 decoder 内部 arena；
- `JsonSeqAccess` / `JsonMapAccess` / `JsonEnumAccess` / `JsonVariantAccess`
  都参数化 `'alloc` / `A`（它们本身持有 `&mut JsonDecoder`，不需要额外 arena
  字段）；`JsonNameDecoder` / `JsonNameEnumAccess` 携带 `&'alloc A`；
- 名称分派 helper 增加 `arena` 参数，GAT 类型补 `A`；
- `json_denco/tests/support` 的 `decode_next_` 把 decoder 的 `'alloc` 显式写出；
- 保留 `JsonDecoder` 名字与默认 `A = NoArena`；类型注解需要多一个 `'alloc`
  生命周期（与 `MpDecoder` 相同）。

### 14.4 依赖 patch（本地开发期）

`mm_ptr` 的 `dev/0.2.0` 远端提交 `b014cc6` 仍按旧 `abs_mm`（带 `Alloc` GAT）
实现，而本地 `mm_ptr` 分支已到 `cc5c64e`（Follow abs_mm update）。为使工作区在
离线/未推送状态下可构建，`denco/Cargo.toml` 增加了：

```toml
[patch."https://gitee.com/lino_snsalias/mm_ptr.git"]
mm_ptr = { path = "../mm_ptr" }

[patch."https://gitee.com/lino_snsalias/abs_mm.git"]
abs_mm = { path = "../abs_mm" }
```

上游推送 `mm_ptr` 新提交后应移除这两个 patch。

### 14.5 验证

- `cargo check/test/clippy --workspace --all-features` 全绿；
- `denco_bumpalo` 4 个集成测试：`Vec<u8, BumpArena>`、`BumpString`、
  MessagePack `BumpBox<Point>`、JSON `BumpBox<Point>`；
- `abs_mm::TrUnique` 静态断言通过。

### 14.6 剩余

1. `denco_sw_scope`（`ScopeArena` + `ScopeOwning` + 稳定地址 Retain 保活表）；
2. 阶段 C：容器/嵌套与 Drop 语义；
3. 上游 `mm_ptr` 推送后移除本地 patch。

## 15. 现状核对（2026-11-03）

针对「三个目标是否完成」的逐条核对。结论：第 1 条在「值为 owned 字段」的前提下成立；
第 2、3 条**尚未完成**，需要容器与字符串的 arena 化。

### 15.1 目标 1：mp/json 都能用 BumpArena 解码出生命周期随 arena 的 Box

**成立（有限定）**：

- `MpDecoder::new_in(reader, &bump)` 与 `JsonDecoder::new_in(reader, &bump)` 都返回
  `BumpBox<'alloc, T>`（`denco_bumpalo` 对 `BumpBox` 实现了
  `TrDecodable<..., BumpArena>`）；
- `BumpBox` 的生命周期只跟 arena 的借用，不跟输入缓冲；新增测试
  `decode_bump_box_outlives_input` 在内层作用域解码、离开作用域后仍能读取；
- 限定：这里的 `T` 必须是 owned 字段类型；`T` 自身还不能借用 arena（见 15.3）。
  `BumpBox` 是 `denco_bumpalo` 的本地 newtype，不是 `bumpalo::boxed::Box` 本身，
  但 Deref/DerefMut 与析构语义一致（drop 时运行 `T` 的析构）。

### 15.2 目标 2：容器的 Allocator 可设为 BumpArena 或 `&mut Bump`

**只完成前半句，且是 `BumpArena` 而非 `&mut Bump`**：

- `Vec<T, A>` / `Box<T, A>` 等容器实现要求 `A: Allocator + Default`，
  `BumpArena: Default`，所以 `Vec<u8, BumpArena>` 可以解码（已有测试）；
- **`&mut Bump` 不可用**：
  - bumpalo 只对 `&'a Bump` 实现 `Allocator`，且要开启 `allocator_api` feature；
    对 `Bump` / `&mut Bump` 没有实现；
  - 即使换成 `&Bump`，也不满足我们容器实现要求的 `A: Default`（`&Bump` 不是
    `Default`）；
- 要让容器「借用 decoder 的 arena」，需要把容器实现从 `A::default()` 改为从
  `TrDecoder::allocator()` 取得 arena（阶段 C 的工作），并且容器 allocator 类型要
  能从 arena 派生（例如 `BumpArena` → `&Bump` 的投影），目前没有这层抽象。

### 15.3 目标 3：mp/json 正确支持带生命周期的类型

**不成立**：

- `denco_macro` 的 `reject_lifetimes_` 明确拒绝带生命周期参数的类型
  （[model_.rs](../denco_macro/src/model_.rs)）；
- `abs_denco` 没有 `TrDecodable for &'alloc str` / `&'alloc [u8]` 的实现；
- `MpDecoder` / `JsonDecoder` 的字符串/字节路径只产出 owned `String` / `Vec<u8>`，
  `visit_str` 传入的是短生命周期的栈/临时视图，不能升级为 `&'alloc str`；
- 目前能带生命周期的只有 arena 句柄本身（`BumpBox<'alloc, T>`），不是用户结构体的
  字段。

要完成第 3 条，至少需要：

1. decoder 在字符串/字节路径上把数据写入 arena，并调用 `visit_borrowed_str` /
   `visit_borrowed_bytes`；
2. `abs_denco` 为 `&'alloc str` / `&'alloc [u8]` 提供 `TrDecodable` 实现（或在
   `TrArena` 上加 `alloc_str` / `alloc_bytes` 能力）；
3. derive 允许生命周期参数，并把用户类型的 `'a` 绑定到 `'alloc`
   （`impl<'de, 'alloc, A> TrDecodable<'de, 'alloc, A> for Foo<'alloc>`），
   约束命名沿用 `__DencoAlloc` / `'__denco_a` 规则；
4. 相应测试：`Foo<'alloc> { name: &'alloc str, data: &'alloc [u8] }` 在
   MessagePack 与 JSON 上解码为 `BumpBox<'alloc, Foo<'alloc>>`。
