---
name: milestone
description: Implement the next unchecked ROADMAP.md item (or the one named in the arguments) end to end — plan, core tests first, Windows glue, verify, update docs. Use when the user says "next step", "continue", or names a roadmap item.
argument-hint: "[roadmap item text or milestone, optional]"
---

# Implement a roadmap item

Target: $ARGUMENTS (if empty, the first unchecked item in `ROADMAP.md`).

## 1. Orient

- Read `ROADMAP.md`, the relevant parts of `docs/SPEC.md` and
  `docs/ARCHITECTURE.md`, and `docs/DECISIONS.md`.
- Read the existing code you will touch. Do not re-create what exists.
- If the item is ambiguous or conflicts with SPEC/DECISIONS, ask the user one
  focused question before writing code.

## 2. Plan (short)

Tell the user in 3–6 bullet points (in Russian): what changes, which crate,
new public types/functions, how it will be tested. Proceed unless the plan
contains a decision that is clearly the user's (new dependency, behaviour that
differs from SPEC, anything touching privacy rules).

## 3. Core first

- Put every decision that does not need the OS into `retype-core`.
- Write tests first, then the code, until `cargo test -p retype-core` passes.

## 4. Windows glue

- Only after the core part is green. Follow `.claude/rules/unsafe-winapi.md`
  and use `/winapi-ffi` for new API calls.
- Keep it thin: translate events in, execute plans out.

## 5. Finish

1. Run `/verify` and fix until green.
2. Tick the item in `ROADMAP.md`. Add an ADR via `/adr` if a design choice was made.
3. If the item has Windows-visible behaviour, run `/manual-test` to give the
   user a checklist — do not claim it works on Windows without their result.
4. Suggest a commit message (conventional style: `feat(core): …`). Commit only
   if the user asked you to.
