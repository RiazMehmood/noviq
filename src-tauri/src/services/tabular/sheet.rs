//! Spreadsheet → a **typed** table.
//!
//! `parsers::document::extract_xlsx` flattens a workbook to tab-separated text
//! and calls `cell.to_string()` on every cell, which throws away the types
//! calamine already knows. That is fine for retrieval — a chunk is text either
//! way — but it is fatal for computation: you cannot sum a `String`.
//!
//! This module keeps the types, and additionally recovers numbers that were
//! never typed in the first place. Real departmental returns write money as
//! text: `Rs. 1,250.00`, `(45.20)` for a negative, `12.5%`, `1,234`. Proven on
//! the 2026-08-03 fixtures — `Complex_D_MessyTypes` is entirely text cells.
//!
//! Nothing here calls a model. Parsing a spreadsheet is not a language problem.

use std::path::Path;

// calamine 0.24: `Data` is the cell enum; `DataType` is a trait, not a type.
use calamine::{Data, Reader};

/// One cell, after normalisation.
#[derive(Debug, Clone, PartialEq)]
pub enum Cell {
    Num(f64),
    Text(String),
    Empty,
}

impl Cell {
    pub fn as_num(&self) -> Option<f64> {
        match self {
            Cell::Num(n) => Some(*n),
            _ => None,
        }
    }

    /// Display form. Numbers print without a trailing `.0` so a count reads as
    /// `6`, not `6.0` — the answer an officer expects to see.
    pub fn text(&self) -> String {
        match self {
            Cell::Num(n) => {
                if (n.fract()).abs() < f64::EPSILON && n.abs() < 1e15 {
                    format!("{}", *n as i64)
                } else {
                    format!("{n}")
                }
            }
            Cell::Text(s) => s.clone(),
            Cell::Empty => String::new(),
        }
    }

    pub fn is_empty(&self) -> bool {
        matches!(self, Cell::Empty) || matches!(self, Cell::Text(s) if s.trim().is_empty())
    }
}

/// A single sheet: a header row (already flattened, see [`build_headers`]) and
/// the data rows beneath it.
#[derive(Debug, Clone)]
pub struct Table {
    pub name: String,
    pub headers: Vec<String>,
    pub rows: Vec<Vec<Cell>>,
}

impl Table {
    /// Index of a column by name. Case- and space-insensitive, and a prefix
    /// match is accepted so a model that says "Revenue" still finds
    /// "Year-I · Revenue". Exact matches always win over prefixes.
    pub fn column(&self, name: &str) -> Option<usize> {
        let want = norm_key(name);
        if let Some(i) = self.headers.iter().position(|h| norm_key(h) == want) {
            return Some(i);
        }
        self.headers.iter().position(|h| {
            let h = norm_key(h);
            h.ends_with(&want) || h.starts_with(&want) || h.contains(&want)
        })
    }

    pub fn col_values(&self, idx: usize) -> Vec<Cell> {
        self.rows.iter().map(|r| r.get(idx).cloned().unwrap_or(Cell::Empty)).collect()
    }

    /// Rows whose first cell reads as a totals line. These must be excluded
    /// from any sum, or the total is counted twice — the single most common way
    /// a spreadsheet answer comes out exactly 2x too large.
    pub fn is_total_row(&self, row: &[Cell]) -> bool {
        row.first().map(|c| {
            let t = c.text().trim().to_lowercase();
            t == "total" || t == "grand total" || t == "sum" || t == "totals"
        }).unwrap_or(false)
    }
}

fn norm_key(s: &str) -> String {
    s.to_lowercase().chars().filter(|c| c.is_alphanumeric()).collect()
}

/// Parse a numeric value out of a human-written cell.
///
/// Handles, in this order: a leading currency word/symbol, thousands
/// separators, a trailing percent, and accounting negatives — `(45.20)` means
/// **-45.20**, and reading it as positive silently flips the sign of every
/// overspend in a financial statement.
pub fn parse_number(raw: &str) -> Option<f64> {
    let mut s = raw.trim().to_string();
    if s.is_empty() {
        return None;
    }
    let negative_parens = s.starts_with('(') && s.ends_with(')');
    if negative_parens {
        s = s[1..s.len() - 1].to_string();
    }
    let lower = s.to_lowercase();
    for prefix in ["rs.", "rs", "pkr", "$", "£", "€"] {
        if lower.starts_with(prefix) {
            s = s[prefix.len()..].to_string();
            break;
        }
    }
    let percent = s.trim_end().ends_with('%');
    if percent {
        s = s.trim_end().trim_end_matches('%').to_string();
    }
    let cleaned: String = s.chars().filter(|c| !matches!(c, ',' | ' ' | '\u{a0}')).collect();
    if cleaned.is_empty() {
        return None;
    }
    // Reject things that merely CONTAIN digits ("Vendor-01", "2024-07-01"):
    // only a clean numeric literal is a number.
    let mut v: f64 = cleaned.parse().ok()?;
    if negative_parens {
        v = -v;
    }
    Some(v)
}

fn cell_from(d: &Data) -> Cell {
    match d {
        Data::Int(i) => Cell::Num(*i as f64),
        Data::Float(f) => Cell::Num(*f),
        Data::Bool(b) => Cell::Text(b.to_string()),
        Data::Empty => Cell::Empty,
        other => {
            let s = other.to_string();
            if s.trim().is_empty() {
                Cell::Empty
            } else if let Some(n) = parse_number(&s) {
                // A number written as text is still a number (Complex_D).
                Cell::Num(n)
            } else {
                Cell::Text(s.trim().to_string())
            }
        }
    }
}

/// Does this row look like a header rather than data?
fn looks_like_header(row: &[Cell]) -> bool {
    let filled: Vec<&Cell> = row.iter().filter(|c| !c.is_empty()).collect();
    if filled.is_empty() {
        return false;
    }
    let text = filled.iter().filter(|c| matches!(c, Cell::Text(_))).count();
    text * 2 >= filled.len()
}

/// Flatten one or two header rows into column names.
///
/// A merged header cell (`Year-I` spanning Total/Local/FEC in the PC-I phasing
/// table) is stored only in its top-left position; calamine reports the rest of
/// the span as Empty. Filling forward reconstructs the span, so the two rows
/// combine into `Year-I · Total`, `Year-I · Local`, … Without this, a two-row
/// header collapses and every figure under it is attributed to the wrong year.
fn build_headers(rows: &[Vec<Cell>], start: usize, width: usize) -> (Vec<String>, usize) {
    let first = &rows[start];

    // A row containing NUMBERS is data, not a header. Some sheets have no
    // header at all — the PC-I "Financing Plan" is label/value pairs
    // (`Equity - Provincial Government | 500`) — and reading its first row as a
    // header produced a column literally named "500", then summed the remaining
    // rows against the printed total and reported a 500-million discrepancy
    // that does not exist. Fall back to positional names and keep every row.
    if first.iter().take(width).any(|c| matches!(c, Cell::Num(_))) {
        let names = (0..width).map(|i| format!("Column {}", i + 1)).collect();
        return (names, start);
    }

    let second = rows.get(start + 1);

    // A two-row header only exists when the FIRST row has gaps — a merged span
    // stores its value in the top-left and leaves the rest Empty. A flat header
    // (`Date | Department | … | Amount | Status`) has no gaps, and without this
    // test its first DATA row was mistaken for a sub-header: the row was
    // swallowed (239 of 240 counted) and the columns came out named
    // "Amount · 1241226".
    let filled_first = first.iter().take(width).filter(|c| !c.is_empty()).count();
    let has_span_gap = filled_first > 0 && filled_first < width;

    let two_row = has_span_gap
        && second
            .map(|r| looks_like_header(r) && r.iter().filter(|c| !c.is_empty()).count() > width / 2)
            .unwrap_or(false);

    let mut spans: Vec<String> = Vec::with_capacity(width);
    let mut carry = String::new();
    for i in 0..width {
        let here = first.get(i).map(|c| c.text()).unwrap_or_default();
        if !here.trim().is_empty() {
            carry = here.trim().to_string();
        }
        spans.push(carry.clone());
    }

    if !two_row {
        // Single header row: blank names become positional so a column is still
        // addressable rather than silently unnamed.
        let names = spans
            .iter()
            .enumerate()
            .map(|(i, s)| if s.trim().is_empty() { format!("Column {}", i + 1) } else { s.clone() })
            .collect();
        return (names, start + 1);
    }

    let sub = second.unwrap();
    let mut names = Vec::with_capacity(width);
    for i in 0..width {
        let leaf = sub.get(i).map(|c| c.text()).unwrap_or_default();
        let leaf = leaf.trim();
        let span = spans[i].trim();
        names.push(match (span.is_empty(), leaf.is_empty()) {
            (false, false) => format!("{span} · {leaf}"),
            (false, true) => span.to_string(),
            (true, false) => leaf.to_string(),
            (true, true) => format!("Column {}", i + 1),
        });
    }
    (names, start + 2)
}

/// Read every sheet in a workbook into a typed [`Table`].
pub fn read_workbook(path: &Path) -> Result<Vec<Table>, String> {
    let mut wb: calamine::Xlsx<_> =
        calamine::open_workbook(path).map_err(|e| format!("calamine: {e}"))?;
    let names: Vec<String> = wb.sheet_names().to_vec();
    let mut out = Vec::new();

    for name in names {
        let Ok(range) = wb.worksheet_range(&name) else { continue };
        let grid: Vec<Vec<Cell>> =
            range.rows().map(|r| r.iter().map(cell_from).collect()).collect();
        let width = grid.iter().map(|r| r.len()).max().unwrap_or(0);
        if width == 0 {
            continue;
        }

        // Skip leading blanks and a merged title line ("Departmental
        // Expenditure Statement…"), which is one filled cell on an otherwise
        // empty row and is NOT the header.
        let mut start = 0;
        while start < grid.len() {
            let filled = grid[start].iter().filter(|c| !c.is_empty()).count();
            if filled > 1 {
                break;
            }
            start += 1;
        }
        if start >= grid.len() {
            continue;
        }

        let (headers, data_start) = build_headers(&grid, start, width);
        // Stop at the first blank row once data has begun. A departmental return
        // puts its footnote ("* Provisional figures") and trailing lines
        // ("Utilisation … 94.2%") below a blank separator, and counting them
        // added 94.2 to a total of 2,390.45 — an answer that was computed, and
        // wrong. A blank row is the conventional end of a table.
        let mut rows: Vec<Vec<Cell>> = Vec::new();
        for r in &grid[data_start..] {
            if r.iter().all(|c| c.is_empty()) {
                if !rows.is_empty() {
                    break;
                }
                continue;
            }
            rows.push(r.clone());
        }
        out.push(Table { name, headers, rows });
    }

    if out.is_empty() {
        return Err("workbook has no readable sheets".into());
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accounting_negatives_and_currency() {
        assert_eq!(parse_number("Rs. 1,250.00"), Some(1250.0));
        assert_eq!(parse_number("(45.20)"), Some(-45.2));
        assert_eq!(parse_number("12.5%"), Some(12.5));
        assert_eq!(parse_number("1,234"), Some(1234.0));
        assert_eq!(parse_number("-4118"), Some(-4118.0));
    }

    #[test]
    fn text_that_merely_contains_digits_is_not_a_number() {
        assert_eq!(parse_number("Vendor-01"), None);
        assert_eq!(parse_number("2024-07-01"), None);
        assert_eq!(parse_number("Jan"), None);
        assert_eq!(parse_number(""), None);
    }

    #[test]
    fn merged_span_fills_forward_into_two_row_headers() {
        // The PC-I shape: `Year-I` spans three columns, reported by calamine as
        // one value followed by Empty.
        let rows = vec![
            vec![
                Cell::Text("Items".into()),
                Cell::Text("Year-I".into()),
                Cell::Empty,
                Cell::Empty,
                Cell::Text("Year-II".into()),
                Cell::Empty,
                Cell::Empty,
            ],
            vec![
                Cell::Empty,
                Cell::Text("Total".into()),
                Cell::Text("Local".into()),
                Cell::Text("FEC".into()),
                Cell::Text("Total".into()),
                Cell::Text("Local".into()),
                Cell::Text("FEC".into()),
            ],
            vec![Cell::Text("A".into()), Cell::Num(100.0), Cell::Num(70.0), Cell::Num(30.0),
                 Cell::Num(150.0), Cell::Num(100.0), Cell::Num(50.0)],
        ];
        let (headers, start) = build_headers(&rows, 0, 7);
        assert_eq!(start, 2);
        assert_eq!(headers[1], "Year-I · Total");
        assert_eq!(headers[3], "Year-I · FEC");
        assert_eq!(headers[4], "Year-II · Total");
    }

    #[test]
    fn a_sheet_with_no_header_keeps_all_its_rows() {
        // PC-I "Financing Plan": label/value pairs, no header row at all.
        let rows = vec![
            vec![Cell::Text("Equity - Provincial Government".into()), Cell::Num(500.0)],
            vec![Cell::Text("Debt - Local".into()), Cell::Num(120.0)],
            vec![Cell::Text("Total".into()), Cell::Num(850.0)],
        ];
        let (headers, start) = build_headers(&rows, 0, 2);
        assert_eq!(start, 0, "the first row is DATA and must not be consumed");
        assert_eq!(headers, vec!["Column 1", "Column 2"]);
    }

    #[test]
    fn total_rows_are_recognised_so_they_are_never_double_counted() {
        let t = Table {
            name: "s".into(),
            headers: vec!["Item".into(), "Amount".into()],
            rows: vec![
                vec![Cell::Text("A".into()), Cell::Num(1.0)],
                vec![Cell::Text("Total".into()), Cell::Num(1.0)],
            ],
        };
        assert!(!t.is_total_row(&t.rows[0]));
        assert!(t.is_total_row(&t.rows[1]));
    }

    #[test]
    fn columns_resolve_case_and_space_insensitively() {
        let t = Table {
            name: "s".into(),
            headers: vec!["Month".into(), "Year-I · Total".into()],
            rows: vec![],
        };
        assert_eq!(t.column("month"), Some(0));
        assert_eq!(t.column("MONTH"), Some(0));
        assert_eq!(t.column("Year-I · Total"), Some(1));
    }
}
