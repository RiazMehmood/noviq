---
id: 0004
title: Tasks Postgres pgvector port
stage: tasks
date: 2026-06-11
surface: agent
model: claude-fable-5
feature: 011-postgres-pgvector-port
branch: 011-postgres-pgvector-port
user: riaz
command: /sp.tasks
labels: [tasks, postgres, pgvector, migration, bundled-engine, tdd]
links:
  spec: specs/011-postgres-pgvector-port/spec.md
  ticket: null
  adr: history/adr/0015-postgresql-as-single-source-of-truth-bundled-engine-pgvector-hard-cutover.md
  pr: null
files:
  - specs/011-postgres-pgvector-port/tasks.md
tests:
  - "format validation: all 39 tasks follow checkbox + ID (+[P]/[USn]) + file-path format"
---

## Prompt

run /sp.tasks — args: 011-postgres-pgvector-port

## Response snapshot

Generated tasks.md: **39 tasks** across 7 phases, organized by the spec's 4 user
stories. Phase 1 Setup (3): T001 pg_spike HARD GATE (offline bundled PG +
vendored pgvector proof — only task allowed to invalidate the plan), deps swap,
pgvector vendoring. Phase 2 Foundational (8, BLOCKS all): engine.rs lifecycle,
per-tier settings, 25-migrations→001_baseline.sql (vector(768) + halfvec HNSW +
generated tsv), schema runner, pool swap at db/mod.rs (Db=PgPool), test_pg()
helper, mechanical dialect sweep with `// PORT(US1)` placeholders, compile-green
checkpoint. US1 (8, P1=MVP): RED knn_parity+FTS tests → port retrieval.rs (C1),
query_router (C2), indexer writes, call-site sweep → GREEN suites → live dev
gate ≤15 s/no-oomd. US2 (5): orphan adoption, port-conflict retry, EngineError
UX (StartupError.tsx), Tauri packaging of PG+pgvector, loopback test. US3 (7):
RED migration_verify fixture → migration_job table, sqlite_import.rs (rusqlite
confined, binary COPY, resume, index-after-load), commands+TS API, MigrationPanel,
GREEN, live 65 GB run (SC-004). US4 (4): pg_parity ≥95% (with pre-cutover
baseline capture ordering constraint vs T030), seam-audit source test (FR-009),
full sweep (SC-007), constitution v2.0.0 amendment = MERGE GATE (FR-013). Polish
(4): SC-006 throughput, SC-002 10-run stability, dead-code cleanup, changelog.
Dependency note: port nature makes Phases 1→2→US1 sequential; US2 parallelizable
with US1 after T011; T031 baseline must precede T030 cutover.

## Outcome

- ✅ Impact: executable dependency-ordered task list; MVP = Setup+Foundational+US1.
- 🧪 Tests: format validation PASS (39/39 checklist-format with file paths); TDD ordering encoded (RED tasks precede impl in US1/US3).
- 📁 Files: specs/011-postgres-pgvector-port/tasks.md.
- 🔁 Next prompts: optionally /sp.analyze; then /sp.implement starting at T001 (spike gate); apply oomd sudo drop-in before any live runs.
- 🧠 Reflection: for a hard-cutover port, the template's "stories independent after foundational" rule bends — honesty about the sequential Phase 1→2→US1 spine beats pretending stories are parallel.

## Evaluation notes (flywheel)

- Failure modes observed: none; numbering kept simple (T013a for the added FTS RED test rather than renumbering).
- Graders run and results (PASS/FAIL): checklist-format grader PASS; story-coverage check PASS (every FR maps to ≥1 task; SC-001..007 each have a measuring task).
- Prompt variant (if applicable): n/a
- Next experiment (smallest change to try): run T001 spike first — it is the only cheap falsifier of the whole plan.
