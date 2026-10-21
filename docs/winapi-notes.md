# WinAPI notes and gotchas

Living document: add an entry whenever Windows behaviour surprises us.

## Low-level keyboard hook

- `SetWindowsHookExW(WH_KEYBOARD_LL, …)` needs a message loop
  (`GetMessageW`) on the installing thread, otherwise the callback never runs.
- The callback must return fast. If it exceeds `LowLevelHooksTimeout`
  (registry, ~300 ms by default; on Windows 7+ the hook can be removed silently
  after repeated timeouts) the hook stops receiving events. Only enqueue.
- Always call `CallNextHookEx` unless deliberately swallowing the key
  (we swallow only our own hotkeys).
- Events produced by our own `SendInput` carry `LLKHF_INJECTED` in
  `KBDLLHOOKSTRUCT.flags` — ignore them, or the buffer records our replay.
  Other software (AutoHotkey, remote desktop) also injects; decide per case.
- `KBDLLHOOKSTRUCT.vkCode` is the virtual key *after* the layout's VK mapping;
  the scan code is layout-independent and is what we replay.

## Layouts

- Layout is per thread. Read the foreground window's layout with
  `GetKeyboardLayout(GetWindowThreadProcessId(GetForegroundWindow(), null))`.
- Switch another app's layout with
  `PostMessageW(hwnd, WM_INPUTLANGCHANGEREQUEST, 0, hkl)`;
  `ActivateKeyboardLayout` only affects our own thread.
- The switch is asynchronous: poll the foreground layout (short timeout) before
  replaying keys.
- Console windows (conhost / Windows Terminal) report layouts unreliably;
  treat as best-effort and test manually.
- `GetKeyboardLayoutList` gives installed HKLs; low word = language id.

## Character tables

- Build char ⇄ (vk, shift) with `ToUnicodeEx` for each HKL at startup.
- `ToUnicodeEx` mutates kernel keyboard state for dead keys; pass flag `0x4`
  (do not change keyboard state, Windows 10 1607+) or dead keys break for the user.

## SendInput

- Prefer `KEYEVENTF_SCANCODE` for replay so the target layout decides the
  character; use `KEYEVENTF_UNICODE` only for pasting text that has no key.
- Release modifiers the user is still holding (the hotkey's Shift/Ctrl)
  before replay, restore after — or the replay is modified.
- UIPI: input to windows of a higher integrity level (admin) is blocked.

## Clipboard

- `OpenClipboard` fails if another app holds it — retry a few times with small
  sleeps.
- Save all formats you can restore, at least `CF_UNICODETEXT`; restoring rich
  formats is best-effort.
- Wait for the copy to land (sequence number via `GetClipboardSequenceNumber`)
  instead of fixed sleeps.

## Antivirus heuristics

Keyboard hook + `SendInput` resembles a keylogger. Unsigned builds may be
flagged by Defender/SmartScreen. Keep imports minimal, no networking imports,
consider signing for releases.
