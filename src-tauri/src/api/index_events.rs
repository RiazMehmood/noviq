//! Indexing progress SSE endpoint (US4, T024) — `GET /api/index/events`.
//!
//! Streams `IndexingProgress` from the process-wide indexer broadcast (one event
//! per file/stage tick), replacing the Tauri `source:indexing-progress` channel
//! for browser clients (T025 subscribes with `EventSource`).
//!
//! Auth: the browser `EventSource` API can't set an `Authorization` header, so
//! the bearer token is passed as `?token=…` and resolved here. This route is
//! therefore mounted OUTSIDE the bearer middleware and does its own check.

use std::convert::Infallible;

use axum::extract::{Query, State};
use axum::response::sse::{Event, KeepAlive, Sse};
use futures::stream::Stream;
use serde::Deserialize;
use tokio::sync::broadcast::error::RecvError;

use super::auth::resolve_token;
use super::{ApiError, AppState};
use crate::services::indexer;

#[derive(Deserialize)]
pub struct EventsQuery {
    pub token: String,
}

pub async fn index_events(
    State(st): State<AppState>,
    Query(q): Query<EventsQuery>,
) -> Result<Sse<impl Stream<Item = Result<Event, Infallible>>>, ApiError> {
    if resolve_token(&st.pool, &q.token).await.is_none() {
        return Err(ApiError::unauthorized("invalid or expired session"));
    }

    let rx = indexer::index_progress_channel().subscribe();
    let stream = futures::stream::unfold(rx, |mut rx| async move {
        loop {
            match rx.recv().await {
                Ok(p) => {
                    let ev = Event::default()
                        .json_data(&p)
                        .unwrap_or_else(|_| Event::default().comment("encode error"));
                    return Some((Ok(ev), rx));
                }
                // Slow consumer fell behind: skip dropped ticks, keep streaming.
                Err(RecvError::Lagged(_)) => continue,
                Err(RecvError::Closed) => return None,
            }
        }
    });
    Ok(Sse::new(stream).keep_alive(KeepAlive::default()))
}
