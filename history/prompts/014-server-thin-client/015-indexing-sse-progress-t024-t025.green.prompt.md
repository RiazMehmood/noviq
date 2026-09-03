---
id: 015
title: US4 indexing concurrency + SSE progress (T024/T025)
stage: green
date: 2026-06-26
surface: agent
model: claude-opus-4-8
feature: 014-server-thin-client
branch: 014-server-thin-client
user: Riaz Mehmood
command: complete these tasks so we can move to gpu vm tasks
labels: [F014, US4, sse, indexing, eventsource]
links:
  spec: specs/014-server-thin-client/spec.md
  ticket:
  adr: history/adr (ADR-0017)
  pr:
files:
  - src-tauri/src/services/indexer.rs — broadcast channel + publish in emit_progress
  - src-tauri/src/commands/sources.rs — extract index_sources service fn
  - src-tauri/src/commands/folders.rs — extract index_folder service fn
  - src-tauri/src/api/index_events.rs (new) — GET /api/index/events SSE
  - src-tauri/src/api/documents.rs — add_sources/add_indexed_folder triggers
  - src-tauri/src/api/mod.rs, router.rs — register + mount
  - src/api/transport.ts — onIndexProgress (Tauri listen | EventSource)
  - src/components/notebook/SourceList.tsx — rewired to onIndexProgress
tests:
  - cargo check --features server --bin noviq-server (exit 0)
  - cargo check --lib (desktop, exit 0)
  - cargo test --features server --test api_admin --no-run (compiles)
  - tsc --noEmit (clean)
---

## Prompt

complete these tasks so we can move to gpu vm tasks  (T024 + T025, full vertical)

## Response snapshot

Implemented the indexing-progress SSE vertical. Found indexing progress already
funnels through one AppHandle-optional fn (`indexer::emit_progress`), so added a
process-wide `broadcast::Sender<IndexingProgress>` it publishes to alongside the
Tauri event. New `GET /api/index/events` SSE handler streams it; authenticated by
`?token=` query (EventSource can't send headers) so it's mounted in the public
subtree. Extracted `index_sources`/`index_folder` service fns (`Option<AppHandle>`)
and added `POST /api/add_sources`/`add_indexed_folder` triggers. No AppState
changes needed (`_task_budget` unused; FileWatcher stays in the desktop command).
Frontend: `onIndexProgress` in transport.ts (Tauri listen | EventSource), wired
into SourceList.tsx. Single build at end — all four checks green.

## Outcome

- ✅ Impact: browser clients get live per-file indexing progress + a headless
  trigger; desktop behavior unchanged. All local F014 tasks complete.
- 🧪 Tests: server bin + desktop lib + api_admin compile; tsc clean.
- 📁 Files: 1 new (index_events.rs) + 7 edited.
- 🔁 Next prompts: GPU-VM tasks — T026–T028 (cuda build/n_gpu_layers/latency),
  T035 (deploy rollback), T051 (runbook), T052 (acceptance), run api_admin green.
- 🧠 Reflection: checking the actual emit path first turned a "big vertical" into
  a surgical change — the centralized Option<AppHandle> seam did the heavy lifting.

## Evaluation notes (flywheel)

- Failure modes observed: EventSource header limitation (solved via query-token);
  Read-before-Edit gate on cat-opened files.
- Graders run and results (PASS/FAIL): server-bin PASS; desktop lib PASS;
  api_admin --no-run PASS; tsc PASS.
- Prompt variant (if applicable): n/a
- Next experiment (smallest change to try): on the VM, run api_admin green + smoke
  the SSE stream end-to-end (curl the token'd endpoint during a real index).
