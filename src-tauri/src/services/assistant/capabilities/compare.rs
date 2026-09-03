//! T038 (US7) — the **compare** capability: a topic-aligned, cited comparison of
//! the named documents (FR-005e). The in-scope documents' atomic provisions are
//! read across window-sized batches via the shared map-reduce runner, extracted
//! under the `compare` persona fragment (align provisions by topic; cite the
//! provision from EACH document for every point of comparison), then synthesised
//! and citation-gated like every other capability — any point the source text
//! cannot support verbatim is suppressed and flagged (FR-006/FR-007).
//!
//! Scope disambiguation (which documents) is handled before dispatch by the
//! `confirm_documents` command (T041); this handler runs over the confirmed set.

use std::sync::Arc;
use crate::compat::AppHandle;

use crate::db::Db;
use crate::services::assistant::{
    capabilities::run_mapreduce_capability, intent::Capability, memory::MemoryContext,
    persona::PersonaConfig, TurnResponse,
};
use crate::services::query_trace::QueryTrace;
use crate::services::{hardware_detector::HardwareTier, llm::ModelSlot, scheduler::Scheduler};

/// Run a cross-document comparison end-to-end — the shared map-reduce runner with
/// the `compare` persona fragment over the in-scope documents' provisions.
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
        scope_ids, Capability::Compare, trace,
    )
    .await
}

#[cfg(test)]
mod tests {
    use crate::services::assistant::capabilities::guidance;
    use crate::services::assistant::intent::Capability;
    use crate::services::assistant::persona::PersonaConfig;

    #[test]
    fn compare_prompt_aligns_by_topic_and_cites_each_document() {
        let cfg = PersonaConfig::get();
        let sp = guidance::system_prompt(cfg, Capability::Compare, false).to_lowercase();
        assert!(sp.contains("topic"), "must align by topic: {sp}");
        assert!(sp.contains("each document") || sp.contains("each"), "must cite each side: {sp}");
        // grounding/citation machinery retained
        assert!(sp.contains("[1]") || sp.contains("bracketed source index"), "{sp}");
    }
}
