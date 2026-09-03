# Feature 004 — OCR Integration for the PDF Indexer

**Status:** Draft (brainstorming output, pre-plan)
**Date:** 2026-05-24
**Author:** Architect + Claude (brainstorming session)
**Predecessors:** 003-constitution-hierarchical-graph (closed)
**Acceptance corpus:** `/home/riaz/Desktop/Sindh Govt Documents/` (6 legal PDFs)

---

## 1. Problem

EchoAI's PDF pipeline (`src-tauri/src/services/parsers/pdf.rs`) uses a two-tier extractor:

1. `lopdf` (fast, in-process)
2. `pdftotext` (poppler-utils subprocess) — used when lopdf yields < 10 chars/page

A PDF is only treated as "scanned" when *both* tiers produce near-zero text. In practice this misses a second failure mode: **font-corrupted PDFs where `pdftotext` returns plenty of text but the text is glyph-mangled** (curly braces mid-word, missing letters, `�` replacement chars, broken word boundaries). Such text poisons FTS5 tokenization and dense embeddings; downstream retrieval/reranker quality cannot be measured on it.

Real-world impact on this project: a halted reranker eval on Gutenberg-mirror literature (2026-05-19) and — more importantly — degraded retrieval on the Sindh-Government legal corpus, which is the actual product domain. Legal text accuracy must be high enough that a chunk citing a section number still tokenises as that section number, not as a mangled glyph string.

## 2. Goals

- G1. Detect both failure modes — image-only PDFs *and* font-corrupted PDFs — with a single quality verdict.
- G2. Re-extract suspect documents via Tesseract OCR, producing text clean enough for FTS5 and embeddings.
- G3. Surface the extraction method per source so downstream stages and the UI can reason about provenance.
- G4. Auto-heal: already-indexed sources that would now flunk the new quality check get re-extracted on next app startup, without user intervention.
- G5. Degrade gracefully when `tesseract` / `pdftoppm` are not installed.

Non-goals (deferred): multi-language / RTL scripts, GPU OCR, per-page mixing of OCR and native text, bundling tesseract with the app, UI progress bar for OCR (logs only), re-OCR'ing PDFs that already extract as `Good`.

## 3. User-facing behaviour (what changes)

- Indexing a PDF that previously produced garbage now produces clean text — observable as cleaner chat answers and FTS5 hits that match the user's wording.
- On first run after the upgrade, sources whose stored `quality_score` falls below threshold are re-indexed automatically. The existing indexer status surface reports this work; no new UI is added.
- If `tesseract` is missing on the host, indexing still proceeds with the prior text and logs one warning per affected source (`extraction_method = 'ocr_skipped'`). No file is dropped.
- DPI is configurable (`[ocr] dpi = 300`) for users with unusual document types.

## 4. Architecture

### 4.1 Components and boundaries

| Unit | Responsibility | Public interface |
|---|---|---|
| `services/parsers/pdf.rs` | Orchestrate the three-tier extraction | `extract(path) -> Result<ParsedDocument, ParseError>` *(signature unchanged)* |
| `services/parsers/text_quality.rs` *(new)* | Pure scoring function over a `&str` | `score(text, total_pages) -> Quality { score: f32, verdict: Verdict }` where `Verdict ∈ { Good, Suspect, Empty }` |
| `services/parsers/ocr.rs` *(new)* | Rasterise PDF → PNG via `pdftoppm`, OCR each page via `tesseract`, stitch into a `ParsedDocument` with `\x0c` page separators | `ocr_pdf(path, dpi, timeout_per_page) -> Result<ParsedDocument, ParseError>` |
| `services/indexer.rs` (startup hook) | Find sources with `quality_score < THRESHOLD AND extraction_method != 'ocr'` and enqueue them for re-extract | `enqueue_quality_reextract(db) -> usize` (count enqueued) |
| DB migration `20260524_add_extraction_quality.sql` | `ALTER TABLE sources ADD COLUMN extraction_method TEXT`, `ADD COLUMN quality_score REAL` | applied at startup |
| Config | New section `[ocr]` — keys: `enabled` (bool, default true), `dpi` (u32, default 300), `timeout_per_page_secs` (u32, default 60) | read at parser init |

### 4.2 Why these boundaries

- `text_quality` is a pure function with no I/O — trivially unit-testable with fixture strings and re-usable for the startup re-scan without re-parsing PDFs.
- `ocr.rs` owns *all* subprocess and temp-file logic; nothing else in the codebase learns about `pdftoppm` or `tesseract`. If we swap engines later, only this file changes.
- `pdf.rs` stays a thin orchestrator (~80 lines after the change) — readable end-to-end at one screen.

### 4.3 Data flow

```
extract(path)
 └─► lopdf  ─► text_quality::score
                 ├─ Good     ─► return ParsedDocument(method=lopdf)
                 └─ Suspect  ─► pdftotext ─► text_quality::score
                                              ├─ Good    ─► return (method=pdftotext)
                                              └─ Suspect ─► ocr::ocr_pdf
                                                              ├─ Ok    ─► return (method=ocr)
                                                              └─ Err   ─► return best-so-far
                                                                          (method=ocr_skipped) + warn
```

Each terminal arrow also writes `extraction_method` and `quality_score` to the `sources` row.

## 5. The quality score (heart of the design)

`text_quality::score(text, total_pages)` produces `verdict = Suspect` if **any** signal trips. Numbers below are starting points; they will be tuned against the Sindh-corpus eval (§7.3) before the gate is locked.

| Signal | Initial threshold | What it catches |
|---|---|---|
| `chars_per_page < 10` | (existing rule) | True image-only / scanned PDFs |
| `non_ascii_punct_ratio > 0.05` | Over 5% of chars are `{`, `}`, `�`, NUL, or other control chars | Glyph-mangle (the original failure mode) |
| `dict_hit_ratio < 0.60` | Fewer than 60% of alphabetic tokens of length ≥ 3 appear in the bundled English wordlist | Subtle letter-drop (e.g. `Frailty` → `Failty`) that other signals miss |
| `mean_token_len < 2.5` | Typical English averages ≈ 4.7 | Run-on garbage that fragments into 1–2-char tokens |

Dictionary: ~50 k common English words bundled at build time as `text_quality/en_words.txt` (~500 KB). No network. Pure-static lookup via `phf` or a `HashSet<&'static str>` built once with `OnceLock`.

The numeric `score` (a single `f32` summarising the four signals — exact aggregation TBD during implementation, but ordering must preserve the verdict) is persisted on the source row so the startup re-scan in §4.1 can run without re-parsing the PDF.

## 6. OCR sub-flow (`ocr_pdf`)

1. Create a temp directory under the **data partition** (`<DATA_DIR>/tmp/ocr-<uuid>/`) — **never `/tmp` or `/`** (per the user's `feedback_no_root_disk` rule).
2. `pdftoppm -png -r {dpi} <path> page` → `page-001.png`, `page-002.png`, …
3. Sequentially per page: `tesseract page-NNN.png - -l eng --psm 1` → stdout. Sequential, not parallel, to bound RAM on long PDFs; indexing is a batch path so wall-clock is acceptable.
4. Concatenate page texts with form-feed (`\x0c`) separators — the existing `pdftotext_page_offsets()` then computes correct page offsets with zero changes.
5. `rm -rf` the temp dir on success and on every error path.

### 6.1 Failure handling

| Failure | Behaviour |
|---|---|
| `tesseract` not on PATH | Log warn, return best prior text, set `extraction_method = 'ocr_skipped'`. Indexing proceeds. |
| `pdftoppm` not on PATH | Same as above. Install hint mentions a single package (`poppler-utils` + `tesseract-ocr` on Debian/Ubuntu). |
| OCR subprocess crash on one page | Skip the page (preserve its slot in `page_offsets` so downstream chunk alignment stays correct), continue. |
| OCR subprocess timeout (`timeout_per_page_secs`) | Kill, skip page, warn. |
| Temp dir disk-full | Abort OCR for this document, fall back to best prior text, warn. |
| File > 2 GB | Still hard-fail at the existing limit (unchanged from `pdf.rs`). |

## 7. Testing strategy

### 7.1 Unit tests — `text_quality`

Fixtures live in `src-tauri/tests/fixtures/text_quality/`:

- `clean_legal.txt` — a clean paragraph copied from one of the six Sindh PDFs (post-OCR) → must score `Good`.
- `garbage_legal.txt` — the same paragraph with curly-brace glyph noise injected → must score `Suspect`.
- `empty.txt` — single whitespace → must score `Empty`.
- `mixed.txt` — 60 % clean, 40 % noise → must score `Suspect` and exercise the dict-hit signal independently of the punctuation signal.

### 7.2 Unit tests — `ocr_pdf`

A 2-page synthetic scanned PDF generated at test-build time (PNG-of-text embedded via `printpdf`) lets us assert:

- `ocr_pdf` returns text containing the embedded sentence verbatim.
- `page_offsets` length matches page count.
- Cleanup: the temp directory no longer exists after the call returns.

### 7.3 Integration / acceptance gate — Sindh legal corpus

Corpus: `/home/riaz/Desktop/Sindh Govt Documents/` (6 files, listed in §1.)

A new test `cargo test --test ocr_integration_acceptance` (no LLM dependency, in the style of the Feature 003 acceptance harness) runs `extract()` over each file and asserts:

| File | Required outcome |
|---|---|
| `constitution.pdf` | Extracts cleanly without OCR (`extraction_method ∈ {lopdf, pdftotext}`), `verdict = Good`. Born-digital control: confirms the heuristic does not over-trigger. |
| `efficiency and disciplane rules.pdf` | `verdict = Good` after extraction. |
| `appointment promotion and transfer rules.pdf` | `verdict = Good` after extraction. |
| `REMOVAL FROM SERVICE SPECIAL POWERS SINDH.pdf` | `verdict = Good` after extraction. |
| `Govt. of Sindh Manual of Secretariat instruction 2010.pdf` | `verdict = Good` after extraction. |
| `sindh government rules of business 1986.pdf` | `verdict = Good` after extraction. |

Acceptance gate: **every file in the corpus ends with `verdict = Good`**, regardless of which tier produced the text. Files that trip OCR additionally have `extraction_method = 'ocr'` recorded. This is the gate that closes Feature 004.

Section-number round-trip (one hand-picked smoke check): pick a known section heading from one of the rules PDFs (e.g. "Rule 3" or an Article number from the constitution), index the corpus into a scratch DB, and assert the FTS5 query for that section heading returns a chunk that contains the heading verbatim. This is the user-visible quality check the score is ultimately defending.

### 7.4 What we explicitly do NOT test in this feature

- Literature corpus (`data/reranker/litset_v1/`). The earlier halted reranker eval may still benefit from OCR as a side effect, but it is not the gate for this feature.
- Multi-language behaviour.

## 8. Migration & rollout

1. Ship the schema migration `20260524_add_extraction_quality.sql` — adds two nullable columns, backfills nothing. Cost ≈ instant on the 17 GB DB (DDL only, no row rewrite).
2. On startup, `enqueue_quality_reextract` finds sources with `quality_score IS NULL OR (quality_score < THRESHOLD AND extraction_method != 'ocr')` and queues them on the existing indexer pipeline.
3. First boot after upgrade: previously bad PDFs (notably any glyph-mangled ones already in `echo.db`) are re-extracted in the background. Subsequent boots: no-op (all rows now have a `quality_score`).

## 9. Open implementation questions (resolved during plan.md, not now)

- Exact aggregation formula for the single `quality_score` `f32` from the four signals.
- Whether the bundled `en_words.txt` ships as a Rust `phf` map at compile time or is loaded lazily at runtime from a resource file.
- Whether the OCR sub-flow runs *inside* the existing indexer worker (same task budget) or is treated as a distinct queue priority.

## 10. Risks

| Risk | Mitigation |
|---|---|
| Heuristic over-triggers on clean legal text (acronyms, section labels, citation noise) — wastes hours of OCR | §7.3 gate explicitly tests this with `constitution.pdf` as a born-digital control; thresholds tuned before lock. |
| Tesseract output is *worse* than the original on borderline PDFs | The score is recomputed on OCR output; if it does not improve the verdict to `Good`, we keep the best of the two texts and record `extraction_method = 'ocr'` only when it actually helped. |
| Startup re-extract storm after upgrade on a large library | Re-extract runs through the existing indexer queue, which already throttles and respects task budget. No special handling required. |
| `/home/riaz/Desktop/Sindh Govt Documents/` is not checked into the repo — CI cannot run §7.3 | Acceptance test reads a `OCR_FIXTURE_DIR` env var; CI sets it to a tiny synthetic corpus, the architect sets it to the real Sindh path locally. Unit tests (§7.1, §7.2) run everywhere unconditionally. |

---

## Acceptance checklist (to be ticked at end of implementation)

- [x] `text_quality::score` unit tests green for all four fixtures.
- [x] `ocr_pdf` unit test green; temp dir cleaned up.
- [x] `ocr_integration_acceptance` test green: every file in the Sindh corpus ends with `verdict = Good`.
- [ ] Section-heading FTS5 round-trip returns a chunk containing the heading verbatim. *(deferred — needs scratch DB indexing run; tracked as v2 manual smoke.)*
- [ ] Missing-tesseract code path tested manually on a host without tesseract (or via PATH override in test). *(deferred — current acceptance host has tesseract; needs PATH-override test or alt-host run.)*
- [x] Schema migration applied; startup re-extract function shipped + tested (`indexer::reextract_tests`).
- [x] `CHANGELOG_DEV.md` entry appended.
- [x] PHR created for this feature work.
