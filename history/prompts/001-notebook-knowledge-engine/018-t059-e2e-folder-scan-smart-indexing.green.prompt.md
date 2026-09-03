---
id: 018
title: T059 E2E Prep — Folder Scan Bugs + Smart CPU-Aware Indexing
stage: green
date: 2026-04-16
surface: agent
model: claude-sonnet-4-6
feature: 001-notebook-knowledge-engine
branch: 001-notebook-knowledge-engine
user: Riaz Mehmood
command: check memory and plan for next move / yes (T059) / smart CPU design
labels: ["t059","e2e","indexing","performance","schema","cpu","ram"]
links:
  spec: null
  ticket: null
  adr: null
  pr: null
files:
  - src-tauri/src/db/migrations/002_schema_fixes.sql
  - src-tauri/src/db/repositories/folders.rs
  - src-tauri/src/services/indexer.rs
  - src-tauri/src/commands/folders.rs
tests:
  - Manual E2E: add folder → verify scan → global search → revoke → search empty
---

## Prompt

check memory and plan for next move
→ yes (start T059)
→ and as we process multiple files it consumes cpu usage and ram so design in SMARTLY

## Response snapshot

Checked memory; identified next tasks: T059/T068/T077/T085 (E2E validation) + T091 (regression).

Planned T059 first. Code analysis found 3 pre-existing bugs:

1. `indexed_folders.indexing_status` CHECK constraint missing 'processing' → `update_status("processing")` silently fails
2. `sources.indexing_status` CHECK constraint missing 'unindexed' → `remove_folder_source` silently fails to update status
3. `folder_repo::delete` did not clean `chunk_embeddings` (vec0 virtual table, no FK cascade) → orphaned embeddings after folder revoke

Also addressed user concern about CPU/RAM during multi-file indexing.

**Fixes shipped:**

### 002_schema_fixes.sql (new migration)
- Recreates `indexed_folders` with CHECK including 'processing'
- Recreates `sources` with CHECK including 'unindexed'
- Preserves all data; recreates indexes

### folders.rs (repository)
- `folder_repo::delete`: explicitly deletes `chunk_embeddings` before deleting sources/chunks, preventing orphan bloat in the vec0 virtual table

### indexer.rs (service)
- Added `static ENTITY_EXTRACTION_SEM: OnceLock<Semaphore>` (1 permit)
- Pass 2 entity extraction (LLM inference) now acquires the semaphore before running
- Result: folder scan of N files no longer spawns N concurrent LLM inference tasks — they queue and run one at a time

### folders.rs (command)
- Added adaptive inter-file delay in the scan loop based on `TaskBudget::state()`:
  - Active (user just interacted): 150ms between files
  - Throttled (moderate load): 50ms
  - Idle (system free): 0ms (full speed)
- Prevents CPU from pegging 100% during large folder scans

## Outcome

- ✅ Impact: Fixes 3 schema bugs + prevents CPU/RAM spike from concurrent LLM entity extraction + adds adaptive throttling in folder scan loop
- 🧪 Tests: Manual E2E validation (T059) pending app run
- 📁 Files: 4 files changed (1 new migration, 3 modified)
- 🔁 Next prompts: Run the app to validate T059 E2E; then T068/T077/T085
- 🧠 Reflection: The entity extraction semaphore is the highest-impact change — without it, every file in a folder spawned an uncapped LLM inference background task

## Evaluation notes (flywheel)

- Failure modes observed: vec0 virtual tables silently skip FK CASCADE; OnceLock semaphore pattern is idiomatic for static rate limiters
- Graders run and results (PASS/FAIL): build check pending
- Prompt variant (if applicable): null
- Next experiment: verify semaphore doesn't deadlock if LLM model is absent (acquire → None path via .ok())
