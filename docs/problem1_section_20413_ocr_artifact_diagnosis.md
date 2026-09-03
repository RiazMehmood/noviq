# Problem 1 — `Section 20413` Extraction Artifact: Diagnosis, Analysis & Recommendations

**Feature:** 012-doc-intelligence-production · **Date:** 2026-06-19 · **Status:** diagnosed, fix not yet implemented
**Severity:** 🔴 false citation (the only remaining *wrong* label in the corpus)

---

## 1. The Problem

The corpus contains 33 chunks labeled `Section 20413` (and sub-variants `Section 20413(1)`,
`Section 20413(3)(b)`, …). `20413` is not a real section number. If cited, it produces a fabricated
legal reference — a direct citation-grade contract violation.

---

## 2. Evidence (live DB + source file)

**Source:** `04_Sindh_Local_Government_Act_2013.pdf` — **all 33 chunks**, `extraction_method = ocr`.

**The PDF is a genuine image-only scan** (decisive — `pdftotext` / `pdffonts` / `pdfimages`):
- `pdftotext` over all 8 pages → **0 characters** (no text layer).
- `pdffonts` → **no fonts listed** (no embedded text).
- `pdfimages -list` → **each page is ONE full-page JPEG**, RGB, **150 ppi**, 8 pages.
- Page size 541×734 pts.

So there is **no digital text to fall back to** — the document *must* be OCR'd, and the OCR ran on a
**low-resolution (150 ppi) RGB JPEG scan**.

**The OCR text is badly degraded** (chunk content samples):
```
"20413.  Amendment o  section 46 of Sindh Act NG. XiLil of  2013"
"the Ejection Commission"        (← Election Commission)
"under the heaaing"              (← heading)
"the followlag Insertion"        (← following)
"entrieg. 18,"                   (← entries)
```

**The document is an Amendment Act** — its real sections are small ordinals (`23.`, `25.`, `26.`,
`27.`, `30.` are visible in the garbled text: "23. In the said Act, section 75 shall be substituted…").
The repeated "Sindh Act No. XLII of 2013" (XLII = 42) confirms it amends the 2013 Act.

**Why one phantom poisoned 33 chunks:** the OCR garble produced a spurious token `20413.` near the top,
which the heading detector latched onto as `Section 20413`. Because the rest of the OCR is too degraded
for the detector to find the real section headings, almost the entire document collapsed under this one
phantom section, and stray sub-markers `(1)`/`(2)`/`(3)`/`(a)`/`(b)` (from different real sections)
nested beneath it → `Section 20413(3)(b)` etc.

**Magnitude guard is safe (verified):** no legitimate provision label in the whole corpus has ≥4
digits. The only chunks with a ≥5-digit provision number are exactly these 33.

---

## 3. Root Cause

**This is an OCR-quality problem, NOT a heading-regex or pdftotext-merge problem.**

- The earlier hypothesis ("`204` + page `13` merged by pdftotext") is **WRONG**. There is no pdftotext
  text layer at all; the source is an image-only scan and `20413` is OCR noise.
- The input text quality is the ceiling: tesseract on a 150 ppi RGB JPEG of an amendment act yields
  garbled words AND garbled numbers. No downstream regex can recover correct section numbers or
  verbatim quotes from this.

Two distinct failures stack:
1. **Garbled content** — quotes/words are unreliable (`Ejection Commission`, `heaaing`).
2. **Garbled structure** — a phantom `20413` heading swallows the document, mislabeling everything.

---

## 4. Recommendations (layered — cheapest/safest first)

### R1 — Index-time implausible-number guard *(do first; cheap, generic, safe)*
Mirror the existing `looks_like_year` guard: a provision ordinal with an implausible magnitude
(**≥ 5 digits**, i.e. ≥ 10000) is extraction noise — never mint it as a `Rule/Section/Article` label;
flag as unresolved instead. Verified zero false positives (no real label ≥4 digits anywhere). This
**kills the false `Section 20413` citation generically** (turns it into a flagged/unresolved label,
satisfying refuse-or-flag > confidently-wrong). Index-time → one re-index.
*Limitation:* does not fix the garbled content; it only stops the fabricated label.

### R2 — Quality-gate the document *(recommended alongside R1)*
A 150 ppi scan of an amendment act cannot be citation-grade. Flag/score this source as low-confidence
OCR (`quality_score`) so citation-grade drafting **excludes it or surfaces a warning**, rather than
silently quoting `Ejection Commission`. Uncertainty as a first-class state.

### R3 — Fix the source *(the real fix for THIS document)*
This is one known, public law. The cleanest path to citation-grade is to **obtain a digital / higher-
resolution copy** of the *Sindh Local Government (Amendment) Act 2013* and re-ingest it — sidestepping
OCR entirely. Most pragmatic for a single document; gives clean text + real section numbers.

### R4 — Better offline OCR *(if a digital copy is unavailable)*
If only the scan exists, upgrade the OCR path:
- **Preprocess** before OCR: render page images larger via `pdftoppm` (limited gain — source is only
  150 ppi), convert to **grayscale**, **binarize**, **deskew/denoise**. This helps any engine.
- **Swap tesseract → PaddleOCR (PP-OCRv4)**, which is markedly stronger on degraded scans and is
  **offline via the project's existing `ort` (ONNX) runtime** — exactly the "scanned page" case
  PP-Structure was reserved for in the 012 plan. Even so, 150 ppi imposes a hard ceiling.

---

## 5. Research — offline OCR landscape (for R3/R4)

| Engine | Quality on degraded scans | Offline | Notes |
|---|---|---|---|
| **tesseract** (current) | Weak; this exact garble | Yes | Wants ≥300 ppi grayscale binarized; 150 ppi RGB is its worst case |
| **PaddleOCR PP-OCRv4** | Strong | Yes (ONNX/`ort` already vendored) | Best offline option; reserved for scans in the 012 plan |
| **docTR** | Good | Yes | Heavier deps |
| Google Vision / Azure / Textract | Best | **No** (cloud) | Violates offline constraint; not eligible |

**Key finding:** the dominant lever is **input resolution + preprocessing**, not the engine. A 150 ppi
RGB JPEG is below the practical threshold for reliable legal-text OCR; a clean digital source (R3)
beats any OCR. PaddleOCR (R4) is the best offline fallback if no digital source exists.

---

## 6. Recommended decision

1. **R1 (implausible-number guard) + R2 (quality-gate)** now — generic, safe, one re-index; removes the
   false citation and prevents silent garbled quotes. This satisfies the citation-grade contract for
   the corpus today.
2. **R3 (digital source)** for true fidelity of this Act — out-of-band (acquire a better file), then
   re-ingest. **R4 (PaddleOCR)** only if R3 is impossible.

R1+R2 are in-scope code changes; R3 is a data action; R4 is a larger pipeline change (defer unless
needed). Success criterion for Problem 1: **0 `Section 20413` labels**, and the document either
excluded from citation-grade or re-ingested clean.

---

## 7. Proposed code touch-points (for R1+R2, pending approval)

- `heading_detector/regex_pass.rs` — add `looks_like_noise_number(num)` (≥5 digits) beside
  `looks_like_year`; in `scan_with_profile`, skip minting a label when it fires (parallel to the
  year-guard at the `num` capture).
- Mirror the same guard in any runtime label path (`intent_router::decompose`) for symmetry.
- `structured_importer` / source `quality_score` — flag the OCR doc as low-confidence; retrieval/
  drafting honor it (R2).
- Tests: `noise_number_not_minted_as_provision` (20413 → not a Section; 260/25A still parse).
