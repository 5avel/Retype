---
paths:
  - "crates/retype/src/win/**"
---

# unsafe and WinAPI code

Before adding a new WinAPI call, use the `/winapi-ffi` skill. Gotchas:
`docs/winapi-notes.md` — read the relevant section first, add to it when you
learn something new.

- `unsafe` is allowed only in `crates/retype/src/win/`. Everywhere else the
  crate-level lints forbid it.
- Every `unsafe` block has a `// SAFETY:` comment stating why the call is sound
  (pointer validity, buffer sizes, thread requirements, handle ownership).
- Keep `unsafe` blocks minimal: one call (plus its argument setup) per block.
- Wrap each API in a small safe function that returns `windows::core::Result`
  or `anyhow::Result`; callers outside the wrapper never touch raw pointers.
- Check every return value / `GetLastError`. Never ignore a `BOOL` result.
- Handles are owned by RAII wrappers that release them in `Drop`
  (hooks → `UnhookWindowsHookEx`, clipboard → `CloseClipboard`, etc.).
- `windows` crate features: add only what you use, keep the feature list in
  `crates/retype/Cargo.toml` sorted. `Win32_Networking*` is forbidden.
- Hook callbacks: no allocation-heavy work, no locks that can block, no
  logging of key content; filter `LLKHF_INJECTED`; always `CallNextHookEx`
  unless swallowing our own hotkey.
- Keep OS-independent decisions out of here: if you write an `if` that decides
  *what* to do (not *how* to call Windows), it probably belongs in `retype-core`.
