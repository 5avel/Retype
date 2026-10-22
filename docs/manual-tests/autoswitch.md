# Manual test — autoswitch

Build: `cargo build --release -p retype`, run `target\release\retype.exe`.
Log: `%LOCALAPPDATA%\retype\retype.log`. Append results to
`docs/manual-test-log.md`.

Setup: `layouts = ["en-US", "ru-RU", "uk-UA"]`, `[autoswitch] enabled = true`
(saved config is picked up within ~2 s). Notepad first, then a browser
field, Telegram, VS Code.

| #  | Do (start in the layout named)                           | Expected                                                   |
|----|----------------------------------------------------------|------------------------------------------------------------|
| 1  | EN: type `ghbdtn ` (with the space)                      | `привет `, layout now RU; log `autoswitched a word, 7 keys`|
| 2  | Keep typing `vbh ` in RU                                 | `мир ` (no correction needed)                              |
| 3  | RU: type `руддщ цщкдв `                                  | `world` fixed; `hello` may stay (conservative)             |
| 4  | EN: type `ghbdsn `                                       | `привіт `, layout UK                                       |
| 5  | EN: type `hello git npm https kubectl `                  | Nothing changes                                            |
| 6  | RU: type `привет как дела `                              | Nothing changes                                            |
| 7  | EN: type `ghbdtn `, then the convert-word hotkey at once | Back to `ghbdtn `, layout EN                               |
| 8  | After 7, click elsewhere, type `ghbdtn ` again           | Not corrected (remembered until exit)                      |
| 9  | Exit and restart retype, type `ghbdtn `                  | Corrected again (memory is not saved)                      |
| 10 | Type a 2-letter wrong word alone (`yt ` = «не»)          | Not corrected (too short on its own)                       |
| 10a| RU: type `рш ьн тфьу ` («hi my name»)                    | `hi my name `; convert hotkey at once restores all three   |
| 11 | Pause, type `ghbdtn `                                    | Not corrected                                              |
| 12 | In an excluded app, type `ghbdtn `                       | Not corrected                                              |
| 13 | `enabled = false`, save, type `ghbdtn `                  | Not corrected                                              |
| 14 | Type fast without pausing: `ghbdtn rfr ltkf ltkf `       | `привет как дела дела `, nothing lost or reordered         |
| 14a| Type fast in EN with no wrong words: `the cat sat on `   | Text exactly as typed, no visible lag                      |
| 15 | Search the log for any typed word                        | Not found (P4)                                             |
