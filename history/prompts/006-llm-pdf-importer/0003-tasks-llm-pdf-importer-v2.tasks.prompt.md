---
id: "0003"
title: tasks llm pdf importer v2
stage: tasks
date: 2026-05-29
surface: agent
model: claude-opus-4-7
feature: 006-llm-pdf-importer
branch: 006-llm-pdf-importer
user: engr.riaz2010@gmail.com
command: /sp.tasks
labels: [tasks, llm, pdf, importer, dependency-graph]
links:
  spec: specs/006-llm-pdf-importer/spec.md
  ticket: ""
  adr: ""
  pr: ""
files:
  - path: specs/006-llm-pdf-importer/tasks.md
    change: created
tests:
  - target: cargo test --test ir_validator
    status: planned (T010)
  - target: cargo test --test llm_ir_importer_acceptance
    status: planned (T016, T030, T031)
  - target: cargo test --test ir_cache_lifecycle
    status: planned (T049)
---

## Prompt

Generate dependency-ordered tasks.md for feature 006 on branch 006-llm-pdf-importer. Inputs: specs/006-llm-pdf-importer/{spec.md, plan.md, research.md, data-model.md, contracts/*, quickstart.md}. Constitution VIII says each P-story must independently ship; keep that property in task grouping.

## Response snapshot

Wrote `specs/006-llm-pdf-importer/tasks.md` — **62 tasks across 6 phases**:

- **Phase 1 — Setup (T001–T004)**: deps, module skeleton, version constant. 3 parallel.
- **Phase 2 — Foundational (T005–T014)**: migration 018, JSON schema, IR types, validator, IrBuilder trait, ProgressSink, ConstitutionIr→DocIr converter, cache repo. T005 first; then 8 parallel; T009/T010 serial.
- **Phase 3 — US1 P1 MVP (T015–T030)**: chunker, prompts, llguidance grammar binding, classifier, MistralIrBuilder, retry loop, cache writer, indexer dispatch wiring, fallback notification + short-circuit, progress/done events, cancel command. 16 tasks; ends at SC-001/002/003/008.
- **Phase 4 — US2 P2 Repair UI (T031–T048)**: pdftoppm page render, get_doc_ir / save_repair / list_docs_needing_review commands, sticky-repair preservation in cache writer, frontend (ReviewBadge, importer.ts wrappers, useImporterProgress, toast on `outcome:"partial"`, PdfPagePane, IrEditorPane, RepairView, route). 18 tasks; ends at SC-004/005/009.
- **Phase 5 — US3 P3 Cache lifecycle (T049–T056)**: cache lookup short-circuit, file_watcher orphan hook, sweep job, scheduler tick, devtools commands, schema-version bump migration. 8 tasks; ends at SC-006/007.
- **Phase 6 — Polish (T057–T062)**: changelog entry, version re-extract helper, quickstart smoke + screenshot, perf sanity, full cargo test audit, memory close-out. 6 tasks.

**Independence property** (Constitution VIII): each user story's acceptance can be driven through `llm_ir_importer_acceptance.rs` with a **stub `IrBuilder`** returning a frozen golden DocIr. US2 and US3 exercise the cache, validator, partial-reindex, and repair UI without needing the real Mistral runtime — they are independently shippable.

**Parallel opportunities called out**: 30+ tasks tagged `[P]`. Phase 2 has the largest parallel fan-out (8 simultaneous); Phase 4 has 8 frontend-leaning parallel tasks.

**Tests included** (the plan committed to three test targets; the spec's acceptance scenarios are testable). Format-validated: every task starts with `- [ ]`, has a `T0NN` id, a `[P]` or `[Story]` label as appropriate, and an exact file path.

## Outcome

- ✅ Impact: feature 006 has a concrete, dependency-ordered execution plan. 62 tasks, MVP scope = T001–T030 (Phase 1+2+3).
- 🧪 Tests: 3 test targets planned and mapped to specific task IDs.
- 📁 Files: 1 created (tasks.md, ~400 lines).
- 🔁 Next prompts: `/sp.analyze` for a cross-artefact consistency pass before code; or `/sp.implement` to begin executing the task list (recommended path: do Phase 1+2 first, validate, then proceed).
- 🧠 Reflection: keeping each user story executable against a stub `IrBuilder` made the independence property real — without it, US2 and US3 would depend on US1's Mistral integration and we'd lose the constitution-VIII guarantee. The stub is one of the cheapest decisions in this plan.

## Evaluation notes (flywheel)

- Failure modes observed: none.
- Graders run and results (PASS/FAIL): tasks format check — every task carries checkbox+id+(label)+path; no malformed entries.
- Prompt variant (if applicable): n/a.
- Next experiment (smallest change to try): when implementing T021 (`MistralIrBuilder`), measure first-attempt JSON validity with llguidance vs. unconstrained baseline on a 3-chunk sample document — the brainstorm assumed 85% → 99% lift; record the actual numbers in CHANGELOG_DEV.md so future constrained-decoding features have a data point.
