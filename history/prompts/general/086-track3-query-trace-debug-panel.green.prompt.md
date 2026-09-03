---
id: 086
title: Track 3 — unified per-turn query trace + admin query-debug panel
stage: green
date: 2026-07-11
surface: agent
model: claude-opus-4-8
feature: GraphRAG & graph-usage audit (ADR-0019 Track 3)
branch: feat/track3-query-trace-debug
user: Riaz Mehmood
command: recall memory and start working on track 2 and 3
labels: [tracing, observability, admin, graphrag, adr-0019]
links:
  spec:
  ticket:
  adr: history/adr/0019-structured-counting-and-verbatim-synthesis-retrieval.md
  pr:
files:
  - src-tauri/src/services/query_trace.rs
  - src-tauri/src/services/query_router.rs
  - src-tauri/src/services/intent_router/mod.rs
  - src-tauri/src/services/assistant/mod.rs
  - src-tauri/src/services/assistant/capabilities/mod.rs
  - src-tauri/src/services/assistant/capabilities/guidance.rs
  - src-tauri/src/services/assistant/capabilities/draft.rs
  - src-tauri/src/services/assistant/capabilities/additive.rs
  - src-tauri/src/services/assistant/capabilities/transform.rs
  - src-tauri/src/services/assistant/capabilities/count.rs
  - src-tauri/src/services/assistant/capabilities/amendment_chain.rs
  - src-tauri/src/services/assistant/capabilities/compare.rs
  - src-tauri/src/services/assistant/capabilities/conflict.rs
  - src-tauri/src/services/assistant/capabilities/action_items.rs
  - src-tauri/src/services/assistant/capabilities/summarize_doc.rs
  - src-tauri/src/services/assistant/capabilities/notice_assist/mod.rs
  - src-tauri/src/api/system.rs
  - src-tauri/src/api/router.rs
  - src/api/assistant.ts
  - src/api/index.ts
  - src/pages/Admin.tsx
  - src/pages/ChatPage.tsx
tests:
  - query_trace::capability_is_recorded_in_json_and_text (new)
  - frontend tsc --noEmit (PASS, exit 0)
  - VM cargo check + cargo test --lib query_trace:: assistant:: (PENDING)
---

## Prompt

recall memory and start working on track 2 and 3

## Response snapshot

Recalled the GraphRAG-audit memory + ADR-0019. Confirmed with the user (AskUserQuestion):
Track 3 FIRST (debug panel helps verify Track 2), deep version (viewer + trace ALL
capabilities + graph steps); Track 2 = full build incl. VM re-index, next. Planned + built
Track 3:

- Unified ONE query trace per turn: `assistant::run_turn` owns a `QueryTrace`, threads
  `&mut trace` into every capability, finalizes + writes once, stamps `TurnResponse.trace_id`.
- `route_scoped_with_override` gained `external_trace: Option<&mut QueryTrace>` (borrow, don't
  write when external; legacy desktop path unchanged). Guards every internal `trace.write`.
- `query_trace.rs`: `capability` field + `finished` flag + setter + test.
- Per-capability stage steps (count/amendment_chain/map-reduce runner/transform/additive/
  notice_assist) + new *Graph expansion (citation neighbours)* step in `query_router`.
- Admin viewer endpoints `GET /api/query_traces[?limit]` + `/query_traces/:id` (admin-gated).
- Frontend: Admin "Query Debug" table+modal, ChatPage admin-only per-answer "Debug" link+modal,
  api wrappers, `trace_id?` on the response type.

Frontend `tsc --noEmit` clean. No local cargo (Windows) → Rust build is a VM step.

## Outcome

- ✅ Impact: every answer path is now inspectable end-to-end; admins can browse/inspect any
  query's step-by-step pipeline. Sets up Track 2 verification.
- 🧪 Tests: frontend typecheck PASS; new query_trace unit test added; VM cargo check/test PENDING.
- 📁 Files: 17 Rust + 4 TS (see list). CHANGELOG_DEV `2026-07-11 (latest3)`.
- 🔁 Next prompts: VM verify (cargo check/test + npm build + manual endpoint sweep), then Track 2.
- 🧠 Reflection: the rich trace already ran on the live path — the real gaps were (a) unifying
  ownership so non-guidance capabilities share it and (b) a viewer. Threading `&mut trace`
  (single async task) beat `Arc<Mutex<>>` churn.

## Evaluation notes (flywheel)

- Failure modes observed: none yet (pre-VM-build).
- Graders run and results (PASS/FAIL): frontend tsc PASS; Rust PENDING (no local toolchain).
- Prompt variant (if applicable): n/a
- Next experiment (smallest change to try): VM `cargo check` on the branch; if green, manual
  per-capability trace sweep, then start Track 2.
