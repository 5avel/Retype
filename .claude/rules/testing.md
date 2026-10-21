---
paths:
  - "crates/**/*.rs"
---

# Testing

## Where

- Core logic: unit tests in `#[cfg(test)] mod tests` at the bottom of each file
  in `retype-core`. These must pass on Linux and Windows.
- Policy checks: `crates/retype/tests/policy.rs` (scans sources).
- Windows behaviour (hook, SendInput, clipboard, real layouts) is not unit
  tested; it goes through `/manual-test` and `docs/manual-test-log.md`.
  Make the Windows code thin enough that this is acceptable.

## How

- Write the core test first when adding behaviour (red → green).
- Test layout inside `mod tests`: imports, constants, test functions (grouped
  in sub-modules when a category has 2+ tests), helpers at the bottom.
- Use hardcoded layout tables (en-US, ru-RU, uk-UA) as test fixtures in
  `retype-core/src/testing.rs` (behind `#[cfg(test)]`).

## What

- Buffer bounds and every reset rule (Enter, focus change, mouse, idle, Ctrl/Alt/Win).
- Backspace handling, including backspace past the start of the word.
- Conversion plans: word, line, trailing spaces, toggle back, empty buffer.
- Selection text conversion: mixed case, digits, punctuation, unmapped chars.
- Config: defaults, invalid hotkey strings, unknown layouts, error messages.

## Not

- Trivial constructors/getters.
- Tests that need a real Windows desktop inside `cargo test`.
