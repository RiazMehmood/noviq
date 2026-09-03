//! The **spreadsheet** capability: answers arithmetic questions about an
//! attached workbook by computing, never by asking the model to calculate.
//!
//! Measured baseline (prod, 2026-08-03, 53 questions over 7 workbooks): **19
//! correct**. The wrong answers were confident and carried citation marks —
//! `"The total revenue is **1,263,651**. [1]"` against a true 1,273,872 — and a
//! 240-row sheet failed outright, every turn returning HTTP 400 from vLLM in
//! ~15 ms because the rows overflowed `max_model_len`.
//!
//! None of that is a retrieval failure. `Financial_Test_1` fits in ONE chunk,
//! so the model saw every row. Asked `107506 + 107559` with no document at all
//! it answered `415065`.
//!
//! So this lane splits the work the way ADR-0019 split counting:
//!
//! * the model sees **column names and three sample rows** — never the data —
//!   and emits a small JSON [`Plan`]; a reading task;
//! * [`tabular::execute`] computes the answer in Rust; pure and unit-tested;
//! * the sheet's own `Total` rows are cross-footed by [`tabular::cross_foot`],
//!   which needs no model and no question at all.
//!
//! Sending only the schema also fixes the overflow (the prompt no longer grows
//! with the sheet) and is the stronger privacy position: the figures never
//! leave the box, which matters because the active engine may be a hosted API.

use crate::compat::AppHandle;
use crate::db::Db;
use crate::services::assistant::{
    self, context_budget::EstimateCounter, intent::Capability, persona::PersonaConfig,
    AssembleInput, CitationOut, TurnResponse,
};
use crate::services::hardware_detector::HardwareTier;
use crate::services::llm::ModelSlot;
use crate::services::query_router;
use crate::services::query_trace::QueryTrace;
use crate::services::tabular::{self, Table};

/// Spreadsheet formats this lane can compute over.
pub fn is_spreadsheet(file_name: &str) -> bool {
    let f = file_name.to_lowercase();
    f.ends_with(".xlsx") || f.ends_with(".xlsm")
}

/// The attached workbooks for this conversation, as (file_name, path).
pub async fn attached_workbooks(pool: &Db, conversation_id: &str) -> Vec<(String, String)> {
    sqlx::query_as::<_, (String, String)>(
        "SELECT file_name, file_path FROM sources \
         WHERE conversation_id = $1 AND indexing_status = 'indexed'",
    )
    .bind(conversation_id)
    .fetch_all(pool)
    .await
    .unwrap_or_default()
    .into_iter()
    .filter(|(name, _)| is_spreadsheet(name))
    .collect()
}

/// Does this question want a computed answer?
///
/// Deliberately broad: with a spreadsheet attached, arithmetic is the common
/// case, and the cost of being wrong is asymmetric. A miss here means the old
/// path answers with an invented number; a false positive means a plan is
/// attempted, fails to fit, and falls back.
pub fn wants_computation(q: &str) -> bool {
    let q = q.to_lowercase();

    // Multi-word cues are unambiguous, so a substring test is safe.
    const PHRASES: &[&str] = &["how many", "how much", "add up", "adds up", "work out"];
    if PHRASES.iter().any(|c| q.contains(c)) {
        return true;
    }

    // Single words must match as WHOLE WORDS. `"sum"` as a substring also fires
    // on "summarise", "summary" and "consumption" — which sent every
    // "summarise this document" into the arithmetic lane until the unit test
    // caught it.
    const WORDS: &[&str] = &[
        "total", "totals", "sum", "summed", "average", "mean", "count", "highest",
        "lowest", "largest", "smallest", "maximum", "minimum", "max", "min", "most",
        "least", "calculate", "compute", "margin", "percentage", "percent", "share",
        "ratio", "difference", "reconcile", "reconciles", "tally", "balance",
        "balances", "spend", "spent",
    ];
    q.split(|c: char| !c.is_alphanumeric()).any(|w| WORDS.contains(&w))
}

const PLAN_INSTRUCTIONS: &str = r#"You translate a question about a spreadsheet into ONE JSON object.
You must NOT do any arithmetic yourself — the system computes the result.

Reply with JSON only, no prose, no code fence.

Fields:
  op          one of: sum, avg, min, max, count, count_distinct, arg_max, arg_min,
              lookup, ratio, list, group_sum
  sheet       sheet name (optional)
  column      the column to operate on
  label_column  column holding the row label (for arg_max/arg_min/list)
  denominator   second column (for ratio)
  group_by      column to group by (for group_sum)
  filter      {"column":..,"op":"> >= < <= == != contains","value":".."}
  row         one of {"key":{"key_column":..,"key":..}}, {"max_of":{"column":..}},
              {"min_of":{"column":..}}   — selects ONE row for lookup/ratio
  as_percent  true to render the result as a percentage

Examples:
  "what is the total revenue"        -> {"op":"sum","column":"Revenue"}
  "which month had the most profit"  -> {"op":"arg_max","column":"Profit","label_column":"Month"}
  "how many months above 50000"      -> {"op":"count","filter":{"column":"Profit","op":">","value":"50000"}}
  "profit for March"                 -> {"op":"lookup","column":"Profit","row":{"key":{"key_column":"Month","key":"Mar"}}}
  "margin of the best month"         -> {"op":"ratio","column":"Profit","denominator":"Revenue","row":{"max_of":{"column":"Profit"}},"as_percent":true}
  "spend by department"              -> {"op":"group_sum","column":"Amount","group_by":"Department"}
  "how many distinct vendors"        -> {"op":"count_distinct","column":"Vendor"}
"#;

/// Pull the first JSON object out of a free-text reply.
///
/// Free text, tolerantly parsed — **never llguidance**, which has crashed the
/// process twice in this codebase.
fn extract_json(raw: &str) -> Option<String> {
    let s = raw.trim().trim_start_matches("```json").trim_start_matches("```").trim();
    let start = s.find('{')?;
    let mut depth = 0usize;
    for (i, ch) in s[start..].char_indices() {
        match ch {
            '{' => depth += 1,
            '}' => {
                depth -= 1;
                if depth == 0 {
                    return Some(s[start..start + i + 1].to_string());
                }
            }
            _ => {}
        }
    }
    None
}

fn plan_prompt(schema: &str, question: &str) -> String {
    format!(
        "<|im_start|>system\n{PLAN_INSTRUCTIONS}<|im_end|>\n\
         <|im_start|>user\nSpreadsheet:\n{schema}\nQuestion: {question}\n\
         Reply with the JSON plan only.<|im_end|>\n<|im_start|>assistant\n{no_think}",
        // Without this Qwen3 opens a <think> block and the whole token budget
        // goes to reasoning, so no JSON is ever emitted and the lane silently
        // falls through to the model answer it exists to replace.
        no_think = query_router::no_think_assistant_prefix(),
    )
}

/// Does this question need the CORPUS as well as the sheet?
///
/// A spreadsheet is data, never a legal instrument — the rule that reads it is
/// always in the corpus. But a live attachment narrows retrieval to itself, so
/// "which forum approves this project?" searched only the workbook and answered
/// "the retrieved documents do not contain information about the forum", while
/// the Manual for Development Projects sat indexed and citable the whole time.
///
/// Deliberately narrow: only questions that plainly reach for a rule widen the
/// scope, so "summarise this sheet" is not answered out of the legal corpus.
pub fn needs_corpus_rules(q: &str) -> bool {
    let q = q.to_lowercase();
    const CUES: &[&str] = &[
        "forum", "approve", "approval", "approves", "sanction", "authority",
        "authorised", "authorized", "rule", "rules", "regulation", "regulations",
        "comply", "compliant", "compliance", "permissible", "permitted", "allowed",
        "eligible", "threshold", "ceiling", "limit", "law", "act ", "statute",
        "requirement", "required by", "who can", "entitled",
    ];
    CUES.iter().any(|c| q.contains(c))
}

/// Computed figures for a corpus turn — empty when there is no workbook.
pub async fn facts_for_turn(pool: &Db, conversation_id: &str) -> String {
    let books = attached_workbooks(pool, conversation_id).await;
    if books.is_empty() {
        return String::new();
    }
    let mut tables = Vec::new();
    for (_, path) in &books {
        if let Ok(mut t) = tabular::read_workbook(std::path::Path::new(path)) {
            tables.append(&mut t);
        }
    }
    tabular::facts(&tables)
}

/// "Does it add up?" — answered from the sheet's own totals, no plan needed.
pub fn is_reconcile_question(q: &str) -> bool {
    let q = q.to_lowercase();
    ["reconcile", "add up", "adds up", "tally", "cross-foot", "cross foot", "consistent"]
        .iter()
        .any(|k| q.contains(k))
        || (q.contains("match") && (q.contains("total") || q.contains("cost")))
}

fn reconcile_answer(
    cfg: &PersonaConfig,
    conversation_id: &str,
    query: &str,
    names: &[String],
    tables: &[Table],
) -> TurnResponse {
    let findings = tabular::cross_foot(tables);
    let answer = if findings.is_empty() {
        format!(
            "**Yes — it reconciles.** Every printed total in {} equals the sum of its own rows.",
            names.join(", ")
        )
    } else {
        let mut s = format!("**No — {} discrepancies.**\n\n", findings.len());
        for f in &findings {
            s.push_str(&format!("- {}\n", f.line()));
        }
        s.push_str(
            "\nEach line compares a total printed in the sheet against the sum of the rows \
             above it. Computed, not estimated.\n",
        );
        s
    };
    let window =
        assistant::n_ctx::env_override().unwrap_or(assistant::n_ctx::FALLBACK_TRAINED) as usize;
    let mem = assistant::memory::MemoryContext::default();
    let budget = assistant::plan_budget(
        &EstimateCounter, window, Capability::Tabular, cfg, false, &mem, query, &[],
    );
    assistant::assemble_response(AssembleInput {
        conversation_id: conversation_id.to_string(),
        capability: Capability::Tabular,
        answer,
        context: &tabular::describe(tables, 2),
        citations: Vec::<CitationOut>::new(),
        plan: &budget,
        fill_threshold: cfg.settings.context_fill_threshold,
        chat_fill: None,
        offer_tailor: false,
    })
}

/// Run a spreadsheet turn. Returns `Ok(None)` when this lane cannot help, so
/// the caller falls through to ordinary retrieval rather than failing the turn.
#[allow(clippy::too_many_arguments)]
pub async fn run(
    pool: &Db,
    cfg: &PersonaConfig,
    model_slot: &ModelSlot,
    tier: &HardwareTier,
    app: Option<&AppHandle>,
    conversation_id: &str,
    query: &str,
    trace: &mut QueryTrace,
) -> Result<Option<TurnResponse>, String> {
    let books = attached_workbooks(pool, conversation_id).await;
    if books.is_empty() {
        return Ok(None);
    }

    // Parse every attached workbook. A file that has been moved or is corrupt
    // is skipped rather than failing the turn.
    let mut tables: Vec<Table> = Vec::new();
    let mut names: Vec<String> = Vec::new();
    for (name, path) in &books {
        match tabular::read_workbook(std::path::Path::new(path)) {
            Ok(mut t) => {
                names.push(name.clone());
                tables.append(&mut t);
            }
            Err(e) => log::warn!("tabular: cannot read {name}: {e}"),
        }
    }
    if tables.is_empty() {
        return Ok(None);
    }

    let schema = tabular::describe(&tables, 3);
    trace.step("Spreadsheet detected", &[
        ("Workbooks", names.join(", ")),
        ("Sheets", tables.len().to_string()),
        ("Rows", tables.iter().map(|t| t.rows.len()).sum::<usize>().to_string()),
    ]);

    // "Does this add up?" is answered by the sheet itself — no plan, no model.
    if is_reconcile_question(query) {
        return Ok(Some(reconcile_answer(cfg, conversation_id, query, &names, &tables)));
    }

    // Plan deterministically first. Measured 2026-08-03: asking the model for
    // every plan produced a usable one on only 12 of 53 turns — Qwen3 spends
    // the token budget on `<think>` unless the no-think prefix is set, and even
    // when it replied it mis-planned ("average profit" came back as `sum of
    // Revenue`). The ordinary shapes name an operation and a column, and the
    // column names are in the sheet, so no language model is required.
    let (plan, json, how) = match tabular::from_question(&tables, query) {
        Some(p) => {
            let j = serde_json::to_string(&p).unwrap_or_default();
            (p, j, "rules")
        }
        None => {
            let raw = query_router::generate_direct(
                model_slot,
                tier,
                &plan_prompt(&schema, query),
                384,
                None,
                app.cloned(),
            )
            .await
            .map_err(|e| format!("plan generation failed: {e}"))?;

            let Some(json) = extract_json(&raw) else {
                trace.step("Plan not parseable", &[("Raw", raw.chars().take(160).collect())]);
                return Ok(None);
            };
            match serde_json::from_str::<tabular::Plan>(&json) {
                Ok(p) => (p, json, "model"),
                Err(e) => {
                    trace.step("Plan rejected", &[("JSON", json), ("Error", e.to_string())]);
                    return Ok(None);
                }
            }
        }
    };
    trace.step("Planned", &[("By", how.to_string()), ("Plan", json.clone())]);

    let computed = match tabular::execute(&tables, &plan) {
        Ok(c) => c,
        Err(e) => {
            // A plan that does not fit the sheet is reported, never guessed
            // around. Falling back to the model here would reintroduce exactly
            // the invented numbers this lane exists to remove.
            trace.step("Plan did not fit the sheet", &[("Reason", e.clone())]);
            return Ok(Some(refusal(cfg, conversation_id, query, &schema, &e)));
        }
    };

    trace.step("Computed deterministically", &[
        ("Plan", json.clone()),
        ("Workings", computed.workings.clone()),
        ("Rows used", computed.rows_used.to_string()),
        ("Result", computed.value.clone()),
    ]);

    // Every turn also cross-foots the sheet's own totals. A misprinted total is
    // worth saying even when nobody asked, because every later answer that
    // trusts it inherits the error.
    let findings = tabular::cross_foot(&tables);

    let mut answer = format!("**{}**\n\n", computed.value);
    answer.push_str(&format!(
        "_Computed from {} — {}._\n",
        names.join(", "),
        computed.workings
    ));
    if !findings.is_empty() {
        answer.push_str("\n**The sheet does not add up:**\n");
        for f in findings.iter().take(6) {
            answer.push_str(&format!("- {}\n", f.line()));
        }
        answer.push_str(
            "\nThese are the sheet's own printed totals against the sum of its rows.\n",
        );
    }

    let context = format!("{schema}\nplan: {json}\nresult: {}", computed.value);
    let window =
        assistant::n_ctx::env_override().unwrap_or(assistant::n_ctx::FALLBACK_TRAINED) as usize;
    let mem = assistant::memory::MemoryContext::default();
    let budget = assistant::plan_budget(
        &EstimateCounter, window, Capability::Tabular, cfg, false, &mem, query, &[],
    );

    Ok(Some(assistant::assemble_response(AssembleInput {
        conversation_id: conversation_id.to_string(),
        capability: Capability::Tabular,
        answer,
        context: &context,
        citations: Vec::<CitationOut>::new(),
        plan: &budget,
        fill_threshold: cfg.settings.context_fill_threshold,
        chat_fill: None,
        offer_tailor: false,
    })))
}

/// Say what could not be computed and why — with the column names, so the
/// officer can re-ask in the sheet's own words.
fn refusal(
    cfg: &PersonaConfig,
    conversation_id: &str,
    query: &str,
    schema: &str,
    reason: &str,
) -> TurnResponse {
    let answer = format!(
        "I could not compute that from the attached spreadsheet: {reason}.\n\n\
         The workbook holds:\n\n{schema}\n\
         Ask again naming one of these columns and I will compute it exactly."
    );
    let window =
        assistant::n_ctx::env_override().unwrap_or(assistant::n_ctx::FALLBACK_TRAINED) as usize;
    let mem = assistant::memory::MemoryContext::default();
    let budget = assistant::plan_budget(
        &EstimateCounter, window, Capability::Tabular, cfg, false, &mem, query, &[],
    );
    assistant::assemble_response(AssembleInput {
        conversation_id: conversation_id.to_string(),
        capability: Capability::Tabular,
        answer,
        context: schema,
        citations: Vec::<CitationOut>::new(),
        plan: &budget,
        fill_threshold: cfg.settings.context_fill_threshold,
        chat_fill: None,
        offer_tailor: false,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn json_survives_a_chatty_model() {
        assert_eq!(
            extract_json("Sure!\n```json\n{\"op\":\"sum\",\"column\":\"Revenue\"}\n```").as_deref(),
            Some("{\"op\":\"sum\",\"column\":\"Revenue\"}")
        );
        assert_eq!(
            extract_json("{\"op\":\"count\",\"filter\":{\"column\":\"P\",\"op\":\">\",\"value\":\"1\"}}")
                .map(|s| s.contains("filter")),
            Some(true),
            "nested braces must not truncate the object"
        );
        assert!(extract_json("no json here").is_none());
    }

    #[test]
    fn spreadsheet_detection() {
        assert!(is_spreadsheet("Financial_Test_1.xlsx"));
        assert!(is_spreadsheet("BOOK.XLSM"));
        assert!(!is_spreadsheet("notice.pdf"));
    }

    #[test]
    fn computation_cues() {
        assert!(wants_computation("What is the total revenue?"));
        assert!(wants_computation("Which month has the highest profit?"));
        assert!(wants_computation("How many months have profit above 50000?"));
        assert!(wants_computation("Does the capital cost phasing reconcile?"));
        assert!(wants_computation("Which department has the highest total spend?"));
        // "sum" hides inside all three of these — whole-word matching only.
        assert!(!wants_computation("Summarise this document"));
        assert!(!wants_computation("Give me a summary"));
        assert!(!wants_computation("What is the consumption policy?"));
        assert!(!wants_computation("Draft a notice under Rule 12"));
    }
}
