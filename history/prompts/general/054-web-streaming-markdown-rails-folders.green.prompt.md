---
id: 054
title: Web streaming markdown rails folders
stage: green
date: 2026-07-02
surface: agent
model: claude-fable-5
feature: general
branch: release
user: Riaz Mehmood
command: fix user document visibility + chat UX overhaul
labels: [web-ui, streaming, sse, markdown, layout, folders]
links:
  spec: none
  ticket: none
  adr: none
  pr: none
files:
  - src-tauri/src/db/repositories/folders.rs
  - src-tauri/src/services/chat_stream.rs (new)
  - src-tauri/src/services/mod.rs
  - src-tauri/src/services/llm.rs
  - src-tauri/src/compat.rs
  - src-tauri/src/api/chat_events.rs (new)
  - src-tauri/src/api/{mod,router,query}.rs
  - src/components/Markdown.tsx (new)
  - src/pages/ChatPage.tsx
  - src/api/{transport,assistant}.ts
tests:
  - VM deploy-parity cargo check RC=0 (both commits)
  - additive/transform lib tests still green (10 pass)
  - npx tsc --noEmit clean
---

## Prompt

still documents are not being shown on user screen.

[mid-task addition:] and the response is not interactive with headings
subheadings and, so make it also looking good so UX can increase also. and on
right side remove syntehsis tools option, and also remove researc insights from
right also and place document listing on right and left is only for user chat
session only, and response must be streaming not direct push whole response
through it is is also not looking good,

## Response snapshot

Two deployments. (1) `5f579d98` — root cause of the empty user document tree:
`folders::list_for_owner` filtered `WHERE owner_id = $1` while the corpus
folder is global (`owner_id NULL`, verified live via bundled psql on port
36443); now includes NULL-owner folders; writes stay owner/admin-gated.
(2) `143b3a30` — chat UX overhaul: (a) live web streaming — new `chat_stream`
process-wide broadcast + `GET /api/chat/events` SSE; the headless compat
`AppHandle` (previously a no-op stub) now carries a conversation id and
forwards every service-layer `emit` (`chat://token`, `assistant://progress`)
into the broadcast, and `api/query.rs` passes `Some(&handle)` — zero
service-layer changes because `app` was already threaded through the whole
generation path; ChatPage opens the stream per turn and renders tokens live.
(b) markdown answers — `Markdown.tsx` (marked+DOMPurify+prose, deps already in
package.json unused) + GROUNDING_SYSTEM_PROMPT v5.4.0 headings/lists/bold
instruction. (c) rails — left = chat sessions only, right = DocumentTree;
Research Insights + Synthesis tool removed.

## Outcome

- ✅ Impact: users see the corpus; answers stream live with structure instead
  of a 40s spinner then a text wall.
- 🧪 Tests: VM deploy-parity checks RC=0 both pushes; tsc clean; existing lib
  tests green.
- 📁 Files: 13 changed (3 new).
- 🔁 Next prompts: user browser test (documents visible, streaming, markdown,
  new layout); reranker next in retrieval queue.
- 🧠 Reflection: the compat-AppHandle seam made web streaming a ~100-line
  change; the "deliberately excluded" comment on list_for_owner shows why
  visibility bugs need the intent re-checked against CURRENT UI reality.

## Evaluation notes (flywheel)

- Failure modes observed: streamed text is the raw pre-assembly draft;
  grounding-retry passes append tokens (same as desktop) — possible future
  `stream-reset` event.
- Graders run and results (PASS/FAIL): PASS (compile+tests+tsc).
- Prompt variant (if applicable): GROUNDING_SYSTEM_PROMPT 5.2.0 → 5.4.0.
- Next experiment (smallest change to try): if models over-format short
  answers, tighten the "short factual answers stay plain" clause.
