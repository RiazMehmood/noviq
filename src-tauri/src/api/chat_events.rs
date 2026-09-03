//! Live chat-turn SSE endpoint — `GET /api/chat/events?token=…&conversation_id=…`.
//!
//! Streams the answer tokens (`chat://token`) and pipeline phases
//! (`assistant://progress`) of the subscribed conversation's in-flight turn,
//! replacing the desktop-only Tauri events for browser clients. Backed by the
//! process-wide [`crate::services::chat_stream`] broadcast, which the compat
//! `AppHandle` (constructed per-turn in `api/query.rs`) publishes into.
//!
//! Auth: like `/api/index/events`, the browser `EventSource` API can't set an
//! Authorization header, so the bearer token rides in the query string and the
//! route mounts OUTSIDE the bearer middleware with its own check.

use std::convert::Infallible;

use axum::extract::{Query, State};
use axum::response::sse::{Event, KeepAlive, Sse};
use futures::stream::Stream;
use serde::Deserialize;
use tokio::sync::broadcast::error::RecvError;

use super::auth::resolve_token;
use super::{ApiError, AppState};
use crate::services::chat_stream;

#[derive(Deserialize)]
pub struct ChatEventsQuery {
    pub token: String,
    #[serde(alias = "conversationId")]
    pub conversation_id: String,
}

pub async fn chat_events(
    State(st): State<AppState>,
    Query(q): Query<ChatEventsQuery>,
) -> Result<Sse<impl Stream<Item = Result<Event, Infallible>>>, ApiError> {
    if resolve_token(&st.pool, &q.token).await.is_none() {
        return Err(ApiError::unauthorized("invalid or expired session"));
    }

    let rx = chat_stream::channel().subscribe();
    let conv = q.conversation_id;
    let stream = futures::stream::unfold((rx, conv), |(mut rx, conv)| async move {
        loop {
            match rx.recv().await {
                Ok(ev) if ev.conversation_id == conv => {
                    // SSE event name = the original Tauri event name, so the
                    // client subscribes with addEventListener("chat://token").
                    let out = Event::default()
                        .event(&ev.event)
                        .json_data(&ev.payload)
                        .unwrap_or_else(|_| Event::default().comment("encode error"));
                    return Some((Ok(out), (rx, conv)));
                }
                Ok(_) => continue, // another conversation's turn
                // Slow consumer fell behind: skip dropped ticks, keep streaming.
                Err(RecvError::Lagged(_)) => continue,
                Err(RecvError::Closed) => return None,
            }
        }
    });
    Ok(Sse::new(stream).keep_alive(KeepAlive::default()))
}
