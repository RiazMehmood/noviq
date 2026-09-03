---
id: 080
title: Notice Assist implement increment 1
stage: green
date: 2026-07-10
surface: agent
model: claude-opus-4-8
feature: 022-notice-assist
branch: 022-notice-assist
user: Riaz Mehmood
command: sp.implement
labels: [notice-assist, green, slash-command, foundation, us1]
links:
  spec: specs/022-notice-assist/spec.md
  ticket:
  adr:
  pr:
files:
  - src-tauri/src/db/pg/migrations/010_notice_assist_session.sql
  - src-tauri/src/services/assistant/capabilities/notice_assist/mod.rs
  - src-tauri/src/services/assistant/capabilities/notice_assist/frame.rs
  - src-tauri/src/services/assistant/capabilities/notice_assist/session.rs
  - src-tauri/src/services/assistant/capabilities/notice_assist/{ask,compose,extract}.rs
  - src-tauri/src/services/assistant/capabilities/mod.rs
  - src-tauri/src/services/assistant/intent.rs
  - src-tauri/src/services/assistant/mod.rs
  - src/components/ConfidentialComposer.tsx
  - src/pages/ChatPage.tsx
tests:
  - "VM cargo check --release --features llm,server,cuda = EXIT 0"
  - "VM cargo test (intent + notice_assist) = 21 passed / 0 failed"
  - "frontend npm run build (tsc+vite) = EXIT 0"
---

## Prompt

proceed to /sp.implement

## Response snapshot

Implemented Increment 1 of tasks.md (Setup + Foundational + US1), in verifiable slices given no
local Rust toolchain:
- Migration 010 `notice_assist_session`; `frame.rs` (NoticeAssistState/Frame/Allegation/
  SlotQuestion/CitedRule/Phase/DocKind/SlotStatus, serde-default, forward-compat) + tests;
  `session.rs` (save/load/delete, one_ask seam).
- `intent::is_notice_assist_command` (/help|/reply|/audit) + tests.
- `run_turn` intercept: a command OR an open session routes to `notice_assist::run` before
  capability dispatch (common persist/stamp tail preserved).
- Dispatcher `notice_assist::run`: assist-mode entry ("attach the notice"), exit (/cancel), Analyze
  greet; response via plan_budget + assemble_response (no LLM call for fixed messages). US2–US4
  phases stubbed (extract/ask/compose modules present, empty).
- Frontend: "/" command palette in ConfidentialComposer (mirrors the @-mention machinery) +
  assist-mode banner in ChatPage.

Tasks marked done: T001,T003,T004,T005,T006,T008,T009,T010,T011,T012,T013,T014. Deferred to next
increment: T002 (config fragments), T007 (local generator), T015–T034 (US2 extraction, US3 gather,
US4 grounding/draft, US5 audit template), T035–T040 (polish).

Verification: frontend npm build EXIT 0; VM cargo check EXIT 0; VM tests 21 passed/0 failed
(incl. the 3 new). Pushed to origin/022-notice-assist (commit 5f68e57).

## Outcome

- ✅ Impact: compiling, tested foundation + US1 (command entry + assist mode) on a feature branch.
- 🧪 Tests: 3 new unit tests green; full check/test green on VM.
- 📁 Files: 1 migration, 6 new capability files, 3 edited backend, 2 edited frontend.
- 🔁 Next prompts: Increment 2 = US2 (local Qwen3-8B extraction + tolerant JSON) → US3 (ask-loop)
  → US4 (grounding + draft). Then polish (T035–T040).
- 🧠 Reflection: verifiable-increment strategy (frontend build local, backend cargo on VM) keeps
  each step provably green despite no local Rust toolchain; the risky extraction (US2) is next and
  gated by a prompt-eval task (T038).

## Evaluation notes (flywheel)

- Failure modes observed: none this increment (foundation).
- Graders run and results (PASS/FAIL): npm build PASS; VM cargo check PASS; VM tests PASS (21/0).
- Prompt variant (if applicable): n/a
- Next experiment (smallest change to try): T007+T017+T018 — wire local Qwen3-8B extraction and
  prove tolerant JSON parse on 3–5 real notices before the gather UI.
