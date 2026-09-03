//! Build a [`Plan`] from the question **without a model**.
//!
//! The first cut asked the local model for every plan. Measured on prod: only
//! 12 of 53 turns produced a usable one — Qwen3 spends its budget on `<think>`
//! unless the no-think prefix is set, and even when it answers it mis-plans
//! ("average profit" came back as `sum of Revenue`).
//!
//! But "what is the total revenue" does not need a language model. The question
//! names an operation and a column, and the column names are right there in the
//! sheet. This resolves the ordinary shapes deterministically — which is faster,
//! free, testable, and cannot drift. The model stays as the fallback for
//! phrasings that do not match.

use super::plan::{Filter, Op, Plan, RowSelector};
use super::sheet::{parse_number, Table};

fn words(q: &str) -> Vec<String> {
    q.to_lowercase()
        .split(|c: char| !c.is_alphanumeric())
        .filter(|w| !w.is_empty())
        .map(|s| s.to_string())
        .collect()
}

fn has(ws: &[String], any: &[&str]) -> bool {
    ws.iter().any(|w| any.contains(&w.as_str()))
}

fn norm(s: &str) -> String {
    s.to_lowercase().chars().filter(|c| c.is_alphanumeric()).collect()
}

/// Everyday wording for column names an accountant abbreviates.
///
/// "Calculate total operating expenses" names no header — the column is `Opex`
/// — and without this the planner fell back to the first numeric column and
/// confidently summed Revenue instead.
const ALIASES: &[(&str, &str)] = &[
    ("operating expenses", "opex"),
    ("operating expense", "opex"),
    ("operating costs", "opex"),
    ("operating cost", "opex"),
    ("cost of goods sold", "cogs"),
    ("cost of sales", "cogs"),
    ("turnover", "revenue"),
    ("sales", "revenue"),
    ("foreign exchange component", "fec"),
    ("net profit", "profit"),
];

fn expand_aliases(q: &str) -> String {
    let mut s = q.to_lowercase();
    for (phrase, canonical) in ALIASES {
        if s.contains(phrase) {
            s = s.replace(phrase, canonical);
        }
    }
    s
}

/// The column whose name appears in the question, longest name first so
/// "Total · FEC" wins over "Total".
fn mentioned_column(t: &Table, q: &str) -> Option<String> {
    let qn = norm(&expand_aliases(q));
    let numeric = numeric_columns(t);
    let mut best: Option<(usize, String)> = None;
    for h in &t.headers {
        // Match the LEAF of a two-row header ("Year-I · FEC" -> "fec"), which is
        // how an officer usually refers to it — but try the WHOLE header first,
        // so "what is the total FEC" resolves to `Total · FEC` (the grand total)
        // rather than `Year-I · FEC`, which is the same leaf under a different
        // span and a different number.
        let span = h.split('·').next().map(norm).unwrap_or_default();
        let grand = matches!(span.as_str(), "total" | "totals" | "grandtotal" | "overall");

        for (idx, part) in std::iter::once(h.as_str()).chain(h.split('·')).enumerate() {
            let p = norm(part);
            if p.len() < 3 || !qn.contains(&p) {
                continue;
            }
            // Ranked, most significant first:
            //  * a NUMERIC column always beats a text one — "Which month has
            //    highest COGS?" contains both "month" and "cogs", and plain
            //    longest-match picked `Month`, then found nothing to compare;
            //  * a WHOLE-header match beats a leaf match, so "the Year-I total"
            //    is not answered out of the grand-total column;
            //  * otherwise the GRAND-TOTAL span wins, because "the total capital
            //    cost" means the aggregate, not year one — plain leaf matching
            //    tied `Year-I · Total` with `Total · Total` and returned 280
            //    where the answer is 850;
            //  * length last, as a tie-break.
            let score = (numeric.contains(h) as usize) * 10_000
                + ((idx == 0) as usize) * 1_000
                + (grand as usize) * 100
                + p.len();
            if best.as_ref().map(|(n, _)| score > *n).unwrap_or(true) {
                best = Some((score, h.clone()));
            }
        }
    }
    best.map(|(_, h)| h)
}

/// Columns holding numbers in most rows.
fn numeric_columns(t: &Table) -> Vec<String> {
    let mut out = Vec::new();
    for (i, h) in t.headers.iter().enumerate() {
        let n = t.rows.iter().filter(|r| r.get(i).and_then(|c| c.as_num()).is_some()).count();
        if !t.rows.is_empty() && n * 2 > t.rows.len() {
            out.push(h.clone());
        }
    }
    out
}

/// The numeric column to use when the question names none.
///
/// Only when there is exactly ONE — then "the largest single transaction" is
/// unambiguous. With several, guessing is how you sum Revenue for a question
/// about expenses, so the planner defers instead and lets the model decide (or,
/// failing that, the lane refuses and says which columns exist).
fn sole_numeric(t: &Table) -> Option<String> {
    let n = numeric_columns(t);
    (n.len() == 1).then(|| n[0].clone())
}

/// The first mostly-text column — the natural label for "which month…".
fn label_column(t: &Table) -> Option<String> {
    let numeric = numeric_columns(t);
    t.headers.iter().find(|h| !numeric.contains(h)).cloned()
}

/// Words that are an INSTRUCTION, never a data value.
///
/// "What is the correct total capital cost?" contains "total", and the Items
/// column of a PC-I phasing table contains a row labelled "Total" — so this
/// matched a filter `Items == "Total"`, selecting precisely the totals row that
/// `eligible()` always excludes. Zero rows survived and the lane reported
/// "column 'Year-I · Total' has no numeric values", which reads like a parse
/// failure and is nothing of the kind.
const NEVER_A_VALUE: &[&str] = &[
    "total", "totals", "grand", "subtotal", "sum", "average", "mean", "count",
    "highest", "lowest", "largest", "smallest", "maximum", "minimum", "share",
    "margin", "ratio", "correct", "value", "amount", "cost", "figure",
];

/// A bare value in the question that matches an entry of a text column
/// ("Rejected" -> Status, "Consultancy" -> Category).
fn value_filter(t: &Table, ws: &[String]) -> Option<Filter> {
    let numeric = numeric_columns(t);
    for (i, h) in t.headers.iter().enumerate() {
        if numeric.contains(h) {
            continue;
        }
        for w in ws {
            if w.len() < 4 || NEVER_A_VALUE.contains(&w.as_str()) {
                continue;
            }
            let hit = t.rows.iter().any(|r| {
                r.get(i).map(|c| norm(&c.text()) == norm(w)).unwrap_or(false)
            });
            if hit {
                return Some(Filter {
                    column: h.clone(),
                    op: "==".into(),
                    // Use the sheet's own spelling, not the question's.
                    value: t
                        .rows
                        .iter()
                        .find_map(|r| {
                            r.get(i).map(|c| c.text()).filter(|v| norm(v) == norm(w))
                        })
                        .unwrap_or_else(|| w.clone()),
                });
            }
        }
    }
    None
}

/// A numeric threshold: "above 50000", "less than 100", "negative".
fn threshold_filter(t: &Table, q: &str, ws: &[String], col: &Option<String>) -> Option<Filter> {
    let column = col.clone().or_else(|| numeric_columns(t).first().cloned())?;
    if has(ws, &["negative", "overspent", "overspend", "deficit"]) {
        return Some(Filter { column, op: "<".into(), value: "0".into() });
    }
    let ql = q.to_lowercase();
    let gt = ["above ", "greater than ", "more than ", "over ", "exceeds ", "exceeding "];
    let lt = ["below ", "less than ", "under ", "fewer than "];
    for (set, op) in [(gt.as_slice(), ">"), (lt.as_slice(), "<")] {
        for kw in set {
            if let Some(pos) = ql.find(kw) {
                let tail = &ql[pos + kw.len()..];
                let tok: String =
                    tail.chars().take_while(|c| c.is_ascii_digit() || *c == ',' || *c == '.').collect();
                if parse_number(&tok).is_some() {
                    return Some(Filter { column, op: op.to_string(), value: tok });
                }
            }
        }
    }
    None
}

/// A key like "for March" / "in Jan" that selects one row.
fn key_selector(t: &Table, ws: &[String]) -> Option<RowSelector> {
    let numeric = numeric_columns(t);
    for (i, h) in t.headers.iter().enumerate() {
        if numeric.contains(h) {
            continue;
        }
        for w in ws {
            if w.len() < 3 || NEVER_A_VALUE.contains(&w.as_str()) {
                continue;
            }
            // Never key on the totals line — it is excluded from the data, so a
            // selector pointing at it can only ever resolve to nothing.
            let hit = t.rows.iter().filter(|r| !t.is_total_row(r)).any(|r| {
                let v = norm(&r.get(i).map(|c| c.text()).unwrap_or_default());
                !v.is_empty() && (v == norm(w) || norm(w).starts_with(&v))
            });
            if hit {
                return Some(RowSelector::Key { key_column: h.clone(), key: w.clone() });
            }
        }
    }
    None
}

/// Does the question restrict to a period this planner cannot express?
///
/// Date ranges are not in the plan language. Ignoring the qualifier is the
/// worst outcome available: "the total for Jul-Sep 2024" returned the total for
/// the WHOLE sheet, labelled as computed. Better to plan nothing and let the
/// question be refused or handled elsewhere.
fn has_unsupported_period(q: &str) -> bool {
    let q = q.to_lowercase();
    if q.contains("between ") || q.contains(" to ") && q.contains("20") {
        return true;
    }
    const MONTHS: &[&str] = &[
        "jan", "feb", "mar", "apr", "may", "jun", "jul", "aug", "sep", "oct", "nov", "dec",
    ];
    // Two month names, or a quarter, is a range rather than a single key.
    let hits = MONTHS.iter().filter(|m| q.contains(*m)).count();
    hits > 1 || q.contains("quarter") || q.contains("q1") || q.contains("q2")
        || q.contains("q3") || q.contains("q4")
}

/// Resolve a question to a plan, or `None` to let the model try.
pub fn from_question(tables: &[Table], question: &str) -> Option<Plan> {
    if has_unsupported_period(question) {
        return None;
    }
    let ws = words(&expand_aliases(question));
    // Plan against the sheet that has the most rows AND a column named in the
    // question; otherwise the largest sheet.
    let t = tables
        .iter()
        .filter(|t| !t.rows.is_empty())
        .max_by_key(|t| {
            (mentioned_column(t, question).is_some() as usize) * 1_000_000 + t.rows.len()
        })?;

    let col = mentioned_column(t, question);
    let numeric = numeric_columns(t);
    let label = label_column(t);
    let mut base = Plan {
        op: Op::Sum,
        sheet: Some(t.name.clone()),
        column: col.clone(),
        label_column: label.clone(),
        denominator: None,
        group_by: None,
        filter: None,
        row: None,
        as_percent: false,
    };

    // "spend by department" / "total per category"
    if let Some(pos) = ws.iter().position(|w| w == "by" || w == "per") {
        if let Some(next) = ws.get(pos + 1) {
            if let Some(g) = t.headers.iter().find(|h| norm(h).contains(&norm(next))) {
                base.op = Op::GroupSum;
                base.group_by = Some(g.clone());
                base.column = col.or_else(|| sole_numeric(t));
                return base.column.is_some().then_some(base);
            }
        }
    }

    // Ratio: "profit margin for the best month", "FEC share of total cost"
    if has(&ws, &["margin", "share", "ratio", "proportion"]) {
        // "X as a share of total" is a COMPONENT over its total, so the
        // numerator is never the total column itself. Without this, "the FEC
        // share of total cost" matched `Total · Total` (the word "total" scores
        // twice) and divided it by `Year-I · Total`, reporting 301.79%.
        let leaf_is_total =
            |h: &str| h.rsplit('·').next().map(|l| norm(l) == "total").unwrap_or(false);
        let num = col
            .clone()
            .filter(|c| !leaf_is_total(c))
            .or_else(|| {
                // Re-match ignoring the total columns entirely, and keep the
                // grand-total span preference — otherwise "the FEC share" picks
                // `Year-I · FEC` simply because it comes first.
                let qn = norm(&expand_aliases(question));
                numeric
                    .iter()
                    .filter(|h| !leaf_is_total(h))
                    .filter(|h| {
                        h.rsplit('·').next().map(|l| qn.contains(&norm(l))).unwrap_or(false)
                    })
                    .max_by_key(|h| {
                        let span = h.split('·').next().map(norm).unwrap_or_default();
                        matches!(span.as_str(), "total" | "totals" | "grandtotal" | "overall")
                            as usize
                    })
                    .cloned()
            })?;
        // Denominator: the total of the SAME span, so a component is compared
        // against its own whole rather than a neighbouring year's.
        let span = num.split('·').next().map(norm).unwrap_or_default();
        let den = numeric
            .iter()
            .find(|h| {
                leaf_is_total(h) && h.split('·').next().map(norm).unwrap_or_default() == span
            })
            .or_else(|| numeric.iter().find(|h| leaf_is_total(h)))
            .or_else(|| numeric.iter().find(|h| **h != num))?
            .clone();
        let (num, den) = (num, den);
        base.op = Op::Ratio;
        base.column = Some(num.clone());
        base.denominator = Some(den);
        base.as_percent = true;
        base.row = if has(&ws, &["best", "highest", "largest", "top", "maximum"]) {
            Some(RowSelector::MaxOf { column: num })
        } else {
            key_selector(t, &ws)
        };
        return Some(base);
    }

    let superlative_hi = has(&ws, &["highest", "largest", "maximum", "max", "most", "best", "biggest"]);
    let superlative_lo = has(&ws, &["lowest", "smallest", "minimum", "min", "least", "worst"]);
    let asks_which = has(&ws, &["which", "what", "who"]) && label.is_some();

    // "how many X" / "count"
    if question.to_lowercase().contains("how many") || has(&ws, &["count", "number"]) {
        if has(&ws, &["distinct", "different", "unique"]) || col.as_ref().map(|c| !numeric.contains(c)).unwrap_or(false) {
            base.op = Op::CountDistinct;
            base.column = col.clone().or_else(|| label.clone());
        } else {
            base.op = Op::Count;
        }
        base.filter = threshold_filter(t, question, &ws, &col).or_else(|| value_filter(t, &ws));
        return Some(base);
    }

    // "which items are overspent" -> list
    if has(&ws, &["which", "list", "name"]) && has(&ws, &["negative", "overspent", "overspend"]) {
        base.op = Op::List;
        base.label_column = label.clone();
        base.filter = threshold_filter(t, question, &ws, &col);
        return Some(base);
    }

    if superlative_hi || superlative_lo {
        let target = col.clone().or_else(|| sole_numeric(t))?;
        base.column = Some(target);
        // "which month has the highest profit" wants the LABEL; "what is the
        // largest transaction" wants the value.
        base.op = match (superlative_hi, asks_which) {
            (true, true) => Op::ArgMax,
            (true, false) => Op::Max,
            (false, true) => Op::ArgMin,
            (false, false) => Op::Min,
        };
        base.filter = value_filter(t, &ws);
        return Some(base);
    }

    if has(&ws, &["average", "mean", "avg"]) {
        base.op = Op::Avg;
        base.column = col.or_else(|| sole_numeric(t));
        base.filter = threshold_filter(t, question, &ws, &base.column.clone()).or_else(|| value_filter(t, &ws));
        return base.column.is_some().then_some(base);
    }

    if has(&ws, &["total", "totals", "sum", "calculate", "compute", "spend", "spent"]) {
        // "the total of the negative balances" is a filtered sum.
        base.op = Op::Sum;
        base.column = col.clone().or_else(|| sole_numeric(t));
        base.filter = threshold_filter(t, question, &ws, &base.column.clone())
            .or_else(|| value_filter(t, &ws));
        return base.column.is_some().then_some(base);
    }

    // "what is profit for March" — a plain lookup.
    if let (Some(c), Some(row)) = (col.clone(), key_selector(t, &ws)) {
        if numeric.contains(&c) {
            base.op = Op::Lookup;
            base.column = Some(c);
            base.row = Some(row);
            return Some(base);
        }
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::services::tabular::sheet::Cell;

    fn monthly() -> Vec<Table> {
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
            headers: ["Month", "Revenue", "COGS", "Opex", "Profit"]
                .iter()
                .map(|s| s.to_string())
                .collect(),
            rows: raw
                .iter()
                .map(|(m, r, c, o, p)| {
                    vec![Cell::Text((*m).into()), Cell::Num(*r), Cell::Num(*c), Cell::Num(*o), Cell::Num(*p)]
                })
                .collect(),
        }]
    }

    /// Every question from the prod questionnaire, planned WITHOUT a model.
    fn answer(q: &str) -> String {
        let t = monthly();
        let p = from_question(&t, q).unwrap_or_else(|| panic!("no plan for: {q}"));
        super::super::plan::execute(&t, &p).unwrap_or_else(|e| panic!("{q}: {e}")).value
    }

    #[test]
    fn the_whole_questionnaire_resolves_without_a_model() {
        assert_eq!(answer("What is the total revenue?"), "1,273,872");
        assert_eq!(answer("Which month has the highest profit?"), "Oct (94,064)");
        assert_eq!(answer("What is the average profit?"), "43114.20");
        assert_eq!(answer("Which month has the lowest revenue?"), "Apr (93,173)");
        assert_eq!(answer("Calculate total operating expenses."), "227,078");
        assert_eq!(answer("What is profit for March?"), "66,269");
        assert_eq!(answer("How many months have profit above 50000?"), "6");
        assert_eq!(answer("Which month has highest COGS?"), "Jun (89,983)");
        assert_eq!(answer("What is total COGS?"), "615,652");
        assert_eq!(answer("What is the profit margin for the best month?"), "54.57%");
    }

    /// The PC-I phasing shape: spans of Total/Local/FEC, and an Items column
    /// whose last row is literally labelled "Total".
    fn phasing() -> Vec<Table> {
        let h: Vec<String> = [
            "Items",
            "Year-I · Total", "Year-I · Local", "Year-I · FEC",
            "Total · Total", "Total · Local", "Total · FEC",
        ]
        .iter()
        .map(|s| s.to_string())
        .collect();
        let row = |n: &str, a: f64, b: f64, c: f64, d: f64, e: f64, f: f64| {
            vec![Cell::Text(n.into()), Cell::Num(a), Cell::Num(b), Cell::Num(c),
                 Cell::Num(d), Cell::Num(e), Cell::Num(f)]
        };
        vec![Table {
            name: "Capital Cost Phasing".into(),
            headers: h,
            rows: vec![
                row("A", 100.0, 70.0, 30.0, 300.0, 210.0, 90.0),
                row("B", 80.0, 60.0, 20.0, 300.0, 230.0, 70.0),
                row("C", 60.0, 40.0, 25.0, 120.0, 85.0, 35.0),
                row("D", 40.0, 30.0, 10.0, 130.0, 95.0, 35.0),
                row("Total", 280.0, 200.0, 80.0, 845.0, 620.0, 230.0),
            ],
        }]
    }

    #[test]
    fn the_word_total_is_an_instruction_not_a_row_label() {
        // Both of these built the filter `Items == "Total"`, which selects only
        // the row `eligible()` excludes — reported as "no numeric values",
        // which reads like a parse failure and is nothing of the kind.
        let t = phasing();
        let p = from_question(&t, "What is the correct total capital cost?").unwrap();
        assert!(p.filter.is_none(), "must not filter on the totals label: {:?}", p.filter);
        // Sums the four ITEM rows, not the misprinted 845.
        assert_eq!(super::super::plan::execute(&t, &p).unwrap().value, "850");
    }

    #[test]
    fn a_share_is_a_component_over_its_own_total() {
        // Reported 301.79% (Total·Total ÷ Year-I·Total) before this: the word
        // "total" scored twice and won the numerator.
        let t = phasing();
        let p = from_question(&t, "What is the FEC share of total cost?").unwrap();
        assert_eq!(p.column.as_deref(), Some("Total · FEC"));
        assert_eq!(p.denominator.as_deref(), Some("Total · Total"));
        // 230 / 845 — 845 is what the ITEM ROWS print, and item B's row total is
        // itself misprinted (295 where its years sum to 300). Either way the
        // share is above the 25% that gates the DDWP's power.
        let v = super::super::plan::execute(&t, &p).unwrap().value;
        assert!(v.starts_with("27."), "expected ~27%, got {v}");
    }

    #[test]
    fn naming_the_span_still_beats_the_grand_total() {
        // The grand-total preference must not hijack a question that says which
        // span it means.
        let t = phasing();
        let p = from_question(&t, "What is the Year-I Total?").unwrap();
        assert_eq!(p.column.as_deref(), Some("Year-I · Total"));
        assert_eq!(super::super::plan::execute(&t, &p).unwrap().value, "280");
    }

    #[test]
    fn total_fec_means_the_grand_total_not_year_one() {
        let t = phasing();
        let p = from_question(&t, "What is the total FEC?").unwrap();
        assert_eq!(p.column.as_deref(), Some("Total · FEC"));
        assert_eq!(super::super::plan::execute(&t, &p).unwrap().value, "230");
    }

    #[test]
    fn a_non_arithmetic_question_gets_no_plan() {
        assert!(from_question(&monthly(), "Summarise this workbook").is_none());
    }
}
