# EchoAI — Complete Codebase Overview

> **Purpose of this document**: A single reference for understanding the entire EchoAI codebase — architecture, data flows, every file's role, DB schema, key algorithms, and design decisions.

---

## Table of Contents

1. [What EchoAI Is](#1-what-echoai-is)
2. [Technology Stack](#2-technology-stack)
3. [Repository Layout](#3-repository-layout)
4. [Architecture Overview](#4-architecture-overview)
5. [Startup Sequence](#5-startup-sequence)
6. [Database Schema](#6-database-schema)
7. [Core Pipeline: Indexing](#7-core-pipeline-indexing)
8. [Core Pipeline: Query & Chat](#8-core-pipeline-query--chat)
9. [Knowledge Graph (KG)](#9-knowledge-graph-kg)
10. [Meeting Agent](#10-meeting-agent)
11. [Batch Processing](#11-batch-processing)
12. [Analysis Engine](#12-analysis-engine)
13. [Scheduler & Priority System](#13-scheduler--priority-system)
14. [Hardware Detection & Model Selection](#14-hardware-detection--model-selection)
15. [Cloud LLM (Groq) Integration](#15-cloud-llm-groq-integration)
16. [File Watcher](#16-file-watcher)
17. [Frontend Architecture (SolidJS)](#17-frontend-architecture-solidjs)
18. [Tauri IPC Contract](#18-tauri-ipc-contract)
19. [Key Design Decisions & Constraints](#19-key-design-decisions--constraints)
20. [File-by-File Reference](#20-file-by-file-reference)

---

## 1. What EchoAI Is

EchoAI is a **fully-offline desktop AI assistant** built as a Tauri v2 application. It lets users:

- **Index** local documents (PDF, DOCX, XLSX, TXT, MD, images, code) into a local SQLite database
- **Chat** with those documents using RAG (Retrieval-Augmented Generation) with local LLM inference
- **Build a Knowledge Graph** of entities and relationships extracted from documents
- **Run batch operations** (summarize, extract dates, flag keywords) across entire folders
- **Transcribe meetings** with real-time audio capture → VAD → Whisper STT (Phase 2, experimental)
- **Optionally route** to Groq cloud API for faster inference when configured

All inference (embeddings + LLM) runs locally with zero network dependency by default.

---

## 2. Technology Stack

| Layer | Technology | Purpose |
|---|---|---|
| Desktop shell | **Tauri v2** | Native window, OS integration, IPC bridge |
| Backend language | **Rust 1.87 (stable)** | All computation, inference, DB |
| Frontend language | **TypeScript + SolidJS** | Reactive UI, zero virtual DOM |
| UI styling | **Tailwind CSS v3** | Utility-first CSS |
| Graph rendering | **@xyflow/react** (port) | Interactive KG canvas |
| Database | **SQLite** (WAL mode) | Single-file embedded DB |
| Vector search | **sqlite-vec (vec0)** | ANN similarity search (no external service) |
| Full-text search | **SQLite FTS5** (Porter stemming) | Keyword search |
| Embeddings | **fastembed-rs** (ONNX, pure Rust) | No Python, no network |
| LLM inference | **llama-cpp-2** (Rust FFI to llama.cpp) | Local GPU/CPU inference |
| Audio transcription | **whisper-rs** sidecar binary | Speech-to-text for meetings |
| Cloud LLM (opt-in) | **Groq API** (OpenAI-compatible) | Faster cloud inference |
| Async runtime | **Tokio** | Async I/O, background tasks |
| Markdown render | **marked** + **DOMPurify** | Safe markdown in chat |

### Feature Flags (Cargo)

```
llm     — Enable llama.cpp inference (default ON)
stt     — Enable whisper.cpp inference (OFF by default — ggml symbol conflict)
cuda    — NVIDIA GPU acceleration
vulkan  — AMD GPU acceleration
```

---

## 3. Repository Layout

```
echo-AI/
├── src/                          # SolidJS frontend
│   ├── api/index.ts              # All typed invoke() wrappers
│   ├── stores/                   # SolidJS reactive stores
│   │   ├── query.ts              # Chat session + streaming state
│   │   ├── notebooks.ts          # Notebook list
│   │   ├── analysis.ts           # Analysis results
│   │   ├── batch.ts              # Batch job state
│   │   ├── scheduler.ts          # Workload bar state
│   │   └── system.ts             # Hardware tier + cloud settings
│   ├── components/
│   │   ├── chat/                 # ChatPanel, MessageBubble, Citations, Scope picker
│   │   ├── notebook/             # NotebookList, NotebookView, SourceList
│   │   ├── knowledge-graph/      # KG canvas, relation filter panel
│   │   ├── analysis/             # TemplateSelector, AnalysisResult
│   │   ├── batch/                # BatchPanel, BatchResultTable
│   │   ├── meeting/              # MeetingPanel (start/stop, segment list)
│   │   ├── search/               # GlobalSearch
│   │   ├── overlay/              # Overlay window
│   │   ├── WorkloadBar.tsx       # Background task progress bar
│   │   └── ModelDownloadBanner.tsx
│   └── pages/
│       ├── Dashboard.tsx         # Main layout (Notebooks + Chat)
│       ├── KnowledgeGraphPage.tsx
│       ├── SearchPage.tsx
│       ├── SettingsPage.tsx      # Folders, GPU, Cloud, Audit, Meetings
│       └── Overlay.tsx
│
├── src-tauri/                    # Rust backend
│   ├── src/
│   │   ├── lib.rs                # App wiring, startup, all command registration
│   │   ├── commands/             # Tauri command handlers (thin layer)
│   │   │   ├── notebooks.rs      # CRUD for notebooks
│   │   │   ├── sources.rs        # Add/remove/retry sources, KG status
│   │   │   ├── query.rs          # Chat session + streaming
│   │   │   ├── system.rs         # Hardware, model download, GPU, cloud settings
│   │   │   ├── folders.rs        # Indexed folder management
│   │   │   ├── search.rs         # Global FTS5 search
│   │   │   ├── entities.rs       # KG entity/relationship queries
│   │   │   ├── analysis.rs       # Analysis template runner
│   │   │   ├── batch.rs          # Batch job CRUD + pause/resume
│   │   │   ├── meetings.rs       # Meeting start/stop/list
│   │   │   ├── scheduler.rs      # Foreground/cancel/list index jobs
│   │   │   ├── audit.rs          # Audit log queries
│   │   │   ├── archive.rs        # Notebook export/import
│   │   │   ├── trial.rs          # Trial status
│   │   │   ├── kg_preferences.rs # KG entity-type preferences
│   │   │   └── mod.rs
│   │   ├── services/             # Core business logic
│   │   │   ├── indexer.rs        # Main ingestion pipeline
│   │   │   ├── query_router.rs   # 5-tier RAG query routing
│   │   │   ├── llm.rs            # ModelSlot (llama.cpp wrapper)
│   │   │   ├── embedder.rs       # fastembed ONNX wrapper
│   │   │   ├── entity_extractor.rs # 2-pass NER
│   │   │   ├── knowledge_graph.rs  # Graph query helpers
│   │   │   ├── analysis_engine.rs  # Analysis template runner
│   │   │   ├── batch_processor.rs  # Batch folder operations
│   │   │   ├── scheduler.rs        # Foreground priority scheduler
│   │   │   ├── hardware_detector.rs # RAM/GPU tier detection
│   │   │   ├── cloud_llm.rs        # Groq API integration
│   │   │   ├── chunker.rs          # Text → token-size chunks
│   │   │   ├── file_watcher.rs     # inotify/kqueue watcher
│   │   │   ├── stt_sidecar.rs      # Whisper sidecar IPC
│   │   │   ├── task_budget.rs      # CPU/memory budget guard
│   │   │   ├── scheduler_eta.rs    # ETA estimation
│   │   │   ├── priority_model_slot.rs
│   │   │   ├── query_trace.rs      # Query telemetry logger
│   │   │   ├── parsers/            # File format parsers (PDF, DOCX, XLSX…)
│   │   │   ├── exporters/          # PDF export for analysis
│   │   │   └── meeting/            # Meeting Agent subsystem
│   │   │       ├── audio_capture.rs
│   │   │       ├── segmenter.rs
│   │   │       ├── service.rs
│   │   │       └── transcribe_worker.rs
│   │   ├── db/
│   │   │   ├── schema.rs           # DB init, vec0 table creation
│   │   │   ├── migrations/         # 12 SQL migration files
│   │   │   └── repositories/       # All DB access (one file per table)
│   │   └── agent/                  # (Reserved for agent overlay)
│   ├── Cargo.toml                  # Rust deps + features
│   ├── tauri.conf.json             # Window config, identifiers, bundle
│   └── capabilities/default.json  # Tauri v2 permission list
│
├── specs/                          # SDD feature specs
├── history/                        # PHRs + ADRs
├── docs/                           # Architecture docs, plans
├── config/personas/                # TOML persona configs for LLM prompts
├── models/                         # Downloaded GGUF model files (gitignored)
└── .specify/                       # SDD tooling (templates, scripts)
```

---

## 4. Architecture Overview

```
┌─────────────────────────────────────────────────────┐
│                   SolidJS Frontend                   │
│  Dashboard ─ KnowledgeGraph ─ Search ─ Settings     │
│  stores/ (query, notebooks, analysis, batch, sys)    │
│  api/index.ts (typed invoke() wrappers)              │
└─────────────┬───────────────────────────────────────┘
              │  Tauri IPC (invoke + events)
┌─────────────▼───────────────────────────────────────┐
│                  Tauri Command Layer                  │
│  commands/*.rs — thin handlers, validate, delegate   │
└─────────────┬───────────────────────────────────────┘
              │
┌─────────────▼───────────────────────────────────────┐
│                  Services Layer (Rust)                │
│                                                       │
│  ┌──────────┐ ┌────────────┐ ┌────────────────────┐ │
│  │ indexer  │ │query_router│ │  entity_extractor  │ │
│  │ (pipeline│ │ (5-tier    │ │  (2-pass NER)      │ │
│  │  engine) │ │  RAG)      │ │                    │ │
│  └────┬─────┘ └─────┬──────┘ └────────────────────┘ │
│       │             │                                 │
│  ┌────▼─────┐ ┌─────▼──────┐ ┌────────────────────┐ │
│  │ embedder │ │    llm     │ │   scheduler        │ │
│  │(fastembed│ │(ModelSlot) │ │ (foreground        │ │
│  │  ONNX)   │ │llama.cpp   │ │  priority)         │ │
│  └──────────┘ └────────────┘ └────────────────────┘ │
└─────────────┬───────────────────────────────────────┘
              │
┌─────────────▼───────────────────────────────────────┐
│                   Database Layer (SQLite)             │
│   WAL mode + FTS5 + sqlite-vec (vec0 ANN)            │
│   repositories/*.rs — all SQL access                 │
└─────────────────────────────────────────────────────┘
```

**Critical architectural constraint**: The `llm` and `stt` features cannot be enabled simultaneously — both `llama-cpp-sys-2` and `whisper-rs-sys` bundle `ggml`, causing link-time symbol conflicts. The Meeting Agent works around this by running Whisper as a **sidecar process** (separate binary), communicating over stdio/JSON.

---

## 5. Startup Sequence

`lib.rs::run()` orchestrates startup in two phases to avoid freezing the window:

### Phase 1 — Synchronous (blocks window open)
1. `HardwareTier::detect()` — detect RAM/GPU tier
2. `init_db_only()` (called via `block_on`) — open SQLite pool, run migrations, create vec0 table
3. `cloud_llm::load_settings()` — seed cloud settings cache
4. Register `SqlitePool` in Tauri managed state

### Phase 2 — Background (spawned after window opens)
5. Load embedding model (fastembed ONNX, ~2 minutes first run)
6. Load LLM model slot (llama.cpp, GPU/CPU)
7. Apply GPU preference from `db_config`
8. Warm up embedder + LLM (so first query is fast)
9. Init `TaskBudget`, `Scheduler`, `FileWatcherState`
10. Reset stuck pending/processing sources (crash recovery)
11. Resume NER for `ner_status='pending'` sources (crash recovery)
12. Backfill folder stub notebooks (schema repair)
13. Re-watch all previously registered folders
14. Spawn file watcher event loop
15. Spawn Meeting Agent sidecar (if binary exists)
16. Register all remaining state

---

## 6. Database Schema

All tables in a single `echo.db` SQLite file (WAL mode, FK enforcement, 128 MB cache).

### Core Tables

| Table | Purpose |
|---|---|
| `db_config` | Key-value app settings (embedding_dim, gpu_enabled, etc.) |
| `notebooks` | User-created document collections (UUID PK) |
| `sources` | Indexed files (path, hash, status, format) |
| `notebook_sources` | M:N join between notebooks and sources |
| `indexed_folders` | Watched filesystem folders |
| `chunks` | Text segments (≤512 tokens) from sources |
| `chunks_fts` | FTS5 virtual table over chunks.content (Porter stemmer) |
| `chunk_embeddings` | **vec0 virtual table** — ANN vector search (384 or 768 dim) |
| `query_cache` | Semantic cache for chat queries (cosine ≥ 0.92 hit) |
| `query_sessions` | Chat session per notebook |
| `query_messages` | Chat turn history (user + assistant) |
| `entities` | Named entities (person, org, date, project, decision, amount, email) |
| `entity_mentions` | Entity ↔ chunk M:N with context text |
| `entity_relationships` | Directed graph edges between entities |
| `batch_jobs` | Batch operation jobs (summarize/extract/flag) |
| `batch_results` | Per-file results from batch jobs |
| `audit_log` | Event log (query_answered, folder_revoked, data_deleted) |
| `index_jobs` | Track in-flight folder indexing for crash recovery |
| `meetings` | Meeting session metadata |
| `meeting_segments` | Whisper transcript segments (t0_ms, t1_ms, text) |

### Vector Table Details

`chunk_embeddings` is a `vec0` virtual table — **not** a regular table. It cannot be queried with normal SQL JOINs. It uses a custom ANN distance syntax:

```sql
SELECT chunk_id, distance
FROM chunk_embeddings
WHERE embedding MATCH ?   -- query vector as blob
ORDER BY distance
LIMIT 10
```

Dimension is determined at startup: **768** for `nomic-embed-text-v1.5` (≥12 GB RAM), **384** for `all-MiniLM-L6-v2` (otherwise). Once set in `db_config`, it is **immutable** — changing requires re-indexing everything.

### Migration Files

| File | What it adds |
|---|---|
| `001_initial_schema.sql` | Full base schema (all core tables) |
| `002_schema_fixes.sql` | Format CHECK widen, extra indexes |
| `003_status_check_widen.sql` | Indexing status values |
| `004_index_jobs.sql` | `index_jobs` table for crash recovery |
| `005_incomplete_messages.sql` | `is_incomplete` column on `query_messages` |
| `006_agent_tables.sql` | Agent overlay tables (placeholder) |
| `007_agent_support_tables.sql` | Agent support |
| `008_ner_progress.sql` | `ner_status` column on sources |
| `009_universal_entity_types.sql` | Broader entity type CHECK |
| `010_trial_meta.sql` | Trial mode metadata |
| `011_gpu_settings.sql` | GPU toggle in `db_config` |
| `012_meetings.sql` | `meetings` + `meeting_segments` tables |

---

## 7. Core Pipeline: Indexing

File: `src-tauri/src/services/indexer.rs`

### Concurrency Controls
- `INDEXING_SEM` — semaphore(1): only 1 file parses+embeds at a time
- `ENTITY_EXTRACTION_SEM` — semaphore(1): only 1 LLM NER job at a time

### Pipeline per File

```
add_sources() command
      │
      ▼
indexer::index_file()
      │
      ├─ 1. Dedup check (SHA-256 hash) — skip if already indexed
      ├─ 2. Insert source row (status='processing')
      ├─ 3. Emit source:indexing-progress (0%)
      │
      ├─ [spawn_blocking] ─────────────────────────────────────
      │   ├─ parsers::parse() — extract text from file format
      │   │    PDF  → lopdf
      │   │    DOCX → docx-rs
      │   │    XLSX → calamine
      │   │    TXT/MD/code → raw read
      │   │    Images → OCR stub (placeholder)
      │   │
      │   ├─ chunker::chunk() — split into ≤512 token segments
      │   │    Respects sentence boundaries
      │   │    Returns Vec<Chunk> with char offsets + page numbers
      │   │
      │   └─ embedder::embed_batch() — ONNX inference (fastembed)
      │        Returns Vec<Vec<f32>> (384 or 768 dims)
      │
      ├─ 4. Bulk insert chunks (single transaction)
      ├─ 5. Upsert chunk_embeddings (vec0)
      ├─ 6. Update source status='indexed'
      ├─ 7. Emit source:indexing-progress (100%)
      │
      └─ [background spawn] entity_extractor::run_for_source()
           ├─ Pass 1: regex NER (dates, amounts, article refs)
           └─ Pass 2: LLM NER (person, org, concept, term…)
                Uses ModelSlot with GBNF JSON grammar
                Cap: 20 chunks (env ECHO_PASS2_CHUNK_LIMIT)
```

### Pass 2 NER — LLM Entity Extraction

- Runs **after** indexing completes (source is queryable immediately)
- `ner_status` column: `null` → `pending` → `done` / `failed`
- On startup: resumes any `ner_status='pending'` sources (crash recovery)
- LLM prompt returns structured JSON: `{"entities": [{"value":"...", "type":"person"}]}`
- Entity types: `person`, `org`, `date`, `project`, `decision`, `amount`, `email`, `concept`, `term`, `reference`

---

## 8. Core Pipeline: Query & Chat

File: `src-tauri/src/services/query_router.rs`

### 5-Tier Routing

```
User sends query text
        │
        ▼
[Tier 0] Semantic Cache
  embed query → cosine vs query_cache
  if similarity ≥ 0.92 → return cached answer immediately
        │ miss
        ▼
[Tier 1] Entity Pattern Router
  regex check for dates, amounts, "Article N", etc.
  if match → FTS5 SQL query (no LLM needed)
        │ no match
        ▼
[Tier 2] Semantic Retrieval
  embed query → vec0 ANN search → top-K chunks
  also: FTS5 BM25 search → merge + deduplicate
  classify_query() → Regular vs Summary type
        │
        ▼
[Tier 3] LLM Synthesis (3B model)
  build context: top-8 chunks (≤1200 context tokens)
  system prompt v3.1.0 (grounding rules + citation format)
  stream tokens → emit chat://token events
  grounding validator: check every citation maps to a known source
        │ if hardware tier ≥ 4 and query type = Summary
        ▼
[Tier 4] 7B model (if available)
  same as Tier 3 but with larger model
```

### Streaming Events (Tauri → Frontend)

| Event | Payload | When |
|---|---|---|
| `chat://started` | `{chat_id}` | Before LLM call |
| `chat://thinking` | `{stage, detail}` | Retrieving/Ranking/Composing |
| `chat://token` | `{token}` | Each LLM output token |
| `chat://done` | `{message, quality_notice}` | Generation complete |
| `chat://cancelled` | `{partial_text}` | User cancelled |
| `chat://error` | `{message}` | Error during generation |
| `chat://background-paused` | `{kind}` | Scheduler paused chat |

### Grounding Validator

After LLM generates answer:
1. Extract all `[filename, page N]` citation markers
2. Verify each filename maps to a retrieved source
3. Count factual sentences (>8 words, non-question) without citations
4. If grounding_score < threshold → replace with `GROUNDING_FALLBACK` or add quality notice
5. Return `GroundingResult` with `is_grounded`, `citation_count`, `grounding_score`

### Chat Scope

Users can scope chat to:
- **Notebook** — query only sources in the active notebook
- **Folder** — query all sources across an indexed folder
- **Both** — combine notebook + folder sources

---

## 9. Knowledge Graph (KG)

### Entity Extraction (2-pass)

**Pass 1** (`entity_extractor.rs`): Regex patterns, runs synchronously during chunk indexing:
- ISO dates (`2024-01-15`)
- Numeric dates (`dd/mm/yyyy`)
- Currency amounts (`$1,234.56`)
- Legal references (`Article 8`, `Section 3(2)`, `Schedule IV`)

**Pass 2** (background, post-indexing): LLM with GBNF JSON grammar:
- Types: person, org, date, project, decision, amount, email, concept, term, reference
- Runs up to 20 chunks per source (configurable via `ECHO_PASS2_CHUNK_LIMIT`)
- Creates `entities` rows with `canonical_value` + `entity_type`
- Creates `entity_mentions` linking entity ↔ chunk

### Relationship Extraction

`knowledge_graph.rs` queries entity co-occurrence within the same chunk to infer relationships. Stored in `entity_relationships` as directed edges.

### KG Preferences

Users can configure which entity types to display and custom relationship verb lists via `kg_preferences` commands. Stored in `db_config`.

### Frontend Rendering

`KnowledgeGraphPage.tsx` uses `@xyflow/react` for interactive graph:
- Radial layout algorithm
- Nodes colored by entity type
- Edge labels show relationship type
- `RelationTypesPanel` for filtering displayed relationships

---

## 10. Meeting Agent

**Status**: Phase 2 complete (Phase 3 UI in progress)

### Architecture (workaround for ggml conflict)

Since `llm` and `stt` features cannot coexist in the same binary, Whisper runs as a **sidecar binary** (`src-tauri/binaries/whisper-sidecar`):

```
MeetingService
      │
      ├─ audio_capture.rs    — cpal mic capture → raw PCM frames
      ├─ segmenter.rs        — webrtc-vad → speech segments
      │                         rubato → resample to 16kHz
      │                         hound → encode WAV
      │
      └─ transcribe_worker.rs
              │ base64-encodes WAV frames
              ▼
         stt_sidecar.rs
              │ JSON over stdio
              ▼
         whisper-sidecar (external process)
              │
              ▼
         Transcript text → meetings DB (t0_ms, t1_ms, text)
```

### Commands

| Command | Action |
|---|---|
| `start_meeting` | Open mic, start VAD + transcription |
| `stop_meeting` | Close mic, finalize meeting record |
| `list_meeting_segments` | Return transcript segments for a meeting |
| `get_meeting` | Return meeting metadata |

### Database Tables

- `meetings`: `id`, `title`, `started_at`, `ended_at`, `language`, `model_path`, `status`
- `meeting_segments`: `id`, `meeting_id`, `t0_ms`, `t1_ms`, `text`, `created_at`

---

## 11. Batch Processing

File: `src-tauri/src/services/batch_processor.rs`

Processes an entire folder with a single LLM operation type:

| Operation | What it does |
|---|---|
| `summarize` | Generate summary of each file |
| `extract_dates` | List all dates found in each file |
| `flag_keywords` | Mark files containing user-specified keywords |
| `extract_actions` | Extract action items from each file |

### Flow

```
create_batch_job command
      │
      ▼
batch_processor::spawn_batch_job()  [background tokio task]
      │
      ├─ Register WorkloadKind::Batch with Scheduler
      ├─ Scan folder for supported file formats
      ├─ For each file:
      │   ├─ Index file (if not already indexed)
      │   ├─ Fetch chunks from DB
      │   ├─ Build operation-specific prompt
      │   ├─ LLM inference → result text
      │   ├─ Store in batch_results
      │   └─ Emit batch:progress event
      │
      └─ System notification on completion
```

Pause/resume via `pause_batch_job` / `resume_batch_job` commands — uses `WorkloadHandle.should_pause()` cooperative pause points.

---

## 12. Analysis Engine

File: `src-tauri/src/services/analysis_engine.rs`

Runs structured multi-source analysis with 5 templates:

| Template | Output |
|---|---|
| `compare_sources` | Side-by-side comparison of arguments/positions |
| `extract_action_items` | Actionable tasks from all sources |
| `find_contradictions` | Conflicting claims across sources |
| `generate_timeline` | Chronological events from all sources |
| `summarize_all` | Consolidated summary |

Each template:
1. Fetches top-50 chunks by FTS5 / full scan
2. Builds template-specific system prompt + GBNF grammar for structured JSON output
3. Routes to best available model tier
4. Returns `AnalysisResult` with `output`, `citations[]`, `quality_constrained` flag

Analysis results are ephemeral (not persisted to DB). Users can export to PDF via `export_analysis` command.

---

## 13. Scheduler & Priority System

File: `src-tauri/src/services/scheduler.rs`

### Workload Kinds & Priorities

| Kind | Priority | Uses LLM |
|---|---|---|
| `Chat` | 2 (highest) | Yes |
| `Batch` | 2 | Yes |
| `KgExtraction` | 1 | Yes |
| `Pass2Ner` | 1 | Yes |
| `Indexing` | 0 | No |
| `FileWatcher` | 0 | No |

### Mechanism

- Each workload registers a `WorkloadHandle` with the `Scheduler`
- Workers call `handle.should_pause().await` at natural loop boundaries
- When a `Chat` starts, the `Scheduler` pauses all lower-priority LLM workloads
- Workers observe `is_cancelled()` for graceful shutdown
- Progress emitted at ≤4 Hz via `start_progress_pump()` → `workload://state` events → frontend `WorkloadBar`

### Index Job Management

`index_jobs` table tracks in-flight folder indexing across app restarts. On startup, `reconcile_orphaned_running_jobs()` flips `running → paused` so users can resume interrupted folder scans.

---

## 14. Hardware Detection & Model Selection

File: `src-tauri/src/services/hardware_detector.rs`

### Tier Assignment

| RAM | Tier | Embedding Model | LLM |
|---|---|---|---|
| ≥32 GB | 5 | nomic-embed-text-v1.5 (768d) | 13B+ |
| ≥16 GB | 4 | nomic-embed-text-v1.5 (768d) | 7B |
| ≥12 GB | 3 | nomic-embed-text-v1.5 (768d) | 3B |
| ≥8 GB | 3 | all-MiniLM-L6-v2 (384d) | 3B |
| <8 GB | 2 | all-MiniLM-L6-v2 (384d) | 1.5B |

GPU detection (via `lspci`/`wmic`):
- Reads VRAM
- `has_discrete_gpu` = VRAM ≥ 2 GB
- GPU vendor: `Nvidia`, `Amd`, or `None`
- Affects whether `cuda`/`vulkan` features are used

### GPU Toggle

Users can enable/disable GPU in Settings. The preference is persisted in `db_config` key `gpu_enabled`. Applied to `ModelSlot` via `AtomicBool` at startup and on toggle.

---

## 15. Cloud LLM (Groq) Integration

File: `src-tauri/src/services/cloud_llm.rs`

When enabled, **replaces** local LLM calls for chat queries. Embeddings always remain local.

### Settings (`cloud_settings.json` in app data dir)

```json
{
  "enabled": true,
  "provider": "groq",
  "api_key": "gsk_...",
  "model": "llama-3.1-8b-instant"
}
```

Env override: `ECHO_GROQ_API_KEY`

### Flow

- Settings loaded into a `static CACHED: Mutex<CloudSettings>` at startup
- Non-Tauri code (indexer, batch) can read settings without `AppHandle` via `load_settings_cached()`
- Streams from Groq's OpenAI-compatible API: `https://api.groq.com/openai/v1/chat/completions`
- Emits the same `chat://token` events — frontend is unchanged

---

## 16. File Watcher

File: `src-tauri/src/services/file_watcher.rs`

Uses `notify` crate (inotify on Linux, kqueue on macOS, ReadDirectoryChangesW on Windows).

### Event Flow

```
OS filesystem event
      │
      ▼
FileWatcherState (sends to channel)
      │
      ▼
File watcher event loop (dedicated tokio task)
      ├─ Filter: skip Office temp files (~$...), dot-files, unsupported formats
      ├─ Debounce: 800ms per path (handles Create→Modify→CloseWrite burst)
      │
      ├─ On Create/Modify:
      │   ├─ Check file is non-zero size (skip in-progress writes)
      │   ├─ Find owner folder from indexed_folders
      │   └─ indexer::index_file() → emit folder:progress refresh
      │
      └─ On Delete:
          ├─ Find source_id by file_path
          └─ DELETE chunks + chunk_embeddings + sources (single transaction)
```

Folders are re-watched on startup by iterating `indexed_folders` table.

---

## 17. Frontend Architecture (SolidJS)

### Core Principles

- **No virtual DOM**: SolidJS uses fine-grained reactivity — only the exact DOM nodes that depend on a signal update
- **Stores**: `createStore()` for complex nested state; `createSignal()` for simple values
- **Async pattern**: Register Tauri event listeners **before** `invoke()` call (events fire during invoke)

### Store Summary

| Store | State |
|---|---|
| `query.ts` | Session ID, message history, streaming text, thinking text, error, cancel state |
| `notebooks.ts` | Notebook list, active notebook |
| `analysis.ts` | Analysis results, template selection |
| `batch.ts` | Active batch job, refresh key, fullscreen state |
| `scheduler.ts` | Active workloads (for WorkloadBar) |
| `system.ts` | Hardware tier, cloud settings |

### Component Hierarchy (Dashboard)

```
Dashboard
├── WorkloadBar (background task progress)
├── [sidebar]
│   ├── NotebookList
│   └── BatchPanel (collapsible)
└── [main]
    ├── NotebookView
    │   ├── SourceList (drag-drop, status badges)
    │   └── [tabs]
    │       ├── ChatPanel
    │       │   ├── ChatScopePicker (notebook/folder/both)
    │       │   ├── MessageBubble[]
    │       │   │   ├── CitationChip[]
    │       │   │   └── QualityNoticeBanner
    │       │   └── [input + cancel button]
    │       └── AnalysisView
    │           └── TemplateSelector
    └── BatchResultTable (when job active)
```

### Tauri Event Subscriptions Pattern

```typescript
// CORRECT: listen before invoke
const unlisteners = await Promise.all([
  listen<ChatTokenEvent>("chat://token", ({ payload }) => {
    setState("streamingText", prev => prev + payload.token);
  }),
]);
try {
  await api.streamQueryNotebook(...);
} finally {
  unlisteners.forEach(u => u());
}
```

---

## 18. Tauri IPC Contract

All commands registered in `lib.rs`. Frontend calls via `invoke(commandName, args)`.

### Full Command List

**Notebooks**
- `create_notebook(name)` → `Notebook`
- `list_notebooks()` → `Notebook[]`
- `get_notebook(id)` → `Notebook`
- `rename_notebook(id, name)` → void
- `delete_notebook(id)` → void

**Sources**
- `add_sources(notebook_id, file_paths[])` → void (fires `source:indexing-progress` events)
- `list_notebook_sources(notebook_id)` → `Source[]`
- `remove_source(notebook_id, source_id)` → void
- `retry_failed_source(source_id)` → void
- `get_kg_status(notebook_id)` → `KgStatus`
- `clean_orphan_sources()` → `{cleaned: number}`
- `resume_pending_ner()` → void

**Query / Chat**
- `create_session(notebook_id)` → `QuerySession`
- `stream_query_notebook(session_id, notebook_id, query, scope?)` → void (fires chat:// events)
- `cancel_chat(chat_id)` → void
- `save_partial_message(session_id, text)` → void
- `get_session_history(session_id)` → `Message[]`

**System**
- `get_hardware_tier()` → `HardwareTier`
- `check_model(path)` → `{exists: bool, size_mb: number}`
- `download_model(url, path)` → void (fires `model:download-progress`)
- `restart_app()` → void
- `get_gpu_settings()` → `GpuSettings`
- `set_gpu_enabled(enabled)` → void
- `get_cloud_settings()` → `CloudSettings`
- `set_cloud_settings(settings)` → void
- `open_query_logs_dir()` → void
- `read_latest_query_trace()` → `string`
- `download_query_trace_to(dest_path)` → void

**Folders**
- `add_indexed_folder(path)` → `IndexedFolder`
- `list_indexed_folders()` → `IndexedFolder[]`
- `revoke_indexed_folder(id)` → void
- `list_folder_sources(folder_id)` → `Source[]`
- `list_sources_in_folder(folder_id)` → `Source[]`
- `retry_folder_source(source_id)` → void
- `remove_folder_source(source_id)` → void

**Search**
- `global_search(query, limit?)` → `SearchResult[]`

**Entities / KG**
- `query_entities(filters)` → `Entity[]`
- `get_entity_relationships(entity_id)` → `Relationship[]`
- `get_entity_sources(entity_id)` → `Source[]`
- `get_knowledge_graph(notebook_id)` → `KgGraph`
- `get_kg_preferences()` → `KgPreferences`
- `set_kg_preferences(prefs)` → void

**Analysis**
- `run_analysis(notebook_id, template)` → `AnalysisResult`
- `analyze_selected(notebook_id, source_ids[], template)` → `AnalysisResult`
- `export_analysis(result, dest_path)` → void

**Batch**
- `create_batch_job(folder_path, operation, params?)` → `BatchJob`
- `get_batch_job(id)` → `BatchJob`
- `get_batch_results(job_id)` → `BatchResult[]`
- `export_batch_results(job_id, dest_path)` → void
- `pause_batch_job(id)` → void
- `resume_batch_job(id)` → void
- `cancel_batch_job(id)` → void

**Scheduler**
- `set_foreground(kind)` → void
- `cancel_index_job(job_id)` → void
- `discard_index_job(job_id)` → void
- `list_active_index_jobs()` → `IndexJob[]`

**Meetings**
- `start_meeting(title, language)` → `Meeting`
- `stop_meeting(meeting_id)` → void
- `list_meeting_segments(meeting_id)` → `MeetingSegment[]`
- `get_meeting(meeting_id)` → `Meeting`

**Other**
- `get_audit_log(limit?)` → `AuditEvent[]`
- `export_notebook(notebook_id, dest_path)` → void
- `import_notebook(src_path)` → `Notebook`
- `get_trial_status()` → `TrialStatus`

### Frontend Events (Rust → Frontend)

| Event | Emitted By | Payload |
|---|---|---|
| `chat://started` | query_router | `{chat_id}` |
| `chat://thinking` | query_router | `{stage, detail}` |
| `chat://token` | query_router / cloud_llm | `{token}` |
| `chat://done` | query_router | `{message, quality_notice}` |
| `chat://cancelled` | query.rs | `{partial_text}` |
| `chat://error` | query.rs | `{message}` |
| `chat://background-paused` | scheduler | `{kind}` |
| `source:indexing-progress` | indexer | `{source_id, percent, stage}` |
| `folder:progress` | indexer / file_watcher | `{folder_id, indexed, total, status}` |
| `batch:progress` | batch_processor | `{job_id, processed, total}` |
| `model:download-progress` | system.rs | `{bytes_downloaded, total_bytes}` |
| `workload://state` | scheduler | `{workloads[]}` |

---

## 19. Key Design Decisions & Constraints

### 1. Single-process, two-window app
Tauri runs everything in one process. The main window and overlay window share the same Rust state. No inter-process communication needed — state is `Arc<T>` in managed state.

### 2. ggml conflict — Whisper sidecar
`llama-cpp-sys-2` and `whisper-rs-sys` both statically link `ggml`. When both features are enabled, the linker finds two definitions of every `ggml_*` symbol and fails. Solution: Whisper runs as a separately compiled binary (`src-tauri/binaries/`) called via `tauri-plugin-shell` sidecar API with JSON over stdio.

### 3. NTFS target directory
The project lives on an NTFS drive. The Cargo build target must be on the same partition to avoid corrupted inodes. Always use `CARGO_HOME` and `CARGO_TARGET_DIR` pointing to the NTFS volume.

### 4. Embedding dimension immutability
Once `embedding_dim` is written to `db_config`, it cannot be changed without wiping `chunks`, `chunk_embeddings`, and re-indexing. The vec0 table must match the dimension exactly.

### 5. SQLite over PostgreSQL
Single-user local app — no network, no server, no setup. WAL mode gives concurrent read + write performance. sqlite-vec adds ANN search without a separate vector database.

### 6. Cooperative scheduler (not preemptive)
Workers must call `should_pause().await` at their own loop boundaries. There is no forced preemption. This means very long LLM inference calls (single token generation phase) cannot be interrupted mid-token but only between tokens.

### 7. Speed is top priority
Chat response speed is the primary user-visible metric. Every change that adds latency to the query path must be justified. The reviewer agent (ECHO_REVIEWER=1) is opt-in because it adds 1-3 extra LLM round-trips.

### 8. Grounding system prompt is versioned
`GROUNDING_SYSTEM_PROMPT_VERSION = "3.1.0"`. Logged in `audit_log` with every LLM answer so answers remain traceable to the exact prompt version. Never generate this string at runtime.

---

## 20. File-by-File Reference

### Backend (src-tauri/src/)

| File | Responsibility |
|---|---|
| `lib.rs` | App wiring: startup sequence, all command registration, state management |
| `commands/notebooks.rs` | CRUD for notebooks table |
| `commands/sources.rs` | Add/remove/retry sources; KG status; orphan cleanup |
| `commands/query.rs` | Create session; stream_query_notebook (calls query_router); cancel; history |
| `commands/system.rs` | Hardware tier; model check/download; restart; GPU settings; cloud settings; trace logs |
| `commands/folders.rs` | Add/revoke/list indexed folders; per-folder source management |
| `commands/search.rs` | Global FTS5 full-text search across all notebooks |
| `commands/entities.rs` | KG entity queries; relationship queries; source membership |
| `commands/analysis.rs` | Run analysis template; export to PDF |
| `commands/batch.rs` | Batch job lifecycle (create/get/pause/resume/cancel/export) |
| `commands/meetings.rs` | Meeting Agent commands (start/stop/list segments) |
| `commands/scheduler.rs` | Set foreground workload; cancel/discard/list index jobs |
| `commands/audit.rs` | Fetch audit_log entries |
| `commands/archive.rs` | Export/import notebook as ZIP archive |
| `commands/trial.rs` | Trial status (SQLite-based, 7-day) |
| `commands/kg_preferences.rs` | Get/set KG entity type display preferences |
| `services/indexer.rs` | **Ingestion pipeline**: parse → chunk → embed → store → NER |
| `services/query_router.rs` | **5-tier RAG**: cache → entity → FTS5 → semantic → LLM |
| `services/llm.rs` | `ModelSlot`: llama.cpp wrapper, 90s idle eviction, grounding prompt constants |
| `services/embedder.rs` | fastembed ONNX wrapper; `embed_one()` + `embed_batch()` |
| `services/entity_extractor.rs` | 2-pass NER: regex (Pass 1) + LLM JSON grammar (Pass 2) |
| `services/knowledge_graph.rs` | KG graph queries: entity co-occurrence → relationships |
| `services/analysis_engine.rs` | 5 analysis templates with GBNF-constrained LLM output |
| `services/batch_processor.rs` | Folder batch operations with cooperative pause/resume |
| `services/scheduler.rs` | Foreground-priority workload scheduler; progress pump |
| `services/hardware_detector.rs` | RAM/GPU detection → `HardwareTier` (tier 2-5, embedding model choice) |
| `services/cloud_llm.rs` | Groq API streaming, cached settings, same event interface as local |
| `services/chunker.rs` | Text → ≤512 token chunks with sentence boundary respect |
| `services/file_watcher.rs` | notify-based watcher; 800ms debounce; auto-index on create/modify |
| `services/stt_sidecar.rs` | Spawn + IPC with whisper-sidecar binary (JSON/base64 over stdio) |
| `services/task_budget.rs` | CPU/memory budget guard for background tasks |
| `services/scheduler_eta.rs` | ETA calculation for workloads |
| `services/query_trace.rs` | Query telemetry: embed time, retrieval time, LLM time logged to file |
| `services/parsers/` | Format-specific text extractors (PDF/DOCX/XLSX/TXT/images) |
| `services/exporters/` | PDF exporter for analysis results (printpdf) |
| `services/meeting/audio_capture.rs` | cpal mic → PCM frames |
| `services/meeting/segmenter.rs` | webrtc-vad → speech segments; rubato resample → 16kHz WAV |
| `services/meeting/service.rs` | `MeetingService`: orchestrates capture + transcription + DB |
| `services/meeting/transcribe_worker.rs` | Sends WAV frames to sidecar; writes segments to DB |
| `db/schema.rs` | SQLite pool init, WAL config, vec0 table creation, migration runner |
| `db/migrations/*.sql` | 12 sequential migration files |
| `db/repositories/notebooks.rs` | All SQL for notebooks table |
| `db/repositories/sources.rs` | All SQL for sources + notebook_sources |
| `db/repositories/chunks.rs` | Chunk insert, fetch by source/notebook |
| `db/repositories/entities.rs` | Entity upsert, mention insert, relationship queries |
| `db/repositories/folders.rs` | Indexed folder CRUD; backfill stub notebooks |
| `db/repositories/batch_jobs.rs` | Batch job + result CRUD |
| `db/repositories/sessions.rs` | Query session + message CRUD |
| `db/repositories/audit.rs` | Audit log insert + fetch |
| `db/repositories/index_jobs.rs` | Index job tracking + crash recovery |
| `db/repositories/meetings.rs` | Meeting + segment CRUD |

### Frontend (src/)

| File | Responsibility |
|---|---|
| `api/index.ts` | All typed `invoke()` wrappers; all TS interface definitions |
| `stores/query.ts` | Chat session state; streaming update handlers; cancel flow |
| `stores/notebooks.ts` | Notebook list; active notebook selection |
| `stores/analysis.ts` | Analysis template selection; result storage |
| `stores/batch.ts` | Active batch job; fullscreen mode |
| `stores/scheduler.ts` | Workload list subscription; WorkloadBar data |
| `stores/system.ts` | Hardware tier; GPU settings; cloud settings |
| `pages/Dashboard.tsx` | Main layout: NotebookList sidebar + NotebookView main area |
| `pages/KnowledgeGraphPage.tsx` | Full-page KG canvas with relation filter panel |
| `pages/SearchPage.tsx` | Global FTS5 search with result highlighting |
| `pages/SettingsPage.tsx` | Folders, GPU, cloud, audit log, meetings, trace download |
| `pages/Overlay.tsx` | Floating overlay window (minimal) |
| `components/chat/ChatPanel.tsx` | Message list; streaming display; send/cancel |
| `components/chat/ChatScopePicker.tsx` | Notebook / Folder / Both scope selector |
| `components/chat/MessageBubble.tsx` | Renders markdown with citation chips |
| `components/chat/CitationChip.tsx` | Inline source + page reference badge |
| `components/chat/QualityNoticeBanner.tsx` | Shows quality degradation notice |
| `components/notebook/NotebookList.tsx` | Sidebar notebook list with create/rename/delete |
| `components/notebook/NotebookView.tsx` | Tab container: SourceList + ChatPanel + Analysis |
| `components/notebook/SourceList.tsx` | File drop target; indexing progress; status badges |
| `components/notebook/SourceStatusBadge.tsx` | Color-coded status pill (pending/processing/indexed/failed) |
| `components/knowledge-graph/` | KG canvas (XYFlow) + relation type filter panel |
| `components/analysis/TemplateSelector.tsx` | Template buttons + analysis output display |
| `components/batch/BatchPanel.tsx` | Folder picker + operation selector + launch |
| `components/batch/BatchResultTable.tsx` | Per-file result table with export |
| `components/meeting/MeetingPanel.tsx` | Start/stop meeting + segment transcript list |
| `components/search/` | Global search input + result list |
| `components/WorkloadBar.tsx` | Background task progress bar (indexing/NER/KG/batch) |
| `components/ModelDownloadBanner.tsx` | Model download prompt + progress |

---

*Generated: 2026-05-05 | Source: reverse engineering of EchoAI codebase at `/media/riaz/New Volume/echo-AI`*
