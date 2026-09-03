---
id: "005"
title: Notebook Engine Tasks
stage: tasks
date: 2026-04-13
surface: agent
model: claude-sonnet-4-6
feature: 001-notebook-knowledge-engine
branch: 001-notebook-knowledge-engine
user: Riaz Mehmood
command: /sp.tasks
labels: ["tasks","notebook","knowledge-engine","implementation","tauri","solidjs","sqlite","llm"]
links:
  spec: specs/001-notebook-knowledge-engine/spec.md
  ticket: null
  adr: history/adr/0001-llm-model-hosting-isolation-strategy.md
  pr: null
files:
  - specs/001-notebook-knowledge-engine/tasks.md
tests:
  - N/A
---

## Prompt

/sp.tasks

## Response snapshot

Generated `specs/001-notebook-knowledge-engine/tasks.md` — 93 dependency-ordered implementation tasks
across 8 phases for the Notebook & Knowledge Engine (App 1).

**Phase breakdown**:
- Phase 1 Setup (T001–T006, 6 tasks): Tauri 2.0 scaffold, Cargo.toml, package.json, tauri.conf.json,
  Rust module skeleton, SolidJS directory skeleton.
  Checkpoint: `cargo tauri dev` opens empty app window.

- Phase 2 Foundational (T007–T022, 16 tasks) — BLOCKS all user stories:
  SQL migration (all 11 tables), schema.rs (sqlx migrate + vec0 DIM detection), hardware_detector.rs
  (HardwareTier), all 7 repositories (notebooks, sources, chunks, sessions, entities, batch_jobs, audit),
  embedder.rs (fastembed-rs nomic/MiniLM), chunker.rs (512-token semantic), llm.rs (ModelSlot + 90s
  eviction + VisionSlot), task_budget.rs (Active/Throttled/Idle + priority channel), file_watcher.rs
  (notify v6 + 500ms debounce), query_cache service.

- Phase 3 US1/P1 (T023–T048, 26 tasks): File parsers (PDF/DOCX/XLSX/TXT/audio/image/scanned),
  indexer orchestrator, 5-tier query_router, query_cache commands, notebook+source+session commands,
  chat commands (ask_question, stream_answer, regenerate), SolidJS stores + components
  (NotebookList, SourceList, ChatPanel, QueryRouter indicator, Hardware tier indicator).
  Checkpoint: End-to-end Ask Question test against real PDF.

- Phase 4 US2/P2 (T049–T059, 11 tasks): folder_watcher_commands, global_search with FTS5 + vec,
  auto-indexing on FS events, GlobalSearchPanel component, search result dedup.

- Phase 5 US3/P3 (T060–T068, 9 tasks): analysis_engine.rs (GBNF structured output),
  template library (summary/QA/compare/timeline/matrix), PDF/TXT exporters,
  AnalysisPanel + ExportButton components.

- Phase 6 US4/P4 (T069–T077, 9 tasks): entity_extractor two-pass (regex sync + LLM async),
  knowledge_graph.rs (adjacency queries), entities Tauri commands, GraphCanvas SolidJS component
  (force-directed layout), entity drill-down panel.

- Phase 7 US5/P5 (T078–T085, 8 tasks): batch_processor.rs (parallel with TaskBudget),
  batch Tauri commands, BatchPanel + BatchResultTable components, progress streaming.

- Phase N Polish (T086–T093, 8 tasks): Error boundaries, audit_log integration,
  .echoai archive export/import, low-storage guard (<500 MB free), CPU/memory profiling
  (criterion + dhat), integration test harness (real DB + sample corpus).

**Parallel opportunities**: 40+ tasks marked [P] — all repositories (T010–T016), all parsers
(T025–T031), all Tauri command files (T033–T047), all SolidJS components within each story phase.

**Dependencies**: Phase 2 → all subsequent phases. US2 depends on US1 indexer.
US3/US4/US5 are independent of each other after US1.

**Suggested MVP scope**: Phase 1 + Phase 2 + Phase 3 (US1 complete = end-to-end ingest + Q&A).

## Outcome

- ✅ Impact: 93 dependency-ordered tasks covering all 5 user stories; each phase is independently
  testable with explicit checkpoints; [P] markers enable parallel execution across 40+ tasks;
  all design documents (spec, plan, data-model, contracts) fully mapped into concrete file-level tasks.
- 🧪 Tests: N/A (task generation stage — tests embedded as acceptance criteria in each task)
- 📁 Files: specs/001-notebook-knowledge-engine/tasks.md
- 🔁 Next prompts: /sp.implement (begin T001 Phase 1 Setup)
- 🧠 Reflection: The foundational phase (Phase 2) is the critical path — 16 tasks that must all
  complete before any user-story work begins. The ModelSlot (T019) and embedder (T017) tasks are
  the highest-risk items due to native library integration (llama-cpp-2 + fastembed-rs). Suggesting
  these are the first to unblock in Phase 2 after the migration (T007/T008) is stable.

## Evaluation notes (flywheel)

- Failure modes observed: None — all source documents (spec, plan, data-model, contracts, research)
  were complete and consistent before task generation ran.
- Graders run and results (PASS/FAIL): All 93 tasks follow checklist format (checkbox + ID + optional
  [P]/[Story] + description + file path). All user stories have independent test criteria. PASS.
- Prompt variant (if applicable): N/A
- Next experiment (smallest change to try): /sp.implement T001–T006 (Phase 1 Setup) to validate
  that Tauri 2.0 scaffold + Cargo.toml compiles cleanly before moving to Phase 2.
