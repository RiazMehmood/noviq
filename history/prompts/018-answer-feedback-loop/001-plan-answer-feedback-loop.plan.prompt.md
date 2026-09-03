---
id: 001
title: Plan answer feedback loop
stage: plan
date: 2026-07-03
surface: agent
model: claude-opus-4-8
feature: 018-answer-feedback-loop
branch: release
user: Riaz Mehmood
command: /sp.plan
labels: [feedback, plan, citations, postgres, api, solidjs]
links:
  spec: specs/018-answer-feedback-loop/spec.md
  ticket:
  adr:
  pr:
files:
  - specs/018-answer-feedback-loop/plan.md
  - specs/018-answer-feedback-loop/research.md
  - specs/018-answer-feedback-loop/data-model.md
  - specs/018-answer-feedback-loop/contracts/feedback-api.md
  - specs/018-answer-feedback-loop/quickstart.md
tests:
  - planned — cargo test answer_feedback (insert/supersede/summary), browser US1 verify
---

## Prompt

/sp.plan  (no additional user input)

## Response snapshot

setup-plan.sh aborts on `release` (branch guard) so ran the plan manually against
specs/018-answer-feedback-loop/. Grounded the design in the live code: `chat_message.id` is minted
per web turn by `memory::append_turn` and returned in `TurnResponse.message_id` → chosen as the
feedback anchor (matches FR-003, no extra round-trip). FR-004 "full trace" already satisfied by
`query_log` + on-disk `/api/traces/*`, so no new trace store. `sqlx::migrate!` auto-applies numbered
`.sql`, so a single additive `008_answer_feedback.sql` deploys cleanly.

Produced plan.md (summary, technical context, 13-principle Constitution Check — one PRE-EXISTING
Principle IV PostgreSQL divergence, not widened; Complexity Tracking), research.md (D1–D6:
anchor, trace reuse, supersede/latest-wins, independent-nullable questions, NULL desktop user,
TEXT+CHECK enums), data-model.md (`answer_feedback` schema, two enums, write/read semantics),
contracts/feedback-api.md (`POST /api/feedback` + `GET /api/admin/feedback/summary` + transport
`submitFeedback`), quickstart.md (migration/psql/curl/browser verification + US1 acceptance).

Scope limited to User Story 1 (P1). US2 implicit signals, US3 SME queue + golden/regression sets,
FR-005 per-citation, FR-014 beta-exit dashboard explicitly deferred to follow-on plans.

## Outcome

- ✅ Impact: Complete Phase 0/1 plan for the two-question citation feedback MVP; no new deps; one
  additive PG migration; small API + SolidJS surface reusing the existing transport seam.
- 🧪 Tests: none run (planning). Planned: cargo test answer_feedback + browser US1.
- 📁 Files: plan.md, research.md, data-model.md, contracts/feedback-api.md, quickstart.md.
- 🔁 Next prompts: /sp.tasks to generate dependency-ordered tasks (migration → repo → route →
  transport → CitationFeedback.tsx → admin summary → VM compile + browser verify).
- 🧠 Reflection: Reusing message_id + existing trace avoided a large FR-004 build; the only real
  design choice was supersede semantics for re-taps.

## Evaluation notes (flywheel)

- Failure modes observed: setup-plan.sh / check-prerequisites.sh both branch-guard on `release`.
- Graders run and results (PASS/FAIL): n/a
- Prompt variant (if applicable): n/a
- Next experiment (smallest change to try): /sp.tasks, then implement migration 008 first and
  verify it applies on the VM before wiring the route/UI.
