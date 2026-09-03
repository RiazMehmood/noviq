---
id: "024"
title: "Streaming UX T5–T13 Implementation"
stage: green
date: "2026-04-25"
surface: agent
model: claude-sonnet-4-6
feature: "001-notebook-knowledge-engine"
branch: "001-notebook-knowledge-engine"
user: "Riaz Mehmood"
command: "ok start remaining tasks"
labels: ["streaming", "ux", "rust", "solidjs", "scheduler", "analysis"]
links:
  spec: "null"
  ticket: "null"
  adr: "null"
  pr: "null"
files:
  - "src-tauri/src/commands/query.rs"
  - "src-tauri/src/services/scheduler.rs"
  - "src-tauri/src/services/analysis_engine.rs"
  - "src-tauri/src/commands/analysis.rs"
  - "src-tauri/src/lib.rs"
  - "src/api/index.ts"
  - "src/stores/query.ts"
  - "src/components/chat/ChatPanel.tsx"
  - "src/components/chat/MessageBubble.tsx"
  - "src/components/analysis/TemplateSelector.tsx"
tests:
  - "cargo test --features llm (scheduler tests)"
---

## Prompt

ok start remaining tasks

## Response

Implemented T5–T13 of the response-speed concurrent UX plan:

**T5** — Replaced `query_notebook` with `stream_query_notebook` + `cancel_chat` + `save_partial_message` in `commands/query.rs`. Added `is_incomplete` to `MessageDto`. Added event payload structs (ThinkingEvent, DoneEvent, CancelledEvent, ErrorEvent).

**T6** — Updated `lib.rs` invoke_handler registrations: stream_query_notebook, cancel_chat, save_partial_message, analyze_selected.

**T7** — Added `BackgroundPaused` variant to `SchedulerEvent`. Made `Scheduler.app` an `Option<tauri::AppHandle>`. Emits `chat://background-paused` when Chat preempts an LLM workload.

**T8** — Added `fetch_top_chunks_selected` and `run_template_selected` to `analysis_engine.rs`.

**T9** — Added `analyze_selected` command to `commands/analysis.rs`.

**T10** — Updated `api/index.ts`: added `is_incomplete` to Message, added 7 event payload types, replaced `queryNotebook` with streaming bindings + `analyzeSelected`.

**T11** — Rewrote `stores/query.ts` with streaming state (isStreaming, streamingText, thinkingText, chatId, showKeepDiscard, partialText, backgroundPausedKind). Uses `listen()` event pattern.

**T12** — Updated `ChatPanel.tsx`: streaming bubble with thinking narrative, Stop/Send morph button, Keep/Discard modal, background-paused toast.

**T13** — Added source picker pill to `TemplateSelector.tsx` (plan referenced wrong file). Loads indexed sources on mount, shows "N of N docs" pill, expandable checkbox list, uses `analyzeSelected`.

**Key bug fixes during implementation:**
- `MessageDto` needed `Clone` derive for `DoneEvent`
- `Scheduler::new()` takes `Option<tauri::AppHandle>` for test compatibility
- `app.handle().clone()` → `app.clone()` (app is already AppHandle in setup_models_and_watchers)

All 9 commits pushed to branch. cargo check clean. Tests running.

## Outcome

All T5–T13 committed. Backend: cargo check clean. Frontend: tsc clean. Tests running at session end.
