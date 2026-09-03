//! T026 (US4) — the **draft** capability: gather-situation → plain-prose cited
//! draft + approval pipeline, with a **larger generation reserve** (FR-013).
//!
//! A draft is the same grounded single-pass turn as guidance, but framed as a
//! Summary/Note the officer can copy out and send for approval: it produces body
//! prose only (no letterhead/salutation/signature — the draft persona fragment
//! enforces this), cites the law behind each substantive point, and states the
//! approvals/pipeline needed to sanction it. The bigger gen reserve is selected
//! automatically by the allocator from `cfg.gen_reserve(Capability::Draft)`
//! (`bureaucrat.toml` `gen_reserve_draft`), and—unlike guidance—a draft never
//! appends an offer-to-tailor (the assemble step only adds that for `Guidance`).
//!
//! The produced draft is persisted as the assistant turn; the `export_draft`
//! command (T027) renders that turn to plain text on demand (FR-022).
//!
//! Mechanically this delegates to the shared single-pass engine in
//! [`super::guidance::run`] (which is capability-generic), so drafts inherit the
//! exact same citation gate, no-truncation budget plan, and progress streaming.
//! Keeping a dedicated module gives the draft path its own dispatch seam and a
//! home for draft-specific behaviour (export hook, future gather-situation
//! prompts) without entangling the guidance handler.

use std::sync::Arc;
use crate::compat::AppHandle;

use crate::db::Db;
use crate::services::assistant::{
    intent::Capability, memory::MemoryContext, persona::PersonaConfig, TurnResponse,
};
use crate::services::query_trace::QueryTrace;
use crate::services::{
    embedder::EmbedderState, hardware_detector::HardwareTier, llm::ModelSlot,
    query_router::HistoryTurn, reranker::RerankerSlot, scheduler::Scheduler,
};

/// Run a drafting turn end-to-end. Identical pipeline to a guidance turn but
/// dispatched as `Capability::Draft`, which selects the draft persona fragment
/// and the larger generation reserve (FR-013) and suppresses the offer-to-tailor.
#[allow(clippy::too_many_arguments)]
pub async fn run(
    pool: &Db,
    embedder: &Arc<EmbedderState>,
    model_slot: &ModelSlot,
    reranker_slot: &RerankerSlot,
    tier: &HardwareTier,
    scheduler: &Arc<Scheduler>,
    app: Option<&AppHandle>, // T009b: None on the headless HTTP path
    cfg: &PersonaConfig,
    mem: &MemoryContext,
    history: &[HistoryTurn],
    conversation_id: &str,
    text: &str,
    scope_ids: &[String],
    // scope-routing Phase 3: forwarded to `guidance::run` so a draft turn on a
    // live attachment stays strictly within it (see guidance::run).
    scoped_to_attachment: bool,
    // 2026-07-29: per-user "Next steps" offer, threaded through unchanged.
    show_offer: bool,
    trace: &mut QueryTrace,
) -> Result<TurnResponse, String> {
    trace.step("Draft", &[("Pipeline", "guidance (draft persona + reserve)".to_string())]);
    super::guidance::run(
        pool,
        embedder,
        model_slot,
        reranker_slot,
        tier,
        scheduler,
        app,
        cfg,
        mem,
        history,
        conversation_id,
        text,
        Capability::Draft,
        scope_ids,
        scoped_to_attachment,
        show_offer,
        trace,
    )
    .await
}

#[cfg(test)]
mod tests {
    use crate::services::assistant::capabilities::guidance;
    use crate::services::assistant::context_budget::EstimateCounter;
    use crate::services::assistant::intent::Capability;
    use crate::services::assistant::memory::MemoryContext;
    use crate::services::assistant::persona::PersonaConfig;
    use crate::services::assistant::{self, CitationOut};

    fn cfg() -> &'static PersonaConfig {
        PersonaConfig::get()
    }

    #[test]
    fn draft_system_prompt_carries_draft_fragment_and_no_letterhead_rule() {
        let sp = guidance::system_prompt(cfg(), Capability::Draft, false);
        let l = sp.to_lowercase();
        assert!(l.contains("summary") || l.contains("note"), "{sp}");
        assert!(l.contains("no letterhead") || l.contains("body content"), "{sp}");
    }

    #[test]
    fn draft_response_is_cited_prose_with_no_offer_to_tailor() {
        let mem = MemoryContext::default();
        let plan = assistant::plan_budget(
            &EstimateCounter,
            8192,
            Capability::Draft,
            cfg(),
            false,
            &mem,
            "draft a note",
            &[],
        );
        let cites = vec![CitationOut {
            n: 1,
            instrument: "Sindh Civil Servants Act".into(),
            path: "Section 18".into(),
            verbatim: "Section 18 provides leave on full pay.".into(),
            page: Some(3),
            ..Default::default()
        }];
        let resp = guidance::build_response(
            "c1",
            Capability::Draft,
            "This note proposes leave under Section 18.",
            cites,
            &plan,
            None,
            cfg(),
        );
        assert_eq!(resp.capability, "draft");
        assert!(resp.offer.is_none(), "a draft must not offer-to-tailor: {resp:?}");
        assert!(resp.answer.contains("Section 18"));
        assert_eq!(resp.citations.len(), 1);
    }

    #[test]
    fn draft_reserve_is_larger_than_qa() {
        assert!(cfg().gen_reserve(Capability::Draft) > cfg().gen_reserve(Capability::Guidance));
    }
}
