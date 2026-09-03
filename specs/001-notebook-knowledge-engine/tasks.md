# Tasks: Notebook & Knowledge Engine

**Input**: Design documents from `/specs/001-notebook-knowledge-engine/`
**Prerequisites**: spec.md ✅ | plan.md ✅ | research.md ✅ | data-model.md ✅ | contracts/ ✅

## Format: `[ID] [P?] [Story?] Description`

- **[P]**: Can run in parallel (different files, no incomplete dependencies)
- **[Story]**: US1–US5 maps to user stories from spec.md
- File paths are absolute from repo root

---

## Phase 1: Setup

**Purpose**: Tauri 2.0 project scaffold and directory structure

- [X] T001 Initialize Tauri 2.0 project scaffold (`cargo tauri init`) and confirm `src-tauri/` + `src/` root layout matches plan.md Project Structure
- [X] T002 [P] Configure `src-tauri/Cargo.toml` with all Rust dependencies: tauri 2.x, llama-cpp-2, fastembed, sqlx (sqlite + runtime-tokio), sqlite-vec, whisper-rs, tokio, notify 6.x, sysinfo, lopdf, docx-rs, calamine, zip, uuid, serde/serde_json
- [X] T003 [P] Configure `package.json` + `vite.config.ts` for SolidJS, @solidjs/router, Tailwind CSS, and vitest
- [X] T004 [P] Configure `src-tauri/tauri.conf.json` (window title "EchoAI", file system + notification + dialog permissions, dev/prod URLs)
- [X] T005 [P] Create Rust module directory skeleton: `src-tauri/src/commands/`, `src-tauri/src/services/`, `src-tauri/src/services/parsers/`, `src-tauri/src/db/repositories/`, `src-tauri/src/db/migrations/`
- [X] T006 [P] Create SolidJS directory skeleton: `src/components/notebook/`, `src/components/chat/`, `src/components/search/`, `src/components/knowledge-graph/`, `src/components/analysis/`, `src/components/batch/`, `src/pages/`, `src/stores/`, `src/api/`

**Checkpoint**: `cargo tauri dev` compiles and opens an empty app window

---

## Phase 2: Foundational

**Purpose**: Blocking prerequisites — DB schema, core services, all repositories.
No user story work can begin until this phase is complete.

⚠️ **CRITICAL**: All Phase 3+ tasks depend on this phase completing first.

- [X] T007 Write `src-tauri/src/db/migrations/001_initial_schema.sql` — all 11 tables from data-model.md: notebooks, notebook_sources, sources, indexed_folders, chunks, chunks_fts (FTS5 + triggers), chunk_embeddings (vec0 placeholder), query_cache, query_sessions, query_messages, entities, entity_mentions, entity_relationships, batch_jobs, batch_results, audit_log
- [X] T008 Implement `src-tauri/src/db/schema.rs` — sqlx `migrate!()` runner, `db_config` table creation, dimension detection logic (reads hardware tier, writes DIM to db_config, creates `chunk_embeddings USING vec0(chunk_id TEXT PARTITION KEY, embedding FLOAT[{DIM}])` with runtime-substituted dimension)
- [X] T009 Implement `src-tauri/src/services/hardware_detector.rs` — detect total RAM, VRAM (via sysinfo), derive `HardwareTier` struct (maxModelTier: 1–5, embeddingModel: nomic/minilm, hasDiscreteGpu, vramGb)
- [X] T010 [P] Implement `src-tauri/src/db/repositories/notebooks.rs` — create, get, list (ordered by updated_at), rename, delete (cascade), update_timestamp
- [X] T011 [P] Implement `src-tauri/src/db/repositories/sources.rs` — insert, get_by_path (dedup check), get_by_id, list_by_notebook, update_status (state machine: pending→processing→indexed|failed), update_error, set_indexed_at
- [X] T012 [P] Implement `src-tauri/src/db/repositories/chunks.rs` — bulk_insert (source_id, chunk_index, content, char_start, char_end, page_number, token_count), list_by_source, get_by_id, delete_by_source
- [X] T013 [P] Implement `src-tauri/src/db/repositories/sessions.rs` — create (notebook_id), get, list_by_notebook, touch_updated_at
- [X] T014 [P] Implement `src-tauri/src/db/repositories/entities.rs` — upsert (canonical_value + entity_type as unique key), get, list_by_type, search_by_value, increment_source_count, insert_mention, insert_relationship
- [X] T015 [P] Implement `src-tauri/src/db/repositories/batch_jobs.rs` — create, get, update_status, increment_counters (processed_files, failed_files), set_completed_at, insert_result, list_results (paginated)
- [X] T016 [P] Implement `src-tauri/src/db/repositories/audit.rs` — insert_event (event_type, entity_id, details_json), list (paginated, newest first)
- [X] T017 Implement `src-tauri/src/services/embedder.rs` — fastembed-rs wrapper: load nomic-embed-text-v1.5 (768d) if RAM ≥ 12 GB else all-MiniLM-L6-v2 (384d); expose `embed(texts: Vec<String>) → Vec<Vec<f32>>`; handle first-run model download with fallback to bundled MiniLM
- [X] T018 Implement `src-tauri/src/services/chunker.rs` — 512-token semantic boundary chunking: split on paragraph → sentence → word boundaries; 64-token overlap; return `Vec<Chunk>` with char_start, char_end, token_count, page_number
- [X] T019 Implement `src-tauri/src/services/llm.rs` — `ModelSlot` struct with `Arc<Mutex<Option<LoadedModel>>>` in Tauri managed state; load GGUF via llama-cpp-2; 90s Tokio idle timer (reset on each call); `evict()` drops current model; `infer(messages, tools, grammar) → String`; separate `VisionSlot` for Qwen2-VL
- [X] T020 Implement `src-tauri/src/services/task_budget.rs` — `TaskBudget` with three states (Active/Throttled/Idle); sysinfo polling every 5s (CPU%, battery%); Tokio `mpsc` priority channel (0=user-interactive, 1=background-normal, 2=background-low); `submit(priority, task)` API; Tauri event listener for user interaction → reset Active timer
- [X] T021 Implement `src-tauri/src/services/file_watcher.rs` — notify crate v6 watcher with 500ms debounce; watch registered folder paths; emit `FileChanged { path, kind }` events to a Tokio channel consumed by the indexer
- [X] T022 Wire `src-tauri/src/main.rs` + `src-tauri/src/lib.rs` — Tauri builder with managed state (SqlitePool, ModelSlot, VisionSlot, EmbedderState, TaskBudget, FileWatcher, HardwareTier); register all command stubs (empty returns); confirm app builds with `cargo tauri build --debug`

**Checkpoint**: App builds, DB migrations run on launch, hardware tier detected and logged

---

## Phase 3: User Story 1 — Create Notebook and Chat with Documents (P1) 🎯 MVP

**Goal**: User creates a notebook, adds local files, asks questions, receives cited answers from those files.
**Independent Test**: Create notebook → add 3 PDFs → ask cross-document question → verify cited answer appears within 60s of adding files.

### Implementation for User Story 1

- [X] T023 [P] [US1] Implement `src-tauri/src/services/parsers/pdf.rs` — lopdf text extraction; detect scanned PDF (< 100 chars/page avg → route to vision); return raw text with page offsets
- [X] T024 [P] [US1] Implement `src-tauri/src/services/parsers/document.rs` — docx-rs for DOCX (paragraph text + table cells); calamine for XLSX/XLSM (cell values as TSV); Office Open XML ZIP parsing for PPTX (slide text extraction)
- [X] T025 [P] [US1] Implement `src-tauri/src/services/parsers/text.rs` — plain TXT/MD (direct string); code files via tree-sitter (syntax-aware chunking preserving function/class boundaries); enforce 500 MB file size limit, return error above 2 GB
- [X] T026 [P] [US1] Implement `src-tauri/src/services/parsers/audio_video.rs` — whisper-rs transcription for MP3/MP4; enforce transcription-only for 500 MB–2 GB media (transcript indexed, raw file not archived); return transcript text + segment timestamps
- [X] T027 [P] [US1] Implement `src-tauri/src/services/parsers/image.rs` — load Qwen2-VL 2B via VisionSlot for image description; on 8 GB hardware (vision unavailable) return `CapabilityNotice { message: "Image analysis requires 16 GB RAM" }` and mark source status as `failed` with that reason
- [X] T028 [US1] Implement `src-tauri/src/services/indexer.rs` — full ingestion pipeline: detect format → route to parser (T023–T027) → chunk (T018) → embed (T017) → bulk_insert chunks → FTS5 sync triggers fire automatically → upsert vec0 embeddings → update source status `indexed`; submit via TaskBudget (priority 1); handle parser errors → set status `failed` with error_reason
- [X] T029 [US1] Implement `src-tauri/src/services/query_router.rs` — 5-tier routing: (0) cosine ≥ 0.92 vs query_cache embeddings → return cached; (1) entity-pattern regex → FTS5 SQL; (2) semantic → sqlite-vec ANN; (3) Tier 3 orchestrator LLM with GBNF-constrained output; (4) Tier 4 worker LLM; quality notice injected when Tier 4 request handled by Tier 3 on 8 GB hardware
- [X] T030 [P] [US1] Implement `src-tauri/src/commands/notebooks.rs` — thin `#[tauri::command]` handlers calling repositories: `create_notebook`, `list_notebooks`, `get_notebook`, `rename_notebook`, `delete_notebook`; write audit_log entry on delete
- [X] T031 [P] [US1] Implement `src-tauri/src/commands/sources.rs` — `add_sources` (dedup by file_path, queue new files as pending, fire indexer via TaskBudget), `remove_source` (unlink from notebook, cascade-delete if no other refs), `list_notebook_sources`, `retry_failed_source` (reset status to pending, re-queue)
- [X] T032 [US1] Implement `src-tauri/src/commands/query.rs` — `create_session` (new QuerySession for notebook), `query_notebook` (call query_router, persist user+assistant messages with citations and tier_used, write query audit entry, return QueryResponse with quality_notice), `get_session_history`
- [X] T033 [P] [US1] Implement `src-tauri/src/commands/system.rs` — `get_hardware_tier` returns HardwareTier from managed state
- [X] T034 [US1] Register all P1 commands in `src-tauri/src/lib.rs` invoke_handler: create_notebook, list_notebooks, get_notebook, rename_notebook, delete_notebook, add_sources, remove_source, list_notebook_sources, retry_failed_source, create_session, query_notebook, get_session_history, get_hardware_tier
- [X] T035 [P] [US1] Implement `src/api/index.ts` — typed `invoke<T>()` wrappers for all P1 Tauri commands matching contracts/tauri-commands.ts interfaces
- [X] T036 [P] [US1] Implement `src/stores/notebooks.ts` — `createStore` for notebook list, selected notebook ID, and source list; actions: loadNotebooks, selectNotebook, createNotebook, deleteNotebook
- [X] T037 [P] [US1] Implement `src/stores/query.ts` — `createStore` for current session ID, messages array, isLoading flag; actions: startSession, sendQuery, appendMessage
- [X] T038 [P] [US1] Implement `src/stores/system.ts` — `createSignal<HardwareTier>` populated on app load via `get_hardware_tier`
- [X] T039 [P] [US1] Implement `src/components/notebook/SourceStatusBadge.tsx` — coloured badge for pending (grey), processing (blue spinner), indexed (green), failed (red + error tooltip + retry button)
- [X] T040 [P] [US1] Implement `src/components/notebook/SourceList.tsx` — drag-and-drop zone (Tauri dialog fallback), file list with SourceStatusBadge per item, remove action per item
- [X] T041 [P] [US1] Implement `src/components/chat/CitationChip.tsx` — clickable chip showing source name + page; emits highlight event with chunk_id to parent
- [X] T042 [P] [US1] Implement `src/components/chat/QualityNoticeBanner.tsx` — yellow banner displayed when `qualityNotice.constrained === true`; shows model tier notice message; dismissible per session
- [X] T043 [P] [US1] Implement `src/components/chat/MessageBubble.tsx` — user (right-aligned) and assistant (left-aligned) bubbles; assistant bubble renders CitationChips inline; QualityNoticeBanner shown above answer when notice present
- [X] T044 [US1] Implement `src/components/chat/ChatPanel.tsx` — text input with send button, message list (auto-scroll), session loading state, "no content found" empty state; depends on T041, T042, T043
- [X] T045 [US1] Implement `src/components/notebook/NotebookView.tsx` — two-column layout: SourceList (left) + ChatPanel (right); source passage highlight handler (scroll to chunk + highlight text); depends on T040, T044
- [X] T046 [US1] Implement `src/pages/Dashboard.tsx` — sidebar NotebookList + main NotebookView; new notebook button; depends on T045
- [X] T047 [US1] Implement `src/App.tsx` — `@solidjs/router` routes: `/` → Dashboard, `/search` → SearchPage (stub), `/settings` → SettingsPage (stub); HardwareTier loaded on mount
- [X] T048 [US1] End-to-end validation per `quickstart.md` scenario P1: create notebook, add 50-page PDF, wait ≤ 60s for `indexed` status, ask question, verify inline citation with page number; confirm zero network calls via system network monitor — VERIFIED 2026-04-17: constitution.pdf indexed, "definition of state" returns "Article 7 ... [constitution.pdf, page 17]" with page-17 references; 3 queries over 2394s watched by monitor-dev.sh — 449 offline ticks / 0 online ticks (log: logs/monitoring-states-20260417-205551.log)

**Checkpoint**: P1 fully functional — notebook + cited Q&A working independently on minimum hardware

---

## Phase 4: User Story 2 — Semantic Folder Indexing and Global Recall (P2)

**Goal**: User indexes local folders in background; global semantic search across all indexed content.
**Independent Test**: Add folder with 50+ files → semantic search for concept not literally present → results with file path + excerpt within 3s; revoke folder → content gone within 5s.

### Implementation for User Story 2

- [X] T049 [P] [US2] Implement `src-tauri/src/db/repositories/folders.rs` — insert, list, get_by_path, update_status, update_file_count, update_last_scanned, delete (cascade removes all source refs from that folder path prefix)
- [X] T050 [P] [US2] Implement `src-tauri/src/commands/folders.rs` — `add_indexed_folder` (validate path exists, insert, queue folder scan via TaskBudget), `list_indexed_folders`, `revoke_indexed_folder` (delete folder record + all sources/chunks/embeddings/entities derived from folder path within single SQLite transaction, write audit entry, target ≤ 5s per SC-009)
- [X] T051 [US2] Implement `src-tauri/src/commands/search.rs` — `global_search`: embed query → sqlite-vec ANN across ALL chunks (not notebook-scoped); deduplicate by source_id; return top-20 SearchResults with excerpt, file path, last_modified, relevance score
- [X] T052 [US2] Wire `file_watcher.rs` into indexed folder pipeline in `src-tauri/src/services/indexer.rs` — on `FileChanged` event: if file in indexed folder → upsert source (dedup by path) → queue re-index; on file delete → set source status `failed` with reason "file deleted"
- [X] T053 [P] [US2] Extend `src/api/index.ts` — add typed wrappers for: add_indexed_folder, list_indexed_folders, revoke_indexed_folder, global_search
- [X] T054 [P] [US2] Implement `src/components/search/GlobalSearchBar.tsx` — debounced (300ms) search input at top of app; calls global_search; keyboard shortcut trigger (Ctrl/Cmd+K)
- [X] T055 [P] [US2] Implement `src/components/search/SearchResultCard.tsx` — file name, path, last modified date, excerpt with query terms highlighted; click opens file via Tauri shell
- [X] T056 [US2] Implement `src/pages/SearchPage.tsx` — GlobalSearchBar + SearchResultCard list; empty state ("No indexed folders yet — add one in Settings"); depends on T054, T055
- [X] T057 [US2] Add Indexed Folders panel to `src/pages/SettingsPage.tsx` — list of IndexedFolders with status badges, add-folder button (Tauri dialog), revoke action with confirmation prompt
- [X] T058 [US2] Update `src/App.tsx` to route `/search` → SearchPage (T056) and render GlobalSearchBar in top navigation
- [X] T059 [US2] End-to-end validation per `quickstart.md` scenario P2: add folder, semantic search conceptual phrase, verify results surface + no fan/CPU spike; revoke folder, confirm content removed within 5s — VERIFIED 2026-04-19: global search returns relevant semantic hits; folder revocation clears indexed content

**Checkpoint**: P1 + P2 independently functional — global recall works without any notebook

---

## Phase 5: User Story 3 — Multi-Source Deep Analysis (P3)

**Goal**: One-click analysis templates across notebook sources with full citations.
**Independent Test**: Notebook with 3 reports → run "Compare Sources" → output table with per-cell citations; run on 8 GB hardware → quality notice shown, template not disabled.

### Implementation for User Story 3

- [X] T060 [US3] Implement `src-tauri/src/services/analysis_engine.rs` — template runner for all 5 templates (compare_sources, extract_action_items, find_contradictions, generate_timeline, summarize_all); each template: retrieve relevant chunks → build GBNF-constrained prompt → route to best available model tier → parse structured JSON output → map to AnalysisResult with citations; inject QualityNotice when Tier 3 used instead of Tier 4
- [X] T061 [P] [US3] Implement `src-tauri/src/services/exporters/pdf.rs` — render AnalysisResult (title + body text + citation footnotes) to PDF using a pure-Rust PDF layout crate (e.g., `printpdf`)
- [X] T062 [P] [US3] Implement `src-tauri/src/services/exporters/txt.rs` — render AnalysisResult to plain-text with citation markers in brackets
- [X] T063 [P] [US3] Implement `src-tauri/src/commands/analysis.rs` — `run_analysis` (dispatch to analysis_engine, persist result), `export_analysis` (call exporter by format, write to destinationPath, write audit entry)
- [X] T064 [P] [US3] Implement `src/components/analysis/TemplateSelector.tsx` — 5 template cards with icon, name, description; click triggers run_analysis; loading state per template
- [X] T065 [P] [US3] Implement `src/components/analysis/AnalysisResultView.tsx` — rendered output with citation chips inline (same CitationChip from P1); click-to-highlight source passage; export button (PDF/TXT format picker); QualityNoticeBanner if constrained
- [X] T066 [US3] Add analysis toolbar to `src/components/notebook/NotebookView.tsx` — "Analyze" button opens TemplateSelector modal; result panel below chat area; depends on T064, T065
- [X] T067 [US3] Regist
er analysis commands in `src-tauri/src/lib.rs`: run_analysis, export_analysis
- [X] T068 [US3] End-to-end validation: add 3 quarterly reports → "Compare Sources" → verify cited table, click citation → source highlights; export to PDF; confirm quality notice on 8 GB simulation (ECHO_FORCE_TIER=3) — VERIFIED 2026-04-19: Compare Sources produced SIMILARITIES/DIFFERENCES/CONFLICTS across Q1-Q3 reports; PDF exported to Source_Comparison.pdf (7621 bytes); quality notice banner shown; model path bug fixed (analysis_engine now uses resolve_model_path)

**Checkpoint**: P1 + P2 + P3 all independently functional

---

## Phase 6: User Story 4 — Knowledge Graph and Structured Entity Queries (P4)

**Goal**: Auto-extracted entities from indexed content; sub-second structured queries; interactive graph view.
**Independent Test**: Index 20 meeting notes → query "decisions about product launch" → results in < 1s with source + date; open graph view → entities and relationships visible.

### Implementation for User Story 4

- [X] T069 [US4] Implement `src-tauri/src/services/entity_extractor.rs` — Pass 1 (synchronous, during indexing): regex patterns for ISO dates, dd/mm/yyyy, monetary amounts ($X,XXX), email addresses → upsert entities; Pass 2 (async background, post-indexing): per-chunk LLM prompt with GBNF grammar `{"entities":[{"type":..,"value":..}]}` for people/orgs/projects/decisions → upsert entities + insert entity_mentions; submit Pass 2 via TaskBudget priority 2
- [X] T070 [US4] Wire entity extraction into `src-tauri/src/services/indexer.rs` — after chunks stored (status → indexed), enqueue entity extraction Pass 2 for all new chunks via TaskBudget; Pass 1 runs synchronously inside the indexer pipeline
- [X] T071 [P] [US4] Implement `src-tauri/src/services/knowledge_graph.rs` — entity query builder (natural-language → SQL: parses entity type + value filters + date range); relationship traversal (BFS up to 2 hops); subgraph fetch (paginated, 500-entity cap)
- [X] T072 [P] [US4] Implement `src-tauri/src/commands/entities.rs` — `query_entities` (routes to knowledge_graph SQL builder, < 1s target), `get_entity_relationships`, `get_entity_sources`, `get_knowledge_graph` (paginated subgraph)
- [X] T073 [P] [US4] Extend `src/api/index.ts` — typed wrappers for: query_entities, get_entity_relationships, get_entity_sources, get_knowledge_graph
- [X] T074 [P] [US4] Implement `src/components/knowledge-graph/EntityPanel.tsx` — entity detail panel: type badge, canonical value, source count, list of source documents with excerpts; rendered in sidebar when entity clicked in graph
- [X] T075 [US4] Implement `src/components/knowledge-graph/GraphCanvas.tsx` — force-directed graph using `@xyflow/react`; nodes = entities (coloured by type); edges = relationships with type label; 500-node cap with type-filter dropdown; click node → EntityPanel; pan + zoom
- [X] T076 [US4] Add Knowledge Graph tab to `src/pages/NotebookPage.tsx` (or Dashboard) — entity query input (natural language) + results list + GraphCanvas; register entity commands in `src-tauri/src/lib.rs`
- [X] T077 [US4] End-to-end validation: index meeting notes folder → query "decisions about product launch" → measure response time (target < 1s) → confirm SQL-only path (no LLM invoked); open graph → verify entities + relationships rendered — VERIFIED 2026-04-19: entity queries render in graph with radial layout + dynamic relation-type preferences shipped in Session 14

**Checkpoint**: P1–P4 all independently functional; knowledge graph navigable

---

## Phase 7: User Story 5 — Batch Processing on File Collections (P5)

**Goal**: Background folder-level processing operations with per-file results and system notification on completion.
**Independent Test**: Point at folder with 20 PDFs → run "Summarize All" → processing runs in background → notification on completion → one entry per file in results.

### Implementation for User Story 5

- [X] T078 [US5] Implement `src-tauri/src/services/batch_processor.rs` — batch executor: scan folder → build per-file job queue → submit each file as TaskBudget priority 2 task → run operation (summarize/extract_dates/flag_keywords/extract_actions) via query_router → update batch_results per file (status, output_text or error_reason) → update batch_jobs counters; on failure for individual file → flag + continue; emit Tauri event `batch:progress` per file; on completion → fire system notification
- [X] T079 [P] [US5] Implement `src-tauri/src/commands/batch.rs` — `create_batch_job` (validate folder exists, insert BatchJob, queue in batch_processor), `get_batch_job` (returns current status + counts), `get_batch_results` (paginated per-file results)
- [X] T080 [P] [US5] Extend `src/api/index.ts` — typed wrappers for: create_batch_job, get_batch_job, get_batch_results
- [X] T081 [P] [US5] Implement `src/components/batch/BatchPanel.tsx` — folder path picker (Tauri dialog), operation selector dropdown (Summarize/Extract Dates/Flag Keywords/Extract Actions), optional keyword input for flag operation, launch button, active job progress bar (subscribes to `batch:progress` Tauri event)
- [X] T082 [P] [US5] Implement `src/components/batch/BatchResultTable.tsx` — sortable table of batch results: file name, status (success/failed), output preview, error reason on failed; click row opens source file via Tauri shell
- [X] T083 [US5] Add Batch Processing section to `src/pages/Dashboard.tsx` — BatchPanel for launching + active job status; BatchResultTable for viewing completed job results; register batch commands in `src-tauri/src/lib.rs`
- [X] T084 [US5] Wire Tauri notification plugin in `src-tauri/src/lib.rs` — emit system tray notification on batch job completion with job summary (N files processed, M failed)
- [X] T085 [US5] End-to-end validation: 20 PDFs → "Summarize All" → navigate away → receive notification → verify one result row per file, failed files flagged individually, all others complete — VERIFIED 2026-04-19: 20/20 files summarised successfully; system notification fired on completion; FK-787 stub-notebook bug fixed; batch results UX improved (multi-expand, Expand All, search filter, Export All, fullscreen toggle)

**Checkpoint**: All 5 user stories independently functional — full App 1 feature-complete

---

## Phase N: Polish & Cross-Cutting Concerns

**Purpose**: Audit, hardening, and validation across all stories

- [X] T086 [P] Audit all `src-tauri/src/commands/*.rs` for unhandled errors — replace any `unwrap()` / `expect()` with proper `Result<T, String>` returns mapped to user-facing error messages
- [X] T087 [P] Implement `src-tauri/src/commands/audit.rs` — `get_audit_log` (paginated); add audit log view to `src/pages/SettingsPage.tsx` (event type, timestamp, details)
- [X] T088 [P] Implement notebook export/import in `src-tauri/src/services/archive.rs` — ZIP builder with manifest.json + sources/ copy + notebook-scoped db.sqlite export; import: unzip to temp, read manifest, check schema version, merge or re-index; wire commands in `src-tauri/src/commands/archive.rs` and register in `src-tauri/src/lib.rs`
- [X] T089 Add low-storage guard to `src-tauri/src/services/task_budget.rs` — before each indexing/batch work item executes, check `sysinfo::Disks` for < 1 GB free on DB partition → pause + emit `storage:low` Tauri event; show banner in `src/App.tsx`
- [X] T090 [P] Performance profiling: run sqlite-vec ANN query against 10,000-chunk corpus; measure p95 latency; if > 2s adjust `limit` in vec0 query or enable HNSW when available; document result in `specs/001-notebook-knowledge-engine/research.md`
- [ ] T091 [P] Run all `quickstart.md` manual validation scenarios P1–P4 in sequence on a clean database; fix any regressions found; confirm zero network calls via `nmap`/`tcpdump` during all operations
- [X] T092 Update `src-tauri/tests/integration/test_indexing_pipeline.rs` — integration test: add test PDF → assert source status transitions pending→processing→indexed; assert FTS5 hit on known keyword; assert sqlite-vec hit on semantic query
- [X] T093 Update `src-tauri/tests/integration/test_query_router.rs` — integration test: pre-populate cache with known Q→A; assert Tier 0 hit; pre-populate FTS5; assert Tier 1 hit without embedder; assert Tier 2 hit; assert refusal on out-of-scope query (SC-003)

### Hybrid Retrieval & Grounding (FR-039–FR-042)

- [X] T094 [P] Implement RRF merging in `src-tauri/src/services/query_router.rs` — add `fn rrf_merge(fts_results: Vec<ChunkId>, vec_results: Vec<(ChunkId, f32)>, k: u32) -> Vec<(ChunkId, f32)>` using RRF formula `score = Σ 1/(60 + rank_i)`; called in the routing path when both Tier 1 and Tier 2 produce results; add unit test asserting correct merge order for a known pair of ranked lists (FR-039)

- [X] T095 [P] Implement context builder in `src-tauri/src/services/query_router.rs` — `fn build_context(ranked: Vec<(ChunkId, f32)>, pool: &SqlitePool, max_tokens: usize) -> Vec<ContextChunk>` fetching chunk text + metadata, deduplicating by chunk_id, enforcing `max_tokens` budget (model context window − 512), stopping at K=8; replace inline chunk-fetching in Tier 3/4 escalation path with this function; add unit test asserting dedup and token-cap behaviour (FR-040)

- [X] T096 [P] Define versioned grounding system prompt in `src-tauri/src/services/llm.rs` — add `const GROUNDING_SYSTEM_PROMPT: &str` with exact 4-rule template from AD-7; wire into all `infer()` calls in query_router (remove any ad-hoc prompt strings); set `temperature = 0.1` in LLM call options for all grounding queries; add constant to audit_log on each LLM invocation (FR-041)

- [X] T097 Implement grounding validator in `src-tauri/src/services/query_router.rs` — `fn validate_grounding(response: &str, context_chunks: &[ContextChunk]) -> GroundingResult`; split into sentences, detect factual sentences (non-question, > 5 words), check citation markers `[<source>, page N]`, verify citations reference context sources, replace response with fallback string if uncited claims exist; log `GroundingResult` to audit_log; target < 50 ms (pure string ops); add unit tests: grounded response passes, uncited response triggers fallback, citation to non-context source triggers fallback (FR-042)

---

## Dependencies & Execution Order

### Phase Dependencies

- **Setup (Phase 1)**: No dependencies — start immediately
- **Foundational (Phase 2)**: Depends on Setup (Phase 1) — **BLOCKS all user stories**
- **US1 (Phase 3)**: Depends on Foundational (Phase 2) — first story to implement
- **US2 (Phase 4)**: Depends on Foundational + US1 indexer (T028) — shares indexer pipeline
- **US3 (Phase 5)**: Depends on Foundational + US1 query_router (T029) — templates use same query engine
- **US4 (Phase 6)**: Depends on Foundational + US1 indexer (T028) — entity extraction wires into indexer
- **US5 (Phase 7)**: Depends on Foundational + US1 indexer (T028) + US1 query_router (T029)
- **Polish (Phase N)**: Depends on all user stories complete

### User Story Internal Dependencies

**US1**:
- Parsers (T023–T027) → Indexer (T028) → must come before query (T029)
- Repositories (T010–T016, Phase 2) → Commands (T030–T034) → API wrappers (T035)
- Components: SourceStatusBadge (T039) → SourceList (T040); CitationChip (T041) + QualityNoticeBanner (T042) + MessageBubble (T043) → ChatPanel (T044) → NotebookView (T045) → Dashboard (T046) → App.tsx (T047)

**US2**:
- Folders repository (T049) → Folders commands (T050) → Search command (T051)
- FileWatcher integration (T052) → depends on T028 (indexer) from US1

**US3**:
- Analysis engine (T060) → depends on T029 (query_router) from US1
- Exporters (T061, T062) → Analysis commands (T063) → Analysis components (T064–T065) → NotebookView wiring (T066)

**US4**:
- Entity extractor (T069) → indexer wiring (T070) → knowledge graph service (T071) → commands (T072)
- Components: EntityPanel (T074) → GraphCanvas (T075) → page integration (T076)

**US5**:
- Batch processor (T078) → depends on T028 (indexer) + T029 (query_router)
- Commands (T079) → components (T081, T082) → Dashboard wiring (T083)

### Parallel Opportunities Within Each Story

```
# US1 parallel execution:
Parsers (T023, T024, T025, T026, T027) — all run in parallel
Repositories (T010–T016) — all run in parallel (Phase 2)
Commands (T030, T031, T033) — run in parallel
Stores (T036, T037, T038) — run in parallel
Components (T039, T040, T041, T042, T043) — run in parallel

# US2 parallel execution:
T049, T050, T051 — run in parallel
T054, T055 — run in parallel

# US3 parallel execution:
T061, T062, T063 — run in parallel after T060
T064, T065 — run in parallel

# US4 parallel execution:
T071, T072, T073 — run in parallel after T070
T074 — runs in parallel with T075

# US5 parallel execution:
T079, T080 — run in parallel after T078
T081, T082 — run in parallel
```

---

## Implementation Strategy

### MVP First (User Story 1 Only — Shippable)

1. Complete Phase 1 (Setup)
2. Complete Phase 2 (Foundational) — **BLOCKING**
3. Complete Phase 3 (US1) — T023 through T048
4. **STOP AND VALIDATE**: Run quickstart.md scenario P1 end-to-end
5. Ship: functional notebook + cited Q&A on all hardware tiers ✅

### Incremental Delivery

| Increment | Adds | Validates |
|-----------|------|-----------|
| MVP | US1: Notebook + Chat | P1 quickstart scenario |
| +US2 | Global Recall + Folder Indexing | P2 + revocation test |
| +US3 | Analysis Templates | Compare 3 reports, export |
| +US4 | Knowledge Graph | Sub-second entity query |
| +US5 | Batch Processing | 20-file batch + notification |

Each increment is independently deployable and testable before the next begins.

---

## Notes

- `[P]` = different files, no incomplete dependencies — safe to parallelise
- Each `[USN]` task maps to exactly one user story for traceability
- Commit after each task or logical group; run `cargo test` after each Phase
- Validate each story against its quickstart.md scenario before moving to next
- `ECHO_FORCE_TIER=3` in `.env` simulates 8 GB hardware for constraint testing
- Constitution Check: all 8 principles verified in `plan.md` before these tasks run
