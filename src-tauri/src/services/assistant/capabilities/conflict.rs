//! T039 (US7) — the **conflict** capability: surface POTENTIAL contradictions
//! between the named documents, each citing the conflicting provision from BOTH
//! documents and framed as a candidate for the officer to judge — never asserted
//! as established fact (FR-005g, research R8).
//!
//! Mechanically this is the shared map-reduce runner with the `conflict` persona
//! fragment: provisions are read across batches and the model proposes candidate
//! conflicts. The citation gate is the hard guarantee behind SC-011 — a conflict
//! statement referencing a provision the source text cannot support verbatim is
//! suppressed, so no conflict is ever asserted without both grounded citations.
//! Semantic contradiction is genuinely hard; over-claiming breaks the
//! citation-grade contract, so the output is candidates-for-review only.

use std::sync::Arc;
use crate::compat::AppHandle;

use crate::db::Db;
use crate::services::assistant::{
    capabilities::run_mapreduce_capability, intent::Capability, memory::MemoryContext,
    persona::PersonaConfig, TurnResponse,
};
use crate::services::query_trace::QueryTrace;
use crate::services::{hardware_detector::HardwareTier, llm::ModelSlot, scheduler::Scheduler};

/// Run a cross-document conflict scan end-to-end — the shared map-reduce runner
/// with the `conflict` persona fragment over the in-scope documents' provisions.
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
        scope_ids, Capability::Conflict, trace,
    )
    .await
}

#[cfg(test)]
mod tests {
    use crate::services::assistant::capabilities::guidance;
    use crate::services::assistant::intent::Capability;
    use crate::services::assistant::persona::PersonaConfig;

    #[test]
    fn conflict_prompt_demands_both_citations_and_never_asserts() {
        let cfg = PersonaConfig::get();
        let sp = guidance::system_prompt(cfg, Capability::Conflict, false).to_lowercase();
        assert!(sp.contains("both"), "must cite both provisions: {sp}");
        assert!(sp.contains("potential") || sp.contains("candidate"), "candidates only: {sp}");
        assert!(sp.contains("never assert") || sp.contains("never"), "never asserts: {sp}");
    }
}
