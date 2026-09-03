//! Axum router assembly (T006/T017/T018) — `/api/*` first, then the static
//! SolidJS build with SPA fallback. One binary serves API + UI (FR-003/FR-016).

use axum::extract::DefaultBodyLimit;
use axum::http::{header, HeaderValue, Method};
use axum::routing::{get, post};
use axum::{middleware, Router};
use tower_http::cors::CorsLayer;
use tower_http::services::{ServeDir, ServeFile};

use super::{
    admin, auth, chat_events, documents, feedback, health, history, index_events, projects, query,
    maintenance, quotes, session, system, trial, uploads, AppState,
};

/// Max request body for `/api/*` (T049). Chat/draft turns are small JSON; this
/// is a generous ceiling that still rejects abusive payloads.
const MAX_BODY_BYTES: usize = 8 * 1024 * 1024;

/// Larger ceiling for the document-upload route (multipart file payloads).
const UPLOAD_MAX_BODY_BYTES: usize = 256 * 1024 * 1024;

/// Add baseline security response headers to every `/api/*` response (T049).
async fn security_headers(req: axum::extract::Request, next: middleware::Next) -> axum::response::Response {
    let mut resp = next.run(req).await;
    let h = resp.headers_mut();
    h.insert("X-Content-Type-Options", HeaderValue::from_static("nosniff"));
    h.insert("X-Frame-Options", HeaderValue::from_static("DENY"));
    h.insert("Referrer-Policy", HeaderValue::from_static("no-referrer"));
    resp
}

/// Cache policy for the static frontend.
///
/// Vite content-hashes asset filenames (`index-CT2wiFm1.js`), so an asset URL is
/// immutable by construction and can be cached forever. `index.html` is the
/// opposite: it is the ONLY file whose URL stays constant while its contents
/// change, and it is what points at the current hashed bundle.
///
/// Served with no directive at all (the previous behaviour) a browser applies
/// *heuristic* caching to `index.html` and may reuse a stored copy without
/// revalidating — pinning the user to a superseded bundle indefinitely. That is
/// not theoretical: on 2026-07-27 the admin console kept showing three documents
/// as "pending summaries" for hours after the work had finished, purely because
/// the tab was running an old bundle. Ctrl+Shift+R fixed it; nothing else did.
///
/// `no-cache` does not mean "do not store" — it means "revalidate before use", so
/// the browser still gets a cheap 304 when nothing changed.
async fn static_cache_headers(
    req: axum::extract::Request,
    next: middleware::Next,
) -> axum::response::Response {
    let is_asset = req.uri().path().starts_with("/assets/");
    let mut resp = next.run(req).await;
    let h = resp.headers_mut();
    h.insert(
        "Cache-Control",
        if is_asset {
            // Content-hashed: a change produces a NEW url, so this can never go stale.
            HeaderValue::from_static("public, max-age=31536000, immutable")
        } else {
            HeaderValue::from_static("no-cache")
        },
    );
    resp
}

/// Build the application router.
///
/// - `/api/health`, `/api/login` — unauthenticated.
/// - everything else under `/api/*` — requires a valid bearer session
///   (`require_auth` middleware); the authenticated user is injected for the
///   FR-010 identity hook and US2 enforcement.
/// - non-`/api` paths → static files from `$NOVIQ_WEB_DIR` (default `dist`),
///   SPA fallback to `index.html`.
pub fn build_router(state: AppState) -> Router {
    // Unauthenticated endpoints (the SSE stream self-authenticates via `?token=`
    // since the browser `EventSource` API can't send an Authorization header).
    let public = Router::new()
        .route("/health", get(health::health))
        .route("/login", post(auth::login))
        .route("/index/events", get(index_events::index_events)) // US4/T024 SSE
        .route("/chat/events", get(chat_events::chat_events)) // live turn tokens/phases SSE
        // ⚠️ The ONLY unauthenticated WRITE in this system. It exists because a
        // ministry evaluating Noviq has no account yet and must be able to reach
        // us from the public page. Rate limited per-IP and globally, honeypotted,
        // length-bounded, and its payload is never interpolated anywhere that
        // executes -- see `api/quotes.rs`.
        .route("/quotes", post(quotes::submit))
        // Free trial sign-in (migration 029). Public by necessity — the caller
        // has no account yet; that is what this creates. It accepts nothing but
        // a Google ID token, and refuses unless the token's `aud` is our own
        // client id (see `api/trial.rs`).
        .route("/auth/google", post(trial::google_signin));

    // Authenticated endpoints (US1 feature endpoints mount here as they land).
    let protected = Router::new()
        .route("/logout", post(auth::logout))
        .route("/trial/status", get(trial::status))
        .route("/me", get(auth::me))
        .route("/query", post(query::query)) // US1/T011 — one chat turn
        // Account-bound chat history (migration 020) — the transcript follows the
        // login, not the device. All three are scoped to the session's user.
        .route("/list_conversations", post(history::list_conversations))
        .route("/conversation_turns", post(history::conversation_turns))
        .route("/hide_conversation", post(history::hide_conversation))
        // System surfaces the browser needs (were Tauri-only): active model +
        // chat answer-mode toggle. Command-named so `transport.call` reaches them.
        .route("/model_status", post(system::model_status))
        .route("/get_chat_extractive", post(system::get_chat_extractive))
        .route("/set_chat_extractive", post(system::set_chat_extractive))
        // Per-user preferences (2026-07-29) — distinct from the server-wide
        // settings above; keyed by the authenticated user.
        .route("/get_user_prefs", post(system::get_user_prefs))
        .route("/set_user_prefs", post(system::set_user_prefs))
        // Browser "View latest trace" — mirror of the desktop Tauri command.
        .route("/read_latest_query_trace", post(system::read_latest_query_trace))
        // Browser trace downloads (blob via transport.downloadAuthed).
        .route("/traces/latest", get(system::download_latest_trace))
        .route("/traces/all", get(system::download_all_traces))
        .route("/traces/recent", get(system::download_recent_traces))
        // Track 3: admin query-debug panel — per-query list + full step detail.
        .route("/query_traces", get(system::list_query_traces))
        .route("/query_traces/:id", get(system::get_query_trace))
        // Track 2 (ADR-0019 §2): admin — (re)build the recall summary index.
        .route("/build_summaries", post(system::build_summaries))
        // GraphRAG Phase 3: admin — (re)build the cross-document overlay.
        .route("/link_graph", post(system::link_graph))
        // GraphRAG: admin — regenerate the whole overlay (intra + cross-doc) from
        // existing data, no re-ingest.
        .route("/rebuild_graph", post(system::rebuild_graph))
        // admin — server-side structure re-extraction (re-parse stale PDFs with the
        // current parser; the version-bump startup hook is desktop-only).
        .route("/reindex_structure", post(system::reindex_structure))
        // T010: command-named alias so the shared frontend `transport.call(
        // "send_message", {conversationId,text})` reaches the same handler.
        .route("/send_message", post(query::query))
        // Recover a turn whose HTTP response was killed by the tunnel's ~100 s
        // limit — the detached turn still persists, the browser polls it back.
        .route("/poll_turn", post(query::poll_turn))
        // F026 stop button — cancel the conversation's live chat turn.
        .route("/cancel_turn", post(query::cancel_turn))
        // F022 — leave Notice Assist without running a chat turn (the assist
        // session is server state; the frontend banner only mirrors it).
        .route("/assist_exit", post(query::assist_exit))
        // F022 — read that server state back when opening a past conversation, so
        // the banner reflects the session instead of guessing.
        .route("/assist_status", post(query::assist_status))
        // Feature 018 (US1) — citation feedback for one answer. Command-named
        // alias so the shared transport `call("submit_feedback", {...})` works.
        .route("/feedback", post(feedback::submit))
        .route("/submit_feedback", post(feedback::submit))
        // US1/T013 — document + knowledge-graph read endpoints (command-named).
        .route("/list_indexed_folders", post(documents::list_indexed_folders))
        .route("/list_sources_in_folder", post(documents::list_sources_in_folder))
        .route("/list_notebook_sources", post(documents::list_notebook_sources))
        // ── Feature 027 — My Projects (migration 023) ──────────────────────
        // Every handler below checks the deployment-wide gate as its FIRST act
        // and refuses when it is closed, so registering these routes does not
        // expose the feature: the gate seeds to '0' (FR-038).
        .route("/list_projects", post(projects::list_projects))
        .route("/create_project", post(projects::create_project))
        .route("/rename_project", post(projects::rename_project))
        .route("/delete_project", post(projects::delete_project))
        .route("/get_project", post(projects::get_project))
        .route("/add_project_documents", post(projects::add_project_documents))
        .route("/remove_project_document", post(projects::remove_project_document))
        .route("/list_project_chats", post(projects::list_project_chats))
        .route("/start_project_chat", post(projects::start_project_chat))
        .route("/delete_project_chat", post(projects::delete_project_chat))
        .route("/is_project_chat", post(projects::is_project_chat))
        .route("/share_project", post(projects::share_project))
        // Chat share links. `get_shared_chat`/`get_shared_chunk` take AuthUser
        // like every other route, so a signed-out caller is refused before the
        // token is examined — the link never reaches outside Noviq (FR-027).
        .route("/share_chat", post(projects::share_chat))
        .route("/revoke_chat_share", post(projects::revoke_chat_share))
        .route("/get_shared_chat", post(projects::get_shared_chat))
        .route("/get_shared_chunk", post(projects::get_shared_chunk))
        .route("/fork_shared_chat", post(projects::fork_shared_chat))
        // The gate itself: read by any signed-in user (nav rendering), written
        // by an admin only.
        .route("/get_my_projects_enabled", post(system::get_my_projects_enabled))
        .route("/set_my_projects_enabled", post(system::set_my_projects_enabled))
        // Migration 021 — category NAMES for any signed-in user, so the chat
        // document picker can group by them. The admin route carries counts and
        // stays admin-only.
        .route("/categories", get(documents::list_categories))
        .route("/query_entities", post(documents::query_entities))
        .route("/get_entity_relationships", post(documents::get_entity_relationships))
        .route("/get_entity_sources", post(documents::get_entity_sources))
        .route("/get_knowledge_graph", post(documents::get_knowledge_graph))
        // US4/T024 — indexing triggers; progress streams over /api/index/events.
        .route("/add_sources", post(documents::add_sources))
        .route("/add_indexed_folder", post(documents::add_indexed_folder))
        // Browser users index files from their own device (multipart upload).
        // Per-route body limit overrides the small JSON ceiling above.
        .route(
            "/upload_documents",
            post(uploads::upload_documents).layer(DefaultBodyLimit::max(UPLOAD_MAX_BODY_BYTES)),
        )
        // F021 — user documents: visibility, usage, attachments.
        .route("/set_visibility", post(documents::set_visibility))
        .route("/my_usage", post(documents::my_usage))
        .route("/delete_my_document", post(documents::delete_my_document))
        // Officer-facing source viewer — backs the click-through from a citation.
        .route("/source_chunks", post(documents::source_chunks))
        // Search tab. Scoped through `visible_source_ids` — the desktop command
        // it mirrors searches every indexed chunk, which would leak other users'
        // private uploads if exposed verbatim on a multi-user server.
        .route("/global_search", post(documents::global_search))
        .route("/sources/:id/file", get(documents::source_file))
        .route("/list_conversation_attachments", post(documents::list_conversation_attachments))
        // Tunnel-safe progress poll — the quick tunnel buffers /index/events (SSE).
        .route("/index_progress", post(documents::index_progress))
        .route("/promote_attachment", post(documents::promote_attachment))
        .route("/delete_conversation", post(documents::delete_conversation))
        .route(
            "/attach_document",
            post(uploads::attach_document).layer(DefaultBodyLimit::max(UPLOAD_MAX_BODY_BYTES)),
        )
        .route("/admin/usage", get(admin::usage))
        // Per-user folder management (owner or admin — enforced in the handlers).
        .route("/revoke_indexed_folder", post(documents::revoke_indexed_folder))
        // Read-only: what a folder delete would destroy (drives the confirm dialog).
        .route("/folder_delete_preview", post(documents::folder_delete_preview))
        .route("/list_folder_sources", post(documents::list_folder_sources))
        .route("/retry_folder_source", post(documents::retry_folder_source))
        .route("/deep_index_folder_source", post(documents::deep_index_folder_source))
        .route("/undeep_index_folder_source", post(documents::undeep_index_folder_source))
        .route("/remove_folder_source", post(documents::remove_folder_source))
        .route("/delete_folder_source", post(documents::delete_folder_source))
        // US7 admin (T045) — the `AdminUser` extractor 403s non-admins.
        .route("/admin/users", get(admin::list_users).post(admin::create_user))
        .route("/admin/users/:id/disable", post(admin::disable_user))
        .route("/admin/users/:id/enable", post(admin::enable_user))
        // F014 follow-on — per-user usage: recent activity + downloadable export.
        .route("/admin/users/:id/activity", get(admin::user_activity))
        .route("/admin/users/:id/export", get(admin::export_user))
        // Chunk-level corpus export (all or one document) — ingestion audit tool.
        .route("/admin/chunks/documents", get(admin::list_chunk_documents))
        .route("/admin/chunks/export", get(admin::export_chunks))
        // Paginated chunk browse — backs the indexing console and the
        // click-through from an inline citation.
        .route("/admin/chunks", get(admin::browse_chunks))
        .route("/admin/sources/delete_bulk", post(admin::delete_sources_bulk))
        // Corpus organisation (migration 021) — categories and per-document
        // disable. Label-only: no file moves, no re-index. A disable is enforced
        // in `visible_source_ids`, so it applies to chat, global search and
        // citation opening at once, on the next turn.
        .route("/admin/categories", get(admin::list_categories).post(admin::create_category))
        .route("/admin/categories/:id", post(admin::update_category))
        .route("/admin/categories/:id/delete", post(admin::delete_category))
        .route("/admin/sources/categorize", post(admin::categorize_sources))
        .route("/admin/sources/:id/disabled", post(admin::set_source_disabled))
        // Audit trail (2026-08-13) — browse + date-ranged download. Read-only,
        // and deliberately no delete route: an audit log the admin can prune is
        // worth little under inspection.
        .route("/admin/audit", get(crate::api::audit::browse))
        .route("/admin/audit/export", get(crate::api::audit::export))
        // Landing-page quote requests (migration 028).
        // Trial sign-ups (migration 029) — who has tried the product.
        // Repair oversize chunks in place. ⚠️ Dry run unless dry_run=false.
        .route("/admin/resplit_oversize", post(maintenance::resplit_oversize))
        .route("/admin/trials", get(trial::list_trials))
        .route("/admin/trials/:id/revoke", post(trial::revoke_trial))
        .route("/admin/quotes", get(quotes::list))
        .route("/admin/quotes/:id/status", post(quotes::set_status))
        .route("/admin/share_requests", get(admin::list_share_requests))
        .route("/admin/share_requests/:id/decide", post(admin::decide_share_request))
        .route("/admin/sources/:id/download", get(admin::download_source))
        // Does this document hold one legal instrument, or several? Read-only —
        // proposes boundaries for a human to confirm, never applies them.
        .route("/admin/sources/:id/inspect", get(admin::inspect_source))
        // Carry out a CONFIRMED split. Refuses overlapping or out-of-range
        // ranges; the container's row is replaced by one source per instrument.
        .route("/admin/sources/:id/split", post(admin::split_source))
        // Derived-layers pending status (which docs still need summaries/structure).
        .route("/admin/derived_status", get(admin::derived_status))
        // Migration 022 review surface — documents already stored that look like
        // duplicates of each other. Read-only: it reports, it never deletes.
        .route("/admin/duplicates", get(admin::duplicates))
        // Feature 018 (FR-009) — aggregate distribution of citation feedback.
        .route("/admin/feedback/summary", get(admin::feedback_summary))
        // One-click GPU re-embed of the corpus (drops a trigger sentinel a systemd
        // `.path` unit acts on; orchestration in /opt/noviq/gpu-reindex.sh).
        .route("/admin/gpu_reindex", post(admin::gpu_reindex))
        // Inference engine toggle — local GPU (vLLM) ⇄ hosted API (Groq/…) + API key.
        .route("/admin/inference", get(admin::get_inference).post(admin::set_inference))
        // Provider/model catalogue for the picker, and a live key+model probe.
        .route("/admin/providers", get(admin::get_providers))
        .route("/admin/inference/validate", post(admin::validate_inference))
        .route_layer(middleware::from_fn_with_state(state.clone(), session::require_auth));

    // CORS (T049): same-origin in the bundled deploy (UI + API on one port), but
    // allow the bearer Authorization + JSON content-type headers for any future
    // split-origin tester. Methods are limited to what the API uses.
    let cors = CorsLayer::new()
        .allow_methods([Method::GET, Method::POST, Method::DELETE])
        .allow_headers([header::AUTHORIZATION, header::CONTENT_TYPE])
        .allow_origin(tower_http::cors::AllowOrigin::mirror_request());

    let api = public
        .merge(protected)
        .with_state(state)
        .layer(DefaultBodyLimit::max(MAX_BODY_BYTES))
        .layer(cors)
        .layer(middleware::from_fn(security_headers));

    let web_dir = std::env::var("NOVIQ_WEB_DIR").unwrap_or_else(|_| "dist".to_string());
    let index = format!("{web_dir}/index.html");
    let static_files = ServeDir::new(&web_dir).fallback(ServeFile::new(index));

    Router::new()
        .nest("/api", api)
        .fallback_service(static_files)
        // NOTE: this layer sits on the OUTER router, so it also reaches `/api/*`,
        // which therefore gets `no-cache` (nothing under /api starts with
        // `/assets/`). That is correct and wanted — API responses must never be
        // cached — but it is not "static only", so do not describe it that way.
        .layer(middleware::from_fn(static_cache_headers))
}
