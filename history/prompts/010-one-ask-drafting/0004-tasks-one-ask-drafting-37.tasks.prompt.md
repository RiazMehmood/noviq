---
id: 0004
title: tasks one-ask drafting 37
stage: tasks
date: 2026-06-11
surface: agent
model: claude-fable-5
feature: 010-one-ask-drafting
branch: 010-one-ask-drafting
user: Riaz Mehmood
command: /sp.tasks
labels: [tasks, tdd, drafting, knn-seam]
links:
  spec: specs/010-one-ask-drafting/spec.md
  ticket: null
  adr: history/adr/0013-one-ask-summary-composition-architecture.md, history/adr/0014-database-portability-seam-for-postgresql-re-platform.md
  pr: null
files:
  - specs/010-one-ask-drafting/tasks.md
tests:
  - planned: one_ask_knn_parity.rs (T004 risk gate), one_ask_acceptance.rs (canned writer), lib units per module, tsc; live gates manual (T035)
---

## Prompt

Generate tasks for Feature 010 one-ask-drafting from specs/010-one-ask-drafting/{spec,plan,research,data-model,quickstart}.md and contracts/one-ask-commands.md. Honor ADR-0013 (composition architecture) and ADR-0014 (portability seam rules: all vector search through retrieval.rs, state access confined to state.rs, ANSI-portable SQL). TDD per constitution; tests use canned writer output (no live LLM in CI); knn_parity test gates retrieval.rs before dependents.

## Response snapshot

tasks.md written: **37 tasks** — Setup 3 (scaffolding, migration 025, fixture corpus + canned writer outputs incl. malformed/marker-free variants) · Foundational 6 (T004 RED knn_parity → T005 GREEN retrieval.rs seam with documented fallback; intent/skeleton/state [P]; decompose visibility) · **US1 12** (research/compose/verify as RED→GREEN pairs, SectionWriter trait for canned testing, run_pipeline orchestrator, start_one_ask command, acceptance test, oneAsk.ts + OneAskView.tsx + FR-015 routing removal) · **US2 4** (no-coverage flags, refusal path, SC-007 acceptance scenario, flag/refusal UI) · **US3 5** (deterministic route_revision, run_revision, 5 contract commands, revision+interrupted acceptance, resume/finalize UI) · **US4 3** (stage callbacks + cancel token + 10 s heartbeat, UI) · Polish 4 (remediation audit, T035 live quickstart perf gate with SC table, full regression incl. 009 suite, CHANGELOG). Dependency graph: knn_parity blocks all retrieval dependents; US2 sequential after US1 (shared files); US3⊥US4 parallelizable. MVP = Phases 1–2 + US1 + US2 (accuracy gate ships with first draft). Format validated: 37/37 tasks match the checklist regex with IDs, [P]/[USn] labels, file paths.

## Outcome

- ✅ Impact: 010 is executable end-to-end; every task self-contained with exact paths; ADR-0014 seam rules embedded as binding constraints in the header and individual tasks.
- 🧪 Tests: TDD pairs per module; CI runs zero live-LLM; live validation isolated to T035.
- 📁 Files: specs/010-one-ask-drafting/tasks.md.
- 🔁 Next prompts: /sp.implement (or phase-by-phase: T001–T009 foundation first); /sp.analyze optional consistency pass before implementing.
- 🧠 Reflection: SectionWriter trait + injected embedder closure are what keep the whole pipeline testable without a model — same canned-output strategy that made the Phase D ingest gate testable.

## Evaluation notes (flywheel)

- Failure modes observed: none this stage; format regex validation run (37/37 PASS, one stray non-English token caught and fixed).
- Graders run and results (PASS/FAIL): checklist-format grader — PASS; story-independence review — US2 intentionally sequential after US1 (shared verify.rs/acceptance harness), documented in Dependencies.
- Prompt variant (if applicable): n/a
- Next experiment (smallest change to try): start implementation with T004/T005 alone and measure real KNN latency on the 65 GB DB before building dependents — converts the SC-005 assumption into data early.
