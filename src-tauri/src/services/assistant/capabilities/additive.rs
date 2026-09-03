//! Additive follow-up — EXTEND the previous assistant answer with a new aspect
//! ("also cover the penalties", "add a section on leave rules", "expand on the
//! approval limits"). The new aspect runs through the full guidance pipeline
//! (retrieval boosts, carry-over, budget allocator, citation gate), then the
//! result is merged deterministically: the prior answer is kept verbatim, the
//! new answer is appended with its `[n]` markers renumbered to continue after
//! the prior citations, and the citation lists are unioned (a re-retrieved
//! chunk maps back to its existing number instead of duplicating).

use std::collections::HashMap;
use std::sync::{Arc, OnceLock};

use regex_lite::Regex;

use crate::compat::AppHandle;
use crate::db::Db;
use crate::services::assistant::{
    capabilities::guidance, intent::Capability, memory::MemoryContext, persona::PersonaConfig,
    CitationOut, TurnResponse,
};
use crate::services::query_router::HistoryTurn;
use crate::services::query_trace::QueryTrace;
use crate::services::{
    embedder::EmbedderState, hardware_detector::HardwareTier, llm::ModelSlot,
    reranker::RerankerSlot, scheduler::Scheduler,
};

#[allow(clippy::too_many_arguments)]
pub async fn run(
    pool: &Db,
    embedder: &Arc<EmbedderState>,
    model_slot: &ModelSlot,
    reranker_slot: &RerankerSlot,
    tier: &HardwareTier,
    scheduler: &Arc<Scheduler>,
    app: Option<&AppHandle>,
    cfg: &PersonaConfig,
    mem: &MemoryContext,
    history: &[HistoryTurn],
    conversation_id: &str,
    text: &str,
    scope_ids: &[String],
    prior_answer: &str,
    // scope-routing Phase 3: forwarded to `guidance::run` so an additive follow-up
    // on a live attachment stays strictly within it (see guidance::run).
    scoped_to_attachment: bool,
    // 2026-07-29: per-user "Next steps" offer, threaded through unchanged.
    show_offer: bool,
    trace: &mut QueryTrace,
) -> Result<TurnResponse, String> {
    trace.step("Additive follow-up", &[
        ("Mode", "extend prior answer with a new aspect".to_string()),
        ("Prior answer", format!("{} chars", prior_answer.chars().count())),
    ]);
    // Prior turn's citations (best-effort — missing/malformed degrades to a
    // plain append with the new citations starting at [1]).
    let prior_cites: Vec<CitationOut> =
        match crate::db::repositories::query_log::latest_citations_for_conversation(
            pool,
            conversation_id,
        )
        .await
        {
            Ok(Some(json)) => serde_json::from_str(&json).unwrap_or_default(),
            _ => Vec::new(),
        };

    // New-aspect answer over the proven pipeline. The existing follow-up
    // machinery (prior Q+A in the prompt, keyword carry-over) supplies context,
    // so the raw turn text is the retrieval query.
    let mut resp = guidance::run(
        pool, embedder, model_slot, reranker_slot, tier, scheduler, app, cfg, mem, history,
        conversation_id, text, Capability::Guidance, scope_ids, scoped_to_attachment, show_offer, trace,
    )
    .await?;

    let (merged_cites, mapping) = merge_citations(prior_cites, std::mem::take(&mut resp.citations));
    resp.answer = format!(
        "{}\n\n{}",
        prior_answer.trim_end(),
        remap_citation_markers(&resp.answer, &mapping)
    );
    resp.citations = merged_cites;
    Ok(resp)
}

/// Union prior + new citations. A new citation whose verbatim text already
/// appears in the prior list reuses that number; the rest are renumbered to
/// continue after the prior list. Returns the merged list plus the
/// old-number → new-number mapping for the new answer's markers.
fn merge_citations(
    prior: Vec<CitationOut>,
    new: Vec<CitationOut>,
) -> (Vec<CitationOut>, HashMap<i64, i64>) {
    let mut merged = prior;
    let mut mapping = HashMap::new();
    let mut next = merged.iter().map(|c| c.n).max().unwrap_or(0);
    for mut c in new {
        let old_n = c.n;
        if let Some(existing) = merged.iter().find(|p| p.verbatim == c.verbatim) {
            mapping.insert(old_n, existing.n);
        } else {
            next += 1;
            c.n = next;
            mapping.insert(old_n, next);
            merged.push(c);
        }
    }
    (merged, mapping)
}

/// Rewrite `[n]` markers in the new answer through the merge mapping. Markers
/// with no mapping (model noise beyond the citation list) pass through as-is.
fn remap_citation_markers(text: &str, mapping: &HashMap<i64, i64>) -> String {
    static RE: OnceLock<Regex> = OnceLock::new();
    let re = RE.get_or_init(|| Regex::new(r"\[(\d+)\]").expect("citation-marker regex compiles"));
    re.replace_all(text, |caps: &regex_lite::Captures| {
        let n: i64 = caps[1].parse().unwrap_or(0);
        match mapping.get(&n) {
            Some(m) => format!("[{m}]"),
            None => caps[0].to_string(),
        }
    })
    .into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cite(n: i64, verbatim: &str) -> CitationOut {
        CitationOut {
            n,
            instrument: "Test Act".into(),
            path: format!("Rule {n}"),
            verbatim: verbatim.into(),
            page: None,
            ..Default::default()
        }
    }

    #[test]
    fn merge_renumbers_new_citations_after_prior() {
        let prior = vec![cite(1, "alpha"), cite(2, "beta")];
        let new = vec![cite(1, "gamma"), cite(2, "delta")];
        let (merged, map) = merge_citations(prior, new);
        assert_eq!(merged.len(), 4);
        assert_eq!(merged[2].n, 3);
        assert_eq!(merged[3].n, 4);
        assert_eq!(map[&1], 3);
        assert_eq!(map[&2], 4);
    }

    #[test]
    fn merge_reuses_prior_number_for_same_chunk() {
        let prior = vec![cite(1, "alpha"), cite(2, "beta")];
        let new = vec![cite(1, "beta"), cite(2, "gamma")];
        let (merged, map) = merge_citations(prior, new);
        assert_eq!(merged.len(), 3);
        assert_eq!(map[&1], 2); // "beta" already cited as [2]
        assert_eq!(map[&2], 3);
    }

    #[test]
    fn remap_rewrites_only_mapped_markers() {
        let mut map = HashMap::new();
        map.insert(1, 3);
        map.insert(2, 4);
        assert_eq!(
            remap_citation_markers("Penalties apply [1]; appeals per [2]. Noise [9].", &map),
            "Penalties apply [3]; appeals per [4]. Noise [9]."
        );
    }
}
