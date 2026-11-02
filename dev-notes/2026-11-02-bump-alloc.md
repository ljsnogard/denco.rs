# 把 bumpalo 移到 mp_denco，以及借用 allocator 的 abs_denco 限制

**日期**：2026-11-02
**分支**：`feat/bump-alloc`
**结论**：`BumpAlloc` / `BumpString` 可以在 `mp_denco` 内独立实现；但「decoder 借用
allocator 并把借用结果交给 `TrDecodable`」会触及 `abs_denco` 的 GAT 接口限制，
`mp_denco` / `json_denco` 单独无法完成。

## 1. 为什么回滚 abs_denco

`abs_denco` 的定位是 `serde` 式抽象层：只定义 trait 与约束，不绑定具体格式或
具体分配器。上一轮把 `BumpAlloc` 放进 `abs_denco` 并加 `bump` feature，违背了
这个定位。本分支已 `git revert` 该提交，`abs_denco` 恢复原有接口；bumpalo 相关
代码全部移到 `mp_denco`。

## 2. mp_denco 的实现

### 2.1 `BumpAlloc`

`mp_denco/src/alloc_.rs`（feature `bump`）：

- `BumpAlloc` 内部是 `SpinningMutexOwned<Bump>`；
- 实现 `Default`（每次新建 arena）与 `Allocator`；
- **不实现 `Clone`**：clone arena 没有共享语义；
- `allocate` 在锁内 `Bump::try_alloc_layout`，`deallocate` no-op，`shrink` 返回原块。

它使 `abs_denco` 现有 `A: Allocator + Default` 的容器实现直接可用：

```rust
let value: Vec<u8, BumpAlloc> = Vec::<u8, BumpAlloc>::decode_async(de)....;
```

### 2.2 `BumpString<A>`

标准库的 `String` 没有 allocator 参数。实测（当前 nightly 1.101）：

```text
error[E0107]: struct takes 0 generic arguments but 1 generic argument was supplied
error[E0599]: no associated function or constant named `new_in` found for struct `String`
```

`BumpString<A>` 是 `Vec<u8, A>` 的薄封装：

- 只接受 `&str` / `String` 构造，内容始终是合法 UTF-8；
- `Deref<Target = str>` / `AsRef<str>` / `Debug` / `Display`；
- `TrDecodable<'de>` 要求 `A: Allocator + Default`，用 `A::default()` 新建 arena，
  visitor 在 `visit_str` / `visit_string` 里把字节复制进去。

测试（`mp_denco/tests/bump_alloc_.rs`）：

- `decode_vec_with_bump_alloc`：MessagePack 数组解码为 `Vec<u8, BumpAlloc>`；
- `decode_bump_string`：MessagePack fixstr 解码为 `BumpString<BumpAlloc>`。

`cargo test -p mp_denco --features bump --test bump_alloc_` 通过。

## 3. 借用 allocator 的尝试与失败

### 3.1 `MpBumpDecoder<'r, 'alloc, R, A>`

目标：decoder 持有 `&'alloc mut A`，读到的字符串复制进 allocator，再
`visit_borrowed_str`。实现 `impl<'de, 'r, 'alloc, R, A> TrDecoder<'de> for
MpBumpDecoder<'r, 'alloc, R, A> where 'alloc: 'de`。

失败点：GAT 的 future 类型包含 `'alloc`（`BumpStrOpAsync<'de, 'r, 'alloc, 'x, R, V, A>`），
`gen_mcf2` 因此要求 `V: 'alloc`。但 `TrDecoder` 的 GAT 声明只有
`V: Visitor<'de> + 'a`（`'a` 是 future 寿命），impl 的 GAT where 补
`V: 'alloc` 仍报：

```text
error[E0309]: the parameter type `V` may not live long enough
  ... so that the type `V` will meet its required lifetime bounds
```

`'alloc` 是 impl 的泛型，trait 的 GAT 看不到它，所以无法在 trait 契约里表达这条约束。

### 3.2 `BorrowedStrSeed<'a, A>`

目标：不改 `TrDecoder`，改用 `TrDecodableSeed` 自带 `&'a mut A`，在 seed 内复制字符串
并返回 `Value = &'a str`。

失败点相同：`TrDecodableSeed` 的 `DecodeAsync<'x, D>` 的 where 只有
`'de: 'x, D: TrDecoder<'de> + 'x, D::Error: 'x, Self: 'x, Self::Value: 'x`；
helper future 需要 `D: 'a` / `D::Error: 'a`，而 GAT 无法加这两条（`'a` 是 seed 的
内部借用寿命），报 `E0309`；在 GAT where 里补 `D: 'a` / `D::Error: 'a` 又与 trait
声明不一致。

### 3.3 结论

**`abs_denco` 的 `TrDecoder<'de>` / `TrDecodableSeed<'de>` 只带一个生命周期参数，
GAT 的 future 类型无法同时表达「allocator 的借用寿命」与「visitor/decoder 的
约束」。** 因此：

- `mp_denco` 可以在 `Default` 语义下使用 bump（每个容器/字符串一个 arena），
  但不能让解码结果借用 decoder 传入的 allocator；
- `json_denco` 的结构相同（`JsonDecoder` 也实现 `TrDecoder<'de>`，字符串 owned），
  结论应一致（未实测）；
- 要真正「Decoder 接收 `BorrowMut<Allocator>` 并让容器/字符串借用」，必须改
  `abs_denco` 的接口。

## 4. 对 abs_denco 的接口建议（需要团队讨论）

1. **给 `TrDecoder` 增加 allocator 生命周期/类型参数**：`TrDecoder<'de, 'alloc>`
   或 `TrDecoder<'de, A: Allocator>`。GAT 里就能写 `V: 'alloc` / `D: 'alloc`，
   容器实现也能用 `D::Alloc` 注入。
2. **给 `TrDecodable` 增加 allocator 参数**：`TrDecodable<'de, A>`，与
   `TrDecoder<'de, Alloc = A>` 对齐；改动面覆盖所有实现与 derive 生成代码。
3. **保持现状**：接受「容器/字符串各自 `Default` arena」的语义（本分支的
   `BumpAlloc` / `BumpString`），把「借用外部 allocator」留到接口改造之后。

## 5. 未验证

- `json_denco` 的相同改造（结构类似，未实测）；
- `BumpAlloc` 的 `Send` / `Sync`（`Bump` 本身 `!Sync`，多线程解码未测）；
- 大容器 / 高频 grow 的 arena 内存放大；
- `abs_denco` 接口改造后的完整编译与 derive 影响。
