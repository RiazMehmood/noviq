//! F014 (ADR-0017) — headless HTTP API + static web-client serving.
//!
//! Compiled only with `--features server`. The desktop (Tauri) build never
//! pulls this in. The router (`router::build_router`) mounts `/api/*` over the
//! existing service layer and serves the built SolidJS frontend as static files
//! so backend + frontend ship as one binary (no API/UI version skew, FR-016).
//!
//! T003 lays down the module skeleton; handlers are filled per user story:
//!   - health   (T007, done)      GET /api/health
//!   - auth     (US2, T016–T019)  login / logout / me, argon2id + sessions
//!   - session  (US2, T018)       bearer/cookie auth middleware
//!   - static_  (T006, done)      ServeDir + SPA fallback (in router.rs)

pub mod admin;
pub mod audit;
pub mod auth;
pub mod chat_events;
pub mod documents;
pub mod feedback;
pub mod health;
pub mod history;
pub mod index_events;
// Feature 027 — project workspaces, sharing, and chat share links.
pub mod projects;
pub mod query;
// Migration 028 -- public "request a quote" intake + its admin inbox. NOTE the
// submit handler is the ONLY unauthenticated write in this system.
pub mod maintenance;
pub mod quotes;
// Migration 029 -- Google sign-in for the free trial, and its quota endpoint.
pub mod trial;
pub mod router;
pub mod session;
pub mod system;
pub mod uploads;

use std::sync::Arc;

use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;

use crate::services::{
    embedder::EmbedderState, hardware_detector::HardwareTier, llm::ModelSlot,
    reranker::RerankerSlot, scheduler::Scheduler,
};

pub use router::build_router;

/// Shared, cheaply-cloneable handler state (all `Arc`/`Clone`). Carries the chat
/// pipeline dependencies so HTTP handlers call `assistant::run_turn` with the
/// same singletons the desktop uses. Identity/visibility context (FR-010) is
/// added per-request by the auth middleware (US2), not stored here.
#[derive(Clone)]
pub struct AppState {
    pub pool: crate::db::Db,
    pub tier: HardwareTier,
    pub embedder: Arc<EmbedderState>,
    pub model_slot: Arc<ModelSlot>,
    pub reranker_slot: Arc<RerankerSlot>,
    pub scheduler: Arc<Scheduler>,
    /// Generation gate (US4/T023, FR-008): a 1-permit semaphore that serializes
    /// LLM generation across concurrent requests so the single ModelSlot
    /// (Principle III) never runs two inferences at once (no GPU OOM). Handlers
    /// `acquire().await` before calling `run_turn` and hold the permit for the
    /// duration of the turn.
    pub gen_gate: Arc<tokio::sync::Semaphore>,
}

/// Structured error envelope — every handler returns `{error:{code,message}}`,
/// never a silent failure (US1 acceptance, FR-001).
pub struct ApiError {
    pub status: StatusCode,
    pub code: &'static str,
    pub message: String,
}

impl ApiError {
    pub fn new(status: StatusCode, code: &'static str, message: impl Into<String>) -> Self {
        Self { status, code, message: message.into() }
    }
    pub fn unauthorized(message: impl Into<String>) -> Self {
        Self::new(StatusCode::UNAUTHORIZED, "unauthorized", message)
    }
    pub fn bad_request(message: impl Into<String>) -> Self {
        Self::new(StatusCode::BAD_REQUEST, "bad_request", message)
    }
    pub fn internal(message: impl Into<String>) -> Self {
        Self::new(StatusCode::INTERNAL_SERVER_ERROR, "internal", message)
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let body = Json(serde_json::json!({
            "error": { "code": self.code, "message": self.message }
        }));
        (self.status, body).into_response()
    }
}

pub type ApiResult<T> = Result<T, ApiError>;
