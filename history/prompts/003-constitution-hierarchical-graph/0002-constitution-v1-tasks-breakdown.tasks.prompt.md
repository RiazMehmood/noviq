---
id: 0002
title: constitution v1 tasks breakdown
stage: tasks
date: 2026-05-21
surface: agent
model: claude-opus-4-7
feature: 003-constitution-hierarchical-graph
branch: feature/tier3-hierarchical-retrieval
user: Riaz Mehmood
command: /sp.tasks
labels: [legal-rag, hierarchical-retrieval, graph, constitution, tasks]
links:
  spec: specs/003-constitution-hierarchical-graph/spec.md
  ticket: N/A
  adr: pending — /sp.adr legal-corpus-hierarchical-graph-model
  pr: N/A
files:
  - specs/003-constitution-hierarchical-graph/tasks.md
tests:
  - structure_parser_test.rs (T012, planned)
  - graph_builder_test.rs (T020, planned)
  - constitution_v1_acceptance.rs (T029, planned)
  - db_inspection scripts 09 / 10 (T016, T021, planned)
---

## Prompt

`/sp.tasks` (then `yes` to proceed) — generate `tasks.md` for feature
003-constitution-hierarchical-graph from the plan/spec/research/data-model/
quickstart docs: 7 stages S1–S7, dependency-ordered, each task with acceptance
checks.

## Response snapshot

Generated `specs/003-constitution-hierarchical-graph/tasks.md` — **34 tasks**
(T001–T034) across 10 phases:

- Phase 1 Setup (T001–T003) · Phase 2 Foundational (T004–T005, migration 014 —
  blocks S3–S7) · Phases 3–9 = stages S1–S7 (each ending in an acceptance task)
  · Phase 10 Polish (T032–T034).

Adapted the SpecKit user-story template to the reality that S1–S7 are a
**dependency chain**, not independent stories — used `[S1]`–`[S7]` stage labels,
and stated explicitly that the shippable MVP is the full S1→S7 chain with the
A1–A4 gate green. Migration 014 hoisted into Foundational (it blocks S3–S7);
noted S1/S2 are DB-independent and may run parallel to it. Every task carries an
exact file path; acceptance tasks map to spec A1–A4.

## Outcome

- ✅ Impact: v1 is now fully decomposed into executable, dependency-ordered tasks
  with per-stage acceptance gates — ready for `/sp.implement` or manual build.
- 🧪 Tests: test tasks included (T012, T020, T029 + db_inspection 09/10) — the
  spec defines an A1–A4 acceptance gate, so tests are in scope.
- 📁 Files: `tasks.md` created; PHR 0002 recorded.
- 🔁 Next prompts: `/sp.adr legal-corpus-hierarchical-graph-model`; then begin
  T001 (or `/sp.implement`).
- 🧠 Reflection: the template assumes parallel user stories; a sequential
  pipeline needed honest adaptation rather than forcing artificial independence.

## Evaluation notes (flywheel)

- Failure modes observed: none this step.
- Graders run and results (PASS/FAIL): N/A (planning artifact).
- Prompt variant (if applicable): N/A.
- Next experiment (smallest change to try): start T001–T004 (skeleton + migration
  014), the unblocking foundation.
