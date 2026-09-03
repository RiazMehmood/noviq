# EchoAI — Response Speed & Concurrent UX Design

**Date:** 2026-04-25  
**Branch:** 001-notebook-knowledge-engine  
**Status:** APPROVED — ready for implementation planning  
**Author:** Brainstorming session (Riaz + Claude)

---

## 1. Feature Inventory & Decisions

| Feature | Decision |
|---------|----------|
| LLM response delivery | Token streaming via Tauri `chat://token` events — replace `query_notebook` with `stream_query_notebook` |
| Thinking state (0s → first token) | Context-aware narrative using real retrieval metadata: "Found 6 passages in constitution.pdf...", "Composing 3-sentence answer..." |
| Stop button placement | Send button morphs into Stop during streaming (ChatGPT style); input disabled while streaming |
| Stop behaviour | Cancel token; modal appears: "Keep partial response?" [Keep] [Discard] |
| Chat vs background LLM (batch/analysis) | Chat always wins — preempts background LLM; auto-resumes after; shows dismissible toast |
| Chat vs indexing | Already concurrent (non-LLM); add visible "Indexing in progress" badge in chat header |
| Selective document analysis | Per-run picker — collapsed pill "N docs ▾" expands; all pre-selected; resets after each run |

---

## 2. Tauri Event API

All chat streaming events are emitted on named channels. Frontend subscribes via `listen()`.

### Events (backend → frontend)

```
chat://thinking   { stage: "retrieving" | "ranking" | "composing" | "verifying",
                    detail: String }
                  ← emitted before first token; 1 event per RAG pipeline stage

chat://token      { token: String }
                  ← emitted once per generated token during LLM decode

chat://done       { message: MessageDto, quality_notice: Option<QualityNotice> }
                  ← emitted when generation completes normally

chat://cancelled  { partial_text: String }
                  ← emitted when CancellationToken fires; partial_text = tokens so far

chat://error      { message: String }
                  ← emitted on LLM crash or OOM mid-stream

chat://background-paused  { kind: WorkloadKind }
                  ← emitted when Chat preempts a background LLM workload
```

### Commands (frontend → backend)

```
stream_query_notebook(session_id, notebook_id, query_text)  → ()
  Registers Chat workload, runs RAG pipeline, emits events above.

cancel_chat(chat_id: String)  → ()
  Cancels the in-flight Chat CancellationToken.

analyze_selected(notebook_id, source_ids: Vec<String>)  → AnalysisResultDto
  Runs analysis only on the specified sources.
```

---

## 3. Architecture

### Backend (Rust/Tauri)

**`src-tauri/src/services/llm.rs`**
- Add `app: tauri::AppHandle` parameter to `infer_with_cancel()`
- Inside the token decode loop: `app.emit("chat://token", &token_str)`
- Token check already runs per iteration — no additional overhead

**`src-tauri/src/commands/query.rs`**
- Remove `query_notebook` command
- Add `stream_query_notebook`: registers Chat workload → emits `chat://thinking` events per RAG stage → calls `infer_with_cancel` (with AppHandle) → emits `chat://done` or `chat://cancelled`
- Add `cancel_chat(chat_id)`: calls `scheduler.request_cancel(id)`

**`src-tauri/src/services/query_router.rs`**
- `route()` accepts `app: &tauri::AppHandle` and emits `chat://thinking` events at each stage:
  - After embed query: `{ stage: "retrieving", detail: "Searching N passages..." }`
  - After vector/FTS retrieval: `{ stage: "ranking", detail: "Found N passages in <filename>" }`
  - Before LLM call: `{ stage: "composing", detail: "Composing answer..." }`
  - After grounding check: `{ stage: "verifying", detail: "Verifying citations..." }`

**`src-tauri/src/services/scheduler.rs`**
- In `apply_policy_to()`: when a Chat workload cancels a background LLM workload, emit `chat://background-paused { kind }`
- After `chat://done` / `chat://cancelled`: re-queue the cancelled background workload (new WorkloadHandle, same kind)

**`src-tauri/src/commands/analysis.rs`**
- Add `analyze_selected(notebook_id: String, source_ids: Vec<String>)` command
- Filters `analysis_engine` run to only process sources in `source_ids`

### Frontend (SolidJS)

**`src/stores/query.ts`**
- Replace `invoke("query_notebook")` with:
  - `invoke("stream_query_notebook")` to start
  - `listen("chat://thinking", ...)` → update thinking state signal
  - `listen("chat://token", ...)` → append token to streaming bubble
  - `listen("chat://done", ...)` → finalise message, clear streaming state
  - `listen("chat://cancelled", ...)` → show Keep/Discard modal
  - `listen("chat://error", ...)` → show error in bubble
  - `listen("chat://background-paused", ...)` → trigger toast
- Add `cancelChat(chatId)` → `invoke("cancel_chat")`
- Unlisten all listeners on component cleanup

**`src/components/chat/ChatPanel.tsx`**
- Send button morphs to Stop (`⏹ Stop`) when `isStreaming` signal is true
- Input `disabled` while streaming
- Thinking bubble: renders `chat://thinking` detail text with animated dots before first token
- Streaming bubble: tokens append in real-time; cursor blink animation at end
- Keep/Discard modal: appears on `chat://cancelled`; Keep saves partial, Discard removes bubble
- Incomplete message tag: `[incomplete]` shown on kept-partial messages
- Indexing badge: shown in chat header when `WorkloadKind::Indexing` active (from `workload://state` events)

**`src/components/analysis/AnalysisResultView.tsx`**
- Source picker: collapsed pill "N docs ▾" above Analyze button
- Expands to checkbox list on click; all pre-checked
- Analyze button label: "Analyze N Documents →" updates as user checks/unchecks
- On submit: calls `analyze_selected(notebookId, selectedIds)`
- Resets to all-selected after run completes

---

## 4. Data Model

```sql
-- Migration: add is_incomplete column to messages
ALTER TABLE messages ADD COLUMN is_incomplete INTEGER NOT NULL DEFAULT 0;
```

| Column | Type | Purpose |
|--------|------|---------|
| `is_incomplete` | `INTEGER` (0/1) | 1 = kept-partial response stopped mid-generation |

No other schema changes required.

---

## 5. Error Handling

| Scenario | Backend | Frontend |
|----------|---------|----------|
| Stop clicked | Cancel token → emit `chat://cancelled { partial_text }` | Show Keep/Discard modal |
| Keep chosen | Save partial to DB with `is_incomplete=1` | Render bubble + `[incomplete]` tag |
| Discard chosen | Nothing saved | Remove streaming bubble |
| LLM crash / OOM | Emit `chat://error { message }` | Show error bubble: "Response failed — try again" |
| Background preempted | Scheduler cancels token; emits `chat://background-paused { kind }` | Toast: "Analysis paused — tap Resume to restart" + [Resume] button |
| Background resume | User taps [Resume] in toast | Frontend re-invokes the original analysis/batch command; toast clears |
| Listener leak | `unlisten()` called in SolidJS `onCleanup` | No dangling listeners on tab switch |

---

## 6. Scenarios Covered

| Scenario | Behaviour |
|----------|-----------|
| Normal chat | Thinking narrative → tokens stream → done |
| Chat while indexing | Chat runs; indexing continues (non-LLM); badge shown |
| Chat while batch processing | Batch LLM paused; chat gets slot; batch auto-resumes |
| Chat while analyzing documents | Analysis LLM paused; chat gets slot; analysis auto-resumes |
| User stops mid-response | Cancel → Keep/Discard modal → partial saved or discarded |
| Selective document analysis | Per-run checkbox picker; all selected by default; resets after run |
| LLM error mid-stream | Error bubble shown; user can retry |

---

## 7. Key File Changes

| File | Change |
|------|--------|
| `src-tauri/src/services/llm.rs` | Add `app: AppHandle` to `infer_with_cancel`; emit `chat://token` per token |
| `src-tauri/src/services/query_router.rs` | Emit `chat://thinking` events per RAG stage |
| `src-tauri/src/commands/query.rs` | Replace `query_notebook` → `stream_query_notebook` + `cancel_chat` |
| `src-tauri/src/services/scheduler.rs` | Emit `chat://background-paused`; auto-requeue cancelled workloads |
| `src-tauri/src/commands/analysis.rs` | Add `analyze_selected(notebook_id, source_ids)` |
| `src/stores/query.ts` | Replace invoke with event listeners; add cancel |
| `src/components/chat/ChatPanel.tsx` | Streaming bubble, Stop button, Keep/Discard modal, thinking narrative, indexing badge |
| `src/components/analysis/AnalysisResultView.tsx` | Collapsed pill source picker |
| `src/api/index.ts` | Remove `queryNotebook`; add `streamQueryNotebook`, `cancelChat`, `analyzeSelected` |
| DB migration | `ALTER TABLE messages ADD COLUMN is_incomplete INTEGER NOT NULL DEFAULT 0` |

---

## 8. Implementation Order (dependency-safe)

1. **DB migration** — `is_incomplete` column (no code deps)
2. **`llm.rs`** — add AppHandle + `chat://token` emit (self-contained)
3. **`query_router.rs`** — emit `chat://thinking` events (depends on llm.rs change)
4. **`commands/query.rs`** — `stream_query_notebook` + `cancel_chat` (depends on router)
5. **`scheduler.rs`** — `chat://background-paused` + auto-requeue (depends on commands)
6. **`commands/analysis.rs`** — `analyze_selected` (independent of above)
7. **`src/stores/query.ts`** — replace invoke with listeners (depends on backend commands)
8. **`ChatPanel.tsx`** — streaming UI, Stop button, modal, thinking narrative (depends on store)
9. **`AnalysisResultView.tsx`** — source picker (depends on analyze_selected command)
10. **`src/api/index.ts`** — update API bindings (depends on all commands)
