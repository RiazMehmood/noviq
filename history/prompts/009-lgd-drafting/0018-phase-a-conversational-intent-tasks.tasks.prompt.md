---
id: 0018
title: phase A conversational intent tasks
stage: tasks
date: 2026-06-09
surface: agent
model: claude-opus-4-8
feature: 009-lgd-drafting
branch: 009-lgd-drafting
user: engr.riaz2010@gmail.com
command: /sp.tasks
labels: [drafting, conversational-ux, phase-a, tdd]
links:
  spec: specs/009-lgd-drafting/spec.md
  ticket:
  adr:
  pr:
files:
  - specs/009-lgd-drafting/tasks.md
tests:
  - cargo test drafting
  - cargo test --test lgd_conversation
---

## Prompt

Generate tasks for Phase A ONLY (the intent-chat MVP) of the conversational drafting session enhancement to Feature 009. Phase A scope per plan.md: additive migration 023 (chat_message + conversation_summary), services/drafting/chat.rs (turn-action parser → DraftAction, intent fields only for this phase), services/drafting/conversation.rs (append + load messages; NO compaction yet), commands/drafting.rs send_chat_turn + load_conversation, frontend ChatSession.tsx collecting intent conversationally and calling existing start_draft, store + api bindings. Replaces IntentForm at parity. Defer Phase B (fold doc-set/gap/refine into transcript) and Phase C (rolling-summary compaction) — do not generate tasks for those. TDD: tests before implementation. Reuse existing session.rs/docset.rs and start_draft command; do not duplicate.

## Response snapshot

Appended a **Phase A** section to `specs/009-lgd-drafting/tasks.md` (existing tasks ended at T048; Phase A = **T049–T063**), labelled `[CONV]`, TDD-ordered, with exact file paths grounded in the real code (`start_draft` at `commands/drafting.rs:345`, `session.rs` `save`/`load`/`DraftState`/`IntentAnswers`, `DraftSessions`, `docset::{intent_query,propose_candidates}`).

- **Schema**: T049 additive migration 023 (`chat_message` + `conversation_summary`; latter unused until Phase C); follow 021/022 additive convention.
- **RED**: T050 chat.rs parser unit tests, T051 conversation.rs append+load tests, T052 `tests/lgd_conversation.rs` parity acceptance (send_chat_turn intent turns == direct start_draft; turns persisted).
- **GREEN**: T053 `chat.rs` (DraftAction{SetIntent,Clarify} + parse_turn), T054 `conversation.rs` (ChatMessage, append_message monotonic seq, load_conversation, no compaction), T055 refactor `start_draft` → reusable `propose_for_session` so the conversation fills an existing draft_id, T056 `send_chat_turn` command, T057 `load_conversation` command, T058 api bindings (serde camelCase caution), T059 store signals, T060 `ChatSession.tsx`, T061 wire `Drafting.tsx` (ChatSession replaces IntentForm at step==intent; DocSet/DraftView + Index-Folder gate untouched).
- **Verify**: T062 run tests + manual quickstart parity, T063 CHANGELOG_DEV entry + confirm no 2nd model (n/a Phase A).
- Deferred B/C explicitly out of scope; dependency lines added (Phase A after Foundational + US3 + US5; RED before GREEN; T058/T059 [P]).

## Outcome

- ✅ Impact: Phase A intent-chat MVP is now an executable, TDD-ordered task list (15 tasks, T049–T063) reusing the existing pipeline; B/C deferred.
- 🧪 Tests: planned `cargo test drafting`, `cargo test --test lgd_conversation` (written in T050–T052, green at T062).
- 📁 Files: specs/009-lgd-drafting/tasks.md (Phase A section + deps).
- 🔁 Next prompts: `/sp.implement` (or execute T049→T063 manually); then `/sp.tasks` again for Phase B.
- 🧠 Reflection: kept the conversation a thin interaction layer over `start_draft` (parity test T052 is the guardrail) so the citation-grade contract is untouched.

## Evaluation notes (flywheel)

- Failure modes observed: none (planning stage).
- Graders run and results (PASS/FAIL): n/a (no tests run this stage).
- Prompt variant (if applicable): n/a
- Next experiment (smallest change to try): implement T049–T052 first (schema + RED) and confirm the parity test fails before T053+.
