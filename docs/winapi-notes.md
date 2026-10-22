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

- Ctrl+Pause arrives as `VK_CANCEL` (0x03), not `VK_PAUSE`. `retype-core`
  (`Hotkey::matches`) maps it back, so pass the raw vk code.

- Focus changes come from `SetWinEventHook(EVENT_SYSTEM_FOREGROUND /
  EVENT_OBJECT_FOCUS, WINEVENT_OUTOFCONTEXT)`: out-of-context means no DLL
  injection (P7), but events are delivered through the installing thread's
  message loop, so it lives on the hook thread.

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
  `LCIDToLocaleName` turns it into a tag (`0x0419` → `ru-RU`). Variants of one
  language (US / Dvorak) share the tag; the high word tells them apart.
- `WM_INPUTLANGCHANGEREQUEST` is a posted message, and posted messages are
  retrieved before queued input. Switching back right after `SendInput` can
  make the app translate the replayed keys in the old layout — wait first.

## Character tables

- Build char ⇄ (vk, shift) with `ToUnicodeEx` for each HKL at startup.
- `ToUnicodeEx` mutates kernel keyboard state for dead keys; pass flag `0x4`
  (do not change keyboard state, Windows 10 1607+) or dead keys break for the user.

## SendInput

- Prefer `KEYEVENTF_SCANCODE` for replay so the target layout decides the
  character; use `KEYEVENTF_UNICODE` only for pasting text that has no key.
- Release modifiers the user is still holding (the hotkey's Shift/Ctrl)
  before replay, restore after — or the replay is modified.
- Releasing a lone Alt opens the window menu, a lone Win opens Start. Tap an
  unassigned vk (`0xE8`) first ("mask key", as AutoHotkey does).
- A held hotkey auto-repeats key-downs; LL hooks have no repeat flag, so
  repeats are told apart via key-ups.
- Sticky Keys counts five Shift presses in a row; keys we inject in between
  do not seem to reset the count, so repeated double-Shift conversions can
  bring up the Sticky Keys prompt. Documented, not worked around (owner's
  decision; changing the user's accessibility settings is not ours to do).
- Editing text the user is still typing (autoswitch) races with their next
  keys: hold the physical keys in the LL hook (swallow + remember), run the
  edit, then replay the held keys with `SendInput`.
- The LL hook never sees key-ups that happen on the secure desktop
  (Ctrl+Alt+Del, UAC, Win+L); self-tracked key state must be reconciled.
- `SendInput` blocked by UIPI reports neither an error nor a short count.
- UIPI: input to windows of a higher integrity level (admin) is blocked.

## Clipboard

- `OpenClipboard` fails if another app holds it — retry a few times with small
  sleeps.
- Save all formats you can restore, at least `CF_UNICODETEXT`; restoring rich
  formats is best-effort.
- Wait for the copy to land (sequence number via `GetClipboardSequenceNumber`)
  instead of fixed sleeps.
- `OpenClipboard(NULL)` + `EmptyClipboard` makes `SetClipboardData` fail; the
  owner must be a window, and one with a message loop: other apps *send* it
  `WM_DESTROYCLIPBOARD`, so a non-pumping owner thread would hang them. A
  window owned by another thread is no better: that thread may be blocked
  (e.g. the main thread in `join` at shutdown).
- Reading delay-rendered formats makes the source app render them while we
  hold the clipboard; cap the snapshot.
- Ctrl+C with nothing selected interrupts the program in a terminal; use
  Ctrl+Insert / Shift+Insert for copy/paste.
- There is no signal for "the app has read the clipboard after Ctrl+V";
  restore after a delay (400 ms), or the app pastes the original.
- Keep our writes out of Win+V history and cloud sync with the registered
  formats `ExcludeClipboardContentFromMonitorProcessing`,
  `CanIncludeInClipboardHistory` (DWORD 0), `CanUploadToCloudClipboard` (DWORD 0).
- windows-rs quirks: `GlobalFree` success returns NULL and is reported as
  `Err`; `GlobalUnlock` reaching lock count 0 is reported as `Err` with code 0.

## Tray

- Re-add the icon when the registered `TaskbarCreated` message arrives
  (Explorer restarted); a message-only window would not receive it, so the
  tray owns a normal hidden window.
- Before `TrackPopupMenu` call `SetForegroundWindow(hwnd)`, after it post
  `WM_NULL`, or the menu does not close on outside clicks.
- `NIM_DELETE` on `WM_DESTROY` and `WM_ENDSESSION`, otherwise a ghost icon
  stays until the mouse passes over it.

## Icons

- GDI text drawn into a 32-bit DIB leaves the alpha byte at 0 where it
  draws; set alpha yourself after drawing (we apply a rounded-square mask).
  32-bit icon colors are straight (not premultiplied) alpha.
- `CreateIconIndirect` copies the bitmaps: delete ours afterwards, and keep
  the `HICON` alive while the tray shows it (destroy only after replacing).
- `MAKEINTRESOURCE(id)` in windows-rs: `PCWSTR(std::ptr::without_provenance(id))`.
- The MSVC linker accepts a compiled `.res` file as a plain input
  (`cargo:rustc-link-arg-bins=...`); a `.res` starts with an empty 32-byte
  resource header, and icons are RT_ICON images plus one RT_GROUP_ICON.

## Antivirus heuristics

Keyboard hook + `SendInput` resembles a keylogger. Unsigned builds may be
flagged by Defender/SmartScreen. Keep imports minimal, no networking imports,
consider signing for releases.
