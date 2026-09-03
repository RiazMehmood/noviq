//! Feature 022 — Notice Assist session persistence (repository seam, ADR-0014).
//!
//! One session per conversation; the whole `NoticeAssistState` aggregate lives in
//! `notice_assist_session.state_json` (mirrors `drafting::one_ask::state`). All
//! access to the table lives here.

use super::frame::NoticeAssistState;
use crate::db::Db;

fn now_iso() -> String {
    chrono::Utc::now().to_rfc3339()
}

/// Upsert the session state for a conversation (ANSI `ON CONFLICT`).
pub async fn save(pool: &Db, conversation_id: &str, state: &NoticeAssistState) -> Result<(), String> {
    let json = serde_json::to_string(state).map_err(|e| e.to_string())?;
    sqlx::query(
        "INSERT INTO notice_assist_session (conversation_id, state_json, updated_at) \
         VALUES ($1, $2, $3) \
         ON CONFLICT(conversation_id) DO UPDATE SET state_json = $2, updated_at = $3",
    )
    .bind(conversation_id)
    .bind(&json)
    .bind(now_iso())
    .execute(pool)
    .await
    .map_err(|e| e.to_string())?;
    Ok(())
}

/// Load a session; `Ok(None)` when the conversation has none.
pub async fn load(pool: &Db, conversation_id: &str) -> Result<Option<NoticeAssistState>, String> {
    let row: Option<(String,)> =
        sqlx::query_as("SELECT state_json FROM notice_assist_session WHERE conversation_id = $1")
            .bind(conversation_id)
            .fetch_optional(pool)
            .await
            .map_err(|e| e.to_string())?;
    match row {
        None => Ok(None),
        Some((json,)) => serde_json::from_str(&json)
            .map(Some)
            .map_err(|e| format!("corrupt notice_assist_session state for {conversation_id}: {e}")),
    }
}

/// End the assist session (officer exited, or the flow reached Done and the next
/// plain turn should be ordinary chat again).
pub async fn delete(pool: &Db, conversation_id: &str) -> Result<(), String> {
    sqlx::query("DELETE FROM notice_assist_session WHERE conversation_id = $1")
        .bind(conversation_id)
        .execute(pool)
        .await
        .map_err(|e| e.to_string())?;
    Ok(())
}
