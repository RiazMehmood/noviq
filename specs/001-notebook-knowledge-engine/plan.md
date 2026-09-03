# Implementation Plan: Notebook & Knowledge Engine

**Branch**: `001-notebook-knowledge-engine` | **Date**: 2026-04-13 | **Spec**: [spec.md](spec.md)
**Input**: Feature specification from `/specs/001-notebook-knowledge-engine/spec.md`

---

## Summary

Build App 1 of EchoAI: a fully offline notebook-based knowledge engine that lets users
add local files to named notebooks, ask questions with cited answers, search semantically
across all indexed folders, explore an auto-extracted knowledge graph, run multi-source
analysis templates, and batch-process file collections — all without any network calls,
running on 8–32 GB RAM hardware with graceful capability adaptation.

**Technical approach**: Tauri 2.0 desktop app (Rust backend + SolidJS frontend). All
storage in a single SQLite file (WAL mode) with sqlite-vec for vector search and FTS5
for keyword search. A 5-tier query router escalates from cache → SQL → semantic →
3B LLM → 7B LLM, loading models on demand with a 90-second idle eviction. A Tokio
TaskBudget scheduler ensures background indexing never competes with user activity.

---

## Technical Context

**Language/Version**: Rust stable (≥ 1.75) — backend; TypeScript 5.x + SolidJS 1.x — frontend
**Primary Dependencies**:
- `tauri` 2.x (desktop framework)
- `llama-cpp-2` (Rust FFI to llama.cpp for Tier 3/4 LLM inference)
- `fastembed` (Rust, ONNX-based embeddings — nomic-embed-text-v1.5 / all-MiniLM fallback)
- `sqlx` 0.7+ (async SQLite driver with compile-time query checking)
- `sqlite-vec` (loadable SQLite extension for vector search — Tier 2)
- `whisper-rs` (Rust bindings for whisper.cpp — audio transcription)
- `tokio` 1.x (async runtime, task scheduling)
- `notify` 6.x (cross-platform filesystem watcher)
- `sysinfo` (CPU/battery monitoring for TaskBudget)
- `lopdf` (pure-Rust PDF text extraction)
- `docx-rs` (DOCX parsing)
- `calamine` (XLSX/XLSM parsing)
- `zip` (archive read/write for .echoai format)
- `solidjs` + `@solidjs/router` + `tailwindcss` (frontend)

**Storage**: Single SQLite file (WAL mode) + sqlite-vec extension + FTS5 virtual tables.
No external DB processes. See `data-model.md`.

**Testing**: `cargo test` (Rust unit + integration); `vitest` (frontend); Tauri integration tests.

**Target Platform**: Windows 10+, macOS 12+, Ubuntu 20.04+ (via Tauri 2.0 cross-platform build)

**Project Type**: Desktop application (Tauri: Rust backend process + WebView frontend)

**Performance Goals**:
- Tier 0–2 queries (cache/FTS/vec): < 3 seconds on 8 GB RAM (SC-002, SC-005)
- Document processing: ≤ 60 seconds for 50-page PDF to "ready" state (SC-001 / US1)
- Background indexing: CPU ≤ 25% during user-active hours (SC-004)
- Entity queries: < 1 second (pure SQL, no inference) (US4 acceptance scenario)
- Folder revocation: complete data removal ≤ 5 seconds (SC-009)

**Constraints**:
- Peak RAM (16 GB machine): ≤ 12 GB — orchestrator (1.9 GB) + worker LLM (4.5 GB)
  + embedder (0.2 GB) + SQLite/app (1.5 GB) + OS headroom (3.9 GB)
- 8 GB hardware tier: Tier 1–3 only; Tier 4/5 demand-loaded only when memory permits;
  analysis templates still available with quality notice (FR-027a)
- Zero external network calls after model download (Principle I)
- Q4_K_M minimum quantization for all GGUF models (Principle III)
- No Python runtime anywhere in the execution path (Principle VI)

**Scale/Scope**: 10,000+ indexed files, 500-entity graph view cap (paginated beyond),
single-user, no authentication, offline-first.

---

## Constitution Check

*GATE: Must pass before Phase 0 research. Re-check after Phase 1 design.*

| Principle | Check | Status |
|-----------|-------|--------|
| I. Privacy-First, Offline-Always | Zero network calls in feature scope; FR-035 + SC-008 enforce this; whisper/llama/fastembed all local | ✅ |
| II. Tiered Intelligence | Query router implements Tier 0→1→2→3→4→5; RRF merging of Tier 1+2 results (AD-6); Context Builder (AD-6); Grounding Validator post-processor (AD-8); Distil-to-SQL for entity queries; Semantic Cache for session hits | ✅ |
| III. Single Model Slot | ModelSlot singleton; 90s idle eviction; RAM budget calculated at 12 GB peak on 16 GB machine | ✅ |
| IV. SQLite Single Source of Truth | All state in SQLite; sqlite-vec + FTS5 embedded; no Chroma/Qdrant/external DB | ✅ |
| V. Pending Actions Queue | App 1 has no Dangerous-tier agent operations — all file operations are read-only; user-initiated delete (FR-036) is a settings action, not an agent action | ✅ |
| VI. Persona-as-Config | No agent personas in App 1; no Python orchestration anywhere | ✅ |
| VII. Hardware-Adaptive | Hardware detected on startup; ModelSlot configures tier limit; FR-027a keeps templates available on all tiers with quality notice; TaskBudget prevents thermal issues | ✅ |
| VIII. Incremental Feature-App | Scoped entirely to App 1; shared infrastructure (SQLite schema, ModelSlot, embedder) is the only coupling point with future App 2 | ✅ |

**All 8 gates pass. No violations to justify.**

---

## Project Structure

### Documentation (this feature)

```text
specs/001-notebook-knowledge-engine/
├── spec.md              # Feature specification (fully clarified)
├── plan.md              # This file
├── research.md          # Phase 0 decisions (chunking, models, entity extraction, etc.)
├── data-model.md        # Full SQLite schema with all tables, indexes, constraints
├── quickstart.md        # Developer onboarding guide
├── checklists/
│   └── requirements.md  # Spec quality checklist (all items pass)
├── contracts/
│   └── tauri-commands.ts  # Full Tauri IPC contract (TypeScript interfaces)
└── tasks.md             # Phase 2 output (/sp.tasks command — NOT created here)
```

### Source Code (repository root)

```text
echo-AI/
├── src-tauri/                        # Rust backend (Tauri process)
│   ├── Cargo.toml
│   ├── tauri.conf.json
│   ├── src/
│   │   ├── main.rs                   # Tauri builder entry point
│   │   ├── lib.rs                    # Command registration + plugin setup
│   │   ├── commands/                 # Thin #[tauri::command] handlers
│   │   │   ├── notebooks.rs          # create, list, get, rename, delete
│   │   │   ├── sources.rs            # add, remove, list, retry
│   │   │   ├── query.rs              # create_session, query_notebook, history
│   │   │   ├── search.rs             # global_search
│   │   │   ├── entities.rs           # query_entities, relationships, graph
│   │   │   ├── analysis.rs           # run_analysis, export_analysis
│   │   │   ├── batch.rs              # create_batch_job, get_job, get_results
│   │   │   ├── archive.rs            # export_notebook, import_notebook
│   │   │   ├── folders.rs            # add_indexed_folder, list, revoke
│   │   │   ├── audit.rs              # get_audit_log
│   │   │   └── system.rs             # get_hardware_tier
│   │   ├── services/                 # Business logic (no Tauri dependency)
│   │   │   ├── indexer.rs            # Ingestion pipeline: parse→chunk→embed→store
│   │   │   ├── chunker.rs            # 512-token semantic boundary chunking
│   │   │   ├── embedder.rs           # fastembed-rs wrapper (hardware-adaptive model)
│   │   │   ├── query_router.rs       # 5-tier routing engine
│   │   │   ├── llm.rs                # ModelSlot: single-slot load/unload/90s timer
│   │   │   ├── entity_extractor.rs   # Two-pass NER (regex + LLM)
│   │   │   ├── knowledge_graph.rs    # Graph SQL query builder
│   │   │   ├── analysis_engine.rs    # Template runner + quality notice logic
│   │   │   ├── batch_processor.rs    # Background batch job executor
│   │   │   ├── task_budget.rs        # TaskBudget: Active/Throttled/Idle state machine
│   │   │   ├── file_watcher.rs       # notify-based watcher → re-index triggers
│   │   │   ├── archive.rs            # .echoai ZIP export/import
│   │   │   └── hardware_detector.rs  # RAM/VRAM/NPU detection → HardwareTier
│   │   └── db/
│   │       ├── schema.rs             # sqlx migrate!() runner
│   │       ├── migrations/
│   │       │   └── 001_initial_schema.sql
│   │       └── repositories/         # One file per entity type
│   │           ├── notebooks.rs
│   │           ├── sources.rs
│   │           ├── chunks.rs
│   │           ├── sessions.rs
│   │           ├── entities.rs
│   │           ├── batch_jobs.rs
│   │           └── audit.rs
│   └── tests/
│       └── integration/
│           ├── test_indexing_pipeline.rs
│           ├── test_query_router.rs
│           └── test_revocation.rs
│
├── src/                              # SolidJS frontend
│   ├── App.tsx
│   ├── components/
│   │   ├── notebook/
│   │   │   ├── NotebookList.tsx
│   │   │   ├── NotebookView.tsx
│   │   │   ├── SourceList.tsx
│   │   │   └── SourceStatusBadge.tsx
│   │   ├── chat/
│   │   │   ├── ChatPanel.tsx
│   │   │   ├── MessageBubble.tsx
│   │   │   ├── CitationChip.tsx
│   │   │   └── QualityNoticeBanner.tsx
│   │   ├── search/
│   │   │   ├── GlobalSearchBar.tsx
│   │   │   └── SearchResultCard.tsx
│   │   ├── knowledge-graph/
│   │   │   ├── GraphCanvas.tsx       # D3 or xyflow-based force graph
│   │   │   └── EntityPanel.tsx
│   │   ├── analysis/
│   │   │   ├── TemplateSelector.tsx
│   │   │   └── AnalysisResultView.tsx
│   │   └── batch/
│   │       ├── BatchPanel.tsx
│   │       └── BatchResultTable.tsx
│   ├── pages/
│   │   ├── Dashboard.tsx
│   │   ├── NotebookPage.tsx
│   │   ├── SearchPage.tsx
│   │   └── SettingsPage.tsx          # Folders, audit log, hardware info
│   ├── stores/
│   │   ├── notebooks.ts
│   │   ├── query.ts
│   │   └── system.ts
│   └── api/
│       └── index.ts                  # Typed invoke() wrappers
│
├── models/                           # GGUF files (gitignored)
├── data/                             # SQLite DB (gitignored)
├── specs/
└── tests/
    └── unit/
```

**Structure Decision**: Tauri desktop architecture (Option 2 variant — Rust backend
process with web frontend, not a traditional web app). `src-tauri/` is the Rust
backend; `src/` is the SolidJS frontend. No `backend/` + `frontend/` split needed —
Tauri's project structure integrates both.

---

## Architecture Decisions

### AD-1: Tiered Query Router

The `query_router.rs` service implements a rule-based classifier that routes each
incoming query to the cheapest satisfying tier:

```
1. Compute embedding of query (always — needed for Tier 0 and Tier 2)
2. Check Tier 0: cosine_similarity(query_embedding, cached) ≥ 0.92 → return cached
3. Check Tier 1: query contains entity-pattern (name/date/amount regex) → FTS5 SQL
4. Check Tier 2: semantic query → sqlite-vec ANN search; if top result < 0.7 confidence → escalate
   4a. RRF merge: if both Tier 1 and Tier 2 produced results, merge via RRF(k=60):
       score = Σ 1/(k + rank_i) across both lists; unified top-K forwarded to Context Builder
5. Context Builder (AD-6): select top-8 unique chunks, dedup, fit context window
6. Check Tier 3: escalate to orchestrator (3B) with grounding prompt (AD-7);
   if response signals insufficient context → escalate
7. Check Tier 4: escalate to worker LLM (7B); on 8 GB hardware → Tier 3 with quality notice
8. Grounding Validator (AD-8): post-process LLM response before returning to caller
```

**Confidence thresholds are configurable** in `config.toml` (not hardcoded).

### AD-2: ModelSlot Singleton

A single `Arc<Mutex<ModelSlot>>` lives in Tauri's managed state. It holds at most
one `LoadedModel` (either Tier 3 or Tier 4). Tier 5 (vision) is a separate
`Arc<Mutex<VisionSlot>>` — vision inference is always short-lived and swap-safe.

```
ModelSlot {
    current: Option<LoadedModel>,
    idle_timer: Option<JoinHandle>   // 90s Tokio timer; reset on each use
}
```

Loading a model that is not the current one drops the current model first (synchronous
drop to free GGUF memory before allocating new model).

### AD-3: TaskBudget State Machine

Three states: `Active` → `Throttled` → `Idle`
- Transitions triggered by system metrics polled every 5 seconds via `sysinfo`.
- All background work items submitted to a Tokio `mpsc` channel with priority u8.
- Background worker tasks consume from the channel only when TaskBudget permits
  their priority level.
- User interaction events (keypress, mouse move) sent from frontend via Tauri event
  system → reset the `Active` state timer.

### AD-4: Entity Extraction Two-Pass

Pass 1 (synchronous, during chunking):
- Regex patterns for: ISO dates, dd/mm/yyyy dates, monetary amounts ($X,XXX), email addresses
- Output → `entities` table directly (fast, zero model cost)

Pass 2 (async background, post-indexing):
- Each chunk sent to orchestrator (Tier 3) with a structured extraction prompt
- Output constrained via GBNF grammar to `{ "entities": [{"type": ..., "value": ...}] }`
- Deduplicates against existing entities before insert

### AD-5: Archive Format (.echoai)

```
notebook-<name>-<unix-ts>.echoai  (renamed ZIP)
├── manifest.json     { "version": "1.0", "notebook_id": "...", "created_at": ..., "source_count": N }
├── sources/          original files verbatim (preserves all content even if paths change)
│   └── <source_id>.<ext>
└── db.sqlite         notebook-scoped export (only rows belonging to this notebook)
```

Import procedure:
1. Unzip to temp dir
2. Read `manifest.json`; check schema version
3. If version mismatch: import sources only, re-index from scratch
4. If version matches: merge `db.sqlite` rows into main database (conflict = skip)
5. Clean up temp dir

### AD-6: Context Builder

Runs between retrieval (Tier 1/2) and LLM inference (Tier 3/4). Implemented as
`build_context()` in `query_router.rs`.

```
fn build_context(
    ranked: Vec<(ChunkId, f32)>,   // RRF-ranked list from AD-1 step 4a
    pool: &SqlitePool,
    max_tokens: usize,             // model context window − 512 reserved for prompt + response
) -> Vec<ContextChunk>

ContextChunk { chunk_id, text, source_file, page_number, token_count }
```

**Selection rules**:
- K = 8 (default); configurable in `config.toml` as `context.top_k`
- Deduplicate by `chunk_id` — same chunk appearing in both FTS5 and vec lists counts once
- Walk ranked list top-to-bottom; accumulate `token_count`; stop when next chunk would
  exceed `max_tokens` budget or K is reached
- Minimum 1 chunk always included even if it alone exceeds budget (truncate text)

**Traceability**: source_file + page_number carried through to citation mapping.
The numbered context list passed to LLM uses format `[N] <text> — (source_file, p. page)`.

### AD-7: LLM System Prompt (Grounding Template)

All Tier 3/4 inferences use a versioned constant `GROUNDING_SYSTEM_PROMPT` defined
in `src-tauri/src/services/llm.rs`. No runtime string construction permitted.

```
const GROUNDING_SYSTEM_PROMPT: &str = "
You are a document assistant. Answer the user's question using ONLY the numbered
context passages provided below.

Rules you MUST follow:
1. Do NOT use any knowledge from your training data. Use only the passages below.
2. Every factual claim MUST include a citation in the format [source_file, page N].
3. If the answer is not present in the passages, respond with this exact phrase:
   \"No relevant information found in provided documents\"
4. Do not speculate, infer beyond the text, or fill gaps from prior knowledge.
";
```

**Temperature = 0.1** for all grounding queries (determinism over creativity).
Template version is logged in audit_log on each LLM call for auditability.

### AD-8: Post-Processing Grounding Validator

Runs immediately after LLM inference returns, before the response is returned to
the caller. Implemented as `validate_grounding()` in `query_router.rs`.

```
fn validate_grounding(
    response: &str,
    context_chunks: &[ContextChunk],
) -> GroundingResult

GroundingResult {
    is_grounded: bool,
    citation_count: u32,
    uncited_sentences: Vec<String>,
    fallback_triggered: bool,
    grounding_score: f32,           // citation_count / total_factual_sentences
}
```

**Validation steps** (rule-based, no LLM):
1. Split response into sentences
2. Identify factual sentences (non-question, non-greeting, > 5 words)
3. For each factual sentence: check for citation marker `[<source>, page N]`
4. For each citation found: verify `<source>` matches a `source_file` in context_chunks
5. If `uncited_sentences` is non-empty OR any citation references a non-context source:
   → replace response with fallback string, set `fallback_triggered = true`
6. Append `GroundingResult` to audit_log (event_type = "grounding_validation")

**Latency budget**: validator runs sync, targets < 50 ms (pure string ops, no I/O).
Fallback always safe — prefer false negative over hallucinated answer.

---

## Performance Budgets

### Per-Operation Targets

| Operation | Hardware | Target | How Achieved |
|-----------|----------|--------|--------------|
| Tier 0 cache lookup | Any | < 10 ms | In-memory HashMap |
| Tier 1 FTS5 search | 8 GB | < 500 ms | SQLite FTS5 index |
| Tier 2 sqlite-vec search | 8 GB | < 2 s | SIMD-accelerated ANN |
| Tier 3 LLM synthesis | 8 GB (CPU) | < 15 s | 3B Q4_K_M, 4096 ctx |
| Tier 4 LLM synthesis | 16 GB iGPU | < 30 s | 7B Q4_K_M, GPU layers |
| Entity query (SQL) | Any | < 1 s | Indexed SQLite query |
| 50-page PDF indexing | Any | < 60 s | Background pipeline |
| Folder revocation | Any | < 5 s | Cascading DELETE + tx |

### RAM Budget (16 GB machine, peak load)

```
Orchestrator 3B Q4_K_M (warm)      1.9 GB
Worker LLM 7B Q4_K_M (active)      4.5 GB
nomic-embed embedder (always)       0.2 GB
SQLite page cache                   0.5 GB
App + WebView baseline              1.0 GB
OS headroom                         3.9 GB
─────────────────────────────────────────
Peak total                         12.0 GB  ✓ within 16 GB
```

### 8 GB Machine Budget

```
Orchestrator 3B Q4_K_M (warm)      1.9 GB
nomic-embed (or MiniLM fallback)    0.2 GB
SQLite + App + OS                   5.5 GB
─────────────────────────────────────────
Total                               7.6 GB  ✓ within 8 GB (no Tier 4 loaded)
```

---

## Complexity Tracking

> No constitution violations. Table not required.

---

## Risks & Mitigations

| Risk | Blast Radius | Mitigation |
|------|-------------|------------|
| sqlite-vec ANN query time exceeds 3s on 10k files | SC-005 fails | Add HNSW index in sqlite-vec once available (2026); interim: limit vec search to 5k most-recent files if latency budget exceeded |
| llama-cpp-2 compilation fails on target platform | All Tier 3/4 features broken | Wrap in feature flag; app ships without LLM support and degrades to Tier 2 max until model loads |
| fastembed-rs first-run download requires internet | Violates Principle I on first boot | Bundle all-MiniLM-L6-v2 (22 MB) in the installer as minimum fallback; nomic-embed downloaded on first run if online |
| scanned PDF vision model not available on 8 GB | FR-008 unmet for scanned docs | Mark scanned-PDF sources as `failed` with reason "Vision model unavailable on this hardware"; user informed clearly |

---

## Follow-Up ADRs Recommended

📋 **Architectural decision detected**: Single `ModelSlot` vs sidecar process for LLM inference — crash isolation tradeoffs. Run `/sp.adr model-hosting-isolation` before implementation begins.

📋 **Architectural decision detected**: Embedding model selection (nomic-embed-text-v1.5 vs hardware-adaptive fallback to all-MiniLM) — affects sqlite-vec schema dimension, migration strategy. Run `/sp.adr embedding-model-selection`.
