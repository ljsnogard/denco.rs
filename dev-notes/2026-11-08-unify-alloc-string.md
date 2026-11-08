# 统一 allocator-aware String：移除 `denco_bumpalo::BumpString`

**日期**：2026-11-08
**分支**：`main`
**结论**：`abs_str::str_alloc::String<A>` 是 denco 唯一的 allocator-aware
字符串类型。`denco_bumpalo` 删除自己的 `BumpString` 实现，改为
`pub use abs_denco::x_deps::abs_str::str_alloc::String;`；编解码实现继续由
`abs_denco` 的 `str_alloc` feature 提供。

## 1. 背景

`abs_denco` 在 `2026-11-04` 引入了 `abs_str::str_alloc::String<A>` 的编解码
支持（见 [`2026-11-04-str-alloc.md`](2026-11-04-str-alloc.md)），但
`denco_bumpalo` 仍保留着更早的 `BumpString<A>`。两者都是
「`Vec<u8, A>` 包成合法 UTF-8 字符串」，都通过 `TrAllocSource` 从 decoder
投影 allocator，却是不兼容的两个类型：

- 使用者要在 `BumpString` 和 `abs_str::String` 之间二选一；
- 两个类型各自带一套 `TrStrFromUtf8` / `TrDecodable`，行为需要同步维护；
- `BumpString` 没有 `TrEncodable`，而 `abs_str::String` 有，能力面不一致。

## 2. 功能并集

以 `abs_str-rs` 的设计为底线，逐项对比两个类型：

| 能力 | `BumpString` | `abs_str::str_alloc::String` |
| --- | --- | --- |
| `new_in` | 有 | 有 |
| `Deref<Target = str>` | 有 | 有 |
| `AsRef<str>` / `AsRef<[u8]>` | 仅 `AsRef<str>` | 都有 |
| `Display` | 有 | 有 |
| `Debug` | 按字符串内容输出 | 派生 `Debug`（显示内部 `Vec`） |
| `Default` | 有（`A: Default`） | 无 |
| `TrStrFromUtf8` / `TrDecodable` | `denco_bumpalo` 提供 | `abs_denco` 提供 |
| `TrEncodable` | 无 | `abs_denco` 提供 |
| 字符串操作（`push_str` / `truncate` / `retain` 等） | 无 | 完整 |
| 与标准 `String` 的转换、比较、哈希、`fmt::Write` | 无 | 完整 |

结论：`abs_str::String` 覆盖了 `BumpString` 的全部用途，并在编码、字符串操作
等方面更多。`BumpString` 额外的 `as_str_` / `as_bytes_` 只是 `Deref` /
`AsRef` 的薄包装；`Default` 没有直接对应的 trait 实现，空串用
`String::new_in(Global)` 或 `String::from_str_in("", Global)` 构造；`Debug`
从「按内容输出」变为派生实现。除这些 trait 层面的差异外，不再需要保留独立
实现。

## 3. 实现

- 删除 `denco_bumpalo/src/string_.rs`（`BumpString` 类型、`TrStrFromUtf8`、
  `DecodeBumpStringOp` 与 `TrDecodable` 实现）；
- `denco_bumpalo/src/lib.rs`：
  - 删除 `mod string_;`；
  - 新增 `pub use abs_denco::x_deps::abs_str::str_alloc::String;`；
  - crate 文档改为说明「字符串类型直接重导出 `abs_str` 的实现」；
- `denco_bumpalo/Cargo.toml`：对 `abs_denco` 显式开启 `str_alloc`，保证
  `x_deps::abs_str` 存在，不依赖 `abs_denco` 的默认 feature 长期不变；
- `abs_denco/src/de/str_.rs`、`abs_denco/src/de/decodable_.rs` 中指向
  `BumpString` 的文档改为 `abs_str::str_alloc::String`；
- 测试改用 `denco_bumpalo::String as AllocString`，并新增类型同一性测试，
  确认它只是重导出而不是新类型；
- `abs_denco` 的 feature 文档同步修正为「默认开启 `str_alloc`，其余可选」。

## 4. 公开 API 变化

这是有意的破坏性变更，已在本轮确认：

| 旧 | 新 |
| --- | --- |
| `denco_bumpalo::BumpString<A>` | `denco_bumpalo::String<A>` |
| `BumpString::as_str_()` | `String::deref()` / `AsRef<str>::as_ref()` |
| `BumpString::as_bytes_()` | `String::as_bytes()`（经 `Deref<Target = str>`） |
| `BumpString::<A>::default()` | `String::new_in(A::default())` |
| `denco_bumpalo::DecodeBumpStringOp`（关联类型投影） | `abs_denco` 的 `DecodeAllocStringOp`（实现细节，不承诺名称） |

`denco_bumpalo::String` 同时获得 `TrEncodable`，这是旧 `BumpString` 没有的
能力。

## 5. 验证

- `cargo test -p denco_bumpalo --all-features --offline`：22 个测试通过；
- `cargo test --workspace --all-features --offline`：通过；
- `cargo clippy --workspace --all-targets --all-features --offline -- -D warnings`：
  通过；
- `RUSTDOCFLAGS="-D warnings" cargo doc --workspace --all-features --offline --no-deps`：
  通过；
- `python3 format_use_imports.py --check denco/denco_bumpalo denco/abs_denco/src/lib.rs denco/abs_denco/src/de/str_.rs denco/abs_denco/src/de/decodable_.rs`：
  合规。

## 6. 后续建议

如果确实需要 `Default` 或与旧 `BumpString` 完全一致的内容式 `Debug`，应在
`abs_str-rs` 中补齐对应实现，再由 denco 重导出；不要在 `denco_bumpalo` 里新增
包装层，否则会重新引入本次要消除的重复字符串类型。
