# retype

Fix text typed in the wrong keyboard layout with one key — `ghbdtn` → `привет`.
Open source, offline, no telemetry, no auto-update, typed text never leaves memory.

Status: pre-alpha (scaffold). See `ROADMAP.md`.

## Build

Requirements: Rust stable (MSVC toolchain on Windows).

```
cargo build --release -p retype
```

The binary is `target/release/retype.exe`.

## Default hotkeys

| Action              | Key           |
|---------------------|---------------|
| Convert last word   | `Pause`       |
| Convert last line   | `Shift+Pause` |
| Convert selection   | `Scroll Lock` |
| Pause / resume      | `Ctrl+Pause`  |

Configuration: see `config.example.toml`.

## Privacy

- No networking code; banned network crates are enforced in CI (`deny.toml`,
  `crates/retype/tests/policy.rs`).
- Keystrokes are kept in a small in-memory buffer (one line) and are never
  logged or written to disk.

## License

MIT OR Apache-2.0
