---
id: 0004
title: agentic-deep-search-tasks
stage: tasks
date: 2026-07-13
surface: agent
model: claude-opus-4-8
feature: 025-agentic-deep-search
branch: 025-agentic-deep-search
user: Riaz Mehmood
command: /sp.tasks
labels: [deep-search, agentic-rag, tasks]
links:
  spec: specs/025-agentic-deep-search/spec.md
  ticket:
  adr: history/adr/0021-bounded-agentic-multi-pass-deep-retrieval.md
  pr:
files:
  - specs/025-agentic-deep-search/tasks.md
tests:
  - included per contract acceptance checks (facet-expansion, coverage, packing, grounding, loop invariants)
---

## Prompt

/sp.tasks for feature 025. Generate dependency-ordered tasks.md from plan+research+data-model+contracts. MVP =
P1 completeness path behind ECHO_AGENTIC_DEEP. Front-load a coverage-check prototype BEFORE the full loop.
Include: extract retrieve_for_query (behavior-preserving + normal-path regression guard FR-008/INV-7); the
deep_search module (mod/expand/coverage/assemble/verify); reuse decompose, one_ask quote_verifies/
foreign_number_in_prose, pack_windows, graph/summary arms, DEEP_SEARCH task-local; api/query.rs flag routing;
DeepTrace; cancellation at boundaries; unit tests per contract (facet parse+fallback, coverage parse+bias,
dedup+pending-first packing, grounding verifier incl. Zoonology fabrication-drop, loop invariants ≤3/≤2); A/B
harness under research/025-agentic-deep-search/. No new schema, no window/GPU change. Map to FR-001..014 /
SC-001..006.

## Response snapshot

Wrote `specs/025-agentic-deep-search/tasks.md` — **30 tasks** across 7 phases, tests included, every task with
id/labels/file path.

- **Phase 1 Setup (T001–T003)**: module skeleton + `agentic_enabled()` flag + eval scaffold.
- **Phase 2 Foundational (T004–T008)**: extract shared `retrieve_for_query` from `route_scoped_with_override`
  (T004) + normal-path regression guard (T005, FR-008/INV-7/SC-005); expose `pack_windows`/decompose/task-local
  (T006); in-memory types (T007); **front-loaded coverage-check spike (T008)** measuring latency+accuracy on
  5–10 gold questions → GO/No-Go before the loop.
- **Phase 3 US1 (P1, MVP, T009–T018)**: tests-first (facet/coverage/assembly/invariants) then expand→coverage→
  assemble→orchestrator `run_agentic_deep`→flag routing→fast-path/degrade.
- **Phase 4 US2 (P1, T019–T021)**: grounding verifier (reuse one_ask primitives) + Zoonology fabrication-drop
  regression + wire gate (≥1 verified cite or grounded-fallback).
- **Phase 5 US3 (P2, T022–T024)**: cap #1/#2 max_tokens, bound assertions ≤3/≤2, cancellation test.
- **Phase 6 US4 (P3, T025–T026)**: DeepTrace populate + admin panel render.
- **Phase 7 Polish (T027–T030)**: A/B eval vs legacy deep, CHANGELOG+quickstart, **guarded retirement of legacy
  one-pass deep after A/B passes**, frontend copy check.

Includes dependency graph, parallel examples, MVP/incremental strategy, and an FR-001..014 / SC-001..006 →
task coverage matrix (every FR and SC mapped).

## Outcome

- ✅ Impact: Executable task list ready; MVP scope = US1 behind the flag. De-risk spike gates the novel piece.
- 🧪 Tests: included (5 unit-test task groups + regression + integration).
- 📁 Files: specs/025-agentic-deep-search/tasks.md.
- 🔁 Next prompts: `/sp.implement` (or start T001). Recommend running T004+T005 (extraction+guard) and T008
  (coverage spike) first as the two highest-risk foundational items.
- 🧠 Reflection: US1 and US2 are both P1 but split cleanly — US1 delivers the loop end-to-end; US2 adds the
  deterministic grounding gate as a separable, independently-testable hardening. Keeps MVP demoable while the
  citation gate lands right after.

## Evaluation notes (flywheel)

- Failure modes checked: cross-story dependencies that break independence (avoided — US2/3/4 wrap US1 but each
  has its own independent test); vague tasks (each names a file + FR); same-file parallel conflicts (P markers
  only on different files).
- Graders run and results (PASS/FAIL): format check — all 30 tasks carry checkbox+id+labels+path; coverage
  matrix — every FR/SC mapped. PASS.
- Prompt variant (if applicable): n/a.
- Next experiment (smallest change to try): execute T008 spike; if coverage-check accuracy is weak on the gold
  set, revisit the coverage contract (e.g., require support-passage ids) before building T014.
