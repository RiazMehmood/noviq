---
id: 0008
title: US5 schema anchored table reconstruction
stage: green
date: 2026-06-18
surface: agent
model: claude-opus-4-8
feature: 012-doc-intelligence-production
branch: 012-doc-intelligence-production
user: Riaz Mehmood
command: /sp.tasks
labels: [US5, table-engine, header-anchored, delete-recurrence, wave-d, TDD]
links:
  spec: specs/012-doc-intelligence-production/spec.md
  ticket: null
  adr: null
  pr: null
files:
  - src-tauri/src/services/parsers/table.rs
tests:
  - "cargo test --lib parsers::table — 8 passed (table_without_enumerator_falls_back RED→GREEN; real_delegation_page8 under schema-anchoring; each_page_self_anchors)"
---

## Prompt

User: "ok go for next tasks" (after re-index confirmed T031–T033: confident-garble 121→2, COLLEGE
EDUCATION garble→0, 95 table_row tagged — but recurrence still produced column-garbled rows that
pass the monotonic-serial gate). Proceed to Wave-D core: T034 (delete recurrence) + T035
(header+enumerator-anchored reconstruction) per docs/US5_table_engine_FINAL_DECISIONS.md.

## Response snapshot

TDD: wrote RED `table_without_enumerator_falls_back` (a clean monotonic table with NO `(1)(2)(3)(4)`
enumerator — recurrence wrongly reconstructed it), verified failing.

Then the GREEN refactor of `parsers/table.rs`:
- DELETED `detect_columns` (recurrence), `detect_continuation`, `line_aligns`, `take_header`,
  `starts_with_digit`, and the cross-page `head.cols` stitch loop. `reflow_tables_with_rows` is now a
  per-page loop (each schedule page re-declares its schema ~55×, so pages self-anchor).
- ADDED `detect_schema` (enumerator `(1)…(N)` + N-cell header → count + labels), `group_cells`,
  `body_column_lefts` (column left-edges from serial-led DATA rows only; keep N strongest clusters, no
  support threshold ⇒ sparse Extent survives, over-split `(i)` drops), `is_serial`. `detect_page_table`
  rewired: schema → anchor rows → body lefts → region-bound (stop at non-serial leading col) →
  build_rows → verify_table → linearize(schema.labels).
- Explicitly REJECTED deriving bands from header/enumerator x-positions (headers centered, body
  left-aligned → re-splices Extent into authority column; proved on page-8 geometry).

Updated tests: real_delegation_page8 (verbatim page-8 bbox) passes under schema-anchoring;
each_page_self_anchors replaces the deleted cross-page-stitch test; heading/non-monotonic tests now
carry enumerators so they exercise verify_table. 8/8 green; full lib compiles.

## Outcome

- ✅ Impact: column garble is now structurally addressed (not just gated) — columns come from the
  document's declared schema, not statistics. Non-enumerator tables fall back (conservative/correct).
- 🧪 Tests: `cargo test --lib parsers::table` 8/8; full lib compiles. Index-time → effect on re-index.
- 📁 Files: src-tauri/src/services/parsers/table.rs.
- 🔁 Next prompts: re-index to verify Delegation schedule rows reconstruct correctly + recurrence
  column-garble gone + table_row coverage; then T036 (cross-page row continuation), T038–T040 (US6
  schedule boundaries — fixes Rule-label misattribution + Section 20413), T041–T042 (curation).
- 🧠 Reflection: the live re-index between T033 and T034 was decisive — it proved the gate alone
  leaves column-garble, exactly motivating the recurrence deletion. The centered-header vs
  left-aligned-body distinction was the key implementation pitfall avoided.

## Evaluation notes (flywheel)

- Failure modes observed (pre-fix, live): recurrence emitted monotonic-but-column-garbled rows; a
  numbered officer LIST reconstructed as a 4-col table.
- Graders run and results (PASS/FAIL): table unit suite PASS (8/8). Live re-index reconstruction
  + coverage check pending.
- Prompt variant (if applicable): n/a
- Next experiment (smallest change to try): re-index; query reconstructed Delegation rows for
  verbatim cells + `block_class='table_row'` count; confirm no `— To whom delegated <garbage>` rows.
