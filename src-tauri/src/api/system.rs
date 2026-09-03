//! System endpoints over HTTP (F014 follow-on).
//!
//! Mirror the desktop Tauri commands `model_status`, `get_chat_extractive`, and
//! `set_chat_extractive` so the **browser** thin client can read the active
//! model and switch the chat answer mode. These surfaces were Tauri-only, so on
//! the web build the chat showed "model not loaded" and the answer-mode toggle
//! did nothing. Command-named routes (`POST /api/<cmd>`) so the shared frontend
//! `transport.call(...)` reaches them unchanged.

use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::Json;
use serde::{Deserialize, Serialize};

use super::auth::AuthUser;
use super::{ApiError, ApiResult, AppState};

/// Admin-only gate for the query-debug endpoints (Track 3). Traces can contain
/// any user's question + retrieved passages, so browsing them is operator-only.
fn require_admin(user: &AuthUser) -> Result<(), ApiError> {
    if user.role == "admin" {
        Ok(())
    } else {
        Err(ApiError::new(StatusCode::FORBIDDEN, "forbidden", "Admin only."))
    }
}

/// `model_status` payload — mirrors `commands::assistant::ModelStatusOut`.
#[derive(Serialize)]
pub struct ModelStatusOut {
    pub model_file: String,
    pub n_ctx: u32,
    pub trained_ctx: u32,
    pub ram_clamped: bool,
    pub source: String,
}

/// POST /api/model_status — the active model + the window derived for it, so the
/// browser chat header shows the real model instead of "model not loaded".
pub async fn model_status(
    State(st): State<AppState>,
    _user: AuthUser, // any authenticated user may read it
) -> ApiResult<Json<ModelStatusOut>> {
    use crate::services::{assistant::n_ctx, query_router};

    let (rel_path, _label, _notice) = query_router::select_model_for_tier(&st.tier);
    let model_path = query_router::resolve_model_path(&rel_path);
    let model_file = model_path
        .split(['/', '\\'])
        .next_back()
        .unwrap_or(&model_path)
        .to_string();

    let (trained_ctx, resolved) = st
        .model_slot
        .window_info(&model_path, &st.tier)
        .map_err(|e| ApiError::internal(e.to_string()))?;
    let overridden = n_ctx::env_override().is_some();
    Ok(Json(ModelStatusOut {
        model_file,
        n_ctx: resolved,
        trained_ctx,
        ram_clamped: !overridden && resolved < trained_ctx,
        source: if overridden { "ECHO_N_CTX" } else { "auto" }.to_string(),
    }))
}

/// POST /api/get_chat_extractive — current chat answer mode (`true` = extractive
/// bypass; `false` = LLM, the default).
pub async fn get_chat_extractive(
    State(st): State<AppState>,
    _user: AuthUser,
) -> ApiResult<Json<bool>> {
    let raw: Option<String> =
        sqlx::query_scalar("SELECT value FROM db_config WHERE key = 'chat_extractive'")
            .fetch_optional(&st.pool)
            .await
            .map_err(|e| ApiError::internal(e.to_string()))?;
    Ok(Json(raw.map(|v| v == "1").unwrap_or(false)))
}

/// POST /api/get_my_projects_enabled — F027 availability gate (FR-037).
///
/// Readable by any signed-in user so the client can decide whether to render the
/// nav entry. ⚠️ That is presentation only: every project route refuses on its
/// own when the gate is closed, so a client that ignores this still gets nothing
/// (FR-038).
pub async fn get_my_projects_enabled(
    State(st): State<AppState>,
    _user: AuthUser,
) -> ApiResult<Json<bool>> {
    crate::services::projects::my_projects_enabled(&st.pool)
        .await
        .map(Json)
        .map_err(|e| ApiError::internal(e.to_string()))
}

#[derive(Deserialize)]
pub struct SetMyProjectsEnabledReq {
    pub enabled: bool,
}

/// POST /api/set_my_projects_enabled — admin only. Applies at runtime, no
/// restart, which is what FR-039 (off, then on again, everything intact)
/// depends on. Nothing is deleted either way.
pub async fn set_my_projects_enabled(
    State(st): State<AppState>,
    user: AuthUser,
    Json(req): Json<SetMyProjectsEnabledReq>,
) -> ApiResult<Json<bool>> {
    if !user.role.eq_ignore_ascii_case("admin") {
        return Err(ApiError::new(
            axum::http::StatusCode::FORBIDDEN,
            "forbidden",
            "admin only",
        ));
    }
    crate::services::projects::set_my_projects_enabled(&st.pool, req.enabled)
        .await
        .map_err(|e| ApiError::internal(e.to_string()))?;
    Ok(Json(req.enabled))
}

#[derive(Deserialize)]
pub struct SetExtractiveReq {
    pub extractive: bool,
}

/// POST /api/set_chat_extractive — switch the server-wide chat answer mode.
/// Returns the new value so the shared `transport.call` has a JSON body to read.
pub async fn set_chat_extractive(
    State(st): State<AppState>,
    _user: AuthUser,
    Json(req): Json<SetExtractiveReq>,
) -> ApiResult<Json<bool>> {
    let value = if req.extractive { "1" } else { "0" };
    sqlx::query(
        "INSERT INTO db_config (key, value) VALUES ($1, $2) \
         ON CONFLICT(key) DO UPDATE SET value = excluded.value",
    )
    .bind("chat_extractive")
    .bind(value)
    .execute(&st.pool)
    .await
    .map_err(|e| ApiError::internal(e.to_string()))?;
    Ok(Json(req.extractive))
}

// ── Per-user preferences (2026-07-29) ────────────────────────────────────────
//
// Note the contrast with `chat_extractive` directly above: that is a SERVER-WIDE
// operator setting in `db_config`, so one user flipping it changes everyone's
// answers. These are per-user and keyed by `AuthUser.id`, which is why they use
// `user_preference` rather than being bolted onto the same table.

/// POST /api/get_user_prefs — the calling user's preferences, defaults applied.
pub async fn get_user_prefs(
    State(st): State<AppState>,
    user: AuthUser,
) -> ApiResult<Json<UserPrefs>> {
    use crate::db::repositories::user_preference as prefs;
    let refinement_offer =
        prefs::get_bool(&st.pool, &user.id, prefs::KEY_REFINEMENT_OFFER, true)
            .await
            .map_err(|e| ApiError::internal(e.to_string()))?;
    Ok(Json(UserPrefs { refinement_offer }))
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UserPrefs {
    /// Whether answers end with a "Next steps" follow-up offer. Default true.
    pub refinement_offer: bool,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SetUserPrefsReq {
    /// Only the fields present are changed, so the client can send one toggle
    /// without having to echo back every other preference it did not touch.
    #[serde(default)]
    pub refinement_offer: Option<bool>,
}

/// POST /api/set_user_prefs — update the calling user's preferences. Returns the
/// full resulting set so the client can render from the response.
pub async fn set_user_prefs(
    State(st): State<AppState>,
    user: AuthUser,
    Json(req): Json<SetUserPrefsReq>,
) -> ApiResult<Json<UserPrefs>> {
    use crate::db::repositories::user_preference as prefs;
    if let Some(v) = req.refinement_offer {
        prefs::set_bool(&st.pool, &user.id, prefs::KEY_REFINEMENT_OFFER, v)
            .await
            .map_err(|e| ApiError::internal(e.to_string()))?;
    }
    get_user_prefs(State(st), user).await
}

/// POST /api/read_latest_query_trace — mirror of the desktop Tauri command so the
/// **browser** "View latest trace" button works on the headless server. The
/// desktop command (`commands::system::read_latest_query_trace`) is Tauri-only
/// (takes an `AppHandle`); without this route the web build hit `POST
/// /api/read_latest_query_trace` → 404 → "read failed". Reads the newest
/// `<ECHO_LOG_DIR>/queries/*.log`; returns `[filename, body]` (empty pair when no
/// trace exists yet), matching the frontend's `[string, string]` expectation.
pub async fn read_latest_query_trace(
    _user: AuthUser, // any authenticated user may read their trace logs
) -> ApiResult<Json<(String, String)>> {
    // Headless: resolve_log_dir returns early on ECHO_LOG_DIR — no AppHandle needed.
    let dir = crate::services::query_trace::QueryTrace::resolve_log_dir(&None).join("queries");
    let entries = match std::fs::read_dir(&dir) {
        Ok(it) => it,
        Err(_) => return Ok(Json((String::new(), String::new()))),
    };
    let mut newest: Option<(std::time::SystemTime, std::path::PathBuf)> = None;
    for e in entries.flatten() {
        let p = e.path();
        if p.extension().and_then(|s| s.to_str()) != Some("log") {
            continue;
        }
        if let Ok(meta) = e.metadata() {
            if let Ok(modified) = meta.modified() {
                if newest.as_ref().map_or(true, |(t, _)| modified > *t) {
                    newest = Some((modified, p));
                }
            }
        }
    }
    match newest {
        Some((_, path)) => {
            let name = path
                .file_name()
                .and_then(|s| s.to_str())
                .unwrap_or("trace.log")
                .to_string();
            let body = std::fs::read_to_string(&path)
                .map_err(|e| ApiError::internal(e.to_string()))?;
            Ok(Json((name, body)))
        }
        None => Ok(Json((String::new(), String::new()))),
    }
}

// ── Query-trace DOWNLOADS over HTTP (browser) ─────────────────────────────────
// The desktop download commands (`download_query_trace_to`,
// `download_all_query_traces_to`, `download_recent_query_traces_to`) write to a
// filesystem `dest` chosen via a Tauri save dialog — meaningless on the web,
// where the dialog call throws → "Download failed". These GET endpoints return
// the trace body so the browser fetches it (with the bearer token) and triggers
// a blob download via `transport.downloadAuthed`. Concatenation format mirrors
// the desktop commands so the two surfaces produce identical files.

/// Collect `<ECHO_LOG_DIR>/queries/*.log` sorted by mtime (oldest first).
fn collect_sorted_traces() -> Result<(std::path::PathBuf, Vec<std::path::PathBuf>), ApiError> {
    let dir = crate::services::query_trace::QueryTrace::resolve_log_dir(&None).join("queries");
    let entries = std::fs::read_dir(&dir).map_err(|_| {
        ApiError::new(StatusCode::NOT_FOUND, "no_traces", "No traces yet — run a query first.")
    })?;
    let mut logs: Vec<(std::time::SystemTime, std::path::PathBuf)> = Vec::new();
    for e in entries.flatten() {
        let p = e.path();
        if p.extension().and_then(|s| s.to_str()) != Some("log") {
            continue;
        }
        if let Ok(meta) = e.metadata() {
            if let Ok(modified) = meta.modified() {
                logs.push((modified, p));
            }
        }
    }
    if logs.is_empty() {
        return Err(ApiError::new(
            StatusCode::NOT_FOUND,
            "no_traces",
            "No traces in queries folder yet — run at least one query.",
        ));
    }
    logs.sort_by_key(|(t, _)| *t);
    Ok((dir, logs.into_iter().map(|(_, p)| p).collect()))
}

/// Concatenate the given trace files into one human-scrollable export (matches
/// the desktop `download_all/recent_query_traces_to` banner format).
fn concat_traces(dir: &std::path::Path, logs: &[std::path::PathBuf], header: &str) -> String {
    let mut combined = String::with_capacity(64 * 1024);
    combined.push_str(&format!(
        "{header}\nGenerated: {}\nTraces in this file: {}\nSource folder: {}\n\n",
        chrono::Utc::now().format("%Y-%m-%d %H:%M:%S UTC"),
        logs.len(),
        dir.display()
    ));
    for (i, path) in logs.iter().enumerate() {
        let name = path.file_name().and_then(|s| s.to_str()).unwrap_or("?.log");
        combined.push_str(&"#".repeat(80));
        combined.push_str(&format!("\n# TRACE {}/{} — {}\n", i + 1, logs.len(), name));
        combined.push_str(&"#".repeat(80));
        combined.push('\n');
        match std::fs::read_to_string(path) {
            Ok(body) => combined.push_str(&body),
            Err(e) => combined.push_str(&format!("[failed to read {name}: {e}]\n")),
        }
        combined.push_str("\n\n");
    }
    combined
}

/// GET /api/traces/latest — the most recent trace `.log` as a downloadable body.
pub async fn download_latest_trace(_user: AuthUser) -> ApiResult<String> {
    let (_dir, logs) = collect_sorted_traces()?;
    let path = logs.last().expect("collect_sorted_traces guarantees non-empty");
    std::fs::read_to_string(path).map_err(|e| ApiError::internal(e.to_string()))
}

/// GET /api/traces/all — every trace concatenated into one export.
pub async fn download_all_traces(_user: AuthUser) -> ApiResult<String> {
    let (dir, logs) = collect_sorted_traces()?;
    Ok(concat_traces(&dir, &logs, "EchoAI — Combined Query Trace Export"))
}

#[derive(Deserialize)]
pub struct RecentQuery {
    pub limit: Option<usize>,
}

/// GET /api/traces/recent?limit=N — the most recent N traces concatenated
/// (newest-last, so the file reads in run order).
pub async fn download_recent_traces(
    _user: AuthUser,
    Query(q): Query<RecentQuery>,
) -> ApiResult<String> {
    let (dir, logs) = collect_sorted_traces()?;
    let take = q.limit.unwrap_or(30).clamp(1, logs.len());
    let recent: Vec<std::path::PathBuf> = logs[logs.len() - take..].to_vec();
    Ok(concat_traces(
        &dir,
        &recent,
        &format!("EchoAI — Recent Query Trace Export (last {take})"),
    ))
}

// ── Admin query-debug panel (Track 3) ─────────────────────────────────────────
// A per-query LIST (from the `queries.jsonl` sidecar) + a per-query DETAIL (the
// full step-by-step `.log`). Powers the Admin → Query Debug panel and the chat
// "Debug this answer" link. Admin-only (traces expose questions + retrieved law).

/// One row in the query-debug list — the jsonl summary minus the heavy `steps`.
#[derive(Serialize)]
pub struct TraceSummary {
    pub id: String,
    pub started_at: String,
    pub query: String,
    pub capability: String,
    pub total_ms: u64,
    pub outcome: String,
    pub tier_used: String,
    pub citations: u64,
    pub answer_chars: u64,
}

/// GET /api/query_traces?limit=N — recent query traces, newest first (admin only).
/// Reads the `queries.jsonl` sidecar tail; empty list when no trace exists yet.
pub async fn list_query_traces(
    user: AuthUser,
    Query(q): Query<RecentQuery>,
) -> ApiResult<Json<Vec<TraceSummary>>> {
    require_admin(&user)?;
    let limit = q.limit.unwrap_or(50).clamp(1, 500);
    let jsonl = crate::services::query_trace::QueryTrace::resolve_log_dir(&None).join("queries.jsonl");
    let body = match std::fs::read_to_string(&jsonl) {
        Ok(b) => b,
        Err(_) => return Ok(Json(Vec::new())),
    };
    let mut out: Vec<TraceSummary> = Vec::new();
    // jsonl is append-order (oldest first); walk from the end for newest-first.
    for line in body.lines().rev() {
        if out.len() >= limit {
            break;
        }
        let v: serde_json::Value = match serde_json::from_str(line) {
            Ok(v) => v,
            Err(_) => continue,
        };
        out.push(TraceSummary {
            id: v["id"].as_str().unwrap_or("").to_string(),
            started_at: v["started_at"].as_str().unwrap_or("").to_string(),
            query: v["query"].as_str().unwrap_or("").to_string(),
            capability: v["capability"].as_str().unwrap_or("").to_string(),
            total_ms: v["total_ms"].as_u64().unwrap_or(0),
            outcome: v["outcome"].as_str().unwrap_or("").to_string(),
            tier_used: v["tier_used"].as_str().unwrap_or("").to_string(),
            citations: v["citations"].as_u64().unwrap_or(0),
            answer_chars: v["answer_chars"].as_u64().unwrap_or(0),
        });
    }
    Ok(Json(out))
}

/// GET /api/query_traces/:id — the full step-by-step `.log` for one trace (admin
/// only). The on-disk name is `<ts>_<id>.log`; the id is our 8-hex uuid prefix.
pub async fn get_query_trace(
    user: AuthUser,
    Path(id): Path<String>,
) -> ApiResult<Json<String>> {
    require_admin(&user)?;
    // Guard against path traversal — the id is alphanumeric by construction.
    if id.is_empty() || !id.chars().all(|c| c.is_ascii_alphanumeric()) {
        return Err(ApiError::new(StatusCode::BAD_REQUEST, "bad_id", "Invalid trace id."));
    }
    let dir = crate::services::query_trace::QueryTrace::resolve_log_dir(&None).join("queries");
    let entries = std::fs::read_dir(&dir).map_err(|_| {
        ApiError::new(StatusCode::NOT_FOUND, "no_traces", "No traces yet — run a query first.")
    })?;
    let suffix = format!("_{id}.log");
    for e in entries.flatten() {
        let p = e.path();
        let is_match = p
            .file_name()
            .and_then(|s| s.to_str())
            .map_or(false, |n| n.ends_with(&suffix));
        if is_match {
            let body = std::fs::read_to_string(&p).map_err(|e| ApiError::internal(e.to_string()))?;
            return Ok(Json(body));
        }
    }
    Err(ApiError::new(
        StatusCode::NOT_FOUND,
        "trace_not_found",
        "That query trace was not found (it may have rotated out).",
    ))
}

// ── Track 2 (ADR-0019 §2): build the recall summary index ─────────────────────

/// Guards against two heavy builds overlapping (interleaved delete/insert on the
/// same source). One build at a time process-wide.
static SUMMARY_BUILD_RUNNING: std::sync::atomic::AtomicBool =
    std::sync::atomic::AtomicBool::new(false);

/// Guard so only one structure re-extraction runs process-wide.
static REINDEX_RUNNING: std::sync::atomic::AtomicBool =
    std::sync::atomic::AtomicBool::new(false);

#[derive(Deserialize)]
pub struct BuildSummariesReq {
    /// Optional — build for ONE source. Absent = every graph-covered source.
    pub source_id: Option<String>,
}

/// POST /api/build_summaries — admin: (re)build the Track 2 per-provision recall
/// summaries. HEAVY (one model call per provision → vLLM in prod), so it runs in
/// a background task and returns immediately; progress is in the server log.
/// Idempotent per source. Admin-only.
pub async fn build_summaries(
    State(st): State<AppState>,
    user: AuthUser,
    Json(req): Json<BuildSummariesReq>,
) -> ApiResult<Json<serde_json::Value>> {
    require_admin(&user)?;
    use std::sync::atomic::Ordering;
    if SUMMARY_BUILD_RUNNING
        .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
        .is_err()
    {
        return Err(ApiError::new(
            StatusCode::CONFLICT,
            "already_running",
            "A summary build is already in progress — wait for it to finish.",
        ));
    }

    let pool = st.pool.clone();
    let tier = st.tier.clone();
    let embedder = st.embedder.clone();
    let model_slot = st.model_slot.clone();
    let one = req.source_id.clone();
    let scope_label = one.clone().unwrap_or_else(|| "all".to_string());
    tokio::spawn(async move {
        use crate::services::summary_builder as sb;
        let res = match &one {
            Some(sid) => sb::build_summaries_for_source(&pool, &model_slot, &tier, &embedder, sid)
                .await
                .map(|s| vec![s]),
            None => sb::build_summaries_all(&pool, &model_slot, &tier, &embedder).await,
        };
        match res {
            Ok(stats) => {
                let total: usize =
                    stats.iter().map(|s| s.provisions + s.doc_level + s.doc_full).sum();
                let doc_full: usize = stats.iter().map(|s| s.doc_full).sum();
                log::info!(
                    "build_summaries done: {} source(s), {} summaries ({} whole-doc)",
                    stats.len(), total, doc_full
                );
            }
            Err(e) => log::error!("build_summaries failed: {e}"),
        }
        SUMMARY_BUILD_RUNNING.store(false, Ordering::SeqCst);
    });

    Ok(Json(serde_json::json!({
        "status": "started",
        "scope": scope_label,
    })))
}

/// GraphRAG Phase 3 — (re)build the corpus cross-document `cross_reference`
/// overlay. Deterministic + fast (no model), so it runs inline and returns the
/// stats. Idempotent. Admin only; call after a corpus re-index.
pub async fn link_graph(
    State(st): State<AppState>,
    user: AuthUser,
) -> ApiResult<Json<serde_json::Value>> {
    require_admin(&user)?;
    let stats = crate::services::graph_linker::link_corpus(&st.pool)
        .await
        .map_err(|e| {
            ApiError::new(
                StatusCode::INTERNAL_SERVER_ERROR,
                "link_failed",
                &format!("cross-document link failed: {e}"),
            )
        })?;
    log::info!(
        "link_graph done: {} cross-doc edges over {} targetable instruments ({} sources)",
        stats.edges_inserted, stats.instruments_targetable, stats.sources_scanned
    );
    Ok(Json(serde_json::json!({
        "status": "done",
        "edges_inserted": stats.edges_inserted,
        "instruments_targetable": stats.instruments_targetable,
        "sources_scanned": stats.sources_scanned,
    })))
}

/// GraphRAG — regenerate the ENTIRE legal overlay from existing data without a
/// re-ingest: first the within-document overlay (`rebuild_overlay` — applies the
/// generalised provision-kind extraction to already-indexed content), then the
/// cross-document overlay (`link_corpus`). Deterministic + fast (no model, no
/// re-OCR/re-embed), idempotent, runs inline. Admin only. The one-shot button for
/// bringing a corpus indexed before the GraphRAG work up to the full overlay.
pub async fn rebuild_graph(
    State(st): State<AppState>,
    user: AuthUser,
) -> ApiResult<Json<serde_json::Value>> {
    require_admin(&user)?;
    use crate::services::graph_linker as gl;
    let intra = gl::rebuild_overlay(&st.pool).await.map_err(|e| {
        ApiError::new(
            StatusCode::INTERNAL_SERVER_ERROR,
            "rebuild_failed",
            &format!("intra-document overlay rebuild failed: {e}"),
        )
    })?;
    let cross = gl::link_corpus(&st.pool).await.map_err(|e| {
        ApiError::new(
            StatusCode::INTERNAL_SERVER_ERROR,
            "link_failed",
            &format!("cross-document link failed: {e}"),
        )
    })?;
    log::info!(
        "rebuild_graph done: {} intra-doc overlay + {} cross-doc edges ({} sources)",
        intra.edges_inserted, cross.edges_inserted, intra.sources
    );
    Ok(Json(serde_json::json!({
        "status": "done",
        "intra_doc_edges": intra.edges_inserted,
        "cross_doc_edges": cross.edges_inserted,
        "instruments_targetable": cross.instruments_targetable,
        "sources": intra.sources,
    })))
}

/// Server-side structure re-extraction. Re-runs the indexing pipeline for every
/// indexed PDF still below the current `STRUCTURED_IMPORTER_VERSION` — reads the
/// stored file, re-parses with the CURRENT parser, re-chunks, re-embeds. Needed
/// because the version-bump startup re-extract is desktop-only; the headless
/// server has no such hook. Re-embedding is slow (minutes), so it runs as a
/// background task and returns immediately. Admin only. Follow with
/// `/build_summaries` then `/rebuild_graph` to regenerate the derived layers.
pub async fn reindex_structure(
    State(st): State<AppState>,
    user: AuthUser,
) -> ApiResult<Json<serde_json::Value>> {
    require_admin(&user)?;
    use std::sync::atomic::Ordering;
    if REINDEX_RUNNING
        .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
        .is_err()
    {
        return Err(ApiError::new(
            StatusCode::CONFLICT,
            "already_running",
            "A structure re-index is already in progress — wait for it to finish.",
        ));
    }
    let pool = st.pool.clone();
    let tier = st.tier.clone();
    let embedder = st.embedder.clone();
    tokio::spawn(async move {
        match crate::services::indexer::reindex_structure_all(&pool, embedder, &tier).await {
            Ok((attempted, ok, failed)) => log::info!(
                "reindex_structure done: {attempted} attempted, {ok} ok, {failed} failed"
            ),
            Err(e) => log::error!("reindex_structure failed: {e}"),
        }
        REINDEX_RUNNING.store(false, Ordering::SeqCst);
    });
    Ok(Json(serde_json::json!({ "status": "started" })))
}
