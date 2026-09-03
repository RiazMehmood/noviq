//! Feature 015 — Tauri commands for the unified bureaucrat chat assistant.
//!
//! T004 (module + registration) + T014 (`send_message` turn orchestration).
//! `send_message` is the single turn entrypoint: classify → retrieve → allocate
//! → generate → gate → assemble (contracts/assistant-commands.md). It streams
//! `assistant://progress { phase }` and reuses the proven grounded RAG pipeline
//! (`query_router::route_scoped`, which already streams `chat://token`) for
//! retrieval + generation, then wraps the result in the F015 response shape with
//! the model-independent citation gate and an audited budget plan (Σ ≤ window).
//!
//! Designed to be wrapped 1:1 over HTTP by the IBA thin-client server (Spec 014).

use std::sync::Arc;
use tauri::{AppHandle, State};

use serde::Serialize;

use crate::db::Db;
// The turn orchestration moved to `assistant::run_turn` (T011) so the headless
// HTTP server shares one implementation; this module keeps only what the other
// commands (export_draft / model_status / amendment-chain / doc confirm) need.
use crate::services::assistant::{capabilities::amendment_chain, memory, TurnResponse};
use crate::services::{
    embedder::EmbedderState,
    hardware_detector::HardwareTier,
    llm::ModelSlot,
    reranker::RerankerSlot,
    scheduler::Scheduler,
};

/// Primary turn entrypoint — intent-routed, serves all capabilities (Phase 2
/// wires the guidance path end-to-end; dedicated capability handlers land in
/// later phases).
#[tauri::command]
pub async fn send_message(
    pool: State<'_, Db>,
    embedder: State<'_, Arc<EmbedderState>>,
    model_slot: State<'_, Arc<ModelSlot>>,
    reranker_slot: State<'_, Arc<RerankerSlot>>,
    tier: State<'_, HardwareTier>,
    scheduler: State<'_, Arc<Scheduler>>,
    app: AppHandle,
    conversation_id: String,
    text: String,
    // Composer document scope (document-tree checkboxes + @-mention chips). None
    // / empty = no restriction (pre-existing behaviour); a non-empty set narrows
    // retrieval to those source ids.
    source_ids: Option<Vec<String>>,
    // "Search all documents" widen button (scope-routing 2026-07). None/false =
    // default narrow-to-attachment scoping; true = search the full visible corpus.
    widen: Option<bool>,
) -> Result<TurnResponse, String> {
    // T011/T014: the turn orchestration now lives in `assistant::run_turn` so the
    // headless HTTP server (`/api/query`) drives the exact same pipeline with
    // `app: None`. This command is the Tauri-transport wrapper — extract managed
    // state, pass `Some(&app)` to keep the desktop's live progress events.
    // F021: desktop is single-operator (no auth) — unrestricted scope keeps
    // pre-021 behaviour (retrieves across everything indexed).
    crate::services::assistant::run_turn(
        &pool, &embedder, &model_slot, &reranker_slot, &tier, &scheduler,
        Some(&app), &conversation_id, &text,
        &crate::db::repositories::sources::RequestScope::unrestricted(),
        source_ids.as_deref(),
        widen.unwrap_or(false),
    )
    .await
}

/// T027 (US4, FR-022) — render a produced draft to copy-paste-ready **plain
/// text**, with no letterhead, salutation, or signature. Reuses the drafting
/// exporter (`services/drafting/export.rs`): the stored assistant prose is
/// wrapped in a single body block so the officer copies exactly the body the
/// model produced — nothing is decorated or added.
#[tauri::command]
pub async fn export_draft(
    pool: State<'_, Db>,
    conversation_id: String,
    message_id: String,
) -> Result<ExportDraftOut, String> {
    let content: Option<String> = sqlx::query_scalar(
        "SELECT content FROM chat_message
         WHERE id = $1 AND conversation_id = $2 AND role = 'assistant'",
    )
    .bind(&message_id)
    .bind(&conversation_id)
    .fetch_optional(pool.inner())
    .await
    .map_err(|e| e.to_string())?;

    let content = content.ok_or_else(|| "draft message not found".to_string())?;

    use crate::services::drafting::{export, Block, DraftOutput};
    let out = DraftOutput { blocks: vec![Block::Prose { label: None, text: content }] };
    Ok(ExportDraftOut { text: export::draft_to_text(&out) })
}

/// Export payload (contracts/assistant-commands.md `export_draft`).
#[derive(Debug, Clone, Serialize)]
pub struct ExportDraftOut {
    pub text: String,
}

/// T029 (US5, FR-019/FR-010) — surface the active model and the window derived
/// for it, so the officer/UI can see how much law + history the current model
/// can hold (and that it scales when a bigger model is dropped into `models/`).
#[tauri::command]
pub async fn model_status(
    model_slot: State<'_, Arc<ModelSlot>>,
    tier: State<'_, HardwareTier>,
) -> Result<ModelStatusOut, String> {
    use crate::services::{assistant::n_ctx, query_router};

    let (rel_path, _label, _notice) = query_router::select_model_for_tier(&tier);
    let model_path = query_router::resolve_model_path(&rel_path);
    let model_file = model_path
        .split(['/', '\\'])
        .next_back()
        .unwrap_or(&model_path)
        .to_string();

    let (trained_ctx, resolved) =
        model_slot.window_info(&model_path, &tier).map_err(|e| e.to_string())?;

    let overridden = n_ctx::env_override().is_some();
    Ok(ModelStatusOut {
        model_file,
        n_ctx: resolved,
        trained_ctx,
        // The window is RAM/VRAM-clamped when it lands below the trained length
        // for a reason other than an explicit override.
        ram_clamped: !overridden && resolved < trained_ctx,
        source: if overridden { "ECHO_N_CTX" } else { "auto" }.to_string(),
    })
}

/// `model_status` payload (contracts/assistant-commands.md).
#[derive(Debug, Clone, Serialize)]
pub struct ModelStatusOut {
    pub model_file: String,
    pub n_ctx: u32,
    pub trained_ctx: u32,
    pub ram_clamped: bool,
    pub source: String,
}

/// T035 (US8, FR-005f) — deterministic amendment history for a provision. The
/// order/dates/instruments come from structured provenance (`amendment_events`),
/// NOT from a model: the chain is built and ordered oldest→recent in pure code
/// (SC-010). Returns the structured chain for the UI; any narration only
/// restates it.
#[tauri::command]
pub async fn get_amendment_chain(
    pool: State<'_, Db>,
    provision_ref: String,
) -> Result<AmendmentChainOut, String> {
    use crate::db::repositories::graph;
    let events = graph::amendment_events_for_provision(pool.inner(), &provision_ref)
        .await
        .map_err(|e| e.to_string())?;
    let chain = amendment_chain::build_chain(&provision_ref, &events);
    Ok(AmendmentChainOut {
        chain,
        ordered: "oldest_to_recent".to_string(),
        narration: None,
    })
}

/// `get_amendment_chain` payload (contracts/assistant-commands.md).
#[derive(Debug, Clone, Serialize)]
pub struct AmendmentChainOut {
    pub chain: Vec<crate::services::assistant::capabilities::amendment_chain::AmendmentLink>,
    pub ordered: String,
    pub narration: Option<String>,
}

/// T041 (US7, FR-005e) — resolve candidate document mentions to the source ids of
/// the **indexed** documents that match, so the officer can confirm the scope for
/// a summarize/compare/conflict/action-items turn. Matching is deterministic: a
/// candidate matches a source when, case-insensitively, the candidate is a
/// substring of the file name or vice-versa (handles "Sindh Local Government Act"
/// ↔ "Sindh Local Government Act 2013.pdf"). Returns the matched ids, de-duped in
/// match order — the UI presents them for the officer's final confirmation.
#[tauri::command]
pub async fn confirm_documents(
    pool: State<'_, Db>,
    conversation_id: String,
    candidates: Vec<String>,
) -> Result<ConfirmDocumentsOut, String> {
    let _ = conversation_id; // scope is resolved per-turn; conversation id is for tracing/parity
    let rows: Vec<(String, String)> =
        sqlx::query_as("SELECT id, file_name FROM sources WHERE indexing_status = 'indexed'")
            .fetch_all(pool.inner())
            .await
            .map_err(|e| e.to_string())?;

    let mut confirmed: Vec<String> = Vec::new();
    let mut seen: std::collections::HashSet<String> = std::collections::HashSet::new();
    for cand in &candidates {
        let needle = cand.trim().to_lowercase();
        if needle.is_empty() {
            continue;
        }
        for (id, file_name) in &rows {
            let hay = file_name.to_lowercase();
            if (hay.contains(&needle) || needle.contains(&hay)) && seen.insert(id.clone()) {
                confirmed.push(id.clone());
            }
        }
    }
    Ok(ConfirmDocumentsOut { confirmed })
}

/// `confirm_documents` payload (contracts/assistant-commands.md).
#[derive(Debug, Clone, Serialize)]
pub struct ConfirmDocumentsOut {
    pub confirmed: Vec<String>,
}

/// T045 (US3, FR-018b) — start a fresh conversation from the context-fill notice,
/// carrying the officer's non-superseded pinned case facts forward so they need
/// not restate their situation. The new conversation gets a clean, empty window;
/// the pinned facts are copied into it and a brief recap (the prior running
/// summary + the carried facts) is returned for the UI to show.
#[tauri::command]
pub async fn new_chat_from_notice(
    pool: State<'_, Db>,
    from_conversation_id: String,
) -> Result<NewChatOut, String> {
    let new_id = uuid::Uuid::new_v4().to_string();

    // Carry forward the non-superseded pinned facts (FR-018b).
    let carried = memory::load_pinned_facts(&pool, &from_conversation_id)
        .await
        .map_err(|e| e.to_string())?;
    if !carried.is_empty() {
        // seq 0: these facts pre-date the first turn of the fresh conversation.
        memory::record_pinned_facts(&pool, &new_id, 0, &carried)
            .await
            .map_err(|e| e.to_string())?;
    }

    // Brief recap: the prior running summary, if any, so continuity is preserved.
    let prior_summary: Option<String> = sqlx::query_scalar(
        "SELECT summary_text FROM conversation_summary WHERE draft_session_id = $1",
    )
    .bind(&from_conversation_id)
    .fetch_optional(pool.inner())
    .await
    .map_err(|e| e.to_string())?;

    let facts_line = if carried.is_empty() {
        String::new()
    } else {
        let joined = carried
            .iter()
            .map(|(k, v)| format!("{k}: {v}"))
            .collect::<Vec<_>>()
            .join("; ");
        format!("Your case facts carry over ({joined}).")
    };
    let recap = match prior_summary {
        Some(s) if !s.trim().is_empty() => {
            if facts_line.is_empty() { s } else { format!("{facts_line} Earlier: {s}") }
        }
        _ => facts_line,
    };

    Ok(NewChatOut {
        conversation_id: new_id,
        carried_facts: carried
            .into_iter()
            .map(|(key, value)| CarriedFact { key, value })
            .collect(),
        recap,
    })
}

/// One pinned fact carried into a fresh conversation.
#[derive(Debug, Clone, Serialize)]
pub struct CarriedFact {
    pub key: String,
    pub value: String,
}

/// `new_chat_from_notice` payload (contracts/assistant-commands.md).
#[derive(Debug, Clone, Serialize)]
pub struct NewChatOut {
    pub conversation_id: String,
    pub carried_facts: Vec<CarriedFact>,
    pub recap: String,
}
