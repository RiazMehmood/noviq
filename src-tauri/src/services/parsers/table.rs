//! US8/R1 — tabular reconstruction (FR-013).
//!
//! `pdftotext -layout` keeps prose readable but scrambles multi-column tables:
//! it reads strictly left-to-right per visual line, so a wrapped cell interleaves
//! its row with the next. This module takes per-word bounding boxes (from
//! `pdftotext -bbox`, parsed in the `pdf.rs` IO shell) and, for pages that
//! contain a detected table, rebuilds each row as ONE self-describing unit with
//! the column headers folded in — so each row is a coherent, citable provision.
//!
//! Design (roadmap 2026-06-17): pure core (no IO, fully unit-tested); prose
//! regions and prose-only pages are left exactly as `-layout` produced them;
//! cell text is verbatim and no cell is ever dropped. Confident-only: any
//! ambiguous/ragged region falls back to the original `-layout` text — a missed
//! table is acceptable, a scrambled one is not.

/// One word with its bounding box, in pdftotext page coordinates (origin
/// top-left; `x`/`y` are the top-left corner, `w`/`h` the box size). `page` is
/// 1-based, matching the form-feed (`\x0c`) page split of the `-layout` text.
#[derive(Debug, Clone)]
pub struct WordBox {
    pub text: String,
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
    pub page: u32,
}

/// A confidently-detected table on one page (US5 v2, schema-anchored).
struct PageTable {
    /// Prose lines before the table region (reading order, kept verbatim).
    pre: Vec<String>,
    /// Data rows as raw cells (`ncols` wide); wrapped continuations already folded.
    rows: Vec<Vec<String>>,
    /// Prose lines after the table region.
    post: Vec<String>,
    /// Column labels from the document's declared header (folded into each row).
    labels: Vec<String>,
}

/// The schema a Sindh schedule declares on every page: the header row plus the
/// `(1)(2)(3)(4)` enumerator row beneath it (FINAL_DECISIONS §A). The enumerator
/// pins the column COUNT; the header supplies the column LABELS.
struct PageSchema {
    ncols: usize,
    labels: Vec<String>,
    header_idx: usize,
    enum_idx: usize,
}

/// Reflow detected table regions in `layout_text` using word geometry.
///
/// `layout_text` is the `pdftotext -layout` output (pages separated by `\x0c`).
/// Each page is examined independently for a table whose schema it declares (a
/// header row + `(1)(2)(3)(4)` enumerator); Sindh schedules re-declare that
/// schema on every page, so each page self-anchors (no cross-page column carry).
/// Pages with no declared schema keep their original `-layout` text
/// byte-for-byte. Confident-only: any region failing [`verify_table`] falls back
/// to `-layout` (a missed table is acceptable; a scramble is not).
pub fn reflow_tables(words: &[WordBox], layout_text: &str) -> String {
    reflow_tables_with_rows(words, layout_text).0
}

/// Same as [`reflow_tables`] but also returns every reconstructed row line that
/// was emitted (verbatim, post-`linearize`). The caller uses these for
/// provenance — marking the resulting chunks `block_class='table_row'` —
/// without re-detecting structure downstream (US5 T033).
pub fn reflow_tables_with_rows(words: &[WordBox], layout_text: &str) -> (String, Vec<String>) {
    let pages: Vec<&str> = layout_text.split('\x0c').collect();
    // Default: every page keeps its original layout text.
    let mut out: Vec<String> = pages.iter().map(|p| (*p).to_string()).collect();
    let mut emitted_rows: Vec<String> = Vec::new();

    for (i, _) in pages.iter().enumerate() {
        let pno = (i + 1) as u32;
        let page_words: Vec<&WordBox> = words.iter().filter(|w| w.page == pno).collect();
        let Some(pt) = detect_page_table(&page_words) else {
            continue;
        };
        // SAFETY CONTRACT (FINAL_DECISIONS §B): emit ONLY when the built rows are
        // verifiable; otherwise keep this page's `-layout` text byte-for-byte.
        if !verify_table(&pt.rows) {
            continue;
        }
        let row_lines = linearize(&pt.rows, Some(&pt.labels));
        if row_lines.is_empty() {
            continue;
        }
        emitted_rows.extend(row_lines.iter().cloned());

        let mut page: Vec<String> = Vec::new();
        page.extend(pt.pre.clone());
        page.extend(row_lines);
        page.extend(pt.post.clone());
        out[i] = page.join("\n");
    }
    (out.join("\x0c"), emitted_rows)
}

/// Detect a confidently-table page (US5 v2, schema-anchored). The page must
/// declare a schema (header + `(1)(2)(3)(4)` enumerator); columns are then read
/// from the body's serial-led rows (left-aligned), NOT inferred statistically.
/// `None` → caller keeps `-layout` text (a missed table is acceptable).
fn detect_page_table(words: &[&WordBox]) -> Option<PageTable> {
    if words.len() < 6 {
        return None;
    }
    let mh = median_height(words);
    let col_gap = mh * 0.9; // a gutter is wider than a line is tall; a space is not
    let tol = mh * 1.5;

    let lines = group_lines(words);
    let schema = detect_schema(&lines, col_gap)?;

    // Body = everything after the enumerator row. Anchor rows are the serial-led
    // lines; their left-aligned cell-starts define the columns (FINAL §A.4).
    let body = &lines[schema.enum_idx + 1..];
    let anchor_rows: Vec<&Vec<&WordBox>> = body
        .iter()
        .filter(|l| l.first().is_some_and(|w| is_serial(&w.text)))
        .collect();
    let cols = body_column_lefts(&anchor_rows, col_gap, schema.ncols, tol)?;

    // Bound the region: from the first serial row, include serial-led rows and
    // their wrapped continuations (empty leading column); STOP at the first
    // non-serial leading-column line (a Note / heading / prose → table ended).
    let start = body
        .iter()
        .position(|l| l.first().is_some_and(|w| is_serial(&w.text)))?;
    let mut end = start;
    for (idx, l) in body.iter().enumerate().skip(start) {
        if l.is_empty() {
            continue;
        }
        let first_serial = is_serial(&l[0].text);
        let wrapped = col_index(l[0].x, &cols, tol) > 0; // leading col empty → continuation
        if first_serial || wrapped {
            end = idx;
        } else {
            break;
        }
    }

    let mut rows = build_rows(&body[start..=end], &cols, tol);
    rows.retain(|r| !is_enumerator_row(r));
    if rows.is_empty() {
        return None;
    }

    let pre: Vec<String> = lines[..schema.header_idx].iter().map(|l| join_line(l)).collect();
    let post: Vec<String> = body[end + 1..].iter().map(|l| join_line(l)).collect();
    Some(PageTable { pre, rows, post, labels: schema.labels })
}

/// Find the declared schema on a page: an enumerator line `(1)(2)…(N)` whose
/// immediately-preceding line is a header with the SAME number of cells. Returns
/// the column count + labels. `None` → no declared schema (caller keeps layout).
fn detect_schema(lines: &[Vec<&WordBox>], col_gap: f32) -> Option<PageSchema> {
    for i in 1..lines.len() {
        if !line_is_enumerator(&lines[i]) {
            continue;
        }
        let ncols = lines[i].len();
        if ncols < 2 {
            continue;
        }
        let header_cells = group_cells(&lines[i - 1], col_gap);
        if header_cells.len() == ncols {
            return Some(PageSchema {
                ncols,
                labels: header_cells.iter().map(|c| join_line(c)).collect(),
                header_idx: i - 1,
                enum_idx: i,
            });
        }
    }
    None
}

/// Split a visual line into cells at gaps wider than `col_gap` (returns the words
/// per cell, in order). Used to read the header's column labels.
fn group_cells<'a>(line: &[&'a WordBox], col_gap: f32) -> Vec<Vec<&'a WordBox>> {
    let mut cells: Vec<Vec<&WordBox>> = Vec::new();
    for (k, w) in line.iter().enumerate() {
        if k == 0 {
            cells.push(vec![*w]);
            continue;
        }
        let prev = line[k - 1];
        let gap = w.x - (prev.x + prev.w);
        if gap > col_gap {
            cells.push(vec![*w]);
        } else if let Some(cur) = cells.last_mut() {
            cur.push(*w);
        }
    }
    cells
}

/// Column left-edges from the page's serial-led DATA rows, reduced to exactly
/// `ncols` (FINAL §A.4). Body cells are LEFT-ALIGNED, so a column's left edge is
/// the minimum cell-start in its cluster; `ncols` is fixed by the enumerator, so
/// we keep the `ncols` strongest clusters (NO fractional support threshold — that
/// was the recurrence bug that dropped the sparse right column). Wrapped lines do
/// not vote. Requires ≥2 anchor rows, else `None` (conservative).
fn body_column_lefts(
    anchor_rows: &[&Vec<&WordBox>],
    col_gap: f32,
    ncols: usize,
    tol: f32,
) -> Option<Vec<f32>> {
    if anchor_rows.len() < 2 {
        return None;
    }
    let mut starts: Vec<f32> = anchor_rows
        .iter()
        .flat_map(|l| cell_starts(l, col_gap))
        .collect();
    starts.sort_by(|a, b| a.total_cmp(b));
    let mut clusters: Vec<(f32 /*left*/, usize /*support*/)> = Vec::new();
    for s in starts {
        match clusters.last_mut() {
            Some((left, count)) if (s - *left).abs() <= tol => *count += 1,
            _ => clusters.push((s, 1)),
        }
    }
    if clusters.len() < ncols {
        return None;
    }
    // Keep the ncols best-supported clusters; ties broken by leftmost x.
    clusters.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.total_cmp(&b.0)));
    let mut cols: Vec<f32> = clusters.into_iter().take(ncols).map(|(left, _)| left).collect();
    cols.sort_by(|a, b| a.total_cmp(b));
    Some(cols)
}

/// A short bare integer serial at the start of a row ("1.", "12") — the column-0
/// key of a Sindh schedule data row.
fn is_serial(s: &str) -> bool {
    let t = s.trim().trim_end_matches('.').trim();
    !t.is_empty() && t.chars().count() <= 3 && t.chars().all(|c| c.is_ascii_digit())
}

/// Build logical rows from `region` lines under `cols`: a line whose first
/// column is non-empty starts a row; a line whose first column is empty is a
/// wrapped continuation appended per-column to the current row.
fn build_rows(region: &[Vec<&WordBox>], cols: &[f32], tol: f32) -> Vec<Vec<String>> {
    let ncols = cols.len();
    let mut rows: Vec<Vec<String>> = Vec::new();
    for line in region {
        let mut cells = vec![String::new(); ncols];
        for w in line {
            let j = col_index(w.x, cols, tol);
            if !cells[j].is_empty() {
                cells[j].push(' ');
            }
            cells[j].push_str(&w.text);
        }
        // Skip a pure "(1)(2)(3)(4)" enumerator sub-header line outright, so a
        // following wrapped line cannot be absorbed into it.
        if is_enumerator_row(&cells) {
            continue;
        }
        if cells[0].trim().is_empty() {
            if let Some(cur) = rows.last_mut() {
                for j in 0..ncols {
                    let add = cells[j].trim();
                    if !add.is_empty() {
                        if !cur[j].is_empty() {
                            cur[j].push(' ');
                        }
                        cur[j].push_str(add);
                    }
                }
                continue;
            }
        }
        rows.push(cells);
    }
    rows
}

/// A pure column-enumerator token: a parenthesised/bare small number or roman
/// numeral ("(1)", "2", "(iv)") — the "(1)(2)(3)(4)" sub-header beneath the
/// real column titles in Sindh schedules.
fn is_enumerator(s: &str) -> bool {
    let core: String = s.chars().filter(|c| c.is_alphanumeric()).collect();
    if core.is_empty() || core.chars().count() > 3 {
        return false;
    }
    let lc = core.to_lowercase();
    lc.chars().all(|c| c.is_ascii_digit()) || lc.chars().all(|c| "ivxl".contains(c))
}

/// A row that is ONLY column enumerators (every non-empty cell is `is_enumerator`)
/// — a "(1) (2) (3) (4)" sub-header, never a data row. Dropped before linearize.
fn is_enumerator_row(row: &[String]) -> bool {
    let non_empty: Vec<&str> = row.iter().map(|s| s.trim()).filter(|s| !s.is_empty()).collect();
    non_empty.len() >= 2 && non_empty.iter().all(|c| is_enumerator(c))
}

/// A visual line that is ONLY column enumerators (`(1) (2) (3) (4)`) — excluded
/// from column detection (its centered positions don't match the data columns).
fn line_is_enumerator(line: &[&WordBox]) -> bool {
    line.len() >= 2 && line.iter().all(|w| is_enumerator(&w.text))
}

fn median_height(words: &[&WordBox]) -> f32 {
    let mut hs: Vec<f32> = words.iter().map(|w| w.h).filter(|h| *h > 0.0).collect();
    if hs.is_empty() {
        return 12.0;
    }
    hs.sort_by(|a, b| a.total_cmp(b));
    hs[hs.len() / 2]
}

/// Group words into visual lines by y proximity; each line sorted left→right.
fn group_lines<'a>(words: &[&'a WordBox]) -> Vec<Vec<&'a WordBox>> {
    if words.is_empty() {
        return Vec::new();
    }
    let mut sorted: Vec<&WordBox> = words.to_vec();
    sorted.sort_by(|a, b| a.y.total_cmp(&b.y).then(a.x.total_cmp(&b.x)));
    let tol = median_height(words) * 0.6;
    let mut lines: Vec<Vec<&WordBox>> = Vec::new();
    let mut cur: Vec<&WordBox> = Vec::new();
    let mut base_y = sorted[0].y;
    for w in sorted {
        if cur.is_empty() {
            base_y = w.y;
            cur.push(w);
        } else if (w.y - base_y).abs() <= tol {
            cur.push(w);
        } else {
            cur.sort_by(|a, b| a.x.total_cmp(&b.x));
            lines.push(std::mem::take(&mut cur));
            base_y = w.y;
            cur.push(w);
        }
    }
    if !cur.is_empty() {
        cur.sort_by(|a, b| a.x.total_cmp(&b.x));
        lines.push(cur);
    }
    lines
}

/// x-start of each cell on a line (cells split where the inter-word gap exceeds
/// `col_gap`).
fn cell_starts(line: &[&WordBox], col_gap: f32) -> Vec<f32> {
    let mut starts = Vec::new();
    if line.is_empty() {
        return starts;
    }
    starts.push(line[0].x);
    for pair in line.windows(2) {
        let gap = pair[1].x - (pair[0].x + pair[0].w);
        if gap > col_gap {
            starts.push(pair[1].x);
        }
    }
    starts
}

/// Column index for an x position: the greatest column start ≤ x (+ slack).
/// Correct for left-aligned columns even when a cell's text is wide.
fn col_index(x: f32, cols: &[f32], tol: f32) -> usize {
    let mut idx = 0;
    for (j, &c) in cols.iter().enumerate() {
        if x + tol >= c {
            idx = j;
        } else {
            break;
        }
    }
    idx
}

fn join_line(line: &[&WordBox]) -> String {
    line.iter()
        .map(|w| w.text.as_str())
        .collect::<Vec<_>>()
        .join(" ")
}

fn end_period(s: &str) -> String {
    let s = s.trim();
    if s.ends_with(|c: char| c == '.' || c == ':' || c == ';') {
        s.to_string()
    } else {
        format!("{s}.")
    }
}

/// The serial value at the start of a cell ("1." → 1, "12" → 12), if it is a
/// short bare integer. `None` for anything else (text, markers, empty).
fn parse_serial(s: &str) -> Option<u32> {
    let t = s.trim().trim_end_matches('.').trim();
    if t.is_empty() || t.len() > 3 || !t.chars().all(|c| c.is_ascii_digit()) {
        return None;
    }
    t.parse().ok()
}

/// True when a cell text contains a run of ≥2 consecutive ALL-CAPS alphabetic
/// words (≥4 letters each) — a section heading swept into the table band
/// (`COLLEGE EDUCATION DEPARTMENT`), never a data cell (body cells are Title
/// Case). Used to refuse a region that absorbed a heading. FINAL_DECISIONS §B/§C.
fn contains_heading_run(s: &str) -> bool {
    let mut run = 0;
    for word in s.split_whitespace() {
        let alpha: String = word.chars().filter(|c| c.is_alphabetic()).collect();
        let is_caps = alpha.chars().count() >= 4 && alpha.chars().all(|c| c.is_uppercase());
        if is_caps {
            run += 1;
            if run >= 2 {
                return true;
            }
        } else {
            run = 0;
        }
    }
    false
}

/// Decidable correctness gate for a reconstructed table (FINAL_DECISIONS §B.1):
/// emit ONLY if (a) no cell absorbed a section heading, and (b) the serial column
/// is a strictly-increasing integer run of ≥2 rows. Otherwise the segmentation is
/// not trustworthy → caller falls back to `-layout`.
fn verify_table(rows: &[Vec<String>]) -> bool {
    if rows.iter().any(|r| r.iter().any(|c| contains_heading_run(c))) {
        return false;
    }
    let serials: Vec<u32> = rows.iter().filter_map(|r| parse_serial(&r[0])).collect();
    serials.len() >= 2 && serials.windows(2).all(|w| w[1] > w[0])
}

// ─── Phase 9a: external sidecar (pdfplumber) reflow ──────────────────────────

/// One table from the external ingestion sidecar (pdfplumber): column header
/// labels + data rows as raw verbatim cells. Pure data (no IO/serde) so the
/// formatting — and the golden-gated row format — stays colocated with the
/// projection engine.
#[derive(Debug, Clone)]
pub struct SidecarTable {
    pub header: Vec<String>,
    pub rows: Vec<Vec<String>>,
}

/// One page's sidecar extraction: prose above/below the table band + its tables.
#[derive(Debug, Clone)]
pub struct SidecarPage {
    pub page: u32,
    pub above: String,
    pub below: String,
    pub tables: Vec<SidecarTable>,
}

/// One body row of a detected table, with the page it was printed on.
///
/// The page travels WITH the row because a table is stitched across page breaks
/// ([`stitch_continuations`]): the logical table starts on p.26 but row 39 is
/// printed on p.34, and a citation has to open the document where the row
/// actually is. Deriving a row's page from its table's would silently move every
/// continuation row back to the first page.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TableRow {
    pub cells: Vec<String>,
    pub page: u32,
}

/// Feature 023 — the authoritative structure of one detected table, carried from
/// reflow through to unit creation (where it becomes a whole-table chunk + one
/// chunk per row). `id` matches the in-band `⟦TBL:{id}⟧…⟦/TBL:{id}⟧` region
/// markers `reflow_with_sidecar` writes into the text, so the structuring stage
/// can treat the region as atomic and pull the structured cells for that table.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExtractedTable {
    pub id: usize,
    /// Page the table STARTS on; individual rows carry their own page.
    pub page: u32,
    /// The table's own name when it prints one ("SCHEDULE-I (See Rule – 3 (i)").
    /// pdfplumber hands that caption band over as the header cell, which is how
    /// it ended up labelling every row's first column; captured here instead so
    /// the unit can be cited by name rather than as "Table (p.26)".
    pub caption: Option<String>,
    /// Column labels (may be empty for a header-less table).
    pub header: Vec<String>,
    /// Body rows as verbatim cells, AFTER the same normalisation the linearised
    /// output uses (enumerator row dropped, single-cell collapse, serial fix).
    pub rows: Vec<TableRow>,
}

/// Open/close sentinels wrapping a table's linearised rows in the reflowed text.
/// Unusual delimiters (`⟦ ⟧`) so no heading/clause regex matches them; each sits
/// on its own line. Stripped from stored content via [`strip_table_markers`].
pub fn table_open_marker(id: usize) -> String { format!("\u{27E6}TBL:{id}\u{27E7}") }
pub fn table_close_marker(id: usize) -> String { format!("\u{27E6}/TBL:{id}\u{27E7}") }

/// Remove every `⟦TBL:n⟧` / `⟦/TBL:n⟧` marker line from `s` (the whole marker line,
/// including its trailing newline). Round-trips reflow output back to marker-free
/// text (contract C-4: markers never appear in stored content).
pub fn strip_table_markers(s: &str) -> String {
    s.lines()
        .filter(|l| !is_table_marker_line(l))
        .collect::<Vec<_>>()
        .join("\n")
}

/// True when a line is a bare open/close table marker (ignoring surrounding space).
pub fn is_table_marker_line(line: &str) -> bool {
    let t = line.trim();
    (t.starts_with("\u{27E6}TBL:") || t.starts_with("\u{27E6}/TBL:")) && t.ends_with('\u{27E7}')
}

/// Parse a table marker line → `(id, is_open)`; `None` when not a marker.
pub fn parse_table_marker(line: &str) -> Option<(usize, bool)> {
    let t = line.trim();
    let inner = t.strip_prefix('\u{27E6}')?.strip_suffix('\u{27E7}')?;
    let (is_open, num) = match inner.strip_prefix("/TBL:") {
        Some(n) => (false, n),
        None => (true, inner.strip_prefix("TBL:")?),
    };
    num.parse::<usize>().ok().map(|id| (id, is_open))
}

/// Reflow `layout_text` using externally-extracted (pdfplumber) tables instead
/// of the geometric projection engine. For each page the sidecar reported, the
/// page becomes `above-prose + linearized-rows + below-prose`; cells are joined
/// through the SAME [`linearize`] the projection path uses, so the emitted row
/// format (golden-gated) is identical regardless of extractor. Pages the sidecar
/// did not report keep their `-layout` text byte-for-byte.
///
/// Returns the reflowed text, every emitted row line (for `table_row` tagging
/// downstream), and — Feature 023 — the structured [`ExtractedTable`] list. Each
/// table's linearised rows are wrapped with `⟦TBL:{id}⟧…⟦/TBL:{id}⟧` markers in
/// the text so the structuring stage can treat the region as atomic; the same
/// `id` indexes into the returned `Vec<ExtractedTable>`. `emitted` excludes the
/// marker lines (unchanged contract for `table_row` tagging).
pub fn reflow_with_sidecar(
    layout_text: &str,
    pages: &[SidecarPage],
) -> (String, Vec<String>, Vec<ExtractedTable>) {
    let mut out: Vec<String> = layout_text.split('\x0c').map(|p| p.to_string()).collect();
    let mut emitted: Vec<String> = Vec::new();
    let mut pending: Vec<PendingTable> = Vec::new();
    let mut next_id: usize = 0;

    for sp in pages {
        let idx = match (sp.page as usize).checked_sub(1) {
            Some(i) if i < out.len() => i,
            _ => continue, // sidecar page out of range of the -layout pages
        };
        let mut row_lines: Vec<String> = Vec::new();
        for t in &sp.tables {
            // What pdfplumber calls the header is only sometimes one; classify it
            // before rendering, so a caption band or a stray data row never
            // becomes this table's column labels.
            let (header, caption, rows) = classify_table(t);
            let lin = linearize(&rows, header.as_ref());
            if lin.is_empty() || rows.is_empty() {
                continue; // no usable rows → no table unit (data-model edge case)
            }
            // A false positive here DELETES provisions (this function replaces
            // the page's text), so a region that is really prose is left alone.
            if looks_like_prose_not_table(&rows, header.as_ref(), &lin) {
                log::info!(
                    "table reflow: p.{} region rejected as prose ({} row(s), header {} chars, \
                     rows {} chars, single column) — page text kept so its provisions stay detectable",
                    sp.page,
                    rows.len(),
                    header.as_ref().map(|h| h.iter().map(|c| c.chars().count()).sum::<usize>()).unwrap_or(0),
                    rows.iter().map(|r| r.iter().map(|c| c.chars().count()).sum::<usize>()).sum::<usize>()
                );
                continue;
            }
            let id = next_id;
            next_id += 1;
            // Marker-wrap this table's rows in the text; `emitted` (table_row
            // tagging) keeps only the row lines, no markers.
            row_lines.push(table_open_marker(id));
            row_lines.extend(lin.iter().cloned());
            row_lines.push(table_close_marker(id));
            emitted.extend(lin);
            pending.push(PendingTable { id, page: sp.page, header, caption, rows });
        }
        if row_lines.is_empty() {
            continue;
        }

        let mut page: Vec<String> = Vec::new();
        if !sp.above.trim().is_empty() {
            page.push(sp.above.trim_end().to_string());
        }
        page.extend(row_lines);
        if !sp.below.trim().is_empty() {
            page.push(sp.below.trim_start().to_string());
        }
        // Leading + trailing newline so a marker can never share a line with the
        // form feed that joins the pages. `out.join("\x0c")` inserts no newline of
        // its own: without this, a table starting at the top of a page produced
        // `…last line of p.82\x0c⟦TBL:37⟧`, which `is_table_marker_line` does not
        // recognise — the region never opened, and every linearised row in it
        // stayed in the body to be minted as a provision. Measured on the live
        // corpus: 111 duplicate `clause` chunks in Delegation-of-Financial-Powers
        // and a table row stored as APT `Rule 3`. The closing marker fails the
        // same way, which swallows the NEXT page's prose whole.
        out[idx] = format!("\n{}\n", page.join("\n"));
    }

    let tables = stitch_continuations(pending);
    (out.join("\x0c"), emitted, tables)
}

/// A table as first read off one page, before continuation pages are stitched on.
struct PendingTable {
    id: usize,
    page: u32,
    header: Option<Vec<String>>,
    caption: Option<String>,
    rows: Vec<Vec<String>>,
}

/// Longest a cell can be and still be a column LABEL. The corpus's longest real
/// label is "Basic Pay Scale of Officer/Officials" (36); the shortest swallowed
/// paragraph measured was 113.
const HEADER_MAX_CELL: usize = 60;

/// Longest a single populated header cell can be and still be a table's printed
/// NAME ("SCHEDULE-I (See Rule – 3 (i)", "THE SINDH CIVIL SERVANTS (APPOINTMENT,
/// PROMOTION AND TRANSFER) RULES, 1974"). Longer than this it is prose.
const CAPTION_MAX: usize = 140;

/// Words that mark a caption band rather than a data cell.
const CAPTION_WORDS: [&str; 7] =
    ["schedule", "part-", "part ", "annex", "appendix", "table", "form "];

/// Are these cells a column-HEADER row?
///
/// Every test here was bought with a defect measured on the corpus, because the
/// cost of a false positive is high: a data row accepted as a header both fuses
/// the columns under it and stops the table stitching to the page before.
///   * 2+ populated, all label-sized — the baseline shape.
///   * the FIRST cell is named. A header names every column it has, starting with
///     the serial one ("S. NO.", "Sr. #"); what arrives with a blank first cell is
///     a wrapped continuation of the previous page's last row (RoB p.27, p.30:
///     `| | | (ii) Printing & Stationary Deptt: | Stationary Printing Deptt: |`).
///   * no figures anywhere. Real labels across the corpus are digit-free, while a
///     data row leads with its serial — RoB p.29's "header" was
///     `11. | Food Department | Directorate of Food | Director Food`, an entire
///     department demoted to a column label.
/// A header wrongly refused here is not lost: it goes back into the table as a
/// row and the table still stitches.
fn looks_like_header_row(cells: &[String]) -> bool {
    let populated: Vec<&str> =
        cells.iter().map(|c| c.trim()).filter(|c| !c.is_empty()).collect();
    let first = cells.first().map(|c| c.trim()).unwrap_or("");
    populated.len() >= 2
        && !first.is_empty()
        // A roman enumerator gets past the digit test — Delegation p.15 offered
        // `(xviii) | Purchase of Books, Maps and Teaching Materials | …` as its
        // header. A column label is never parenthesised; an enumerated data row
        // always is.
        && !first.starts_with('(')
        && populated.iter().all(|c| c.chars().count() <= HEADER_MAX_CELL)
        && !cells.iter().any(|c| c.chars().any(|ch| ch.is_ascii_digit()))
}

/// The table's printed name, when its one populated header cell is a caption
/// band. `None` ⇒ the cell is data and must be kept as a row instead.
fn caption_of(cells: &[String]) -> Option<String> {
    let populated: Vec<&str> =
        cells.iter().map(|c| c.trim()).filter(|c| !c.is_empty()).collect();
    if populated.len() != 1 {
        return None;
    }
    let only = populated[0];
    if only.chars().count() > CAPTION_MAX {
        return None;
    }
    let lower = only.to_lowercase();
    CAPTION_WORDS
        .iter()
        .any(|w| lower.contains(w))
        .then(|| only.to_string())
}

/// Column count the DATA declares — the widest body row. The header is measured
/// against this rather than the other way round: pdfplumber hands back header
/// arrays that are a column wider than the table (a wrapped cell split in two),
/// and taking their length as the truth is what shifted every cell one column
/// left and fused the Delegation columns.
fn data_ncols(rows: &[Vec<String>]) -> usize {
    rows.iter().map(|r| r.len()).max().unwrap_or(0)
}

/// Split one sidecar table into `(column labels, caption, body rows)`.
///
/// pdfplumber reports a `header` for every table it finds, but on a real corpus
/// that cell array is the column labels only when the table DECLARES them on that
/// page. Measured on the live corpus it is just as often
///   * a caption band — RoB p.26 `SCHEDULE-I (See Rule – 3 (i)` — which then
///     labelled all 39 departments (`SCHEDULE-I (See Rule – 3 (i): 1 | …`);
///   * a wrapped continuation of the previous page's last row — RoB p.27; or
///   * an ordinary data row — RoB p.28 `7. | Culture, Tourism …`, which then
///     prefixed a DIFFERENT department's row (`7.: 10 | Culture …`).
///
/// Nothing is discarded: a header that turns out to be data goes back into the
/// rows verbatim, and when the real labels are sitting in the first body row
/// instead they are promoted out of it.
fn classify_table(t: &SidecarTable) -> (Option<Vec<String>>, Option<String>, Vec<Vec<String>>) {
    // Drop the `(1)(2)(3)(4)` enumerator row; collapse rows with a single
    // populated cell (a spanning Note / wrapped continuation) to one cell so
    // linearize renders them as a clean standalone verbatim line.
    let normalize = |r: &Vec<String>| normalize_serial(collapse_single_cell(r));
    let mut rows: Vec<Vec<String>> =
        t.rows.iter().filter(|r| !is_enumerator_row(r)).map(normalize).collect();

    let populated = t.header.iter().filter(|c| !c.trim().is_empty()).count();
    if populated > 0 && looks_like_header_row(&t.header) && t.header.len() == data_ncols(&rows) {
        return (Some(t.header.clone()), None, rows);
    }
    let caption = caption_of(&t.header);
    if caption.is_none() && populated > 0 {
        rows.insert(0, normalize(&t.header)); // not a header, not a name ⇒ data
    }

    // The labels may be the first body row (pdfplumber gives the caption band the
    // header slot and pushes the real labels down).
    let promote = rows.first().is_some_and(|r| looks_like_header_row(r));
    let header = promote.then(|| rows.remove(0));
    (header, caption, rows)
}

/// Rows to carry into one stitched table before starting a fresh unit.
///
/// The whole point of stitching is that a "how many" question can be answered
/// from ONE retrieved chunk, so the cap has to clear the largest real list in the
/// corpus: RoB Schedule-I is 39 departments over 9 pages (47 rows once wrapped
/// continuations are counted, ~15k characters). Delegation-of-Financial-Powers
/// runs 796 rows over 76 pages and must NOT become one chunk — past that budget
/// the table keeps stitching its header but starts a new unit.
const MAX_STITCH_ROWS: usize = 60;
const MAX_STITCH_CHARS: usize = 20_000;

/// How many pages a table may skip and still be the same table. RoB Schedule-I
/// runs 26→31 then resumes on 33, and joining across that gap is only safe
/// because the serials continue (…32 on p.31, 33 on p.33) — see
/// [`continues_the_serials`].
const MAX_PAGE_GAP: u32 = 2;

/// Join per-page tables back into the logical tables they were printed as.
///
/// pdfplumber sees one table per page, so a schedule that runs over nine pages
/// arrives as nine tables — and only the first page declares the column labels.
/// That is both defects at once: no single chunk holds the whole list (RoB
/// answered "32 departments" of 39 because top-K retrieved five of the nine
/// fragments), and every continuation page's rows carry no labels or the wrong
/// ones.
///
/// A table continues the previous one when it declares no header of its own, sits
/// on the same or the next page, and has the same column count. A page gap or a
/// changed shape ends the group — a new schedule that re-declares its own header
/// therefore always starts fresh, and a table that cannot be matched is left
/// exactly as it was read.
fn stitch_continuations(pending: Vec<PendingTable>) -> Vec<ExtractedTable> {
    let mut out: Vec<ExtractedTable> = Vec::new();
    // Shape of the group currently open: (last page, column count, header, caption).
    let mut group: Option<(u32, usize, Vec<String>, Option<String>)> = None;
    let mut budget = (0usize, 0usize); // (rows, chars) already in the open unit
    let mut last_serial: Option<u32> = None;

    for p in pending {
        let ncols = data_ncols(&p.rows);
        let rows_here = p.rows.len();
        let chars_here: usize =
            p.rows.iter().map(|r| r.iter().map(|c| c.chars().count()).sum::<usize>()).sum();
        let first_serial = p.rows.iter().find_map(|r| r.first().and_then(|c| parse_serial(c)));

        let continues = match (&group, &p.header) {
            // A table that declares its own labels opens a new group.
            (Some((last, cols, _, _)), None) if *cols == ncols => {
                let gap = p.page.saturating_sub(*last);
                gap == 0 || gap == 1 || (gap <= MAX_PAGE_GAP && continues_the_serials(last_serial, first_serial))
            }
            _ => false,
        };

        if continues {
            let (last, _, header, caption) = group.as_mut().expect("continues ⇒ group open");
            *last = p.page;
            let (header, caption) = (header.clone(), caption.clone());
            last_serial = p.rows.iter().rev().find_map(|r| r.first().and_then(|c| parse_serial(c))).or(last_serial);
            let fits = budget.0 + rows_here <= MAX_STITCH_ROWS
                && budget.1 + chars_here <= MAX_STITCH_CHARS;
            match out.last_mut().filter(|_| fits) {
                Some(open) => {
                    open.rows.extend(rows_as(p.rows, p.page));
                    budget = (budget.0 + rows_here, budget.1 + chars_here);
                }
                // Over budget: the header still carries across (the rows would be
                // unlabelled otherwise), but the unit starts again.
                None => {
                    out.push(ExtractedTable {
                        id: p.id,
                        page: p.page,
                        caption,
                        header,
                        rows: rows_as(p.rows, p.page),
                    });
                    budget = (rows_here, chars_here);
                }
            }
            continue;
        }

        group = Some((p.page, ncols, p.header.clone().unwrap_or_default(), p.caption.clone()));
        budget = (rows_here, chars_here);
        last_serial = p.rows.iter().rev().find_map(|r| r.first().and_then(|c| parse_serial(c)));
        out.push(ExtractedTable {
            id: p.id,
            page: p.page,
            caption: p.caption,
            header: p.header.unwrap_or_default(),
            rows: rows_as(p.rows, p.page),
        });
    }
    out
}

/// Do the serials pick up where the open table left off? The only evidence that
/// survives a page the extractor found no table on (RoB p.32) — a schedule
/// numbered …32, 33 is one list, while a new schedule restarts at 1. Both sides
/// must be numbered: with nothing to compare, the tables stay separate.
fn continues_the_serials(last: Option<u32>, first: Option<u32>) -> bool {
    matches!((last, first), (Some(l), Some(f)) if f > l)
}

fn rows_as(rows: Vec<Vec<String>>, page: u32) -> Vec<TableRow> {
    rows.into_iter().map(|cells| TableRow { cells, page }).collect()
}

/// Is this "table" actually PROSE? (2026-07-28 — provisions were disappearing.)
///
/// pdfplumber's ruling-line detector sometimes fires on a page of ordinary
/// provisions. That is destructive here, because [`reflow_with_sidecar`]
/// REPLACES the page's text with the table rendering: the provisions inside
/// never reach heading detection, so they get no label and become
/// uncitable. Measured on the live corpus: Constitution Articles 60, 61, 127,
/// 128, 240 and 241 were all stored as `Table (p.NN)` rows.
///
/// The signature is geometric, not legal, so it needs no document vocabulary:
///   * every row collapses to a SINGLE populated column, and
///   * the rows are PARAGRAPH-sized.
///
/// Corpus evidence — the four false Constitution tables were each ONE row of
/// 1,720–2,600 chars in one column; genuine tables carry 2+ populated columns
/// (`| Sr. # | Nature of power | To whom delegated | Extent |`) with rows
/// averaging 113–576 chars. The single-column test alone excludes every
/// genuine table measured; the size tests keep a short single-column LIST
/// (many small rows) working as a table.
///
/// Rejecting a region is safe: the page keeps its original text, so the
/// content is chunked as prose instead of vanishing.
///
/// **The header counts.** Measured on Constitution pp.89/175/187, pdfplumber
/// put the ENTIRE page of prose in the header cell (1,500–1,900 chars holding
/// Articles 127/128, 240/241/242, and the definitions) and left a single data
/// row holding only the footnote rule (`––––– 1See …`, 133–371 chars). A guard
/// that measured data rows alone saw "371 chars" and let the region through —
/// which is exactly how those Articles stayed lost after the first fix. So the
/// header is measured as one more unit, and a header with 2+ populated cells
/// is itself proof of a real table.
fn looks_like_prose_not_table(
    rows: &[Vec<String>],
    header: Option<&Vec<String>>,
    lin: &[String],
) -> bool {
    /// Total text below this is too small to be a swallowed provision.
    const PROSE_MIN_TOTAL: usize = 400;
    /// Mean unit length below this is a LIST of short cells, not paragraphs.
    const PROSE_MIN_MEAN_UNIT: usize = 200;

    let populated = |cells: &[String]| cells.iter().filter(|c| !c.trim().is_empty()).count();
    // A genuine table names 2+ columns; a swallowed page has one giant cell.
    if header.is_some_and(|h| populated(h) > 1) {
        return false;
    }
    if !rows.iter().all(|r| populated(r) <= 1) {
        return false;
    }

    // Measure the SOURCE cells (header + rows), not the rendering: `lin` may
    // fold a one-column header into each row's prefix, which double-counts.
    let header_chars: usize = header
        .map(|h| h.iter().map(|c| c.chars().count()).sum())
        .unwrap_or(0);
    let row_chars: usize = rows
        .iter()
        .map(|r| r.iter().map(|c| c.chars().count()).sum::<usize>())
        .sum();
    let units = rows.len() + usize::from(header.is_some());
    if units == 0 {
        return false;
    }
    let total = header_chars + row_chars;
    // `lin` is still consulted so an empty rendering can never look prose-sized.
    let rendered: usize = lin.iter().map(|l| l.chars().count()).sum();
    if rendered == 0 {
        return false;
    }

    // Branch A (2026-08-06) — a TITLE BOX, not a table.
    //
    // The size tests above catch a swallowed PAGE (few, huge units). They cannot
    // catch the opposite shape: a ruled box around a document's title, which is a
    // handful of SHORT centred lines. On the APT Rules 1974 that produced a real
    // one-column, two-row "table" in production:
    //
    //     | col1 |
    //     | --- |
    //     | THE SINDH CIVIL SERVANTS (APPOINTMENT, PROMOTION AND |
    //     | TRANSFER) RULES, 1974 |
    //
    // Those two rows then competed as retrieval evidence and took slots in the
    // context window — 2 of the 3 chunks the one governing instrument contributed
    // to Proxima finding #4 were this title matter.
    //
    // Row COUNT is the discriminator the existing tests lack: a genuine
    // single-column list is MANY short rows (a 39-department roster); a title box
    // is a few. Both single-column preconditions above must already hold, so this
    // can never touch a multi-column table.
    if rows.len() < SINGLE_COL_MIN_ROWS {
        return true;
    }

    // Branch B (original) — a swallowed page of prose in one giant cell.
    total >= PROSE_MIN_TOTAL && total / units >= PROSE_MIN_MEAN_UNIT
}

/// Fewest rows a SINGLE-COLUMN region may have and still be treated as a table.
/// Below this it is a title/cover box (see `looks_like_prose_not_table` branch A).
/// A genuine single-column list — the Rules of Business Schedule-I department
/// roster is 39 rows — is far above it.
const SINGLE_COL_MIN_ROWS: usize = 6;

/// A row with exactly one non-empty cell (a spanning `Note:` line or a wrapped
/// continuation) becomes a one-column row so [`linearize`] emits it verbatim as
/// its own line instead of folding it under a column label.
fn collapse_single_cell(row: &[String]) -> Vec<String> {
    let populated: Vec<&String> = row.iter().filter(|c| !c.trim().is_empty()).collect();
    if populated.len() == 1 {
        vec![populated[0].clone()]
    } else {
        row.to_vec()
    }
}

/// pdfplumber emits serials with their trailing dot (`"1."`); [`linearize`] re-adds
/// `". "` after the serial, which would yield `"1.."`. Strip a trailing dot from a
/// purely-numeric serial column (multi-cell rows only) so the output matches the
/// projection engine's bare-serial format byte-for-byte.
fn normalize_serial(mut row: Vec<String>) -> Vec<String> {
    if row.len() > 1 {
        let c0 = row[0].trim();
        if let Some(stripped) = c0.strip_suffix('.') {
            if !stripped.is_empty() && stripped.chars().all(|ch| ch.is_ascii_digit()) {
                row[0] = stripped.to_string();
            }
        }
    }
    row
}

/// One self-describing line per data row, headers folded in:
/// `"{serial}. {description} — {label3}: {cell3}; {label4}: {cell4}"`.
fn linearize(rows: &[Vec<String>], labels: Option<&Vec<String>>) -> Vec<String> {
    let mut out = Vec::new();
    for r in rows {
        let ncols = r.len();
        let c1 = r[0].trim();
        if ncols == 1 {
            if !c1.is_empty() {
                out.push(end_period(c1));
            }
            continue;
        }
        let lead = r.get(1).map(|s| s.trim()).unwrap_or("");
        let mut tail: Vec<String> = Vec::new();
        for j in 2..ncols {
            let cell = r[j].trim();
            if cell.is_empty() {
                continue;
            }
            match labels {
                Some(l) if j < l.len() && !l[j].trim().is_empty() => {
                    tail.push(format!("{}: {}", l[j].trim(), cell));
                }
                _ => tail.push(cell.to_string()),
            }
        }
        let mut s = String::new();
        if !c1.is_empty() {
            s.push_str(c1);
            s.push_str(". ");
        }
        s.push_str(lead);
        if !tail.is_empty() {
            s.push_str(" — ");
            s.push_str(&tail.join("; "));
        }
        let s = s.trim();
        if !s.is_empty() {
            out.push(end_period(s));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn w(text: &str, x: f32, width: f32, y: f32) -> WordBox {
        WordBox { text: text.into(), x, y, w: width, h: 12.0, page: 1 }
    }

    /// Delegation-§3 shape: serial / nature-of-power / authority / extent, with
    /// a wrapped cell in row 11 ("motor vehicles" on the next visual line).
    fn delegation_words() -> Vec<WordBox> {
        vec![
            // header @ y=100
            w("S.No", 10.0, 20.0, 100.0),
            w("Nature", 60.0, 35.0, 100.0),
            w("of", 99.0, 10.0, 100.0),
            w("Power", 112.0, 30.0, 100.0),
            w("Authority", 230.0, 50.0, 100.0),
            w("Extent", 400.0, 35.0, 100.0),
            // (1)(2)(3)(4) enumerator @ y=110 — the declared-schema oracle (US5 v2)
            w("(1)", 10.0, 14.0, 110.0),
            w("(2)", 60.0, 14.0, 110.0),
            w("(3)", 230.0, 14.0, 110.0),
            w("(4)", 400.0, 14.0, 110.0),
            // row 11 @ y=120
            w("11", 10.0, 12.0, 120.0),
            w("Power", 60.0, 30.0, 120.0),
            w("to", 94.0, 10.0, 120.0),
            w("declare", 108.0, 40.0, 120.0),
            w("surplus", 152.0, 38.0, 120.0),
            w("Administrative", 230.0, 70.0, 120.0),
            w("Department", 304.0, 55.0, 120.0),
            w("Full", 400.0, 30.0, 120.0),
            w("powers", 434.0, 40.0, 120.0),
            // wrapped continuation of row 11's col1 @ y=135
            w("motor", 60.0, 35.0, 135.0),
            w("vehicles", 99.0, 50.0, 135.0),
            // row 12 @ y=155
            w("12", 10.0, 12.0, 155.0),
            w("Power", 60.0, 30.0, 155.0),
            w("to", 94.0, 10.0, 155.0),
            w("sanction", 108.0, 52.0, 155.0),
            w("Director", 230.0, 50.0, 155.0),
            w("General", 284.0, 46.0, 155.0),
            w("Within", 400.0, 40.0, 155.0),
            w("limits", 444.0, 31.0, 155.0),
        ]
    }

    #[test]
    fn delegation_grid_reflows_to_self_describing_rows() {
        let out = reflow_tables(&delegation_words(), "GARBLED LAYOUT IGNORED FOR THIS PAGE");
        assert!(
            out.contains(
                "11. Power to declare surplus motor vehicles — \
                 Authority: Administrative Department; Extent: Full powers."
            ),
            "row 11 must fold the wrapped cell + headers: {out}"
        );
        assert!(
            out.contains("12. Power to sanction — Authority: Director General; Extent: Within limits."),
            "row 12 must linearize in order: {out}"
        );
        // Header row is folded into the data rows, not emitted standalone.
        assert!(!out.contains("S.No"), "header must not survive as a chunk line: {out}");
    }

    /// SAFETY CONTRACT (Phase 1): a section heading (`COLLEGE EDUCATION DEPARTMENT`)
    /// that falls inside the detected table band must NOT be linearized as a data
    /// row. When a would-be row is a heading, reconstruction is not verifiable →
    /// the page falls back to its `-layout` text byte-for-byte. (Reproduces the
    /// live `Rule 22(v)` garble: "COLLEGE EDUCATION DEPARTMENT — Extent: work".)
    #[test]
    fn heading_inside_table_band_forces_fallback() {
        let layout = "LAYOUT-SENTINEL-KEEP-ME";
        let ws = vec![
            // header @100 + (1)(2)(3)(4) enumerator @110 → declared schema
            w("Sr.", 10.0, 20.0, 100.0), w("Nature", 60.0, 35.0, 100.0),
            w("Authority", 230.0, 50.0, 100.0), w("Extent", 400.0, 35.0, 100.0),
            w("(1)", 10.0, 14.0, 110.0), w("(2)", 60.0, 14.0, 110.0),
            w("(3)", 230.0, 14.0, 110.0), w("(4)", 400.0, 14.0, 110.0),
            // row 1 @120
            w("1.", 10.0, 12.0, 120.0), w("Approve", 60.0, 40.0, 120.0),
            w("DG", 230.0, 22.0, 120.0), w("Full", 400.0, 30.0, 120.0),
            // an ALL-CAPS section heading swept into the band @140
            w("COLLEGE", 60.0, 55.0, 140.0), w("EDUCATION", 120.0, 60.0, 140.0),
            w("DEPARTMENT", 185.0, 70.0, 140.0),
            // row 2 @160
            w("2.", 10.0, 12.0, 160.0), w("Reject", 60.0, 40.0, 160.0),
            w("Board", 230.0, 40.0, 160.0), w("None", 400.0, 30.0, 160.0),
        ];
        let out = reflow_tables(&ws, layout);
        assert!(
            !out.contains("EDUCATION DEPARTMENT") || out == layout,
            "a heading inside the band must not be emitted as a row — page must fall back: {out}"
        );
        assert_eq!(out, layout, "verification fails → keep -layout byte-for-byte: {out}");
    }

    /// SAFETY CONTRACT (Phase 1): if the detected rows' serial column is not a
    /// monotonically increasing sequence, the row segmentation is wrong →
    /// fall back to `-layout` rather than emit a scrambled order.
    #[test]
    fn non_monotonic_serials_force_fallback() {
        let layout = "LAYOUT-SENTINEL-KEEP-ME";
        // visual order 1, 3, 2 (serials out of order) → segmentation is wrong.
        let ws = vec![
            w("Sr.", 10.0, 20.0, 100.0), w("Nature", 60.0, 35.0, 100.0),
            w("Authority", 230.0, 50.0, 100.0), w("Extent", 400.0, 35.0, 100.0),
            w("(1)", 10.0, 14.0, 110.0), w("(2)", 60.0, 14.0, 110.0),
            w("(3)", 230.0, 14.0, 110.0), w("(4)", 400.0, 14.0, 110.0),
            w("1.", 10.0, 12.0, 120.0), w("Approve", 60.0, 40.0, 120.0),
            w("DG", 230.0, 22.0, 120.0), w("Full", 400.0, 30.0, 120.0),
            w("3.", 10.0, 12.0, 140.0), w("Refer", 60.0, 40.0, 140.0),
            w("Sec", 230.0, 30.0, 140.0), w("Part", 400.0, 30.0, 140.0),
            w("2.", 10.0, 12.0, 160.0), w("Reject", 60.0, 40.0, 160.0),
            w("Board", 230.0, 40.0, 160.0), w("None", 400.0, 30.0, 160.0),
        ];
        let out = reflow_tables(&ws, layout);
        assert_eq!(out, layout, "non-monotonic serials → keep -layout byte-for-byte: {out}");
    }

    /// US5 v2 (T035): reconstruction now requires the document's declared schema
    /// — the header row + a `(1)(2)(3)(4)` enumerator row. A clean multi-column
    /// table with monotonic serials but NO enumerator has no schema oracle, so it
    /// falls back to `-layout` (header-anchored, FINAL §A). Today's recurrence
    /// engine wrongly reconstructs it — this test is RED until the rewrite.
    #[test]
    fn table_without_enumerator_falls_back() {
        let layout = "LAYOUT-NO-ENUM";
        let ws = vec![
            w("Sr.", 10.0, 20.0, 100.0), w("Power", 60.0, 35.0, 100.0),
            w("Authority", 230.0, 50.0, 100.0), w("Extent", 400.0, 35.0, 100.0),
            w("1.", 10.0, 12.0, 120.0), w("Approve", 60.0, 40.0, 120.0),
            w("DG", 230.0, 22.0, 120.0), w("Full", 400.0, 30.0, 120.0),
            w("2.", 10.0, 12.0, 140.0), w("Reject", 60.0, 40.0, 140.0),
            w("Board", 230.0, 40.0, 140.0), w("None", 400.0, 30.0, 140.0),
            w("3.", 10.0, 12.0, 160.0), w("Refer", 60.0, 40.0, 160.0),
            w("Sec", 230.0, 30.0, 160.0), w("Part", 400.0, 30.0, 160.0),
        ];
        assert_eq!(reflow_tables(&ws, layout), layout, "no enumerator → no schema → fallback");
    }

    #[test]
    fn prose_page_is_left_unchanged() {
        // Single-column sentence: small gaps, no columns → no table → verbatim.
        let layout = "The competent authority may sanction expenditure within limits.";
        let words = vec![
            w("The", 10.0, 20.0, 100.0),
            w("competent", 34.0, 55.0, 100.0),
            w("authority", 93.0, 50.0, 100.0),
            w("may", 10.0, 22.0, 114.0),
            w("sanction", 36.0, 50.0, 114.0),
            w("expenditure", 90.0, 65.0, 114.0),
            w("within", 10.0, 38.0, 128.0),
            w("limits", 52.0, 32.0, 128.0),
        ];
        assert_eq!(reflow_tables(&words, layout), layout);
    }

    #[test]
    fn too_few_table_lines_falls_back_to_layout() {
        // Only 2 multi-cell lines (< 3) → not confident → keep layout text.
        let layout = "Sr  Power  Authority\n1   Approve  DG";
        let words = vec![
            w("Sr", 10.0, 16.0, 100.0),
            w("Power", 100.0, 35.0, 100.0),
            w("Authority", 230.0, 50.0, 100.0),
            w("1", 10.0, 10.0, 120.0),
            w("Approve", 100.0, 50.0, 120.0),
            w("DG", 230.0, 22.0, 120.0),
        ];
        assert_eq!(reflow_tables(&words, layout), layout);
    }

    fn wp(text: &str, x: f32, width: f32, y: f32, page: u32) -> WordBox {
        WordBox { text: text.into(), x, y, w: width, h: 12.0, page }
    }

    /// US5 v2 (T035): a multi-page schedule re-declares its header + `(1)(2)(3)(4)`
    /// enumerator on EVERY page (Sindh schedules do this ~55×), so each page
    /// self-anchors independently — no cross-page column carry. Both pages
    /// reconstruct from their own declared schema. (Row-level continuation of a
    /// cell that spills the page break is T036.)
    #[test]
    fn each_page_self_anchors_with_repeated_header() {
        let header = [("S.No", 10.0, 20.0), ("Power", 60.0, 30.0),
                      ("Authority", 230.0, 50.0), ("Extent", 400.0, 35.0)];
        let mut ws: Vec<WordBox> = Vec::new();
        for page in [1u32, 2] {
            // repeated header @100 + enumerator @110
            for (t, x, wd) in header {
                ws.push(wp(t, x, wd, 100.0, page));
            }
            for (m, x) in [("(1)", 10.0), ("(2)", 60.0), ("(3)", 230.0), ("(4)", 400.0)] {
                ws.push(wp(m, x, 14.0, 110.0, page));
            }
        }
        // Page 1 rows 11, 12
        for (n, y, lead, auth, ext) in [
            ("11", 120.0, ("Approve", 55.0), ("DG", 22.0), ("Full", 30.0)),
            ("12", 140.0, ("Sanction", 60.0), ("Secretary", 60.0), ("Limited", 45.0)),
        ] {
            ws.push(wp(n, 10.0, 12.0, y, 1));
            ws.push(wp(lead.0, 60.0, lead.1, y, 1));
            ws.push(wp(auth.0, 230.0, auth.1, y, 1));
            ws.push(wp(ext.0, 400.0, ext.1, y, 1));
        }
        // Page 2 rows 13, 14
        for (n, y, lead, auth, ext) in [
            ("13", 120.0, ("Reject", 45.0), ("Board", 40.0), ("None", 35.0)),
            ("14", 140.0, ("Refer", 40.0), ("Minister", 55.0), ("Partial", 45.0)),
        ] {
            ws.push(wp(n, 10.0, 12.0, y, 2));
            ws.push(wp(lead.0, 60.0, lead.1, y, 2));
            ws.push(wp(auth.0, 230.0, auth.1, y, 2));
            ws.push(wp(ext.0, 400.0, ext.1, y, 2));
        }

        let out = reflow_tables(&ws, "PAGE1 GARBLED\x0cPAGE2 GARBLED");
        assert!(out.contains("11. Approve — Authority: DG; Extent: Full."), "page-1 anchor row: {out}");
        assert!(out.contains("13. Reject — Authority: Board; Extent: None."),
            "page-2 self-anchors on its repeated header: {out}");
        assert!(out.contains("14. Refer — Authority: Minister; Extent: Partial."), "page-2 row 14: {out}");
        assert!(!out.contains("PAGE1 GARBLED") && !out.contains("PAGE2 GARBLED"),
            "both pages reconstructed from their own schema: {out}");
    }

    /// Real geometry from `09_Sindh_Delegation_of_Financial_Powers_2019.pdf`
    /// page 8 (Second Schedule, Part-I). The table has TWO header rows — the
    /// column titles and a "(1)(2)(3)(4)" enumerator sub-header — and the third
    /// column wraps over several lines. Reproduces the live garble cause: the
    /// enumerator row must be dropped, not linearized as data.
    #[test]
    fn real_delegation_page8_reconstructs_first_row() {
        let h = 11.0;
        let wr = |t: &str, x: f32, width: f32, y: f32| WordBox {
            text: t.into(), x, y, w: width, h, page: 1,
        };
        let ws = vec![
            // header titles @ y=150.5
            wr("Sr.", 84.2, 12.0, 150.5), wr("#", 99.1, 8.0, 150.5),
            wr("Nature", 139.5, 36.6, 150.5), wr("of", 179.1, 10.7, 150.5), wr("power", 192.8, 34.4, 150.5),
            wr("To", 282.0, 12.0, 150.5), wr("whom", 296.9, 33.4, 150.5), wr("delegated", 333.2, 57.6, 150.5),
            wr("Extent", 495.7, 32.5, 150.5),
            // enumerator sub-header @ y=164.4  ← must be dropped
            wr("(1)", 88.5, 14.1, 164.4), wr("(2)", 176.3, 14.1, 164.4),
            wr("(3)", 329.3, 14.1, 164.4), wr("(4)", 504.9, 14.1, 164.4),
            // row 1 @ y=184.5
            wr("1.", 91.1, 9.1, 184.5), wr("Creation", 123.6, 46.9, 184.5), wr("of", 173.5, 10.6, 184.5), wr("Post", 187.1, 21.7, 184.5),
            wr("(i)", 254.0, 10.3, 184.5), wr("Administrative", 290.1, 75.1, 184.5),
            wr("Full", 429.7, 16.4, 184.5), wr("Powers", 449.2, 37.5, 184.5),
            // wrapped col-3 continuation @ y=197.9 / 211.5
            wr("Departments", 254.0, 69.6, 197.9), wr("shall", 333.7, 22.9, 197.9), wr("have", 366.7, 27.4, 197.9), wr("full", 404.2, 14.4, 197.9),
            wr("powers", 254.0, 38.6, 211.5), wr("to", 296.9, 10.9, 211.5), wr("create", 312.0, 36.0, 211.5),
            // rows 2-4 @ y=240/260/280 — recurring column left-edges (84/123/254/429)
            // so frequency clustering locks the true columns over header/indent noise.
            wr("2.", 91.0, 9.0, 240.0), wr("Abolition", 123.0, 55.0, 240.0), wr("Secretary", 254.0, 55.0, 240.0), wr("Full", 429.0, 16.0, 240.0), wr("Powers", 449.0, 38.0, 240.0),
            wr("3.", 91.0, 9.0, 260.0), wr("Transfer", 123.0, 50.0, 260.0), wr("Director", 254.0, 48.0, 260.0), wr("Limited", 429.0, 45.0, 260.0),
            wr("4.", 91.0, 9.0, 280.0), wr("Re-appropriation", 123.0, 95.0, 280.0), wr("Department", 254.0, 60.0, 280.0), wr("None", 429.0, 32.0, 280.0),
        ];
        let out = reflow_tables(&ws, "GARBLED");
        assert!(!out.contains("(1) (2) (3) (4)") && !out.contains("(1). (2)"),
            "enumerator sub-header must be dropped, not kept as a data row: {out}");
        assert!(out.contains("Creation of Post"), "row 1 description present: {out}");
        assert!(out.contains("To whom delegated: (i) Administrative"),
            "col-3 folds in its header label + verbatim wrapped cell: {out}");
        assert!(out.contains("Extent: Full Powers"), "col-4 folds in verbatim: {out}");
        assert!(!out.contains("Sr."), "the column-title header is folded, not emitted: {out}");
        // The tight Extent column must NOT interleave into the authority column
        // (the live "(i) Administrative Full Powers Departments…" bug): the col-3
        // wrapped text stays contiguous, "Full Powers" only in the Extent slot.
        assert!(out.contains("Administrative Departments"),
            "col-3 stays contiguous — Extent must not splice into it: {out}");
    }

    // ── Phase 9a: pdfplumber sidecar reflow ──────────────────────────────────

    /// The real Delegation p8 extraction captured from the frozen sidecar binary
    /// (4-col schedule): enumerator row, two serial rows, a wrapped continuation,
    /// and a spanning Note row. Locks that the sidecar path emits the SAME
    /// schema-anchored row format as the projection engine.
    fn delegation_p8_page() -> SidecarPage {
        let r = |a: &str, b: &str, c: &str, d: &str| {
            vec![a.to_string(), b.to_string(), c.to_string(), d.to_string()]
        };
        SidecarPage {
            page: 8,
            above: "SECOND SCHEDULE\n(See Rule-3)\nPART-I".to_string(),
            below: "7".to_string(),
            tables: vec![SidecarTable {
                header: vec![
                    "Sr. #".into(),
                    "Nature of power".into(),
                    "To whom delegated".into(),
                    "Extent".into(),
                ],
                rows: vec![
                    r("(1)", "(2)", "(3)", "(4)"),
                    r("1.", "Creation of Post", "(i) Administrative Departments shall have full powers", "Full Powers"),
                    r("", "", "(ii) After completion of the project", ""),
                    r("2.", "Reciprocal up-gradation", "Administrative Department", "Full Powers"),
                    r("Note: All orders shall be passed by the Administrative Department.", "", "", ""),
                ],
            }],
        }
    }

    #[test]
    fn sidecar_reflow_matches_schema_anchored_format() {
        let page = delegation_p8_page();
        // An 8-page `-layout` so page 8 (1-based) lands in range.
        let layout = vec!["P"; 8].join("\x0c");
        let (text, rows, tables) = reflow_with_sidecar(&layout, &[page]);

        // Feature 023: one structured table returned, marker-wrapped in the text;
        // `rows` (table_row tagging) still excludes the markers.
        assert_eq!(tables.len(), 1, "one ExtractedTable returned");
        assert!(text.contains(&table_open_marker(0)) && text.contains(&table_close_marker(0)),
            "table region marker-wrapped in text");
        assert!(!rows.iter().any(|r| is_table_marker_line(r)), "emitted rows carry no markers");

        // The enumerator row is dropped, never emitted as data.
        assert!(!text.contains("(1). (2)") && !rows.iter().any(|r| r.contains("(2)")),
            "(1)(2)(3)(4) enumerator must be dropped: {rows:?}");
        // Serial row folds its column labels in — identical to the projection path.
        assert!(rows.iter().any(|r| r.starts_with("1. Creation of Post")
            && r.contains("To whom delegated: (i) Administrative")
            && r.contains("Extent: Full Powers")),
            "schema-anchored serial row: {rows:?}");
        // Wrapped continuation (only col-3 populated) → clean standalone verbatim line.
        assert!(rows.iter().any(|r| r.starts_with("(ii) After completion")),
            "wrapped continuation emitted verbatim: {rows:?}");
        // Spanning Note row (only col-0 populated) → standalone verbatim line.
        assert!(rows.iter().any(|r| r.starts_with("Note: All orders")),
            "spanning note emitted verbatim: {rows:?}");
        // Prose above/below the table band is preserved around the rows.
        assert!(text.contains("SECOND SCHEDULE") && text.contains("PART-I"),
            "above-prose preserved: {text}");
    }

    #[test]
    fn sidecar_reflow_leaves_unreported_pages_byte_for_byte() {
        let mut page = delegation_p8_page();
        page.page = 2; // reflow only the middle page of a 3-page layout
        let layout = "PROSE PAGE ONE\x0cGARBLED-P8\x0cPROSE PAGE THREE";
        let (text, _, _) = reflow_with_sidecar(layout, &[page]);
        let pages: Vec<&str> = text.split('\x0c').collect();
        assert_eq!(pages[0], "PROSE PAGE ONE", "page 1 untouched");
        assert_eq!(pages[2], "PROSE PAGE THREE", "page 3 untouched");
        assert_ne!(pages[1], "GARBLED-P8", "page 2 (reported) was reflowed");
    }

    #[test]
    fn sidecar_reflow_no_pages_is_noop() {
        let (text, rows, tables) = reflow_with_sidecar("UNCHANGED\x0cTEXT", &[]);
        assert_eq!(text, "UNCHANGED\x0cTEXT");
        assert!(rows.is_empty());
        assert!(tables.is_empty(), "no pages → no tables");
    }

    #[test]
    fn table_markers_round_trip_and_parse() {
        // Feature 023 (C-4): stripping the markers returns marker-free text, and
        // each marker line parses back to its (id, open/close).
        let page = delegation_p8_page();
        let layout = vec!["P"; 8].join("\x0c");
        let (text, _rows, tables) = reflow_with_sidecar(&layout, &[page]);
        assert_eq!(tables.len(), 1);

        // Markers present, then fully removed by strip.
        assert!(is_table_marker_line(&table_open_marker(0)));
        assert_eq!(parse_table_marker(&table_open_marker(3)), Some((3, true)));
        assert_eq!(parse_table_marker(&table_close_marker(3)), Some((3, false)));
        assert_eq!(parse_table_marker("Rule 4 ordinary line"), None);

        let stripped = strip_table_markers(&text);
        assert!(!stripped.lines().any(is_table_marker_line), "no markers survive strip");
        // Non-marker content is preserved through the strip.
        assert!(stripped.contains("SECOND SCHEDULE"));
        assert!(stripped.contains("1. Creation of Post"));
    }

    // ── Header classification + page stitching (2026-08-05) ─────────────────
    // Shapes measured on the live corpus: `Sindh Rules of Business 1986`
    // Schedule-I is one 39-department table printed over pp.26–34, and only p.26
    // declares anything header-like — a caption band, with the real column labels
    // in the first body row.

    fn cells(v: &[&str]) -> Vec<String> {
        v.iter().map(|s| s.to_string()).collect()
    }

    /// One page carrying one table, with no prose around it (the shape a
    /// continuation page takes).
    fn table_page(page: u32, header: &[&str], rows: &[Vec<String>]) -> SidecarPage {
        SidecarPage {
            page,
            above: String::new(),
            below: String::new(),
            tables: vec![SidecarTable { header: cells(header), rows: rows.to_vec() }],
        }
    }

    fn schedule_i_p26() -> SidecarPage {
        table_page(
            26,
            &["SCHEDULE-I (See Rule – 3 (i)", "", "", ""],
            &[
                cells(&["S. NO.", "Secretariat Department", "Attached Department", "Head of Attached Department"]),
                cells(&["1", "Agriculture Department", "(a) Agriculture Engineering", "Director"]),
                cells(&["2", "Board of Revenue", "(a) Survey & Settlement", "Director"]),
            ],
        )
    }

    /// The caption band pdfplumber hands over as the header must not become a
    /// column label — it was prefixing all 39 departments
    /// (`SCHEDULE-I (See Rule – 3 (i): 1 | Agriculture Department | …`) — and the
    /// real labels, sitting in the first body row, must be promoted out of it.
    #[test]
    fn caption_band_is_not_a_column_label() {
        let layout = vec!["P"; 26].join("\x0c");
        let (_text, _rows, tables) = reflow_with_sidecar(&layout, &[schedule_i_p26()]);
        assert_eq!(tables.len(), 1);
        let t = &tables[0];
        assert_eq!(t.caption.as_deref(), Some("SCHEDULE-I (See Rule – 3 (i)"));
        assert_eq!(t.header[0], "S. NO.", "the real labels were promoted: {:?}", t.header);
        assert_eq!(t.header[3], "Head of Attached Department");
        assert_eq!(t.rows.len(), 2, "the label row is a header, not data: {:?}", t.rows);
        assert_eq!(t.rows[0].cells[1], "Agriculture Department");
    }

    /// A continuation page declares no header of its own, so it must inherit the
    /// one the table opened with — and its rows must join that table rather than
    /// start a new one. This is the "32 departments of 39" defect: nine per-page
    /// fragments, of which top-K retrieved five.
    #[test]
    fn continuation_pages_join_the_table_and_inherit_its_header() {
        let layout = vec!["P"; 28].join("\x0c");
        let p27 = table_page(
            27,
            // pdfplumber parks a wrapped continuation of p.26's last row here.
            &["", "", "v. Charter Inspection and Evaluation Committee (deleted vide Notification No. SORI 4/2016).", "v. Chairman, Charter Inspection and Evaluation Committee."],
            &[cells(&["6", "Cooperative Department", "i) Cooperative Housing", "Registrar"])],
        );
        let p28 = table_page(
            28,
            &["", "", "", ""],
            &[cells(&["7", "Culture & Tourism Department", "(a) Directorate of Culture", "Director General"])],
        );
        let (_text, _rows, tables) =
            reflow_with_sidecar(&layout, &[schedule_i_p26(), p27, p28]);

        assert_eq!(tables.len(), 1, "one logical table, not three: {tables:#?}");
        let t = &tables[0];
        assert_eq!(t.page, 26, "the table starts where it was declared");
        assert_eq!(t.header[1], "Secretariat Department", "header carried across pages");
        assert_eq!(t.caption.as_deref(), Some("SCHEDULE-I (See Rule – 3 (i)"));
        // 2 rows from p.26, the demoted "header" + 1 row from p.27, 1 from p.28.
        assert_eq!(t.rows.len(), 5, "no row dropped or duplicated: {:?}", t.rows);
        assert!(
            t.rows.iter().any(|r| r.cells.iter().any(|c| c.contains("Charter Inspection"))),
            "a header that was really data stays in the table verbatim: {:?}", t.rows
        );
        // Each row keeps the page it was printed on, so its citation still opens
        // the document at the right page.
        assert_eq!(t.rows[0].page, 26);
        assert_eq!(t.rows.last().unwrap().page, 28);
    }

    /// Stitching must not swallow the next schedule: a table that declares its
    /// own labels opens a new unit, and so does one after a page gap.
    #[test]
    fn a_declared_header_or_a_page_gap_starts_a_new_table() {
        let layout = vec!["P"; 40].join("\x0c");
        let declares_own = table_page(
            27,
            &["Sr. #", "Nature of power", "To whom delegated", "Extent"],
            &[cells(&["1", "Creation of post", "Administrative Department", "Full powers"])],
        );
        let (_t, _r, tables) = reflow_with_sidecar(&layout, &[schedule_i_p26(), declares_own]);
        assert_eq!(tables.len(), 2, "a re-declared header starts a new table");
        assert_eq!(tables[1].header[0], "Sr. #");

        let after_a_gap = table_page(
            30,
            &["", "", "", ""],
            &[cells(&["9", "Energy Department", "(a) Directorate", "Director"])],
        );
        let (_t, _r, tables) = reflow_with_sidecar(&layout, &[schedule_i_p26(), after_a_gap]);
        assert_eq!(tables.len(), 2, "pp.26 and 30 are not the same table");
    }

    /// RoB Schedule-I resumes on p.33 after a page the extractor found no table
    /// on. The serials are the only evidence that it is still the same list —
    /// …2 on p.26, 3 on p.28 — and a list that restarts at 1 is a new table.
    #[test]
    fn a_one_page_gap_is_crossed_only_when_the_serials_continue() {
        let layout = vec!["P"; 30].join("\x0c");
        let resumes = table_page(
            28,
            &["", "", "", ""],
            &[cells(&["3", "Energy Department", "(a) Directorate", "Director"])],
        );
        let (_t, _r, tables) = reflow_with_sidecar(&layout, &[schedule_i_p26(), resumes]);
        assert_eq!(tables.len(), 1, "…2 then 3 across a one-page gap is one table");
        assert_eq!(tables[0].rows.len(), 3);

        let restarts = table_page(
            28,
            &["", "", "", ""],
            &[cells(&["1", "Chief Minister's Inspection Team", "(a) Directorate", "Director"])],
        );
        let (_t, _r, tables) = reflow_with_sidecar(&layout, &[schedule_i_p26(), restarts]);
        assert_eq!(tables.len(), 2, "a list that restarts at 1 is a different table");
    }

    /// Delegation-of-Financial-Powers runs 796 rows over 76 pages; stitching it
    /// into one chunk would be useless. Past the budget the unit restarts — but
    /// the header still travels, or the rows come out unlabelled.
    #[test]
    fn an_oversized_table_splits_into_units_that_keep_the_header() {
        let layout = vec!["P"; 90].join("\x0c");
        let mut pages = vec![table_page(
            8,
            &["Sr. #", "Nature of power", "To whom delegated", "Extent"],
            &[cells(&["1", "Creation of post", "Administrative Department", "Full powers"])],
        )];
        for p in 9..=40u32 {
            let rows: Vec<Vec<String>> = (0..4)
                .map(|i| {
                    cells(&[
                        &format!("{}", p * 10 + i),
                        "Power to sanction expenditure on works",
                        "Administrative Department",
                        "Full powers subject to budget provision",
                    ])
                })
                .collect();
            pages.push(table_page(p, &["", "", "", ""], &rows));
        }
        let (_t, _r, tables) = reflow_with_sidecar(&layout, &pages);
        assert!(tables.len() > 1, "the 129-row run must not become one unit");
        for t in &tables {
            assert_eq!(t.header[0], "Sr. #", "every unit keeps the declared header");
            assert!(t.rows.len() <= MAX_STITCH_ROWS, "unit within the row budget");
        }
        let total: usize = tables.iter().map(|t| t.rows.len()).sum();
        assert_eq!(total, 1 + 32 * 4, "every row survives the split");
    }

    /// A marker must never share a line with the form feed that joins the pages —
    /// when it did, the region never opened and its rows were minted as
    /// provisions downstream.
    #[test]
    fn markers_never_share_a_line_with_a_page_break() {
        let layout = "PROSE PAGE ONE\x0cGARBLED\x0cPROSE PAGE THREE";
        let (text, _rows, tables) = reflow_with_sidecar(layout, &[table_page(
            2,
            &["Sr. #", "Nature of power", "To whom delegated", "Extent"],
            &[cells(&["1", "Creation of post", "Administrative Department", "Full powers"])],
        )]);
        assert_eq!(tables.len(), 1);
        for line in text.split('\n') {
            if line.contains('\u{27E6}') {
                assert!(
                    is_table_marker_line(line),
                    "a marker must be alone on its line, got {line:?}"
                );
            }
        }
    }

    // ── Prose-not-table guard (2026-07-28) ──────────────────────────────────
    // Shapes taken from the live corpus audit (scripts/corpus_gap_check.sql):
    // the false Constitution "tables" were ONE single-column row of ~2k chars
    // holding Articles 127/240 verbatim; genuine tables carry 2+ populated
    // columns with much shorter rows.

    /// The exact defect: Article 127's text detected as a one-row table.
    #[test]
    fn prose_paragraph_in_one_column_is_not_a_table() {
        let para = format!(
            "127. Subject to the Constitution, the provisions of clauses (2) to (8) of \
             Article 53, clauses (2) and (3) of Article 54, Article 55, Articles 63 to 67, \
             Article 69, Article 77, Article 87 and Article 88 shall apply to and in \
             relation to a Provincial Assembly. {}",
            "Further provisions of the same length continue here. ".repeat(20)
        );
        let rows = vec![vec![para.clone()]];
        assert!(
            looks_like_prose_not_table(&rows, None, &[para]),
            "a single-column paragraph must be rejected as prose"
        );
    }

    /// The Constitution p.89/175/187 shape, measured from the live sidecar:
    /// the whole page of prose lands in the HEADER cell, and the only data row
    /// is the footnote rule. Measuring rows alone let Articles 127/128/240/241
    /// stay lost after the first attempt at this guard.
    #[test]
    fn prose_in_the_header_with_a_footnote_row_is_not_a_table() {
        let header = vec![format!(
            "127. Subject to the Constitution, the provisions of clauses (2) to (8) of \
             Article 53 shall apply to a Provincial Assembly. {}\
             Ordinances 128. (1) The Governor may, except when the Provincial Assembly \
             is in session, make and promulgate an Ordinance.",
            "Each reference is read as a reference to the Province. ".repeat(20)
        )];
        // The single data row is the footnote separator — small on purpose.
        let rows = vec![vec![
            "–––––––––––––––––––––– 1See footnote 9 on page 88.".to_string(),
        ]];
        let lin = vec![format!("{} :: {}", header[0], rows[0][0])];
        assert!(
            looks_like_prose_not_table(&rows, Some(&header), &lin),
            "prose parked in a one-column header must be rejected"
        );
    }

    /// A real table's header names its columns — never rejected, even if one
    /// cell happens to be wordy.
    #[test]
    fn multi_column_header_is_always_a_table() {
        let header = vec![
            "Sr. #".to_string(),
            "Nature of power".to_string(),
            "To whom delegated".to_string(),
            "Extent".to_string(),
        ];
        let rows = vec![vec![
            "1".to_string(),
            "Power to sanction expenditure".to_string(),
            "Administrative Department".to_string(),
            "Full powers".to_string(),
        ]];
        let lin = vec!["1 | Power to sanction expenditure | Administrative Department".to_string()];
        assert!(
            !looks_like_prose_not_table(&rows, Some(&header), &lin),
            "a table with named columns must never be rejected"
        );
    }

    /// A genuine delegation-of-powers table must still be treated as a table,
    /// however long it is — it has real columns.
    #[test]
    fn multi_column_table_is_kept_even_when_long() {
        let rows: Vec<Vec<String>> = (1..=8)
            .map(|i| {
                vec![
                    i.to_string(),
                    format!("Power to sanction expenditure on works of category {i}, \
                             including all incidental and connected charges thereto"),
                    "Administrative Department".to_string(),
                    "Full powers subject to budget provision".to_string(),
                ]
            })
            .collect();
        let lin: Vec<String> = rows.iter().map(|r| r.join(" | ")).collect();
        assert!(
            !looks_like_prose_not_table(&rows, None, &lin),
            "a multi-column table must never be rejected"
        );
    }

    /// A short single-column list (many small rows) is still a table — the
    /// guard targets paragraphs, not lists.
    #[test]
    fn short_single_column_list_is_kept() {
        let rows: Vec<Vec<String>> =
            (1..=20).map(|i| vec![format!("Item number {i}")]).collect();
        let lin: Vec<String> = rows.iter().map(|r| r[0].clone()).collect();
        assert!(
            !looks_like_prose_not_table(&rows, None, &lin),
            "a list of short single-column rows must stay a table"
        );
    }

    /// The Rules of Business Schedule-I department roster — the canonical
    /// single-column list this corpus depends on. Written to protect the
    /// documented "short single-column LIST stays a table" behaviour against the
    /// title-box guard below.
    #[test]
    fn thirty_nine_row_department_roster_is_kept() {
        let rows: Vec<Vec<String>> = (1..=39)
            .map(|i| vec![format!("{i}. Population Welfare Department")])
            .collect();
        let lin: Vec<String> = rows.iter().map(|r| r[0].clone()).collect();
        assert!(
            !looks_like_prose_not_table(&rows, None, &lin),
            "the 39-department roster must stay a table"
        );
    }

    /// The 2026-08-06 defect: a ruled box around the document title became a
    /// one-column, two-row "table" whose rows then competed as retrieval
    /// evidence. Verbatim from the APT Rules 1974 as stored in production.
    #[test]
    fn single_column_title_box_is_rejected() {
        let rows: Vec<Vec<String>> = vec![
            vec!["THE SINDH CIVIL SERVANTS (APPOINTMENT, PROMOTION AND".to_string()],
            vec!["TRANSFER) RULES, 1974".to_string()],
        ];
        let lin: Vec<String> = rows.iter().map(|r| r[0].clone()).collect();
        assert!(
            looks_like_prose_not_table(&rows, None, &lin),
            "a two-row single-column title box is not a table"
        );
    }

    /// The boundary is exact, and it is the ONLY thing separating the two cases
    /// above — so pin both sides of it.
    #[test]
    fn single_column_row_count_boundary_is_exact() {
        let build = |n: usize| {
            let rows: Vec<Vec<String>> =
                (1..=n).map(|i| vec![format!("Entry {i}")]).collect();
            let lin: Vec<String> = rows.iter().map(|r| r[0].clone()).collect();
            (rows, lin)
        };
        let (r5, l5) = build(5);
        assert!(looks_like_prose_not_table(&r5, None, &l5), "5 rows: title box");
        let (r6, l6) = build(6);
        assert!(!looks_like_prose_not_table(&r6, None, &l6), "6 rows: a list");
    }

    /// A two-row table with real COLUMNS is untouched — the guard must key on
    /// shape, not on row count alone.
    #[test]
    fn two_row_multi_column_table_is_kept() {
        let rows: Vec<Vec<String>> = vec![
            vec!["1".to_string(), "Secretary".to_string(), "Full powers".to_string()],
            vec!["2".to_string(), "Deputy Secretary".to_string(), "Up to Rs. 50,000".to_string()],
        ];
        let lin: Vec<String> = rows.iter().map(|r| r.join(" | ")).collect();
        assert!(
            !looks_like_prose_not_table(&rows, None, &lin),
            "row count must never reject a genuinely multi-column table"
        );
    }

    /// The whole point: a rejected region leaves the page's own text intact,
    /// so the provisions inside it survive to heading detection.
    #[test]
    fn rejected_prose_region_leaves_page_text_untouched() {
        let body = format!(
            "127. Subject to the Constitution, the provisions of clauses (2) to (8) of \
             Article 53 shall apply to a Provincial Assembly. {}",
            "The text continues at length across the page. ".repeat(20)
        );
        let layout = format!("Page one prose.\n\x0c{body}\n\x0cPage three prose.");
        let pages = vec![SidecarPage {
            page: 2,
            above: String::new(),
            below: String::new(),
            tables: vec![SidecarTable { header: Vec::new(), rows: vec![vec![body.clone()]] }],
        }];
        let (text, rows, tables) = reflow_with_sidecar(&layout, &pages);
        assert!(
            text.contains("127. Subject to the Constitution"),
            "the provision must remain in the body text"
        );
        assert!(!text.contains('\u{27E6}'), "no table markers should be written");
        assert!(rows.is_empty() && tables.is_empty(), "no table units should be emitted");
    }
}
