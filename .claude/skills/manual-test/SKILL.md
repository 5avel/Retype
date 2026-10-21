---
name: manual-test
description: Produce a manual Windows test checklist for a feature (apps × scenarios from SPEC §7) and record the user's results in docs/manual-test-log.md. Use after implementing anything with Windows-visible behaviour, or when the user reports test results.
argument-hint: "[feature, e.g. 'convert last word']"
---

# Manual Windows test

Feature under test: $ARGUMENTS (if empty, the last completed ROADMAP item).

## Mode A — no results given yet: produce the checklist

1. Tell the user how to build/run: `cargo build --release -p retype`, run
   `target\release\retype.exe`, where the log file is.
2. Produce a compact checklist (in Russian) for the feature:
   - scenarios: basic case, uppercase/Shift, digits and punctuation, word with
     trailing space, second press toggles back, empty buffer, after Backspace,
     after mouse click (buffer must reset), three layouts if configured;
   - apps from SPEC §7: Notepad, Windows Terminal, VS Code, a browser text
     field, Telegram Desktop, Word, an elevated window (expected: no effect, no crash).
3. Format each line as `- [ ] app — scenario — expected result`.
4. Ask the user to reply with failures only (passes are assumed).

## Mode B — the user reported results

1. Append rows to `docs/manual-test-log.md`:
   `| YYYY-MM-DD | <git short sha or "local"> | scenario | app | ok/FAIL | notes |`
2. For each failure, propose a likely cause (check `docs/winapi-notes.md`) and
   either fix it (then `/verify`) or add an item to `ROADMAP.md`.
