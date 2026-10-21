---
name: winapi-ffi
description: How to add or change a WinAPI call in crates/retype/src/win with the windows crate — finding the function and its cargo feature, writing a safe wrapper with SAFETY comments, error handling, RAII for handles. Use whenever new Win32 functions, structs or constants are needed.
paths:
  - "crates/retype/src/win/**"
  - "crates/retype/Cargo.toml"
---

# Adding a WinAPI call

## 1. Check the notes

Read the matching section of `docs/winapi-notes.md`. Many traps (hook timeout,
injected events, async layout switch, dead keys in `ToUnicodeEx`, clipboard
locking, UIPI) are already documented there.

## 2. Find the function and feature

- The `windows` crate groups APIs by namespace, e.g.
  `windows::Win32::UI::Input::KeyboardAndMouse::SendInput` →
  feature `Win32_UI_Input_KeyboardAndMouse`.
- Feature names mirror the module path (`Win32_UI_WindowsAndMessaging`,
  `Win32_System_DataExchange`, `Win32_UI_Shell`, `Win32_Foundation`, …).
- Add the feature to `crates/retype/Cargo.toml`, keep the list sorted.
  `Win32_Networking*` features are forbidden (policy test will fail).
- If unsure of the exact signature, compile and read the error — the crate's
  signatures differ from raw C (`Option<HWND>`, `PCWSTR`, slices for arrays,
  `windows::core::Result` returns).

## 3. Write the wrapper

Template:

```rust
/// Returns the keyboard layout of the thread that owns the foreground window.
pub fn foreground_layout() -> Option<Hkl> {
    // SAFETY: GetForegroundWindow has no preconditions and may return null.
    let hwnd = unsafe { GetForegroundWindow() };
    if hwnd.is_invalid() {
        return None;
    }
    // SAFETY: hwnd was just obtained; a stale handle only yields thread id 0.
    let thread_id = unsafe { GetWindowThreadProcessId(hwnd, None) };
    // SAFETY: any thread id is accepted; 0 means the current thread.
    let hkl = unsafe { GetKeyboardLayout(thread_id) };
    Some(Hkl::from_raw(hkl))
}
```

Rules:

- One `unsafe` call per block, each with `// SAFETY:`.
- Return `windows::core::Result<T>` / `anyhow::Result<T>`; convert `BOOL`
  failures with `.ok()` and `GetLastError` context.
- Wrap OS values in small newtypes when they cross into core-facing code
  (`Hkl`, `ScanCode`) — the core crate never sees `windows` types.
- Handles that need releasing get an RAII guard with `Drop`.
- Buffers passed to the OS: size them from the API contract, never guess.

## 4. Check it compiles for Windows

On Windows: `cargo clippy -p retype -- -D warnings`.
Elsewhere: `cargo clippy -p retype --target x86_64-pc-windows-gnu -- -D warnings`
(requires `rustup target add x86_64-pc-windows-gnu`; no linker needed for check/clippy).

## 5. Record

If you hit surprising behaviour, add it to `docs/winapi-notes.md`.
