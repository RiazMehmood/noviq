# US5 Table-Reflow — Why It Isn't Working on the Delegation PDF

**Date:** 2026-06-18
**Branch:** `012-doc-intelligence-production`
**Doc under test:** `09_Sindh_Delegation_of_Financial_Powers_2019.pdf` (91 pages, 704 chunks)
**Engine:** `src-tauri/src/services/parsers/table.rs` (pure core) + `pdf.rs::maybe_reflow_tables` (IO shell)
**Status:** Code shipped + 6 pure unit tests green. On the **live re-index it produces confident garble** on the real schedules. This is a correctness/invariant failure, not a "didn't run" failure.

> This file is written to be handed to other AI tools for a second opinion. It contains the
> exact code, the exact geometry from the real PDF, the DB evidence, the root-cause analysis,
> and three candidate fixes with trade-offs. **No fix has been applied yet** — only diagnosis.

---

## 1. TL;DR

- The reflow engine **does run** and **does fire** on the Delegation PDF — **121 chunks** corpus-wide carry the linearizer's signature (`"… — Extent: …"`). So the earlier hypothesis ("reflow never fired") is **wrong**.
- The real failure: on the actual Sindh schedule geometry the **column detector picks a wrong/incomplete column set**, then **row-grouping mis-assigns the wrapped cells**, and the linearizer emits **confident scrambled rows**. That output is *worse* than just keeping `pdftotext -layout`, and it **violates the project's core invariant** ("a missed table is acceptable; a scramble is not").
- Two distinct broken output shapes are in the DB right now (§4).
- A second, orthogonal bug compounds it: the schedule tables are **mis-attributed to the preceding Rule's label** (`Rule 27(5)`, `Rule 22(v)`) because the heading detector doesn't treat `SECOND SCHEDULE / PART-I / <DEPARTMENT>` as new section boundaries. That is a US2/US3 issue, not US5, but it makes the bad table chunks even harder to cite.

---

## 2. The real table shape (this is the crux)

`pdftotext -layout` of page 8 (the first schedule table, "POWERS COMMON TO ALL DEPARTMENTS"):

```
 Sr. #     Nature of power            To whom delegated                          Extent
  (1)            (2)                         (3)                                   (4)
  1.     Creation of Post        (i)    Administrative             Full Powers
                                 Departments shall have full
                                 powers to create new post (s)
                                 on temporary basis in the
                                 development budget (Public
                                 Sector            Development
                                 Programme), included in PC-I
                                 ...  (≈30 wrapped lines of column 3) ...
                                 scheme.

                                 (ii)  After completion of the
                                 ...  (another ≈12 wrapped lines) ...
                                 Department.
  2.     Reciprocal up-          Administrative Department        Full Powers
         gradation and down-
         gradation of posts
```

**What makes this pathological for the current detector:**

1. **Column 3 ("To whom delegated") wraps over 30–40 lines per row.** The genuine
   data rows (`1.`, `2.`, …) are a *tiny minority* of the lines on the page; the
   overwhelming majority are single-column continuation lines of column 3.
2. **The far-right "Extent" column is sparse** — it has text only on the ~2–4 rows
   that start a data row, never on the wrapped lines.
3. **Wrapped column-3 text contains accidental wide gaps** (`Sector            Development`)
   that look exactly like inter-column gutters.
4. **Section sub-headings sit inside the table band** (`COLLEGE EDUCATION DEPARTMENT`,
   `Note: …`, `PART-I`) and get swept into the "table region".

---

## 3. Exact geometry the engine receives (real `pdftotext -bbox`, page 8)

Median word height `mh ≈ 13.4` ⇒ `col_gap = mh*0.9 ≈ 12.1`, `tol = mh*1.5 ≈ 20.1`,
line-grouping `tol = mh*0.6 ≈ 8`.

Cell-start x-positions per visual line (after `group_lines` + `cell_starts`):

| y     | line content                               | cell-starts (x)              | multi? |
|-------|--------------------------------------------|------------------------------|--------|
| 150.6 | `Sr.#  Nature of power  To whom delegated  Extent` (header) | 84, 139.6, 282, 495.7 | ✓ |
| 164.5 | `(1)  (2)  (3)  (4)` (enumerator)          | 88.6, 176.3, 329.4, 504.9    | excluded |
| 184.5 | `1.  Creation of Post  (i) Administrative  Full Powers` (row 1) | 91, 123.6, 254, 290, 429.7 | ✓ |
| 198.0 | `Departments shall have full` (col-3 wrap) | 254                          | ✗ |
| 211.5 | `powers to create new post (s)` (col-3 wrap)| 254                         | ✗ |
| 238.4 | `development budget (Public` (col-3 wrap)  | 254                          | ✗ |
| 252.0 | `Sector            Development` (col-3 wrap)| 254, ~310                    | ✓ (phantom) |

**Frequency-cluster of all cell-starts from the multi-cell lines** (cluster tol ≈ 20):

```
sorted starts: 84, 91, 123.6, 139.6, 254, 254, 282, 290, 310, 429.7, 495.7
clusters (left, count):
  A (84,   2)   ← Sr.#        (header 84 + row1 91)
  B (123.6,2)   ← Nature      (row1 123.6 + header 139.6)
  C (254,  2)   ← To-whom     (row1 254 + Sector-line 254)
  D (282,  2)   ← header "To"/row1 "Administrative" 290   ← SPLITS col 3
  E (310,  1)   ← phantom from "Sector  Development"
  F (429.7,1)   ← Extent text (Full)   ← DROPPED (count < support)
  G (495.7,1)   ← Extent header        ← DROPPED (count < support)
support = ceil(tabular_lines * 0.34), min 2  →  with ~3 tabular lines, support = 2
```

**Surviving columns: `[84, 123.6, 254, 282]`.**

This is *wrong* in two independent ways:
- **The Extent column (429/495) is dropped** because its left edge recurs on too few
  lines (the support threshold needs ≥34% of tabular lines, but Extent only appears on
  data-row lines, which are the minority).
- **Column 3 is split into two** (254 and 282) by the header word "To"/the row's
  "Administrative", so wrapped col-3 text gets sprayed across two output columns.

Once the columns are wrong, `build_rows` mis-bins every wrapped fragment and `linearize`
emits a confidently mangled row. The "confident-only" gate (`tabular.len() >= 3` and
`cols.len() >= 2`) **passes** here — so there is no fallback to `-layout`.

---

## 4. The defect chunk shapes (current DB state, live)

**Failure mode A — confident garble with mis-folded labels** (`Rule 22(v)`, real query):

```
Rule 22 ¶ Gated Pipe etc.).. ¶ Note: Each site or scheme. ¶ to be considered as an
independent 2. ¶ COLLEGE EDUCATION DEPARTMENT — Extent: work. ¶ 1. ¶ Purchase of. ¶
(i) Administrative Department — Extent: Full Powers. ¶ scientific equipment. ¶ and
apparatus,. ¶ (ii) Director General Colleges. ¶ — …
```
- `COLLEGE EDUCATION DEPARTMENT — Extent: work` — a section heading turned into a "row".
- The real description `Purchase of scientific equipment and apparatus` is shattered
  across separate `¶` fragments instead of folded into one cell.
- `— Extent:` labels are attached to the wrong text.

**Failure mode B — rows collapse to one column, period-per-fragment** (`Rule 27(5)`, page 8):

```
Rule 27 ¶ (5) Assistant or Deputy Superintendents of Police. ¶ 6 SECOND SCHEDULE ¶
(See Rule-3) ¶ PART-I ¶ POWERS COMMON TO ALL DEPARTMENTS ¶ 1. ¶ Creation of Post (i). ¶
Administrative Full Powers. ¶ Departments. ¶ shall have full. ¶ powers. ¶ to create new
post (s). ¶ on. ¶ temporary basis in the. ¶ development. ¶ budget (Public. ¶ Sector. ¶
Development. ¶ Programme), …
```
- `linearize`'s `ncols == 1` branch ran (`end_period` per line) → one fragment per line.
- Whole `SECOND SCHEDULE` block is glued onto `Rule 27(5)` (see §6).

Both are **worse than the raw `-layout` text** would have been, and both violate
G1's spirit (no scramble) even though every *token* is technically a verbatim substring.

---

## 5. The code that is currently shipped (Wave C)

### 5.1 Confidence gate — `detect_page_table` (the part that is too weak)

```rust
// src-tauri/src/services/parsers/table.rs:124
fn detect_page_table(words: &[&WordBox]) -> Option<PageTable> {
    if words.len() < 6 { return None; }
    let mh = median_height(words);
    let col_gap = mh * 0.9;
    let tol = mh * 1.5;
    let lines = group_lines(words);
    let cols = detect_columns(&lines, col_gap, tol)?;   // ← can return a WRONG set
    let multi: Vec<usize> = lines.iter().enumerate()
        .filter(|(_, l)| cell_starts(l, col_gap).len() >= 2)
        .map(|(i, _)| i).collect();
    let first = *multi.first()?;
    let last  = *multi.last()?;                          // ← region spans sub-headings & prose
    let mut rows = build_rows(&lines[first..=last], &cols, tol);
    rows.retain(|r| !is_enumerator_row(r));
    if rows.is_empty() { return None; }
    let header = take_header(&mut rows);
    if rows.is_empty() { return None; }
    /* … */
}
```

### 5.2 Column detector — `detect_columns` (frequency clustering; the root weakness)

```rust
// src-tauri/src/services/parsers/table.rs:360
fn detect_columns(lines: &[Vec<&WordBox>], col_gap: f32, tol: f32) -> Option<Vec<f32>> {
    let tabular: Vec<&Vec<&WordBox>> = lines.iter()
        .filter(|l| cell_starts(l, col_gap).len() >= 2 && !line_is_enumerator(l))
        .collect();
    if tabular.len() < 3 { return None; }
    let mut starts: Vec<f32> = tabular.iter()
        .flat_map(|l| cell_starts(l, col_gap)).collect();
    starts.sort_by(|a, b| a.total_cmp(b));
    // cluster within tol, keep left edge + support count
    let mut clusters: Vec<(f32, usize)> = Vec::new();
    for s in starts {
        match clusters.last_mut() {
            Some((left, count)) if (s - *left).abs() <= tol => { *count += 1; }
            _ => clusters.push((s, 1)),
        }
    }
    let support = ((tabular.len() as f32) * 0.34).ceil() as usize;
    let support = support.max(2);
    let cols: Vec<f32> = clusters.into_iter()
        .filter(|(_, count)| *count >= support)        // ← drops the sparse Extent column
        .map(|(left, _)| left).collect();
    if cols.len() >= 2 { Some(cols) } else { None }
}
```

The commit history shows this was already iterated twice on real data:
- `a28df871 fix(012): US5 — drop "(1)(2)(3)(4)" enumerator sub-header`
- `5a7e77f4 fix(012): US5 — frequency-clustered column detection (inverted-gap tables)`

The frequency-clustering fix solved the *inverted-gap* problem (an in-cell indent wider
than a real gutter) but introduced the **sparse-column drop** failure: it assumes column
left-edges recur on a healthy fraction of lines, which is false when one cell wraps over
dozens of lines and other columns are sparse.

### 5.3 Why the unit test passes but the real page fails

`table.rs::tests::real_delegation_page8_reconstructs_first_row` uses **synthetic rows
2/3/4 with recurring left-edges at x=84/123/254/429** specifically so the frequency
clusters lock. The **real** page has row 1's column-3 wrapping over 30 lines and rows
2+ pushed far down, so those recurring edges don't exist in the proportion the test
assumes. **The fixture does not reproduce the real line-count distribution.**

---

## 6. Compounding bug (orthogonal, US2/US3): schedule mis-attribution

The schedule content is being stored under the **previous Rule's** `unit_label`:
- `Creation of Post` table → `Rule 27(5)`
- `COLLEGE EDUCATION DEPARTMENT` table → `Rule 22(v)`

Cause: the heading/structure detector does not recognize
`SECOND SCHEDULE` → `PART-I` → `POWERS COMMON TO ALL DEPARTMENTS` /
`COLLEGE EDUCATION DEPARTMENT` as **new structural boundaries**, so the table text is
coalesced into the trailing sub-clause of the preceding Rule. Even a perfect table
reflow would still be mislabeled. This needs a schedule-boundary rule in
`structure_parser` independent of the table engine.

---

## 7. Root cause (one paragraph)

The column model is **geometry-by-recurrence**: it infers columns from cell-start
x-positions that repeat across "tabular" lines. Real Sindh schedules break every
assumption of that model — a single cell ("To whom delegated") wraps over tens of lines,
so (a) genuine multi-column data rows are a small minority of page lines, (b) sparse
columns (Sr#, Extent) fail the 34% support threshold and get dropped, (c) accidental
wide gaps inside wrapped prose manufacture phantom columns, and (d) embedded sub-headings
and notes fall inside the detected region. The "confident-only" gate is not actually
measuring confidence — it only checks "≥3 multi-cell lines and ≥2 surviving columns,"
both of which a pathological page satisfies while still being mis-segmented. So instead
of falling back to `-layout` (the contract), the engine emits a confident scramble.

---

## 8. Candidate fixes (for discussion — not yet applied)

### Fix 1 — Detect columns from the HEADER row, not from recurrence (recommended)
Real schedules have a stable, explicit header (`Sr. # | Nature of power | To whom
delegated | Extent`) and an enumerator row (`(1) (2) (3) (4)`) directly under it. Use
the **header + enumerator** lines to fix the column count and the column x-anchors, then
assign every subsequent word to the nearest header-anchored column. This removes the
dependence on data-row recurrence entirely and naturally keeps the sparse Extent column.
- *Pro:* matches how these documents are actually authored; the `(1)(2)(3)(4)` row is a
  reliable column-count oracle (we already detect & drop it).
- *Pro:* fixes both the dropped-Extent and the split-col-3 failures.
- *Con:* needs a header/enumerator detector; pages without a header on them (pure
  continuation pages) must inherit anchors from the anchor page (the stitch loop already
  carries `head.cols` — feed header anchors through the same path).

### Fix 2 — Strengthen the confident-only gate so bad pages fall back (safety net)
Add real confidence checks before accepting a reflow, else return `None` (keep `-layout`):
- require the detected `cols.len()` to equal the enumerator-row count when present;
- require ≥N (e.g. 2) data rows whose **first** column starts a serial (`1.`, `2.`…);
- reject if >X% of region lines land in a single column (the "one giant wrapped cell"
  signature) — that's the tell that columns were mis-detected;
- reject if a detected "row" contains an all-caps heading token run (`COLLEGE EDUCATION
  DEPARTMENT`) → it's a sub-heading, not a row.
- *Pro:* directly enforces the "a scramble is not acceptable" invariant; cheap.
- *Con:* on its own it only *prevents garble*, it doesn't *reconstruct* the table
  (acceptable per contract, but loses the data). Best paired with Fix 1.

### Fix 3 — Right-anchor the sparse columns
Keep frequency clustering for the dense left columns, but additionally seed a column at
any x-band that appears in the **header** even with low data-row support (so Extent
survives). Essentially a targeted patch of the support filter for header-confirmed bands.
- *Pro:* smallest diff over current code.
- *Con:* still recurrence-based; won't fix col-3 splitting or sub-headings-in-region;
  partial.

**Suggested path:** Fix 2 first (stop emitting garble — restores the invariant
immediately, low risk), then Fix 1 (reconstruct correctly using the header/enumerator as
the column oracle). Track schedule mis-attribution (§6) as a separate `structure_parser`
task.

### New fixtures required (current tests are too forgiving)
- A fixture built from the **verbatim page-8 bbox** in §3 (giant wrapped col-3, sparse
  Extent, 2 data rows) — must either reconstruct correctly (Fix 1) or fall back to
  `-layout` (Fix 2). It must **fail** against today's code.
- A `COLLEGE EDUCATION DEPARTMENT` fixture (sub-heading inside the band) — must not
  become a row.

---

## 9. How to reproduce / verify

```bash
# layout + bbox of the live file
PDF="/home/riaz/Desktop/Sindh Govt Documents/09_Sindh_Delegation_of_Financial_Powers_2019.pdf"
pdftotext -layout "$PDF" /tmp/deleg_layout.txt   # page 8 table starts ~line 261
pdftotext -bbox   "$PDF" /tmp/deleg_bbox.html    # "Creation" word = page 8

# live DB evidence (bundled PG; creds in pg/.pgpass)
PSQL=/home/riaz/.local/share/com.echoai.app/pg/install/17.10.0/bin/psql
PGPASSWORD=$(cat /home/riaz/.local/share/com.echoai.app/pg/.pgpass) \
"$PSQL" -h 127.0.0.1 -p 33121 -U postgres -d echo -c \
"SELECT unit_label, left(content,400) FROM chunks
 WHERE source_id='7a7b2509-8d5e-4f84-bafd-8600cb2efab5'
   AND content ILIKE '%Creation of Post%' LIMIT 1;"

# count reflow-touched chunks corpus-wide (linearizer signature)
... -c "SELECT count(*) FROM chunks WHERE content LIKE '%— %'
        AND (content LIKE '%Extent:%' OR content LIKE '%Authority:%');"   -- 121
```

Toggle: `ECHO_TABLE_REFLOW=0` disables reflow entirely (proves the fallback path).
