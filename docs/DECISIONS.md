# Decision log (ADR-lite)

Newest at the bottom. Format: context → decision → consequences. Use `/adr`.

## ADR-001 — Rust + windows-rs, no networking ever

- Context: the tool sees every keystroke; trust is the product.
- Decision: Rust, Microsoft's `windows` crate for WinAPI, zero networking code,
  enforced by `deny.toml` bans and the policy test in `crates/retype/tests/policy.rs`.
- Consequences: no update checker; updates are manual downloads/builds.

## ADR-002 — Split into `retype-core` (pure) and `retype` (Windows)

- Context: development and CI may run on Linux; WinAPI code is hard to test.
- Decision: all logic in a `#![forbid(unsafe_code)]` core crate with unit tests;
  the Windows crate is thin glue, `unsafe` confined to `src/win/`.
- Consequences: interfaces between crates are plain data (events in, plans out).

## ADR-003 — Record key codes, not characters

- Context: converting the last word needs the text in another layout.
- Decision: store scan code + vk + modifiers; conversion = backspaces, switch
  layout, replay the same scan codes.
- Consequences: no per-language mapping tables are needed for word/line
  conversion; tables (via `ToUnicodeEx`) are needed only for selections.

## ADR-004 — Hook thread only enqueues

- Context: `LowLevelHooksTimeout` removes slow hooks silently.
- Decision: hook callback filters and pushes to a bounded channel; a worker
  thread does everything else.

## ADR-005 — Minimal dependency set

- Decision: `windows`, `log`, a small file logger, `serde`, `toml`, `anyhow`
  (app only). Anything else needs explicit approval and an ADR.
