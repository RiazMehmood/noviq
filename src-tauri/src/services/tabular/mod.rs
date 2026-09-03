//! Deterministic computation over an attached spreadsheet.
//!
//! Measured on prod 2026-08-03: 19 of 53 spreadsheet questions correct, with
//! wrong figures presented in bold and carrying a citation mark. The causes
//! were not retrieval — the whole of `Financial_Test_1` fits in ONE chunk, so
//! the model received every row and still returned `1,263,651` for a revenue
//! total of `1,273,872`. Asked `107506 + 107559` with no document at all, it
//! answered `415065`.
//!
//! The fix is therefore not a better prompt or better chunking. It is to stop
//! asking the model to calculate:
//!
//! 1. [`sheet`]  — parse the workbook into a **typed** table (keeping the types
//!    calamine already knows, and recovering numbers written as text).
//! 2. [`plan`]   — the model reads the question and the column names and emits
//!    one small JSON plan. That is a reading task, which it is good at.
//! 3. [`plan::execute`] — Rust computes the answer. Pure, total, unit-tested.
//! 4. [`checks`] — structural invariants that need no model and no question.
//!
//! Same division of labour as ADR-0019's count capability: *the model decides
//! what to compute; the code computes it.*

pub mod plan;
pub mod planner;
pub mod sheet;

pub use plan::{execute, Computed, Plan};
pub use planner::from_question;
pub use sheet::{read_workbook, Cell, Table};

/// One failed arithmetic invariant found in the sheet itself.
#[derive(Debug, Clone)]
pub struct CheckFinding {
    pub sheet: String,
    pub what: String,
    pub stated: f64,
    pub computed: f64,
}

impl CheckFinding {
    pub fn line(&self) -> String {
        format!(
            "{} — {}: stated {}, actual {} (difference {})",
            self.sheet,
            self.what,
            num(self.stated),
            num(self.computed),
            num(self.computed - self.stated)
        )
    }
}

fn num(v: f64) -> String {
    if (v.fract()).abs() < 1e-9 { format!("{}", v as i64) } else { format!("{v:.2}") }
}

/// Arithmetic the sheet asserts about itself, verified.
///
/// Needs no question and no model: if a sheet prints a `Total` row, that row is
/// a claim, and a claim can be checked. On `Complex_B_PC1_Phasing` the printed
/// grand total is 845 where the items sum to 850 — the model reported 845 as
/// "the correct total capital cost", because it had no reason not to believe
/// the sheet.
///
/// Only columns that are numeric in BOTH the total row and the item rows are
/// checked, so a label column or a stray note never raises a false finding.
pub fn cross_foot(tables: &[Table]) -> Vec<CheckFinding> {
    let mut out = Vec::new();
    for t in tables {
        let Some(total_row) = t.rows.iter().find(|r| t.is_total_row(r)) else { continue };
        let items: Vec<&Vec<Cell>> = t.rows.iter().filter(|r| !t.is_total_row(r)).collect();
        if items.is_empty() {
            continue;
        }
        for (i, header) in t.headers.iter().enumerate() {
            let Some(stated) = total_row.get(i).and_then(|c| c.as_num()) else { continue };
            let vals: Vec<f64> =
                items.iter().filter_map(|r| r.get(i).and_then(|c| c.as_num())).collect();
            // Require most item rows to have a value, else this is not a column
            // the total row is summing.
            if vals.len() * 2 < items.len() {
                continue;
            }
            let computed: f64 = vals.iter().sum();
            if (computed - stated).abs() > 0.005 {
                out.push(CheckFinding {
                    sheet: t.name.clone(),
                    what: format!("column '{header}' total"),
                    stated,
                    computed,
                });
            }
        }
    }
    out.extend(row_cross_foot(tables));
    out
}

fn span_of(header: &str) -> String {
    header.split('·').next().unwrap_or("").trim().to_lowercase()
}

fn leaf_of(header: &str) -> String {
    header.rsplit('·').next().unwrap_or("").trim().to_lowercase()
}

fn is_grand_span(s: &str) -> bool {
    matches!(s, "total" | "totals" | "grand total" | "overall")
}

/// Rows whose own Total column disagrees with the periods that make it up.
///
/// [`cross_foot`] checks a printed total row against the items ABOVE it — a
/// vertical check. A PC-I phasing table also asserts a HORIZONTAL identity:
/// `Year-I · Total + Year-II · Total + Year-III · Total` must equal
/// `Total · Total`, for every item. Without this, item B printing 295 where its
/// years sum to 300 went unnoticed, the capital cost came out 845 instead of
/// 850, and every figure derived from it inherited the error.
pub fn row_cross_foot(tables: &[Table]) -> Vec<CheckFinding> {
    let mut out = Vec::new();
    for t in tables {
        // leaf -> (grand column index, per-period column indices)
        let mut groups: Vec<(String, Option<usize>, Vec<usize>)> = Vec::new();
        for (i, h) in t.headers.iter().enumerate() {
            if !h.contains('·') {
                continue; // no span structure to check
            }
            let (span, leaf) = (span_of(h), leaf_of(h));
            let entry = match groups.iter_mut().find(|(l, _, _)| *l == leaf) {
                Some(e) => e,
                None => {
                    groups.push((leaf.clone(), None, Vec::new()));
                    groups.last_mut().unwrap()
                }
            };
            if is_grand_span(&span) {
                entry.1 = Some(i);
            } else {
                entry.2.push(i);
            }
        }

        for (leaf, grand, periods) in groups {
            let (Some(gi), true) = (grand, periods.len() > 1) else { continue };
            for row in &t.rows {
                let Some(stated) = row.get(gi).and_then(|c| c.as_num()) else { continue };
                let parts: Vec<f64> =
                    periods.iter().filter_map(|i| row.get(*i).and_then(|c| c.as_num())).collect();
                if parts.len() != periods.len() {
                    continue; // an incomplete row is not a failed identity
                }
                let computed: f64 = parts.iter().sum();
                if (computed - stated).abs() > 0.005 {
                    let label = row.first().map(|c| c.text()).unwrap_or_default();
                    out.push(CheckFinding {
                        sheet: t.name.clone(),
                        what: format!("row '{label}' — '{leaf}' across the periods"),
                        stated,
                        computed,
                    });
                }
            }
        }
    }
    out
}

/// Figures computed from the workbook, for a turn that ALSO needs the corpus.
///
/// "Which forum approves this project?" cannot be answered by either half
/// alone: the cost and the foreign-exchange share come from the attached PC-I,
/// and the threshold that reads them comes from the indexed Manual. This block
/// supplies the first half **already computed**, so the model is left with a
/// comparison against a retrieved rule rather than any arithmetic.
///
/// Shares are emitted for the leaves of a shared header span — in the PC-I
/// phasing table `Total · Total`, `Total · Local`, `Total · FEC` — because
/// "X as a percentage of the total" is precisely what a threshold tests, and
/// leaving the model to divide 230 by 850 is how it answered "the FEC share is
/// 230 million Rs" on 2026-08-03.
pub fn facts(tables: &[Table]) -> String {
    let mut lines: Vec<String> = Vec::new();

    for t in tables {
        let items: Vec<&Vec<Cell>> = t.rows.iter().filter(|r| !t.is_total_row(r)).collect();
        if items.is_empty() {
            continue;
        }
        // span -> (leaf, column sum)
        let mut spans: Vec<(String, Vec<(String, f64)>)> = Vec::new();
        for (i, header) in t.headers.iter().enumerate() {
            let vals: Vec<f64> =
                items.iter().filter_map(|r| r.get(i).and_then(|c| c.as_num())).collect();
            if vals.len() * 2 < items.len() {
                continue; // not a numeric column
            }
            let total: f64 = vals.iter().sum();
            lines.push(format!(
                "- {} · {}: {} (sum of {} rows)",
                t.name, header, num(total), vals.len()
            ));
            let (span, leaf) = match header.split_once('·') {
                Some((a, b)) => (a.trim().to_string(), b.trim().to_lowercase()),
                None => (String::new(), header.trim().to_lowercase()),
            };
            if span.is_empty() {
                continue;
            }
            match spans.iter_mut().find(|(s, _)| *s == span) {
                Some((_, v)) => v.push((leaf, total)),
                None => spans.push((span, vec![(leaf, total)])),
            }
        }

        for (span, leaves) in &spans {
            let Some((_, whole)) = leaves.iter().find(|(l, _)| l == "total") else { continue };
            if whole.abs() < 1e-9 {
                continue;
            }
            for (leaf, v) in leaves {
                if leaf == "total" {
                    continue;
                }
                lines.push(format!(
                    "- {span} · {leaf} as a share of {span} · total: {:.2}% ({} of {})",
                    v / whole * 100.0,
                    num(*v),
                    num(*whole)
                ));
            }
        }
    }

    for f in cross_foot(tables) {
        lines.push(format!("- DISCREPANCY: {}", f.line()));
    }

    if lines.is_empty() {
        return String::new();
    }
    format!(
        "\n\nFigures computed from the attached spreadsheet (exact, computed by the \
         system — use these rather than adding anything up yourself):\n{}\n",
        lines.join("\n")
    )
}

/// A compact description of the workbook for the model to plan against.
///
/// Deliberately **schema plus a few sample rows, never the whole sheet**. A
/// 240-row sheet put in the prompt exceeded `max_model_len` and every turn died
/// with HTTP 400 in ~15 ms (prod, `Complex_A_Scale`, 0/8). It is also the
/// stronger privacy position: the figures stay on the box, and only the column
/// names cross to the engine.
pub fn describe(tables: &[Table], sample_rows: usize) -> String {
    let mut s = String::new();
    for t in tables {
        s.push_str(&format!(
            "Sheet \"{}\" ({} data rows)\n  columns: {}\n",
            t.name,
            t.rows.len(),
            t.headers.join(" | ")
        ));
        for r in t.rows.iter().take(sample_rows) {
            let cells: Vec<String> = r.iter().map(|c| c.text()).collect();
            s.push_str(&format!("  example: {}\n", cells.join(" | ")));
        }
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    fn phasing() -> Vec<Table> {
        // Complex_B: items sum to 850, the printed Total row says 845.
        Vec::from([Table {
            name: "Capital Cost Phasing".into(),
            headers: vec!["Items".into(), "Total".into()],
            rows: vec![
                vec![Cell::Text("A".into()), Cell::Num(300.0)],
                vec![Cell::Text("B".into()), Cell::Num(300.0)],
                vec![Cell::Text("C".into()), Cell::Num(120.0)],
                vec![Cell::Text("D".into()), Cell::Num(130.0)],
                vec![Cell::Text("Total".into()), Cell::Num(845.0)],
            ],
        }])
    }

    #[test]
    fn a_misprinted_total_is_caught() {
        let f = cross_foot(&phasing());
        assert_eq!(f.len(), 1);
        assert_eq!(f[0].stated, 845.0);
        assert_eq!(f[0].computed, 850.0);
        assert!(f[0].line().contains("difference 5"));
    }

    #[test]
    fn a_correct_total_raises_nothing() {
        let mut t = phasing();
        let last = t[0].rows.len() - 1;
        t[0].rows[last][1] = Cell::Num(850.0);
        assert!(cross_foot(&t).is_empty());
    }

    #[test]
    fn facts_carry_the_share_a_threshold_tests() {
        // The PC-I shape: leaves of one span, where the threshold reads FEC as a
        // percentage of the total. Asked to divide 230 by 850 the model replied
        // "the FEC share of total cost is 230 million Rs", so the share has to
        // arrive already computed.
        let t = vec![Table {
            name: "Capital Cost Phasing".into(),
            headers: vec![
                "Items".into(),
                "Total · Total".into(),
                "Total · Local".into(),
                "Total · FEC".into(),
            ],
            rows: vec![
                vec![Cell::Text("A".into()), Cell::Num(300.0), Cell::Num(210.0), Cell::Num(90.0)],
                vec![Cell::Text("B".into()), Cell::Num(300.0), Cell::Num(230.0), Cell::Num(70.0)],
                vec![Cell::Text("C".into()), Cell::Num(120.0), Cell::Num(85.0), Cell::Num(35.0)],
                vec![Cell::Text("D".into()), Cell::Num(130.0), Cell::Num(95.0), Cell::Num(35.0)],
                vec![Cell::Text("Total".into()), Cell::Num(845.0), Cell::Num(620.0), Cell::Num(230.0)],
            ],
        }];
        let f = facts(&t);
        assert!(f.contains("Total · Total: 850"), "items sum to 850, not the printed 845:\n{f}");
        assert!(f.contains("Total · FEC: 230"), "{f}");
        // 230/850 = 27.06% — above the 25% that gates the DDWP's power.
        assert!(f.contains("27.06%"), "the share a threshold tests must be computed:\n{f}");
        assert!(f.contains("DISCREPANCY"), "the misprinted total must still be flagged:\n{f}");
    }

    #[test]
    fn a_row_total_that_disagrees_with_its_periods_is_caught() {
        // Item B prints 295 where Year-I..III sum to 300. Unnoticed, this made
        // the capital cost 845 instead of 850 and every derived figure wrong.
        let t = vec![Table {
            name: "Capital Cost Phasing".into(),
            headers: vec![
                "Items".into(),
                "Year-I · Total".into(),
                "Year-II · Total".into(),
                "Year-III · Total".into(),
                "Total · Total".into(),
            ],
            rows: vec![
                vec![Cell::Text("A".into()), Cell::Num(100.0), Cell::Num(150.0), Cell::Num(50.0), Cell::Num(300.0)],
                vec![Cell::Text("B".into()), Cell::Num(80.0), Cell::Num(120.0), Cell::Num(100.0), Cell::Num(295.0)],
            ],
        }];
        let f = row_cross_foot(&t);
        assert_eq!(f.len(), 1, "only item B is misprinted: {f:?}");
        assert!(f[0].what.contains("row 'B'"), "{}", f[0].line());
        assert_eq!(f[0].stated, 295.0);
        assert_eq!(f[0].computed, 300.0);
    }

    #[test]
    fn facts_are_empty_without_a_workbook() {
        assert!(facts(&[]).is_empty());
    }

    #[test]
    fn describe_never_dumps_the_whole_sheet() {
        let t = phasing();
        let d = describe(&t, 2);
        assert!(d.contains("columns: Items | Total"));
        assert_eq!(d.matches("example:").count(), 2, "sample rows must be capped");
    }
}
