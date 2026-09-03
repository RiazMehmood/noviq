---
id: 0003
title: tasks query decomposition tdd
stage: tasks
date: 2026-06-07
surface: agent
model: claude-opus-4-8
feature: 008-query-decomposition
branch: 008-query-decomposition
user: Riaz Mehmood
command: /sp.tasks
labels: [tasks, tdd, query-decomposition, intent-router]
links:
  spec: specs/008-query-decomposition/spec.md
  ticket:
  adr:
  pr:
files:
  - specs/008-query-decomposition/tasks.md
tests:
  - "TDD task list: each behavioural fn has a RED test before GREEN impl; suites = cargo test -p echo-ai --lib intent_router::decompose + 15-prompt eval (T020)"
---

## Prompt

Architect: "run /sp.tasks".

## Response snapshot

Generated tasks.md: 20 tasks across 7 phases, organized by the four P1 user stories,
TDD throughout (RED before GREEN). Only two files touched —
`intent_router/decompose.rs` (NEW) and one hook in `intent_router/mod.rs`.

- Phase 1 Setup (T001-02): module + types (Capability/SubQuestion/SubAnswer/MergedAnswer,
  MAX_SUBQUESTIONS=6).
- Phase 2 Foundational (T003-08): gate `decomposition_enabled` (RED matrix→impl), merge
  `merge_subanswers`+`remap_citations` (RED→impl), dispatch orchestration skeleton
  (sequential loop, cancel) with `detect_and_split` stub returning None (single-pass stays
  inert until US1/US2).
- Phase 3 US1 (T009-11): enumeration `(a)(b)(c)` branch — split into stem+facet
  sub-questions; false-positive boundary tests (single marker / sub-clause numbering).
- Phase 4 US2 (T012-14): multi-article branch (slots.section_paths>=2, clause split +
  per-article fallback); merge reports absent provision not-found.
- Phase 5 US3 (T015): single-fact yields None + no regression.
- Phase 6 US4 (T016-17): gate-off bypass (unit + manual big-model/ECHO_DECOMPOSE=0).
- Phase 7 Polish (T018-20): trace step, CHANGELOG, full 15-prompt eval acceptance (V4).

MVP = US1 (Q15). Parallel: T003∥T005; after Foundational T009∥T012∥T015∥T016, but
T010/T013 both edit detect_and_split so serialized.

## Outcome

- ✅ Impact: executable, dependency-ordered TDD task list; MVP clearly = US1.
- 🧪 Tests: every fn gets a RED test first; acceptance = 15-prompt eval (T020).
- 📁 Files: tasks.md.
- 🔁 Next prompts: /sp.implement (or implement task-by-task). First action = T001/T002
  scaffold, then T003 RED gate test.
- 🧠 Reflection: putting gate+merge in Foundational (shared) and only the two split
  branches in US1/US2 keeps stories independently testable while avoiding duplicate infra.

## Evaluation notes (flywheel)

- Failure modes to guard (encoded as tests): single-marker false trigger, sub-clause
  numbering false trigger, citation [n] collision across blocks, gate firing on big GGUF.
- Graders run and results (PASS/FAIL): n/a (task generation).
- Prompt variant (if applicable): n/a.
- Next experiment (smallest change to try): T009 RED — detect_and_split(Q15) → 3 subs.
</content>
