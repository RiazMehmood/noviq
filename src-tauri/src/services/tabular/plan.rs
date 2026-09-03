//! The **query plan** a spreadsheet question is turned into, and its
//! deterministic executor.
//!
//! The division of labour is the whole point, and it is the same one ADR-0019
//! drew for [`counting`](crate::services::counting): the model decides *what to
//! compute*, and Rust *computes it*. A model asked to add ten six-digit numbers
//! gets it wrong — measured on prod 2026-08-03, 19/53, including
//! `107506 + 107559 = 415065`. A model asked which column to sum gets that
//! right, because it is a reading task.
//!
//! So the model emits one small JSON object and never sees an arithmetic
//! result it is expected to reproduce. Everything below is pure and total:
//! no model, no network, no database.

use serde::{Deserialize, Serialize};

use super::sheet::{Cell, Table};

/// What to compute. Deliberately a closed set — an open-ended expression
/// language would need sandboxing, and nothing in the 2026-08-03 fixture suite
/// (53 questions) asks for anything outside this.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Op {
    Sum,
    Avg,
    Min,
    Max,
    Count,
    CountDistinct,
    /// Label of the row holding the largest value of `column`.
    ArgMax,
    /// Label of the row holding the smallest value of `column`.
    ArgMin,
    /// One cell: `column` where `key_column` == `key`.
    Lookup,
    /// `column` ÷ `denominator`, on a single selected row or over the totals.
    Ratio,
    /// Every value of `column`, filtered — for "which items are overspent".
    List,
    /// `column` summed per distinct value of `group_by`, largest first.
    GroupSum,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Filter {
    pub column: String,
    /// One of `> >= < <= == != contains`.
    pub op: String,
    /// Compared numerically when both sides parse as numbers, else as text.
    pub value: String,
}

/// How to pick the single row a `Lookup` or `Ratio` refers to.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RowSelector {
    /// Row whose `key_column` equals `key`.
    Key { key_column: String, key: String },
    /// Row with the largest value in `column`.
    MaxOf { column: String },
    /// Row with the smallest value in `column`.
    MinOf { column: String },
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Plan {
    pub op: Op,
    /// Sheet name; omitted means "the first sheet with the named column".
    #[serde(default)]
    pub sheet: Option<String>,
    #[serde(default)]
    pub column: Option<String>,
    /// Column supplying the label for `ArgMax` / `ArgMin` / `List`.
    #[serde(default)]
    pub label_column: Option<String>,
    #[serde(default)]
    pub denominator: Option<String>,
    #[serde(default)]
    pub group_by: Option<String>,
    #[serde(default)]
    pub filter: Option<Filter>,
    #[serde(default)]
    pub row: Option<RowSelector>,
    /// Multiply the result by 100 and render as a percentage.
    #[serde(default)]
    pub as_percent: bool,
}

/// A computed answer plus the evidence for it.
#[derive(Debug, Clone)]
pub struct Computed {
    /// Rendered result, ready to put in a sentence.
    pub value: String,
    /// How it was derived, in the officer's terms.
    pub workings: String,
    /// How many data rows contributed.
    pub rows_used: usize,
}

fn cmp_ok(op: &str, left: &Cell, want: &str) -> bool {
    let rn = super::sheet::parse_number(want);
    if let (Some(l), Some(r)) = (left.as_num(), rn) {
        return match op {
            ">" => l > r,
            ">=" => l >= r,
            "<" => l < r,
            "<=" => l <= r,
            "==" | "=" => (l - r).abs() < 1e-9,
            "!=" => (l - r).abs() >= 1e-9,
            _ => false,
        };
    }
    let l = left.text().trim().to_lowercase();
    let r = want.trim().to_lowercase();
    match op {
        "==" | "=" => l == r,
        "!=" => l != r,
        "contains" => l.contains(&r),
        // A text value under a numeric comparison is not a match, rather than
        // an error: mixed columns are normal in real returns.
        _ => false,
    }
}

/// Rows passing the filter, with totals lines removed.
///
/// Excluding the printed `Total` row is not a nicety. On `Complex_B` the model
/// reported the printed total (845) instead of the items' true sum (850); a sum
/// that includes the totals line is simply double.
fn eligible<'a>(t: &'a Table, f: &Option<Filter>) -> Result<Vec<&'a Vec<Cell>>, String> {
    let fi = match f {
        Some(f) => Some((
            t.column(&f.column).ok_or_else(|| format!("no column named '{}'", f.column))?,
            f,
        )),
        None => None,
    };
    Ok(t.rows
        .iter()
        .filter(|r| !t.is_total_row(r))
        .filter(|r| match &fi {
            None => true,
            Some((idx, f)) => cmp_ok(&f.op, r.get(*idx).unwrap_or(&Cell::Empty), &f.value),
        })
        .collect())
}

fn nums(rows: &[&Vec<Cell>], idx: usize) -> Vec<f64> {
    rows.iter().filter_map(|r| r.get(idx).and_then(|c| c.as_num())).collect()
}

fn fmt(v: f64, as_percent: bool) -> String {
    let v = if as_percent { v * 100.0 } else { v };
    let s = if (v.fract()).abs() < 1e-9 {
        // Thousands separators: an officer reads 1,273,872, not 1273872.
        let neg = v < 0.0;
        let mut d = format!("{}", v.abs() as i64);
        let mut out = String::new();
        while d.len() > 3 {
            let cut = d.len() - 3;
            out = format!(",{}{}", &d[cut..], out);
            d.truncate(cut);
        }
        format!("{}{}{}", if neg { "-" } else { "" }, d, out)
    } else {
        format!("{:.2}", v)
    };
    if as_percent { format!("{s}%") } else { s }
}

fn pick_table<'a>(tables: &'a [Table], plan: &Plan) -> Result<&'a Table, String> {
    if let Some(name) = &plan.sheet {
        let want = name.to_lowercase();
        if let Some(t) = tables.iter().find(|t| t.name.to_lowercase() == want) {
            return Ok(t);
        }
    }
    // No sheet named, or the named one does not exist: take the first sheet
    // that actually has the column being asked about.
    if let Some(col) = &plan.column {
        if let Some(t) = tables.iter().find(|t| t.column(col).is_some() && !t.rows.is_empty()) {
            return Ok(t);
        }
    }
    tables
        .iter()
        .max_by_key(|t| t.rows.len())
        .ok_or_else(|| "workbook has no data".to_string())
}

/// Execute a plan. Pure; the only failure is a plan that does not fit the sheet.
pub fn execute(tables: &[Table], plan: &Plan) -> Result<Computed, String> {
    let t = pick_table(tables, plan)?;
    let rows = eligible(t, &plan.filter)?;
    let filter_note = plan
        .filter
        .as_ref()
        .map(|f| format!(" where {} {} {}", f.column, f.op, f.value))
        .unwrap_or_default();

    let col_idx = |name: &Option<String>| -> Result<usize, String> {
        let n = name.as_ref().ok_or("plan is missing 'column'")?;
        t.column(n).ok_or_else(|| format!("no column named '{n}' in sheet '{}'", t.name))
    };

    match plan.op {
        Op::Count => Ok(Computed {
            value: fmt(rows.len() as f64, false),
            workings: format!("counted {} rows{}", rows.len(), filter_note),
            rows_used: rows.len(),
        }),
        Op::CountDistinct => {
            let i = col_idx(&plan.column)?;
            let mut seen: Vec<String> = rows
                .iter()
                .map(|r| r.get(i).map(|c| c.text()).unwrap_or_default())
                .filter(|s| !s.trim().is_empty())
                .collect();
            seen.sort();
            seen.dedup();
            Ok(Computed {
                value: fmt(seen.len() as f64, false),
                workings: format!("distinct values of {}{}", plan.column.clone().unwrap_or_default(), filter_note),
                rows_used: rows.len(),
            })
        }
        Op::Sum | Op::Avg | Op::Min | Op::Max => {
            let i = col_idx(&plan.column)?;
            let v = nums(&rows, i);
            if v.is_empty() {
                return Err(format!("column '{}' has no numeric values", plan.column.clone().unwrap_or_default()));
            }
            let (val, word) = match plan.op {
                Op::Sum => (v.iter().sum::<f64>(), "sum"),
                Op::Avg => (v.iter().sum::<f64>() / v.len() as f64, "average"),
                Op::Min => (v.iter().cloned().fold(f64::INFINITY, f64::min), "minimum"),
                _ => (v.iter().cloned().fold(f64::NEG_INFINITY, f64::max), "maximum"),
            };
            Ok(Computed {
                value: fmt(val, plan.as_percent),
                workings: format!(
                    "{word} of {} over {} rows{}",
                    plan.column.clone().unwrap_or_default(), v.len(), filter_note
                ),
                rows_used: v.len(),
            })
        }
        Op::ArgMax | Op::ArgMin => {
            let i = col_idx(&plan.column)?;
            let label = plan
                .label_column
                .as_ref()
                .and_then(|l| t.column(l))
                .unwrap_or(0);
            let mut best: Option<(f64, String)> = None;
            for r in &rows {
                let Some(n) = r.get(i).and_then(|c| c.as_num()) else { continue };
                let lbl = r.get(label).map(|c| c.text()).unwrap_or_default();
                let take = match (&best, plan.op) {
                    (None, _) => true,
                    (Some((b, _)), Op::ArgMax) => n > *b,
                    (Some((b, _)), _) => n < *b,
                };
                if take {
                    best = Some((n, lbl));
                }
            }
            let (n, lbl) = best.ok_or("no numeric values to compare")?;
            Ok(Computed {
                value: format!("{lbl} ({})", fmt(n, plan.as_percent)),
                workings: format!(
                    "{} of {} across {} rows{}",
                    if plan.op == Op::ArgMax { "largest" } else { "smallest" },
                    plan.column.clone().unwrap_or_default(), rows.len(), filter_note
                ),
                rows_used: rows.len(),
            })
        }
        Op::Lookup => {
            let i = col_idx(&plan.column)?;
            let r = select_row(t, &rows, plan)?;
            let c = r.get(i).cloned().unwrap_or(Cell::Empty);
            Ok(Computed {
                value: match c {
                    Cell::Num(n) => fmt(n, plan.as_percent),
                    other => other.text(),
                },
                workings: format!("read {} from the selected row", plan.column.clone().unwrap_or_default()),
                rows_used: 1,
            })
        }
        Op::Ratio => {
            let num_i = col_idx(&plan.column)?;
            let den_name = plan.denominator.as_ref().ok_or("ratio needs 'denominator'")?;
            let den_i = t
                .column(den_name)
                .ok_or_else(|| format!("no column named '{den_name}'"))?;
            let (n, d, note) = if plan.row.is_some() {
                let r = select_row(t, &rows, plan)?;
                (
                    r.get(num_i).and_then(|c| c.as_num()).ok_or("numerator is not numeric")?,
                    r.get(den_i).and_then(|c| c.as_num()).ok_or("denominator is not numeric")?,
                    "on the selected row".to_string(),
                )
            } else {
                let n: f64 = nums(&rows, num_i).iter().sum();
                let d: f64 = nums(&rows, den_i).iter().sum();
                (n, d, format!("over {} rows", rows.len()))
            };
            if d.abs() < 1e-12 {
                return Err("denominator is zero".into());
            }
            Ok(Computed {
                value: fmt(n / d, plan.as_percent),
                workings: format!("{} ÷ {} {note}", fmt(n, false), fmt(d, false)),
                rows_used: rows.len(),
            })
        }
        Op::List => {
            let label = plan
                .label_column
                .as_ref()
                .or(plan.column.as_ref())
                .and_then(|l| t.column(l))
                .unwrap_or(0);
            let items: Vec<String> =
                rows.iter().map(|r| r.get(label).map(|c| c.text()).unwrap_or_default()).collect();
            Ok(Computed {
                value: if items.is_empty() { "none".into() } else { items.join(", ") },
                workings: format!("{} rows{}", items.len(), filter_note),
                rows_used: items.len(),
            })
        }
        Op::GroupSum => {
            let i = col_idx(&plan.column)?;
            let g = plan.group_by.as_ref().ok_or("group_sum needs 'group_by'")?;
            let gi = t.column(g).ok_or_else(|| format!("no column named '{g}'"))?;
            let mut acc: Vec<(String, f64)> = Vec::new();
            for r in &rows {
                let key = r.get(gi).map(|c| c.text()).unwrap_or_default();
                let v = r.get(i).and_then(|c| c.as_num()).unwrap_or(0.0);
                match acc.iter_mut().find(|(k, _)| *k == key) {
                    Some((_, a)) => *a += v,
                    None => acc.push((key, v)),
                }
            }
            acc.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
            let rendered: Vec<String> =
                acc.iter().map(|(k, v)| format!("{k}: {}", fmt(*v, false))).collect();
            Ok(Computed {
                value: rendered.join("; "),
                workings: format!("{} summed by {}{}", plan.column.clone().unwrap_or_default(), g, filter_note),
                rows_used: rows.len(),
            })
        }
    }
}

fn select_row<'a>(
    t: &Table,
    rows: &'a [&'a Vec<Cell>],
    plan: &Plan,
) -> Result<&'a Vec<Cell>, String> {
    let sel = plan.row.as_ref().ok_or("this operation needs a 'row' selector")?;
    match sel {
        RowSelector::Key { key_column, key } => {
            let ki = t.column(key_column).ok_or_else(|| format!("no column '{key_column}'"))?;
            rows.iter()
                .find(|r| {
                    r.get(ki)
                        .map(|c| c.text().trim().eq_ignore_ascii_case(key.trim()))
                        .unwrap_or(false)
                })
                .copied()
                // A prefix match rescues "March" against a "Mar" column.
                .or_else(|| {
                    rows.iter()
                        .find(|r| {
                            let v = r.get(ki).map(|c| c.text()).unwrap_or_default();
                            let (v, k) = (v.trim().to_lowercase(), key.trim().to_lowercase());
                            !v.is_empty() && (k.starts_with(&v) || v.starts_with(&k))
                        })
                        .copied()
                })
                .ok_or_else(|| format!("no row where {key_column} is '{key}'"))
        }
        RowSelector::MaxOf { column } | RowSelector::MinOf { column } => {
            let ci = t.column(column).ok_or_else(|| format!("no column '{column}'"))?;
            let want_max = matches!(sel, RowSelector::MaxOf { .. });
            rows.iter()
                .filter(|r| r.get(ci).and_then(|c| c.as_num()).is_some())
                .copied()
                .reduce(|a, b| {
                    let (x, y) = (
                        a.get(ci).and_then(|c| c.as_num()).unwrap_or(0.0),
                        b.get(ci).and_then(|c| c.as_num()).unwrap_or(0.0),
                    );
                    if (want_max && y > x) || (!want_max && y < x) { b } else { a }
                })
                .ok_or_else(|| format!("column '{column}' has no numeric values"))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::services::tabular::sheet::Cell;

    /// The exact Financial_Test_1 data the model failed on in prod.
    fn fixture() -> Vec<Table> {
        let raw = [
            ("Jan", 107506.0, 39561.0, 15228.0, 52717.0),
            ("Feb", 107559.0, 39512.0, 15619.0, 52428.0),
            ("Mar", 138860.0, 48135.0, 24456.0, 66269.0),
            ("Apr", 93173.0, 46775.0, 43262.0, 3136.0),
            ("May", 145544.0, 46772.0, 11470.0, 87302.0),
            ("Jun", 141298.0, 89983.0, 27201.0, 24114.0),
            ("Jul", 101191.0, 76394.0, 28915.0, -4118.0),
            ("Aug", 93819.0, 85581.0, 17264.0, -9026.0),
            ("Sep", 172535.0, 78286.0, 29993.0, 64256.0),
            ("Oct", 172387.0, 64653.0, 13670.0, 94064.0),
        ];
        vec![Table {
            name: "Data".into(),
            headers: vec!["Month".into(), "Revenue".into(), "COGS".into(), "Opex".into(), "Profit".into()],
            rows: raw
                .iter()
                .map(|(m, r, c, o, p)| {
                    vec![Cell::Text((*m).into()), Cell::Num(*r), Cell::Num(*c), Cell::Num(*o), Cell::Num(*p)]
                })
                .collect(),
        }]
    }

    fn plan(json: &str) -> Plan {
        serde_json::from_str(json).expect("plan parses")
    }

    #[test]
    fn total_revenue_is_exact() {
        // Prod answered 1,263,651 — wrong by 10,221, and cited.
        let c = execute(&fixture(), &plan(r#"{"op":"sum","column":"Revenue"}"#)).unwrap();
        assert_eq!(c.value, "1,273,872");
        assert_eq!(c.rows_used, 10);
    }

    #[test]
    fn argmax_beats_the_model() {
        // Prod said Sep (64,256); Oct is 94,064.
        let c = execute(
            &fixture(),
            &plan(r#"{"op":"arg_max","column":"Profit","label_column":"Month"}"#),
        )
        .unwrap();
        assert_eq!(c.value, "Oct (94,064)");
    }

    #[test]
    fn argmin_beats_the_model() {
        // Prod said Jan (107,506); Apr is 93,173.
        let c = execute(
            &fixture(),
            &plan(r#"{"op":"arg_min","column":"Revenue","label_column":"Month"}"#),
        )
        .unwrap();
        assert_eq!(c.value, "Apr (93,173)");
    }

    #[test]
    fn average_and_counts() {
        let c = execute(&fixture(), &plan(r#"{"op":"avg","column":"Profit"}"#)).unwrap();
        assert_eq!(c.value, "43114.20");
        let c = execute(
            &fixture(),
            &plan(r#"{"op":"count","filter":{"column":"Profit","op":">","value":"50000"}}"#),
        )
        .unwrap();
        assert_eq!(c.value, "6");
    }

    #[test]
    fn lookup_by_key_tolerates_march_for_mar() {
        let c = execute(
            &fixture(),
            &plan(r#"{"op":"lookup","column":"Profit","row":{"key":{"key_column":"Month","key":"March"}}}"#),
        )
        .unwrap();
        assert_eq!(c.value, "66,269");
    }

    #[test]
    fn margin_of_the_best_month_selects_then_divides() {
        // Prod chained two errors here: wrong best month, then a wrong ratio.
        let c = execute(
            &fixture(),
            &plan(
                r#"{"op":"ratio","column":"Profit","denominator":"Revenue",
                    "row":{"max_of":{"column":"Profit"}},"as_percent":true}"#,
            ),
        )
        .unwrap();
        assert_eq!(c.value, "54.57%");
    }

    #[test]
    fn totals_row_is_excluded_from_sums() {
        let mut t = fixture();
        t[0].rows.push(vec![
            Cell::Text("Total".into()),
            Cell::Num(1_273_872.0),
            Cell::Num(0.0),
            Cell::Num(0.0),
            Cell::Num(0.0),
        ]);
        let c = execute(&t, &plan(r#"{"op":"sum","column":"Revenue"}"#)).unwrap();
        assert_eq!(c.value, "1,273,872", "a printed Total row must not double the sum");
    }

    #[test]
    fn a_plan_that_does_not_fit_the_sheet_is_an_error_not_a_guess() {
        let e = execute(&fixture(), &plan(r#"{"op":"sum","column":"Expenditure"}"#));
        assert!(e.is_err(), "must refuse rather than silently sum the wrong column");
    }
}
