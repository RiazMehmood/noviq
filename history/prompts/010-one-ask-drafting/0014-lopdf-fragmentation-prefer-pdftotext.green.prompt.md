---
id: 0014
title: lopdf word-fragmentation root cause — prefer pdftotext extraction
stage: green
date: 2026-06-16
surface: agent
model: claude-opus-4-8
feature: 010-one-ask-drafting
branch: 011-postgres-pgvector-port
user: Riaz Mehmood
command: (debug — systematic-debugging skill)
labels: [extraction, pdf, quality-gate, citation-grade, root-cause]
links:
  spec: specs/010-one-ask-drafting/spec-rechunk-reindex-extension.md
  ticket:
  adr:
  pr:
files:
  - src-tauri/src/services/parsers/text_quality.rs
  - src-tauri/src/services/parsers/pdf.rs
  - CHANGELOG_DEV.md
tests:
  - cargo test --lib parsers (18/18 green, +3 new)
---

## Prompt

User pasted two live one-ask drafts (Canon-scanner / direct-contracting / e-daftar) and confirmed
the corpus IS re-indexed, located at `~/Desktop/Sindh Govt Documents/`. Both drafts still showed
garbled verbatim quotes ("o ne hundred thousand", "agr eement", "sha ll", "t he Authority") and
broken labels ("Rule 2017(4)", bare "(m3)"/"(1)") despite US2/US3 being committed. Earlier turns
established US6/FR-014 (draft now about the officer's matter) is fixed. User asked: "why are we using
lopdf? can we omit it out and use only pdftotext and ocr only?" and chose to defer the label fix.

## Response snapshot

Ran systematic-debugging. Phase 1 evidence (live DB + extractor A/B):
- Stored chunk is exploded one-token-per-line; fragments separated by NEWLINES, but
  `repair_spaced_words` only joins a single SPACE → US2 unit test (`"o ne hundred"`) passed but never
  matched real data.
- Doc extracted by lopdf (Tier 1). `pdftotext -layout` on the same PDF yields the clean whole
  provision. Line-shape A/B: lopdf 42% lines ≤3 chars / mean 16; pdftotext -layout 2% / mean 71.
- `text_quality::score` blind to fragmentation (tokenises across newlines) → lopdf scored Good →
  pdftotext never tried.
- Same root cause breaks labels (index-time structure detection on shredded text).

Answered the lopdf question: it was the dependency-free Tier 1; but OCR already needs poppler
(`pdftoppm`), so poppler is a de-facto dep whenever OCR runs. Recommended demote-not-delete.

Fix (TDD on the scorer; one coherent change):
1. `text_quality.rs` per-line fragmentation signal (>30% lines ≤3 chars over ≥20 lines → Suspect).
2. `pdf.rs` reorder: pdftotext primary → lopdf no-poppler fallback → OCR → best-of.

## Outcome

- ✅ Impact: extraction now prefers clean pdftotext over fragmented lopdf; cures garbled quotes and
  most label damage at the index layer. Requires one re-index to take effect.
- 🧪 Tests: `cargo test --lib parsers` 18/18 (3 new fragmentation tests); no regressions.
- 📁 Files: text_quality.rs (signal + 3 tests), pdf.rs (tier reorder + doc), CHANGELOG_DEV.md.
- 🔁 Next prompts: user re-indexes; re-run the same ask; then re-evaluate residual labels +
  off-topic-card/writer-gloss issue on clean text.
- 🧠 Reflection: the US2 unit test fixture (space-separated) diverged from production (newline-
  separated) — a test that proves the function but not the integration. Measuring real line-shape
  distributions (42% vs 2%) gave a defensible, keyword-free threshold.

## Evaluation notes (flywheel)

- Failure modes observed: unit test green but zero production effect (fixture ≠ real data shape);
  quality gate blind to a whole class of extraction defect (line fragmentation).
- Graders run and results (PASS/FAIL): parsers suite PASS (18/18).
- Prompt variant (if applicable): n/a
- Next experiment (smallest change to try): after re-index, query the SPPRA chunk again — confirm
  extraction_method='pdftotext' and content reads "one hundred thousand" whole.
