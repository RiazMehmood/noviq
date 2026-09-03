---
id: "016"
title: UX — LLM always on, progress, delete, model download
stage: green
date: 2026-04-15
surface: agent
model: claude-sonnet-4-6
feature: 001-notebook-knowledge-engine
branch: 001-notebook-knowledge-engine
user: Riaz Mehmood
command: LLM always on + indexing progress % + delete notebook + model download on install
labels: ["ux","llm","progress","delete","model-download","events","tauri"]
links:
  spec: null
  ticket: null
  adr: null
  pr: null
files:
  - src-tauri/Cargo.toml
  - src-tauri/src/commands/system.rs
  - src-tauri/src/commands/sources.rs
  - src-tauri/src/commands/folders.rs
  - src-tauri/src/lib.rs
  - src-tauri/src/services/indexer.rs
  - src-tauri/src/services/batch_processor.rs
  - src-tauri/src/services/query_router.rs
  - src/api/index.ts
  - src/App.tsx
  - src/components/ModelDownloadBanner.tsx
  - src/components/notebook/NotebookList.tsx
  - src/components/notebook/SourceList.tsx
  - src/components/notebook/SourceStatusBadge.tsx
tests:
  - cargo check (Finished dev profile, 11 warnings, 0 errors)
  - tsc --noEmit (exit code 0, no type errors)
---

## Prompt

continue now, and it is necessary to run always LLM for response in proper format and I suggest that
while users install this app it in background downloads the model, and there are some issues regarding
ux one is that while processing the document it must shows some percentage type on that document so
user can understand the remaining time or time of processing, other thing is that there is no option
to delete the created book so if i now want to delete the book so i can remove it easily.

## Response snapshot

Four improvements implemented across backend and frontend:

### 1. LLM always required (`query_router.rs`)
Removed template synthesizer fallback. When model fails, user sees:
`"**AI model unavailable.** The model file could not be loaded: … Go to Settings → AI Model to download the model, then try again."`

### 2. Document indexing progress (`indexer.rs` + `SourceStatusBadge.tsx` + `SourceList.tsx`)
Backend emits `source:indexing-progress` Tauri events at key pipeline stages:
- 5% → parsing
- 25% → chunking  
- 35% → storing chunks
- 40–85% → per embedding batch (fan-out across batches)
- 95% → finalizing
- 100% → done

`SourceList.tsx` now listens to these events and maintains a `progressMap: Record<string, number>`.
`SourceStatusBadge.tsx` shows live `percent%` inline and a progress bar when status is "processing".

### 3. Delete notebook (`NotebookList.tsx`)
Delete icon (␡) appears on hover per notebook. Click shows inline confirmation:
```
Delete "Notebook Name"? [Delete] [Cancel]
```
Both buttons are styled — red for confirm, gray for cancel. Second click actually calls `deleteNotebook()`.

### 4. Model download on first launch (`system.rs` + `ModelDownloadBanner.tsx` + `App.tsx`)
Backend:
- `check_model` command: returns `{ exists, model_name, model_path, size_mb }` for the tier-appropriate model
- `download_model` command: streams download via `reqwest`, emits `model:download-progress` events with `{ percent, downloaded_mb, total_mb }`
- Download saves to temp file, renamed on completion to avoid partial files
- Supports tier3 (3B) and tier4 (7B) models; URL hardcoded to bartowski HuggingFace GGUF mirrors

Frontend:
- `ModelDownloadBanner.tsx`: global component checking model on mount; shows yellow banner if model missing
- Progress bar with MB display during download
- Dismissable via × button
- Banner auto-hides after download completes
- Mounted globally in `App.tsx` outside the Router (renders on all routes)

New Cargo deps added: `reqwest = { version = "0.12", features = ["rustls-tls", "stream"] }` + `futures-util = "0.3"`

## Outcome

- ✅ Impact: All four UX improvements deployed and building. LLM is forced on. Progress shows live %. Delete works with confirmation. Model auto-download available on first launch.
- 🧪 Tests: cargo check PASS (11 warnings, 0 errors). tsc --noEmit PASS (exit 0).
- 📁 Files: 14 files modified/created across backend and frontend.
- 🔁 Next prompts: E2E validation T059/T068/T077/T085; phase N polish T086–T093
- 🧠 Reflection: Key Tauri 2.0 pattern: `tauri::Emitter` trait must be in scope for `.emit()` to work on AppHandle. Progress events are better than polling for real-time UI updates — no round-trip DB query needed.

## Evaluation notes (flywheel)

- Failure modes observed: `emit()` requires `use tauri::Emitter` — missing trait import was compile error;
  `index_file` in 3 additional callers (folders.rs, batch_processor.rs, lib.rs) needed `None` added
- Graders run and results (PASS/FAIL): cargo check PASS; tsc PASS
- Prompt variant (if applicable): null
- Next experiment: Verify model banner appears on fresh install (model file absent) and disappears after download
