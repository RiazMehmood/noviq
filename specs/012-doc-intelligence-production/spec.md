# Feature 012 — Document-Intelligence Production Overhaul

**Branch:** `012-doc-intelligence-production` · **Date:** 2026-06-17
**Status:** committed for production launch (no spike/exploration phase; build to ship).

## Goal
Make the offline, citation-grade legal-RAG drafting system **production-ready**:
clean ingestion (correct provision boundaries, labels, tables; no page-chrome
masquerading as law) and a drafting layer that **cannot fabricate**. A single
incorrect citation is a severe failure; the architecture must make wrong
citations structurally impossible, not merely unlikely.

Supersedes the diagnostic findings in `docs/chunk-defects-remedies-status-2026-06-17.md`
and the senior-engineer design review (2026-06-17). Replaces the US8 geometric
table heuristic and the downstream boilerplate-flag approach.

## Two non-negotiable production guardrails (apply to every story)
- **G1 — Verbatim-substring invariant.** Every emitted token that is a number,
  quotation, rule reference, monetary value, or table cell MUST be a verbatim
  substring of the source document's digital text. Reconstruction may reorder /
  regroup verbatim spans; it may NEVER synthesize. Enforced at the verify gate.
- **G2 — Golden regression gate.** A frozen set of real-corpus fixtures (known
  provision → expected resolved label + verbatim content; known table → expected
  rows; known chrome → expected exclusion) is the release gate. Every re-index is
  diffed against it. This is the product's safety contract, not exploratory testing.

## Committed architecture
```
PDF
 ├─ has digital text layer? ──────────────────────────────┐ yes
 │                                                         │
 ▼ no (scanned page only)                                  ▼
 rasterize → OCR / PP-Structure (PaddleOCR)         pdftotext -bbox (words + coords)
 [reserved for scans; corpus has none today]        [single trusted geometry source — NO lopdf]
                                                           │
                                                           ▼
 [US1] CHROME-STRIP  ── cross-page positional repetition, margin geometry,
   │                    form-field structure → provision body | chrome (kept, separate)
   ▼
 [US5] TABLE ENGINE ── ruling-lines → projection-profiles → cross-page stitch
   │                    → row-units (verbatim cells, G1-verified)
   ▼
 [US3] HEADING DETECTION ── regex + numbering-sequence prior + confidence; boundary-stop
   ▼
 TREE BUILD ── attach only provision-classified blocks; label-content split action
   │           → chunks: resolved unit_label + raw marker + confidence + provenance  [US2]
   ▼
 EMBED + INDEX (pgvector + tsvector)        [Feature 013+: KG projection — deferred]
   ▼
 RETRIEVAL (hybrid) ─► [US4] DETERMINISTIC DRAFTING (slots+templates) ─► verify gate (G1)
```
Cross-cutting: **refuse-or-flag over confidently-wrong** — uncertainty is a
first-class chunk/draft state; the system flags rather than emits confident garbage.

## User stories (production scope)

### US1 — Upstream chrome-strip (fixes D3) — Priority P0
Page chrome (mastheads, running headers/footers, page markers, contact pages,
form fields) must be detected and **isolated as a non-provision class before tree
construction**, so it never inherits a provision label.
- Generic signals only (NO per-document keyword lists), in power order:
  1. **Cross-page positional repetition** — text recurring at the same vertical
     band across ≥K pages = chrome (mastheads, page numbers).
  2. **Margin geometry** — short text in top/bottom page margins.
  3. **Form-field structure** — fill-blanks (`____`, dotted leaders), isolated
     date lines, all-caps masthead blocks, signature/stamp lines.
- Chrome is **kept** (separate class, auditable), never deleted, never attached
  under a provision node.
- **AC:** the `Rule 345(3)(b)` notification masthead and "P a g e"/"Contact Us"
  chunks are no longer provision chunks; golden provisions unaffected.

### US2 — Resolved labels + provenance (fixes D5) — Priority P0
- `unit_label` = the **resolved citation from the hierarchy** ("Rule 27(i)"),
  computed at persist time; persist the **raw marker** + `unit_path` alongside.
- Eliminate the silent synthetic-id fallback (`m2`/`m66`): a parse that cannot
  assign a marker must surface (flag), not emit a meaningless id.
- Add provenance per chunk: source page, char-span, extraction + reconstruction
  method, boundary confidence.
- **AC:** no chunk cites a bare `(i)`/`m66`; ambiguous hierarchy → "unresolved"
  flag, never a confident-but-wrong resolved label.

### US3 — Robust heading detection (fixes D4) — Priority P0
- **Numbering-sequence prior**: the expected next ordinal is a strong signal —
  recovers missed headings (degraded formatting) and rejects out-of-sequence
  restarts (form fields).
- **Boundary-stop**: never absorb a block whose leading token is a plausible
  next-in-sequence provision number; start a new node instead.
- **Label-content invariant as a split action** (promote the existing warning):
  when content begins with a provision number disagreeing with its label, split.
- Heading confidence is recorded; low confidence → flag, not silent mislabel.
- **AC:** golden set of headings (incl. degraded/`N. Title - body` shapes)
  detected; zero label≠content on the golden provisions.

### US4 — Deterministic drafting (fixes D7) — Priority P0 (parallel; no re-index)
- Compose summaries/notings from **fixed slot+template structure**; the LLM
  authors **no paragraph prose**. Slots filled deterministically (verbatim
  subject, verbatim provision quote+label, verbatim figures, extracted
  stakeholders).
- Verification is a **decidable whitelist check** (templates + allowed connectors),
  not an open-ended fabrication judgment.
- Optional later concession ONLY if prose reads too stiff: **sentence-level
  connectors** from a fixed whitelist; never free prose.
- **AC:** zero fabricated glosses, zero advocacy, zero scaffolding leakage, prayer
  is one sentence; every number/quote/ref passes G1.

### US5 — Rust-native table engine (fixes D1) — Priority P1
- **Single geometry source = `pdftotext -bbox`** (poppler), the same trusted
  extractor as the text. **NO lopdf** — its low-quality `extract_text` heuristic
  is the reason it was demoted, and we will not reintroduce it here.
- **Projection-profile column detection (PRIMARY, sufficient):** project the
  `-bbox` word x-extents across rows; consistent vertical whitespace bands =
  column boundaries. Handles the borderless / vertically-stacked / multi-level
  cases that broke the v1 pairwise-gap heuristic.
- **Cross-page row stitching**: carry a column-signature across page breaks; drop
  repeated headers; merge continuation rows.
- Linearize each row → one self-describing unit, headers folded in, **verbatim
  cells (G1)** lifted from the digital text — never OCR, never synthesized.
- **Ruling-line recovery is DEFERRED** (only if projection profiles prove
  insufficient on bordered tables). If ever pursued, via a high-fidelity reader
  (pdfium) — NOT lopdf.
- **PP-Structure/PaddleOCR reserved for scanned pages only** (no digital text
  layer). The current corpus has none → not used at launch.
- **AC:** the Delegation-of-Financial-Powers rows extract as discrete, in-order,
  correctly-labeled authority statements; every cell is a verbatim substring;
  non-table prose unchanged.

## Non-goals (this feature)
- Knowledge graph + error-detection (Feature 013+, deferred until ingestion stable).
- Full-page OCR of digital documents (violates G1; OCR only for true scans).
- Any per-document keyword/heuristic lists.

## Sequencing & re-index plan
- **Wave A (one re-index): US1 → US2 → US3** — ingestion correctness. These change
  chunk boundaries ⇒ **full re-embed** required (budgeted).
- **Wave B (parallel, no re-index): US4** — deterministic drafting.
- **Wave C (one re-index): US5** — table engine (gated by G1 + golden table fixtures).
- KG deferred.

## Risks
- Re-segmentation invalidates embeddings → full re-embed + cached drafts invalid.
- Table-engine scope creep → bounded by G1 + golden fixtures; ruling-lines first.
- Chrome over-strip → conservative repetition threshold + kept-not-deleted + golden gate.
- NTFS build/re-index cost (~30 min) → batch each wave into a single re-index.
