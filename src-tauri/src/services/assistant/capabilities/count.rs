//! ADR-0019 — the **count/list** capability: answers "how many rules are in the
//! SPPRA Rules?", "how many acts do we have?", "list the sections of the Act"
//! with **SQL only** (`services::counting`) — no retrieval, no LLM, no
//! hallucinated numbers. Mirrors [`amendment_chain`](super::amendment_chain), the
//! other zero-model capability: structured facts → deterministic prose → a
//! grounded response whose context re-gates clean by construction.
//!
//! Two modes, chosen by whether the query names a specific in-scope document:
//!  * **within-document** — `COUNT(DISTINCT citation)` of a structural level
//!    (the officer's unit word is mapped to a `node_type` via the document's
//!    `DocProfile`), optionally listing the item labels;
//!  * **corpus-level** — `GROUP BY doc_type` over the visible `sources`.
//!
//! `COUNT(DISTINCT citation)` (never `COUNT(*)`) absorbs the duplicate-node data
//! bug proven on 2026-07-11 (Constitution: 332 raw article nodes vs 283 distinct
//! ≈ 280 real articles).

use crate::db::Db;
use crate::services::assistant::{
    self, context_budget::EstimateCounter, intent::Capability, persona::PersonaConfig,
    AssembleInput, CitationOut, TurnResponse,
};
use crate::services::counting;
use crate::services::query_trace::QueryTrace;

/// Run a count/list turn. Deterministic — no model call.
pub async fn run(
    pool: &Db,
    cfg: &PersonaConfig,
    conversation_id: &str,
    query: &str,
    scope_ids: &[String],
    trace: &mut QueryTrace,
) -> Result<TurnResponse, String> {
    let (answer, citations, context) = build(pool, query, scope_ids).await;
    trace.step("Deterministic count/list (SQL)", &[
        ("Path", "COUNT(DISTINCT citation) / GROUP BY doc_type — no model".to_string()),
        ("Sources in scope", scope_ids.len().to_string()),
        ("Citations", citations.len().to_string()),
    ]);

    let window =
        assistant::n_ctx::env_override().unwrap_or(assistant::n_ctx::FALLBACK_TRAINED) as usize;
    let mem = assistant::memory::MemoryContext::default();
    let plan = assistant::plan_budget(
        &EstimateCounter, window, Capability::Count, cfg, false, &mem, query, &[],
    );

    Ok(assistant::assemble_response(AssembleInput {
        conversation_id: conversation_id.to_string(),
        capability: Capability::Count,
        answer,
        context: &context,
        citations,
        plan: &plan,
        fill_threshold: cfg.settings.context_fill_threshold,
        chat_fill: None,
        offer_tailor: false,
    }))
}

/// Produce `(answer, citations, context)`. The context is the answer itself:
/// every fact is a database value, so the answer is grounded in its own
/// structured facts by construction (like `amendment_chain`'s `chain_context`).
async fn build(
    pool: &Db,
    query: &str,
    scope_ids: &[String],
) -> (String, Vec<CitationOut>, String) {
    let scope = Some(scope_ids);
    let unit = counting::parse_unit_word(query);
    let list = counting::wants_list(query);

    let sources = counting::sources_for_matching(pool, scope).await.unwrap_or_default();
    let (answer, citations) = if let Some(idx) = counting::best_source_match(query, &sources) {
        within_doc(pool, &sources[idx], unit.as_deref(), list).await
    } else {
        corpus(pool, scope, unit.as_deref()).await
    };
    let context = answer.clone();
    (answer, citations, context)
}

async fn within_doc(
    pool: &Db,
    s: &counting::SourceInfo,
    unit: Option<&str>,
    list: bool,
) -> (String, Vec<CitationOut>) {
    let doc_cite = |verbatim: String| CitationOut {
        n: 1,
        instrument: s.file_name.clone(),
        path: String::new(),
        verbatim,
        page: None,
        ..Default::default()
    };

    if let Some(u) = unit {
        if let Some(nt) = counting::node_type_for_unit(&s.doc_type, u) {
            let n = counting::count_units_in_source(pool, &s.id, nt).await.unwrap_or(0);
            let word = counting::unit_word_for_node_type(&s.doc_type, nt);
            let word_disp = counting::pluralize(&word, n);
            let mut answer = format!("There are **{n}** {word_disp} in *{}*.", s.file_name);
            if list {
                if let Ok(items) = counting::list_units_in_source(pool, &s.id, nt).await {
                    if !items.is_empty() {
                        answer.push_str("\n\n");
                        answer.push_str(
                            &items.iter().map(|c| format!("- {c}")).collect::<Vec<_>>().join("\n"),
                        );
                    }
                }
            }
            return (answer, vec![doc_cite(format!("{n} {word_disp}"))]);
        }
    }

    // No specific / unrecognised unit → full structural breakdown.
    match counting::structural_breakdown(pool, &s.id).await {
        Ok(levels) if !levels.is_empty() => {
            let lines: Vec<String> = levels
                .iter()
                .map(|lc| {
                    let w = counting::unit_word_for_node_type(&s.doc_type, &lc.node_type);
                    format!("- **{}** {}", lc.n, counting::pluralize(&w, lc.n))
                })
                .collect();
            let answer = format!(
                "Structural breakdown of *{}*:\n{}",
                s.file_name,
                lines.join("\n")
            );
            (answer, vec![doc_cite("structural breakdown".into())])
        }
        _ => (
            format!(
                "*{}* has not been structurally indexed, so its units can't be counted. \
                 Run 'Deep re-index' on it first.",
                s.file_name
            ),
            vec![],
        ),
    }
}

async fn corpus(
    pool: &Db,
    scope: Option<&[String]>,
    unit: Option<&str>,
) -> (String, Vec<CitationOut>) {
    let counts = counting::count_by_doc_type(pool, scope).await.unwrap_or_default();
    if counts.is_empty() {
        return ("No indexed documents are in scope to count.".to_string(), vec![]);
    }
    let total: i64 = counts.iter().map(|c| c.n).sum();
    let lines: Vec<String> = counts
        .iter()
        .map(|c| format!("- **{}** {}", c.n, counting::doc_type_label(&c.doc_type, c.n)))
        .collect();

    let lead = match unit.and_then(counting::doc_type_category) {
        Some(cat) => {
            let n = counts.iter().find(|c| c.doc_type == cat).map(|c| c.n).unwrap_or(0);
            format!("There are **{n}** {}.\n\n", counting::doc_type_label(cat, n))
        }
        None => String::new(),
    };

    let answer = format!(
        "{lead}Corpus breakdown ({total} documents total):\n{}",
        lines.join("\n")
    );
    (answer, vec![])
}
