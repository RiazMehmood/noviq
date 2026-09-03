---
id: 010
title: T009b AppHandle-optional seam for headless chat
stage: refactor
date: 2026-06-25
surface: agent
model: claude-opus-4-8
feature: 014-server-thin-client
branch: 015-bureaucrat-chat
user: engr.riaz2010@gmail.com
command: /sp.implement
labels: [refactor, seam, apphandle, headless, chat]
links:
  spec: specs/014-server-thin-client/spec.md
  ticket:
  adr: history/adr/0017-server-deployment-and-update-strategy.md
  pr:
files:
  - src-tauri/src/commands/assistant.rs
  - src-tauri/src/services/assistant/capabilities/guidance.rs
  - src-tauri/src/services/assistant/capabilities/draft.rs
  - src-tauri/src/services/assistant/capabilities/summarize_doc.rs
  - src-tauri/src/services/assistant/capabilities/compare.rs
  - src-tauri/src/services/assistant/capabilities/conflict.rs
  - src-tauri/src/services/assistant/capabilities/action_items.rs
  - src-tauri/src/services/assistant/capabilities/mod.rs
  - src-tauri/src/services/assistant/mapreduce.rs
tests:
  - "cargo check --lib (desktop): exit 0"
  - "cargo check --features server --bin noviq-server: exit 0"
---

## Prompt

take on T009b next

## Response snapshot

Investigated the coupling first: the deep paths (ModelSlot::infer_with_cancel_temp,
query_router::route_scoped_with_override) ALREADY take Option<AppHandle>. So instead of threading a
new EventSink trait through the hot path (large, risky), the minimal correct seam was to make
AppHandle optional at the assistant capability layer. Changed `app: &AppHandle` →
`app: Option<&AppHandle>` across the dispatch (commands/assistant.rs → Some(&app)) and all 7
capability functions + mapreduce.rs; guarded the two emit closures; converted Some(app.clone()) →
app.cloned(). Desktop keeps live progress (Some); headless passes None (full answer+citations, no
live ticks — SSE deferred to US4). Both desktop (`cargo check --lib`) and server
(`--features server`) builds exit 0; zero behavior change for the desktop app.

## Outcome

- ✅ Impact: the chat pipeline can now run headless (None); US1 `/api/query` (T011) + read paths (T013) unblocked.
- 🧪 Tests: desktop + server `cargo check` both exit 0. No new unit tests (signature-only refactor; existing tests cover behavior).
- 📁 Files: 9 (dispatch + 7 capability handlers + mapreduce).
- 🔁 Next prompts: T011 wire `/api/query` (expand AppState to hold embedder/model_slot/reranker/scheduler; call the send_message logic with None) + frontend login + US7 admin endpoints.
- 🧠 Reflection: reading the deep signatures BEFORE designing saved a big trait-extraction refactor — the optional currency already existed; the seam just had to reach it.

## Evaluation notes (flywheel)

- Failure modes observed: none — mechanical, compiled first try after the signature sweep.
- Graders run and results (PASS/FAIL): desktop + server cargo check PASS.
- Prompt variant (if applicable): n/a.
- Next experiment (smallest change to try): wire `/api/query` against None and smoke a real query→answer over HTTP on the VM (needs AppState expansion + bundled PG).
</content>
