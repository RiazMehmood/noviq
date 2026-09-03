//! `POST /api/feedback` (Feature 018, US1) — citation feedback for one answer.
//!
//! Two single-select questions per answer: citation accuracy and completeness.
//! Anchored to the assistant turn (`chat_message.id`) the client already holds as
//! `TurnResponse.message_id`. Either question may be submitted alone; a re-tap
//! supersedes the prior row. This is a separate request from the chat turn, so a
//! feedback failure never affects an answer.

use axum::extract::State;
use axum::Json;
use serde::Deserialize;

use super::auth::AuthUser;
use super::{ApiError, ApiResult, AppState};
use crate::db::repositories::answer_feedback;

/// Allowed codes — keep in lockstep with the migration `008` CHECK constraints
/// and the frontend labels (spec Clarifications 2026-07-03).
const ACCURACY: &[&str] = &["all", "some", "wrong", "unsure"];
const COMPLETENESS: &[&str] = &["yes", "missing", "na"];

#[derive(Deserialize)]
pub struct FeedbackReq {
    // Accept the Tauri invoke arg names (camelCase) too, so the shared frontend
    // `transport.call("submit_feedback", {...})` works unchanged over web.
    #[serde(alias = "messageId")]
    pub message_id: String,
    #[serde(alias = "conversationId")]
    pub conversation_id: Option<String>,
    #[serde(alias = "citationAccuracy")]
    pub citation_accuracy: Option<String>,
    #[serde(alias = "citationCompleteness")]
    pub citation_completeness: Option<String>,
}

fn validate(value: &Option<String>, allowed: &[&str], field: &str) -> Result<(), ApiError> {
    if let Some(v) = value {
        if !allowed.contains(&v.as_str()) {
            return Err(ApiError::bad_request(format!(
                "invalid {field}: '{v}' (expected one of {allowed:?})"
            )));
        }
    }
    Ok(())
}

pub async fn submit(
    State(st): State<AppState>,
    user: AuthUser, // valid session; user_id is taken from here, never the body
    Json(req): Json<FeedbackReq>,
) -> ApiResult<Json<serde_json::Value>> {
    if req.message_id.trim().is_empty() {
        return Err(ApiError::bad_request("message_id is required"));
    }
    if req.citation_accuracy.is_none() && req.citation_completeness.is_none() {
        return Err(ApiError::bad_request(
            "provide at least one of citation_accuracy / citation_completeness",
        ));
    }
    validate(&req.citation_accuracy, ACCURACY, "citation_accuracy")?;
    validate(&req.citation_completeness, COMPLETENESS, "citation_completeness")?;

    answer_feedback::upsert(
        &st.pool,
        &req.message_id,
        req.conversation_id.as_deref(),
        Some(&user.id),
        req.citation_accuracy.as_deref(),
        req.citation_completeness.as_deref(),
    )
    .await
    .map_err(|e| ApiError::internal(e.to_string()))?;

    Ok(Json(serde_json::json!({ "ok": true })))
}
