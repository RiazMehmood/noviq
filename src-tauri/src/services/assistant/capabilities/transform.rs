//! Transform capability — rewrite the PREVIOUS assistant answer per the
//! officer's instruction ("make it more formal", "shorter", "in Urdu", "as an
//! office note", …) with **no new retrieval**. The prior turn's citations are
//! carried forward verbatim from `query_log.citations_json`, so grounding is
//! preserved while only the form (tone, length, language, format) changes.

use std::sync::Arc;

use crate::compat::AppHandle;
use crate::db::Db;
use crate::services::assistant::{
    self, context_budget::EstimateCounter, intent::Capability, memory::MemoryContext,
    persona::PersonaConfig, AssembleInput, CitationOut, TurnResponse,
};
use crate::services::query_trace::QueryTrace;
use crate::services::{
    hardware_detector::HardwareTier,
    llm::ModelSlot,
    query_router,
    scheduler::{Scheduler, WorkloadKind},
};

#[allow(clippy::too_many_arguments)]
pub async fn run(
    pool: &Db,
    model_slot: &ModelSlot,
    tier: &HardwareTier,
    scheduler: &Arc<Scheduler>,
    app: Option<&AppHandle>,
    cfg: &PersonaConfig,
    mem: &MemoryContext,
    conversation_id: &str,
    text: &str,
    prior_answer: &str,
    trace: &mut QueryTrace,
) -> Result<TurnResponse, String> {
    trace.step("Transform (restyle prior answer)", &[
        ("Mode", "rewrite prior answer — no new retrieval".to_string()),
        ("Prior answer", format!("{} chars", prior_answer.chars().count())),
    ]);
    // Carry the prior turn's citations forward (best-effort — a missing or
    // malformed row degrades to an uncited rewrite, never a failed turn).
    let cites: Vec<CitationOut> =
        match crate::db::repositories::query_log::latest_citations_for_conversation(
            pool,
            conversation_id,
        )
        .await
        {
            Ok(Some(json)) => serde_json::from_str(&json).unwrap_or_default(),
            _ => Vec::new(),
        };

    let prompt = query_router::build_transform_prompt(text, prior_answer);

    let handle = scheduler.register(WorkloadKind::Chat);
    let cancel = handle.cancellation_token();
    let max_tokens = cfg.gen_reserve(Capability::Guidance) as u32;
    let out =
        query_router::generate_direct(model_slot, tier, &prompt, max_tokens, Some(cancel), app.cloned())
            .await;
    scheduler.unregister(handle.id);
    let answer = out.map_err(|e| e.to_string())?;

    // Gate haystack: the carried citations' verbatim passages — a preserved
    // `[n]` marker stays grounded against the same source text as the original.
    let context = cites.iter().map(|c| c.verbatim.clone()).collect::<Vec<_>>().join("\n\n");
    let window = assistant::resolve_active_window(model_slot, tier);
    let plan = assistant::plan_budget(&EstimateCounter, window, Capability::Guidance, cfg, false, mem, text, &[]);

    Ok(assistant::assemble_response(AssembleInput {
        conversation_id: conversation_id.to_string(),
        capability: Capability::Guidance,
        answer,
        context: &context,
        citations: cites,
        plan: &plan,
        fill_threshold: cfg.settings.context_fill_threshold,
        // No single grounded prompt is assembled here.
        chat_fill: None,
        offer_tailor: true,
    }))
}
