//! Feature 015 — per-capability handlers behind the single conversational
//! surface. Each capability shares the four invariants (persona, citation gate,
//! context allocator, deterministic-where-structural) but differs in how it
//! retrieves and assembles. Phase 3 lands the single-pass [`guidance`] handler
//! (also the bridge for not-yet-specialised capabilities); later phases add the
//! map-reduce handlers (summarize/compare/conflict/action-items) and drafting.

pub mod action_items;
pub mod additive;
pub mod amendment_chain;
pub mod compare;
pub mod conflict;
pub mod count;
pub mod draft;
pub mod guidance;
pub mod notice_assist;
pub mod summarize_doc;
pub mod tabular;
pub mod transform;

use std::sync::Arc;
use crate::compat::AppHandle;

use crate::db::{repositories::chunks, Db};
use crate::services::assistant::{
    self, capabilities::guidance::system_prompt, context_budget::EstimateCounter,
    intent::Capability, memory::MemoryContext, persona::PersonaConfig, AssembleInput, CitationOut,
    TurnResponse,
};
use crate::services::{hardware_detector::HardwareTier, llm::ModelSlot, scheduler::Scheduler};

/// The atomic provisions of the in-scope documents, in document order. A
/// provision is a **leaf** chunk (one with no child chunks) so a parent
/// container's text is not double-counted; if a document has no hierarchy every
/// chunk is a leaf. Returns the provision texts plus a parallel citation per
/// provision — the set the whole-/cross-document capabilities cover, each tagged
/// with its source instrument (so compare/conflict can attribute per document).
#[allow(dead_code)] // kept for the deferred whole-document audit path (dig-deeper iterations)
pub(crate) async fn gather_provisions(
    pool: &Db,
    scope_ids: &[String],
) -> Result<(Vec<String>, Vec<CitationOut>), String> {
    let mut texts = Vec::new();
    let mut cites = Vec::new();
    let mut n = 0i64;
    for source_id in scope_ids {
        let all = chunks::list_by_source(pool, source_id)
            .await
            .map_err(|e| e.to_string())?;
        // Clean display name for this source (no ordering prefix / extension).
        let file_name: Option<String> =
            sqlx::query_scalar("SELECT file_name FROM sources WHERE id::text = $1")
                .bind(source_id)
                .fetch_optional(pool)
                .await
                .ok()
                .flatten();
        let display = file_name
            .map(|f| assistant::clean_document_name(&f))
            .unwrap_or_else(|| source_id.clone());
        // Leaves only: a chunk that is not the parent of any other chunk.
        let parents: std::collections::HashSet<&str> = all
            .iter()
            .filter_map(|c| c.parent_chunk_id.as_deref())
            .collect();
        for c in &all {
            if parents.contains(c.id.as_str()) || c.content.trim().is_empty() {
                continue;
            }
            n += 1;
            texts.push(c.content.clone());
            cites.push(CitationOut {
                n,
                instrument: display.clone(),
                path: c.unit_label.clone().unwrap_or_default(),
                verbatim: c.content.clone(),
                page: c.page_number,
                chunk_id: Some(c.id.clone()),
                source_id: Some(source_id.clone()),
                provenance: None, // F027 — stamped by `label_citation_provenance`
            });
        }
    }
    Ok((texts, cites))
}

/// Retrieval-scoped provision gather for map-reduce (2026-07-06): the doubled-K
/// hybrid slice most relevant to the query, instead of every leaf chunk of the
/// visible corpus. A whole-corpus SPPRA-class question used to plan 60+ map
/// batches (10+ minutes); the retrieval slice caps it at a handful. Citations
/// keep the provision `unit_label` via one batched lookup. Relevance order.
/// One tranche only — officer-driven deeper iterations are deferred.
pub(crate) async fn gather_provisions_retrieved(
    pool: &Db,
    embedder: &Arc<crate::services::embedder::EmbedderState>,
    query_text: &str,
    scope_ids: &[String],
) -> Result<(Vec<String>, Vec<CitationOut>), String> {
    let retrieved =
        crate::services::query_router::retrieve_for_mapreduce(pool, embedder, query_text, scope_ids)
            .await
            .map_err(|e| e.to_string())?;

    // unit_label per chunk id (the citation "path" — not in the retrieval tuple).
    let mut labels: std::collections::HashMap<String, Option<String>> =
        std::collections::HashMap::new();
    if !retrieved.is_empty() {
        let ids: Vec<String> = retrieved.iter().map(|c| c.0.clone()).collect();
        let placeholders = crate::db::pg_in(ids.len(), 1);
        let sql =
            format!("SELECT id, unit_label FROM chunks WHERE id IN ({placeholders})");
        let mut q = sqlx::query_as::<_, (String, Option<String>)>(&sql);
        for id in &ids {
            q = q.bind(id);
        }
        if let Ok(rows) = q.fetch_all(pool).await {
            labels = rows.into_iter().collect();
        }
    }

    let mut texts = Vec::new();
    let mut cites = Vec::new();
    for (n, (chunk_id, _source_id, content, page, file_name)) in retrieved.iter().enumerate() {
        if content.trim().is_empty() {
            continue;
        }
        texts.push(content.clone());
        cites.push(CitationOut {
            n: (n + 1) as i64,
            instrument: assistant::clean_document_name(file_name),
            path: labels.get(chunk_id).cloned().flatten().unwrap_or_default(),
            verbatim: content.clone(),
            page: *page,
            // Carry the chunk through so the UI can open the source AT this
            // passage when the citation is clicked, not merely preview it.
            chunk_id: Some(chunk_id.clone()),
            source_id: Some(_source_id.clone()),
            provenance: None, // F027 — stamped by `label_citation_provenance`
        });
    }
    Ok((texts, cites))
}

/// Run a whole-/cross-document capability end-to-end over the map-reduce driver:
/// map over the **retrieval-scoped** provision slice (doubled-K hybrid — see
/// `gather_provisions_retrieved`), extract each window-sized batch under the
/// capability's persona fragment + grounding rules, synthesise the per-batch
/// extracts, then gate + assemble the cited response. Shared by
/// summarize/compare/conflict/action-items — they differ only by the persona
/// fragment (`cap`) the prompt selects.
#[allow(clippy::too_many_arguments)]
pub(crate) async fn run_mapreduce_capability(
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
    cap: Capability,
    trace: &mut crate::services::query_trace::QueryTrace,
) -> Result<TurnResponse, String> {
    let (provisions, cites) = gather_provisions_retrieved(pool, embedder, text, scope_ids).await?;
    trace.step("Gather provisions (map-reduce)", &[
        ("Capability", cap.as_str().to_string()),
        ("Provisions swept", provisions.len().to_string()),
        ("Citations", cites.len().to_string()),
        ("Sources in scope", scope_ids.len().to_string()),
    ]);

    // System prompt for both the per-batch extract and the final synthesis: the
    // capability fragment carries the behaviour (cover every section / align by
    // topic / candidate-only conflicts / one item per mandate); the grounding
    // rules keep the `[n]` citation machinery intact.
    let sys = system_prompt(cfg, cap, false);
    let gen_reserve = cfg.gen_reserve(cap);

    let mr = assistant::mapreduce::run(
        model_slot, tier, scheduler, app, &sys, &sys, text, &provisions, gen_reserve,
    )
    .await?;
    trace.step("Map-reduce synthesis", &[
        ("Batches", mr.batches.to_string()),
        ("Answer", format!("{} chars", mr.answer.chars().count())),
    ]);

    // Audited budget plan for the context-fill notice (the map-reduce passes each
    // fit by construction; this reports the whole-corpus pressure for the turn).
    let window = assistant::resolve_active_window(model_slot, tier);
    let plan = assistant::plan_budget(
        &EstimateCounter, window, cap, cfg, false, mem, text, &provisions,
    );
    log::info!(
        "F015 {} batches={} provisions={} window={} law_overflow={}",
        cap.as_str(), mr.batches, provisions.len(), plan.n_ctx, plan.law_overflow,
    );

    // Gate over the full source text — grounded refs survive, fabrications flag.
    let context = provisions.join("\n\n");
    Ok(assistant::assemble_response(AssembleInput {
        conversation_id: conversation_id.to_string(),
        capability: cap,
        answer: mr.answer,
        context: &context,
        citations: cites,
        plan: &plan,
        fill_threshold: cfg.settings.context_fill_threshold,
        chat_fill: None,
        offer_tailor: false,
    }))
}
