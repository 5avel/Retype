# Changelog

## 1.0.0 — 2026-10-23

First release.

### Added

- Convert the last word (`Pause`, or double right Shift) or the last line
  (`Shift+Pause`) typed in the wrong layout; press again to convert back.
- Convert the selected text (`Scroll Lock`) via the clipboard, which is
  restored afterwards; retype's own clipboard writes stay out of the
  Windows clipboard history and cloud sync.
- Optional autoswitch (off by default): words typed in the wrong layout are
  fixed after Space using built-in letter statistics for English, Russian
  and Ukrainian; short words before a corrected word are fixed with it; the
  convert hotkey right after a correction undoes it.
- Tray icon showing the current layout (`EN`/`RU`/… on a colored square,
  grey while paused), with Pause, Open config, Reload config, Open log
  folder, Start with Windows and Exit.
- Any set of installed layouts, `pair` or `cycle` mode; excluded apps;
  opt-in start with Windows; config reloaded when the file changes; a hint
  in the log for wrong layout tags such as `ua-UA`.
- App icon, single small `.exe`, portable mode.

### Privacy

- No networking code or crates (enforced by `deny.toml` and a policy test).
- Typed keys only in a bounded in-memory buffer, wiped when dropped; never
  logged or written to disk; no code injected into other processes.
