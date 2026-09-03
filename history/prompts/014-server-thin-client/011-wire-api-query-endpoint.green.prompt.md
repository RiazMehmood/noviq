---
id: 011
title: Wire /api/query chat endpoint
stage: green
date: 2026-06-25
surface: agent
model: claude-opus-4-8
feature: 014-server-thin-client
branch: 015-bureaucrat-chat
user: engr.riaz2010@gmail.com
command: /sp.implement
labels: [implement, http-api, chat, query, axum]
links:
  spec: specs/014-server-thin-client/spec.md
  ticket:
  adr: history/adr/0017-server-deployment-and-update-strategy.md
  pr:
files:
  - src-tauri/src/services/assistant/mod.rs
  - src-tauri/src/commands/assistant.rs
  - src-tauri/src/api/query.rs
  - src-tauri/src/api/mod.rs
  - src-tauri/src/api/router.rs
  - src-tauri/src/bin/noviq-server.rs
tests:
  - "cargo check --lib (desktop): exit 0"
  - "cargo check --features server --bin noviq-server: exit 0"
  - "cargo test --features server --lib: 616 passed / 0 failed / 2 ignored"
---

## Prompt

wire T011 /api/query next

## Response snapshot

Extracted `send_message`'s turn orchestration into `assistant::run_turn(..., app: Option<&AppHandle>,
...)` so the Tauri command (passes `Some(&app)`) and the new headless `api/query.rs` handler (passes
`None`) share ONE implementation (FR-001, no duplication); moved `all_indexed_source_ids` with it.
Expanded `AppState` with the chat singletons (embedder/model_slot/reranker_slot/scheduler, all Arc);
`noviq-server.rs` constructs them at boot (lazy model load). Mounted `POST /api/query`
(`{conversation_id,text}`→`TurnResponse`) in the auth-protected subtree. Cleaned the dead
imports/fn from the command. Fixes: `resolve_model_path` is pub(crate) (used raw paths in the bin);
`Scheduler::new` already returns Arc. Desktop+server builds exit 0; full lib suite 616/0.

## Outcome

- ✅ Impact: first real end-to-end chat turn over HTTP; the browser client can hit `/api/query` once logged in. One orchestrator for both transports.
- 🧪 Tests: desktop + server cargo check exit 0; `cargo test --lib` 616 passed/0 failed (assistant + auth incl.). Refactor is behavior-preserving. Live PG/HTTP smoke pending on VM.
- 📁 Files: assistant/mod.rs (run_turn), commands/assistant.rs (thin wrapper + import cleanup), api/{query,mod,router}.rs, bin/noviq-server.rs.
- 🔁 Next prompts: `/api/search` + read paths (T013); frontend call-site migration (T010) + login screen; US7 admin endpoints; on VM live query smoke.
- 🧠 Reflection: extracting the orchestrator (not duplicating per transport) keeps the citation-grade pipeline single-sourced — any future fix lands once.

## Evaluation notes (flywheel)

- Failure modes observed: pub(crate) `resolve_model_path` unreachable from bin crate; double-Arc on Scheduler::new. Both fixed.
- Graders run and results (PASS/FAIL): cargo check (desktop+server) PASS; lib tests 616/0 PASS.
- Prompt variant (if applicable): n/a.
- Next experiment (smallest change to try): boot noviq-server on the VM against bundled PG, login, POST /api/query, confirm a grounded answer+citations round-trips.
</content>
