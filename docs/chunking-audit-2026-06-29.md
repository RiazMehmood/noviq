# Forensic Chunking Audit — Live Corpus (2026-06-29)

Audit of the **structured-parser ingestion pipeline** against the criteria in
`chunking_audit.txt`. Evidence drawn from the **live Postgres on the GPU VM**
(`echo` DB, 7,714 chunks across 11 indexed PDFs), cross-checked **page-by-page**
against the original PDFs via `pdftotext -layout`.

> **Premise correction**: "the hierarchical pipeline only works for the
> constitution" is **false**. All 11 docs are `structure_status='structured'`
> through one shared pipeline (`structured_importer` → `structure_parser`). The
> constitution is indexed as a plain `.pdf` via the *same* path; its curated-IR
> fast path (`.json` only) and the shallow size-based fallback
> (`chunker::chunk_tree`) are **not used** by any live document. The defects are
> in **per-document heading detection, chunk sizing, and page-number stamping**,
> not pipeline routing.

## Method & limitations
- Ground truth: `pdftotext -layout` per page (the poppler extractor the pipeline
  already uses for geometry).
- Coverage probe: an independent *liberal* heading detector tolerating `(i)`,
  `[`, and OCR-noise openers after `N.` — the cases the app's `^\d+\.\s+[A-Z]`
  pattern rejects. Every reported defect was verified against chunk content.
- Inline-absorbed headings (reflowed mid-paragraph) are a **lower bound** in the
  coverage probe.
- **Not yet measured corpus-wide** (not fabricated here): reading order /
  two-column flow, OCR char accuracy, table **cell-level** fidelity, hyphenation
  repair, footnote→clause attachment. See §Open items.

## Headline
- **Every document has wrong page numbers** (systematic, worsening with depth).
- **5 documents have missing/absorbed provisions**; 2 of them severely.
- **3 documents are structurally clean**: SPP Act 2009, SPSC 2023, APT Rules 1974.
- **Production readiness: NO.**

## Per-document results

| # | Document | Pages | Missing provisions (in range) | Other | Verdict |
|---|---|---:|---|---|---|
| 01 | Sindh Rules of Business 1986 | 67 | 27 absorbed (3–8,13,14,17,19,23–29,48–58) + dup labels (30×6,31×5,32×4) | 107 chunks <60c | **CRITICAL** |
| 02 | SPPRA Rules 2010 | 52 | 18, 32 (amendment-bracket rules); 56/59/60 = `[XXX]` repealed | — | HIGH |
| 03 | Sindh Public Procurement Act 2009 | 20 | none | — | GOOD |
| 04 | Sindh Local Government Act 2013 | 8 | 11,15–19,21–23,30 (amendment Act; §21 verified inside §20) | 62 footnote/62 unresolved | MEDIUM |
| 05 | Compendium Rules under SLGA | 239 | none (20 parts, legitimate restarts) | dup labels across parts; 551 <60c | MEDIUM |
| 06 | Manual for Development Projects 2019 | 248 | none real (range inflated by false-positive `156`) | **chunk 35,604 chars** (+9 >6k) | HIGH |
| 09 | Sindh Delegation of Financial Powers 2019 | 91 | none (10 parts, restarts) | **57% chunks unresolved** | HIGH |
| 10 | SPSC Regulations 2023 | 83 | 70 only (1 gap) | — | GOOD |
| 11 | Sindh Civil Servants APT Rules 1974 | 11 | 12 only (1 gap) | 58% unresolved | MEDIUM-HIGH |
| — | Govt of Sindh Manual of Secretariat Instr. 2010 | 64 | **~83 real provisions (≈84–180) absorbed** (122/123 verified real) | 5 huge chunks | **CRITICAL** |
| — | constitution.pdf | 222 | **Articles 55–76 (22 consecutive) absent** | 502 unresolved; chunk 42,866 chars | **CRITICAL** |

## Critical & High findings

### C-PAGE — Page numbers are systematically wrong (ALL documents)
Triangulation on SPPRA (printed page == PDF sheet, clean 1:1):

| Rule | stored `page_number` | real PDF sheet / printed page |
|---|---:|---:|
| 17 | 5 | 13 |
| 27 | 7 | 18 |
| 40 | 12 | 29 |
| 50 | 14 | 35 |

Stored page is **compressed and the error grows with depth** (SPSC Reg 100:
stored 26 vs sheet 59). **Impact**: every citation's page reference is wrong by
a widening margin — a lawyer is sent ~20 pages early. **Root cause (RC-PG)**:
`page_by_path`/`body_page_offsets` are computed on chrome/front/back-matter-
**stripped** text, so the page counter advances far slower than real pages.
Page-class chunks need to carry the *original* sheet (and ideally the printed
page) — see §6 of `chunking_audit.txt` (pdf_page / printed_page / logical_page).

### C1 — Constitution Articles 55–76 missing
258/280 distinct captured; 55–76 (Parliament: voting/quorum, Senate, Bills,
Money Bills, President's assent) absent — catchlines survive only in the TOC
chunk. Root cause not yet pinned (contiguous block ⇒ page-region
misclassification or filter drop, not RC-A). **Investigate first.**

### C2 — Sindh Rules of Business: ~16+ rules absorbed + duplicate labels
Rule 3's body verified inside the `Section 2` clause. Duplicates `Section 30`×6
etc. from schedule serial renumbering.

### C3 — Govt Secretariat Manual: ~83 provisions absorbed
Provisions 122/123 are real but uncaptured; deep (~32-space) indentation of the
number defeats the line-start pattern.

### H1 — Amendment-inserted provisions missed (SPPRA 18/32, others)
`[18. Response Time … ]14` starts with `[`, so `^\d+\.` misses it → absorbed.
Directly hits *legal amendments* (audit §6). Same family as RC-A.

### H2 — Oversized chunks
35,604-char (Manual) and 42,866-char (constitution) chunks escape
`split_oversized_leaves` (threshold 1500c) — likely internal-node/table bodies.

### H3 — Uncitable table-row chunks
Delegation (57%) and APT (58%) majority `label_status=unresolved`.

## Root causes

| ID | Root cause | Docs | Fix surface |
|---|---|---|---|
| **RC-PG** | Page map computed on stripped text → compressed page numbers | **ALL** | page-offset stamping in `structured_importer`/`chrome` |
| RC-A | Heading regex needs `[A-Z]` after `N.`; fails on `(i)`/`(1)`/OCR-noise | 01, 04 | `doc_profiles` patterns + `regex_pass.rs` |
| RC-A2 | Same regex fails on `[` amendment-bracket opener | 02, others | as RC-A |
| RC-A3 | Allowed leading whitespace too small for deep-indented numbers | Govt Manual | as RC-A |
| RC-B | ~~Contiguous 22-article region dropped~~ **RESOLVED 2026-06-29** — not a HEAD defect; the audit's live DB predates the RC-A fixes. All Art 55–76 captured on current code (verified via real pdfplumber sidecar + `import`). VM re-index clears it. Locked by `rcb_parliament_articles_with_bracket_openers_all_survive`. | constitution | fixed by RC-A |
| RC-C | **FIXED 2026-06-29** — `SCHEDULE-<roman/num>` now a structural boundary; schedule serials dropped when they collide with a main-body rule or recur across schedules. Real RoB: 39 dup provision labels → 0. | 01 | `scan_structural_boundaries` + `suppress_schedule_table_serials` |
| RC-D | Oversized internal/table bodies escape splitter | 06, constitution | `split_oversized_leaves` |
| RC-E | Table-row provisions unresolvable to a named parent | 09, 11 | delegation-table citation resolution |

## Fix order (broadest impact first)
1. **RC-PG** — page-number stamping (affects every citation in every doc).
2. **RC-A / A2 / A3** — heading-detector robustness (closes 01, 02, 04, Govt Manual gaps). One coherent change: allow `[`, `(`, sub-marker, OCR-punct and deeper indentation after `N.`, guarded against `1.2.3` false positives; re-verify no regression on 03/05/10.
3. **RC-B** — root-cause + restore constitution 55–76.
4. **RC-C** — RoB duplicate suppression.
5. **RC-D** — oversized-chunk splitting.
6. **RC-E** — table-row citability.

After fixes: re-index affected docs on the VM (GPU embed) and re-run this audit.

## Content-correctness dimensions (now measured, against the pdfplumber sidecar)

Measured with the **actual ingestion extractor** — the frozen
`ingestion-sidecar` binary (pdfplumber text+tables; PaddleOCR for scans) — not
pdftotext.

| Dim | Result | Severity |
|---|---|---|
| **1. Reading order / two-column** | PASS — 0 two-column-interleave signatures; single-column corpus; tables extracted structurally, not interleaved | OK |
| **2. OCR / char accuracy** | PASS — 0 mojibake/replacement/ligature artifacts; all 11 docs are digital (embedded text). **PaddleOCR not exercised** by this corpus — latent until a scanned doc is added | OK |
| **3. Table cell fidelity** | MIXED — **0% ragged rows** (column structure clean, beats pdftotext); but **40–54% empty cells** (merged-cell flattening: Delegation 39.9%, Manual 54.2%, Compendium 42.1%) and **misattributed headers on page-spanning tables** (data cell captured as header) | MEDIUM-HIGH |
| **4. Hyphenation** | INCOMPLETE — ~150 residual line-break hyphens (constitution 42, Compendium 43, RoB 16) + broken words (`Govern- ment`: Delegation 14) | MEDIUM |
| **5. Footnote → clause attachment** | WEAK — only **23 `amendment_events`** linked from 600+ footnote chunks (constitution alone 502); several mis-anchor to `Preamble`; amendment history largely disconnected | HIGH |

Added root causes:
- **RC-T** (Dim 3): page-spanning table header carried from page 1 not re-applied
  to continuation pages; merged source cells flatten to blanks. Surface:
  `ingestion_sidecar.py` table extraction + `table::reflow_with_sidecar`.
- **RC-H** (Dim 4): line-break hyphen de-hyphenation incomplete. Surface:
  `text_normalizer`.
- **RC-F** (Dim 5): `anchor_citation_for_page` (single-article-per-page) rarely
  fires and can mis-anchor; most amendments never link. Surface:
  `structured_importer::persist_amendment_events` + footnote anchoring.

## Reproduction
- Scripts on VM: `/tmp/audit.py` (page coverage), `/tmp/gaps.py` (missing-provision
  map), `/tmp/pageaudit.py` + `/tmp/triangulate.py` (page-number).
- DB: `psql -h 127.0.0.1 -p <ephemeral> -U postgres -d echo` (pw in
  `/opt/noviq/data/pg/.pgpass`; port `ss -tlnp | grep postgres`).
- PDFs: `/opt/noviq/data/uploads/admin/`.
