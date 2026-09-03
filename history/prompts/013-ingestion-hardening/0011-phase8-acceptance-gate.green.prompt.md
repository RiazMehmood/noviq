---
id: 0011
title: Phase 8 — acceptance harness + SC-001…SC-009 gate (end-to-end)
stage: green
date: 2026-06-20
surface: agent
model: claude-opus-4-8
feature: 013-ingestion-hardening
branch: 013-ingestion-hardening
user: engr.riaz2010@gmail.com
command: "go ahead with phase 8"
labels: [ingestion, acceptance, end-to-end, gate, chapter-detection]
links:
  spec: specs/013-ingestion-hardening/spec.md
  ticket:
  adr:
  pr:
files:
  - src-tauri/tests/ingestion_hardening_acceptance.rs
  - src-tauri/tests/fixtures/ingestion/fundamental_rights_layout.txt
  - src-tauri/src/services/doc_profiles/manual.rs
  - src-tauri/src/services/heading_detector/regex_pass.rs
  - specs/013-ingestion-hardening/tasks.md
  - MODULAR_IMPROVEMENT_LOG.md
tests:
  - "cargo test --test ingestion_hardening_acceptance: 5/5"
  - "cargo test --lib: full suite green (chapter fix, no regressions)"
---

## Prompt

go ahead with phase 8

## Response snapshot

Built the acceptance harness running the REAL deterministic import+persist pipeline
on the fundamental_rights layout fixture (first true end-to-end check). 5 SC tests:
SC-001 (articles + chapter parentage), SC-002 (no Section leak), SC-004/006 (verbatim),
SC-005 (page_number), SC-009 (graph integrity + capture ran).

First run: 4/5 — caught a real bug the unit tests missed: centered `CHAPTER 1.`
headings didn't match `^CHAPTER` (no `[ \t]*`). Fixed manual.rs to `^[ \t]*CHAPTER`,
regression-locked the regex_pass test with indentation. Re-run: 5/5.

Also recorded the crate-slimming backlog (llguidance) the user wants after testing.

## Outcome

- ✅ Impact: Feature 013 Phases 2–8 code-complete + automated end-to-end gate green;
  D1/D2/D3/D4/D7/D8/D12/US5 all verified on the real document.
- 🧪 Tests: acceptance 5/5; full lib green.
- 📁 Files: acceptance harness + fixture, manual.rs (chapter indent fix), regex_pass.rs
  (regression lock), tasks.md, MODULAR_IMPROVEMENT_LOG.md.
- 🔁 Next prompts: live force-pending re-index + commit; then crate slimming.
- 🧠 Reflection: the end-to-end gate immediately earned its keep — a green unit test
  (un-indented CHAPTER) hid a live failure (centered CHAPTER).

## Evaluation notes (flywheel)

- Failure modes observed: unit-test fixtures that don't mirror real layout (indentation)
  give false confidence — acceptance harness on real text is essential.
- Graders run and results (PASS/FAIL): acceptance gate PASS (5/5); lib PASS.
- Prompt variant (if applicable): n/a
- Next experiment (smallest change to try): live re-index the 3-doc folder; compare DB
  counts to the harness expectations + the Desktop baseline.
