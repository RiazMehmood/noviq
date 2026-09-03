# EchoAI — Complete File Path Reference

> Every source file in the project, grouped by layer, with its exact path and one-line role.

---

## Table of Contents

1. [Frontend — Entry Points](#1-frontend--entry-points)
2. [Frontend — Pages](#2-frontend--pages)
3. [Frontend — Stores](#3-frontend--stores)
4. [Frontend — API Layer](#4-frontend--api-layer)
5. [Frontend — Components: Chat](#5-frontend--components-chat)
6. [Frontend — Components: Notebook](#6-frontend--components-notebook)
7. [Frontend — Components: Knowledge Graph](#7-frontend--components-knowledge-graph)
8. [Frontend — Components: Analysis](#8-frontend--components-analysis)
9. [Frontend — Components: Batch](#9-frontend--components-batch)
10. [Frontend — Components: Meeting](#10-frontend--components-meeting)
11. [Frontend — Components: Search](#11-frontend--components-search)
12. [Frontend — Components: Overlay](#12-frontend--components-overlay)
13. [Frontend — Components: Shared/Global](#13-frontend--components-sharedglobal)
14. [Backend — App Entry & Wiring](#14-backend--app-entry--wiring)
15. [Backend — Commands Layer](#15-backend--commands-layer)
16. [Backend — Services: Core Pipeline](#16-backend--services-core-pipeline)
17. [Backend — Services: LLM & Embeddings](#17-backend--services-llm--embeddings)
18. [Backend — Services: Scheduler & Budget](#18-backend--services-scheduler--budget)
19. [Backend — Services: File Watcher](#19-backend--services-file-watcher)
20. [Backend — Services: Parsers](#20-backend--services-parsers)
21. [Backend — Services: Exporters](#21-backend--services-exporters)
22. [Backend — Services: Meeting Agent](#22-backend--services-meeting-agent)
23. [Backend — Services: Agent (Overlay)](#23-backend--services-agent-overlay)
24. [Backend — Database: Schema & Init](#24-backend--database-schema--init)
25. [Backend — Database: Migrations](#25-backend--database-migrations)
26. [Backend — Database: Repositories](#26-backend--database-repositories)
27. [Backend — Integration Tests](#27-backend--integration-tests)
28. [Config & Personas](#28-config--personas)
29. [Tauri Configuration](#29-tauri-configuration)

---

## 1. Frontend — Entry Points

| Path | Role |
|---|---|
| `src/index.tsx` | Root mount — renders `<App/>` into `#root` DOM node |
| `src/App.tsx` | Router setup — maps URL paths to page components |

---

## 2. Frontend — Pages

Top-level page components, each corresponding to a route.

| Path | Route | Role |
|---|---|---|
| `src/pages/Dashboard.tsx` | `/` | Main layout: notebook sidebar + notebook view + batch panel |
| `src/pages/KnowledgeGraphPage.tsx` | `/kg` | Full-page interactive knowledge graph canvas |
| `src/pages/SearchPage.tsx` | `/search` | Global full-text search across all indexed sources |
| `src/pages/SettingsPage.tsx` | `/settings` | Hardware info, indexed folders, GPU, cloud, audit log, meetings |
| `src/pages/Overlay.tsx` | `/overlay` | Floating overlay window (agent task overlay, minimal) |

---

## 3. Frontend — Stores

SolidJS reactive state stores — shared global state across components.

| Path | State it holds |
|---|---|
| `src/stores/query.ts` | Chat session ID, message history, streaming text, thinking text, cancel state, quality notice |
| `src/stores/notebooks.ts` | Notebook list, active notebook ID |
| `src/stores/analysis.ts` | Selected analysis template, analysis result |
| `src/stores/batch.ts` | Active batch job, refresh key, fullscreen flag |
| `src/stores/scheduler.ts` | Active workload list (for `WorkloadBar` progress display) |
| `src/stores/system.ts` | Hardware tier info, GPU settings, cloud (Groq) settings |

---

## 4. Frontend — API Layer

| Path | Role |
|---|---|
| `src/api/index.ts` | All typed `invoke()` wrappers for every Tauri command; all TypeScript interface definitions (Notebook, Source, Message, Citation, etc.) |

---

## 5. Frontend — Components: Chat

Located in `src/components/chat/`

| Path | Role |
|---|---|
| `src/components/chat/ChatPanel.tsx` | Main chat UI: message list, streaming display, input box, send/cancel buttons |
| `src/components/chat/ChatScopePicker.tsx` | Scope selector — chat against Notebook / Folder / Both |
| `src/components/chat/MessageBubble.tsx` | Renders a single chat message with markdown and inline citation chips |
| `src/components/chat/CitationChip.tsx` | Inline badge showing source filename + page number from LLM citation |
| `src/components/chat/QualityNoticeBanner.tsx` | Warning banner shown when answer was quality-constrained (small model / grounding limit) |

---

## 6. Frontend — Components: Notebook

Located in `src/components/notebook/`

| Path | Role |
|---|---|
| `src/components/notebook/NotebookList.tsx` | Sidebar list of all notebooks; create / rename / delete actions |
| `src/components/notebook/NotebookView.tsx` | Tab container for a selected notebook — holds SourceList + ChatPanel + Analysis tabs |
| `src/components/notebook/SourceList.tsx` | File drop target; shows all sources with indexing progress bars; add / remove / retry |
| `src/components/notebook/SourceStatusBadge.tsx` | Color-coded pill: `pending` / `processing` / `indexed` / `failed` |

---

## 7. Frontend — Components: Knowledge Graph

Located in `src/components/knowledge-graph/`

| Path | Role |
|---|---|
| `src/components/knowledge-graph/GraphCanvas.tsx` | Interactive KG canvas built on `@xyflow/react` — nodes (entities) + edges (relationships) with radial layout |
| `src/components/knowledge-graph/RelationTypesPanel.tsx` | Sidebar panel to filter which relationship types are displayed in the graph |
| `src/components/knowledge-graph/EntityPanel.tsx` | Detail panel shown when a node is clicked — entity info + linked sources |

---

## 8. Frontend — Components: Analysis

Located in `src/components/analysis/`

| Path | Role |
|---|---|
| `src/components/analysis/TemplateSelector.tsx` | Buttons to launch one of 5 analysis templates (compare, actions, contradictions, timeline, summarize) |
| `src/components/analysis/AnalysisResultView.tsx` | Displays structured analysis output with citations and export button |

---

## 9. Frontend — Components: Batch

Located in `src/components/batch/`

| Path | Role |
|---|---|
| `src/components/batch/BatchPanel.tsx` | Folder picker + operation selector + launch button for batch processing jobs |
| `src/components/batch/BatchResultTable.tsx` | Per-file result table with status, output text, and CSV/PDF export |

---

## 10. Frontend — Components: Meeting

Located in `src/components/meeting/`

| Path | Role |
|---|---|
| `src/components/meeting/MeetingPanel.tsx` | Start / stop meeting recording; displays live transcript segment list |

---

## 11. Frontend — Components: Search

Located in `src/components/search/`

| Path | Role |
|---|---|
| `src/components/search/GlobalSearchBar.tsx` | Full-text search input with debounce; triggers FTS5 search via `global_search` command |
| `src/components/search/SearchResultCard.tsx` | Renders a single search result with source name, excerpt, and page number |

---

## 12. Frontend — Components: Overlay

Located in `src/components/overlay/`

| Path | Role |
|---|---|
| `src/components/overlay/OverlayWindow.tsx` | Floating overlay window container for agent task UI |
| `src/components/overlay/PaqApproval.tsx` | "Pause-and-query" approval dialog — user approves/rejects agent actions |
| `src/components/overlay/StepProgress.tsx` | Progress stepper showing agent task steps |
| `src/components/overlay/TaskInput.tsx` | Input for user to describe a task for the agent to execute |

---

## 13. Frontend — Components: Shared/Global

Located in `src/components/`

| Path | Role |
|---|---|
| `src/components/WorkloadBar.tsx` | Horizontal progress bar at top of screen showing active background workloads (indexing, NER, KG, batch) |
| `src/components/ModelDownloadBanner.tsx` | Banner prompting user to download the LLM model file with progress indicator |
| `src/components/NavBadge.tsx` | Navigation badge component (used in sidebar nav items) |

---

## 14. Backend — App Entry & Wiring

| Path | Role |
|---|---|
| `src-tauri/src/main.rs` | Binary entry point — calls `echo_ai_lib::run()` |
| `src-tauri/src/lib.rs` | Full app wiring: startup sequence, all 50+ command registrations, managed state setup, file watcher event loop, meeting agent init |

---

## 15. Backend — Commands Layer

Located in `src-tauri/src/commands/`
Thin Tauri command handlers — validate input, delegate to services/repositories.

| Path | Commands it exposes |
|---|---|
| `src-tauri/src/commands/mod.rs` | Re-exports all command modules |
| `src-tauri/src/commands/notebooks.rs` | `create_notebook`, `list_notebooks`, `get_notebook`, `rename_notebook`, `delete_notebook` |
| `src-tauri/src/commands/sources.rs` | `add_sources`, `list_notebook_sources`, `remove_source`, `retry_failed_source`, `get_kg_status`, `clean_orphan_sources`, `resume_pending_ner`, `list_sources_in_folder` |
| `src-tauri/src/commands/query.rs` | `create_session`, `stream_query_notebook`, `cancel_chat`, `save_partial_message`, `get_session_history` |
| `src-tauri/src/commands/system.rs` | `get_hardware_tier`, `check_model`, `download_model`, `restart_app`, `get_gpu_settings`, `set_gpu_enabled`, `get_cloud_settings`, `set_cloud_settings`, `open_query_logs_dir`, `read_latest_query_trace`, `download_query_trace_to` |
| `src-tauri/src/commands/folders.rs` | `add_indexed_folder`, `list_indexed_folders`, `revoke_indexed_folder`, `list_folder_sources`, `retry_folder_source`, `remove_folder_source` |
| `src-tauri/src/commands/search.rs` | `global_search` |
| `src-tauri/src/commands/entities.rs` | `query_entities`, `get_entity_relationships`, `get_entity_sources`, `get_knowledge_graph` |
| `src-tauri/src/commands/kg_preferences.rs` | `get_kg_preferences`, `set_kg_preferences` |
| `src-tauri/src/commands/analysis.rs` | `run_analysis`, `analyze_selected`, `export_analysis` |
| `src-tauri/src/commands/batch.rs` | `create_batch_job`, `get_batch_job`, `get_batch_results`, `export_batch_results`, `pause_batch_job`, `resume_batch_job`, `cancel_batch_job` |
| `src-tauri/src/commands/meetings.rs` | `start_meeting`, `stop_meeting`, `list_meeting_segments`, `get_meeting` |
| `src-tauri/src/commands/scheduler.rs` | `set_foreground`, `cancel_index_job`, `discard_index_job`, `list_active_index_jobs` |
| `src-tauri/src/commands/audit.rs` | `get_audit_log` |
| `src-tauri/src/commands/archive.rs` | `export_notebook`, `import_notebook` |
| `src-tauri/src/commands/trial.rs` | `get_trial_status` |

---

## 16. Backend — Services: Core Pipeline

Located in `src-tauri/src/services/`

| Path | Role |
|---|---|
| `src-tauri/src/services/mod.rs` | Re-exports all service modules |
| `src-tauri/src/services/indexer.rs` | **Main ingestion pipeline**: SHA-256 dedup → parse → chunk → embed → bulk insert → vec0 upsert → trigger NER. Semaphore-limited to 1 concurrent file. |
| `src-tauri/src/services/query_router.rs` | **5-tier RAG router**: semantic cache (T0) → entity regex (T1) → FTS5+ANN retrieval (T2) → LLM synthesis (T3) → large model (T4). Grounding validator post-processing. Streaming token emission. |
| `src-tauri/src/services/entity_extractor.rs` | **2-pass NER**: Pass 1 = regex for dates/amounts/article refs; Pass 2 = LLM GBNF JSON for person/org/concept/term/reference |
| `src-tauri/src/services/knowledge_graph.rs` | KG query helpers: entity co-occurrence within chunks → relationship inference; graph data assembly for frontend |
| `src-tauri/src/services/analysis_engine.rs` | 5 analysis templates (compare/actions/contradictions/timeline/summarize) with GBNF-constrained LLM output |
| `src-tauri/src/services/batch_processor.rs` | Folder batch processing (summarize/extract_dates/flag_keywords/extract_actions) with cooperative pause/resume |
| `src-tauri/src/services/chunker.rs` | Text → ≤512 token segments with sentence boundary respect; returns `Vec<Chunk>` with char offsets + page numbers |
| `src-tauri/src/services/archive.rs` | Notebook export to ZIP archive and import from ZIP |

---

## 17. Backend — Services: LLM & Embeddings

| Path | Role |
|---|---|
| `src-tauri/src/services/llm.rs` | `ModelSlot`: llama.cpp Rust FFI wrapper; 90-second idle model eviction; `infer_with_cancel()` (token-level cancellation); grounding system prompt constants (`GROUNDING_SYSTEM_PROMPT` v3.1.0) |
| `src-tauri/src/services/embedder.rs` | fastembed ONNX wrapper; `embed_one()` for single query embedding; `embed_batch()` for bulk chunk embedding; model selected by hardware tier |
| `src-tauri/src/services/cloud_llm.rs` | Groq API integration (OpenAI-compatible streaming); process-wide cached `CloudSettings`; emits same `chat://token` events as local path |
| `src-tauri/src/services/hardware_detector.rs` | Detect RAM + GPU (via `lspci`/`wmic`) → `HardwareTier` struct (tier 2-5, embedding model, VRAM, GPU vendor/name) |
| `src-tauri/src/services/priority_model_slot.rs` | Priority-aware model slot wrapper (ensures chat gets model over background NER) |
| `src-tauri/src/services/query_trace.rs` | Query telemetry logger: embed time, retrieval time, LLM time written to log files in app data dir |

---

## 18. Backend — Services: Scheduler & Budget

| Path | Role |
|---|---|
| `src-tauri/src/services/scheduler.rs` | Foreground-priority workload scheduler; `WorkloadHandle` (cooperative pause + cancel); progress pump emitting `workload://state` at ≤4 Hz; 6 workload kinds with priority levels |
| `src-tauri/src/services/task_budget.rs` | CPU/memory budget guard; limits total concurrent background work to avoid OOM on low-RAM machines |
| `src-tauri/src/services/scheduler_eta.rs` | ETA calculation for active workloads (used in `WorkloadBar` progress display) |

---

## 19. Backend — Services: File Watcher

| Path | Role |
|---|---|
| `src-tauri/src/services/file_watcher.rs` | `notify`-based filesystem watcher state; sends `FileChangeEvent` to channel; filters temp files and unsupported formats; 800ms debounce logic lives in the event loop in `lib.rs` |

---

## 20. Backend — Services: Parsers

Located in `src-tauri/src/services/parsers/`

| Path | File formats handled |
|---|---|
| `src-tauri/src/services/parsers/mod.rs` | `FileFormat` enum; `parse()` dispatch function; `ParseError` type |
| `src-tauri/src/services/parsers/pdf.rs` | PDF → text via `lopdf` (pure Rust); page-aware extraction |
| `src-tauri/src/services/parsers/document.rs` | DOCX → text via `docx-rs`; XLSX/XLSM → text via `calamine` |
| `src-tauri/src/services/parsers/text.rs` | TXT, MD, source code files → raw UTF-8 read |
| `src-tauri/src/services/parsers/image.rs` | PNG/JPG/JPEG/GIF/WebP/BMP → OCR stub (placeholder for future OCR) |
| `src-tauri/src/services/parsers/audio_video.rs` | MP3/MP4/audio/video → stub (placeholder for future transcription) |

---

## 21. Backend — Services: Exporters

Located in `src-tauri/src/services/exporters/`

| Path | Role |
|---|---|
| `src-tauri/src/services/exporters/mod.rs` | Exporter dispatch and shared types |
| `src-tauri/src/services/exporters/pdf.rs` | Render analysis result to PDF file using `printpdf` |
| `src-tauri/src/services/exporters/txt.rs` | Render analysis result to plain text file |

---

## 22. Backend — Services: Meeting Agent

Located in `src-tauri/src/services/meeting/`

| Path | Role |
|---|---|
| `src-tauri/src/services/meeting/mod.rs` | Module re-exports; `pub use service::MeetingService` |
| `src-tauri/src/services/meeting/service.rs` | `MeetingService` orchestrator: creates meeting DB record, drives audio capture + transcription lifecycle, start/stop API |
| `src-tauri/src/services/meeting/audio_capture.rs` | `cpal` mic capture → raw PCM frames channel |
| `src-tauri/src/services/meeting/segmenter.rs` | `webrtc-vad` voice activity detection → speech segments; `rubato` resample to 16kHz; `hound` encode to WAV bytes |
| `src-tauri/src/services/meeting/transcribe_worker.rs` | Consumes WAV segments; base64-encodes; sends to `SttSidecar` over stdio JSON; writes `meeting_segments` rows to DB |
| `src-tauri/src/services/stt_sidecar.rs` | Spawn + IPC with external `whisper-sidecar` binary; JSON protocol over stdin/stdout; workaround for ggml duplicate-symbol link conflict |

---

## 23. Backend — Services: Agent (Overlay)

Located in `src-tauri/src/agent/`
*(Reserved for future agent task overlay feature — currently partial implementation)*

| Path | Role |
|---|---|
| `src-tauri/src/agent/commands.rs` | Tauri commands for agent task management |
| `src-tauri/src/agent/paq.rs` | Pause-and-query: suspend agent, ask user for approval |
| `src-tauri/src/agent/persona.rs` | Load TOML persona configs from `config/personas/` |
| `src-tauri/src/agent/react_loop.rs` | ReAct agent loop: reason → act → observe cycle |
| `src-tauri/src/agent/rollback.rs` | Rollback support for agent actions |
| `src-tauri/src/agent/task_manager.rs` | Task state machine: queued → running → done / failed |
| `src-tauri/src/agent/tools/mod.rs` | Agent tool registry |
| `src-tauri/src/agent/tools/file_ops.rs` | File read/write tools available to the agent |
| `src-tauri/src/agent/voice/mod.rs` | Voice I/O pipeline stub for agent (VAD + Whisper + TTS) |

---

## 24. Backend — Database: Schema & Init

| Path | Role |
|---|---|
| `src-tauri/src/db/mod.rs` | Exports `init_db()` and repository modules |
| `src-tauri/src/db/schema.rs` | `init_db()`: open SQLite pool (WAL mode, 90s busy_timeout, 128MB cache, mmap 256MB), run migrations, detect embedding dimension, create `chunk_embeddings` vec0 virtual table |

---

## 25. Backend — Database: Migrations

Located in `src-tauri/src/db/migrations/`
Applied in order by `sqlx::migrate!` at startup.

| Path | What it adds |
|---|---|
| `src-tauri/src/db/migrations/001_initial_schema.sql` | All core tables: `db_config`, `notebooks`, `sources`, `notebook_sources`, `indexed_folders`, `chunks`, `chunks_fts` (FTS5), `query_cache`, `query_sessions`, `query_messages`, `entities`, `entity_mentions`, `entity_relationships`, `batch_jobs`, `batch_results`, `audit_log` |
| `src-tauri/src/db/migrations/002_schema_fixes.sql` | Format CHECK widening, additional indexes |
| `src-tauri/src/db/migrations/003_status_check_widen.sql` | Widens `indexing_status` CHECK constraint |
| `src-tauri/src/db/migrations/004_index_jobs.sql` | `index_jobs` table for crash recovery of in-flight folder scans |
| `src-tauri/src/db/migrations/005_incomplete_messages.sql` | `is_incomplete` column on `query_messages` for cancelled chat recovery |
| `src-tauri/src/db/migrations/006_agent_tables.sql` | Agent task overlay tables (placeholder) |
| `src-tauri/src/db/migrations/007_agent_support_tables.sql` | Additional agent support tables |
| `src-tauri/src/db/migrations/008_ner_progress.sql` | `ner_status` column on `sources` (`null` / `pending` / `done` / `failed`) |
| `src-tauri/src/db/migrations/009_universal_entity_types.sql` | Wider entity type CHECK (adds `concept`, `term`, `reference`, `location`) |
| `src-tauri/src/db/migrations/010_trial_meta.sql` | Trial mode metadata table |
| `src-tauri/src/db/migrations/011_gpu_settings.sql` | GPU enabled/disabled flag in `db_config` |
| `src-tauri/src/db/migrations/012_meetings.sql` | `meetings` + `meeting_segments` tables for Meeting Agent Phase 2 |

---

## 26. Backend — Database: Repositories

Located in `src-tauri/src/db/repositories/`
All SQL access is here — services never write raw SQL except through these.

| Path | Tables it accesses | Key functions |
|---|---|---|
| `src-tauri/src/db/repositories/mod.rs` | — | Re-exports all repository modules |
| `src-tauri/src/db/repositories/notebooks.rs` | `notebooks` | `create`, `list`, `get`, `rename`, `delete` |
| `src-tauri/src/db/repositories/sources.rs` | `sources`, `notebook_sources` | `insert`, `list_by_notebook`, `update_status`, `reset_stuck_pending`, `reset_stuck_processing`, `delete`, `get_by_hash` |
| `src-tauri/src/db/repositories/chunks.rs` | `chunks`, `chunk_embeddings` | `bulk_insert`, `upsert_embeddings`, `get_by_source`, `get_by_notebook`, `delete_by_source` |
| `src-tauri/src/db/repositories/entities.rs` | `entities`, `entity_mentions`, `entity_relationships` | `upsert`, `insert_mention`, `increment_source_count`, `list_by_type`, `get_relationships`, `get_sources` |
| `src-tauri/src/db/repositories/folders.rs` | `indexed_folders`, `notebooks` | `insert`, `list`, `revoke`, `backfill_stub_notebooks`, `get_by_path` |
| `src-tauri/src/db/repositories/sessions.rs` | `query_sessions`, `query_messages`, `query_cache` | `create_session`, `list_messages`, `insert_message`, `cache_lookup`, `cache_insert` |
| `src-tauri/src/db/repositories/batch_jobs.rs` | `batch_jobs`, `batch_results` | `create_job`, `update_job`, `insert_result`, `list_results`, `get_job` |
| `src-tauri/src/db/repositories/audit.rs` | `audit_log` | `insert_event`, `list_events` |
| `src-tauri/src/db/repositories/index_jobs.rs` | `index_jobs` | `create`, `update_status`, `reconcile_orphaned_running_jobs`, `list_active` |
| `src-tauri/src/db/repositories/meetings.rs` | `meetings`, `meeting_segments` | `create_meeting`, `update_meeting`, `insert_segment`, `list_segments`, `get_meeting` |

---

## 27. Backend — Integration Tests

| Path | What it tests |
|---|---|
| `src-tauri/tests/integration/test_indexing_pipeline.rs` | Full index pipeline: temp file → parse → chunk → embed → DB; verifies chunks and embeddings are stored correctly |
| `src-tauri/tests/integration/test_query_router.rs` | Query routing tiers: FTS5 path, semantic path, cache hit; mocked LLM responses |

---

## 28. Config & Personas

| Path | Role |
|---|---|
| `config/personas/agent-default.toml` | TOML persona config for the default agent: name, system prompt template, tool permissions, PAQ settings |

---

## 29. Tauri Configuration

| Path | Role |
|---|---|
| `src-tauri/tauri.conf.json` | App identifier, window config (title, size, min size), bundle settings, external binary (whisper sidecar) registration |
| `src-tauri/capabilities/default.json` | Tauri v2 capability grants: which plugins and IPC commands the frontend is allowed to call (fs, dialog, shell, notification, global-shortcut permissions) |
| `src-tauri/Cargo.toml` | Rust dependency manifest; feature flags (`llm`, `stt`, `cuda`, `vulkan`); build/dev profiles |
| `package.json` | Frontend dependencies (SolidJS, Tauri API, @xyflow/react, marked, DOMPurify, Tailwind); build scripts |
| `vite.config.ts` | Vite bundler config for SolidJS + Tauri dev server |
| `tailwind.config.js` | Tailwind CSS content paths and theme extensions |
| `tsconfig.json` | TypeScript compiler options (strict mode, module resolution) |

---

## Quick Cross-Reference: Feature → Files

| Feature | Frontend files | Backend files |
|---|---|---|
| **Chat / RAG** | `stores/query.ts`, `components/chat/*`, `pages/Dashboard.tsx` | `commands/query.rs`, `services/query_router.rs`, `services/llm.rs`, `services/cloud_llm.rs` |
| **Document indexing** | `components/notebook/SourceList.tsx`, `components/notebook/SourceStatusBadge.tsx` | `commands/sources.rs`, `services/indexer.rs`, `services/chunker.rs`, `services/parsers/*`, `services/embedder.rs` |
| **Knowledge Graph** | `pages/KnowledgeGraphPage.tsx`, `components/knowledge-graph/*` | `commands/entities.rs`, `commands/kg_preferences.rs`, `services/entity_extractor.rs`, `services/knowledge_graph.rs` |
| **Analysis** | `components/analysis/*` | `commands/analysis.rs`, `services/analysis_engine.rs`, `services/exporters/*` |
| **Batch processing** | `stores/batch.ts`, `components/batch/*` | `commands/batch.rs`, `services/batch_processor.rs` |
| **Meetings / STT** | `components/meeting/MeetingPanel.tsx` | `commands/meetings.rs`, `services/meeting/*`, `services/stt_sidecar.rs` |
| **Folder watching** | `pages/SettingsPage.tsx` | `commands/folders.rs`, `services/file_watcher.rs` |
| **Search** | `pages/SearchPage.tsx`, `components/search/*` | `commands/search.rs` |
| **Settings / GPU / Cloud** | `stores/system.ts`, `pages/SettingsPage.tsx` | `commands/system.rs`, `services/hardware_detector.rs`, `services/cloud_llm.rs` |
| **Background tasks** | `components/WorkloadBar.tsx`, `stores/scheduler.ts` | `services/scheduler.rs`, `services/task_budget.rs`, `services/scheduler_eta.rs` |
| **Database** | — | `db/schema.rs`, `db/migrations/*.sql`, `db/repositories/*` |
| **Agent overlay** | `pages/Overlay.tsx`, `components/overlay/*` | `agent/*` |

---

*Generated: 2026-05-05 | EchoAI codebase at `/media/riaz/New Volume/echo-AI`*
