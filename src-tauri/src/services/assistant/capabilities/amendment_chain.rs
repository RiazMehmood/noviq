//! T034 (US8) — the **amendment-chain** capability: how a provision (or act) has
//! been amended, ordered oldest→recent, built **deterministically** from the
//! structured amendment provenance (F013 `amendment_events`), with the model only
//! narrating the already-ordered, already-cited chain (FR-005f, SC-010, R6).
//!
//! Amendment order is the highest-confabulation-risk surface, so the chain is
//! pure structure: [`build_chain`] orders the recorded events; [`render_prose`]
//! renders them to citation-grade prose. No model produces order/dates/
//! instruments — 0 model-invented links by construction. A provision with no
//! recorded provenance returns "none recorded" rather than an invented chain.

use crate::db::{repositories::graph::AmendmentEventRow, Db};
use crate::services::assistant::{
    self, context_budget::EstimateCounter, intent::Capability, persona::PersonaConfig,
    AssembleInput, CitationOut, TurnResponse,
};
use crate::services::query_trace::QueryTrace;

/// One ordered link in an amendment chain (the `get_amendment_chain` contract).
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct AmendmentLink {
    pub amending_instrument: String,
    pub date: Option<String>,
    /// The recorded action: `inserted` | `substituted` | `repealed` | `omitted`.
    pub action: String,
    /// The canonical citation of the amended provision.
    pub citation: String,
}

/// Order the recorded amendment events oldest→recent. Sort key is
/// `effective_date` ascending (ISO dates sort lexically); undated events sink to
/// the end in stable input order. Deterministic — no model, 0 invented links.
pub fn build_chain(provision_ref: &str, events: &[AmendmentEventRow]) -> Vec<AmendmentLink> {
    let mut links: Vec<AmendmentLink> = events
        .iter()
        .map(|e| AmendmentLink {
            amending_instrument: e.by_instrument.clone(),
            date: e.effective_date.clone().filter(|d| !d.trim().is_empty()),
            action: e.kind.clone(),
            citation: e
                .citation
                .clone()
                .filter(|c| !c.trim().is_empty())
                .unwrap_or_else(|| provision_ref.to_string()),
        })
        .collect();

    // Stable sort: dated links ascending, undated links last (original order).
    links.sort_by(|a, b| match (&a.date, &b.date) {
        (Some(x), Some(y)) => x.cmp(y),
        (Some(_), None) => std::cmp::Ordering::Less,
        (None, Some(_)) => std::cmp::Ordering::Greater,
        (None, None) => std::cmp::Ordering::Equal,
    });
    links
}

/// Render an ordered chain to citation-grade prose. Pure — every fact comes from
/// the structured chain, so the answer re-gates clean against the chain's own
/// context. Empty chain → an honest "none recorded" statement (never invented).
pub fn render_prose(provision_ref: &str, chain: &[AmendmentLink]) -> String {
    if chain.is_empty() {
        return format!(
            "No amendments are recorded for {provision_ref} in the indexed documents."
        );
    }
    let mut out = format!(
        "{provision_ref} has {} recorded amendment{}, oldest to most recent:\n",
        chain.len(),
        if chain.len() == 1 { "" } else { "s" },
    );
    for (i, l) in chain.iter().enumerate() {
        let date = l.date.as_deref().unwrap_or("date not recorded");
        out.push_str(&format!(
            "{}. {} by {} ({}) — see {}.\n",
            i + 1,
            capitalize(&l.action),
            l.amending_instrument,
            date,
            l.citation,
        ));
    }
    out
}

fn capitalize(s: &str) -> String {
    let mut c = s.chars();
    match c.next() {
        Some(f) => f.to_uppercase().chain(c).collect(),
        None => String::new(),
    }
}

/// The deterministic context the rendered prose is grounded in (so the citation
/// gate passes by construction — every instrument/citation it states is present).
fn chain_context(chain: &[AmendmentLink]) -> String {
    chain
        .iter()
        .map(|l| {
            format!(
                "{} {} {} {}",
                capitalize(&l.action),
                l.amending_instrument,
                l.date.as_deref().unwrap_or(""),
                l.citation,
            )
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// Run an amendment-chain turn: build the chain deterministically, render it, and
/// assemble a grounded response. No model call for order/dates/instruments.
pub async fn run(
    pool: &Db,
    cfg: &PersonaConfig,
    conversation_id: &str,
    provision_ref: &str,
    scope_ids: &[String],
    trace: &mut QueryTrace,
) -> Result<TurnResponse, String> {
    let events = graph_events(pool, provision_ref, scope_ids).await?;
    let chain = build_chain(provision_ref, &events);
    let answer = render_prose(provision_ref, &chain);
    trace.step("Amendment chain (graph/SQL)", &[
        ("Provision", provision_ref.to_string()),
        ("Amendment events", events.len().to_string()),
        ("Chain links", chain.len().to_string()),
        ("Source", "structured amendment provenance — no model".to_string()),
    ]);

    let citations: Vec<CitationOut> = chain
        .iter()
        .enumerate()
        .map(|(i, l)| CitationOut {
            n: (i + 1) as i64,
            instrument: l.amending_instrument.clone(),
            path: l.citation.clone(),
            verbatim: l.date.clone().unwrap_or_default(),
            page: None,
            ..Default::default()
        })
        .collect();

    let window =
        assistant::n_ctx::env_override().unwrap_or(assistant::n_ctx::FALLBACK_TRAINED) as usize;
    let mem = assistant::memory::MemoryContext::default();
    let plan = assistant::plan_budget(
        &EstimateCounter, window, Capability::AmendmentChain, cfg, false, &mem, provision_ref, &[],
    );

    let context = chain_context(&chain);
    Ok(assistant::assemble_response(AssembleInput {
        conversation_id: conversation_id.to_string(),
        capability: Capability::AmendmentChain,
        answer,
        context: &context,
        citations,
        plan: &plan,
        fill_threshold: cfg.settings.context_fill_threshold,
        chat_fill: None,
        offer_tailor: false,
    }))
}

async fn graph_events(
    pool: &Db,
    provision_ref: &str,
    scope_ids: &[String],
) -> Result<Vec<AmendmentEventRow>, String> {
    // F021: the chain only draws on provenance from sources the requester may
    // see (Some([]) fails closed → honest "none recorded").
    crate::db::repositories::graph::amendment_events_for_provision_scoped(
        pool,
        provision_ref,
        Some(scope_ids),
    )
    .await
    .map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ev(kind: &str, instr: &str, date: Option<&str>, cite: &str) -> AmendmentEventRow {
        AmendmentEventRow {
            kind: kind.into(),
            by_instrument: instr.into(),
            effective_date: date.map(|d| d.into()),
            source_page: None,
            citation: Some(cite.into()),
        }
    }

    #[test]
    fn chain_is_ordered_oldest_to_recent() {
        let events = vec![
            ev("substituted", "Act XX of 2016", Some("2016-05-01"), "Section 18"),
            ev("inserted", "Act V of 2013", Some("2013-01-10"), "Section 18"),
            ev("omitted", "Act IX of 2021", Some("2021-11-20"), "Section 18"),
        ];
        let chain = build_chain("Section 18", &events);
        let dates: Vec<_> = chain.iter().map(|l| l.date.clone().unwrap()).collect();
        assert_eq!(dates, vec!["2013-01-10", "2016-05-01", "2021-11-20"]);
    }

    #[test]
    fn undated_events_sink_to_the_end_stably() {
        let events = vec![
            ev("inserted", "Undated A", None, "Section 5"),
            ev("substituted", "Dated", Some("2010-01-01"), "Section 5"),
            ev("omitted", "Undated B", None, "Section 5"),
        ];
        let chain = build_chain("Section 5", &events);
        assert_eq!(chain[0].amending_instrument, "Dated");
        assert_eq!(chain[1].amending_instrument, "Undated A");
        assert_eq!(chain[2].amending_instrument, "Undated B");
    }

    #[test]
    fn empty_chain_states_none_recorded_not_invented() {
        let prose = render_prose("Section 99", &[]);
        assert!(prose.to_lowercase().contains("no amendments"), "{prose}");
        assert!(prose.contains("Section 99"));
    }

    #[test]
    fn rendered_prose_regates_clean_against_chain_context() {
        let events = vec![ev("inserted", "Act V of 2013", Some("2013-01-10"), "Section 18")];
        let chain = build_chain("Section 18", &events);
        let prose = render_prose("Section 18", &chain);
        let context = chain_context(&chain);
        // Every instrument/citation the prose states is in the deterministic context.
        assert!(assistant::citation_gate::gate(&prose, &context).grounded, "{prose}");
    }
}
