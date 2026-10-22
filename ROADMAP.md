# Roadmap

Work top to bottom. Each item should end with green `/verify` and, for
Windows behaviour, an entry in `docs/manual-test-log.md`.

## M0 — Scaffold

- [x] Workspace, lints, rustfmt, cargo-deny, CI
- [x] Agent docs, rules, skills, subagent
- [x] Policy test (no networking APIs/crates)

## M1 — Core logic (`retype-core`, any OS)

- [x] `Keystroke`, `InputEvent` types
- [x] `KeyBuffer`: append, backspace, word/line boundaries, reset rules, bounds (P2, P3)
- [x] `Session`: event → buffer updates, hotkey recognition from config
- [x] `ConversionPlan` for last word / last line, toggle back on second press
- [x] `LayoutTable` + text conversion for selections (tests with hardcoded en/ru/uk tables)
- [x] `Config`: TOML parsing, defaults, validation errors with clear messages

## M2 — Windows skeleton

- [x] Logger to `%LOCALAPPDATA%\retype\retype.log` (no key content)
- [x] Single-instance mutex
- [x] Hook thread + message loop + bounded channel + worker thread
- [x] Tray icon with Pause / Open config / Exit
- [x] Clean shutdown (unhook, remove tray icon)
- [ ] Manual test on Windows (`docs/manual-tests/M2.md`)

## M3 — Convert last word / line

- [x] Enumerate layouts, foreground layout, switch layout with confirmation
- [x] SendInput executor for `ConversionPlan` (modifier release/restore)
- [x] Manual test matrix from SPEC §7

## M4 — Convert selection

- [x] Clipboard save/restore with retries and sequence-number wait
- [x] Build `LayoutTable` via `ToUnicodeEx` (no state change flag)
- [x] Manual test matrix

## M5 — Polish

- [x] Excluded processes list
- [x] Opt-in autostart (HKCU Run)
- [x] Config reload (tray command and file change)
- [ ] Layout indicator in the tray (optional)
- [ ] Manual test (`docs/manual-tests/M5.md`)

## M6 — Release 1.0

- [ ] `/security-audit` clean
- [ ] Release build in CI with SHA-256, README with build-from-source

## Later

- [ ] Autoswitch (n-gram detection), off by default, behind config flag
