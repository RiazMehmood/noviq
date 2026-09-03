---
id: 0004
title: Ingestion hardening analysis
stage: misc
date: 2026-06-19
surface: agent
model: claude-opus-4-8
feature: 013-ingestion-hardening
branch: 013-ingestion-hardening
user: Riaz Mehmood
command: /sp.analyze
labels: [ingestion, analysis, consistency, read-only]
links:
  spec: specs/013-ingestion-hardening/spec.md
  ticket:
  adr:
  pr:
files:
  - specs/013-ingestion-hardening/spec.md
  - specs/013-ingestion-hardening/plan.md
  - specs/013-ingestion-hardening/tasks.md
tests: []
---

## Prompt

/sp.analyze (no additional user input)

Read-only cross-artifact consistency + quality analysis across spec.md, plan.md, tasks.md (with
research/data-model/contracts) and the constitution. Detect duplication, ambiguity, underspecification,
constitution conflicts, coverage gaps, inconsistency. Produce a compact report; do not modify files.

## Response snapshot

Ran the analysis (artifacts already in session context). Findings: **9** (1 CRITICAL, 4 MEDIUM, 4 LOW),
0 duplications, 0 placeholders.

- **C1 (CRITICAL)**: constitution §IV (SQLite) violated by PostgreSQL stack — pre-existing/inherited
  from F011/F012, tracked in plan Complexity, pending amendment; not introduced here, non-blocking.
- **G1 (MED)**: FR-013 (no-LLM) has no verification task.
- **G2 (MED)**: no test that a legitimate in-body bare integer is NOT stripped (zoning inverse).
- **G3 (MED)**: running-header exclusion (SC-003) untested (relies on existing chrome repetition).
- **U1 (MED)**: catchline-title storage field unpinned in T022/data-model.
- **I1/I2/T1/A1 (LOW)**: Phase 3 mixed-priority label; hybrid-doc edge vs Non-Goals; citation-term
  drift; "user override" layer is a no-op this iteration.

Coverage: 14/15 FRs have ≥1 task (93%; only FR-013 cross-cutting). All SC-001…SC-009 traced. No
unmapped tasks. Concluded: no NEW blocking issues; C1 is known/separate; recommended closing the
MEDIUM test-coverage + storage-field items via small tasks.md/spec.md edits before /sp.implement.
Offered to draft concrete remediation wording (not auto-applied).

## Outcome

- ✅ Impact: Confirmed artifacts are implementation-ready; surfaced 4 cheap pre-implement fixes.
- 🧪 Tests: n/a (read-only analysis).
- 📁 Files: none modified (read-only).
- 🔁 Next prompts: optional remediation edits for G1–G3/U1/I1, then /sp.implement (P0).
- 🧠 Reflection: Strict rule flags the inherited PostgreSQL divergence as CRITICAL; contextualized it
  as pre-existing + pending-amendment so it doesn't false-block this feature.

## Evaluation notes (flywheel)

- Failure modes observed: none.
- Graders run and results (PASS/FAIL): coverage 93%, 0 duplications, 1 contextualized CRITICAL.
- Prompt variant (if applicable): n/a
- Next experiment (smallest change to try): add G1/G2/G3 assertions to Phase 3/8 test tasks.
