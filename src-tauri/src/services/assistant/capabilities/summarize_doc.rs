//! T033 (US6) — the **summarize-doc** capability: a structured, cited summary of
//! a whole act/rule book, covering 100% of its major parts without dropping any
//! section (FR-005d, SC-009).
//!
//! A whole document exceeds any single window, so this runs over the map-reduce
//! driver ([`super::super::mapreduce`]): the document's **atomic** provisions are
//! read across window-sized batches (full coverage by construction), each batch
//! is extracted with the bureaucrat persona + the summarize fragment + the
//! grounding rules, and the per-batch extracts are synthesised into one cited
//! summary. The result is citation-gated exactly like every other capability —
//! any provision the source text cannot support verbatim is suppressed and
//! flagged (FR-005c/FR-006).

use std::sync::Arc;
use crate::compat::AppHandle;

use crate::db::Db;
use crate::services::assistant::{
    self, capabilities::run_mapreduce_capability, context_budget::EstimateCounter,
    intent::Capability, memory::MemoryContext, persona::PersonaConfig, AssembleInput, TurnResponse,
};
use crate::services::counting;
use crate::services::query_router;
use crate::services::query_trace::QueryTrace;
use crate::services::{hardware_detector::HardwareTier, llm::ModelSlot, scheduler::Scheduler};

/// Token budget for the query-time presentation pass — enough to re-present a
/// 1–3 segment stored summary. Kept modest: a larger budget just gives the small
/// model room to fall into a restate-the-same-points loop (the output is dedup'd
/// anyway, but a tight cap avoids the wasted generation).
const PRESENT_MAX_TOKENS: u32 = 400;

/// Serve the ingest-time WHOLE-DOCUMENT summary (migration 012) when the turn asks
/// to summarize a specific, in-scope, named document. The stored summary is passed
/// through the prose model in a light **presentation pass** (grounded strictly in
/// the stored text, no new facts, no citations — product decision) so the answer
/// reads as a coherent reply to the user's phrasing rather than stitched segments;
/// generation failure falls back to the verbatim stored summary. Returns `None`
/// when the query names no in-scope document that has a pre-baked summary, so the
/// caller falls through to the normal path. Independent of the map-reduce
/// kill-switch, which is why the orchestrator calls this BEFORE the
/// SummarizeDoc→Guidance reroute.
pub async fn try_prebaked(
    pool: &Db,
    model_slot: &ModelSlot,
    tier: &HardwareTier,
    cfg: &PersonaConfig,
    conversation_id: &str,
    query: &str,
    scope_ids: &[String],
    trace: &mut QueryTrace,
) -> Result<Option<TurnResponse>, String> {
    // Match the query to a single named, in-scope document (same matcher the
    // counting capability uses — requires ≥1 filename token to overlap the query,
    // so topical summaries that name no document fall through).
    let sources = counting::sources_for_matching(pool, Some(scope_ids))
        .await
        .unwrap_or_default();
    let Some(idx) = counting::best_source_match(query, &sources) else {
        return Ok(None);
    };
    let src = &sources[idx];

    let segments: Vec<String> = sqlx::query_scalar(
        "SELECT summary_text FROM graph_summary \
         WHERE source_id = $1 AND level = 'document_full' ORDER BY unit_path",
    )
    .bind(&src.id)
    .fetch_all(pool)
    .await
    .map_err(|e| e.to_string())?;
    if segments.is_empty() {
        return Ok(None);
    }

    let body = segments.join("\n\n");
    // Light presentation pass — present the stored summary as an answer to the
    // user's phrasing, grounded strictly in that text (falls back to verbatim on
    // any generation failure so this is never worse than serving the stored text).
    let presented = present_summary(model_slot, tier, query, &src.file_name, &body).await;
    let via_model = presented != body;
    let answer = format!("**Summary — {}**\n\n{}", src.file_name, presented);
    trace.step("Pre-baked document summary (ingest-time)", &[
        ("Document", src.file_name.clone()),
        ("Segments", segments.len().to_string()),
        ("Presentation", if via_model { "LLM presented (grounded in stored summary, no citations)" } else { "verbatim stored summary (presentation fallback)" }.to_string()),
    ]);

    let window =
        assistant::n_ctx::env_override().unwrap_or(assistant::n_ctx::FALLBACK_TRAINED) as usize;
    let mem = MemoryContext::default();
    let plan = assistant::plan_budget(
        &EstimateCounter, window, Capability::SummarizeDoc, cfg, false, &mem, query, &[],
    );

    Ok(Some(assistant::assemble_response(AssembleInput {
        conversation_id: conversation_id.to_string(),
        capability: Capability::SummarizeDoc,
        answer,
        // The summary IS its own context — it is grounded in the document at ingest
        // and, by product decision, presented without per-claim citations.
        context: &body,
        citations: vec![],
        plan: &plan,
        fill_threshold: cfg.settings.context_fill_threshold,
        // No single grounded prompt is assembled here.
        chat_fill: None,
        offer_tailor: false,
    })))
}

/// Present the stored whole-document summary as a coherent reply to the user's
/// request via the prose model. STRICTLY grounded: the model is told to use only
/// the stored summary and add nothing, so it smooths presentation (and any residual
/// ingest-parse artefacts in the underlying corpus) without inventing facts —
/// citation-free by product decision. Returns the stored `body` verbatim on any
/// generation failure or empty output, so the pass is never worse than serving it.
async fn present_summary(
    model_slot: &ModelSlot,
    tier: &HardwareTier,
    query: &str,
    file_name: &str,
    body: &str,
) -> String {
    let prompt = format!(
        "You are briefing a government officer. They asked:\n\"{query}\"\n\n\
         Below is an accurate, pre-written summary of the document \"{file_name}\". Present it as a \
         clear, well-organised answer to their request, in plain language. Use ONLY the information \
         in the summary — do NOT add any facts, figures, numbers, dates, or details that are not in \
         it, and do NOT invent section or rule numbers. If the request asks about something the \
         summary does not cover, say the summary does not address that rather than guessing. Write a \
         SINGLE, concise summary of a few short paragraphs; do NOT repeat points or restate the same \
         idea in different words. Output only the answer.\
         \n\nSUMMARY:\n{body}\n\nANSWER:"
    );
    match query_router::generate_direct(model_slot, tier, &prompt, PRESENT_MAX_TOKENS, None, None).await {
        Ok(out) => {
            let cleaned = dedupe_repeats(&strip_think(&out));
            if cleaned.is_empty() { body.to_string() } else { cleaned }
        }
        Err(e) => {
            log::warn!("try_prebaked: presentation pass failed ({e}); serving stored summary verbatim");
            body.to_string()
        }
    }
}

/// Drop sentences the model has already made (global, not windowed) — the small
/// prose model keeps restating a summary's points, often REWORDED, until it hits
/// the token cap. Exact-match dedup misses the reworded repeats, so this compares
/// each sentence's set of significant words (>4 chars) against every kept sentence
/// and drops one that overlaps an earlier one by ≥60% of the smaller word set.
/// Sentences with <4 significant words (short connectors) are always kept.
/// `pub(crate)` so the ingest-time document_full builder applies the SAME
/// near-duplicate dedup to the stored summary (not just the query-time pass).
pub(crate) fn dedupe_repeats(s: &str) -> String {
    const OVERLAP_DROP: f32 = 0.6;
    let mut out = String::with_capacity(s.len());
    let mut kept: Vec<std::collections::HashSet<String>> = Vec::new();
    for part in s.split_inclusive(['.', '\n']) {
        let words: std::collections::HashSet<String> = part
            .to_ascii_lowercase()
            .split(|c: char| !c.is_alphanumeric())
            .filter(|w| w.len() > 4)
            .map(|w| w.to_string())
            .collect();
        if words.len() >= 4 {
            let dup = kept.iter().any(|k| {
                let inter = words.iter().filter(|w| k.contains(*w)).count();
                let denom = words.len().min(k.len());
                denom > 0 && (inter as f32 / denom as f32) >= OVERLAP_DROP
            });
            if dup {
                continue; // a near-duplicate of a sentence already kept → drop it
            }
            kept.push(words);
        }
        out.push_str(part);
    }
    out.trim().to_string()
}

/// Strip a leading `<think>…</think>` reasoning block (the prose model can leak one
/// despite `NOVIQ_NO_THINK`). An UNCLOSED leading `<think>` means the whole output
/// is reasoning → return empty so the caller falls back to the stored summary.
fn strip_think(s: &str) -> String {
    if let Some(end) = s.find("</think>") {
        s[end + "</think>".len()..].trim().to_string()
    } else if s.trim_start().starts_with("<think>") {
        String::new()
    } else {
        s.trim().to_string()
    }
}

/// Run a whole-document summary end-to-end. Reads every in-scope provision via
/// map-reduce (no section skipped), then gates + assembles the cited summary —
/// the shared map-reduce runner with the `summarize_doc` persona fragment.
#[allow(clippy::too_many_arguments)]
pub async fn run(
    pool: &Db,
    embedder: &Arc<crate::services::embedder::EmbedderState>,
    model_slot: &ModelSlot,
    tier: &HardwareTier,
    scheduler: &Arc<Scheduler>,
    app: Option<&AppHandle>, // T009b: None on the headless HTTP path
    cfg: &PersonaConfig,
    mem: &MemoryContext,
    conversation_id: &str,
    text: &str,
    scope_ids: &[String],
    trace: &mut QueryTrace,
) -> Result<TurnResponse, String> {
    run_mapreduce_capability(
        pool, embedder, model_slot, tier, scheduler, app, cfg, mem, conversation_id, text,
        scope_ids, Capability::SummarizeDoc, trace,
    )
    .await
}

#[cfg(test)]
mod tests {
    use crate::services::assistant::capabilities::guidance;
    use crate::services::assistant::intent::Capability;
    use crate::services::assistant::persona::PersonaConfig;

    #[test]
    fn dedupe_repeats_kills_restated_sentences() {
        use super::dedupe_repeats;
        let looped = "The rules govern discipline and penalties for civil servants. \
                      They set out inquiry procedures for accused officers. \
                      The rules govern penalties and discipline for civil servants.";
        let out = dedupe_repeats(looped);
        // exact + reworded repeat of the first sentence is dropped; the distinct one stays
        assert!(out.contains("inquiry procedures"), "{out}");
        assert_eq!(out.matches("discipline").count(), 1, "reworded repeat not dropped: {out}");
    }

    #[test]
    fn dedupe_repeats_keeps_distinct_sentences() {
        use super::dedupe_repeats;
        let text = "Penalties range from censure to dismissal for misconduct. \
                    An anti-corruption council investigates corrupt practices by officers.";
        let out = super::dedupe_repeats(text);
        assert!(out.contains("censure to dismissal"), "{out}");
        assert!(out.contains("anti-corruption council"), "{out}");
    }

    #[test]
    fn strip_think_removes_leading_reasoning_block() {
        use super::strip_think;
        assert_eq!(strip_think("<think>\nplanning\n</think>\n\nThe Rules govern discipline."), "The Rules govern discipline.");
        assert_eq!(strip_think("  Plain answer.  "), "Plain answer.");
        // unclosed leading <think> = all reasoning → empty (caller falls back to stored)
        assert_eq!(strip_think("<think> still reasoning with no close"), "");
    }

    #[test]
    fn summarize_system_prompt_carries_coverage_instruction() {
        let cfg = PersonaConfig::get();
        let sp = guidance::system_prompt(cfg, Capability::SummarizeDoc, false).to_lowercase();
        assert!(sp.contains("every major") || sp.contains("without dropping"), "{sp}");
        // grounding/citation machinery retained
        assert!(sp.contains("[1]") || sp.contains("bracketed source index"), "{sp}");
    }
}
