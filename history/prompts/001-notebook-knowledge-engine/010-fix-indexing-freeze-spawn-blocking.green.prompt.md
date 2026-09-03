---
id: "010"
title: Fix Indexing Pipeline Freeze — spawn_blocking for CPU Work
stage: green
date: 2026-04-15
surface: agent
model: claude-sonnet-4-6
feature: 001-notebook-knowledge-engine
branch: 001-notebook-knowledge-engine
user: Riaz Mehmood
command: user message — fix app freeze on document upload
labels: ["performance", "bug-fix", "async", "spawn-blocking", "indexer"]
links:
  spec: "null"
  ticket: "null"
  adr: "null"
  pr: "null"
files:
  - src-tauri/src/services/indexer.rs
  - src-tauri/src/commands/sources.rs
  - src-tauri/src/lib.rs
  - src-tauri/src/services/entity_extractor.rs
  - src-tauri/src/services/batch_processor.rs
tests: []
---

## Prompt (verbatim)

let us continue working on project, get info from memory where we were, i just want to tell you that in previous sessions, we try to launch the app it runs successfully, then while we create notebook and upload a document it go in the state of processing, while it processes cursor ide and app it self give warning of not responding and i clicks on wait but in the end both app and cursor closed, SO THIS IS CRITICAL YOU NEED TO UNDERSTAND THE ROOT CAUSE OF IT, check HARDWARE, CODE, any infinite loop, bug thoroughly first and MAKE IT SMOOTH, dont attempt multiple times to run app and stuck, first plan how you can resolve this then research the code and other causes then go for testing, at one go we need to solve this issue, cause while it closes app and cursor we missed our progress and dont return on the same point. so work SMARTLY AND IN STEPS.

## Response (key output)

### Root Cause Analysis

The app froze because **CPU-heavy synchronous work ran on tokio async worker threads**, starving the entire tokio runtime (including Tauri's IPC/event loop).

**Issue #1 — CRITICAL**: `embedder.embed(texts)` in `indexer.rs` is a synchronous ONNX/fastembed inference call running on ~300 chunks inside `tokio::spawn`. This blocked all tokio worker threads for minutes, freezing the Tauri UI event loop → "not responding".

**Issue #2 — HIGH**: 300 individual DB transactions for embedding upserts (each its own WAL flush after embedding).

**Issue #3 — HIGH**: Entity extraction ran inline — 300 chunks × 4 regex compiles + 3 DB writes each, all blocking the "indexed" status update.

**Issue #4 — MEDIUM**: PDF parsing (lopdf + pdftotext subprocess) also ran synchronously on the async thread.

### Fixes Applied

1. **`indexer.rs` — complete rewrite**:
   - PDF/document parsing moved to `tokio::task::spawn_blocking`
   - SHA-256 hash computation moved to `tokio::task::spawn_blocking`
   - `embedder.embed(texts)` moved to `tokio::task::spawn_blocking` — THE CRITICAL FIX
   - Embedding upserts batched in a single transaction (300→1 WAL flush)
   - Entity extraction moved to a deferred `tokio::spawn` (runs after "indexed" status is set)
   - `index_file` and `do_index` signatures changed: `embedder: Arc<EmbedderState>` (was `&EmbedderState`)

2. **`sources.rs`**: Updated `add_sources` and `retry_failed_source` to pass `Arc::clone(&embedder_ref)`.

3. **`lib.rs`**: Updated file watcher call to pass `Arc::clone(&embedder2)`.

4. **`batch_processor.rs`**: Updated `run_job` and `process_file` to accept `&Arc<EmbedderState>` and pass `Arc::clone(embedder)` to `index_file`.

5. **`entity_extractor.rs`**: Cached all 4 compiled regexes with `OnceLock` — avoids re-compilation per chunk in the background task.

### Result

`cargo check` — zero errors, only pre-existing warnings. App should now remain fully responsive during document indexing since the heavy ML inference is offloaded to the blocking thread pool.
