---
id: 0007
title: US5 table row provenance tagging
stage: green
date: 2026-06-18
surface: agent
model: claude-opus-4-8
feature: 012-doc-intelligence-production
branch: 012-doc-intelligence-production
user: Riaz Mehmood
command: /sp.tasks
labels: [US5, table-engine, provenance, visibility, wave-d]
links:
  spec: specs/012-doc-intelligence-production/spec.md
  ticket: null
  adr: null
  pr: null
files:
  - src-tauri/src/services/parsers/table.rs
  - src-tauri/src/services/parsers/mod.rs
  - src-tauri/src/services/parsers/pdf.rs
  - src-tauri/src/services/parsers/{image,audio_video,text,ocr,document}.rs
  - src-tauri/src/services/structured_importer.rs
  - src-tauri/src/services/indexer.rs
tests:
  - "cargo test --lib parsers::table — 7 passed"
  - "cargo test --lib chunk_is_table_row — 1 passed (chunk_is_table_row_matches_emitted_row_verbatim)"
  - "full lib compiles clean across 8 touched files"
---

## Prompt

User: "ok do t033". Wave-D task T033 (after the T031/T032 safety contract): tag reconstructed
table row-units `block_class='table_row'` + `reconstruction_method='schema_anchored'` at persist
time so reflow coverage is measurable (currently `reconstruction_method` is null for all 5929
chunks — we are blind to coverage). Per the SDD task list and FINAL_DECISIONS §F Phase 1.

## Response snapshot

Threaded reflow provenance end-to-end, choosing a rewrite-stable + normalize-safe design:
- `reflow_tables_with_rows` returns the emitted verbatim row lines (`reflow_tables` kept as `.0`
  wrapper → 7 existing table tests untouched).
- New `ParsedDocument.reconstructed_rows` carries the rows from the pdftotext reflow path;
  `Vec::new()` on all other constructors (8 constructors across 6 parser files).
- `persist(.., reconstructed_rows)` → `tag_table_row_chunks` UPDATEs matching chunks (mirrors
  `persist_chrome_chunks`). Match = verbatim substring of the **normalized** row in chunk content
  (reflow runs pre-normalize; rows ≥12 chars to avoid prose collisions). Tagged rows stay
  retrievable (NOT boilerplate).
- `indexer.rs` passes `&parsed.reconstructed_rows`.
- Pure `chunk_is_table_row` unit-tested.

Surfaced one entanglement explicitly: precise tagging crosses the `text_normalizer` boundary and
the pending T034–T037 reflow rewrite. Chose verbatim-row-line matching because it is stable across
both (T035 still emits row lines) rather than offset-threading that would be thrown away.

## Outcome

- ✅ Impact: reflow coverage is now observable — after the next re-index,
  `SELECT reconstruction_method, count(*) FROM chunks GROUP BY 1` is the coverage metric; tagged
  rows are citable (retrievable, not boilerplate).
- 🧪 Tests: `parsers::table` 7/7 + `chunk_is_table_row` green; full lib compiles clean across 8
  files. DB-level tagging exercised on re-index (SQL mirrors the proven chrome path).
- 📁 Files: table.rs, mod.rs, pdf.rs, 5 other parser ctors, structured_importer.rs, indexer.rs.
- 🔁 Next prompts: T034 (delete recurrence) → T035–T037 (header+enumerator-anchored reconstruction)
  → re-index to confirm the 121 garble chunks collapse to fallback + measure table_row coverage.
- 🧠 Reflection: keeping `reflow_tables` as a thin wrapper avoided touching 7 passing tests; the
  normalize boundary was the non-obvious risk and drove the normalize-then-substring design.

## Evaluation notes (flywheel)

- Failure modes observed: provenance lost because reflow is inline text replacement, not a separate
  block class (unlike chrome). Normalize boundary would silently break naive byte-offset tagging.
- Graders run and results (PASS/FAIL): table + classifier unit tests PASS. Full-lib test run +
  re-index coverage check pending Wave-D slice-end.
- Prompt variant (if applicable): n/a
- Next experiment (smallest change to try): T034 delete recurrence, then re-index and verify
  `block_class='table_row'` count > 0 and `Section 20413`-class garble gone.
