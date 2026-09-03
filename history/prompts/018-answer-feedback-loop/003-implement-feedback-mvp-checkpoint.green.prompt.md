---
id: 003
title: Implement feedback MVP checkpoint
stage: green
date: 2026-07-03
surface: agent
model: claude-opus-4-8
feature: 018-answer-feedback-loop
branch: release
user: Riaz Mehmood
command: (freeform — "start working on remaining feedback work" → "save progress")
labels: [feedback, green, citations, postgres, api, solidjs, wip]
links:
  spec: specs/018-answer-feedback-loop/spec.md
  ticket:
  adr:
  pr:
files:
  - src-tauri/src/db/pg/migrations/008_answer_feedback.sql
  - src-tauri/src/db/repositories/answer_feedback.rs
  - src-tauri/src/db/repositories/mod.rs
  - src-tauri/src/api/feedback.rs
  - src-tauri/src/api/mod.rs
  - src-tauri/src/api/admin.rs
  - src-tauri/src/api/router.rs
  - src/api/assistant.ts
  - src/components/CitationFeedback.tsx
  - src/pages/ChatPage.tsx
  - specs/018-answer-feedback-loop/tasks.md
  - CHANGELOG_DEV.md
tests:
  - frontend tsc --noEmit EXIT 0
  - Rust NOT compiled (no Windows toolchain — VM pending)
  - T005 repo unit tests NOT yet written
---

## Prompt

ok now start working in remaing work of feedback feature.
[later] save progress upto now we will start working from this point next time.

## Response snapshot

Implemented the F018 US1 MVP + FR-009 aggregate against `release`, grounded in the plan:
- Migration `008_answer_feedback.sql` (additive: table + CHECKs + partial unique current-row index).
- Repo `answer_feedback.rs`: `upsert` (txn supersede/merge, latest-wins with history), `current_for`,
  `summary`; registered in `mod.rs`.
- API: `feedback.rs::submit` (AuthUser, enum+message_id validation, session user_id), `bad_request`
  helper; `admin::feedback_summary` (AdminUser); routes `/feedback`, `/submit_feedback`,
  `/admin/feedback/summary`.
- Frontend: `submitFeedback`+types in `assistant.ts`; `CitationFeedback.tsx` two one-tap chip rows
  (locked officer labels, single-select, supersede on re-tap); mounted in `ChatPage.tsx` under
  Sources when `message_id` present.
Anchor = `chat_message.id` (already on client). FR-004 trace reused from `query_log` + `/api/traces/*`.
Frontend typechecks clean (tsc EXIT 0). Rust unbuilt (compile on GPU VM next). Uncommitted.

## Outcome

- ✅ Impact: End-to-end two-question citation feedback wired (DB → API → transport → UI) minus
  compile/test/verify. Deferred T008 desktop cmd, T014 admin FE, T019 re-hydration.
- 🧪 Tests: frontend tsc EXIT 0; T005 repo tests + T015 VM cargo check/test pending.
- 📁 Files: see list (10 code files + tasks.md + CHANGELOG_DEV).
- 🔁 Next prompts: write T005 tests (`crate::db::test_pool()` harness, no FK so any message_id);
  VM `cargo check -p noviq-server --features server` + `cargo test answer_feedback`; run quickstart
  (psql/curl/browser); then commit + deploy to `release`.
- 🧠 Reflection: Biggest wins were reuse — existing `message_id` anchor and existing trace store
  collapsed FR-003/FR-004 to near-zero new work.

## Evaluation notes (flywheel)

- Failure modes observed: setup scripts branch-guard on `release`; no Rust toolchain on Windows.
- Graders run and results (PASS/FAIL): frontend tsc PASS; Rust compile PENDING.
- Prompt variant (if applicable): n/a
- Next experiment (smallest change to try): compile migration+repo on VM first, confirm
  `\d answer_feedback`, before writing/running T005.
