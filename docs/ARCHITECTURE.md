# Architecture

## Crates

| Crate         | Role                                                         | unsafe | OS        |
|---------------|--------------------------------------------------------------|--------|-----------|
| `retype-core` | Buffers, decisions, conversion plans, layout tables, config  | forbid | any       |
| `retype`      | Windows glue: hook, SendInput, HKL, clipboard, tray, main    | `win/` | Windows   |

Rule of thumb: if a function can be tested without a Windows desktop, it
belongs in `retype-core`.

## Threads and data flow

```
             ┌──────────────────────── hook thread ─────────────────────────┐
 keyboard ─► │ WH_KEYBOARD_LL callback                                      │
 mouse    ─► │  - drop events with LLKHF_INJECTED (our own SendInput)        │
             │  - translate to core::InputEvent                              │
             │  - try_send to channel, CallNextHookEx, return (< 1 ms)       │
             │ GetMessageW loop (required for LL hooks to fire)              │
             └──────────────────────────────┬───────────────────────────────┘
                                            │ mpsc channel (bounded)
             ┌──────────────────────────────▼───────────── worker thread ───┐
             │ core::Session: updates KeyBuffer, detects hotkeys            │
             │ on hotkey → core builds ConversionPlan (pure data)           │
             │ win::executor runs the plan: Backspace×N, switch HKL,        │
             │   wait for layout, SendInput replay / clipboard paste        │
             └──────────────────────────────────────────────────────────────┘
             ┌─────────────── main/UI thread ───────────────────────────────┐
             │ tray icon, config load/reload, single-instance mutex, exit   │
             └──────────────────────────────────────────────────────────────┘
```

Why: Windows silently removes a low-level hook whose callback is slow
(`LowLevelHooksTimeout`). The hook therefore never does work itself.

## Core types (planned)

| Type             | Module            | Purpose                                                |
|------------------|-------------------|--------------------------------------------------------|
| `Keystroke`      | `keystroke`       | vk, scan code, shift, extended flag                    |
| `InputEvent`     | `event`           | Key down/up, mouse click, focus change, timer          |
| `KeyBuffer`      | `buffer`          | Bounded record of the current line, word boundaries    |
| `Session`        | `session`         | Feeds events into the buffer, recognises hotkeys       |
| `ConversionPlan` | `plan`            | Ordered ops: Backspace(n), SwitchLayout(id), Replay(…) |
| `LayoutTable`    | `layout`          | char ⇄ (vk, shift) for one layout, built by the app    |
| `Config`         | `config`          | Parsed TOML with defaults and validation               |

The app supplies `LayoutTable`s (built with `ToUnicodeEx`) and executes plans;
the core never calls the OS, which keeps it deterministic and unit-testable.

## Windows pieces (`crates/retype/src/win/`)

| Module       | Responsibility                                                     |
|--------------|--------------------------------------------------------------------|
| `hook`       | Install/uninstall `WH_KEYBOARD_LL` (+ `WH_MOUSE_LL`), message loop |
| `input`      | `SendInput` helpers (key down/up, backspace, scancode replay)      |
| `layout`     | Enumerate HKLs, foreground layout, switch layout, build tables     |
| `clipboard`  | Save/restore clipboard, copy/paste selection with retries          |
| `tray`       | `Shell_NotifyIconW`, menu, commands to main loop                   |
| `instance`   | Named mutex for single instance                                    |

Gotchas are collected in `docs/winapi-notes.md`.
