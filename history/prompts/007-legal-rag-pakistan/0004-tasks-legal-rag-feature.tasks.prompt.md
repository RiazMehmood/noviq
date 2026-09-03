---
id: "0004"
title: "tasks legal RAG feature"
stage: tasks
date: 2026-06-02
surface: agent
model: claude-opus-4-7
feature: 007-legal-rag-pakistan
branch: 007-legal-rag-pakistan
user: Riaz Mehmood
command: /sp.tasks
labels: [feature-007, legal-rag, pakistan, tasks]
links:
  spec: specs/007-legal-rag-pakistan/spec.md
  ticket: ""
  adr: ""
  pr: ""
files:
  - path: specs/007-legal-rag-pakistan/tasks.md
    change: created
tests:
  - name: task-list format validation
    status: PASS
---

## Prompt

`/sp.tasks` invoked with no user-input block. Inputs: plan.md + spec.md (required) + research.md + data-model.md + contracts/ + quickstart.md (all present).

## Response snapshot

Generated `specs/007-legal-rag-pakistan/tasks.md` with **95 tasks** organized into six phases:

| Phase | Tasks | Story | Highlights |
|---|---|---|---|
| 1 — Setup (Slice 0) | T001–T014 | — | branch, CHANGELOG, migration 020, schema bump 15→16, schema_wipe_test update, migration_020_test, citation module skeleton + types + 12 golden tests + formatter implementation, sidecar migration tool, SpecKit scaffolding |
| 2 — Foundational | T015–T021 | — | legal_ir / deep_reindex / intent_router module skeletons, llm.rs explicit LlmTier param refactor, indexer.rs tier-check refactor |
| 3 — US1 fast-path | T022–T042 | US1 | classifier rule-first + small-LLM tiebreak, slot extractor, Lookup / ListChildren / Summarize / ActionItems / Fallback handlers, chat_query command extension, ChatBubble + Chat.tsx UI, 40-prompt eval set + report, US1 acceptance test |
| 4 — US2 deep + amendment | T043–T067 | US2 | legal_ir types/schema/prompts, amendment_parser deterministic pass, mistral_builder adapter, chunks_replace transaction, deep_reindex orchestrator (5+ passes with resumable progress_store), Tauri commands (deep_reindex_document CAUTIOUS, cancel READ-ONLY, downgrade DANGEROUS), JSON sidecar drop-in, Difference (amendment) + Timeline (amendment) handlers, hierarchical-citation upgrade on Lookup, fast→deep upgrade prompt enforcement, DeepReindexProgress + DocActions + chip UI, slice-A re-index test + US2 acceptance test |
| 5 — US3 cross-doc + procedure | T068–T088 | US3 | xref_extractor Stage 1 regex + Stage 2 LLM disambiguator, xref pass wired into orchestrator, Difference (instrument) + CrossCompare (N-way) + Timeline (thematic) handlers, "see also" surface populated from graph_edges, procedure_extractor heuristic classifier + grammar-constrained step extraction + cross-instrument stitching, Procedure handler, FlowDiagram.tsx linear render + branching expand, xref edge precision test + procedure extractor test + US3 acceptance test |
| 6 — Polish + v1 ship | T089–T095 | — | final acceptance harness `legal_rag_v1_acceptance.rs` (5 gates), 70-prompt eval report archive, CHANGELOG entry, manual memory-tree follow-ups, PR + tag |

**Format compliance:** every line starts with `- [ ] T### ` plus optional `[P]` plus optional `[Story]` plus a description containing an absolute or repo-relative file path. Spot-checked T001 / T026 / T050 / T080 / T089 — all match.

**Parallel opportunities:** 38 of 95 tasks carry `[P]`. Critical path ≈ 60 tasks if all parallelism is exploited.

**MVP scope:** Phase 1 + Phase 2 + Phase 3 (US1) — officer can upload a Pakistan PDF and ask basic chat questions with citations on the fast tier. Smallest shippable increment.

**Story dependencies inline:** US1 and US2 can run in parallel after Phase 2; US3 strictly depends on US2 (cross-references and procedures only exist on deep-indexed instruments); Phase 6 depends on US3.

**Test discipline noted:** TDD tasks (write failing test first, then implement) appear where the spec requires golden-corpus or eval-set verification — Slice 0 citation goldens, migration test, slice-A re-index, slice-B/C/D evals, final acceptance gate. Pure helper functions don't get redundant test scaffolding.

## Outcome

- ✅ Impact: 95-task implementation list ready for `/sp.analyze` cross-checking or direct `/sp.implement` start.
- 🧪 Tests: task-list format validation PASS (all 95 tasks match the strict checklist format).
- 📁 Files: `specs/007-legal-rag-pakistan/tasks.md` (created).
- 🔁 Next prompts: `/sp.analyze` to cross-check spec/plan/tasks consistency; then `/sp.implement` (or `/sp.git.commit_pr` to land the spec/plan/tasks bundle first). Optional `/sp.adr legal-pivot-two-tier-indexing` to record the architectural decision.
- 🧠 Reflection: The plan's Slice A/B/C/D structure mapped cleanly onto SDD user-story phases. The one care point: US1's handlers/lookup.rs gets extended in US2 (T057) and again referenced in US3 — coordinate that file's history if US1 and US2 run in parallel.

## Evaluation notes (flywheel)

- Failure modes observed: none.
- Graders run and results (PASS/FAIL): task-list format scan PASS (95/95).
- Prompt variant (if applicable): n/a.
- Next experiment (smallest change to try): on the next `/sp.tasks` run for any feature, consider auto-generating a dependency graph SVG alongside the ASCII version.
