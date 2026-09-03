//! T040 (US9) — the **action-items** capability: extract the obligations /
//! directives / required actions from the in-scope document(s) as a list, each
//! citing the provision that mandates it (FR-005c). Runs over the shared
//! map-reduce runner so a whole document is covered across passes (no section
//! skipped), with the `action_items` persona fragment driving the extraction and
//! the citation gate ensuring every listed item is grounded — an item the source
//! text cannot support verbatim is suppressed and flagged (FR-006/FR-007). If a
//! document mandates nothing, the model says so rather than inventing items.

use std::sync::Arc;
use crate::compat::AppHandle;

use crate::db::Db;
use crate::services::assistant::{
    capabilities::run_mapreduce_capability, intent::Capability, memory::MemoryContext,
    persona::PersonaConfig, TurnResponse,
};
use crate::services::query_trace::QueryTrace;
use crate::services::{hardware_detector::HardwareTier, llm::ModelSlot, scheduler::Scheduler};

/// Run action-item extraction end-to-end — the shared map-reduce runner with the
/// `action_items` persona fragment over the in-scope documents' provisions.
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
        scope_ids, Capability::ActionItems, trace,
    )
    .await
}

#[cfg(test)]
mod tests {
    use crate::services::assistant::capabilities::guidance;
    use crate::services::assistant::intent::Capability;
    use crate::services::assistant::persona::PersonaConfig;

    #[test]
    fn action_items_prompt_cites_each_mandate_and_handles_none() {
        let cfg = PersonaConfig::get();
        let sp = guidance::system_prompt(cfg, Capability::ActionItems, false).to_lowercase();
        assert!(sp.contains("cite the provision") || sp.contains("cite"), "must cite mandate: {sp}");
        assert!(sp.contains("none") || sp.contains("if there are none"), "must handle empty: {sp}");
    }
}
