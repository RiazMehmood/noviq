//! Tauri commands: query sessions and Q&A.

use serde::{Deserialize, Serialize};
use crate::db::Db;
use std::sync::Arc;
use tauri::State;

use crate::db::repositories::sessions as session_repo;
use crate::services::{
    cloud_llm,
    embedder::EmbedderState,
    hardware_detector::HardwareTier,
    intent_router::{self, ChatQuery, ChatResponse, Scope as IntentScope},
    llm::ModelSlot,
    query_router::{Citation, HistoryTurn},
    reranker::RerankerSlot,
    scheduler::{Scheduler, WorkloadKind},
};

#[derive(Debug, Serialize, Deserialize)]
pub struct SessionDto {
    pub id: String,
    pub notebook_id: String,
    pub created_at: i64,
    pub updated_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MessageDto {
    pub id: String,
    pub session_id: String,
    pub role: String,
    pub content: String,
    pub citations: Vec<Citation>,
    pub tier_used: Option<String>,
    pub is_incomplete: bool,
    pub created_at: i64,
}


#[tauri::command]
pub async fn create_session(
    pool: State<'_, Db>,
    notebook_id: String,
) -> Result<SessionDto, String> {
    let session = session_repo::create(&pool, &notebook_id)
        .await
        .map_err(|e| e.to_string())?;
    Ok(SessionDto {
        id: session.id,
        notebook_id: session.notebook_id,
        created_at: session.created_at,
        updated_at: session.updated_at,
    })
}

#[tauri::command]
pub async fn cancel_chat(
    scheduler: State<'_, Arc<Scheduler>>,
    chat_id: String,
) -> Result<(), String> {
    if let Ok(id) = uuid::Uuid::parse_str(&chat_id) {
        scheduler.request_cancel(id);
    }
    Ok(())
}

/// F026 stop button (desktop) — cancel the conversation's live chat turn.
///
/// Mirrors `POST /api/cancel_turn`: the shared guidance capability records each
/// turn's workload id under its conversation (`note_chat_turn`), so the officer's
/// Stop button can halt a wrong / slow query by conversation id without the
/// frontend having to track the internal workload id. Returns whether a live turn
/// was found (idempotent — `false` just means it already finished).
#[tauri::command]
pub async fn cancel_turn(
    scheduler: State<'_, Arc<Scheduler>>,
    conversation_id: String,
) -> Result<bool, String> {
    Ok(scheduler.cancel_conversation(&conversation_id))
}

/// Feature 007 — single chat surface, intent-routed.
///
/// Calls the new `intent_router::dispatch` instead of the legacy
/// `query_router::route_scoped` directly. Reads the persistent cloud-LLM
/// toggle from `cloud_llm::load_settings_cached()` — the chat surface itself
/// does NOT take a cloud flag (FR-029). Returns the `ChatResponse` directly
/// instead of emitting `chat://done` events; the frontend awaits the result
/// in-line. Streaming variant can be added later if needed.
#[tauri::command]
pub async fn chat_query_legal(
    pool: State<'_, Db>,
    embedder: State<'_, Arc<EmbedderState>>,
    model_slot: State<'_, Arc<ModelSlot>>,
    reranker_slot: State<'_, Arc<RerankerSlot>>,
    tier: State<'_, HardwareTier>,
    scheduler: State<'_, Arc<Scheduler>>,
    app: tauri::AppHandle,
    session_id: String,
    notebook_id: String,
    query_text: String,
    scope: Option<IntentScope>,
    thread_id: Option<String>,
) -> Result<ChatResponse, String> {
    use tauri::Emitter;
    let chat_handle = scheduler.register(WorkloadKind::Chat);
    let chat_id = chat_handle.id.to_string();
    let cancel = chat_handle.cancellation_token();
    // Surface the chat id so the frontend can cancel mid-flight via the
    // existing `cancel_chat` command (FR-013: cancellable chat).
    let _ = app.emit("chat://started", serde_json::json!({ "chat_id": chat_id }));

    let history: Vec<HistoryTurn> = session_repo::list_messages(&pool, &session_id)
        .await
        .unwrap_or_default()
        .into_iter()
        .rev()
        .take(4)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .map(|m| HistoryTurn { role: m.role, content: m.content })
        .collect();

    session_repo::insert_message(&pool, &session_id, "user", &query_text, None, None, false)
        .await
        .map_err(|e| e.to_string())?;

    // Persistent Settings flag — never per-query.
    let use_cloud = cloud_llm::load_settings_cached().enabled;
    let query = ChatQuery {
        text: query_text,
        scope: scope.unwrap_or_default(),
        use_cloud,
        thread_id,
    };

    let resp = intent_router::dispatch(
        &pool,
        &embedder,
        &model_slot,
        &reranker_slot,
        &tier,
        &notebook_id,
        &session_id,
        &history,
        query,
        Some(cancel),
        Some(app),
    )
    .await
    .map_err(|e| e.to_string());

    scheduler.unregister(chat_handle.id);
    resp
}

#[tauri::command]
pub async fn get_session_history(
    pool: State<'_, Db>,
    session_id: String,
) -> Result<Vec<MessageDto>, String> {
    let messages = session_repo::list_messages(&pool, &session_id)
        .await
        .map_err(|e| e.to_string())?;

    let dtos = messages
        .into_iter()
        .map(|m| {
            let citations: Vec<Citation> = m
                .citations_json
                .as_deref()
                .and_then(|j| serde_json::from_str(j).ok())
                .unwrap_or_default();
            MessageDto {
                id: m.id,
                session_id: m.session_id,
                role: m.role,
                content: m.content,
                citations,
                tier_used: m.tier_used,
                is_incomplete: m.is_incomplete != 0,
                created_at: m.created_at,
            }
        })
        .collect();

    Ok(dtos)
}
