# Phase 9 (DEFERRED) — Extraction Sidecars: pdfplumber tables + PaddleOCR scans

> **Status:** PLAN ONLY — no code this iteration. Implement AFTER the current front-matter
> re-index is verified (iteration 7) and we've iterated on those results.
> **Scope:** an *extra, optional phase* of Feature 013. Both upgrades are **extraction-stage**,
> **deterministic (no LLM)** — they keep FR-013. They change *how text/tables are pulled from a
> PDF*, not the structure/label/graph logic that Phases 2–8 fixed.

## 0. Why these two, why now
- **Tables (pdfplumber):** the spike (`~/Desktop/table_ingestion_test/spike_results/`) proved that on
  digital ruled/grid tables, pdfplumber returns correct **cells** where `pdftotext -layout` returns
  unstructured wrapped text the projection engine must reverse-engineer. pdfplumber == PyMuPDF in
  quality but is **MIT** (PyMuPDF is AGPL) → pdfplumber chosen.
- **Scans (PaddleOCR):** the earlier spike proved PaddleOCR fixes the numeric-garble / false-citation
  root cause on scanned docs (read "Nov. 02, 2013" correctly where tesseract invented "2613"). It is
  the scan-stage fix; scans are the last document class in the staged plan.

Both share one piece of infrastructure (a Python sidecar), so they are planned together even if
shipped separately.

---

## 1. Shared infrastructure — the ingestion sidecar

### 1.1 Process model
A single **Python sidecar** ("`ingestion_sidecar`") exposing two capabilities: `extract_tables` and
`ocr_pages`. The Rust pipeline talks to it; the LLM/chat path never does.

- **v1 (simplest):** per-request **CLI** invocation — Rust spawns the sidecar with a JSON request on
  argv/stdin, reads a JSON response on stdout, process exits. Mirrors the existing PaddleOCR-spike and
  STT-sidecar pattern.
- **v2 (perf):** **long-running** sidecar (spawned once per indexing session, fed newline-delimited
  JSON requests over stdio or a Unix socket). Avoids paying Python import + model-load cost per file.
  PaddleOCR model load is the dominant cost (~seconds), so v2 matters for OCR throughput.

Decision: ship **v1 for tables** (pdfplumber import is cheap), **v2 for OCR** (Paddle model load is
expensive) — or go straight to v2 for both. (Open question §5.)

### 1.2 Packaging / distribution (the hard part)
Python is not currently bundled. Options, in order of preference:
1. **PyInstaller / `uv`-frozen single-file binary** of the sidecar (no Python on the user machine).
   Largest artifact (~150–400 MB with Paddle), but self-contained and matches the "ship a binary"
   model of the STT sidecar. **Recommended.**
2. **Bundled venv** alongside the app (needs a Python interpreter shipped/located).
3. **First-run `pip install`** into an app-managed venv (needs network; fragile). Dev-only.

Build-weight note (ties to the crate-slimming goal): the sidecar is **indexing-time only** and
**optional** — it is NOT linked into the Rust app and NOT loaded at chat runtime. Gate it so a build
without it still works (tables fall back to the projection engine; OCR falls back to tesseract).

### 1.3 Lifecycle, discovery, failure handling
- **Discovery:** locate the sidecar binary next to the app (like `discover_ir_builder_gguf`); if
  absent, the capability is simply unavailable and the pipeline uses the existing fallback.
- **Cancellation:** reuse the F006 cancel-token registry; kill the sidecar process on cancel.
- **Timeouts:** per-page/per-file timeout; on timeout or non-zero exit, **fall back** (tables →
  projection engine; OCR → tesseract) and log — never fail the index.
- **Determinism:** pin library versions in the frozen binary so output is reproducible (G2 golden gate).

### 1.4 Licensing
- pdfplumber **MIT** ✓. PaddleOCR **Apache-2.0** ✓ (PaddlePaddle Apache-2.0). Both clean for a
  distributed product. (Avoid PyMuPDF — AGPL.) Record the bundled-wheel licenses in the build.

---

## 2. Component A — pdfplumber table sidecar (digital PDFs)

### 2.1 Goal
Replace the Rust projection-profile engine **for digital PDFs with real tables**: emit correct,
verbatim table rows (cells joined deterministically) instead of column-interleaved guesses.

### 2.2 Seam (where it plugs in)
`services/parsers/pdf.rs` — the extraction layer that today runs `pdftotext -bbox` + the projection
engine and fills `ParsedDocument.reconstructed_rows`. The sidecar **produces `reconstructed_rows`**
for digital PDFs; everything downstream is unchanged:
`reconstructed_rows` → `structured_importer::tag_table_row_chunks` → `block_class='table_row'` +
`reconstruction_method='pdfplumber'`.

### 2.3 Routing (when to call)
- **Digital only:** call when the page has an extractable text layer (the existing
  digital-vs-scan check). Scanned pages skip tables here (OCR path may add tables later).
- **Table pages only:** the sidecar runs `page.find_tables()`/`extract_tables()`; pages with no table
  cost ~nothing. No need to pre-detect in Rust.

### 2.4 Data contract
- **Request:** `{ "op": "extract_tables", "pdf_path": "...", "pages": [1,2,…] | "all" }`
- **Response:** per page → list of tables → list of rows → list of cell strings, plus each table's
  bbox and page. Example:
  `{ "pages": { "8": [ { "bbox":[x0,y0,x1,y1], "rows": [ ["Sr.#","Nature","To whom","Extent"], ["1.","Creation of Post","(i) Administrative …","Full Powers"] ] } ] } }`

### 2.5 Chunk mapping + invariants
- Each table **row** → one `table_row` chunk (as today), content = the cells joined with a stable
  delimiter (e.g. `" — "` or the existing schema-anchored format), label = the enclosing provision.
- **Verbatim (G1):** a reconstructed row is NOT a raw substring of the source — this is the already-
  accepted `reconstruction_method` exception. Cell *text* must be verbatim (pdfplumber returns the
  PDF's own text); only the cell *joining* is synthetic. Keep a per-cell verbatim check in the
  acceptance harness.
- Page number from the table bbox → the chunk `page_number`.

### 2.6 Gating
`ECHO_TABLE_SIDECAR` — **ON by default**; set `=0` to opt out. When off or the sidecar is missing →
current projection engine. No behaviour change for non-table docs. (Shipped default-on per user
request 2026-06-20; absent-binary/failure safely falls back, so default-on adds no risk.)

### 2.7 Acceptance (cells-correct, not vibes)
- On the 4 spike docs: ≥ N% rows match the source cell-for-cell (the spike already gives the baseline:
  Delegation 76 tables / 927 rows). Add a frozen golden for 2–3 table pages.
- No regression on prose docs (they have 0 tables → unchanged).
- SPPRA has **no** machine-detectable tables → confirm the sidecar emits nothing there (its problem is
  amendment brackets, out of scope for tables).

### 2.8 Risks / open
- Borderless tables: pdfplumber "stream"-style is weaker; keep the projection engine as fallback for
  those, or accept lower recall. Measure per-doc.
- Cell delimiter choice affects retrieval/citation — pick one and lock it (golden gate).

---

## 3. Component B — PaddleOCR scan sidecar (scanned PDFs)

### 3.1 Goal
Replace **tesseract** with **PaddleOCR** for scanned pages, to stop the numeric-garble that mints
false citations (the "Section 20413" / "Nov 02 2013 → 2613" class of error).

### 3.2 Seam
`services/parsers/pdf.rs` extraction tier ladder (lopdf → pdftotext → **OCR**). Today the OCR tier is
tesseract (`parsers/ocr.rs`), chosen when `text_quality` judges the page scanned/low-text. The sidecar
**replaces the tesseract call** for those pages; `extraction_method` becomes `"paddleocr"`.

### 3.3 Routing (scan detection — unchanged)
Reuse the existing scanned-page detection (`text_quality` verdict / low extractable-text ratio). Only
pages routed to OCR today go to PaddleOCR. Digital pages never hit it.

### 3.4 Data contract
- **Request:** `{ "op": "ocr_pages", "pdf_path": "...", "pages": [n,…], "dpi": 300, "lang": "en" }`
  (Rust rasterizes to the existing `page_cache/<sha>/p<N>.png`, OR the sidecar rasterizes — decide §5.)
- **Response:** per page → `{ "text": "...", "lines": [ {"text":"…","bbox":[…],"conf":0.97} ], "mean_conf": 0.95 }`.
  Line bboxes + confidence enable later geometry (footnote/column zoning) and a low-confidence flag.

### 3.5 Chunk mapping + invariants
- OCR text flows into the **same** structure/heading pipeline as digital text — Phases 2–8 apply
  unchanged. The "verbatim" baseline for a scan IS the OCR text (no other ground truth).
- **Low-confidence flag:** pages/lines below a confidence floor → mark the resulting chunks
  `label_status='unresolved'` (refuse-or-flag) so a garbled citation is never presented as confident.
- Numeric guard: keep the existing year/number guards; PaddleOCR reduces but doesn't eliminate slips.

### 3.6 Performance
~1.7 min/page CPU in the spike (MKLDNN disabled to dodge the oneDNN crash). Implications:
- **v2 long-running sidecar** (load the model once) is important for multi-page scans.
- OCR stays an **explicit / "deep"-style on-demand** action per file (like the ⚡ deep button), NOT the
  default fast-tier path — a 200-page scan at ~1.7 min/page is hours. Surface progress + cancel.
- Optional GPU path if available (big speedup) — detect at sidecar start.

### 3.7 Acceptance
- On the parked scan (`04_Sindh_Local_Government_Act_2013.pdf`): the dates/numbers tesseract garbled
  (`2613`, `7013`, `20413`) read correctly; every 3+ digit run is real (spike already showed this).
- Golden: a few scanned pages' numbers pinned.

### 3.8 Risks / open
- Paddle 3.x oneDNN crash — pin the working config (MKLDNN off) in the frozen binary.
- Model size inflates the sidecar binary (§1.2). Consider a smaller PP-OCR model.
- Per-page timeout + fallback to tesseract on failure.

---

## 4. Phasing & sequencing
1. **Verify front-matter first** (current re-index → iteration 7) and finish the doc-06 vocab/roster
   fix. Don't start sidecars until the deterministic parser is clean.
2. **Phase 9a — table sidecar (pdfplumber): ✅ DONE 2026-06-20 (NOT committed).** Shipped as a
   PyInstaller frozen binary (`binaries/ingestion-sidecar-<triple>`, gitignored), v1 one-shot CLI,
   **ON by default** (`ECHO_TABLE_SIDECAR=0` opts out; safe because absent-binary/failure falls
   through to the projection engine). Reuses the existing `linearize` for an identical golden row
   format. Proven on the 4 spike docs (Delegation 75t/796r; SPPRA none→fallback; Compendium
   29t/82r; PC-I 63t/373r); `cargo test --lib` 529/0/1 + `golden_delegation_p8_reflow`.
   See CHANGELOG_DEV 2026-06-20. **Pending:** live re-index spot-check of retrieval.
3. **Phase 9b — OCR sidecar (PaddleOCR): ✅ DONE 2026-06-21 (NOT committed).** Runs as a Python
   **venv** (NOT frozen — user decision: Paddle is heavy/finicky to freeze and the IBA GPU server
   runs Python directly). Same `ingestion_sidecar.py`, new `ocr` op; Rust `ocr_sidecar.rs` client;
   `ocr::ocr_pdf_paddle` + `paddle_ocr_enabled()` gate (**ON by default**, `ECHO_PADDLE_OCR=0` opts
   out; safe because it also requires the venv `available()` + OCR only fires for scans); `pdf.rs`
   Tier-3 prefers Paddle when enabled, else tesseract (always the fallback).
   Key fix: `enable_mkldnn=False` **constructor kwarg** dodges the 3.x oneDNN/PIR crash (env var
   ignored). Verified on doc 04: `2613`/`7013` → correct `2013`; conf 0.98–0.99. `cargo test --lib`
   533/0/2. See CHANGELOG_DEV 2026-06-21. **Pending:** full app re-index of doc 04 with the flag;
   GPU paddle build on the IBA server; low-conf→`unresolved` chunk flag (deferred).
4. **Shared work (do in 9a):** the sidecar discovery/lifecycle/packaging harness, reused by 9b.

## 5. Open questions (decide before coding)
- **Packaging:** PyInstaller-frozen binary (recommended) vs bundled venv vs first-run pip?
- **Process model:** v1 CLI vs v2 long-running for tables (OCR needs v2 regardless).
- **Rasterization owner:** Rust (reuse `page_cache`) vs the sidecar, for OCR.
- **Gating surface:** env flags vs a Settings toggle vs folding into the existing "deep" per-file action.
- **Cell delimiter** for table rows (locks into citations/golden).
- **Fast vs deep tier:** tables in fast tier (deterministic) or deep? OCR is effectively deep (slow).

## 6. Non-goals
- No LLM/structure changes (Phases 2–8 stand). No PyMuPDF (AGPL). No camelot (lattice ≈ pdfplumber but
  slower/needs Ghostscript; stream worse). No change to digital prose extraction (pdftotext stays for
  text; pdfplumber adds tables only).
