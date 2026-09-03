---
id: 0020
title: US8 R1+R2 — tabular reflow + year-rule + boilerplate flag (GREEN 451/0)
stage: green
date: 2026-06-17
surface: agent
model: claude-opus-4-8
feature: 010-one-ask-drafting
branch: 011-postgres-pgvector-port
user: Riaz Mehmood
command: "start the TDD implementation of R1 and R2 run build and tests at end only" (corrected mid-flight to SDD, not TDD)
labels: [us8, ingestion, tabular, boilerplate, year-rule, green]
links:
  spec: specs/010-one-ask-drafting/spec-secretariat-summary-refactor.md
  ticket:
  adr:
  pr:
files:
  - src-tauri/src/services/parsers/table.rs
  - src-tauri/src/services/parsers/boilerplate.rs
  - src-tauri/src/services/parsers/pdf.rs
  - src-tauri/src/services/parsers/mod.rs
  - src-tauri/src/services/heading_detector/regex_pass.rs
  - src-tauri/src/services/structured_importer.rs
  - src-tauri/src/services/drafting/retrieval.rs
  - src-tauri/src/db/pg/migrations/002_chunks_boilerplate.sql
  - specs/010-one-ask-drafting/tasks-secretariat-summary-refactor.md
  - CHANGELOG_DEV.md
tests:
  - "cargo test --lib: 451 passed, 0 failed (was 441; +10)"
  - "parsers::table 3/3; parsers::boilerplate 6/6; regex_pass year-rule 1/1"
---

## Prompt

start the TDD implementation of R1 and R2 run build and tests at end only

(User then corrected: "we need to use SDD instead of TDD" — implemented via the
repo's spec→tasks→implement flow; tests added per tasks, single build at end.)

## Response snapshot

Implemented US8 R1 (robust ingestion) + R2 (generic boilerplate, flag-not-delete)
against tasks T019–T026, one build/test cycle at the end (NTFS ~30 min).

R1: new pure `parsers/table.rs::reflow_tables` (bbox word-geometry → table-region
detection → logical rows → headers-folded verbatim linearization; confident-only
fallback to `-layout`); `pdf.rs` IO shell (`-bbox` extract + `parse_bbox` +
`maybe_reflow_tables` behind `ECHO_TABLE_REFLOW` default ON; page_offsets
recomputed); year-rule `looks_like_year` guard in `heading_detector/regex_pass.rs`
`scan_with_profile` (no "Rule 2017" minted).

R2: new pure `parsers/boilerplate.rs::is_boilerplate` (structural signals,
provision-label-keep invariant, conservative); migration 002 `chunks.is_boilerplate`;
`structured_importer::flag_boilerplate_chunks` (flag at persist, never delete) +
`dry_run_boilerplate` read-only pre-re-index gate; `drafting/retrieval.rs`
`AND c.is_boilerplate = FALSE` on lexical + KNN-hydration.

Build-ops: stopped rust-analyzer (pid 19939) before the build to avoid the shared
target-lock contention; single `cargo test --lib` = 451/0.

## Outcome

- ✅ Impact: §3 tabular garble, year-as-rule labels, and junk-chunk ranking all addressed at index time; one re-index applies all three. Drafting retrieval no longer ranks flagged boilerplate (the ¶2 fabricated-citation source).
- 🧪 Tests: cargo test --lib 451 passed, 0 failed (+10 new; all green).
- 📁 Files: 2 new modules, 1 migration, 4 edits, tasks/CHANGELOG updated.
- 🔁 Next prompts: ONE re-index → verify Delegation rows coherent, 0 year-labels, dry_run_boilerplate zero false positives, prose zero regression; then extend boilerplate filter to chat query_router + expose dry-run as a command.
- 🧠 Reflection: scoped retrieval exclusion to the one_ask path only (where ¶2 junk appeared) to bound blast radius for a single blind build; chat query_router filter deferred.

## Evaluation notes (flywheel)

- Failure modes observed: none in lib tests; live re-index verification still pending (T026 b/c/d).
- Graders run and results (PASS/FAIL): lib suite PASS (451/0).
- Prompt variant (if applicable): n/a
- Next experiment (smallest change to try): run `dry_run_boilerplate` against the live corpus and eyeball the would-flag list before committing to a re-index.
