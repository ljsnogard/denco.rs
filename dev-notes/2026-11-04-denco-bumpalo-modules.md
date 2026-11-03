# `denco_bumpalo` 源码按职责拆分模块

**日期**：2026-11-04
**分支**：`exp/remove-de`
**结论**：把原本 440 行、全部堆在 `lib.rs` 的实现拆成 `arena_` / `box_` /
`string_` 三个模块；`lib.rs` 只保留 crate 文档、`mod` 声明和逐项 `pub use`，
公开路径不变。

## 1. 背景

`denco_bumpalo/src/lib.rs` 同时包含 arena 分配器、`BumpBox` 句柄、`BumpString`
容器，以及三者的 `TrArena` / `TrArenaBytes` / `TrStrFromUtf8` / `TrDecodable`
实现与 `gen_mcf2` helper。文件接近 500 行，不符合 AGENTS.md 对模块划分的要求：

- 多个职责共处一个文件，查找与评审成本高；
- `mod.rs` / crate 根不应承载实际逻辑，只应控制导出层级；
- 导出项必须逐项列出，不能 `*`。

## 2. 拆分结果

| 文件 | 内容 |
| --- | --- |
| `src/lib.rs` | crate 文档、`#![no_std]`、`mod` 声明、逐项 `pub use`（36 行） |
| `src/arena_.rs` | `BumpArena`、`BumpArenaError`、`Allocator` / `TrArena` / `TrArenaBytes` 实现 |
| `src/box_.rs` | `BumpBox`、`Deref` / `TrBoxed` / `TrUnique`、引用版 `TrDecodable` 与 helper |
| `src/string_.rs` | `BumpString`、`TrStrFromUtf8`、字符串解码 helper 与 `TrDecodable` |

导出保持原样，公开类型仍是：

```rust
pub use arena_::{BumpArena, BumpArenaError};
pub use box_::BumpBox;
pub use string_::BumpString;
```

因此 `denco_bumpalo::BumpArena` / `::BumpBox` / `::BumpString` /
`::BumpArenaError` 全部不变；模块本身保持私有，不新增公开路径。

## 3. 跨模块可见性

- `BumpBox::from_raw_` 改为 `pub(crate)`，仅供 `arena_` 的 `TrArena` 实现使用；
- `BumpArena` 由 `arena_` 导出到 crate 根，`box_` 通过 `crate::arena_::BumpArena`
  使用；
- `BumpString.bytes_`、`BumpArena.inner_` 仍是模块私有字段，命名保持 `_` 结尾；
- `gen_mcf2` 生成的 helper 仍为 `pub`（沿用 `abs_denco` 的做法：private module
  里的 `pub` 类型可以被公开 impl 的 GAT 引用）。

## 4. 验证

- `cargo test -p denco_bumpalo --all-features`：6 个测试块，14 passed / 0 failed；
- `cargo test --workspace --all-features`：32 个测试块，150 passed / 0 failed /
  18 ignored（与拆分前一致）；
- `cargo clippy -p denco_bumpalo --all-targets --all-features -- -D warnings`：通过；
- `RUSTDOCFLAGS="-D warnings" cargo doc -p denco_bumpalo --all-features --no-deps`：
  通过；
- `python3 format_use_imports.py --check denco/denco_bumpalo`：合规。
