---
name: adr
description: Record an architecture or product decision in docs/DECISIONS.md (context, decision, consequences). Use when a design choice was made, a dependency added, or SPEC behaviour changed.
argument-hint: "[short title]"
---

# Record a decision

1. Read `docs/DECISIONS.md`, find the highest `ADR-NNN`.
2. Append:

   ```markdown
   ## ADR-NNN — $ARGUMENTS

   - Context: why a decision was needed (1–3 lines).
   - Decision: what we chose; alternatives considered in one line.
   - Consequences: what this makes easier/harder; follow-ups.
   ```

3. If the decision changes requirements, update `docs/SPEC.md` in the same
   change and mention the ADR number there.
4. Decisions that are the user's (new dependency, privacy trade-off, dropping a
   goal) must be confirmed by the user before writing them as accepted.
