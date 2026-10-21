# AGENTS.md

Guidance for coding agents (Claude Code and others) working in this repository.
`CLAUDE.md` imports this file, so keep shared instructions here.

## Project overview

**retype** is a small, open-source Windows utility that fixes text typed in the
wrong keyboard layout (the classic "ghbdtn" → "привет" problem), in the spirit of
Punto Switcher, but with hard privacy guarantees:

- no network access of any kind (no updates, no telemetry, no "cloud" features);
- typed text is never written to disk and never logged;
- one small native `.exe`, no installer required.

Full requirements: `docs/SPEC.md`. Design: `docs/ARCHITECTURE.md`.
Plan and current status: `ROADMAP.md`. Past decisions: `docs/DECISIONS.md`.

The owner communicates in Russian. Reply to the user in Russian; keep code,
identifiers, comments, commit messages and repository docs in English.

## Repository layout

```
retype/
├── crates/
│   ├── retype-core/        # Pure logic, no OS calls, #![forbid(unsafe_code)], tested on any OS
│   └── retype/             # Windows app: hook, SendInput, layouts, clipboard, tray
│       └── src/win/        # The ONLY place where unsafe / WinAPI code may live
├── docs/                   # SPEC, ARCHITECTURE, DECISIONS, winapi notes, manual test log
├── config.example.toml     # Documented example of the user config
├── deny.toml               # cargo-deny: licenses + banned (network) crates
└── .claude/                # rules, skills, subagents, settings for Claude Code
```

## Commands

| Task                       | Command                                                      |
|----------------------------|--------------------------------------------------------------|
| Fast check                 | `cargo check --workspace`                                    |
| Format                     | `cargo fmt --all`                                            |
| Lint (must be clean)       | `cargo clippy --workspace --all-targets -- -D warnings`      |
| Test                       | `cargo test --workspace`                                     |
| Core only (any OS)         | `cargo test -p retype-core`                                  |
| Dependency policy          | `cargo deny check` (if cargo-deny is installed)              |
| Release build (Windows)    | `cargo build --release -p retype`                            |
| Full verification          | `/verify` skill                                              |

The app crate only does real work on Windows. On other systems it compiles to a
stub, so `cargo test --workspace` works everywhere; Windows-specific code must be
checked with `cargo check -p retype --target x86_64-pc-windows-msvc` (or `-gnu`).

## Ground rules

1. **Logic goes into `retype-core`.** Anything that can be expressed without the
   OS (buffers, decisions, conversion plans, layout tables as data, config) is
   written and unit-tested in the core crate. The Windows crate is thin glue.
2. **`unsafe` only in `crates/retype/src/win/`**, each block with a `// SAFETY:`
   comment, each WinAPI call wrapped in a small safe function.
3. **Privacy invariants are non-negotiable** (see `.claude/rules/security.md`):
   no networking code or crates, never log or persist key content.
4. **No new dependencies without asking the user.** Allowed today: `windows`,
   `log`, a tiny logger, `serde`, `toml`, `anyhow` (app only). Record every new
   dependency in `docs/DECISIONS.md`.
5. **Finish every task with `/verify`** and do not report done while it fails.
6. Work in small steps that follow `ROADMAP.md`; tick items off there when done.
7. Never use `println!`/`eprintln!`/`dbg!` outside tests; use `log` macros.

## Markdown

Space-pad table cells so columns line up in plain text (docs are read raw in
terminals and diffs as often as rendered).
