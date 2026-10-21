@AGENTS.md

## Claude Code specifics

- Topic rules live in `.claude/rules/` (some load only for matching paths).
- Workflows are skills in `.claude/skills/`:
  - `/milestone` — implement the next ROADMAP item end to end
  - `/verify` — fmt + clippy + tests + policy checks; run before saying "done"
  - `/winapi-ffi` — how to add a WinAPI call safely
  - `/security-audit` — privacy/network/unsafe audit (run before releases)
  - `/manual-test` — produce and log a manual Windows test checklist
  - `/adr` — record a design decision
  - `/release` — build and package a release (user-invoked only)
- Use the `reviewer` subagent to review a finished diff before committing.
- You usually cannot see the Windows desktop: when behaviour can only be
  checked by hand, say so and produce a checklist via `/manual-test` instead of
  claiming it works.
