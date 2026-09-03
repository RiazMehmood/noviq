---
id: 0003
title: structured-table-capture tasks
stage: tasks
date: 2026-07-12
surface: agent
model: claude-opus-4-8
feature: 023-structured-table-capture
branch: 023-structured-table-capture
user: Riaz Mehmood
command: /sp.tasks
labels: [ingestion, tables, chunking, graph, tasks]
links:
  spec: specs/023-structured-table-capture/spec.md
  ticket:
  adr:
  pr:
files:
  - specs/023-structured-table-capture/tasks.md
tests: []
---

## Prompt

/sp.tasks (no extra args) — generate the dependency-ordered task list for feature
023-structured-table-capture from its plan + spec + data-model + contracts.

## Response snapshot

Generated tasks.md: **21 tasks** in 6 phases, organized by the spec's 3 user stories.
- Setup (T001): resolve live CHECK constraint names.
- Foundational (T002–T006): additive migration 013 (widen unit_type + node_type CHECK); `ExtractedTable`
  type + region-marker helpers; `reflow_with_sidecar` emits markers + structured tables; thread
  `Vec<ExtractedTable>` out of extraction; update reflow goldens + marker round-trip test.
- US1 / P1 / MVP (T007–T011): atomic table region in structure_parser (no clause-shred); whole-table chunk
  in structured_importer (markdown, suppress in-region prose); table graph node; wire counting to table
  nodes; C-1/C-2/C-6 tests. Delivers RoB=39.
- US2 / P1 (T012–T013): whole-table chunk included in `load_content_for_summary` so summary covers tables.
- US3 / P2 (T014–T016): per-row `table_row` chunks + row graph nodes; C-2/C-3 tests.
- Polish (T017–T021): prose-only regression guard (SC-004), false-positive guard (FR-009), full VM test run,
  live acceptance per quickstart, CHANGELOG + ADR suggestion.

All tasks follow `- [ ] Txxx [P?] [US?] desc + path`. Dependencies: Setup→Foundational→US1(MVP)→US2/US3→Polish.
MVP = T001–T011.

## Outcome

- ✅ Impact: Executable, dependency-ordered task list; MVP (RoB=39) isolated as US1.
- 🧪 Tests: test tasks included (C-1..C-6 contract + regression/false-positive guards + live acceptance).
- 📁 Files: specs/023-structured-table-capture/tasks.md
- 🔁 Next prompts: /sp.implement (or hand-execute starting T001) on the VM build path.
- 🧠 Reflection: Story-sliced so US1 alone ships the headline count/list fix; US2/US3 are additive, keeping
  the risky pipeline change shippable in a bounded first increment.

## Evaluation notes (flywheel)

- Failure modes observed: reflow goldens will break on markers (T006 handles); counting must learn new node types (T010).
- Graders run and results (PASS/FAIL): format check PASS (all tasks have checkbox+ID+path; story labels on US phases)
- Prompt variant (if applicable): n/a
- Next experiment (smallest change to try): implement Foundational + US1 and validate RoB=39 live before US2/US3.
