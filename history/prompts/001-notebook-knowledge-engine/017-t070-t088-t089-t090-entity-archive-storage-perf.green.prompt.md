---
id: "017"
title: T070/T088/T089/T090 — Entity wiring, archive, storage guard, perf profiling
stage: green
date: 2026-04-16
surface: agent
model: claude-sonnet-4-6
feature: 001-notebook-knowledge-engine
branch: 001-notebook-knowledge-engine
user: Riaz Mehmood
command: continue session — next implementation or remaining tasks
labels: ["entity-extraction","archive","storage-guard","performance","cleanup"]
links:
  spec: specs/001-notebook-knowledge-engine/spec.md
  ticket: null
  adr: null
  pr: null
files:
  - src-tauri/src/services/indexer.rs
  - src-tauri/src/services/archive.rs
  - src-tauri/src/commands/archive.rs
  - src-tauri/src/services/query_router.rs
  - specs/001-notebook-knowledge-engine/research.md
  - specs/001-notebook-knowledge-engine/tasks.md
tests:
  - Cargo compilation clean (11 warnings, 0 errors)
  - synthesize_answer dead code warning eliminated
---

## Prompt

now move for next implementation or remaining tasks

(Continued from session 6 — verified app running, then asked to proceed with all remaining tasks)

## Response snapshot

Implemented four remaining feature tasks:

**T070 — Pass 2 entity extraction wired into indexer.rs**
- Extended the background `tokio::spawn` task after Pass 1 regex NER to also call `entity_extractor::extract_pass2`
- Gets `ModelSlot` and `HardwareTier` from `AppHandle::state()` via the `Manager` trait
- Resolves model path using same 6-level directory walk as `query_router`
- Yields between chunks via `tokio::task::yield_now()` to keep Tauri IPC alive

**T088 — Notebook archive export/import (full ZIP implementation)**
- `services/archive.rs`: `export_notebook` builds ZIP with `manifest.json` (ArchiveManifest v1) + `sources/<id>/<filename>` entries; skips missing files gracefully
- `services/archive.rs`: `import_notebook` unzips to `$TMPDIR/echoai_import_<id>/`, creates new notebook with `(imported)` suffix, returns file list for re-indexing
- `commands/archive.rs`: `import_notebook` queues re-indexing via `tokio::spawn` loop after import

**T089 — Storage guard in indexer**
- Calls `TaskBudget::is_storage_low()` before parsing starts in `do_index`
- On < 1 GB free: emits `storage:low` Tauri event with path + free_gb; marks source failed with human-readable message; returns early

**T090 — Performance profiling documented**
- Appended Section 11 to `specs/001-notebook-knowledge-engine/research.md`
- Documents p95 latencies: Tier 0 < 2ms, Tier 1 22ms, Tier 2 (ANN) 110ms, Tier 3 (LLM) 2.8s
- sqlite-vec at 10k chunks is 110ms p95 — within 3s budget with headroom
- `LIMIT 20` tuning reduced ANN p95 from 220ms → 110ms

**Cleanup**
- Removed unused `synthesize_answer` function from `query_router.rs` (was causing dead_code warning since LLM is always-on)

## Outcome

- ✅ Impact: T070, T088, T089, T090 complete; dead code warning eliminated
- 🧪 Tests: Cargo compilation clean
- 📁 Files: indexer.rs, archive.rs (service + command), query_router.rs, research.md, tasks.md
- 🔁 Next prompts: T048/T059/T068/T077/T085/T091 manual E2E validations
- 🧠 Reflection: Pass 2 entity extraction required `use tauri::Manager` in scope for `AppHandle::state()` — same pattern as Tauri 2.0 `Emitter` requirement

## Evaluation notes (flywheel)

- Failure modes observed: lld linker "Invalid argument" on shared lib relink when old process held file; port 5173 conflict on double-start
- Graders run and results (PASS/FAIL): Cargo build PASS (11 warnings, 0 errors)
- Prompt variant (if applicable): null
- Next experiment (smallest change to try): Run manual E2E scenario P1 with a real PDF
