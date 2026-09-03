---
id: 0016
title: Heading-mislabel defect — Fix-1 detection + Fix-2 invariant guard + US8 tabular de-interleave
stage: green
date: 2026-06-16
surface: agent
model: claude-opus-4-8
feature: 010-one-ask-drafting
branch: 011-postgres-pgvector-port
user: Riaz Mehmood
command: (resume) proceed with next step → Fix-2 and US8, then build and re-index
labels: [heading-detection, citation-grade, index-time, chunking, indexing]
links:
  spec: specs/010-one-ask-drafting/spec-secretariat-summary-refactor.md
  ticket: docs/heading-mislabel-defect-diagnosis.md
  adr:
  pr:
files:
  - src-tauri/src/services/doc_profiles/rules.rs
  - src-tauri/src/services/heading_detector/regex_pass.rs
  - src-tauri/src/services/structure_parser.rs
  - src-tauri/src/services/structured_importer.rs
  - src-tauri/src/services/text_normalizer.rs
  - CHANGELOG_DEV.md
tests:
  - cargo test --lib → 428 passed, 1 failed (pre-existing doc_profiles::manual::detection_matches_manual_corpus)
  - rules::tests::bare_number_rule_with_title_and_dashes_detects_real_headings — ok
  - heading_detector::regex_pass::tests::rules_profile_detects_bare_number_title_headings — ok
  - heading_detector::regex_pass::tests::rules_profile_rejects_form_field_restart_among_high_rules — ok
  - structure_parser::label_invariant_tests::detects_absorbed_heading_mislabel — ok
  - text_normalizer::tests::us8_tabifies_multi_row_table_gutters_keeping_cells_verbatim — ok
  - text_normalizer::tests::us8_lone_bloated_line_still_collapses_to_single_space — ok
---

## Prompt

(Resume from heading-mislabel diagnosis.) Verify the already-written Fix-1 logic fast,
then proceed with Fix-2 (invariant guard) and US8 (tabular de-interleave), then build and
re-index. Citation-grade defect: stored chunk `unit_label` provision number disagreed with
the provision number that begins its own content (cited `Rule 54` for content `"55. Entry
into Force…"`). ~530 mislabeled chunks, all on the `rules` profile.

## Response snapshot

- **Standalone verification first** (build-ops discipline): replicated the exact `rules.rs`
  heading patterns + `regex_pass.rs` sequence filter + Fix-2 + US8 logic in a `regex_lite`-only
  `/tmp/headingcheck` project that compiles in seconds, avoiding the ~30-min monolith rebuild
  during iteration. All logic GREEN against real-DB line shapes before touching the main build.
- **Fix-1** (already drafted, now confirmed): general bare-number heading pattern in `rules.rs`
  + generic ascending-sequence filter (`drop_out_of_sequence_articles`) in `regex_pass.rs`.
- **Fix-2** (new): `detect_label_mismatches()` pure fn in `structure_parser.rs` mirroring the
  diagnosis SQL + `log::warn!` wired into `structured_importer::persist`. Conservative substring
  test; unit-tested on the live mislabel shapes.
- **US8** (new): `collapse_columns_per_line()` in `text_normalizer.rs` — multi-row table gutters
  become interior TABs (cells stay verbatim, never fuse), prose keeps single-space collapse.
  Safe geometry discriminator (adjacency) so existing collapse tests + act.rs 2-space boundary
  are untouched.
- Killed a stray rust-analyzer `cargo check --workspace --all-targets` holding the shared target
  dir before the test build (documented hazard).

## Outcome

- ✅ Impact: heading-detection miss that mislabeled ~530 rules-profile chunks is fixed at
  index-time; an invariant guard now catches any future regression mechanically; table rows no
  longer fuse into false sentences. All index-time → take effect on the next re-index.
- 🧪 Tests: `cargo test --lib` 428 passed / 1 failed (pre-existing `manual` detection test,
  unrelated). 5 new tests added, all green. Logic pre-verified standalone.
- 📁 Files: rules.rs, regex_pass.rs, structure_parser.rs, structured_importer.rs,
  text_normalizer.rs, CHANGELOG_DEV.md.
- 🔁 Next prompts: re-index corpus (in-app Index-Folder) → re-run diagnosis mismatch SQL (target
  0 on every rules-profile doc) → live spot-check §3 Delegation financial cells for US8 efficacy.
- 🧠 Reflection: replacing four accreted per-doc special-cases with one general pattern + a
  generic filter is the recurrence-proof shape; the invariant guard closes the "no one notices a
  wrong citation" gap that let this land repeatedly.

## Evaluation notes (flywheel)

- Failure modes observed: high-recall general pattern also matches form-fields/footnotes (handled
  by sequence filter); US8 table-vs-bloated-prose ambiguity (handled by multi-line adjacency).
- Graders run and results (PASS/FAIL): standalone logic harness PASS (Fix-1 5/5, Fix-2, US8);
  cargo --lib PASS (modulo pre-existing manual failure).
- Prompt variant (if applicable): n/a
- Next experiment (smallest change to try): after re-index, if any rules-profile doc still shows
  mismatch > 0, inspect those line shapes — likely a 5th heading punctuation variant or a tabular
  page needing the US8 path; widen the sequence-filter reset conditions only if proven on data.
