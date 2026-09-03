//! Process-wide broadcast of live chat-turn events for the headless web
//! transport. On desktop, generation streams tokens/phases over Tauri events
//! (`chat://token`, `assistant://progress`); the headless build has no event
//! bus, so the compat [`crate::compat::AppHandle`] forwards those same emits
//! into this channel and `GET /api/chat/events` (SSE) fans them out to the
//! browser, filtered per conversation. Mirrors the indexer's
//! `index_progress_channel` pattern.

use std::sync::OnceLock;

use tokio::sync::broadcast;

/// One live event from an in-flight chat turn.
#[derive(Debug, Clone, serde::Serialize)]
pub struct ChatStreamEvent {
    /// The conversation the turn belongs to (SSE subscribers filter on this).
    pub conversation_id: String,
    /// Original event name (`chat://token`, `assistant://progress`, …).
    pub event: String,
    /// The event's JSON payload, unchanged.
    pub payload: serde_json::Value,
}

/// The process-wide sender. Subscribing when no turn is running is fine; events
/// from turns nobody watches are dropped (broadcast with no receivers).
pub fn channel() -> &'static broadcast::Sender<ChatStreamEvent> {
    static CH: OnceLock<broadcast::Sender<ChatStreamEvent>> = OnceLock::new();
    CH.get_or_init(|| broadcast::channel(512).0)
}
