---
id: 0011
title: Phase 7 — conversation memory polish
stage: green
date: 2026-06-21
surface: agent
model: claude-opus-4-8
feature: 015-bureaucrat-chat
branch: 015-bureaucrat-chat
user: Riaz Mehmood
command: start phase 7
labels: [feature-015, memory, pinned-facts, eviction, context-fill, us3]
links:
  spec: specs/015-bureaucrat-chat/spec.md
  ticket:
  adr: history/adr (unified-bureaucrat-chat-architecture)
  pr:
files:
  - src-tauri/src/services/assistant/memory.rs
  - src-tauri/src/commands/assistant.rs
  - src-tauri/src/lib.rs
  - src/components/ContextFillNotice.tsx
  - src/pages/ChatPage.tsx
  - src/api/assistant.ts
  - src-tauri/tests/bureaucrat_chat_acceptance.rs
  - specs/015-bureaucrat-chat/tasks.md
  - CHANGELOG_DEV.md
tests:
  - "cargo test --lib assistant: 69 passed / 0 failed"
  - "cargo test --test bureaucrat_chat_acceptance: 16 passed / 0 failed"
---

## Prompt

start phase 7

## Response snapshot

Implemented F015 Phase 7 (T043–T046, T052) — the conversation-memory layer.

- T043 — deterministic pinned-fact extraction (`extract_facts`: grade/BPS,
  length-of-service, leave type) → `pinned_case_fact` rows; restatement supersedes,
  same-value restatement is a no-op; surfaced in the always-present allocator slot.
- T044 — `evict_old_turns(keep_recent)`: folds oldest non-summarized turns into
  `conversation_summary`, marks them `summarized`; pinned facts exempt; incremental.
- T045 — `new_chat_from_notice` command (carry non-superseded facts + recap into a
  fresh conversation), registered in `lib.rs`; `ContextFillNotice.tsx` + ChatPage
  `startFreshChat` swap; `api/assistant.ts` wrappers.
- Orchestrator: `send_message` captures facts after the user turn, evicts after the
  assistant turn.
- T046 — SC-004 (early facts honored after a 40-turn evicting session) + SC-008
  (notice at threshold + facts carry to a fresh chat). T052 — FR-021 restart
  (reconnect a second independent pool to the same durable DB; conversation reloads
  intact). First DB-backed acceptance gates, still no LLM.

## Outcome

- ✅ Impact: long sessions never lose load-bearing facts; officers are nudged to a
  fresh chat that preserves continuity (US3 complete). No migration.
- 🧪 Tests: lib assistant 69/0; acceptance 16/0 (3 new Phase-7 gates).
- 📁 Files: memory.rs (core), 1 command, 1 component + ChatPage/api wiring, 3 gates.
- 🔁 Next prompts: Phase 8 — T047 (trace audit), T048 (persona tuning), T049
  (MODULAR_IMPROVEMENT_LOG), T050 (full-suite green gate).
- 🧠 Reflection: keeping pinned facts in their own table (not the summary) is what
  makes "honored late" hold by construction — eviction can never touch them.

## Evaluation notes (flywheel)

- Failure modes observed: first FR-021 draft mis-modelled `test_pool` (isolated DB
  per call) — fixed by reconnecting via the pool's own connect options.
- Graders run and results (PASS/FAIL): SC-004/SC-008/FR-021 PASS; all prior gates PASS.
- Prompt variant (if applicable): n/a (extraction is regex, no prompt).
- Next experiment (smallest change to try): LLM-assisted summary in `evict_old_turns`
  (swap `compact_turn` for a single-pass extractive summary) once Phase 8 lands tracing.
