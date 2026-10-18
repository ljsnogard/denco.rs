# 标准库容器支持：工作量、实施方案与扩展路径

**当前结论（已实施）**：`abs_denco` 已通过可选的 feature 提供标准库容器实现：

| feature | 增加的实现 |
| --- | --- |
| `alloc-collections` | `BTreeMap` / `BTreeSet` / `VecDeque` / `LinkedList` / `BinaryHeap` |
| `rc` | `Rc<T>`（`Arc<T>` 保持无条件提供） |
| `cow` | `Cow<'a, str>` / `Cow<'a, [T]>`（解码一律得到 `Owned`） |
| `std` | `HashMap` / `HashSet`；隐含 `alloc-collections`，并启用 `extern crate std` |

默认 feature 为空，`cargo check -p abs_denco` 仍然只依赖 `alloc`。
另外无条件补了 `impl TrEncodable for &T`，容器实现复用 `TrEncoderExt::collect_seq` /
`collect_map` 与两个泛型解码 visitor。**`denco_macro` 不需要改动**：派生只按字段类型
调用 `abs_denco` 的 trait，容器实现补齐后派生自动可用。实测 10 个容器集成测试通过
（含派生结构体 + 容器字段、以及自定义 allocator），`cargo test -p mp_denco` 全量无回归，
`cargo clippy --all-features -D warnings` 通过。

所有实现随后在 `feat/allocator-generic` 分支上泛化到 `A: Allocator`（含解码构造的
`A: Default` 约束），详见 §9。

## 1. 现状盘点

`abs_denco/src/ser/impls_.rs`（275 行）已覆盖：标量、`str`/`String`、`[T]`/`[T; N]`/
`Vec<T>`、`Option<T>`、`Box<T>`、`Arc<T>`。`abs_denco/src/de/impls_.rs`（937 行）覆盖对应的
解码实现。格式层（`mp_denco`）已经支持 `encode_seq` / `encode_map` / `decode_seq` /
`decode_map`，所以**容器支持是 `abs_denco` 单层的事**，`mp_denco` / `json_denco` 不需要动。

已具备的、可以直接复用的零件：

- 编码：`TrEncoderExt::collect_seq` / `collect_map`（`ser/ext_.rs`）已经实现了
  「按 `IntoIterator` 编码序列 / 映射」，并会利用 `size_hint` 写长度头；
- 解码：`SeqAccessExt::next_element`、`MapAccessExt::next_entry` 已经封装了逐元素/逐键值
  的 seed 读取；
- 容器构造：`FromIterator` 足以覆盖所有标准容器（`BTreeMap` 是
  `FromIterator<(K, V)>`，`BTreeSet`/`VecDeque`/`LinkedList`/`BinaryHeap` 是
  `FromIterator<T>`，`HashMap` 是 `FromIterator<(K, V)>`）。

缺口有三个：

1. 没有 `impl TrEncodable for &T`，因此 `container.iter()` 产出的 `&T` 不能直接喂给
   `collect_seq` / `collect_map`（`I::Item: TrEncodable` 不成立）；
2. 没有泛型的「序列 → 容器」「映射 → 容器」解码 visitor，每个容器都得复制一份
   `Vec` 的 visitor + helper；
3. `HashMap` / `HashSet` 不在 `alloc`，需要给 `abs_denco` 增加可选的 `std` feature。

### 1.1 可行性验证（已做原型，随后还原）

按 §2 的形状在 `abs_denco` 内临时实现了 `&T`、`VecDeque`、`BTreeMap`、`Rc` 的编解码，
`cargo check -p abs_denco` 通过。踩到的三个点已写进实现方案：

- `collect_seq` / `collect_map` 生成的 `Collect*OpAsync` 与解码 helper future 出现在公开
  类型的 GAT 上时，必须声明为 `pub`，否则 `E0446: private type in public interface`；
- 泛型收集 helper 的类型参数 `C` 需要显式的 `C: 'a` 约束，否则 `E0309`；
- `Collect*OpAsync` 的 `new` 需要 `pub(crate)` 才能在兄弟模块里构造。

这说明剩余工作主要是**量**（测试与一容器一实现），不是类型系统上的未知数。

## 2. 实施方案

### 2.1 编码侧：一个 `&T` 实现 + 复用现成 helper

先加一个转发实现（与 serde 的 `impl Serialize for &T` 同形）：

```rust
impl<T> TrEncodable for &T
where
    T: ?Sized + TrEncodable,
{
    type EncodeAsync<'a, S>
        = T::EncodeAsync<'a, S>
    where
        Self: 'a,
        T: 'a,
        S: TrEncoder + 'a,
        S::Ok: 'a,
        S::Error: 'a;

    fn encode_async<'a, S>(&'a self, encoder: S) -> Self::EncodeAsync<'a, S>
    where
        Self: 'a,
        T: 'a,
        S: TrEncoder + 'a,
        S::Ok: 'a,
        S::Error: 'a,
    {
        T::encode_async(*self, encoder)
    }
}
```

然后把 `CollectSeqOp` / `CollectMapOp` 生成的类型提升为 `pub`（`new` 设为 `pub(crate)`），
容器实现就能直接命名它：

```rust
impl<K, V> TrEncodable for BTreeMap<K, V>
where
    K: TrEncodable + Ord,
    V: TrEncodable,
{
    type EncodeAsync<'a, S>
        = CollectMapOpAsync<'a, 'a, S, &'a K, &'a V, btree_map::Iter<'a, K, V>>
    where
        S: TrEncoder + 'a,
        S::Ok: 'a,
        S::Error: 'a,
        Self: 'a;

    fn encode_async<'a, S>(&'a self, encoder: S) -> Self::EncodeAsync<'a, S>
    where
        S: TrEncoder + 'a,
        S::Ok: 'a,
        S::Error: 'a,
        Self: 'a,
    {
        CollectMapOpAsync::new(encoder, self.iter())
    }
}
```

`BTreeSet` / `VecDeque` / `LinkedList` / `BinaryHeap` 把 `self.iter()` 交给
`CollectSeqOpAsync` 即可；每个容器约 25 行。注意 `BinaryHeap` 的遍历顺序不保证，
线序只是「一堆元素」，往返不承诺顺序。

### 2.2 解码侧：两个泛型 visitor，容器只写薄壳

初版用 `FromIterator` 参数化目标容器、visitor 只写一次；allocator 泛化时发现标准库
只为默认 allocator 实现 `FromIterator`，于是改成 `TrSeqSink` / `TrMapSink`
（`empty_` + `push_` / `insert_`），见 §9。原先的形状如下，保留作对照：

```rust
struct SeqCollectVisitor<T, C>(PhantomData<(T, C)>);

impl<'de, T, C> Visitor<'de> for SeqCollectVisitor<T, C>
where
    T: TrDecodable<'de> + 'de,
    C: FromIterator<T>,
{
    type Value = C;

    fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("a sequence")
    }

    fn visit_seq<'a, A>(
        self,
        seq: A,
    ) -> impl TrMayCancel<'a, MayCancelOutput = Result<Self::Value, A::Error>>
    where
        'de: 'a, Self: 'a, Self::Value: 'a,
        A: SeqAccess<'de> + 'a,
        A::Error: 'a,
    {
        DecodeSeqCollectOpAsync::<'de, 'a, 'a, T, C, A>::new(seq)
    }
}

#[gen_mcf2::gen_may_cancel_future(DecodeSeqCollectOp)]
async fn decode_seq_collect_async_<'de, 'a, T, C, A, Tok>(
    mut seq: A,
    cancel: Tok,
) -> Result<C, A::Error>
where
    'de: 'a,
    T: TrDecodable<'de> + 'de,
    C: FromIterator<T> + 'a,
    A: SeqAccess<'de> + 'a,
    A::Error: 'a,
    Tok: TrCancellationToken,
{
    let mut out = Vec::new();
    while let Some(value) = seq
        .next_element_seed(PhantomData::<T>)
        .may_cancel_with(cancel.child_token())
        .await?
    {
        out.push(value);
    }
    Ok(out.into_iter().collect())
}
```

映射版只需把循环换成 `map.next_entry::<K, V>()`、把约束换成
`C: FromIterator<(K, V)> + 'a`。随后每个容器：

```rust
impl<'de, K, V> TrDecodable<'de> for BTreeMap<K, V>
where
    K: TrDecodable<'de> + 'de + Ord,
    V: TrDecodable<'de> + 'de,
{
    type DecodeAsync<'a, D>
        = DecodeMapCollectOpAsync<'de, 'a, 'a, K, V, BTreeMap<K, V>, D>
    where
        'de: 'a,
        D: TrDecoder<'de> + 'a,
        D::Error: 'a,
        Self: 'a;

    fn decode_async<'a, D>(de: D) -> Self::DecodeAsync<'a, D>
    /* ... */ {
        DecodeMapCollectOpAsync::new(de)
    }
}
```

每个容器 25–35 行。重复键语义与 serde 一致：`FromIterator` 后者覆盖前者，不报错。

### 2.3 `std` 容器：独立的可选 feature

`BTreeMap` / `BTreeSet` / `VecDeque` / `LinkedList` / `BinaryHeap` / `Rc` 都在 `alloc`，
不需要动 `no_std`。`HashMap` / `HashSet` 需要：

实际采用一个 `std` feature，同时承载「链接 std」与「`HashMap`/`HashSet` 实现」：

```toml
[features]
std = ["alloc-collections"]
```

```rust
#[cfg(feature = "std")]
extern crate std;
```

`std` 隐含 `alloc-collections`，与 serde 的 `std => alloc` 一致。实现放在
`#[cfg(feature = "std")]` 的模块里。解码的约束是 `S: BuildHasher + Default`、
`K: Eq + Hash`：

```rust
impl<'de, K, V, S> TrDecodable<'de> for HashMap<K, V, S>
where
    K: TrDecodable<'de> + 'de + Eq + Hash,
    V: TrDecodable<'de> + 'de,
    S: BuildHasher + Default,
{ /* 用 DecodeMapCollectOpAsync，C = HashMap<K, V, S> */ }
```

`std` 建议**默认关闭**，避免把 no_std 目标挡在门外；`mp_denco` 的互操作测试在
`dev-dependencies` 里开 `abs_denco/std` 即可。

### 2.4 模块划分

实际按 feature 拆成独立文件，避免 `impls_.rs` 继续膨胀：

- `ser/alloc_collections_.rs` + `de/alloc_collections_.rs`：五个 alloc 容器与 `BTreeMap`；
- `de/collect_.rs`：两个泛型收集 visitor（`SeqCollectVisitor` / `MapCollectVisitor`）；
- `ser/std_collections_.rs` + `de/std_collections_.rs`：`HashMap` / `HashSet`；
- `ser/rc_.rs` + `de/rc_.rs`、`ser/cow_.rs` + `de/cow_.rs`；
- `&T` 的转发实现放在无条件的 `ser/impls_.rs`。

容器的互操作测试放在新文件 `mp_denco/tests/containers_.rs`，`mp_denco` 的
`dev-dependencies` 打开全部容器 feature。

## 3. `denco_macro` 需要做什么

**基础容器支持：零改动。** 派生宏对每个命名字段生成的是对字段类型本身的
`TrEncodable` / `TrDecodable` 调用；对结构体的类型参数则统一加
`T: TrEncodable` / `T: TrDecodable<'de> + 'de` 约束。因此：

```rust
#[derive(TrEncodable, TrDecodable)]
struct Payload {
    headers: BTreeMap<String, String>,
    scores: BTreeMap<u64, Vec<u32>>,
}
```

只要 `abs_denco` 补上 `BTreeMap` 与 `&T`（`&String`、`&u64` 等）的实现就能直接工作。
需要用户自己负责的只有容器特有的额外约束（如 `K: Ord`）：把约束写在类型声明或
`where` 子句上，派生会原样保留。

真正需要动宏的是两类**可选扩展**：

1. **借用型容器**（`Cow<'a, str>`、`&'a str`、`BTreeMap<&'a str, _>`）：`model_.rs`
   现在用 `reject_lifetimes_` 明确拒绝生命周期参数。要支持就得让派生的 `DecodeAsync`
   把 `'de` 与类型的 `'a` 关联起来，并生成 `T: TrDecodable<'de> + 'de` 之外的
   `'de: 'a` 约束。改动集中在 `model_.rs` + `decode_.rs`，约 300–500 行并需要重跑全部
   派生测试，风险高于容器实现本身；而且 `MpDecoder` 目前一律解成 owned 值，
   `&'de str` 这类零拷贝解码没有格式层支撑，收益有限。
2. **自定义容器表示**（例如把 `HashMap` 编成有序的键值数组以获得确定性字节）：
   宏目前没有 `with` / `serialize_with` 一类按字段覆盖的入口。加
   `#[denco(with = "path")]` 约 150–250 行，属于独立的特性，不是容器支持的前置条件。

## 4. 工作量拆分（事前估算，供后续同类扩展参考）

| 项目 | 实现量（约） | 测试量（约） | 熟练者工期 |
| --- | --- | --- | --- |
| 地基：`&T`、`Collect*Op` 可见性、两个泛型解码 visitor | 300–350 行 | 250–300 行 | 0.5–1 天 |
| `BTreeMap` | 60–80 行 | 100–150 行 | 0.3 天 |
| `BTreeSet` / `VecDeque` / `LinkedList` / `BinaryHeap` | 120–160 行 | 250–350 行 | 0.5 天 |
| `Rc` | 40–50 行 | 60–80 行 | 0.2 天 |
| `std` feature + `HashMap` / `HashSet` | 120–160 行 | 150–200 行 | 0.5 天 |
| 文档与迁移说明 | ~100 行 | — | 0.3 天 |
| **小计（BTreeMap + alloc + std）** | **~650 行** | **~800 行** | **2–4 人日** |
| 可选：`Result` / 元组（1–16）/ `PhantomData` / `Duration` 等 | 300–500 行 | 300–500 行 | 1–2 天 |
| 可选：派生宏支持生命周期（借用容器） | 300–500 行 | 重跑全部派生测试 | 1–2 天 |

「只做 `BTreeMap` 最小闭环」= 地基 + `BTreeMap` + 测试，约 0.5–1 人日。

## 5. 风险与边界

- **E0446（私有类型出现在公开接口）**：helper future 会出现在公开类型的 GAT 关联类型里，
  必须像现有派生代码那样把 helper 类型声明为 `pub`（放在私有模块里即可）。
- **`gen_mcf2` 的泛型顺序**：最后一个类型参数必须是取消令牌；容器泛型要排在它前面，
  否则宏拒绝展开。
- **`no_std` 矩阵**：默认 feature 必须保持空，CI 要同时跑
  `cargo test -p abs_denco` 与 `cargo test -p abs_denco --features std`。
- **非 `Global` allocator 没有 `FromIterator`**：标准库只为默认 allocator 实现
  `FromIterator`，解码必须改用「`new_in(A::default())` + 逐个插入」。
- **解码的 allocator 来源**：`TrDecodable` 没有额外上下文，只能要求 `A: Default`；
  `&A` 这类没有默认实例的 allocator 需要先解到默认容器再迁移。
- **BTree 的额外约束**：`BTreeMap` / `BTreeSet` 的 allocator 参数要求
  `A: AllocatorClone`，其 `new_in` 还受 nightly `btreemap_alloc` 控制。
- **`HashMap` / `HashSet` 字节不确定**：遍历顺序随机，不能做「字节相等」断言，只能做
  「解码后集合相等」。这一点与 serde 相同，需要在文档里写明。
- **`BinaryHeap` 顺序**：线序不承诺堆序，往返只保证元素集合相同。
- **重复键**：映射解码后写覆盖，不报错；与 serde 的 map 语义一致。
- **借用解码仍不支持**：`&'de str`、`Cow` 的 `Borrowed` 分支没有格式层支撑；`Cow`
  的解码因此一律产出 `Owned`，且不要求 `'de: 'a`。
- **`Cow` 与 `gen_mcf2` 的一个坑**：helper future 的类型参数里，`D` 必须显式满足
  `D: 'f`，其中 `'f` 是专门加进函数泛型的「future 借用生命周期」；否则宏展开内部会报
  `E0309`。实现里把 `'f` 与关联类型的 `'x` 都实例化为同一个生命周期。
- **公共 API 变更**：`impl TrEncodable for &T` 与新增容器实现都是对外承诺，按项目纪律
  需要先确认；coherence 上它们是安全的（不与现有实现重叠）。

## 6. 扩展更多泛型容器的路径

推荐路线是「**显式实现 + 共享 helper**」，与 serde 的策略一致，而不是 blanket impl：

- 内建容器共享私有模块 `de/sink_.rs` 里的 `TrSeqSink` / `TrMapSink`，每个容器只写
  20–30 行；这两个 trait 目前不导出，因为解码 visitor 也只是内部实现细节；
- 第三方容器（`smallvec`、`indexmap`、`hashbrown`）受孤儿规则限制，不能由用户侧直接
  为它们实现 `TrEncodable` / `TrDecodable`（两个 trait 都在 `abs_denco`）；只能由
  `abs_denco` 以可选 feature 提供实现，或由用户在自己的 crate 里用 newtype 适配；
- 用户为**本地**容器手写 `TrDecodable` 时，可以直接使用公开的 `SeqAccess` /
  `MapAccess` / `Visitor`，或者自己按「空容器 + 逐个插入」的循环实现；
- 不能用
  `impl<C: 某个容器 trait> TrEncodable for C` 的 blanket impl：它会与现有的
  `Vec<T>`、`[T; N]` 等具体实现重叠，除非把全部实现都改成经该 trait 转发，
  改动面远大于收益；
- 如果以后要把构造抽象开放给第三方，可以把两个 sink trait 与对应 visitor 导出成
  受支持的扩展 API，但那需要单独讨论。

## 7. 实施顺序（已完成）

1. 地基：`&T` + 泛型 seq/map 解码 helper；
2. `alloc-collections`：`BTreeMap` + `BTreeSet` + `VecDeque` + `LinkedList` + `BinaryHeap`；
3. `rc`：`Rc`；
4. `cow`：`Cow<str>` / `Cow<[T]>`；
5. `std`：`HashMap` / `HashSet`；
6. 测试与文档。

`mptp_smux_v1_demo` 里手写的 `Headers` 编解码现在可以由 `BTreeMap<u16, HeaderVal>`
直接替代，可作为下一轮验收（本次未改动该 demo）。

## 8. 验证命令

```console
$ cargo check -p abs_denco                                  # 默认无容器
$ cargo check -p abs_denco --features alloc-collections
$ cargo check -p abs_denco --features rc
$ cargo check -p abs_denco --features cow
$ cargo check -p abs_denco --features std
$ cargo test  -p abs_denco --all-features
$ cargo test  -p mp_denco --test containers_                # 9 passed
$ cargo test  -p mp_denco                                   # 全量无回归
$ cargo clippy -p abs_denco --all-features --all-targets -- -D warnings
```

## 9. allocator 泛化（`feat/allocator-generic`）

**结论**：所有容器（`Vec` / `Box` / 五个 `alloc::collections` 容器 / `HashMap` /
`HashSet` / `Rc` / `Arc`）的实现都按 `A: Allocator` 泛化，不再只覆盖 `Global`。
解码额外要求 `A: Default`，`BTreeMap` / `BTreeSet` 还要求 `A: AllocatorClone`。

### 9.1 为什么不能沿用 `FromIterator`

标准库中 `FromIterator` 只对默认 allocator 的容器有实现（`Vec<T>`、
`VecDeque<T>`、`BTreeMap<K, V>`、`HashMap<K, V, S>` …），自定义 `A` 没有对应
impl。解码因此改为两个内部构造 trait：

```rust
pub trait TrSeqSink<T>: Sized {
    fn empty_() -> Self;
    fn push_(&mut self, item: T);
}

pub trait TrMapSink<K, V>: Sized {
    fn empty_() -> Self;
    fn insert_(&mut self, key: K, value: V);
}
```

它们放在私有模块 `de/sink_.rs`，为每个容器实现；visitor 遍历元素后调用
`push_` / `insert_`。这样解码与 allocator 无关的部分仍然只写一遍。

### 9.2 allocator 实例从哪来

`TrDecodable` 没有参数可以携带 allocator，所以内建实现统一用
`A::default()` 创建空容器。这带来两个已知限制：

1. `A: Default` 是硬约束；`&A`、需要运行时句柄的 allocator 不满足。
   需要这类 allocator 时，应先解码到默认 allocator 的容器，再自行迁移，或未来
   增加携带 allocator 的 `TrDecodableSeed`。
2. 每次解码都会新建一个 allocator 实例；allocator 若带状态，调用方需要确保
   `Default` 得到的实例与实际使用场景一致。

### 9.3 nightly 特性

当前工具链（1.101.0-nightly）上：

- `Allocator` trait、`Vec<T, A>` / `Box<T, A>` 已稳定（1.100）；
- `VecDeque` / `LinkedList` / `BinaryHeap` / `HashMap` / `HashSet` / `Rc` / `Arc`
  的 allocator 参数与 `new_in` / `with_hasher_in` 仍受 `allocator_ext` 控制；
- `BTreeMap` / `BTreeSet` 的 allocator 参数要求 `AllocatorClone`，`new_in` 还需要
  `btreemap_alloc`。

因此 crate 无条件开启 `#![feature(allocator_ext)]`（`Arc` 的无条件泛化实现也需要它），
`btreemap_alloc` 则仅在 `alloc-collections` feature 下开启。如果未来这些 API 稳定，
可以删除对应的 feature 属性。

### 9.4 验证

`mp_denco/tests/containers_.rs` 新增了两条与 allocator 有关的测试：

- `custom_allocator_containers_`：一个只转发给 `Global`、同时统计 `allocate`
  次数的 `CountingAlloc`（实现了 `Allocator` 与 `AllocatorClone`），覆盖全部容器
  + `Box` / `Rc` / `Arc` 的往返、与 `Global` 版本的字节一致性，并断言分配计数增长；
- `allocator_a1_encode_a2_decode_`：用类型级编号的两个 allocator
  `TaggedAlloc<0>`（A1）与 `TaggedAlloc<1>`（A2）演示「A1 编码、A2 解码」。
  解码值与源值相同、字节与 `Global` 版本一致，且解码期间只有 A2 的分配计数增长。
  这同时说明了 allocator 身份不进入线格式，以及解码确实用目标 allocator 重新分配。

```console
$ cargo test -p mp_denco --test containers_     # 11 passed
$ cargo test -p abs_denco --all-features
$ cargo clippy -p abs_denco --all-features --all-targets -- -D warnings
$ cargo clippy -p mp_denco --all-targets -- -D warnings
```
