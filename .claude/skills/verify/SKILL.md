---
name: verify
description: Run the full verification pipeline (fmt, clippy, tests, policy, dependency bans, Windows target check) and report results. Use before saying a task is done, before committing, or when the user asks to check the build.
allowed-tools: Bash(cargo fmt *) Bash(cargo clippy *) Bash(cargo test *) Bash(cargo check *) Bash(cargo deny *) Bash(rustup target list *) Bash(git status *) Bash(git diff *)
---

# Verify

Run these steps in order. Do not stop at the first failure: run them all, then
fix what failed and re-run the failed steps until everything is green.

1. **Format**: `cargo fmt --all -- --check`
2. **Lint**: `cargo clippy --workspace --all-targets -- -D warnings`
3. **Tests**: `cargo test --workspace`
   (includes the policy test `crates/retype/tests/policy.rs`)
4. **Windows target** (only when not running on Windows): if
   `rustup target list --installed` shows a `*-pc-windows-*` target, run
   `cargo clippy -p retype --target <that target> -- -D warnings`.
   If none is installed, say so in the report — Windows code was not compiled.
5. **Dependency policy**: `cargo deny check` if `cargo deny` is available;
   otherwise report it as skipped (CI runs it).
6. **Hygiene**: `git status` — no stray files, no `target/` or logs staged.

## Report

Reply with a short table:

| Step              | Result            |
|-------------------|-------------------|
| fmt               | ok / fixed / FAIL |
| clippy            | …                 |
| tests             | N passed          |
| windows target    | ok / skipped (why)|
| cargo-deny        | ok / skipped      |

Never describe a step as passing if it was skipped. If something cannot be
fixed without a design decision, stop and ask the user.
