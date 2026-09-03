---
id: 0008
title: Phase 3 grounded no-truncation MVP
stage: green
date: 2026-06-21
surface: agent
model: claude-opus-4-8
feature: 015-bureaucrat-chat
branch: 015-bureaucrat-chat
user: Riaz Mehmood
command: start phase 3 of feature 015
labels: [assistant, guidance, citation-gate, acceptance, mvp]
links:
  spec: specs/015-bureaucrat-chat/spec.md
  ticket:
  adr: history/adr (ADR-0016 citation gate)
  pr:
files:
  - src-tauri/src/services/assistant/citation_gate.rs
  - src-tauri/src/services/assistant/mod.rs
  - src-tauri/src/services/assistant/capabilities/mod.rs
  - src-tauri/src/services/assistant/capabilities/guidance.rs
  - src-tauri/src/commands/assistant.rs
  - src-tauri/tests/bureaucrat_chat_acceptance.rs
  - specs/015-bureaucrat-chat/tasks.md
  - CHANGELOG_DEV.md
tests:
  - "cargo test --lib assistant:: : 44/0"
  - "cargo test --test bureaucrat_chat_acceptance : 6/0"
---

## Prompt

start phase 3 of feature 015

## Response snapshot

Implemented the F015 Phase 3 MVP (US1+US2+US3, tasks T016–T024):

- T018: added `citation_gate::redact_ungrounded` (sentence-level suppression of
  any provision the retrieved context cannot support verbatim) and routed the
  universal `assemble_response` through it, so ungrounded claims are suppressed +
  flagged (offer-to-index) rather than only flagged.
- T016/T017/T019: new `capabilities/guidance.rs` — `system_prompt` (persona +
  capability fragment + retained `GROUNDING_SYSTEM_PROMPT`), pure `build_response`
  (gate + assemble, offer-to-tailor only for Guidance), and async `run`
  orchestration (retrieve+generate via `route_scoped_with_override` → allocate →
  gate → assemble). It bridges every capability for now.
- `send_message` command reduced to a thin dispatcher to `guidance::run`; turns
  now use the bureaucrat system prompt.
- T020: ChatPage already renders offer/flags/context-fill from Phase 2 — no change.
- T021–T024: new `tests/bureaucrat_chat_acceptance.rs` (NO live LLM) proving
  SC-003 (Σ ≤ window across a 200-turn stress conversation), SC-002 (atomic
  provisions + law-outranks-history), SC-001 (zero fabricated citations in/out of
  corpus), SC-005 (complete cited first answer + offer, no clarifying gate).

## Outcome

- ✅ Impact: grounded, no-truncation bureaucrat MVP — a guidance turn round-trips
  with the bureaucrat persona, fabricated citations are suppressed model-
  independently, and the P1 invariants are acceptance-gated.
- 🧪 Tests: `cargo test --lib assistant::` 44/0; `cargo test --test
  bureaucrat_chat_acceptance` 6/0; `cargo build --lib` clean.
- 📁 Files: 1 new capability module (+mod), 1 new acceptance test, gate +
  assembler + command edits, tasks/changelog updated.
- 🔁 Next prompts: Phase 4 (T026–T031/T051) draft handler + `<think>` strip +
  model_status + retire slot/template path.
- 🧠 Reflection: keeping suppression in the universal assembler (not just
  guidance) means every later capability inherits the citation contract for free.

## Evaluation notes (flywheel)

- Failure modes observed: one acceptance assertion initially mis-tuned (budget
  left room for one history turn); fixed the synthetic token sizes, not the
  allocator.
- Graders run and results (PASS/FAIL): SC-001 PASS, SC-002 PASS, SC-003 PASS,
  SC-005 PASS.
- Prompt variant (if applicable): n/a
- Next experiment (smallest change to try): wire the real `ModelTokenCounter` +
  per-model window into the command budget plan (currently EstimateCounter).
