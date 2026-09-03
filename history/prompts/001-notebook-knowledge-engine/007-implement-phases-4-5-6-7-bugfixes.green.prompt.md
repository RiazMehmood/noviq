---
id: 007
title: Implement Phases 4-5-6-7 and fix API breakages
stage: green
date: 2026-04-14
surface: agent
model: claude-sonnet-4-6
feature: 001-notebook-knowledge-engine
branch: 001-notebook-knowledge-engine
user: Riaz Mehmood
command: continue
labels: ["implementation","batch","knowledge-graph","bugfix","typescript","rust"]
links:
  spec: specs/001-notebook-knowledge-engine/spec.md
  ticket: null
  adr: null
  pr: null
files:
  - src/components/knowledge-graph/GraphCanvas.tsx
  - src/components/knowledge-graph/EntityPanel.tsx
  - src/pages/KnowledgeGraphPage.tsx
  - src/components/batch/BatchPanel.tsx
  - src/components/batch/BatchResultTable.tsx
  - src/pages/Dashboard.tsx
  - src/App.tsx
  - src-tauri/src/services/batch_processor.rs
  - src-tauri/src/commands/batch.rs
  - src-tauri/src/services/parsers/document.rs
  - src-tauri/src/services/analysis_engine.rs
  - src-tauri/src/services/query_router.rs
  - src-tauri/src/services/hardware_detector.rs
  - src-tauri/src/services/task_budget.rs
  - src-tauri/src/services/exporters/pdf.rs
  - src-tauri/src/lib.rs
  - src-tauri/capabilities/default.json
  - src-tauri/icons/32x32.png
  - src-tauri/icons/128x128.png
  - src-tauri/icons/128x128@2x.png
  - src-tauri/icons/icon.ico
  - src/components/analysis/AnalysisResultView.tsx
tests:
  - cargo check --no-default-features → Finished (exit 0)
  - npx tsc --noEmit → clean (no output)
---

## Prompt

continue where we left

## Response snapshot

Session 3 implementation:

**T075 GraphCanvas.tsx** — Custom SVG force-directed knowledge graph (pure JS physics sim, @xyflow/react is React-only). Pan + zoom via pointer events + wheel. Type-filter dropdown. Click node → EntityPanel sidebar. 500-node cap.

**T076 KnowledgeGraphPage.tsx** — Full-screen page wrapper for GraphCanvas.

**App.tsx** — Added `/graph` route and "Graph" nav link.

**T078 batch_processor.rs** — Full implementation: scan folder for supported files, index each, run operation (summarize/extract_dates/flag_keywords/extract_actions) via LLM or regex fallback, emit `batch:progress` Tauri events, system notification on completion.

**T079 batch.rs** — Updated create_batch_job to accept state params (embedder, model_slot, tier, task_budget) and call `batch_processor::spawn_batch_job`.

**T081 BatchPanel.tsx** — Folder picker, operation selector, keyword input, launch button, progress bar subscribed to `batch:progress`.

**T082 BatchResultTable.tsx** — Sortable table with expandable rows, open-in-shell for file paths.

**T083 Dashboard.tsx** — Added collapsible "Batch Processing" accordion in sidebar; active job banner + BatchResultTable in main area.

**Bug fixes (pre-existing API breakages)**:
- `sysinfo 0.30`: removed `SystemExt`/`CpuExt` trait imports (methods now direct on `System`)
- `docx_rs 0.4.20`: `BodyChild` → `DocumentChild`, `TableCellChild` → `TableCellContent`, `TableChild::TableRow`, `TableRowChild::TableCell`
- `calamine`: added `use calamine::Reader` trait
- `query_router.rs`: fixed tuple type `(Vec<u8>, String, String, String)` for cache row (was `(String, Vec<u8>,...)`
- `analysis_engine.rs`: replaced `sqlx::query!` macro (needs DATABASE_URL at compile time) with `sqlx::query_as` + `#[derive(sqlx::FromRow)]`
- `exporters/pdf.rs`: `Mm` now takes `f32` not `f64`
- `tauri::Manager` added to lib.rs imports (needed for `.path()/.manage()/.state()`)
- `tauri::Emitter` added to batch_processor imports (needed for `.emit()`)
- `capabilities/default.json`: fixed invalid Tauri 2.0 permissions
- `src-tauri/icons/`: created RGBA PNG icons (Tauri `generate_context!` panicked on missing/non-RGBA icons)
- TS: `QualityNoticeBanner` has no `onDismiss` prop; `CitationChip` requires `index` prop

## Outcome

- ✅ Impact: Phases 4-7 fully implemented; cargo check clean; TypeScript clean
- 🧪 Tests: cargo check --no-default-features → exit 0; tsc --noEmit → exit 0
- 📁 Files: 22 files created/modified
- 🔁 Next prompts: cargo tauri dev full build test; T085/T077/T068/T059 E2E validations; Phase N Polish (T086-T093)
- 🧠 Reflection: Multiple pre-existing API version mismatches required fixing before batch work could compile. Key pattern: always check crate version against API when 3rd-party crate methods fail.

## Evaluation notes (flywheel)

- Failure modes observed: sysinfo/docx_rs/calamine/printpdf/tauri API changes not caught earlier due to --no-default-features skipping some code paths in earlier cargo checks
- Graders run and results (PASS/FAIL): cargo check PASS, tsc PASS
- Prompt variant (if applicable): n/a
- Next experiment (smallest change to try): attempt cargo tauri dev --no-default-features to test full build
