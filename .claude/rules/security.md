# Security and privacy invariants

These protect the user from their own keylogger. Treat a violation as a bug
that blocks the task, even if the user did not mention it.

## Never

- Add networking: no `std::net`, sockets, HTTP clients, `WinHttp*`,
  `Internet*` (WinINet), `URLDownloadToFile`, `Win32_Networking*` features,
  named pipes/RPC to other machines, update checkers, telemetry, crash upload.
- Log key content: no characters, scan codes, vk codes or buffer text in log
  messages at any level. Log event kinds and counts only
  (`"converted last word, 7 keys"` is fine; `"converted 'ghbdtn'"` is not).
- Write typed text anywhere: files, registry, clipboard history, temp files.
- Inject code into other processes (global DLL hooks, `CreateRemoteThread`).
- Keep the clipboard modified: always restore what was there.
- Add a dependency without the user's explicit approval and an ADR.

## Always

- Keep `KeyBuffer` bounded and cleared per SPEC §4 (P2, P3).
- Zero sensitive buffers when they are dropped/reset if cheap to do so.
- Respect `exclusions.processes` before recording or acting.
- If a feature seems to need any of the "never" items, stop and ask the user.

## Enforcement

- `deny.toml` bans network crates (`cargo deny check bans`).
- `crates/retype/tests/policy.rs` fails on forbidden APIs in source.
- `/security-audit` before every release.
