---
id: 014
title: F014 local-task batch — docs endpoints, transport, gen-gate, CORS
stage: green
date: 2026-06-26
surface: agent
model: claude-opus-4-8
feature: 014-server-thin-client
branch: 014-server-thin-client
user: Riaz Mehmood
command: first complete all local tasks
labels: [F014, US1, US4, US5, transport, cors, semaphore]
links:
  spec: specs/014-server-thin-client/spec.md
  ticket:
  adr: history/adr (ADR-0017)
  pr:
files:
  - src-tauri/src/api/documents.rs (new) — T013 read endpoints
  - src-tauri/src/api/mod.rs — register documents, AppState.gen_gate
  - src-tauri/src/api/router.rs — mount routes, send_message alias, CORS/limits/headers
  - src-tauri/src/api/query.rs — gen_gate acquire + conversationId alias
  - src-tauri/src/bin/noviq-server.rs — construct gen_gate
  - src-tauri/tests/api_admin.rs — AppState.gen_gate in literal
  - src/api/index.ts — invoke -> transport.call
  - src/api/assistant.ts — invoke -> transport.call
tests:
  - cargo check --features server --bin noviq-server (exit 0)
  - cargo check --lib (desktop, exit 0)
  - cargo test --features server --test api_admin --no-run (compiles)
  - tsc --noEmit (clean)
---

## Prompt

first complete all local tasks  (+ follow-up: first write all code and then run single build and test at end)

## Response snapshot

Worked the remaining local F014 tasks as one batch, single build at the end.
T013: new `api/documents.rs` with command-named POST read endpoints (folders,
sources, entities, KG) reusing repo/knowledge_graph fns. T012: discovered the
`one_ask`/`drafting` command modules are retired (F015) — drafting is a unified-
assistant capability already served by `/api/query`, so reverted an exploratory
one_ask wiring and marked T012 subsumed by T011. T010: `index.ts`/`assistant.ts`
now `import { call as invoke } from "./transport"`; added `/api/send_message`
alias + `conversationId` serde alias so web chat works. T023: `AppState.gen_gate`
Semaphore(1) acquired in the query handler. T022/T037: verified stateless
per-request handlers + AuthUser identity seam (audited). T049: body limit + CORS
+ security-header middleware. T024/T025 deferred (need headless indexing
vertical) with reasoning in tasks.md + CHANGELOG.

## Outcome

- ✅ Impact: browser client gains the document/KG read surface; one transport for
  all wrappers; concurrent generation serialized (no GPU OOM); /api hardened.
- 🧪 Tests: server bin + desktop lib check clean; api_admin compiles; tsc clean.
- 📁 Files: 1 new (documents.rs) + 7 edited; one_ask.rs reverted to pristine.
- 🔁 Next prompts: T024/T025 indexing SSE vertical; VM-gated T026–T028/T035/T051/
  T052 + run api_admin green.
- 🧠 Reflection: the biggest save was catching that one_ask/drafting are retired
  before sinking effort into wiring dead modules — verify module liveness first.

## Evaluation notes (flywheel)

- Failure modes observed: nearly wired a retired command module (caught via
  `commands/mod.rs` showing it commented out); Read-before-Edit gate on cat-opened
  files.
- Graders run and results (PASS/FAIL): server-bin check PASS; desktop lib PASS;
  api_admin --no-run PASS; tsc PASS.
- Prompt variant (if applicable): n/a
- Next experiment (smallest change to try): design the headless indexing progress
  channel (broadcast → SSE) as the T024 spike.
