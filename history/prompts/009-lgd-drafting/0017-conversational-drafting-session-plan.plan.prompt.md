---
id: 0017
title: conversational drafting session plan
stage: plan
date: 2026-06-09
surface: agent
model: claude-opus-4-8
feature: 009-lgd-drafting
branch: 009-lgd-drafting
user: engr.riaz2010@gmail.com
command: /sp.plan
labels: [drafting, conversational-ux, chat-persistence, compaction]
links:
  spec: specs/009-lgd-drafting/spec.md
  ticket:
  adr:
  pr:
files:
  - specs/009-lgd-drafting/plan.md
  - specs/009-lgd-drafting/research.md
  - specs/009-lgd-drafting/data-model.md
  - specs/009-lgd-drafting/contracts/conversation.md
  - specs/009-lgd-drafting/quickstart.md
  - CLAUDE.md
tests:
  - cargo test --test lgd_drafting_acceptance
  - cargo test drafting
---

## Prompt

Refresh the 009-lgd-drafting plan to cover the new conversational drafting session (FR-027–FR-030), chat persistence (FR-031–FR-032), and rolling-summary context compaction (FR-033–FR-034). This is an enhancement to existing Feature 009, not a new feature — amend the existing plan/design artifacts in specs/009-lgd-drafting/. Phase the implementation: intent-chat end-to-end first, then fold doc-set/gap/refine into the single transcript. Semantic retrieval over stored turns is deferred (rolling summary only this cut).

(Preceding discussion established: user chose scope **B** = whole drafting flow as one continuous chat transcript; every chat turn persisted in the DB; rolling-summary compaction so the small-context model reads `summary + recent turns`, never the full transcript; this is a 009 enhancement so amend existing spec artifacts, do NOT scaffold a new feature.)

## Response snapshot

Ran the /sp.plan workflow. `setup-plan.sh` reset `plan.md` to the template (specs/009 is untracked, no committed copy) — reconstructed a complete plan covering both the already-GREEN backend and the new conversational enhancement.

- **plan.md**: Summary, Technical Context (Rust 1.87 + SolidJS/Tauri; sqlx; single model slot for prose+summarization; regex_lite turn parsing; additive migration 023), full **Constitution Check (13 principles, PASS)** — notably III (summarization reuses the single resident slot, no 2nd model), IV (chat tables stay in echo.db; Postgres pivot explicitly out of scope), XI (conversation has no generative authority over citations). Project structure (new `chat.rs` parser + `conversation.rs` persist/compact; frontend `ChatSession.tsx`). **Implementation phasing A→B→C** (intent-chat MVP → fold pipeline into transcript → rolling-summary compaction).
- **research.md**: appended R8 (conversation drives existing deterministic pipeline via parsed `DraftAction`), R9 (chat persistence = dedicated `chat_message` table, not the DraftState blob), R10 (rolling-summary on single slot; semantic retrieval deferred).
- **data-model.md**: added persisted `chat_message` + `conversation_summary` (migration 023); corrected the stale "in-memory only" note (US5 already persists DraftState via migration 022); added FR-027–034 mapping.
- **contracts/conversation.md**: new — `send_chat_turn` + `load_conversation` commands, with invariants I1–I5 (no row deletion; model sees summary+recent only; no generative citation authority; state preserved in DraftState; single-slot summarization).
- **quickstart.md**: appended conversational walkthrough + manual FR checks.
- Ran `update-agent-context.sh claude` (CLAUDE.md updated).

## Outcome

- ✅ Impact: Feature 009 plan + design artifacts now cover the conversational session, chat persistence, and compaction as an amendment (no new feature); Constitution Check PASS with no violations.
- 🧪 Tests: none run this stage (planning); acceptance/unit suites named for the green stage.
- 📁 Files: plan.md, research.md, data-model.md, contracts/conversation.md, quickstart.md, CLAUDE.md.
- 🔁 Next prompts: `/sp.tasks` to generate phased tasks (A: migration 023 + chat.rs + conversation.rs append/load + send_chat_turn + ChatSession.tsx; B: full action coverage + live panels; C: compaction).
- 🧠 Reflection: setup-plan.sh overwriting an untracked plan.md is a footgun — reconstructed from spec + code + memory; consider committing specs/009 to avoid silent loss.

## Evaluation notes (flywheel)

- Failure modes observed: plan template clobbered the prior (untracked) plan.md.
- Graders run and results (PASS/FAIL): Constitution Check — PASS (13/13).
- Prompt variant (if applicable): n/a
- Next experiment (smallest change to try): run /sp.tasks scoped to Phase A only to keep the first slice shippable.
