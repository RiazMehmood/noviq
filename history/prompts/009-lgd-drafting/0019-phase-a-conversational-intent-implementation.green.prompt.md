---
id: 0019
title: phase A conversational intent implementation
stage: green
date: 2026-06-09
surface: agent
model: claude-opus-4-8
feature: 009-lgd-drafting
branch: 009-lgd-drafting
user: engr.riaz2010@gmail.com
command: /sp.implement
labels: [drafting, conversational-ux, phase-a, tdd, green]
links:
  spec: specs/009-lgd-drafting/spec.md
  ticket:
  adr:
  pr:
files:
  - src-tauri/src/db/migrations/023_draft_chat.sql
  - src-tauri/src/services/drafting/chat.rs
  - src-tauri/src/services/drafting/conversation.rs
  - src-tauri/src/services/drafting/mod.rs
  - src-tauri/src/commands/drafting.rs
  - src-tauri/src/lib.rs
  - src/api/drafting.ts
  - src/stores/drafting.ts
  - src/components/drafting/ChatSession.tsx
  - src/pages/Drafting.tsx
  - CHANGELOG_DEV.md
  - specs/009-lgd-drafting/tasks.md
tests:
  - "cargo test --lib drafting:: (40/40)"
  - cargo test --test lgd_drafting_acceptance
---

## Prompt

Implement Phase A ONLY (tasks T049–T063, labelled [CONV]) of the conversational drafting session in specs/009-lgd-drafting/tasks.md. Follow TDD: write the RED tests (T050–T052) and confirm they fail before implementing GREEN (T053–T061). Do NOT implement Phase B or Phase C. Reuse existing session.rs/docset.rs/start_draft — refactor start_draft into propose_for_session rather than duplicating. Stop and report after T063 (or sooner if blocked).

## Response snapshot

Implemented Phase A (intent-chat MVP) with TDD. RED first: wrote chat.rs/conversation.rs with stub impls + tests → `cargo test --lib drafting::` showed **7 failures** (4 parser + 3 persistence), 32 others pass. Then GREEN.

- **T049** migration `023_draft_chat.sql` (additive; `chat_message` append-only + `conversation_summary` dormant until Phase C).
- **T053** `services/drafting/chat.rs`: `DraftAction{SetIntent,Clarify}` + `parse_turn`/`next_field`/`question_for`/`apply`/`intent_complete` (deterministic, no model, no citation authority — FR-028).
- **T054** `services/drafting/conversation.rs`: `append_message` (monotonic seq), `load_conversation` (ordered, never deletes) — FR-031/032.
- **T055** refactored `start_draft` → reusable `propose_for_session` (one home for FR-016 whole-corpus propose).
- **T056** `send_chat_turn` command (first turn mints session from template_id → greeting+Q1; answer turns parse→apply→persist; on intent-complete calls propose_for_session + flips intentComplete). **T057** `load_conversation` command. Both registered in `lib.rs`.
- **Frontend T058–T061**: api `sendChatTurn`/`loadConversation` + types; store `messages`/`draftId` signals; `ChatSession.tsx` (template chooser → transcript → composer; on intent-complete sets session/confirmedDocs/step=docset = parity with IntentForm); wired into `Drafting.tsx` (replaces IntentForm at intent step).
- **T052** parity/persistence test green. **T062**: `cargo test --lib drafting::` **40/40** (was 32+8), backend `cargo build --lib` exit 0, frontend `tsc --noEmit` exit 0. **T063**: CHANGELOG_DEV entry + tasks T049–T063 marked [X].

Phase B (fold doc-set/gap/refine into transcript) and Phase C (rolling-summary compaction) NOT implemented, as instructed.

## Outcome

- ✅ Impact: officers now collect intent via a chat transcript that drives the existing deterministic pipeline; IntentForm replaced at parity; every turn persisted; citation-grade contract untouched.
- 🧪 Tests: drafting lib 40/40 (8 new), acceptance unaffected, frontend tsc clean, backend build clean.
- 📁 Files: migration + 2 new services + command/lib wiring + 4 frontend files + CHANGELOG + tasks.
- 🔁 Next prompts: commit Phase A; later `/sp.tasks` + `/sp.implement` for Phase B, then Phase C.
- 🧠 Reflection: kept retrieval parity as a manual check (needs embedder+corpus) and asserted logic+persistence parity in-test — pragmatic given tauri-command State can't be built in unit tests.

## Evaluation notes (flywheel)

- Failure modes observed: regex for marking [P]-tagged tasks missed T050/T051/T058/T059 (fixed with a second sed).
- Graders run and results (PASS/FAIL): RED 7 failing → GREEN 40/40 PASS; backend build PASS; tsc PASS.
- Prompt variant (if applicable): n/a
- Next experiment (smallest change to try): run the app and walk the quickstart steps 1–2 to confirm the chat→docset handoff visually (the one check not covered by automated tests).
