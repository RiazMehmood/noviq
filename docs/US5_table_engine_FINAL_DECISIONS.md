# US5 Table Engine — Final Implementation Decisions (Tech-Lead Sign-off)

**Date:** 2026-06-18 · **Branch:** `012-doc-intelligence-production`
**Author role:** acting technical lead, shipping the next production release.
**Supersedes the open questions in:** `docs/US5_table_reflow_diagnosis.md`,
`docs/US5_table_reflow_design_review.md`.
**Established findings (1–11) are taken as given.** This document makes decisions and
specifies exact code. It is not another review.

---

## 0. Decision summary (read this first)

| # | Workstream | Decision |
|---|---|---|
| 1 | Recurrence-based column detector (`detect_columns`) | **DELETE** |
| 2 | Header+enumerator schema-anchored reconstruction | **BUILD** (new core) |
| 3 | Column bands from header/enumerator *x-positions* | **REJECT** (header is centered, body is left-aligned — see §A.3) |
| 4 | Column left-edges from **body anchor rows** + `col_index` | **ADOPT** |
| 5 | "Total verifiable function" safety contract | **SHIP FIRST** (week 1) |
| 6 | `block_class='table_row'` + `reconstruction_method` tagging | **ADD** (we are currently blind) |
| 7 | Cross-page column-signature stitch (`detect_continuation` + `head.cols` carry) | **DELETE** (header repeats 55×; per-page self-anchor) |
| 8 | Confidence check "no dominant single-column occupancy" | **REJECT** (false-rejects valid Delegation tables — col-3 legitimately dominates) |
| 9 | Schedule/Part/Annexure boundary detection in `structure_parser` | **BUILD** (week 2) |
| 10 | Manual curation of critical schedules | **YES** — as a *backstop + golden ground-truth*, not the primary path |
| 11 | PaddleOCR / PP-Structure | **DO NOT INTRODUCE** — reserve for scans/images only |
| 12 | Keep iterating recurrence (Option C) | **STOP — sunk-cost trap** |

**Sunk-cost traps explicitly killed:** (a) any further tuning of the recurrence support
threshold; (b) cross-page column-signature stitching; (c) PP-Structure for digital,
schema-bearing PDFs.

**Accelerated:** the safety contract (week 1) and schedule-boundary detection (week 2) —
both are independent of the harder reconstruction work and each delivers citation-grade
value on its own.

---

## A. TABLES — delete recurrence, build schema-anchored reconstruction

### A.1 Verdict: DELETE the recurrence detector entirely

`detect_columns` (table.rs:360) infers columns from cell-start *recurrence frequency*.
This is the root cause of confident corruption: it discards the sparse Extent column
(34% support threshold), manufactures phantom columns from intra-cell wide gaps, and
votes using wrapped-continuation lines that carry no column structure. Finding 7 is
decisive — it is inferring a schema the document **declares explicitly 55 times.** Do not
improve it. Do not header-seed it (rejected Fix 3). Delete it.

### A.2 Replace with: header+enumerator as the *count/validation* oracle, body anchor rows as the *position* oracle

The algorithm has two independent oracles, and using the right one for each job is the
whole game:

- **Column COUNT and VALIDATION → header line + `(1)(2)(3)(4)` enumerator line.**
  The enumerator pins `N` with near-zero ambiguity. The header supplies column *labels*
  (for the linearizer) and confirms `N`.
- **Column POSITIONS (left-edges) → the page's body "anchor rows"** (lines whose first
  token is a serial, e.g. `1.`, `2.`). Body cells are **left-aligned**, so a column's left
  edge is the *minimum cell-start over anchor rows*.

### A.3 REJECTED sub-approach (important): do NOT derive column bands from header/enumerator x-positions

A prior review proposed assigning body words to bands defined by header word positions or
enumerator centers. **I reject this** — it reintroduces the exact interleaving bug.
Verified against the real page-8 geometry:

```
header  "Extent"  x = 495.7   (centered/right over its column)
body    "Full Powers" x = 429.7 (left-aligned, ~66pt to the LEFT of the header)
```

Header-gutter midpoint between col-3 and col-4 ≈ 443. Body "Full" at 437 < 443 → it falls
into **col-3**, splicing "Full Powers" into the authority column. That *is* the live bug.
Headers are centered; the body is left-aligned; their x-positions do not coincide. **Use
the body's own left-edges for assignment**, and use header/enumerator only to fix `N` and
validate.

### A.4 Why body-anchor-row left-edges work where recurrence failed

Two changes kill both failure modes:
1. **Cluster only anchor rows, never wrapped-continuation lines** → no phantom columns
   from wrapped prose, no dilution from single-column wraps.
2. **Pin the column count to `N` from the enumerator and take the `N` strongest clusters —
   no fractional support threshold** → the sparse Extent column survives (it appears on
   every anchor row), and the over-split col-3 marker cluster (`(i)` at 290) loses to the
   true edge (254) and is dropped.

Worked example, page 8 (anchor rows `1.` and `2.`):

```
anchor-row cell-starts: 91, 123.6, 254, 290, 429.7   (row 1, col-3 over-split at 290)
                        91, 123,   254, 429           (row 2)
cluster (left, support): (91,2) (123,2) (254,2) (290,1) (429,2)
take N=4 strongest      : 91, 123, 254, 429   ← 290 dropped, 429 (Extent) KEPT
```

Assignment via the existing `col_index` (greatest left-edge ≤ x): `(i)`@254→col3,
`Administrative`@290→col3, `Full`@429→col4. Correct. The wrapped col-3 lines
(`Departments…` at x=254) all bind to col-3 and fold into the row.

### A.5 Concrete code

**Delete:** `detect_columns`, `detect_continuation`, `line_aligns`, and the `head.cols`
cross-page carry in `reflow_tables`. **Add** (table.rs):

```rust
/// A parsed schema for one schedule page: column count + labels, from the header
/// line and the `(1)(2)(3)(4)` enumerator line directly beneath it.
struct PageSchema {
    ncols: usize,
    labels: Vec<String>,   // header words grouped per column (for the linearizer)
    header_line_idx: usize,
    enum_line_idx: usize,
}

/// Find a header line immediately followed by an enumerator line `(1)(2)..(N)`.
/// The enumerator pins N; the header (same N column-groups) supplies labels.
/// `None` → this page has no declared schema (caller keeps `-layout`).
fn detect_schema(lines: &[Vec<&WordBox>], col_gap: f32) -> Option<PageSchema> {
    for i in 0..lines.len().saturating_sub(1) {
        if !line_is_enumerator(&lines[i + 1]) { continue; }
        let n = lines[i + 1].len();                 // count of (1)(2)(3)(4)
        if n < 2 { continue; }
        let groups = group_cells(&lines[i], col_gap); // header words → cells
        if groups.len() != n { continue; }            // header must match N
        return Some(PageSchema {
            ncols: n,
            labels: groups.iter().map(|g| join_words(g)).collect(),
            header_line_idx: i,
            enum_line_idx: i + 1,
        });
    }
    None
}

/// Column left-edges from the page's DATA rows (first token = serial), reduced to
/// exactly `ncols`. Body cells are LEFT-ALIGNED → a column's left edge is the
/// minimum cell-start in its cluster. No fractional support threshold: N is fixed
/// by the schema, so we simply keep the N strongest clusters. Wrapped lines are
/// EXCLUDED (only serial-led rows vote). Requires >= 2 anchor rows, else None.
fn body_column_lefts(
    anchor_rows: &[&Vec<&WordBox>], col_gap: f32, ncols: usize, tol: f32,
) -> Option<Vec<f32>> {
    if anchor_rows.len() < 2 { return None; }       // conservative: missed > scrambled
    let mut starts: Vec<f32> =
        anchor_rows.iter().flat_map(|l| cell_starts(l, col_gap)).collect();
    starts.sort_by(|a, b| a.total_cmp(b));
    let mut clusters: Vec<(f32 /*left*/, usize /*support*/)> = Vec::new();
    for s in starts {
        match clusters.last_mut() {
            Some((left, c)) if (s - *left).abs() <= tol => *c += 1,
            _ => clusters.push((s, 1)),
        }
    }
    if clusters.len() < ncols { return None; }
    clusters.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.total_cmp(&b.0)));
    let mut cols: Vec<f32> = clusters.into_iter().take(ncols).map(|(l, _)| l).collect();
    cols.sort_by(|a, b| a.total_cmp(b));
    Some(cols)
}

fn is_serial(s: &str) -> bool {
    let t = s.trim().trim_end_matches('.');
    !t.is_empty() && t.chars().all(|c| c.is_ascii_digit()) && t.len() <= 3
}
```

`detect_page_table` becomes: `detect_schema` → collect anchor rows (first token
`is_serial`) in the band below the enumerator → `body_column_lefts` → `build_rows`
(unchanged) → **verify gate (§B)** → `linearize` with `schema.labels`. Per-page only; no
cross-page column carry. (A row whose wrapped cell spills past the page break is handled by
the §B continuation rule, not by carrying columns.)

---

## B. SAFETY — formal correctness contract

**`reflow_page` is a total function. It returns reconstructed rows ONLY when correctness
is verifiable; otherwise it returns the original `-layout` text byte-for-byte.** No partial
output, no best-effort.

### B.1 Required conditions to EMIT a reconstruction (all must hold)

1. **Schema present:** a header line + `(1)…(N)` enumerator line parsed on this page
   (`detect_schema` returned `Some`).
2. **Geometry resolved:** `body_column_lefts` returned exactly `N` left-edges from `>= 2`
   anchor rows.
3. **Monotonic serials:** the serials extracted into column 0 of the emitted rows form a
   strictly increasing integer run (reset allowed only at a new Part/Section boundary).
   *This is the primary correctness oracle.*
4. **Verbatim cells (G1):** every emitted cell is a contiguous substring run of the page's
   word stream in reading order (no reordering across columns). Already an invariant; now
   *enforced as a gate*, not assumed.
5. **No heading-as-row:** no emitted row contains an all-caps multi-word run that matches
   the structural-boundary lexicon (§C) — that is a section heading, not a row.

### B.2 Conditions that FORCE fallback to `-layout` (any one)

- No schema (§B.1.1 fails) — the common prose-page case.
- `< 2` anchor rows, or clusters `< N`.
- Serials non-monotonic or non-integer (segmentation is wrong).
- Any cell fails the verbatim-substring check.
- A would-be row matches the heading lexicon.

### B.3 Formalizing "a missed table is acceptable; a scramble is not"

```
reflow_page : (words_on_page, layout_text) -> String
  POSTCONDITION:
    result == linearize(rows)        IF  verify(rows) == OK
    result == layout_text            OTHERWISE     (byte-for-byte)
  where verify(rows) == OK  ⇔  B.1.(3) ∧ B.1.(4) ∧ B.1.(5)
```

The defect today is that the gate checks *shape before building* and never re-checks the
*output*. The contract inverts this: **build, then verify the built rows, then decide.**
Corruption becomes structurally impossible — the worst case is a missed table that falls
back to (still-honest) `-layout`, which §D backstops.

### B.4 Cross-page row continuation (the only residual cross-page logic)

A page whose **first body line has an empty serial column** is a continuation of the prior
schedule page's last row: append its cells per-column, then resume. This is a 10-line rule;
it does **not** require carrying column geometry across the break (each page re-anchors on
its own repeated header).

---

## C. STRUCTURE PARSER — generic hierarchy + inheritance reset

Finding 8 is a `structure_parser` problem, independent of tables. A perfect table glued to
`Rule 27(5)` is still uncitable.

### C.1 Hierarchy model

```
Document
 ├─ Chapter            (optional)
 │   └─ Rule / Section          ← provision nodes (existing)
 │       └─ sub-clauses (1),(a),(i)
 └─ Schedule("Second Schedule")          ← NEW structural node
     └─ Part("Part-I")                    ← NEW
         └─ Section("Powers Common to All Departments")   ← NEW (table section)
             └─ Row(serial=1, cells…)     ← table rows (from §A)
 (Annexure / Appendix / Form are siblings of Schedule, same machinery)
```

### C.2 Boundary lexicon (generic, document-CLASS grammar — not per-document keywords)

`SCHEDULE` (`FIRST|SECOND|THIRD|…|nth`), `PART`, `ANNEXURE`, `APPENDIX`, `FORM`,
`CHAPTER`. These are legal-document structural grammar, not content keywords, so they
honor the "no per-document keyword lists" guardrail.

### C.3 Heading vs. reference disambiguation (mandatory)

"Second Schedule" appears constantly in prose ("…in column 2 of the Second Schedule…").
Treat a lexicon hit as a **boundary only when it is a heading line**: short, standalone,
all-caps or title-case-centered, not embedded mid-sentence. Reuse the chrome/geometry
signals already built in US1 (`chrome.rs` margin/line-shape heuristics).

### C.4 Inheritance rules

- **Inherit** the current provision label down into sub-clauses (existing US2 behavior).
- **RESET** inherited provision label to *none* the moment a boundary heading (§C.2) is
  recognized. From there, the active label is the schedule path
  (`Second Schedule › Part-I › <Section>`), not the trailing `Rule`.
- A new top-level provision (`Rule N` / `Section N`) also resets, as today.

### C.5 Preventing future absorption

The same boundary lexicon + heading-disambiguation handles annexures, appendices, forms,
and future schedules uniformly. Add a regression fixture per structural keyword so a new
annexure in any future document cannot silently inherit a preceding rule's label.

---

## D. MANUAL CURATION — YES, as backstop + golden ground-truth

For 10 PDFs, curation is cheap insurance and the only thing that *guarantees* citation
grade on the financial tables on day one.

- **Curate:** the high-value schedule tables that drive drafting — primarily the
  **Delegation of Financial Powers Second Schedule (Parts I–II)**, and any schedule page
  that still fails the §B gate after §A ships. Bounded and finite.
- **Stay automatic:** all prose, all provision/sub-clause extraction, chrome, label
  resolution, and any table page that passes the §B gate.
- **Storage:** a versioned **sidecar JSON keyed by file `sha`** (mirrors the existing
  `ir_cache/<sha>.json` pattern), e.g. `curated/<sha>.schedules.json`, holding
  `{ schedule, part, section, rows:[{serial, nature, delegatee, extent}] }` with verbatim
  cells. At index time, **curated rows override** reconstructed rows for that page; the
  override is logged and the verbatim-substring gate (G1) is still enforced against the
  source text so curation can't smuggle in non-source text. These curated rows double as
  the **golden fixtures** for the §B gate.

Curation is a *backstop*, not the primary path — §A must still aim to reconstruct
automatically so the approach generalizes to the other schedule-bearing docs (Compendium,
APT Rules, SPPRA) without per-document hand-work.

---

## E. PADDLEOCR / PP-STRUCTURE — do NOT introduce now

Reserve strictly for **scanned PDFs, image documents, and photographs.** Justification,
given the established findings:

- **It solves a problem we do not have.** Finding 1 + 6: the corpus is digital with an
  explicit, 55×-repeated textual schema. PP-Structure exists to *recover* structure from
  pixels; here the structure is already in the text stream.
- **It is worse for citation-grade.** Finding 9: ML table-structure detection is
  probabilistic. Replacing a deterministic, verifiable schema reader (§A/§B) with a
  confidence-scored model moves us *away* from "verifiable correctness."
- **It violates deployment constraints in spirit.** Finding 11: an offline Rust desktop app
  would have to bundle an ONNX/Python runtime and hundreds of MB of weights, with
  non-deterministic output — a permanent maintenance and packaging burden for zero benefit
  on this corpus.
- **Escape hatch, documented:** if a *scanned* or *header-less* table ever enters the
  corpus, the path is PP-Structure for region detection + digital-text fill by bbox
  overlap — gated behind the same §B verifier. Not the launch path.

---

## F. ROADMAP — concrete sequence

Principle: **stop corruption → fix attribution → reconstruct correctly → backstop the
residue.** Each phase is independently shippable and independently valuable.

### Phase 1 (Week 1) — Safety contract + visibility  ·  *blocks nothing, unblocks trust*
- **Objective:** make confident corruption structurally impossible.
- **Changes:** implement §B as the `reflow_page` contract (build → verify → emit-or-
  fallback); delete the confident-emit path; add `block_class='table_row'` +
  `reconstruction_method` tagging; add the verbatim + monotonic-serial verifier.
- **Quality gain:** the 121 confident-garble chunks collapse to honest `-layout` fallback.
  No more confidently-wrong table citations.
- **Risk:** low (pure logic; conservative default).
- **Verification:** new golden fixtures from the **real page-8 bbox** (giant wrapped col-3,
  sparse Extent) and a `COLLEGE EDUCATION DEPARTMENT` page that **fail today** and pass
  after; `cargo test --lib parsers::table`; DB check: `reconstruction_method` populated,
  zero rows with the linearizer signature on un-verified pages.

### Phase 2 (Week 2) — Schedule boundaries  ·  *highest pure ROI*
- **Objective:** correct attribution for all schedules/annexures, corpus-wide.
- **Changes:** §C lexicon + heading disambiguation + explicit `Schedule/Part/Section`
  nodes + inheritance reset in `structure_parser`.
- **Quality gain:** schedule chunks cite as "Second Schedule, Part-I, …" not "Rule 27(5)".
- **Risk:** low–medium (the prose-vs-heading call; mitigated by reusing US1 geometry).
- **Verification:** re-index; assert no schedule chunk carries a `Rule N` label; one
  fixture per structural keyword.

### Phase 3 (Weeks 3–4) — Schema-anchored reconstruction  ·  *the real feature*
- **Objective:** citable, verbatim row-units for schedule tables.
- **Changes:** §A — delete `detect_columns`/`detect_continuation`/cross-page carry; add
  `detect_schema` + `body_column_lefts` + serial-led row segmentation + §B.4 continuation.
- **Quality gain:** Delegation Second Schedule rows reconstruct discretely, in order,
  verbatim, with folded column labels.
- **Risk:** medium (anchor-row edge cases) — bounded by the §B gate (failures fall back,
  never scramble).
- **Verification:** §B golden gate over **all 55 schedule pages**; full manual spot-check
  of the Delegation Second Schedule end-to-end; G1 substring audit.

### Phase 4 (Week 5) — Curation backstop + generalization
- **Objective:** guarantee citation-grade on critical financial tables now; widen coverage.
- **Changes:** §D sidecar curation for any page still failing the gate; extend fixtures to
  Compendium / APT Rules / SPPRA schedules.
- **Risk:** low. **Verification:** curated rows pass G1 against source; locked as golden.

### Phase 5 (Week 6) — Hardening & regression lock
- Freeze the §B golden gate (G2) into the pre-re-index checklist; append `CHANGELOG_DEV.md`;
  update the MEMORY topic file. **Explicitly deferred:** recurrence (deleted), PP-Structure
  (scans only).

---

## G. Recommendations from prior reviews that I REJECT

1. **"Improve / header-seed the recurrence detector" (Fix 3).** Rejected — it preserves
   recurrence's entire failure surface (phantom columns, region bounding, voting) for no
   benefit. Delete recurrence; don't decorate it.
2. **"Derive column bands from header/enumerator x-positions."** Rejected — headers are
   centered, the body is left-aligned (proved on page 8: header "Extent"@495 vs body "Full
   Powers"@429); header-derived boundaries splice Extent into the authority column. Use
   body anchor-row left-edges for assignment; use header/enumerator only for `N` + labels.
3. **Confidence check "no dominant single-column occupancy."** Rejected — col-3 ("To whom
   delegated") *legitimately* holds 80–90% of words because it wraps 30–40 lines. This
   check would false-reject every valid Delegation table. Use monotonic-serial + verbatim
   instead.
4. **Cross-page column-signature stitching.** Rejected/deleted — the header repeats on all
   55 pages, so every page self-anchors. Keep only the empty-serial continuation rule
   (§B.4).
5. **Adopt PP-Structure now (Option D).** Rejected for this corpus — see §E.
6. **Continue tuning recurrence thresholds (Option C).** Rejected — sunk-cost trap.
