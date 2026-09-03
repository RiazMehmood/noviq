# Legal-RAG Ingestion & Drafting — Defects, Remedies, Current State, and Plan

**Date:** 2026-06-17 · **Purpose:** a self-contained briefing (shareable with
external AI models / reviewers) describing the data-quality and drafting defects
in our offline legal-RAG system, every remedy we have tried, what we still
observe on the live re-indexed corpus (with hard numbers), and the options we
are weighing next. No access to our codebase is assumed.

---

## 1. System context (what this is)

We are building an **offline, on-device** retrieval-augmented system that drafts
**citation-grade** Government-of-Sindh secretariat summaries and notings from a
fixed corpus of Pakistani legal/procurement documents. "Citation-grade" is a
hard contract:

> The system may **never** invent or paraphrase a rule/section number, a quoted
> legal text, or a monetary figure. All such tokens must be lifted **verbatim**
> from the source. The language model is confined to writing connective prose
> with placeholder markers (`[CITE:n]`, `[FIG:n]`); the numbers/quotes are
> spliced in deterministically and verified by a gate. A single wrong citation
> is a critical failure.

**Stack (for technical context):** Rust + Tauri desktop app; PostgreSQL 17 +
`pgvector` for storage and vector search; `fastembed` embeddings; a 7B
instruction model (Mistral-7B-Instruct, Q4_K_M GGUF) for prose; hybrid retrieval
= dense KNN (pgvector) + lexical (`tsvector`/`ts_rank`) fused with Reciprocal
Rank Fusion. PDF text is extracted with **poppler `pdftotext`** (`-layout` mode
primary; `lopdf` and Tesseract OCR as fallbacks).

**Corpus (live, 10 source PDFs, 4,277 chunks total):**

| Source | Chunks |
|---|---|
| 05_Compendium_Rules_under_SLGA_2013.pdf | 1,304 |
| 10_SPSC_Regulations_2023.pdf | 1,134 |
| 09_Sindh_Delegation_of_Financial_Powers_2019.pdf | 856 |
| 02_SPPRA_Rules_2010_Revised_2013.pdf | 190 |
| 03_Sindh_Public_Procurement_Act_2009.pdf | 137 |
| 11_Sindh_Civil_Servants_APT_Rules_1974.pdf | 123 |
| 01_Sindh_Rules_of_Business_1986.pdf | 105 |
| 04_Sindh_Local_Government_Act_2013.pdf | 85 |
| 06_Manual_for_Development_Projects_2019_PC-I.pdf | 264 |
| Govt. of Sindh Manual of Secretariat instruction 2010.pdf | 79 |

### Glossary
- **Chunk** — a stored unit of text retrieved and cited. Each row in the
  `chunks` table has `content` (the text), `unit_label` (e.g. `Rule 27`, or a
  bare marker like `(i)`), `unit_path` (hierarchical position), and now an
  `is_boilerplate` flag.
- **`pdftotext -layout`** — extracts text preserving visual columns with spaces,
  read strictly left-to-right per visual line. **`pdftotext -bbox`** — emits XML
  with each word's bounding box (x/y/width/height).
- **Structure parser** — regex/profile-driven pass that detects provision
  headings ("Rule N", "Section N") and builds a hierarchical tree; leaves become
  chunks with hierarchical `unit_label`s.
- **Two-lane drafting** — the LLM writes prose with `[CITE:n]` markers only; a
  deterministic step inserts the verbatim quote + label for card *n*; a verify
  gate drops anything that breaks the contract.

---

## 2. The defects (with live evidence)

### D1 — Multi-column tables are scrambled ("tabular garble") 🔴 OPEN
The single most important tables (Delegation of Financial Powers: who may
sanction how much) are multi-column with multi-level headers (Category I–IV),
merged cells, and rows that span page breaks. `pdftotext -layout` reads
left-to-right per visual line, so wrapped cells interleave and the row's meaning
is destroyed.

Live example (a real stored chunk, Delegation 2019):
```
unit_label = (i)
content    = "Rule 27 / in Category I. / in Category II. / in Category III. /
              in Category IV.; Full Powers. / Full Powers. / Full Powers"
```
Another: `"8\x0C— (v) Officers in Category IV; Up to Rs.0.050 the financial;
year; million during"` — note the `\x0C` (form-feed) showing a **row split
across a page boundary**, and the scrambled word order. Such a chunk is useless
as a citation and, when retrieved, the LLM fabricates a plausible-sounding gloss
over it (a citation-grade violation).

### D2 — Year numbers minted as provision ordinals ✅ RESOLVED
Headings like "…Rules, **2017**…" were parsed so the year became the ordinal →
labels like `Rule 2017`, `Section 2013`. Originally 39 such labels.

### D3 — Boilerplate / page-chrome indexed as provisions 🔴 OPEN (worse than thought)
Contact pages, notification mastheads, form fields, admission-slip footers, and
page markers ("P a g e", "__ 107 __") are stored as retrievable chunks. Crucially
(see §4) they are **swept as sub-content under real provision nodes** and inherit
provision labels.

Live examples:
```
unit_label=(a)  content="...Contact Us: Email ..."        (under Rule 158)
unit_label=(e)  content="10 | P a g e\x0C15.. Re-Advertisement..."
unit_label=(b)  content="Rule 345 / __ 107 __ / GOVERNMENT OF SINDH /
                          LOCAL GOVERNMENT AND HOUSING / TOWN PLANNING
                          DEPARTMENT / Karachi dated 4 th October"
```
The last one is the worst case: it composes to the **plausible label
"Rule 345(3)(b)"** but its content is a notification masthead. A draft cited it
as the legal basis for a procurement decision.

### D4 — Label ≠ content (mislabel) 🔴 OPEN
Distinct from D3 in cause but related: a chunk's `unit_label` disagrees with the
provision its content actually begins. Root cause is heading-detection misses —
a real heading isn't recognised, so its text is absorbed under the preceding
sub-clause and inherits the wrong label.

### D5 — Bare-marker labels (`(i)`, `(b)`, `m2`, `m66`) 🟠 OPEN
Many chunks carry only a bare sub-marker (`(i)`, `(5)`) or a fallback token
(`m2`, `m66`) instead of a resolved provision citation. When cited, the label is
meaningless to a reader.

### D6 — Over-fragmentation 🟢 MOSTLY ADDRESSED
The corpus was previously shattered into tiny scraps (median ~41 chars; 55%
under 50 chars), so no single chunk held a complete provision. A coalescing pass
merged adjacent small leaves into ~300–800-char provision units (corpus dropped
from ~20k to ~8k, now 4,277 after subsequent work). Largely fixed, but residual
fragmentation persists inside the mangled tables (D1).

### D7 — Model artifacts in the draft (drafting layer, not ingestion) 🔴 OPEN
Even with clean inputs, the 7B prose model: (a) adds advocacy/puffery ("essential",
"swift acquisition", "would facilitate"); (b) **fabricates glosses** over cited
provisions; (c) emits broken sentences ("direct contracting … may the acquisition
process"); (d) leaks skeleton scaffolding ("Paragraph 3 (Optional):") into output;
(e) writes a 3-sentence prayer where one is required; (f) restates the same facts
across sections.

---

## 3. Remedies we have tried (and the outcome of each)

| # | Remedy | Targeted | Outcome |
|---|---|---|---|
| R-a | **Coalescing pass** — merge adjacent tiny leaves into provision-sized units | D6 | ✅ Worked; corpus de-fragmented |
| R-b | **Prefer `pdftotext -layout` over `lopdf`** + per-line fragmentation quality signal | extraction quality | ✅ Worked for prose; clean digital text |
| R-c | **Heading-mislabel fix** — broaden bare-number heading regex + a generic out-of-sequence filter + an index-time `label≠content` warning | D4 | 🟠 Partial; reduced mislabels but the Compendium/SPSC content-sweep cases (D3/D4) persist |
| R-d | **Year-rule** — treat a bare 1800–2099 4-digit in heading position as a year, not an ordinal | D2 | ✅ Worked (see §4) |
| R-e | **Tabular module v1** — detect tables from `pdftotext -bbox` geometry (≥3 consecutive lines with ≥2 x-aligned columns), reconstruct rows, linearize headers-folded with verbatim cells | D1 | ❌ Did **not** fire on the real tables (see §4) |
| R-f | **Boilerplate flag (flag-not-delete)** — a generic structural classifier (page markers, email/URL/phone, low prose ratio) sets `is_boilerplate`; retrieval excludes flagged chunks; never deletes; a "provision-label-keep invariant" protects anything with a `unit_label` | D3 | ❌ Flagged **0** chunks (see §4) — the invariant neutralised it |
| R-g | **Advocacy strip** — remove a generic pleading/puffery lexicon from prose, verbatim-safe | D7a | 🟠 Too weak; most advocacy survives, and one variant broke grammar |
| R-h | **Two-lane + verify gate** — LLM never authors numbers/quotes; deterministic insertion + verification | D7 (numbers) | ✅ Numbers/quotes are never model-authored; but the model still fabricates *prose glosses* around them |

---

## 4. Live verification after the latest re-index (the evidence base)

Queried directly against the production PostgreSQL DB (4,277 chunks, 10 sources)
after implementing R-d/R-e/R-f and re-indexing. The running binary **is** the new
build (the new `is_boilerplate` column exists; year labels are gone).

**✅ R-d (year-rule) — WORKED.** A scan for `(Rule|Section) <year>` labels now
returns only `Section 20413` (a 5-digit extraction garble, not a year). All
`Rule 2017/2023/2016/1978`, `Section 2013/2018` labels are gone.

**❌ R-f (boilerplate flag) — flagged 0 of 4,277.** Two findings explain why:
- **Every chunk has a `unit_label` (0 of 4,277 are null/empty).** Our invariant
  "has a label ⇒ has provision identity ⇒ never flag" therefore protects
  *everything*. The classifier can never fire.
- **The junk inherits real provision labels.** The masthead chunk is stored as
  `unit_label=(b)` under `Rule 345`; "Contact Us: Email" sits under `Rule 158`.
  The boilerplate is **swept under provision nodes during chunking**, so it is
  *not separable by label*. R-f's founding premise (boilerplate is label-less)
  is false for this corpus.

**❌ R-e (tabular reconstruction) — did NOT fire.** The Delegation PDF is
`pdftotext`-extracted (so the module had its chance), yet the garbled
`"Rule 27 / in Category I. / in Category II. …"` chunk is still present
verbatim. Cause: the real table is **vertically-stacked (one cell fragment per
line), multi-level-header, and split across pages** — our v1 detector assumed
clean side-by-side rows on a single visual line, per page. It is **downstream of
`pdftotext`, which had already mangled the table**, so the geometry it needs is
gone before it runs.

---

## 5. What still persists (current state, honest)

- 🔴 **D1 tabular garble** — unchanged; R-e's geometric approach is too naive for
  these tables and is poisoned by upstream `pdftotext` mangling.
- 🔴 **D3 boilerplate** — unchanged; flag-based de-prioritisation is defeated
  because junk inherits provision labels (the problem is *upstream* at chunking).
- 🔴 **D4 mislabel** (e.g. masthead labelled "Rule 345(3)(b)") — persists.
- 🟠 **D5 bare-marker labels** — persist.
- 🔴 **D7 model artifacts** — persist (advocacy, fabricated glosses, scaffolding
  leak, long prayer, restating). These are drafting-layer, untouched by ingestion
  work.
- ✅ **D2 year labels** — resolved.
- 🟢 **D6 fragmentation** — resolved except inside mangled tables.

**Key reframing from the evidence:** D1 and D3 are both, at root, an **upstream
segmentation/extraction problem** — the structure parser sweeps non-provision
content (tables, mastheads, forms, page chrome) into provision subtrees. Any
downstream fix (retrieval filter, flag, post-hoc regex) is fighting the symptom.

---

## 6. What we are planning / weighing next

### 6.1 Tables (D1) — replace the geometric module
- **Option A — PP-Structure (PaddleOCR) hybrid.** Use PaddleOCR's table-structure
  recogniser on the **rasterised page image** to get the cell grid (it handles
  merged cells, multi-level headers, borderless tables — and is *not* downstream
  of `pdftotext`), then fill each cell's text from the **digital `pdftotext -bbox`
  layer by coordinate overlap** (NOT OCR), preserving verbatim cells.
  *Cost:* export several models to ONNX + drive under our Rust `ort` runtime; must
  still stitch cross-page rows. *Risk:* integration weight; if cells are OCR'd
  instead of digital-mapped, OCR errors would violate citation-grade.
- **Option B — improve the geometric detector** to handle vertical stacking,
  multi-level headers, and cross-page rows on existing `-bbox` data. No new deps,
  no OCR risk, but tables this complex are genuinely hard geometrically.
- **Option C — one-time curated extraction** of the handful of critical
  financial-powers tables into clean structured rows (authored once). Inelegant
  but citation-grade-safe and fast; pairs well with A as the long-term solution.
- *Leaning:* C now (unblock quality fast) + A as the real fix. Explicitly **not**
  full-page OCR of digital documents.

### 6.2 Boilerplate + mislabel (D3/D4) — move the fix UPSTREAM
- Stop **sweeping** mastheads / contact pages / form fields / page markers under
  provision nodes during structure-parsing. Detect them at segmentation time and
  keep them as separate, non-provision (or dropped) units, so they neither
  inherit a provision label nor pollute a provision's content.
- Redefine "provision identity" to mean a **resolved top-level provision label**
  (Rule N / Section N / Article N), not any bare sub-marker — so a future
  flag/guard isn't neutralised by D5-style labels.
- Add a retrieval-time / index-time **label≠content guard** (we already have a
  warning; promote it to an action) for the masthead-under-Rule-345 class.

### 6.3 Drafting artifacts (D7) — deterministic frames (no LLM for prose)
- Replace the 7B prose writer with **fixed sentence templates** filled from
  already-deterministic slots (verbatim subject, verbatim provision quote+label,
  verbatim figures, extracted stakeholders). This removes fabrication, advocacy,
  broken sentences, scaffolding leaks, and prayer-length issues **by
  construction**, at the cost of stiffer (but contract-safe) prose. Open question:
  fully deterministic vs. an optional LLM "polish" pass.

### 6.4 Knowledge graph (relational queries) — additive, not a replacement
- Add a **rule-based knowledge graph** (cross-references, amendments, definitions,
  hierarchy) as a fast-path for *navigational* queries ("what does Section X say",
  "what amended Y"), while **keeping vector search** for *situation-matching*
  queries (officer's words ≠ statute's words). A router picks per query class.
- A later phase: **error/inconsistency detection** over the graph (dangling
  cross-refs, broken amendment chains, numbering gaps, cycles) — but only **after**
  ingestion is clean, since today most "errors" found would be our own ingestion
  artifacts.

---

## 7. Open questions (for discussion with other models/reviewers)

1. **Tables:** for complex, cross-page, multi-level government tables in *digital*
   PDFs, is image-based table-structure recognition (PP-Structure) + digital-text
   cell-fill the right call, or is there a more robust *text-layer* approach we're
   missing (e.g. reconstructing from the PDF content stream / operator positions
   rather than `pdftotext`)?
2. **Boilerplate that inherits provision labels:** what is the most principled,
   *generic* (no per-document keyword lists) way to prevent page-chrome from being
   absorbed into provision subtrees during hierarchical segmentation?
3. **Verbatim vs. structure trade-off:** any technique to get table *structure*
   from an image model while guaranteeing *cell text* is the untouched digital
   text (coordinate alignment robustness)?
4. **Deterministic drafting:** for a citation-grade legal-summary register, is
   fully template-based prose acceptable, or is a constrained LLM polish pass
   worth the re-introduced (small) fabrication surface?
5. **Cross-page rows:** generic heuristics for stitching a table row split by a
   page break, without a layout model.

---

## 8. One-paragraph summary

We have a citation-grade offline legal-drafting RAG over 10 Sindh government PDFs
(4,277 chunks). Of seven defect classes, fragmentation and year-as-rule labels
are fixed; but after the latest re-index, **multi-column table reconstruction
still fails** (the tables are vertically-stacked, multi-level, cross-page, and
already mangled by `pdftotext` before our geometric module runs), and **boilerplate
de-prioritisation flags nothing** because page-chrome is swept under real
provision nodes and inherits their labels — so neither is separable downstream.
The reframing: D1 and D3 are one **upstream segmentation problem**. Our plan:
(1) tables via image-based table-structure recognition with digital-text cell-fill
(plus a curated stopgap), (2) move boilerplate/mislabel fixes upstream into
segmentation, (3) replace the LLM prose writer with deterministic templates to
kill fabrication, and (4) add a rule-based knowledge graph as a relational
fast-path alongside (not replacing) vector search.
