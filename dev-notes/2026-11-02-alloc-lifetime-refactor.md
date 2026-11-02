# allocator 生命周期重构：`TrDecoder<'de, 'alloc>` 与引用版 `TrDecodable`

**当前结论**：现有 `TrDecoder<'de>` / `TrDecodable<'de>` 的单生命周期无法同时描述
「结果为值」与「结果为 `&'alloc mut T` / `Owning<'alloc, T>`」。推荐让
`TrDecoder` 增加 `'alloc` 与 allocator 访问，让 `TrDecodable` 增加 `'alloc`（以及
很可能还需要 allocator 类型参数），引用版复用值版解码；**不需要重写
`decode_*` / visitor 的读取逻辑**。bumpalo 与 sw-scope 的差异不在「寿命」，而在
「allocator 返回的引用形态」：前者是 `&'alloc mut T`，后者是 `Owning<'alloc, T>`。

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
    type Alloc: TrAlloc<'alloc>;

    /// 借用 decoder 的 allocator；`'alloc` 是 arena 的借用寿命。
    fn allocator(&self) -> &'alloc Self::Alloc;

    // 30 个 decode_* 的 GAT / 方法只加 `'alloc: 'a`；方法体不变。
}
```

### 3.2 `TrAlloc`：不绑定具体 allocator 的抽象（放在 `abs_denco`）

```rust
pub trait TrAlloc<'alloc> {
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
pub trait TrDecoder<'de, 'alloc, A: TrAlloc<'alloc>>: Sized { ... }
pub trait TrDecodable<'de, 'alloc, A: TrAlloc<'alloc>>: Sized { ... }

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
定义带 `A` 或 `D::Alloc: TrAlloc<'alloc, Ref<Foo> = Self>` 的约束；后者又因为
`Self` 的 `Foo` 类型参数无法在 trait 定义中表达而不可行。因此 B2 通常退化为 B1。

**结论**：要同时支持 bumpalo 与 sw-scope，`TrDecoder` / `TrDecodable` 很可能需要
第三个 allocator 类型参数（B1），而不是只加 `'alloc`。这是本次重构最需要先确认的
接口决策。

## 5. 分阶段实施

### 阶段 A：只加 `'alloc` 与 `allocator()`，值版全量通过

- `abs_denco`：`TrDecoder` / `TrDecodable` / `TrDecodableSeed` / `Visitor` /
  `impls.rs` / `cow.rs` / `rc.rs` / `alloc_collections.rs` / `std_collections.rs`
  的 GAT/where 加 `'alloc`；
- `mp_denco` / `json_denco`：decoder 的 `TrDecoder` impl 加 `'alloc`；
- `denco_macro`：`DecodeAsync` GAT、`decode_async`、`visit_newtype_struct` 加
  `'alloc`；
- 验证：`cargo check/test` 按 `abs_denco → mp_denco → json_denco → denco_macro`
  顺序；阶段 A 不引入 `TrAlloc` 的 `Ref`，引用版暂不实现。

### 阶段 B：`TrAlloc` + 引用版

- `abs_denco` 定义 `TrAlloc`；`TrDecoder` 的 `Alloc` / `allocator()`；
- bumpalo：`mp_denco` 的 `BumpAlloc` 实现 `TrAlloc`（`Ref<T> = &'alloc mut T`），
  `MpBumpDecoder` 携带 `&'alloc mut BumpAlloc`；
- sw-scope：`ScopeArena`（Scope + Retain 保活表）实现 `TrAlloc`
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

**对 `TrAlloc::Ref<T>` 的要求**：它必须能覆盖 `&'alloc mut T` 与 `Owning<'alloc, T>`；
如果只定义 `&'alloc mut T`，sw-scope 的 `Owning` 就无法表达。因此
`TrAlloc`/`Ref<T>` 是同时兼容两类 arena 的关键抽象。

## 7. 当前代码状态

- 分支：`feat/bump-alloc`；
- `abs_denco`：已回滚 bumpalo 集成（commit `6d6b87f`），保持抽象层；
- `mp_denco`：已有 `BumpAlloc` / `BumpString`（commit `71dc435`，feature `bump`），
  但只支持 `A: Allocator + Default`；
- `TrDecoder<'de, 'alloc>` 重构尚未开始；上一轮尝试已回滚，错误记录见
  `2026-11-02-bump-alloc.md`；
- 工作区有未提交改动：`.gitignore`、`abs_denco/src/lib.rs`、`mp_denco/src/config_.rs`
  （不是本次改动，不要覆盖）。

## 8. 下一会话提示词

把下面这段整段交给下一个会话：

```text
你在 denco 仓库工作（/root/projects/me.noli/old-cantare/denco）。目标：按
dev-notes 的规划重构 TrDecoder/TrDecodable 的 allocator 生命周期，先完成阶段 A。

开始前按顺序读：
1. dev-notes/2026-11-02-alloc-lifetime-refactor.md —— 总纲：结论、已废弃方案、
   TrAlloc/allocator() 设计、B1/B2 接口决策、阶段 A/B/C、bumpalo 与 sw-scope 差异。
2. dev-notes/2026-11-02-bump-alloc.md —— 已尝试方案的失败记录（E0309/E0478、
   BorrowedStrSeed、MpBumpDecoder），避免重复踩坑。
3. dev-notes/2026-10-18-gat-and-messagepack.md —— TrDecoder/TrDecodable 的 GAT 基线。
4. dev-notes/2026-10-18-derive-macros.md —— derive 生成代码的泛型/约束命名规则。

工作方法：
- 先 git status；有未提交改动先报告，不要覆盖。
- 阶段 A：给 TrDecoder 加 'alloc 与 allocator()（暂不加 TrAlloc::Ref 的引用版），
  给 TrDecodable/TrDecodableSeed 的 DecodeAsync GAT 加 'alloc；不要改 decode_* 的
  读取逻辑；按 abs_denco → mp_denco → json_denco → denco_macro 顺序，每步
  cargo check 通过再进下一步。
- 遇到 GAT 生命周期错误，回到总纲 §3/§4：优先用 'alloc trait 参数 + allocator()
  的方向，不要回到「V: 'alloc 统一加」的旧方案。
- 阶段 A 全绿后，再按总纲 §4 决定 B1（allocator 类型参数）还是 B2，并实现
  bumpalo 的 &'alloc mut T 与 sw-scope 的 Owning<'alloc, T>。
- 每阶段结束跑 cargo test/clippy；用 .tmp 放临时验证。
```
