---
id: 0003
title: Wave A US1 chrome US2 labels green
stage: green
date: 2026-06-17
surface: agent
model: claude-opus-4-8
feature: 012-doc-intelligence-production
branch: 012-doc-intelligence-production
user: Riaz Mehmood
command: /sp.implement Wave A
labels: [green, ingestion, chrome-strip, labels, us1, us2]
links:
  spec: specs/012-doc-intelligence-production/spec.md
  ticket:
  adr:
  pr:
files:
  - src-tauri/src/db/pg/migrations/003_chunks_provenance.sql
  - src-tauri/src/services/parsers/chrome.rs
  - src-tauri/src/services/parsers/mod.rs
  - src-tauri/src/services/structure_parser.rs
  - src-tauri/src/services/structured_importer.rs
  - src-tauri/src/services/indexer.rs
  - src-tauri/tests/structured_importer_acceptance.rs
  - CHANGELOG_DEV.md
  - specs/012-doc-intelligence-production/tasks.md
tests:
  - "cargo test --lib = 467 passed, 0 failed (was 451; +14 chrome, +2 US2 resolve)"
---

## Prompt

proceed to /sp.implement Wave A  ·  "we want to complete this feature today."

## Response snapshot

Implemented Wave A sub-slice 1 (US1 chrome-strip + US2 hierarchical label
resolution) and got the full lib suite green.

- **US1**: new pure `parsers/chrome.rs` (cross-page repetition with digit-stripped
  key, margin geometry, form-field structure; provision-keep invariant; 14 tests)
  + `strip_chrome(text, page_offsets)`. Threaded into `structured_importer::import`
  (page_offsets from `indexer`) so chrome is removed BEFORE tree-build; persisted as
  separate `block_class='chrome'`+`is_boilerplate=TRUE` chunks (excluded from retrieval).
  Superseded the post-persist boilerplate flag (was matching 0/4277 live).
- **US2**: `structure_parser::resolve_bare_citations` qualifies bare sub-markers against
  the nearest named ancestor (`(i)`→`Rule 27(i)`) before coalescing; never invents a
  label; `flag_unresolved_labels` sets `label_status='unresolved'` for still-bare labels.
- **Migration 003** (additive): block_class, label_status, raw_marker, extraction_method,
  reconstruction_method, boundary_confidence + index.

Build cadence: full suite errored once on a contrived test fixture (body lines differing
only by a digit → correctly collapsed by digit-strip); fixed the fixture → green.
Two earlier "ghost" runs were caused by `| tail` suppressing output and a cargo
two-filter arg error; re-ran cleanly.

Deferred (documented): US3 split-action + heading confidence + boundary_confidence
population; provenance audit-field population; `/m{seq}` rename (internal path, not a
rendered citation); live-DB golden harness.

## Outcome

- ✅ Impact: D3 (chrome) fixed at its true upstream site + D5 (bare labels) core resolved;
  `cargo test --lib` 467/0. INDEX-TIME → needs one Wave A re-index + re-embed to take effect.
- 🧪 Tests: 467 passed, 0 failed (+16 net).
- 📁 Files: migration 003, chrome.rs (+mod), structure_parser, structured_importer, indexer,
  acceptance test sig, CHANGELOG, tasks.md.
- 🔁 Next prompts: Wave B (US4 deterministic drafting — no re-index); then US3 follow-up;
  then Wave C (US5 table engine). User to run the Wave A re-index to validate live.
- 🧠 Reflection: the downstream→upstream move is the crux — chrome must be classified before
  labels are assigned, exactly because the live failure was label-inheritance.

## Evaluation notes (flywheel)

- Failure modes observed: contrived test fixture (digit-only-differing bodies) tripped the
  digit-stripped repetition signal — real provisions are protected by the identity check.
- Graders run and results (PASS/FAIL): cargo test --lib PASS (467/0).
- Prompt variant (if applicable): n/a
- Next experiment (smallest change to try): Wave A re-index, then confirm chrome excluded +
  no bare `(i)` surfaced on the live SPSC/Delegation corpus.
