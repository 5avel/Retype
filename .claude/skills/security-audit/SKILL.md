---
name: security-audit
description: Audit the codebase against the privacy and security invariants — networking, key-content logging, persistence of typed text, unsafe blocks without SAFETY comments, dependency tree and windows features. Use before a release, after adding a dependency, or when the user asks for an audit.
context: fork
agent: reviewer
allowed-tools: Bash(cargo tree *) Bash(cargo deny *) Bash(cargo test *) Bash(git log *) Bash(git diff *)
---

# Security audit

Audit against `.claude/rules/security.md` and `docs/SPEC.md` §4. Read-only:
report findings, do not fix them in this skill.

## Checks

1. **Dependencies**
   - `cargo tree -e normal --target all` — list every crate. Flag anything not
     in the approved set from ADR-005 and anything network-related
     (http, tls, socket, dns, url-fetching, tokio, async runtimes).
   - `cargo deny check` if available.
2. **Windows features**: read `crates/retype/Cargo.toml`; flag any
   `Win32_Networking*`, `Win32_Security_Cryptography` (unexpected), WinINet/WinHTTP.
3. **Source scan** (Grep over `crates/`):
   - networking: `std::net`, `TcpStream`, `UdpSocket`, `WinHttp`, `Internet`,
     `URLDownload`, `socket`;
   - persistence: `std::fs::write`, `File::create`, `OpenOptions`, registry writes
     (`RegSetValue`) — each must be justified (config, log, autostart only);
   - logging: every `log::` / `info!` / `debug!` / `trace!` call near
     buffer/keystroke code — check that no key content is formatted in;
   - `unsafe` outside `crates/retype/src/win/`, and `unsafe` blocks without a
     preceding `// SAFETY:` line;
   - process injection: `CreateRemoteThread`, `WriteProcessMemory`,
     `SetWindowsHookEx` with anything other than `WH_KEYBOARD_LL`/`WH_MOUSE_LL`.
4. **Buffer bounds**: confirm the max buffer size and every reset rule from
   SPEC P2/P3 exist in `retype-core` and have tests.
5. **Clipboard**: confirm restore happens on every path, including errors.
6. Run `cargo test --workspace` (policy test included).

## Report

Group findings as **Blocker / Should fix / Note**, each with `file:line` and a
one-line reason. End with an overall verdict: ready / not ready for release.
