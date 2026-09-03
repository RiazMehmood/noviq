---
id: 0010
title: Phase 6 — compare/conflict + action items
stage: green
date: 2026-06-21
surface: agent
model: claude-opus-4-8
feature: 015-bureaucrat-chat
branch: 015-bureaucrat-chat
user: Riaz Mehmood
command: start phase 6
labels: [feature-015, capabilities, map-reduce, citation-grade, us7, us9]
links:
  spec: specs/015-bureaucrat-chat/spec.md
  ticket:
  adr: history/adr (unified-bureaucrat-chat-architecture)
  pr:
files:
  - src-tauri/src/services/assistant/capabilities/mod.rs
  - src-tauri/src/services/assistant/capabilities/summarize_doc.rs
  - src-tauri/src/services/assistant/capabilities/compare.rs
  - src-tauri/src/services/assistant/capabilities/conflict.rs
  - src-tauri/src/services/assistant/capabilities/action_items.rs
  - src-tauri/src/commands/assistant.rs
  - src-tauri/src/lib.rs
  - src-tauri/tests/bureaucrat_chat_acceptance.rs
  - specs/015-bureaucrat-chat/tasks.md
  - CHANGELOG_DEV.md
tests:
  - "cargo test --lib assistant: 62 passed / 0 failed"
  - "cargo test --test bureaucrat_chat_acceptance: 13 passed / 0 failed"
---

## Prompt

start phase 6

## Response snapshot

Implemented F015 Phase 6 (T038–T042): the cross-document compare/conflict +
action-items capabilities, the `confirm_documents` scope-disambiguation command,
and the SC-011 acceptance gate.

- Factored the shared map-reduce path (gather leaf provisions → map-reduce →
  citation gate → assemble) out of `summarize_doc.rs` into
  `capabilities/mod.rs` as `gather_provisions` + `run_mapreduce_capability(.., cap)`.
- Added thin `compare.rs`, `conflict.rs`, `action_items.rs` capability files, each
  delegating to the shared runner with its own `Capability` + a persona-fragment
  unit test; `summarize_doc::run` reduced to a delegate (behaviour unchanged).
- Wired the three new arms into `send_message`; added `confirm_documents`
  (deterministic file-name substring resolution to indexed source ids) and
  registered it in `lib.rs`.
- SC-011 (T042): both-cited candidate conflict survives and re-gates clean; a
  half-grounded conflict claim is suppressed + flagged — 0 conflicts asserted
  without both grounded citations (research R8).

## Outcome

- ✅ Impact: US7 (compare/conflict) + US9 (action items) backend complete; reuses
  the Phase-5 map-reduce driver and the model-independent citation gate, so the
  citation-grade no-LLM contract holds.
- 🧪 Tests: lib assistant 62/0; acceptance 13/0 (incl. 2 new SC-011 gates).
- 📁 Files: 1 shared runner, 3 new capabilities, 1 new command, 1 test gate.
- 🔁 Next prompts: Phase 7 (T043–T046, T052) conversation-memory polish; then
  Phase 8 polish (T047–T050).
- 🧠 Reflection: the persona-as-config design meant three "new" capabilities cost
  almost no new logic — only a `Capability` arg + prompt fragment differ.

## Evaluation notes (flywheel)

- Failure modes observed: none — clean build, no warnings from new code.
- Graders run and results (PASS/FAIL): SC-011 PASS, SC-009/SC-010 PASS (regression).
- Prompt variant (if applicable): n/a (fragments unchanged from T048-pending defaults).
- Next experiment (smallest change to try): live-LLM smoke of a compare turn over
  two indexed acts once Phase 7 lands the memory polish.
