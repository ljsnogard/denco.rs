# BumpAlloc：用 bumpalo 替代 Global 的尝试

**日期**：2026-11-01
**分支**：`feat/bump-alloc`
**目标**：给 denco 提供一个 `Allocator + Default` 的 bump 分配器，让现有容器的
`A: Allocator + Default` 约束直接可用，避免把 allocator 从 decoder 注入。

## 背景

上一轮调研的结论是：bumpalo 的 `&'bump Bump` 是标准 `Allocator`，但不是 `Default`，
所以 `abs_denco` 的容器实现（`Vec<T, A>`、`Box<T, A>`、`Rc<T, A>`、`Arc<T, A>` 等）
无法直接用 `&'bump Bump`，必须把 allocator 从 decoder 注入（公共 API 大改）。

本分支换一个方向：**自己提供一个 `Default` 的 arena allocator**，内部用
`atomic_sync::SpinningMutexOwned<Bump>` 包装 `Bump`。这样：

- `BumpAlloc: Allocator + Default`；
- 现有 `A: Allocator + Default` 的容器实现零改动即可使用；
- 不需要改 `TrDecoder` / `TrDecodable` / derive 生成代码。

## 设计

新增 `abs_denco/src/de/bump_.rs`，feature `bump`（`abs_denco/Cargo.toml`）：

```rust
pub struct BumpAlloc {
    inner_: SpinningMutexOwned<Bump>,
}

impl Default for BumpAlloc {
    fn default() -> Self {
        Self { inner_: SpinningMutexOwned::new_owned(Bump::new()) }
    }
}

unsafe impl Allocator for BumpAlloc {
    fn allocate(&self, layout: Layout) -> Result<NonNull<[u8]>, AllocError> { ... }
    unsafe fn deallocate(&self, _ptr: NonNull<u8>, _layout: Layout) { /* no-op */ }
    unsafe fn shrink(&self, ptr, _old, new) -> ... { /* 返回原块 */ }
}
```

要点：

- `allocate` 在 `SpinningMutexOwned` 的锁内调用 `Bump::try_alloc_layout`；
  `deallocate` 是 no-op，arena 在 `BumpAlloc` 析构时整体释放；
- `shrink` 直接返回原块（bump 无法原地收缩），`Vec::shrink_to_fit` 不会浪费新块；
- 零尺寸分配返回满足 `layout.align()` 的非空悬垂指针；
- 不实现 `Clone`：clone 一个 arena 没有共享语义；需要共享的调用方自行 `Arc` 包装；
- `BumpAlloc` 不保证 `Send` / `Sync`（`Bump` 本身 `!Sync`），`Arc<T, BumpAlloc>`
  不能跨线程 `Send`，单线程解码不受影响。

## 验证

- `abs_denco/src/de/bump_.rs` 的单元测试（3 个）：
  - `vec_in_bump_alloc_grows_and_shrinks`：`Vec<u32, BumpAlloc>` 推入 1024 个元素并
    `shrink_to_fit`；
  - `raw_allocate_roundtrip`：直接 `allocate` / 写入 / 读回 / `deallocate`；
  - `zero_sized_allocation_is_dangling`：零尺寸 + 16 字节对齐。
- `mp_denco/tests/bump_alloc_.rs`（2 个）：
  - `decode_vec_with_bump_alloc`：MessagePack 数组解码为 `Vec<u8, BumpAlloc>`；
  - `decode_box_rc_arc_with_bump_alloc`：`Box<u32, BumpAlloc>`、`Rc<u32, BumpAlloc>`、
    `Arc<u32, BumpAlloc>` 都能解码。

结论：**`BumpAlloc` 让「容器用 bump」这条路径零改动落地**，`Rc` / `Arc` 的自定义
allocator 路径也被验证可用。

## 限制

1. **`String` 仍不支持**：`alloc::string::String` 没有 allocator 参数。需要
   `bumpalo::collections::String<'bump>` 或 `Cow` 借用路径。
2. **每个容器一个独立 arena**：`Default` 每次创建新的 `Bump`，所以 `Foo` 的每个
   `Vec<T, BumpAlloc>` 字段各自拥有 arena。好处是自包含、无需外部生命周期；代价是
   每个容器至少一个 arena 的初始 chunk（bumpalo 的首块默认约 512 字节起）。
3. **容器不借用外部 arena 的 lifetime**：`Vec<T, BumpAlloc>` 是 `'static` 类型，
   不带 `'a`。如果目标是 `Foo<'a> { items: Vec<u8, &'a Bump> }`，这仍不满足；
   前者解决的是「容器内存走 bump」，不是「容器借用外部 arena」。
4. **`&'a str` 还没接**：解码结果里的 `&'a str` 需要 decoder 持有 `&'bump Bump`，
   把读出的字节复制进 arena 后 `visit_borrowed_str`。这需要 `mp_denco` 的
   `MpDecoder` 携带 `'bump`，以及 `denco_macro` 的生命周期支持（见上一轮报告）。
5. **`Send` / `Sync`**：`BumpAlloc` 不是 `Send + Sync`，跨线程解码需要每线程一个 arena
   或外层加锁；当前没有验证多线程场景。

## 与 `&'bump Bump` 方案的关系

- `BumpAlloc`（本分支）：解决 **L2/L3 容器分配**，`A: Default` 直接可用，零公共 API 改动。
- `&'bump Bump` + decoder 持有 arena（上一轮）：解决 **L1 借用 str/bytes** 和
  **容器借用外部 arena**，但需要 `TrDecoder`/`TrDecodable` 的 allocator 注入改造。
- 两者可以共存：`Foo<'a> { name: &'a str, items: Vec<u8, BumpAlloc> }`，`name` 借用
  decoder 的 arena，`items` 拥有自己的 arena。

## 未验证

- `no_std + alloc` 下的完整构建（当前测试在 std 下跑）；
- 多线程 / 跨线程 `Arc<T, BumpAlloc>`；
- 大容器与高频 `grow` 的内存放大（bump 语义下旧块不回收）；
- `BumpAlloc` 的 `Clone` 语义（当前刻意不实现）。
