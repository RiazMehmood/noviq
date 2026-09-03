---
id: 0007
title: Phase 2 foundational spine (T004/T008/T011-T015)
stage: green
date: 2026-06-21
surface: agent
model: claude-opus-4-8
feature: 015-bureaucrat-chat
branch: 015-bureaucrat-chat
user: Riaz Mehmood
command: implement Phase 2
labels: [foundational, assistant, allocator, persona, memory, orchestration]
links:
  spec: specs/015-bureaucrat-chat/spec.md
  ticket:
  adr: history/adr (unified bureaucrat chat architecture)
  pr:
files:
  - src-tauri/src/services/query_router.rs
  - src-tauri/src/services/llm.rs
  - src-tauri/src/services/assistant/mod.rs
  - src-tauri/src/services/assistant/persona.rs
  - src-tauri/src/services/assistant/memory.rs
  - src-tauri/src/services/assistant/intent.rs
  - src-tauri/src/services/assistant/context_budget.rs
  - src-tauri/src/commands/assistant.rs
  - src-tauri/src/commands/mod.rs
  - src-tauri/src/lib.rs
  - src/api/assistant.ts
  - src/pages/ChatPage.tsx
  - config/personas/bureaucrat.toml
tests:
  - cargo test --lib assistant:: (37 passed / 0 failed)
  - cargo test --lib query_router:: (59 passed / 0 failed)
---

## Prompt

start phase 2 with T008 and run one single build and test at last after complete phase

## Response snapshot

Implemented the six remaining Phase 2 tasks of Feature 015, then ran one build +
test pass:
- T008: removed the per-chunk 25% cap in `query_router::build_context_pass` so
  provisions are whole-or-omitted (FR-014); deleted the unused knob.
- T011: `llm.rs` resolves `n_ctx == 0` dynamically from the loaded model
  (trained ctx clamped to RAM/VRAM); explicit values pass through.
- T012: new `persona.rs` typed loader for `bureaucrat.toml` (embedded +
  `ECHO_PERSONA_TOML` override; persona/capability/settings).
- T013: new `memory.rs` (load recent turns + summary, append turn) keyed on
  `conversation_id`; pinned/eviction stubbed for Phase E.
- T014/T004: new `commands/assistant.rs::send_message` + orchestration in
  `mod.rs` (response types, `plan_budget`, `assemble_response`); reuses
  `route_scoped` for grounded retrieval+generation, runs the citation gate,
  logs the budget plan, persists both turns; registered in lib.rs.
- T015: new `src/api/assistant.ts` + `src/pages/ChatPage.tsx` single chat
  surface (answer + citations + offer + flags + context-fill).

## Outcome

- ✅ Impact: F015 foundational spine complete — a guidance turn round-trips
  end-to-end with citations and an audited budget plan (Σ ≤ window). Unblocks
  Phases 3–7.
- 🧪 Tests: assistant:: 37/0 (incl. DB-backed memory on bundled PG), query_router:: 59/0; `cargo build --lib` clean.
- 📁 Files: 13 touched (3 shared edits, 5 new assistant/command files, 2 new frontend files, tasks.md + CHANGELOG_DEV).
- 🔁 Next prompts: Phase 3 (T016–T024) guidance capability handler + acceptance gates SC-001/002/003/005.
- 🧠 Reflection: command-layer budget plan uses EstimateCounter/ECHO_N_CTX as a Phase-2 approximation; the live ModelTokenCounter + per-model window wire through capability handlers in Phase 3.

## Evaluation notes (flywheel)

- Failure modes observed: none in tests; T008 loosens the legacy RAG context builder globally — re-run the 22-prompt eval if oversized TOC/cover chunks resurface.
- Graders run and results (PASS/FAIL): cargo test PASS (assistant 37/0, query_router 59/0).
- Prompt variant (if applicable): n/a
- Next experiment (smallest change to try): wire the real ModelTokenCounter into the command budget plan once a capability handler owns the loaded model.
