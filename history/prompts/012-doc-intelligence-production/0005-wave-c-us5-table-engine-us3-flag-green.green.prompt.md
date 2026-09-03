---
id: 0005
title: Wave C US5 table engine US3 flag green
stage: green
date: 2026-06-18
surface: agent
model: claude-opus-4-8
feature: 012-doc-intelligence-production
branch: 012-doc-intelligence-production
user: Riaz Mehmood
command: /sp.implement (Wave C / US5 + US3)
labels: [green, tables, cross-page, headings, us5, us3]
links:
  spec: specs/012-doc-intelligence-production/spec.md
  ticket:
  adr:
  pr:
files:
  - src-tauri/src/services/parsers/table.rs
  - src-tauri/src/services/structured_importer.rs
  - CHANGELOG_DEV.md
  - specs/012-doc-intelligence-production/tasks.md
tests:
  - "cargo test --lib = 477 passed, 0 failed (+1 cross-page table test)"
---

## Prompt

Do US5 + US3 now (AskUserQuestion answer)

## Response snapshot

US5 — Rust-native cross-page table engine (parsers/table.rs, confident-only):
- Projection-profile column detection (detect_columns reimplemented over word
  x-extent occupancy bands split by gutters).
- Cross-page stitching: per-page strong anchor + detect_continuation(words, cols)
  relaxed continuation keyed to the anchor column signature; repeated header
  dropped (take_header); continuation post-prose preserved; shared build_rows.
- The real live gap was that each page of a cross-page table had <3 multi-cell
  lines so detection never fired; anchor+relaxed-continuation fixes it.
- New test cross_page_table_stitches_rows_across_break; existing tests preserved.

US3 — label≠content invariant promoted from warning to a flag:
- structured_importer::flag_label_content_mismatches sets label_status='unresolved'
  on any chunk whose content begins with a provision number disagreeing with its
  label (reuses detect_label_mismatches per chunk). Refuse-or-flag. Full
  re-segmentation split deferred.

cargo test --lib = 477 passed, 0 failed. Both index-time → next re-index.

## Outcome

- ✅ Impact: cross-page tables now reconstruct (D1); mislabeled chunks flagged not
  shipped (D4). Confident-only ⇒ no prose regression. All 5 user stories now have
  shipped code (US3 partial: flag yes, split/confidence deferred).
- 🧪 Tests: 477 lib passed, 0 failed.
- 📁 Files: table.rs, structured_importer.rs, CHANGELOG, tasks.md.
- 🔁 Next prompts: one Wave C re-index → verify Delegation rows reconstruct +
  label_status corrected; optional follow-ups (table_row tagging, US3 split,
  inline P-a-g-e scrub, chat query_router chrome exclusion).
- 🧠 Reflection: the per-page detection threshold was the true blocker to
  cross-page tables; splitting anchor (strong) vs continuation (relaxed, keyed to
  anchor cols) is what makes a split row reconstruct while staying confident-only.

## Evaluation notes (flywheel)

- Failure modes observed: per-page ≥3-line threshold prevented cross-page firing — solved by anchor+relaxed-continuation.
- Graders run and results (PASS/FAIL): cargo test --lib PASS (477/0).
- Prompt variant (if applicable): n/a
- Next experiment (smallest change to try): Wave C re-index; inspect the live Delegation table reconstruction + verbatim cells.
