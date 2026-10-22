# retype — specification

Status: draft v0.1 (before implementation). Owner decides changes; record
non-trivial ones in `DECISIONS.md`.

## 1. Problem

The user types in the wrong keyboard layout (`ghbdtn` instead of `привет`, or
`руддщ` instead of `hello`) and wants to fix it with one key instead of
retyping. Existing tools are either closed-source with telemetry and bundled
services (Punto Switcher), abandoned, or carry features (auto-update, cloud
translate, sync) the user does not want near a keyboard hook.

## 2. Goals

- G1. Convert the **last typed word** to the other layout with one hotkey.
- G2. Convert the **last typed line** (since Enter / focus change) with a hotkey.
- G3. Convert the **selected text** with a hotkey.
- G4. Optionally switch the system layout to the target layout after converting.
- G5. Work with any set of installed layouts; the user configures the pair (or
      ordered list) to switch between, e.g. `en-US` ⇄ `ru-RU`, or a cycle
      `en-US` → `uk-UA` → `ru-RU`.
- G6. Be auditable: small codebase, minimal dependencies, reproducible build.

## 3. Non-goals (v1)

- Automatic detection/correction while typing (autoswitch). Planned later,
  behind a config flag, off by default.
- Spell checking, translation, snippets, clipboard history, "diary".
- Any network feature, including update checks.
- Linux/macOS support (core stays portable so it remains possible).
- Working inside elevated (admin) windows when retype runs unelevated — this is
  a Windows UIPI restriction; document it, do not try to bypass it.

## 4. Privacy and security requirements (hard)

| ID  | Requirement                                                                 |
|-----|-----------------------------------------------------------------------------|
| P1  | The binary contains no networking code and links no networking crates.     |
| P2  | Typed text exists only in memory, in a bounded buffer (≤ 1 line, ≤ 512 keys). |
| P3  | The buffer is cleared on Enter, focus change, mouse click, Ctrl/Alt/Win combos, idle timeout. |
| P4  | Key content is never logged at any level in release builds; logs contain only event kinds and counts. |
| P5  | No auto-update, no telemetry, no crash upload. Logs stay local.            |
| P6  | Clipboard used for selection conversion is restored afterwards.            |
| P7  | No code injection into other processes (no global DLL hooks).              |

## 5. Functional behaviour

### 5.1 Default hotkeys (configurable)

| Action                   | Default        |
|--------------------------|----------------|
| Convert last word        | `Pause`        |
| Convert last line        | `Shift+Pause`  |
| Convert selection        | `Scroll Lock`  |
| Pause/resume retype      | `Ctrl+Pause`   |

### 5.2 Convert last word / line

1. Keystrokes are recorded as key codes plus modifier state (not as characters).
2. On hotkey: send N × Backspace (N = number of recorded printable keys),
   switch the foreground window's layout to the target, replay the same keys.
3. Trailing spaces typed after the word are preserved (configurable).
4. Pressing the hotkey again converts back (toggle).

### 5.3 Convert selection

1. Save clipboard → send copy → read text.
2. Map every character from the source layout to the target layout via layout
   tables built at startup from the installed layouts.
3. Paste result, restore the original clipboard.
4. Characters absent from both tables are kept as is.

### 5.4 Tray

Tray icon with: current status (active/paused), Pause, Open config, Reload
config, Open log folder, Start with Windows (checkbox), Exit. No other UI in v1.

### 5.5 Configuration

TOML file, see `config.example.toml`. Location: next to the exe if
`portable` file exists there, otherwise `%APPDATA%\retype\config.toml`.
Reloaded on tray command or file change; a broken file is reported in the
log and the running config stays.

## 6. Non-functional requirements

| Area        | Target                                                        |
|-------------|---------------------------------------------------------------|
| Hook latency| Hook callback returns in < 1 ms; no blocking work in it       |
| Memory      | < 10 MB working set                                           |
| Binary      | < 2 MB release `.exe`, no runtime dependencies                |
| OS          | Windows 10 1809+ and Windows 11, x64                          |
| Startup     | Optional autostart via HKCU Run key (opt-in, off by default)  |

## 7. Compatibility targets (manual test matrix)

Notepad, Windows Terminal / PowerShell, VS Code, Chrome/Edge/Firefox text
fields, Telegram Desktop, MS Word, an elevated window (expected: not working,
no crash), games/fullscreen apps (expected: retype ignores them).

## 8. Resolved questions

| ID | Question                                  | Decision                                                                 |
|----|-------------------------------------------|--------------------------------------------------------------------------|
| Q1 | Default target for three layouts          | `pair` by default (current ⇄ first other in list); `cycle` via config. ADR-006 |
| Q2 | Ship autostart in v1?                     | Yes, opt-in, off by default (HKCU Run, M5). ADR-007                      |
| Q3 | Code signing                              | Unsigned in v1; publish SHA-256 and build-from-source docs. Revisit before 1.0 if Defender/SmartScreen flags builds. ADR-008 |
