# Contract — Table Engine (US5)

**Module:** `src-tauri/src/services/parsers/table.rs` (evolve), called from
`parsers/pdf.rs::maybe_reflow_tables`.

## Pure core
```rust
pub struct WordBox { /* existing: text, x0, x1, y0, y1, page */ }

/// Detect column boundaries by projecting word x-extents onto the x-axis and
/// finding consistent zero-density bands. PRIMARY column detector (replaces the
/// pairwise-gap heuristic). Pure.
fn column_bands(words: &[&WordBox]) -> Vec<ColumnBand>;

/// Reflow a region into logical rows: non-empty leading column starts a row,
/// empty leading column = wrapped continuation. Headers folded into each row.
fn reflow_region(words: &[&WordBox], bands: &[ColumnBand]) -> Option<Vec<RowUnit>>;

/// Stitch continuation rows and drop repeated headers across page breaks by
/// carrying the column signature.
fn stitch_cross_page(rows: Vec<RowUnit>, sig: &ColumnSig) -> Vec<RowUnit>;

/// Top-level: confident-only. Ambiguous/ragged region => None (caller keeps -layout text).
pub fn reflow_tables(words: &[WordBox], layout_text: &str) -> String;
```

## Invariants (G1)
- Every emitted cell is a **verbatim substring** of the digital text; reflow may reorder/regroup, never synthesize.
- Confident-only: ragged/ambiguous region ⇒ keep original `-layout` text (a missed table is acceptable; a scramble is not).
- No OCR on digital pages (PaddleOCR reserved for true scans).

## Acceptance
- Delegation-of-Financial-Powers rows extract as discrete, in-order, correctly-labeled authority statements; every cell verbatim.
- Cross-page rows stitched; repeated header dropped once.
- Non-table prose unchanged (G2 prose fixtures clean).
- `cargo test --lib parsers::table` green (projection bands, wrapped continuation, cross-page stitch, ragged-fallback).
