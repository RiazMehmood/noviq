---
id: 142
title: Table and parser defects at the marker seam
stage: green
date: 2026-08-05
surface: agent
model: claude-opus-5
feature: general
branch: release-deploy
user: Riaz Mehmood
command: "fix table and parser issues."
labels: [tables, parsers, ingestion, structured-importer, pdfplumber]
links:
  spec: specs/023-structured-table-capture/spec.md
  ticket: n/a
  adr: n/a
  pr: n/a
files:
  - src-tauri/src/services/parsers/table.rs
  - src-tauri/src/services/parsers/table_sidecar.rs
  - src-tauri/src/services/parsers/pdf.rs
  - src-tauri/src/services/structured_importer.rs
  - CHANGELOG_DEV.md
tests:
  - cargo test --lib --no-default-features --features server (VM): 921 passed / 2 failed
  - "baseline c367a71a for comparison: 913 passed / 2 failed (same two)"
  - table.rs: caption_band_is_not_a_column_label
  - table.rs: continuation_pages_join_the_table_and_inherit_its_header
  - table.rs: a_declared_header_or_a_page_gap_starts_a_new_table
  - table.rs: a_one_page_gap_is_crossed_only_when_the_serials_continue
  - table.rs: an_oversized_table_splits_into_units_that_keep_the_header
  - table.rs: markers_never_share_a_line_with_a_page_break
  - structured_importer.rs: marker_glued_to_a_page_break_still_opens_the_region
  - structured_importer.rs: an_unclosed_region_cannot_swallow_the_next_page
  - "table_sidecar.rs: spike_structured_tables (gated, run against 5 real corpus PDFs)"
---

## Prompt

fix table and parser issues.

(Follow-up to a question in the same session asking what the data boundary,
table extraction issues and pending parser fix were.)

## Response snapshot

Diagnosed against the production database first, then fixed four defects that
all sit in the seam between the table reflow and the structured importer:

1. `⟦TBL:n⟧` markers were being glued to the form feed that joins pages, so a
   table opening at the top of a page never opened its region — its linearised
   rows stayed in the body and were minted as provisions (111 duplicate `clause`
   chunks in Delegation, a table row stored as APT `Rule 3`). A lost CLOSE marker
   deleted the following page's prose (RoB p.32 had no chunks at all). Fixed by
   newline-wrapping each rewritten page AND scanning page-by-page on the strip.
2. pdfplumber's `header` was accepted verbatim; on this corpus it is as often a
   caption band, a wrapped continuation, or a data row. Added a header test
   (named first column, no figures, no parenthesised lead, label-sized cells,
   matching the column count the DATA declares), a caption capture, and
   promotion of the real labels when they sit in the first body row.
3. Per-page tables now stitch into the logical table they were printed as, with
   header inheritance, a row/char budget, per-row page numbers, and a one-page
   gap crossed only on serial continuity.
4. The pdftotext tier discarded its table list while still writing the regions
   the importer deletes — the Manual for Development Projects had 63 tables and
   0 table units. Threaded through; the degraded fallback now strips markers
   rather than losing rows.

Verified against the real PDFs through the real sidecar: RoB Schedule-I is one
table over pp.26–34 with serials 1–39 complete (was 9 fragments answering 32);
Delegation and APT have 0 linearised rows left in the body; the Constitution
still yields 0 tables, so the prose guard did not regress.

## Outcome

- ✅ Impact: the table→provision contamination is removed at its source, the
  39-department list lives in one chunk, and every table row now carries the
  right column labels and its own page number.
- 🧪 Tests: 921 passed / 2 failed on the VM; both failures reproduce on the
  pre-change baseline and are untouched by this work. 7 new unit tests plus a
  gated real-PDF diagnostic.
- 📁 Files: 4 source files + changelog.
- 🔁 Next prompts: run the re-index (user runs it on GPU), then re-run the
  corpus gap check and confirm the Manual gets table units on its own tier.
- 🧠 Reflection: the four symptoms the architect had been carrying as separate
  items (count, fusion, mislabelled rows, missing tables) were one seam. Reading
  the production rows BEFORE theorising is what surfaced that — the fusion was
  never a geometry problem.

## Evaluation notes (flywheel)

- Failure modes observed: the first fix pass still left Schedule-I in four
  pieces; only running it against the real PDF exposed two more data shapes
  passing the header test (`11. | Food Department | …` and a blank-led wrapped
  fragment). A unit test built from imagined shapes would have shipped green.
- Graders run and results (PASS/FAIL): PASS — serials 1–39 complete in one
  table; PASS — 0 linearised rows left in the body on all three table-heavy
  documents; PASS — Constitution still 0 tables.
- Prompt variant (if applicable): n/a
- Next experiment (smallest change to try): after the re-index, ask "how many
  departments are there in the Rules of Business" and confirm 39 from the single
  Schedule-I chunk.
