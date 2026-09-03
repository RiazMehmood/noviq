---
id: 057
title: Clarify lock citation feedback labels
stage: spec
date: 2026-07-03
surface: agent
model: claude-opus-4-8
feature: 018-answer-feedback-loop
branch: release
user: Riaz Mehmood
command: /sp.clarify
labels: [feedback, spec, citations, clarify, ux]
links:
  spec: specs/018-answer-feedback-loop/spec.md
  ticket:
  adr:
  pr:
files:
  - specs/018-answer-feedback-loop/spec.md
tests:
  - none (spec-only change)
---

## Prompt

run /sp.clarify to lock the label wording

[interactive answers: Q1 accuracy labels → A; Q2 completeness labels → A]

## Response snapshot

Ran clarify narrowly scoped to option-label wording (prereq script aborts on `release` — not a
feature branch — so worked directly against specs/018-answer-feedback-loop/spec.md). Asked two
questions, both answered "A":

- Citation accuracy labels → **All citations correct** / **Some citations correct** /
  **Citations wrong/missing** / **Not sure** (enum `all`/`some`/`wrong`/`unsure`).
- Citation completeness labels → **Fully cited** / **Some citations missing** / **Not applicable**
  (enum `yes`/`missing`/`na`).

Added `## Clarifications` → `### Session 2026-07-03` with one bullet per answer; folded the locked
display labels into User Story 1 and FR-001; moved SME-role/sampling, beta-exit thresholds, and
retention policy under a "Still to resolve (before/at planning)" subheading.

## Outcome

- ✅ Impact: Officer-facing labels for both citation questions locked and mapped to stored enums;
  UI copy is now unambiguous for planning.
- 🧪 Tests: none (spec-only).
- 📁 Files: specs/018-answer-feedback-loop/spec.md.
- 🔁 Next prompts: /sp.plan — `answer_feedback` additive migration + two-question chip UI.
- 🧠 Reflection: check-prerequisites.sh requires a feature branch; on `release` it aborts, so
  clarify was run manually against the known spec path.

## Evaluation notes (flywheel)

- Failure modes observed: prereq script branch guard blocks non-feature branches.
- Graders run and results (PASS/FAIL): n/a
- Prompt variant (if applicable): n/a
- Next experiment (smallest change to try): resolve SME role + sampling rate at /sp.plan
