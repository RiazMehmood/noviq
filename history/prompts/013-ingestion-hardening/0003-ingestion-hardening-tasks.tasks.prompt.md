---
id: 0003
title: Ingestion hardening tasks
stage: tasks
date: 2026-06-19
surface: agent
model: claude-opus-4-8
feature: 013-ingestion-hardening
branch: 013-ingestion-hardening
user: Riaz Mehmood
command: /sp.tasks
labels: [ingestion, citation-grade, deterministic, tasks]
links:
  spec: specs/013-ingestion-hardening/spec.md
  ticket:
  adr:
  pr:
files:
  - specs/013-ingestion-hardening/tasks.md
tests:
  - cargo test --lib
  - cargo test --test ingestion_hardening_acceptance
---

## Prompt

/sp.tasks (no additional user input)

Generate an actionable, dependency-ordered tasks.md for feature 013-ingestion-hardening from
plan.md, spec.md, research.md, data-model.md, contracts/, and quickstart.md. Organize by user story,
include test tasks (spec mandates the FR-012 regression fixture), strict checklist format with file
paths, dependency graph, parallel examples, and MVP scope.

## Response snapshot

Generated `tasks.md` with **43 tasks** across 8 phases, honoring the plan's LOCKED phase order
(P0→P1→P2→P3→P4→US5) as the sequential delivery spine, each phase tagged with the user story it serves.

- **Phase 1 Setup** (T001–T003): acceptance-test scaffold + frozen 177-chunk fixture assets +
  verbatim-substring helper.
- **Phase 2 Foundational / P0** (T004–T008): D9 JOIN_TARGETS audit + D12 page_offsets→persist_tree.
- **Phase 3 / P1** (T009–T015, US2+US3): geometry zoning — footnote + footer page-number bands,
  persist_footnote_chunks, marker-based anchoring.
- **Phase 4 / P2** (T016–T024, US1): structural tree — alpha-suffix articles, sequence guard, Chapter
  nodes, catchline merge, unit_type, 25/25A flag.
- **Phase 5 / P3** (T025–T029, US1): vocabulary resolver, stop generic.rs "Section {num}" leak.
- **Phase 6 / P4** (T030–T033, US4): clause + amendment-bracket boundaries.
- **Phase 7** (T034–T039, US5): cross_reference + amended_by edges, amendment_events; conditional
  migration 004.
- **Phase 8 Polish** (T040–T043): full acceptance gate (SC-001…SC-009), corpus property test,
  re-index + fixture diff, log/tracker updates.

Each task has a checkbox, ID, [P]/[Story] labels where applicable, and an exact file path with the
verified function/line anchors. Cross-phase parallelism intentionally disabled (locked order).
MVP = Phase 1→2→3→4→5 (US1 fully delivered, citation-grade core).

## Outcome

- ✅ Impact: Immediately-executable, dependency-ordered task plan ready for /sp.implement.
- 🧪 Tests: 12 test tasks (per-phase, written-first) + acceptance gate; map to SC-001…SC-009.
- 📁 Files: specs/013-ingestion-hardening/tasks.md.
- 🔁 Next prompts: /sp.implement (start P0); optionally /sp.adr for the two-pass + geometry-zoning
  architecture decision.
- 🧠 Reflection: The generic "stories in priority order" template was overridden by the plan's locked
  clean-signal-before-structure ordering — documented explicitly so the sequence is unambiguous.

## Evaluation notes (flywheel)

- Failure modes observed: none in task generation.
- Graders run and results (PASS/FAIL): format self-check PASS (all tasks have checkbox+ID+path; story
  labels on US phases only).
- Prompt variant (if applicable): n/a
- Next experiment (smallest change to try): execute T004–T008 (P0), re-index, diff fixture.
