# Manual test log

Windows behaviour that cannot be unit-tested is checked by hand. `/manual-test`
generates a checklist; results are appended here.

| Date       | Build / commit | Scenario | App | Result | Notes |
|------------|----------------|----------|-----|--------|-------|
| 2026-10-21 | local (M2)     | Start: hooks, worker, tray            | —       | ok     | from retype.log |
| 2026-10-21 | local (M2)     | Convert-word hotkey recognised        | —       | ok     | Ctrl+Shift+Z, 6 keys buffered |
| 2026-10-21 | local (M2)     | Pause/resume hotkey                   | —       | ok     | Ctrl+Shift+P |
| 2026-10-21 | local (M2)     | Open config creates file from example | —       | ok     | |
| 2026-10-21 | local (M2)     | Exit: hooks removed, worker stopped   | —       | ok     | |
| 2026-10-21 | local (M2)     | Log has no key content                | —       | ok     | checked by Claude |
| 2026-10-22 | local (M3)     | Convert last word / line (M3.md)      | —       | ok     | user: all works; build before review fixes (modifier restore) |
| 2026-10-22 | local (M3)     | Convert selection hotkey              | —       | n/a    | not implemented until M4 (logs only) |
| 2026-10-22 | 5cd41bb        | Convert selection (M4.md)             | —       | ok     | user: works |
| 2026-10-22 | 8defa3d        | Exclusions, autostart, reload (M5.md) | —       | ok     | user: partially checked, all ok |
| 2026-10-22 | local (M5.5)   | App icon in Explorer (M5.5.md #18)    | —       | ok     | user screenshot |
