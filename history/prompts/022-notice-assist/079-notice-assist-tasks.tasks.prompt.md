---
id: 079
title: Notice Assist task breakdown
stage: tasks
date: 2026-07-10
surface: agent
model: claude-opus-4-8
feature: 022-notice-assist
branch: 022-notice-assist
user: Riaz Mehmood
command: sp.tasks
labels: [notice-assist, tasks, qwen3-8b, slot-filling]
links:
  spec: specs/022-notice-assist/spec.md
  ticket:
  adr:
  pr:
files:
  - specs/022-notice-assist/tasks.md
tests:
  - "n/a (tasks stage) — test tasks embedded per story (extraction parse, slot loop, command detect, template/grounding)"
---

## Prompt

proceed to /sp.tasks

## Response snapshot

Generated `specs/022-notice-assist/tasks.md` — 40 tasks organized by user story:
- Phase 1 Setup (T001–T003): capability module skeleton, config fragment stubs, additive
  `notice_assist_session` migration.
- Phase 2 Foundational (T004–T009): frame/data-model types, session repo seam,
  `is_notice_assist_command`, local-Qwen3-8B extraction generator accessor
  (`NOVIQ_NOTICE_EXTRACT_ENGINE=local`, `NO_THINK`), `run_turn` intercept, phase dispatcher.
- Phase 3 US1 (T010–T014): `/` command palette in ConfidentialComposer (reuse `@` machinery),
  assist-mode entry + banner.
- Phase 4 US2 (T015–T020): analysis prompt fragment, doc-text assembly (window+heading cues),
  Qwen call, tolerant JSON parse (retry-once→fallback), Analyze phase, fixture tests.
- Phase 5 US3 (T021–T025): ask.rs next/record/gap, Gather phase, "draft now" transition, N/A
  detector + no-re-ask guard, resume tests.
- Phase 6 US4 (T026–T031): corpus grounding + citation gate, show-cause template, Draft/compose
  with ⚠ gaps, export via export_draft, F020 masking check, template/grounding tests.
- Phase 7 US5 (T032–T034): audit-para template + selection branch + fixture test.
- Phase 8 Polish (T035–T040): quickstart smoke, VM cargo check/test (glibc runbook), npm build,
  prompt eval on real notices, CHANGELOG, corpus-coverage GA gate.

Dependency order + parallel examples + MVP (US1–US4) documented. Strict checklist format
(checkbox/ID/[P]/[USn]/path) throughout.

## Outcome

- ✅ Impact: executable, story-ordered task list ready for `/sp.implement`.
- 🧪 Tests: per-story test tasks (T014/T020/T025/T031/T034) + VM verification (T036).
- 📁 Files: specs/022-notice-assist/tasks.md.
- 🔁 Next prompts: `/sp.implement` (start MVP US1→US4); optionally `/sp.adr` first.
- 🧠 Reflection: US2 (extraction) is the risk center — front-loaded fixture tests + a prompt-eval
  polish task (T038) to de-risk the free-text-JSON reliability before UI polish.

## Evaluation notes (flywheel)

- Failure modes observed: none (planning artifact).
- Graders run and results (PASS/FAIL): n/a.
- Prompt variant (if applicable): n/a
- Next experiment (smallest change to try): T038 — measure JSON-parse success + allegation recall
  on 3–5 real notices/paras before building the loop UI.
