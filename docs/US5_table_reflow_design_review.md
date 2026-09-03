# US5 Table Reflow — Design Review of the Follow-up Diagnosis

**Date:** 2026-06-18
**Branch:** `012-doc-intelligence-production`
**Reviews:** `docs/US5_table_reflow_diagnosis.md`
**Doc under test:** `09_Sindh_Delegation_of_Financial_Powers_2019.pdf` (91 pages, 704 chunks)
**Engine:** `src-tauri/src/services/parsers/table.rs` + `pdf.rs::maybe_reflow_tables`

> This is a rigorous design review of the diagnosis and the three proposed fixes, plus the
> architecture / roadmap questions. Written to be handed to other AI tools for a second
> opinion. **No fix has been applied yet** — this is analysis and a recommended sequence.

---

## Decisive new empirical finding

Before any opinion — one measurement reshapes several answers below.

`pdftotext -layout` of the Delegation PDF was grepped for the schedule header and the
enumerator sub-header:

```
schedule header  "Sr.# Nature of power To whom delegated Extent" : 55 occurrences
enumerator row   "(1) (2) (3) (4)"                               : 55 occurrences
3-col / 5-col enumerator variants                                : 0
form-feed pages                                                  : 91
```

**Every one of the ~55 schedule pages re-declares its own 4-column header, immediately
followed by `(1) (2) (3) (4)`.** The header/enumerator pair is not a one-time thing on the
first page — it is repeated on every page, 55 times, always 4-column. This means the
document **carries a built-in, self-repeating schema declaration**, and any page can be
anchored on its own header without depending on neighbours.

---

## PART 1 — Root cause: VALIDATED, with one sharpening

**Agreed with the diagnosis.** All seven mechanisms are real; the load-bearing ones were
verified against the actual bbox geometry (see §3 of the diagnosis doc).

**Sharpening — do not over-generalize:** the recurrence model is **not "fundamentally
flawed for tables."** It is flawed for *this document class specifically*, for a precise
reason: it tries to *infer* the column schema from body-row statistics **when the document
already declares its schema explicitly, on every page, 55 times.** Using a noisy
statistical estimator to recover information sitting in plain text two lines above is the
actual design error. The fix is therefore not "better statistics" — it is "stop
estimating, start reading the declared schema."

Per-mechanism:
- **(1)–(4) Sparse-column drop / minority-row:** confirmed by the cluster math — Extent
  (x≈429/495) appears only on row-start lines and is filtered by the 34% support
  threshold. Intrinsic to recurrence + long wrapped cells.
- **(5) Phantom columns (`Sector····Development`):** real; header-anchoring also fixes it
  (a word at x≈310 falls inside col-3's band [254,429) and stays put).
- **(6) Sub-headings inside the region (`COLLEGE EDUCATION DEPARTMENT`):** real but
  *orthogonal* — a region-bounding failure, not a column failure. Fixed by boundary
  detection (Part 4), not by columns.
- **(7) The gate measures shape, not correctness:** "≥3 lines, ≥2 columns" is
  necessary-not-sufficient. Confirmed.

**Alternative interpretation of the geometry?** None benign. The stored chunks are
objectively scrambled (§4 of the diagnosis). At most a handful of the 121 reflowed chunks
are accidentally correct (trivial 2-row tables) — but the engine cannot tell which, which
is precisely the citation-grade hazard.

---

## PART 2 — Fix review

### Fix 1 (header-anchored): the right primitive — implement as the CORE, not an add-on

Empirically backed now: header + enumerator = a schema declaration repeated on every
page. Implementation, with divergences from the original sketch:

1. **Enumerator row = column-count oracle.** `(1)(2)(3)(4)` gives N with near-zero
   ambiguity. If a page's header parse doesn't yield exactly N bands → reject the page
   (don't guess).
2. **Header row = column-band oracle.** Cluster header words into N groups; each column's
   band is `[group_left, next_group_left)` (last → page right edge). **Do not** use
   enumerator x-centers for bands — enumerators are *centered* while body cells are
   *left-aligned*; mixing them reintroduces misalignment. Enumerator for COUNT, header for
   SPANS.
3. **Hard assignment, no voting.** Every body word → the band its x-center falls in.
   Recurrence deleted entirely.
4. **Row segmentation by serial pattern, not "col-0 non-empty."** New row starts when
   col-0 holds a serial token (`^\d+\.?$`); empty col-0 → wrapped continuation, append
   per-band. More robust than the current `cells[0].trim().is_empty()`.
5. **Monotonic-serial gate** (Fix 2) as the correctness check.

**Continuation pages — big simplification from the 55× finding:** you mostly **do not
need cross-page column inheritance.** Each page self-anchors on its repeated header.
Cross-page logic shrinks to one residual case: a *single row* whose wrapped description
spills past the page break before the next page's header. Rule: re-anchor each page on its
own header; if a page's first data line has an *empty serial column*, treat it as a
continuation of the prior page's last row. This lets you **delete** the fragile
column-signature stitch currently carried via `head.cols`.

### Fix 2 (confidence gate): ship FIRST — but fix the metric set (one check is dangerous)

- ❌ **"No dominant single-column occupancy" — drop it.** col-3 ("To whom delegated")
  *legitimately* holds 80–90% of words because it wraps 30–40 lines. This check would
  **false-reject every valid Delegation table** — it mistakes the correct shape for the
  broken one.
- ✅ **Enumerator-count == header-band-count == N.** Strong, cheap, document-specific.
- ✅ **Monotonic serial sequence.** Best correctness oracle: emitted col-0 serials must be
  a clean increasing integer run (resettable per Part). If not → row segmentation is wrong
  → `None`. Nearly free.
- ✅ **All-caps standalone heading inside region → reject (or split out, Part 4).**
- ✅ **Header anchor must exist** (or clean continuation). No anchor → `None`.

**Formalizing "a missed table is acceptable; a scramble is not":** make `reflow_page` a
**total, verifiable function** with this contract:

> Return reconstructed rows **only if** (a) a header+enumerator schema was parsed, (b)
> every emitted cell is a contiguous verbatim substring of the page's word stream (G1), and
> (c) the serial column is monotonic. **Otherwise return the original `-layout` text
> byte-for-byte.** Reconstruction is never partial, never best-effort.

This makes the safety property *checkable* rather than aspirational. The current gate fails
because it asserts shape pre-build and never re-checks output.

⚠️ **Fix 2 alone is necessary but not sufficient:** falling back to `-layout` still yields
interleaved/scrambled text for these tables (just not *confidently folded* scramble). It
removes the false confidence (the citation-grade danger) but does not make tables citable.
Pair with Fix 1.

### Fix 3 (header-seed sparse columns onto recurrence): REJECT

Patching the wrong model. If you have the header (you do, 55×), anchor on it completely
(Fix 1). Keeping recurrence + bolting on header-retention preserves all of recurrence's
failure surface (phantom columns, region bounding, voting) for zero benefit — the worst of
both, more code, same fragility.

---

## PART 3 — Architecture: continue the geometry engine; do NOT adopt PP-Structure here

**Not a sunk-cost trap IF you pivot the algorithm — and it IS one if you keep iterating
recurrence (Option C).** The distinction:

- The **input signal (`pdftotext -bbox`) is correct and sufficient.** Word boxes are
  exactly what's needed.
- The **column-inference algorithm is the dead end.** Replacing "statistical recurrence"
  with "read the declared header/enumerator schema" reuses the same good input with a
  better algorithm. The bbox plumbing, page-split, line-grouping, linearizer, and tests are
  all reusable — not thrown away.

**Against PP-Structure for THIS corpus:**
- **Correctness / citation-grade:** ML layout detection is probabilistic. For "one wrong
  citation = critical failure," a deterministic reader of an explicit, repeated, textual
  schema is strictly more auditable than a neural table-structure model. Trading a
  verifiable oracle for a confidence score is the wrong direction.
- **Engineering / maintenance:** PaddlePaddle/PP-Structure means an ONNX or Python runtime,
  hundreds of MB of weights, and non-determinism — inside an **offline Rust desktop app.**
  A large, permanent bundling and maintenance burden.
- **Wrong tool for the problem:** the corpus is **digital text with a self-describing
  schema repeated 55×.** Using ML here is like OCR-ing a `.txt` file.
- **The spec already reserves PP-Structure for true scans** — and this corpus has none.

**When PP-Structure WOULD be justified:** a future *scanned* schedule, or a table with no
textual header/enumerator. Keep it as a documented escape hatch (region detection +
digital-text fill by bbox overlap), not the launch path.

---

## PART 4 — Schedule misattribution: agreed, a structure-parser problem

A flawless table reflow attached to `Rule 27(5)` is still uncitable. Fix separately from
tables.

**Redesign:**
1. **Structural-boundary lexicon (generic, not per-document):** `SCHEDULE`
   (FIRST/SECOND/…/nth), `PART`, `ANNEXURE`, `APPENDIX`, `FORM`, `CHAPTER`. When one
   appears **as a heading line** (short, standalone, all-caps/centered, not mid-sentence)
   it **opens a new top-level structural node** and **resets provision-label inheritance.**
2. **Disambiguation is essential.** "Second Schedule" appears constantly in prose ("…in
   column 2 of the Second Schedule…"). Treat as a boundary only when geometric/lexical
   signals say *heading* not *reference*: standalone short line, caps, often centered, no
   trailing sentence. Reuse the existing chrome/geometry signals.
3. **Explicit `Schedule` node kind: yes.** Model:
   `Schedule("Second Schedule") → Part("Part-I") → Section("Powers Common to All
   Departments" / "College Education Department") → Row(...)`. Rows cite as "Second
   Schedule, Part-I, Sr. 1" — not "Rule 27(5)."
4. **Stop label inheritance at boundaries.** The US2 resolver currently carries "Rule 27"
   across the schedule break; the boundary token must clear the inherited label.

**Preventing future absorption (annexures/appendices/forms):** the lexicon above is one
generic mechanism for all of them — generalizes across the corpus without per-document
keyword lists. SCHEDULE/PART are document-*class* grammar, not document-specific content
(honors the "no per-document keyword lists" guardrail).

---

## PART 5 — ROI ranking

| Rank | Option | Quality gain | Complexity | Risk | Maintainability |
|---|---|---|---|---|---|
| **1** | **A — Fix 2 safety gate** | Med (stops *confident* corruption; tables still uncitable) | **Low** | **Low** | High — pure, testable invariant |
| **2** | **E — schedule-boundary detection** | **High** (correct labels/location for all annexures, corpus-wide) | Med | Low–Med (prose-vs-heading disambiguation) | High — generic lexicon |
| **3** | **B — Fix 1 header-anchored reconstruction** | **High** (actually citable rows) | Med | Med (continuation edge cases) | **High** (deletes cross-page stitch fragility) |
| 4 | **F — manual curation of schedules** | High (guaranteed-correct now) | Low–Med | Low | **Low** (breaks on doc updates; doesn't scale) — best as *stopgap + golden ground-truth* |
| 5 | **C — keep improving recurrence** | Low/negative | Med | **High** (whack-a-mole) | Low — **sunk-cost trap. Reject.** |
| 6 | **D — PP-Structure + bbox fill** | Med here / High only for scans | **High** (ML runtime, bundling, non-determinism) | High for citation-grade | Low–Med — **wrong tool for this corpus. Defer.** |

Nuance: **A alone removes danger, not the gap.** **E is the highest pure ROI** —
correct-by-construction, generic, fixes labeling for every schedule/annexure regardless of
table handling. **B is the real feature.** **F is a smart backstop** — for 10 PDFs,
hand-curating the few critical schedules gives guaranteed citation-grade now and doubles as
golden fixtures while B is built.

---

## PART 6 — Recommended implementation sequence (tech lead, 4–6 weeks)

Ordering: **stop corruption → correct attribution → correct reconstruction → backstop the
residue.** Optimize for verifiable correctness, not coverage.

**Week 1 — Stop the bleeding (A) + measurement**
- Rewrite `reflow_page` as a **total verifiable function**: emit only if (header+enumerator
  parsed) ∧ (all cells verbatim) ∧ (monotonic serials); else `-layout` unchanged. Drop the
  "dominant single-column" idea.
- Tag output: `block_class='table_row'` + `reconstruction_method` so reflow coverage is
  measurable in the DB (currently null everywhere — flying blind).
- Build golden fixtures from the **real page-8 bbox** (giant wrapped col-3, sparse Extent)
  and a `COLLEGE EDUCATION DEPARTMENT` page — both must **fail against today's code.**
  Re-index; confirm the 121 confident-garble chunks collapse to honest `-layout` fallback.

**Week 2 — Schedule boundaries (E)**
- Boundary lexicon + heading-vs-reference disambiguation. Explicit `Schedule`/`Part`/
  `Section` nodes. Clear inherited Rule labels at boundaries.
- Re-index; verify schedule chunks label as "Second Schedule, Part-I, …" not "Rule 27(5)."
  This alone makes the (still-imperfect) table text correctly located and section-citable.

**Weeks 3–4 — Header-anchored reconstruction (B)**
- Enumerator → column count; header words → bands; hard x-assignment; serial-pattern row
  segmentation; wrapped-line append; monotonic-serial verification.
- Per-page self-anchoring (header repeats 55×); single continuation rule for cross-break
  rows. **Delete** the column-signature cross-page stitch.
- Re-index; run the golden gate on **all 55 schedule pages**; full manual spot-check of the
  Delegation Second Schedule end-to-end.

**Week 5 — Backstop + generalize (F + fixtures)**
- For any schedule page still failing the gate after B, **hand-curate as structured data**
  (bounded — finite pages across 10 PDFs); lock as golden ground-truth. Guarantees
  citation-grade on the critical financial tables now.
- Extend fixtures/boundary tests to other schedule-bearing docs (Compendium, APT Rules,
  SPPRA).

**Week 6 — Hardening & regression lock**
- Freeze the golden gate (G2), wire it into the pre-re-index checklist, append
  `CHANGELOG_DEV.md`, update the MEMORY topic file.

**Explicitly deferred:** Option C (recurrence) — deleted, not iterated. Option D
(PP-Structure) — documented escape hatch for a future *scanned* table only.

---

## On the appended observation (header+enumerator as the column oracle)

Strongly agree — and the data backs it harder than the original note stated: the
header+enumerator is not merely "valuable," it is a **schema declaration the document
repeats 55 times.** Biasing to it over body-row statistics is not a heuristic preference;
for this document family it is the difference between *reading* the schema and *guessing*
it. It also collapses the cross-page complexity — a maintenance win on top of the
correctness win.
