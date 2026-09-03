---
id: 0001
title: Resume Feature 005 and close Removal Act fixture
stage: green
date: 2026-05-25
surface: agent
model: claude-opus-4-7
feature: 005-structured-importer
branch: 005-structured-importer
user: engr.riaz2010@gmail.com
command: resume where we left
labels: [feature-005, structured-importer, doc-profiles, act, pakistani-legal]
links:
  spec: docs/superpowers/specs/2026-05-24-generic-structured-importer-design.md
  ticket: ~
  adr: ~
  pr: ~
files:
  - path: src-tauri/src/services/doc_profiles/act.rs
    change: modified
  - path: CHANGELOG_DEV.md
    change: appended
tests:
  - cmd: cargo test --test structured_importer_acceptance
    result: pass (3/3, 1.33s)
  - cmd: cargo test --lib
    result: pass (170/171; 1 pre-existing failure in manual profile detection test, unrelated)
---

## Prompt

resume where we left

## Response snapshot

Resumed Feature 005 from pause-time note (2026-05-24 night). Memory hypothesised
the Removal Act fixture failed acceptance due to coverage below the 10%
threshold, with three suggested fixes (lower threshold, add nested-clause
pattern, add marginal-label pass).

Per `feedback-diagnose-before-code`, ran a one-shot diagnostic test before
patching. Hard numbers: normalized_len=20628, doc_type=`act` ✓,
**headings=0**, coverage=0.00. The threshold was not the issue — the
profile's heading regexes matched nothing at all in this fixture.

Inspected the fixture and identified three heading shapes the existing
patterns missed:
1. Marginal-label section (`Short title     1. (1) ...`).
2. Bare-number section with title or bracket marker
   (`3. [Dismissal...]`, `4. Suspension. ---`, ... 13 occurrences).
3. Indented clauses (`        (2) It extended ...`, 7 occurrences) —
   the existing `^\(N\)` pattern required column-0 `(`.

Applied a purely-additive patch to `act` profile: added two new heading
patterns (bare-number-section, marginal-label-section) and prefixed `\s*`
on four existing patterns so indent and form-feed page-breaks don't
defeat them. Threshold left at 0.10.

Post-fix: 27 headings, coverage 0.91, structured=true. Acceptance 3/3 pass.
Throwaway diagnostic test deleted; CHANGELOG_DEV entry appended.

## Outcome

- ✅ Impact: Feature 005 acceptance gate now 3/3 green offline. All four
  Sindh fixtures route to non-generic profiles AND register
  `structured=true`. Branch `005-structured-importer` ready for manual
  UI re-index validation and merge to master.
- 🧪 Tests: `cargo test --test structured_importer_acceptance` → 3 pass;
  `cargo test --lib` → 170 pass, 1 pre-existing failure in
  `services::doc_profiles::manual::tests::detection_matches_manual_corpus`
  (the test confuses detection-patterns with heading-patterns; not
  introduced by this change).
- 📁 Files: 1 source file (act.rs) + CHANGELOG_DEV.md. ~22 lines added,
  5 lines reflowed.
- 🔁 Next prompts: (a) drive-by fix for the pre-existing `manual`
  detection test; (b) launch `npm run tauri dev` to confirm the startup
  hook re-enqueues all six Sindh PDFs at structure_importer_version=0
  and that `graph_nodes` jumps from 1488 to >2000; (c) merge to master.
- 🧠 Reflection: The pause-time memory note's hypothesis was wrong
  (threshold vs zero-matches). The diagnose-before-code rule prevented
  ~30 min of futile threshold tuning. Worth noting that
  `headings=0 → coverage=0` and `headings>0 → coverage<threshold` look
  identical from the `structured=false` outcome; a future improvement
  is to log both numbers when the importer rejects a doc.

## Evaluation notes (flywheel)

- Failure modes observed: existing heading patterns assumed clean
  column-0 numbering; real OCR text has marginal labels, indented
  clauses, and form-feed page-break prefixes. Pattern design must
  budget for `\s*` and label-prefix shapes.
- Graders run and results (PASS/FAIL): structured_importer_acceptance
  3/3 PASS.
- Prompt variant (if applicable): n/a — code-level change, not prompt.
- Next experiment (smallest change to try): manual UI re-index of the
  six Sindh PDFs; capture before/after `graph_nodes` and
  `doc_type, structure_status` columns.
