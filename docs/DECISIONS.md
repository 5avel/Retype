# Decision log (ADR-lite)

Newest at the bottom. Format: context → decision → consequences. Use `/adr`.

## ADR-001 — Rust + windows-rs, no networking ever

- Context: the tool sees every keystroke; trust is the product.
- Decision: Rust, Microsoft's `windows` crate for WinAPI, zero networking code,
  enforced by `deny.toml` bans and the policy test in `crates/retype/tests/policy.rs`.
- Consequences: no update checker; updates are manual downloads/builds.

## ADR-002 — Split into `retype-core` (pure) and `retype` (Windows)

- Context: development and CI may run on Linux; WinAPI code is hard to test.
- Decision: all logic in a `#![forbid(unsafe_code)]` core crate with unit tests;
  the Windows crate is thin glue, `unsafe` confined to `src/win/`.
- Consequences: interfaces between crates are plain data (events in, plans out).

## ADR-003 — Record key codes, not characters

- Context: converting the last word needs the text in another layout.
- Decision: store scan code + vk + modifiers; conversion = backspaces, switch
  layout, replay the same scan codes.
- Consequences: no per-language mapping tables are needed for word/line
  conversion; tables (via `ToUnicodeEx`) are needed only for selections.

## ADR-004 — Hook thread only enqueues

- Context: `LowLevelHooksTimeout` removes slow hooks silently.
- Decision: hook callback filters and pushes to a bounded channel; a worker
  thread does everything else.

## ADR-005 — Minimal dependency set

- Decision: `windows`, `log`, a small file logger, `serde`, `toml`, `anyhow`
  (app only). Anything else needs explicit approval and an ADR.

## ADR-006 — Default conversion mode is `pair`

- Context: with three or more layouts the target is ambiguous (SPEC Q1).
- Decision: default `mode = "pair"` converts between the current layout and the
  first other one in `layouts`; `mode = "cycle"` is available via config.
- Consequences: two-layout users need no setup; matches `config.example.toml`.

## ADR-007 — Autostart in v1, opt-in

- Context: SPEC Q2.
- Decision: ship autostart via the HKCU Run key in v1 (M5), off by default.
- Consequences: no installer needed; the user enables it from config/tray.

## ADR-008 — No code signing for v1

- Context: SPEC Q3; unsigned hook + SendInput binaries may trigger
  Defender/SmartScreen heuristics.
- Decision: v1 ships unsigned with published SHA-256 hashes and
  build-from-source instructions.
- Consequences: revisit signing before 1.0 if builds get flagged.

## ADR-009 — Core behaviour details settled in M1

- Context: implementing `retype-core` needed choices the SPEC leaves open.
- Decision:
  - `serde` + `toml` (both allowed by ADR-005) are dependencies of
    `retype-core`, so config parsing is unit-tested on any OS. Unknown config
    keys are errors.
  - Only Space separates words: punctuation keys are letters in other layouts
    (`,` = `б`, `;` = `ж`).
  - `keep_trailing_spaces = false` drops the spaces after the word when
    converting it.
  - On buffer overflow (512 keys) the oldest key is dropped.
  - A hotkey on a key that types text needs Ctrl, Alt or Win; two actions may
    not share a hotkey.
  - Toggle works by re-planning from the new current layout: the session
    keeps the replayed keys in the buffer after a conversion.
  - Layout tables are keyed by (scan code, Shift); `LayoutId` is a
    lowercased language tag.
- Consequences: the app passes the foreground layout to `Session::plan` and
  supplies `LayoutTable`s for selection conversion.

## ADR-010 — Executing word/line conversions (M3)

- Context: running a `ConversionPlan` on Windows needed choices about held
  modifiers, auto-repeat, asynchronous layout switches and layout matching.
- Decision:
  - `retype-core::inject` turns a plan plus the held sided modifiers into
    `Step`s (`Keys` / `SwitchLayout` / `Settle`); the executor only calls the
    OS. Keys are injected by scan code, one `SendInput` per `Keys` step.
  - Held modifiers are released before and pressed again after, but only those
    still physically held (the hook tracks them from non-injected events;
    `GetAsyncKeyState` also sees our injection). One released mid-conversion
    would otherwise stay stuck. If Alt or Win is involved, an unassigned key
    (vk `0xE8`) is tapped before the release and after the restore so the
    Start/window menu does not open.
  - The executor stops at the first short `SendInput` (a lost Backspace must
    not double the word) or when the foreground window changes; modifiers are
    restored and the session buffer is cleared.
  - The hook also forwards key-ups (`InputEvent::KeyUp`, vk only); the session
    ignores a hotkey's auto-repeat until its key-up or another non-modifier key.
  - A layout switch is confirmed by polling the foreground thread's layout
    (300 ms). If it does not confirm, the plan continues: replaying the same
    keys in the unchanged layout restores the erased text, so nothing is lost.
  - Before switching back (`switch_layout_after_convert = false`) the executor
    waits 100 ms (`Settle`): posted messages overtake queued input, so an early
    switch would make the app translate the replay in the old layout.
  - Configured layouts match installed ones by the language of the HKL
    (`LCIDToLocaleName`); the first installed variant of a language is used.
  - If a target layout is not installed, nothing is injected and the buffer
    is cleared.
- Consequences: new `windows` feature `Win32_Globalization` (no new crate).
  Two variants of one language (e.g. US and Dvorak) cannot both be configured.

## ADR-011 — Selection conversion via the clipboard (M4)

- Context: SPEC 5.3 converts selected text by copy → convert → paste. The
  copy is done by the target app, and the clipboard may hold anything,
  including passwords.
- Decision:
  - Flow: snapshot every `HGLOBAL` clipboard format (at most 64 formats,
    32 MiB) → Ctrl+Insert → wait for the clipboard sequence number to change
    (500 ms; no change = nothing selected) → read `CF_UNICODETEXT` → convert
    in the core → write → Shift+Insert → wait 400 ms → restore the snapshot.
    Whether to restore is decided by the sequence number on every path (also
    after a failed injection, and 300 ms more for a late copy), not by which
    step succeeded. GDI-handle formats (bitmap, metafile, palette) are not
    saved; Windows synthesises most from the DIB/text formats that are.
  - Copy/paste use Ctrl+Insert / Shift+Insert, by scan code: with nothing
    selected, Ctrl+C in a terminal interrupts the running program, while
    Ctrl+Insert is harmless; navigation keys are the same in every layout.
  - Everything retype writes (the converted text and the restore) carries
    `ExcludeClipboardContentFromMonitorProcessing`, `CanIncludeInClipboardHistory = 0`
    and `CanUploadToCloudClipboard = 0`. The app's own copy of the selection
    does land in the Win+V history (and cloud clipboard, if the user enabled
    sync); this is inherent to SPEC 5.3 and accepted by the owner.
  - The source layout is detected from the text: the configured layout whose
    distinctive characters it uses most (ties: the foreground layout). The
    target follows `mode` from that source.
  - Selections over 10 000 characters, or that would not change, are left
    alone. Copied and converted text is zeroed after use, the snapshot on drop.
  - A message-only window on the worker thread owns the clipboard; the
    worker pumps its messages at least every 50 ms, because other apps *send*
    it `WM_DESTROYCLIPBOARD`. Its lifetime ends with the worker, so shutdown
    cannot deadlock on, or outlive, the tray window.
  - The hook reads modifiers from its own physical key tracking instead of
    `GetAsyncKeyState`, which also reflects our injected releases. Missed
    key-ups (secure desktop, hook timeout) are reconciled against
    `GetAsyncKeyState` on every worker tick and before each conversion.
- Consequences: new `windows` features `Win32_System_DataExchange` and
  `Win32_System_Memory` (no new crate). Mixed-script selections convert only
  the characters of the detected source layout. Apps that copy the whole line
  when nothing is selected (VS Code) get that line converted and pasted.
