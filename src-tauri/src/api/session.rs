//! Session auth middleware + extractors (US2, T018; US7 role guard, T044).
//!
//! `require_auth` resolves the `Authorization: Bearer <token>` header to an
//! `AuthUser`, stores it in request extensions, and rejects missing/expired/
//! disabled sessions with `401`. Apply it (via `middleware::from_fn_with_state`)
//! to the authenticated `/api/*` subtree — `/api/health` and `/api/login` are
//! mounted outside it.
//!
//! Handlers then take `AuthUser` as an extractor (read from extensions). The
//! admin-only routes additionally use `AdminUser`, which 403s non-admins.

use axum::extract::{FromRequestParts, Request, State};
use axum::http::request::Parts;
use axum::http::{header::AUTHORIZATION, StatusCode};
use axum::middleware::Next;
use axum::response::Response;

use super::auth::{resolve_token, AuthUser};
use super::{ApiError, AppState};

/// Middleware: require a valid bearer session; inject `AuthUser` into extensions.
pub async fn require_auth(
    State(st): State<AppState>,
    mut req: Request,
    next: Next,
) -> Result<Response, ApiError> {
    let token = req
        .headers()
        .get(AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .map(|s| s.to_string());

    let Some(token) = token else {
        return Err(ApiError::unauthorized("missing bearer token"));
    };
    let Some(user) = resolve_token(&st.pool, &token).await else {
        return Err(ApiError::unauthorized("invalid or expired session"));
    };
    // ── Free-trial surface gate (migration 029) ────────────────────────────
    //
    // ⚠️ ONE place, deliberately. A trial user gets Chat and Search; every other
    // capability belongs to organisations and paid accounts. Enforcing it here
    // rather than in each handler means a NEW route is refused by default for
    // trial users until someone adds it to `TRIAL_ALLOWED_PREFIXES` — the safe
    // direction to fail. Hiding the tabs in the UI is presentation and is never
    // the gate; this is (deep search learned that once already).
    //
    // The prefixes below are matched against the path INSIDE `/api`, since this
    // middleware is layered onto the `/api` subtree.
    if user.role == "trial" {
        let path = req.uri().path().trim_start_matches("/api");
        const TRIAL_ALLOWED_PREFIXES: &[&str] = &[
            "/me",
            "/logout",
            "/trial/",
            // Chat: one turn, its polling, cancellation and transcript.
            "/query",
            "/send_message",
            "/poll_turn",
            "/cancel_turn",
            "/list_conversations",
            "/conversation_turns",
            "/hide_conversation",
            // Search + the document rail that scopes it.
            "/search",
            "/global_search",
            "/list_indexed_folders",
            "/list_sources_in_folder",
            "/list_notebook_sources",
            // Reading a citation the answer produced.
            "/chunk",
            "/source",
            // Preferences the chat UI reads on load.
            "/get_user_prefs",
            "/set_user_prefs",
            "/model_status",
        ];
        let allowed = TRIAL_ALLOWED_PREFIXES.iter().any(|p| path.starts_with(p));
        if !allowed {
            return Err(ApiError::new(
                StatusCode::FORBIDDEN,
                "trial_not_included",
                "That is not part of the free trial. Chat and Search are included;                  everything else is for organisations and paid accounts.",
            ));
        }
    }

    req.extensions_mut().insert(user);
    Ok(next.run(req).await)
}

/// Extract the authenticated user from request extensions (set by `require_auth`).
#[axum::async_trait]
impl<S: Send + Sync> FromRequestParts<S> for AuthUser {
    type Rejection = ApiError;
    async fn from_request_parts(parts: &mut Parts, _state: &S) -> Result<Self, Self::Rejection> {
        parts
            .extensions
            .get::<AuthUser>()
            .cloned()
            .ok_or_else(|| ApiError::unauthorized("not authenticated"))
    }
}

/// Admin-only extractor (US7, FR-020): 403 if the session user isn't an admin.
pub struct AdminUser(pub AuthUser);

#[axum::async_trait]
impl<S: Send + Sync> FromRequestParts<S> for AdminUser {
    type Rejection = ApiError;
    async fn from_request_parts(parts: &mut Parts, _state: &S) -> Result<Self, Self::Rejection> {
        let user = parts
            .extensions
            .get::<AuthUser>()
            .cloned()
            .ok_or_else(|| ApiError::unauthorized("not authenticated"))?;
        if user.role != "admin" {
            return Err(ApiError::new(StatusCode::FORBIDDEN, "forbidden", "admin only"));
        }
        Ok(AdminUser(user))
    }
}
