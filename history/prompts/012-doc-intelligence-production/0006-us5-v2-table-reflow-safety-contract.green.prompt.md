---
id: 0006
title: US5 v2 table reflow safety contract
stage: green
date: 2026-06-18
surface: agent
model: claude-opus-4-8
feature: 012-doc-intelligence-production
branch: 012-doc-intelligence-production
user: Riaz Mehmood
command: /sp.tasks
labels: [US5, table-engine, safety-contract, TDD, wave-d]
links:
  spec: specs/012-doc-intelligence-production/spec.md
  ticket: null
  adr: null
  pr: null
files:
  - src-tauri/src/services/parsers/table.rs
  - specs/012-doc-intelligence-production/tasks.md
  - docs/US5_table_engine_FINAL_DECISIONS.md
  - CHANGELOG_DEV.md
tests:
  - "cargo test --lib parsers::table — 7 passed, 0 failed (2 new: heading_inside_table_band_forces_fallback, non_monotonic_serials_force_fallback)"
---

## Prompt

Following multiple independent architecture reviews, the user (acting as architect) asked
me to act as technical lead and make final implementation decisions for the US5 table
engine, then directed: "use sdd" and "ok" to begin Phase 1 of the agreed roadmap.

Established findings: the Delegation PDF is digital; table reflow IS firing; the failure is
**confident corruption**, not missed reconstruction; the document repeats its
`Sr.# | Nature of power | To whom delegated | Extent` header + `(1)(2)(3)(4)` enumerator
~55× (an explicit self-declared schema); schedule content is mis-attributed to preceding
rules; the project is citation-grade (a missed extraction is acceptable, a scrambled one is
not); corpus is 10 PDFs; offline Rust desktop is a hard requirement.

Phase 1 objective: make `reflow_tables` a total verifiable function so today's confident
garble falls back to `-layout`, via SDD + TDD.

## Response snapshot

- Folded the tech-lead decisions into `docs/US5_table_engine_FINAL_DECISIONS.md` (§A delete
  recurrence → header+enumerator-anchored; §B safety contract; §C schedule boundaries; §D
  curation backstop; §E reject PaddleOCR for digital corpus; §F roadmap; §G rejected prior
  recommendations).
- SDD: added Wave D phases to `tasks.md` — Phase 9 (US5 v2, T031–T037), Phase 10 (US6
  schedule boundaries, T038–T040), Phase 11 (curation, T041–T042); task summary 30→42.
- TDD (T031/T032): wrote two RED tests asserting byte-for-byte `-layout` fallback; verified
  they FAIL for the right reason (heading folded into a row; serials emitted 1,3,2); then
  implemented the GREEN minimum: `verify_table` gate in `reflow_tables` + `parse_serial` +
  `contains_heading_run`. `cargo test --lib parsers::table` = 7/7, no regression.

## Outcome

- ✅ Impact: confident table corruption is now structurally impossible — a reconstruction is
  emitted only if it passes a decidable gate (≥2 strictly-increasing integer serials, no
  heading absorbed into a cell); otherwise the page keeps its `-layout` text verbatim.
- 🧪 Tests: `cargo test --lib parsers::table` 7/7 (2 new RED→GREEN; 5 prior reconstructions
  unaffected). Change is additive (pure fns + one gate call; no API change).
- 📁 Files: table.rs (gate + helpers), tasks.md (Wave D), FINAL_DECISIONS doc, CHANGELOG_DEV.
- 🔁 Next prompts: T033 (tag `block_class='table_row'`/`reconstruction_method` for
  visibility) → T034 (delete recurrence) → T035–T037 (header+enumerator-anchored
  reconstruction) → re-index to confirm the 121 garble chunks collapse to fallback.
- 🧠 Reflection: the live DB evidence (121 linearizer-signature chunks; 55× repeated header)
  was decisive — it overturned the earlier "reflow not firing" assumption and reframed the
  fix from "better statistics" to "read the declared schema."

## Evaluation notes (flywheel)

- Failure modes observed: recurrence detector drops sparse `Extent` (34% support), splits
  col-3 via header/body alignment mismatch, absorbs section headings as rows, manufactures
  phantom columns from wide intra-cell gaps.
- Graders run and results (PASS/FAIL): table unit suite PASS (7/7). Full-lib + re-index gate
  pending Wave D slice-end.
- Prompt variant (if applicable): n/a
- Next experiment (smallest change to try): T033 visibility tagging, then re-index and query
  `reconstruction_method` coverage + confirm `Section 20413`-class garble is gone.
