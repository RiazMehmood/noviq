---
id: 056
title: Simplify answer feedback to two citation questions
stage: spec
date: 2026-07-03
surface: agent
model: claude-opus-4-8
feature: 018-answer-feedback-loop
branch: release
user: Riaz Mehmood
command: (freeform)
labels: [feedback, spec, citations, ux]
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

no we just want citation accuracy and citation completness feedback in the form of all citation
are correct, some citation are correct, citations are missing, i am not sure, and for citation
completness yes completely citated, no some citations are missing, not applicable. that is it no
more boxes or etc just these two option in the form of radio button or numbers or any other form
possible to increase UX.

[followed by:] yes update the spec with these two citation questions

## Response snapshot

Recommended one-tap chips / radio groups (single-select) as the best-UX form and rewrote
`specs/018-answer-feedback-loop/spec.md` to replace the 👍/👎 + stage-mapped reason-chip +
free-text model with exactly two single-select questions:

- Citation accuracy: all correct / some correct / wrong or missing / not sure
- Citation completeness: fully cited / some missing / not applicable

Updated Overview, Clarifications, User Story 1, Edge Cases, FR-001..FR-005, FR-007, FR-009,
FR-014, Key Entities, and SC-001/SC-002. Answer trace persistence (FR-004), SME queue,
golden/regression promotion, offline/local-PG, and additive-migration constraints unchanged.
Storage narrows to two fields: `citation_accuracy` (all/some/wrong/unsure) and
`citation_completeness` (yes/missing/na) on `answer_feedback`, linked to `message_id`.

## Outcome

- ✅ Impact: Feedback surface reduced to two citation questions; spec internally consistent (no
  stray 👍/👎 / reason-code references outside "no 👍/👎" disclaimers).
- 🧪 Tests: none (spec-only).
- 📁 Files: specs/018-answer-feedback-loop/spec.md.
- 🔁 Next prompts: /sp.clarify option-label wording; then /sp.plan for `answer_feedback` schema +
  two-question UI.
- 🧠 Reflection: Kept per-citation feedback as an explicit v1.1 non-goal (FR-005) rather than
  deleting it, preserving the highest-value future signal.

## Evaluation notes (flywheel)

- Failure modes observed: n/a
- Graders run and results (PASS/FAIL): n/a
- Prompt variant (if applicable): n/a
- Next experiment (smallest change to try): finalize officer-friendly option labels in /sp.clarify
