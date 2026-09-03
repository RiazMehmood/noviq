---
id: 050
title: Chunking audit follow-ups — RC-H de-hyphenation, RC-F footnote anchoring, RC-E table-row citability
stage: green
date: 2026-07-01
surface: agent
model: claude-opus-4-8
feature: general
branch: release
user: noviq.ai.pk@gmail.com
command: (freeform) "resolve all" remaining chunk issues
labels: [chunking, ingestion, text_normalizer, structured_importer, hyphenation, footnotes, tables]
links:
  spec: docs/chunking-audit-2026-06-29.md
  ticket:
  adr:
  pr:
files:
  - src-tauri/src/services/text_normalizer.rs
  - src-tauri/src/services/structured_importer.rs
  - CHANGELOG_DEV.md
tests:
  - text_normalizer::tests::rc_h_joins_line_break_hyphen_with_surrounding_whitespace
  - text_normalizer::tests::rc_h_joins_same_line_break_hyphen_for_known_words_only
  - structured_importer::tests::footnote_marker_extracts_leading_ordinal
  - structured_importer::tests::footnote_anchors_ambiguous_page_via_marker
  - structured_importer::tests::nearest_named_ancestor_walks_to_first_real_citation
---

## Prompt

resolve all.  (remaining chunk issues from the 2026-06-29 forensic audit)

## Response snapshot

Resolved three deferred, deterministic audit items (RC-C was already fixed;
RC-PG/A/B/D shipped earlier):

- **RC-H** (`text_normalizer.rs`): broadened the line-break hyphen regex to
  tolerate whitespace around the newline (`[a-z]-[ \t]*\n[ \t]*[a-z]`) and added
  a dictionary-gated same-line break-hyphen rejoin in `repair_spaced_words`
  ("Govern- ment"→"Government"), verbatim-safe.
- **RC-F** (`structured_importer.rs`): `anchor_citation_for_page` dropped every
  multi-provision page → most footnotes never linked. Added `footnote_marker` +
  `anchor_citation_for_footnote` that rescues an ambiguous page by matching the
  footnote marker digit to the in-body amendment marker (`N[`/`]N`), unique-only.
- **RC-E** (`structured_importer.rs`): table rows under `SCHEDULE-I`/`ANNEXURE-A`
  stayed unresolved because `named_provision` needs `keyword<space>ordinal`. New
  `nearest_named_ancestor` walks `parent_chunk_id` to the first non-bare ancestor
  (skips the filename document root) and relabels the row `"<ancestor> — <serial>"`.

NOT done (need live-VM reproduction): RC-T (page-spanning headers + merged cells —
also rebuilds the frozen Python sidecar; merged-cell fill risks inventing data)
and LG-Act OCR page overshoot.

## Outcome

- ✅ Impact: fewer broken words in retrieval text; more amendment_events linked to
  the right provision; table rows become citable instead of `unresolved`.
- 🧪 Tests: VM `text_normalizer::` 20/20; new `structured_importer::` tests green.
  Sole failure is the pre-existing embedded-PG DB test (`initdb: libssl.so.3`).
- 📁 Files: text_normalizer.rs, structured_importer.rs, CHANGELOG_DEV.md.
- 🔁 Next prompts: re-index corpus on VM to realise the fixes; tackle RC-T + LG-Act
  with live data (reproduce, edit sidecar, rebuild binary, validate).
- 🧠 Reflection: kept every change verbatim-safe / refuse-or-flag — no invented
  labels or cell values, consistent with the citation-grade contract.

## Evaluation notes (flywheel)

- Failure modes observed: earlier audit measured RC-F/RC-E on the pre-RC-PG DB, so
  part of their weakness was the (now-fixed) page-scale bug; re-index will re-baseline.
- Graders run and results (PASS/FAIL): unit tests PASS; live validation pending re-index.
- Prompt variant (if applicable): n/a
- Next experiment (smallest change to try): after re-index, measure unresolved-row
  % (Delegation/APT) and amendment_event count to quantify RC-E/RC-F gains.
