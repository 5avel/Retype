---
paths:
  - "crates/**/*.rs"
  - "**/Cargo.toml"
---

# Rust style

## Basics

- Latest stable Rust, edition 2024. Formatting is `cargo fmt` (a hook runs it
  after every edit) — don't hand-format against it.
- `cargo clippy --workspace --all-targets -- -D warnings` must be clean.
  Workspace lints live in the root `Cargo.toml`; don't silence them with
  `#[allow]` unless you add a comment explaining why.
- No `println!`, `eprintln!`, `dbg!` outside tests; use `log::{error,warn,info,debug}`.
- No `unwrap()`/`expect()` in non-test code except for invariants that are
  truly impossible, with a comment saying why.

## Errors

- `retype-core`: own error enums per module (`ConfigError`, …) implementing
  `std::error::Error` + `Display`. No `anyhow` in the core.
- `retype` app: `anyhow::Result` with `.context("…")` at boundaries.

## Naming

- Optional values that are "maybe present" use a `maybe_` prefix:
  `maybe_target_layout`, `maybe_last_word`.
- Types are nouns (`KeyBuffer`, `ConversionPlan`), functions are verbs.
- WinAPI wrappers keep the Win32 name in snake_case where it helps grep:
  `get_keyboard_layout`, `send_input_keys`.

## File layout (top-to-bottom reading)

1. Constants after imports.
2. The primary public type of the file, then its supporting types.
3. `impl` blocks: constructors, public methods, private helpers.
   A helper sits *below* the method that calls it.
4. `#[cfg(test)] mod tests` at the bottom.

## Imports

- Groups: std, external crates, workspace crates, `crate::`.
- Prefer `use` over inline long paths; inline only for one-offs.
- When names collide, alias with a suffix: `use windows::Win32::Foundation::HWND as HwndWin;`

## Modules

- One concept per file. Keep files under ~300 lines; split when larger.
- Public API of `retype-core` is re-exported from `lib.rs`; keep it small.
