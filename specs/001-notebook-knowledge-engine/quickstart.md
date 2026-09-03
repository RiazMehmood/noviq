# Quickstart: Notebook & Knowledge Engine

**Branch**: `001-notebook-knowledge-engine`
**Target**: Developer onboarding — get the feature running locally in < 15 minutes.

---

## Prerequisites

| Tool | Version | Install |
|------|---------|---------|
| Rust | stable (≥ 1.75) | `curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs \| sh` |
| Node.js | ≥ 20 LTS | https://nodejs.org |
| Tauri CLI | 2.x | `cargo install tauri-cli --version "^2.0"` |
| pnpm | ≥ 8 | `npm install -g pnpm` |
| CMake | ≥ 3.20 | Required to build llama.cpp via llama-cpp-2 crate |
| libsqlite3-dev | system | `sudo apt install libsqlite3-dev` (Linux) |

**macOS extras**: `xcode-select --install`
**Windows extras**: MSVC Build Tools (Visual Studio 2022, Desktop C++ workload)

---

## 1. Clone and Install

```bash
git clone <repo-url>
cd echo-AI
git checkout 001-notebook-knowledge-engine

# Install frontend dependencies
pnpm install
```

---

## 2. Download Models

Models are NOT bundled. Download to `models/` at the repo root:

```bash
mkdir -p models

# Orchestrator (Tier 3) — always required
# Qwen2.5-3B-Instruct Q4_K_M (~1.9 GB)
curl -L -o models/qwen2.5-3b-instruct-q4_k_m.gguf \
  https://huggingface.co/Qwen/Qwen2.5-3B-Instruct-GGUF/resolve/main/qwen2.5-3b-instruct-q4_k_m.gguf

# Worker LLM (Tier 4) — required on 16 GB+ machines
# Qwen2.5-7B-Instruct Q4_K_M (~4.5 GB)
curl -L -o models/qwen2.5-7b-instruct-q4_k_m.gguf \
  https://huggingface.co/Qwen/Qwen2.5-7B-Instruct-GGUF/resolve/main/qwen2.5-7b-instruct-q4_k_m.gguf

# Vision model (Tier 5) — optional; only needed for image/scanned-PDF indexing
# Qwen2-VL-2B Q4_K_M (~1.5 GB)
curl -L -o models/qwen2-vl-2b-q4_k_m.gguf \
  https://huggingface.co/Qwen/Qwen2-VL-2B-Instruct-GGUF/resolve/main/qwen2-vl-2b-q4_k_m.gguf
```

The embedding model (`nomic-embed-text-v1.5`) is downloaded automatically by
fastembed-rs on first run to `~/.cache/huggingface/hub/`.

---

## 3. Configure Environment

Create `src-tauri/.env` (not committed):

```env
# Model paths (absolute)
ECHO_MODEL_TIER3=./models/qwen2.5-3b-instruct-q4_k_m.gguf
ECHO_MODEL_TIER4=./models/qwen2.5-7b-instruct-q4_k_m.gguf
ECHO_MODEL_VISION=./models/qwen2-vl-2b-q4_k_m.gguf

# Database path (defaults to app data dir in production)
ECHO_DB_PATH=./data/echoai.sqlite

# Hardware override for testing (optional)
# ECHO_FORCE_TIER=3   # Forces max tier to 3 (simulates 8 GB hardware)
```

---

## 4. Run in Development Mode

```bash
# Start Tauri dev server (starts both Vite frontend + Rust backend)
cargo tauri dev
```

First run takes 2–5 minutes (compiles llama-cpp-2 + Rust dependencies).
Subsequent runs start in ~15 seconds.

The app window opens automatically. The database is created at `ECHO_DB_PATH`
with all migrations applied.

---

## 5. Run Tests

```bash
# Rust unit tests (backend only — no Tauri runtime needed)
cd src-tauri && cargo test

# Integration tests (requires built binary)
cargo tauri build --debug
cargo test --test integration -- --test-threads=1

# Frontend unit tests
pnpm test
```

### Key test scenarios to validate manually:

**P1 — Notebook + Chat**:
1. Create a notebook named "Test"
2. Drag a PDF (≥ 10 pages) into the notebook
3. Wait for `indexed` status (≤ 60s for 50-page PDF)
4. Ask: "What is the main topic of this document?"
5. Verify: answer includes citation with page number

**P2 — Semantic Recall**:
1. Go to Settings → Indexed Folders → Add folder with 10+ mixed files
2. Wait for scanning to complete
3. Open global search → type a conceptual phrase not literally in any file
4. Verify: relevant results surface within 3 seconds

**P3 — Hardware constraint simulation**:
1. Set `ECHO_FORCE_TIER=3` in `.env`, restart app
2. Open a notebook with 2+ documents
3. Click "Compare Sources" analysis template
4. Verify: template runs (not greyed out) and shows quality notice banner

**P4 — Folder revocation**:
1. Add a folder, wait for indexing
2. Search for content from that folder (verify it appears)
3. Revoke folder access
4. Search again (verify content is gone within 5 seconds)

---

## 6. Source Layout

```
echo-AI/
├── src-tauri/                   # Rust backend (Tauri)
│   ├── Cargo.toml
│   ├── src/
│   │   ├── main.rs              # Tauri app entry point
│   │   ├── lib.rs               # Command registration
│   │   ├── commands/            # #[tauri::command] handlers (thin layer)
│   │   │   ├── notebooks.rs
│   │   │   ├── sources.rs
│   │   │   ├── query.rs
│   │   │   ├── search.rs
│   │   │   ├── entities.rs
│   │   │   ├── analysis.rs
│   │   │   ├── batch.rs
│   │   │   └── archive.rs
│   │   ├── services/            # Business logic
│   │   │   ├── indexer.rs       # Document ingestion pipeline
│   │   │   ├── chunker.rs       # 512-token semantic chunking
│   │   │   ├── embedder.rs      # fastembed-rs wrapper
│   │   │   ├── query_router.rs  # Tier 0→4 routing engine
│   │   │   ├── llm.rs           # ModelSlot manager (single slot)
│   │   │   ├── entity_extractor.rs  # Two-pass NER
│   │   │   ├── knowledge_graph.rs   # Graph query logic
│   │   │   ├── analysis_engine.rs   # Template execution
│   │   │   ├── batch_processor.rs   # Background batch jobs
│   │   │   ├── task_budget.rs       # CPU/battery scheduler
│   │   │   ├── file_watcher.rs      # notify crate wrapper
│   │   │   └── archive.rs           # Export/import .echoai
│   │   └── db/
│   │       ├── schema.rs        # sqlx migration runner
│   │       ├── migrations/      # 001_initial_schema.sql, ...
│   │       └── repositories/    # One per entity: notebooks.rs, sources.rs, ...
│   └── tests/
│       └── integration/
│
├── src/                         # SolidJS frontend
│   ├── App.tsx
│   ├── components/
│   │   ├── notebook/            # NotebookView, SourceList, SourceCard
│   │   ├── chat/                # ChatPanel, MessageBubble, CitationChip
│   │   ├── search/              # GlobalSearch, SearchResultCard
│   │   ├── knowledge-graph/     # GraphCanvas, EntityPanel
│   │   ├── analysis/            # TemplateSelector, AnalysisResult
│   │   └── batch/               # BatchPanel, BatchResultTable
│   ├── pages/
│   │   ├── Dashboard.tsx
│   │   ├── NotebookPage.tsx
│   │   ├── SearchPage.tsx
│   │   └── SettingsPage.tsx
│   ├── stores/                  # SolidJS createStore / createSignal
│   │   ├── notebooks.ts
│   │   ├── query.ts
│   │   └── system.ts            # hardware tier, task budget state
│   └── api/                     # Tauri invoke() wrappers (typed)
│       └── index.ts             # re-exports from tauri-commands.ts contracts
│
├── models/                      # GGUF model files (gitignored)
├── data/                        # SQLite database (gitignored)
├── specs/001-notebook-knowledge-engine/
└── tests/
    └── unit/
```

---

## 7. Common Issues

| Symptom | Cause | Fix |
|---------|-------|-----|
| `llama-cpp-2` build fails | CMake not found | Install CMake ≥ 3.20 |
| `sqlite-vec` not loading | Extension not in `LD_LIBRARY_PATH` | Built as static extension; ensure feature flag `bundled` in Cargo.toml |
| App opens but no models found | `.env` paths wrong | Check absolute paths in `ECHO_MODEL_TIER3` |
| Indexing never completes | TaskBudget pausing due to CPU | Set `ECHO_FORCE_TIER=3`, run in idle mode |
| `fastembed-rs` download fails | First run needs internet | Models cached to `~/.cache/huggingface/hub/`; manual download supported |
