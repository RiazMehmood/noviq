---
id: 002
title: Tasks answer feedback loop
stage: tasks
date: 2026-07-03
surface: agent
model: claude-opus-4-8
feature: 018-answer-feedback-loop
branch: release
user: Riaz Mehmood
command: /sp.tasks
labels: [feedback, tasks, citations, postgres, api, solidjs]
links:
  spec: specs/018-answer-feedback-loop/spec.md
  ticket:
  adr:
  pr:
files:
  - specs/018-answer-feedback-loop/tasks.md
tests:
  - planned — cargo test answer_feedback (insert/supersede/summary); browser US1 verify
---

## Prompt

/sp.tasks  (no additional user input)

## Response snapshot

Generated specs/018-answer-feedback-loop/tasks.md — 19 tasks, US1-organized, from plan.md +
data-model.md + contracts/feedback-api.md. Phases: (1) Setup — branch; (2) Foundational (BLOCKING)
— migration 008_answer_feedback.sql, repository answer_feedback.rs (upsert/supersede + current_for +
summary), module wiring, repo unit tests [P]; (3) US1 MVP — POST /api/feedback handler + route +
desktop alias, transport submitFeedback [P], CitationFeedback.tsx two-chip control [P], ChatPage
mount; (4) FR-009 operator aggregate (cross-cutting, separable) — /api/admin/feedback/summary +
admin FE [P]; (5) Polish — VM compile+test, quickstart verify, CHANGELOG_DEV, PHR, and a DEFERRED
reload re-hydration follow-up (T019).

All tasks follow `- [ ] Txxx [P?] [US1?] desc + file path`. Dependencies: Foundational blocks US1
and aggregate; within US1 backend (T006→T007→T008) precedes end-to-end FE test, T009/T010 parallel.
MVP = Phases 1–3 (collection only); aggregate is a second-pass ship.

## Outcome

- ✅ Impact: Executable, dependency-ordered task list for the two-question citation feedback MVP;
  small surface (1 migration, 1 repo, 1 route, 1 transport helper, 1 component, 1 mount) + separable
  aggregate.
- 🧪 Tests: none run (task planning). Planned: cargo test answer_feedback + browser US1.
- 📁 Files: specs/018-answer-feedback-loop/tasks.md.
- 🔁 Next prompts: /sp.implement (start T002 migration, verify it applies on the VM, then repo →
  route → UI).
- 🧠 Reflection: Kept reload re-hydration out of MVP (T019 deferred) to honor the user's
  keep-it-simple constraint; in-session selection state suffices for the first ship.

## Evaluation notes (flywheel)

- Failure modes observed: n/a (task generation).
- Graders run and results (PASS/FAIL): n/a
- Prompt variant (if applicable): n/a
- Next experiment (smallest change to try): implement + apply migration 008 on the VM first; confirm
  `\d answer_feedback` before wiring the route/UI.
