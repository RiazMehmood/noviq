---
id: 0005
title: analyze 010 artifacts consistency
stage: misc
date: 2026-06-11
surface: agent
model: claude-fable-5
feature: 010-one-ask-drafting
branch: 010-one-ask-drafting
user: Riaz Mehmood
command: /sp.analyze
labels: [analyze, consistency, read-only, coverage]
links:
  spec: specs/010-one-ask-drafting/spec.md
  ticket: null
  adr: history/adr/0013-one-ask-summary-composition-architecture.md, history/adr/0014-database-portability-seam-for-postgresql-re-platform.md
  pr: null
files:
  - (read-only — no files modified)
tests:
  - n/a (analysis stage)
---

## Prompt

/sp.analyze (no additional user input) — non-destructive cross-artifact consistency and quality analysis across spec.md, plan.md, tasks.md for 010-one-ask-drafting after task generation.

## Response snapshot

Read-only report produced. **0 CRITICAL, 8 findings**: C1/C2 (MEDIUM) — SC-004 (weeks→1-day pilot, 3 cases) and SC-006 (80% no structural rework) have zero measuring tasks (both need real officer pilot asks); U1 (MEDIUM) — vague-ask edge case (empty figures ⇒ FinancialAspect must omit, not invent) not pinned in T012 RED list; N1 (MEDIUM) — constitution §XI canonical citation form for FAST-indexed sources (instrument + page/span + "hierarchical detail unavailable" disclosure) not carried into rendered_label tasks T015/T020 — plan's §XI ✅ optimistic for fast-tier; I1 (LOW) — resume UI (T030/US3) lags drafting-tab routing (T021/US1) across checkpoints, acceptable phasing; I2 (LOW) — T002 TARGET_SCHEMA_VERSION wording ambiguous vs 021/023/024 precedent; A1 (LOW) — T004 assumes vec0 loads in cargo test harness, needs an assert-or-ignore strategy; T1 (LOW) — "provision extracts" (spec) vs "provision cards" (design) naming drift. Coverage: 21/23 requirements tasked = 91% (only SC-004/SC-006 gap); all 37 tasks map; constitution 13/13 no violations (§IV intact, ADR-0014 correctly scoped). Verdict: proceed to /sp.implement; offered remediation edits for C1/C2/U1/N1 pending user approval.

## Outcome

- ✅ Impact: implementation can start with known, bounded gaps; the two uncovered SCs are pilot-stage by nature, not build-stage.
- 🧪 Tests: n/a; A1 flagged a real test-infra risk for knn_parity (vec0 in test harness).
- 📁 Files: none (read-only).
- 🔁 Next prompts: user approves remediation edits (T038 pilot task, T012/T015/T020 one-liners) → /sp.implement starting at T004/T005.
- 🧠 Reflection: the §XI fast-tier disclosure (N1) is the kind of constitution detail that silently erodes — plan checked XI ✅ at the deep-tier happy path; analyze caught the fast-tier branch.

## Evaluation notes (flywheel)

- Failure modes observed: optimistic constitution check at plan stage (N1) — re-check principles per branch/tier, not per happy path.
- Graders run and results (PASS/FAIL): coverage grader 91% PASS-with-gaps; constitution grader 13/13 PASS (1 refinement).
- Prompt variant (if applicable): n/a
- Next experiment (smallest change to try): apply the 4 remediation one-liners before T001 so implementers inherit them in tasks.md rather than from this report.
