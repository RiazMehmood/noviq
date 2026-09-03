---
id: 064
title: User documents task breakdown
stage: tasks
date: 2026-07-05
surface: agent
model: claude-fable-5
feature: 021-user-documents
branch: 021-user-documents
user: Riaz Mehmood
command: /sp.tasks
labels: [tasks, privacy, retrieval-scoping, uploads, sharing, attachments]
links:
  spec: specs/021-user-documents/spec.md
  ticket: none
  adr: none
  pr: none
files:
- specs/021-user-documents/tasks.md
tests:
- T006 unit tests for visible_source_ids invariant; quickstart §1–§5 verification tasks embedded per story
---

## Prompt

run the tasks

## Response snapshot

Generated specs/021-user-documents/tasks.md — 33 tasks in 7 phases:

- Phase 1 Setup (T001): changelog + design re-read, branch check.
- Phase 2 Foundational (T002–T006, blocks all): PG migration 009 + SQLite mirror,
  Source struct + RequestScope + visible_source_ids + usage_for_owner, indexing
  ownership threading, invariant unit tests.
- Phase 3 US1 scoped retrieval (T007–T011, MVP): run_turn swap, api/query.rs scope,
  capability-path audit (the SC-001 surface), entity/graph endpoint filters, VM verify.
- Phase 4 US2 My documents (T012–T019): quota enforcement, my_usage/delete_my_document/
  admin usage endpoints + routes, transport, un-gated My-documents section, admin FE table.
- Phase 5 US3 sharing (T020–T024): set_visibility (folder cascade tx), listings include
  shared + exclude attachments, badges/toggle FE.
- Phase 6 US4 attachments (T025–T029): attach_document, list/promote endpoints,
  delete_conversation cascade, ChatPage paperclip/chips/promote.
- Phase 7 Polish (T030–T033): DB spot checks, per-owner dedup guard, changelog, merge+deploy.

Counts: US1=5, US2=8, US3=5, US4=5, setup/foundational=6, polish=4. Parallel: migrations
T002∥T003; US2 backend trio T013∥T014∥T18; US1 T009∥T010; US2 backend ∥ US1 audit.
Dependency rule captured: US2+ must never deploy without US1 (uploads without scoping =
live leak). MVP = Phase 2 + US1.

## Outcome

- ✅ Impact: immediately executable, dependency-ordered task list; each story ends with its
  quickstart verification.
- 🧪 Tests: T006 unit tests (privacy invariant), SQL asserts + browser walkthroughs per story.
- 📁 Files: specs/021-user-documents/tasks.md.
- 🔁 Next prompts: /sp.implement (or start with Phase 2+US1 as standalone privacy deploy);
  optional /sp.adr user-document-ownership-scoping still open.
- 🧠 Reflection: T009 (capability-path audit) is the highest-risk task — the citation
  fast-path incident showed bypass paths are this codebase's recurring failure mode.

## Evaluation notes (flywheel)

- Failure modes observed: none.
- Graders run and results (PASS/FAIL): checklist format validated — all 33 tasks have
  checkbox + ID + file paths; story phases labeled [US1]–[US4]; setup/foundational/polish unlabeled.
- Prompt variant (if applicable): n/a
- Next experiment (smallest change to try): consider making T011 (US1 VM verify) a gate
  before starting US2 frontend work to keep the no-deploy-without-US1 rule enforced.
