---
id: 0011
title: Re-chunk & re-index tasks extension (010)
stage: tasks
date: 2026-06-15
surface: agent
model: claude-opus-4-8
feature: 010-one-ask-drafting
branch: 011-postgres-pgvector-port
user: Riaz Mehmood
command: /sp.tasks (re-chunk & re-index extension)
labels: [tasks, rechunk, reindex, coalescing, tdd, 010-extension]
links:
  spec: specs/010-one-ask-drafting/spec-rechunk-reindex-extension.md
  ticket:
  adr:
  pr:
files:
  - specs/010-one-ask-drafting/tasks-rechunk-reindex-extension.md (new — 21 tasks across 6 phases)
tests:
  - "n/a (task list); TDD tasks defined: coalescing/repair/label units + 2 goldens"
---

## Prompt

commit the plan then tasks it

(/sp.tasks input) Generate tasks for the re-chunk & re-index extension … write to
specs/010-one-ask-drafting/tasks-rechunk-reindex-extension.md (do NOT overwrite 011's tasks.md).
Slice 1 (P1 MVP) = generic coalesce_undersized_leaves + wire into tree finalize + Settings re-index
trigger reusing indexer::do_index, gated CAUTIOUS/DANGEROUS; Slice 2 (P2) = verbatim-safe OCR repair
in text_normalizer; Slice 3 (P3) = hierarchical label fix + year-not-number via decompose. Tests:
units + Constitution no-regress golden + fragmented-rule golden. First task pins coalescing thresholds.
Reuse existing pipeline; no third chunking path / new re-index mechanism; preserve verbatim contract.

## Response snapshot

Committed plan (`edf7e9da`). Wrote 21 tasks (T001–T021) across 6 phases to the 010 extension tasks
file (NOT 011's tasks.md). Phase 1 Setup: T001 pin thresholds (~300/600/800c) + T002 fixtures
(fragmented-rule + Constitution). Phase 3 US1 MVP: RED units + 2 goldens (T003–T005) → implement
coalesce_undersized_leaves mirroring split_oversized_leaves (T006) → wire into finalize (T007) →
Settings re-index command reusing indexer::do_index, §V-gated (T008–T010) → GREEN one cargo run (T011)
→ live gate SC-001/002/005 (T012). Phase 4 US2 repair (T013–T015). Phase 5 US3 labels (T016–T018).
Phase 6 polish + CHANGELOG + full suite (T019–T021). Build cadence honored: one cargo per slice.
MVP = US1 alone.

## Outcome

- ✅ Impact: executable, dependency-ordered task list; MVP (US1) clearly isolated; goldens guard verbatim + Constitution.
- 🧪 Tests: TDD tasks defined (RED before impl per slice).
- 📁 Files: tasks-rechunk-reindex-extension.md under specs/010.
- 🔁 Next prompts: /sp.implement Slice 1 (US1) — start with T001 thresholds + T002 fixtures + T003–T005 RED.
- 🧠 Reflection: kept the extension fully inside existing files; the only new files are tests — matches "no third chunking path / no new mechanism."

## Evaluation notes (flywheel)

- Failure modes observed: SDD scripts are branch-coupled (011); all 010-extension artifacts placed manually to avoid clobbering 011.
- Graders run and results (PASS/FAIL): n/a (task gen); checklist + plan gates already PASS.
- Prompt variant (if applicable): n/a
- Next experiment (smallest change to try): implement T001+T002+T003 (RED coalescing test) as the first executable step.
