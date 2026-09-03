//! Admin endpoints (US7, T045, FR-021) — beta-tester management.
//!
//! All routes are guarded by the `AdminUser` extractor (403 for non-admins,
//! `api/session.rs`). Mounted under the authenticated `/api/admin/*` subtree.
//!
//!   GET  /api/admin/users                     list users (no password hashes)
//!   POST /api/admin/users                     create a user with a temp password
//!   POST /api/admin/users/:id/disable         disable + revoke sessions
//!   POST /api/admin/users/:id/enable          re-enable
//!
//! Create returns a one-time temporary password the admin hands to the tester
//! (never stored in plaintext). Disable sets `disabled_at` and drops the user's
//! sessions so access is cut immediately (FR-021); login then refuses them (the
//! login/resolve paths already check `disabled_at`, T046).

use std::collections::HashMap;
use std::time::{SystemTime, UNIX_EPOCH};

use axum::extract::{Path, Query, State};
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::auth::hash_password;
use super::session::AdminUser;
use super::{ApiError, ApiResult, AppState};
use crate::db::repositories::query_log;

fn now_secs() -> i64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs() as i64).unwrap_or(0)
}

/// POST /api/admin/gpu_reindex — kick off a one-shot GPU re-embed of the corpus.
///
/// This handler only *requests* the job: it drops a trigger sentinel under the
/// data dir. A systemd `.path` unit (`noviq-gpu-reindex.path`) watches for that
/// file and launches `/opt/noviq/gpu-reindex.sh`, which stops vLLM, flips the
/// embedder to the CUDA ONNX Runtime, re-indexes every admin source on the GPU,
/// then restores the CPU runtime + vLLM. The orchestration runs out-of-process
/// precisely because it restarts *this* server — so it can't run inline here.
///
/// Refuses (200 `ok:false`) if a deploy or reindex is already mid-flight (the
/// shared `DEPLOYING` banner is up), so a double-click can't stack two runs.
pub async fn gpu_reindex(_admin: AdminUser) -> ApiResult<Json<serde_json::Value>> {
    let data_dir =
        std::env::var("NOVIQ_DATA_DIR").unwrap_or_else(|_| ".noviq-data".to_string());
    let dir = std::path::PathBuf::from(&data_dir);
    if dir.join("DEPLOYING").exists() {
        return Ok(Json(serde_json::json!({
            "ok": false,
            "message": "A deploy or GPU re-embed is already in progress — try again once it finishes."
        })));
    }
    let trigger = dir.join("GPU_REINDEX_REQUESTED");
    std::fs::write(&trigger, format!("requested_at={}\n", now_secs()))
        .map_err(|e| ApiError::internal(format!("failed to write reindex trigger: {e}")))?;
    Ok(Json(serde_json::json!({
        "ok": true,
        "message": "GPU re-embed started. Chat pauses ~7–10 min while embeddings run on the GPU; the app shows an \"updating\" banner and comes back automatically."
    })))
}

#[derive(Serialize, sqlx::FromRow)]
pub struct UserRow {
    pub id: String,
    pub username: String,
    pub role: String,
    pub created_at: i64,
    pub disabled_at: Option<i64>,
    // ── Usage rollup (F014 follow-on) — populated from query_log ──────────────
    #[sqlx(default)]
    pub query_count: i64,
    #[sqlx(default)]
    pub last_active: Option<i64>,
    #[sqlx(default)]
    pub avg_cpu_pct: Option<f64>,
    #[sqlx(default)]
    pub peak_ram_mb: Option<f64>,
    #[sqlx(default)]
    pub avg_gpu_util_pct: Option<f64>,
    #[sqlx(default)]
    pub avg_latency_ms: Option<f64>,
}

/// GET /api/admin/users — list all accounts (newest first), no secrets, with a
/// per-user usage rollup (query count, last active, avg CPU/GPU, peak RAM).
pub async fn list_users(
    State(st): State<AppState>,
    _admin: AdminUser,
) -> ApiResult<Json<Vec<UserRow>>> {
    let mut rows: Vec<UserRow> = sqlx::query_as(
        "SELECT id, username, role, created_at, disabled_at, \
                0::int8 AS query_count, NULL::int8 AS last_active, \
                NULL::float8 AS avg_cpu_pct, NULL::float8 AS peak_ram_mb, \
                NULL::float8 AS avg_gpu_util_pct, NULL::float8 AS avg_latency_ms \
         FROM users ORDER BY created_at DESC",
    )
    .fetch_all(&st.pool)
    .await
    .map_err(|e| ApiError::internal(e.to_string()))?;

    // Merge in usage stats (best-effort: an empty/absent log just leaves zeros).
    if let Ok(usage) = query_log::usage_by_user(&st.pool).await {
        let by_id: HashMap<String, _> = usage.into_iter().map(|u| (u.user_id.clone(), u)).collect();
        for r in &mut rows {
            if let Some(u) = by_id.get(&r.id) {
                r.query_count = u.query_count;
                r.last_active = u.last_active;
                r.avg_cpu_pct = u.avg_cpu_pct;
                r.peak_ram_mb = u.peak_ram_mb;
                r.avg_gpu_util_pct = u.avg_gpu_util_pct;
                r.avg_latency_ms = u.avg_latency_ms;
            }
        }
    }
    Ok(Json(rows))
}

/// GET /api/admin/users/:id/activity — recent queries for one user (newest
/// first, capped) with their per-turn resource peaks. Drives the activity list.
pub async fn user_activity(
    State(st): State<AppState>,
    _admin: AdminUser,
    Path(id): Path<String>,
) -> ApiResult<Json<Vec<query_log::QueryLogRow>>> {
    let rows = query_log::list_for_user(&st.pool, &id, 200)
        .await
        .map_err(|e| ApiError::internal(e.to_string()))?;
    Ok(Json(rows))
}

#[derive(Deserialize)]
pub struct ExportQuery {
    /// `json` (default) or `csv`.
    pub format: Option<String>,
}

/// GET /api/admin/users/:id/export?format=json|csv — full query history for one
/// user as a downloadable file (all turns, with question/answer + metrics).
pub async fn export_user(
    State(st): State<AppState>,
    _admin: AdminUser,
    Path(id): Path<String>,
    Query(q): Query<ExportQuery>,
) -> ApiResult<Response> {
    let rows = query_log::export_for_user(&st.pool, &id)
        .await
        .map_err(|e| ApiError::internal(e.to_string()))?;

    let as_csv = q.format.as_deref() == Some("csv");
    if as_csv {
        let mut body = String::from(
            "created_at,latency_ms,cpu_pct,ram_mb,gpu_util_pct,gpu_mem_mb,question,answer\n",
        );
        for r in &rows {
            body.push_str(&format!(
                "{},{},{},{},{},{},{},{}\n",
                r.created_at,
                r.latency_ms.unwrap_or(0),
                r.cpu_pct.unwrap_or(0.0),
                r.ram_mb.unwrap_or(0.0),
                r.gpu_util_pct.map(|v| v.to_string()).unwrap_or_default(),
                r.gpu_mem_mb.map(|v| v.to_string()).unwrap_or_default(),
                csv_field(&r.question),
                csv_field(r.answer.as_deref().unwrap_or("")),
            ));
        }
        Ok((
            [
                (axum::http::header::CONTENT_TYPE, "text/csv".to_string()),
                (
                    axum::http::header::CONTENT_DISPOSITION,
                    format!("attachment; filename=\"queries-{id}.csv\""),
                ),
            ],
            body,
        )
            .into_response())
    } else {
        let json = serde_json::to_string_pretty(&rows)
            .map_err(|e| ApiError::internal(e.to_string()))?;
        Ok((
            [
                (axum::http::header::CONTENT_TYPE, "application/json".to_string()),
                (
                    axum::http::header::CONTENT_DISPOSITION,
                    format!("attachment; filename=\"queries-{id}.json\""),
                ),
            ],
            json,
        )
            .into_response())
    }
}

/// Escape a value for a single CSV field (quote + double inner quotes).
fn csv_field(s: &str) -> String {
    let cleaned = s.replace('\n', " ").replace('\r', " ");
    format!("\"{}\"", cleaned.replace('"', "\"\""))
}

/// One row of the corpus-wide chunk export — a chunk joined with its document.
#[derive(sqlx::FromRow)]
struct ChunkExportRow {
    document: String,
    chunk_index: i64,
    unit_type: String,
    unit_label: Option<String>,
    unit_path: Option<String>,
    page_number: Option<i64>,
    parent_chunk_id: Option<String>,
    length: i64,
    token_count: i64,
    content: String,
}

#[derive(Deserialize)]
pub struct ChunkExportQuery {
    /// When set, export only this document's chunks; otherwise the whole corpus.
    pub source_id: Option<String>,
}

/// GET /api/admin/chunks/export[?source_id=…] — chunks as a downloadable CSV
/// (Excel opens it natively). Without `source_id` the whole corpus is exported;
/// with it, only that document's chunks. One row per chunk with its document,
/// citation label, unit path, page, parent link, length and content — the
/// chunk-level audit view used to spot missing provisions, duplicate labels and
/// bad pages.
pub async fn export_chunks(
    State(st): State<AppState>,
    _admin: AdminUser,
    Query(q): Query<ChunkExportQuery>,
) -> ApiResult<Response> {
    let source_id = q.source_id.filter(|s| !s.trim().is_empty());
    let base = "SELECT s.file_name AS document, c.chunk_index, c.unit_type, \
                c.unit_label, c.unit_path, c.page_number, c.parent_chunk_id, \
                c.char_end AS length, c.token_count, c.content \
         FROM chunks c JOIN sources s ON s.id = c.source_id";
    let rows: Vec<ChunkExportRow> = match &source_id {
        Some(id) => sqlx::query_as(&format!(
            "{base} WHERE c.source_id = $1 ORDER BY c.chunk_index"
        ))
        .bind(id)
        .fetch_all(&st.pool)
        .await,
        None => sqlx::query_as(&format!("{base} ORDER BY s.file_name, c.chunk_index"))
            .fetch_all(&st.pool)
            .await,
    }
    .map_err(|e| ApiError::internal(e.to_string()))?;

    // Filename: the document's own name for a single-doc export, else the corpus.
    let filename = match &source_id {
        Some(_) => rows
            .first()
            .map(|r| sanitize_filename(&r.document))
            .unwrap_or_else(|| "document".to_string()),
        None => "all-chunks".to_string(),
    };

    let mut body = String::from(
        "document,chunk_index,unit_type,unit_label,unit_path,page_number,parent_chunk_id,length,token_count,content\n",
    );
    for r in &rows {
        body.push_str(&format!(
            "{},{},{},{},{},{},{},{},{},{}\n",
            csv_field(&r.document),
            r.chunk_index,
            csv_field(&r.unit_type),
            csv_field(r.unit_label.as_deref().unwrap_or("")),
            csv_field(r.unit_path.as_deref().unwrap_or("")),
            r.page_number.map(|v| v.to_string()).unwrap_or_default(),
            csv_field(r.parent_chunk_id.as_deref().unwrap_or("")),
            r.length,
            r.token_count,
            csv_field(&r.content),
        ));
    }

    Ok((
        [
            (axum::http::header::CONTENT_TYPE, "text/csv".to_string()),
            (
                axum::http::header::CONTENT_DISPOSITION,
                format!("attachment; filename=\"chunks-{filename}.csv\""),
            ),
        ],
        body,
    )
        .into_response())
}

/// One chunk as the browser shows it. Mirrors `ChunkExportRow` minus the join,
/// plus `id` (needed to deep-link a citation) and `label_status` (the parse-health
/// flag the importer already writes and nothing has ever surfaced).
#[derive(Serialize, sqlx::FromRow)]
pub struct ChunkBrowseRow {
    pub id: String,
    pub chunk_index: i64,
    pub unit_type: String,
    pub unit_label: Option<String>,
    pub unit_path: Option<String>,
    pub page_number: Option<i64>,
    pub token_count: i64,
    pub label_status: Option<String>,
    pub content: String,
}

#[derive(Deserialize)]
pub struct ChunkBrowseQuery {
    pub source_id: String,
    #[serde(default)]
    pub offset: i64,
    pub limit: Option<i64>,
    /// Optional case-insensitive substring filter over label and content.
    pub q: Option<String>,
}

#[derive(Serialize)]
pub struct ChunkBrowseResp {
    pub total: i64,
    pub offset: i64,
    pub chunks: Vec<ChunkBrowseRow>,
}

/// Largest page a browse request may ask for. A document can hold thousands of
/// chunks; an unbounded limit would let one request pull the whole corpus into
/// memory and stall the server.
const CHUNK_BROWSE_MAX_LIMIT: i64 = 200;

/// GET /api/admin/chunks?source_id=…&offset=…&limit=…&q=… — paginated chunk
/// browse for one document. The CSV export answers "give me everything to audit
/// offline"; this answers "show me this document's chunks in the UI", and backs
/// both the indexing console and the click-through from a citation.
pub async fn browse_chunks(
    State(st): State<AppState>,
    _admin: AdminUser,
    Query(q): Query<ChunkBrowseQuery>,
) -> ApiResult<Json<ChunkBrowseResp>> {
    if q.source_id.trim().is_empty() {
        return Err(ApiError::bad_request("source_id is required"));
    }
    let limit = q.limit.unwrap_or(50).clamp(1, CHUNK_BROWSE_MAX_LIMIT);
    let offset = q.offset.max(0);
    // `%` and `_` are LIKE wildcards; escape them so a literal search for "50%"
    // does not silently match everything.
    let filter = q.q.as_deref().map(str::trim).filter(|s| !s.is_empty()).map(|s| {
        format!("%{}%", s.replace('\\', "\\\\").replace('%', "\\%").replace('_', "\\_"))
    });

    let (total, chunks) = match &filter {
        Some(pat) => {
            let total: i64 = sqlx::query_scalar(
                "SELECT COUNT(*) FROM chunks WHERE source_id = $1 \
                 AND (content ILIKE $2 ESCAPE '\\' OR unit_label ILIKE $2 ESCAPE '\\')",
            )
            .bind(&q.source_id).bind(pat)
            .fetch_one(&st.pool).await.map_err(|e| ApiError::internal(e.to_string()))?;
            let rows: Vec<ChunkBrowseRow> = sqlx::query_as(
                "SELECT id, chunk_index, unit_type, unit_label, unit_path, page_number, \
                        token_count, label_status, content \
                 FROM chunks WHERE source_id = $1 \
                 AND (content ILIKE $2 ESCAPE '\\' OR unit_label ILIKE $2 ESCAPE '\\') \
                 ORDER BY chunk_index LIMIT $3 OFFSET $4",
            )
            .bind(&q.source_id).bind(pat).bind(limit).bind(offset)
            .fetch_all(&st.pool).await.map_err(|e| ApiError::internal(e.to_string()))?;
            (total, rows)
        }
        None => {
            let total: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM chunks WHERE source_id = $1")
                .bind(&q.source_id)
                .fetch_one(&st.pool).await.map_err(|e| ApiError::internal(e.to_string()))?;
            let rows: Vec<ChunkBrowseRow> = sqlx::query_as(
                "SELECT id, chunk_index, unit_type, unit_label, unit_path, page_number, \
                        token_count, label_status, content \
                 FROM chunks WHERE source_id = $1 \
                 ORDER BY chunk_index LIMIT $2 OFFSET $3",
            )
            .bind(&q.source_id).bind(limit).bind(offset)
            .fetch_all(&st.pool).await.map_err(|e| ApiError::internal(e.to_string()))?;
            (total, rows)
        }
    };
    Ok(Json(ChunkBrowseResp { total, offset, chunks }))
}

/// A reduced document descriptor for the chunk-export picker.
#[derive(Serialize, sqlx::FromRow)]
pub struct ChunkDocRow {
    pub source_id: String,
    pub file_name: String,
    pub chunk_count: i64,
}

/// GET /api/admin/chunks/documents — documents that have chunks, with counts,
/// so the admin can pick one to export (or export them all).
pub async fn list_chunk_documents(
    State(st): State<AppState>,
    _admin: AdminUser,
) -> ApiResult<Json<Vec<ChunkDocRow>>> {
    let rows: Vec<ChunkDocRow> = sqlx::query_as(
        "SELECT s.id AS source_id, s.file_name, COUNT(c.id) AS chunk_count \
         FROM sources s JOIN chunks c ON c.source_id = s.id \
         GROUP BY s.id, s.file_name ORDER BY s.file_name",
    )
    .fetch_all(&st.pool)
    .await
    .map_err(|e| ApiError::internal(e.to_string()))?;
    Ok(Json(rows))
}

/// What a bulk delete would destroy, per document.
#[derive(Serialize, sqlx::FromRow)]
pub struct DeleteTarget {
    pub source_id: String,
    pub file_name: String,
    pub chunk_count: i64,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DeleteBulkReq {
    pub source_ids: Vec<String>,
    /// When true, report what WOULD be deleted and change nothing.
    #[serde(default)]
    pub preview: bool,
}

#[derive(Serialize)]
pub struct DeleteBulkResp {
    pub targets: Vec<DeleteTarget>,
    pub deleted: usize,
    pub preview: bool,
}

/// Hard cap on one bulk delete. A delete is irreversible; an accidental
/// select-all on a large corpus should be refused, not obeyed.
const DELETE_BULK_MAX: usize = 100;

/// POST /api/admin/sources/delete_bulk — delete several documents at once,
/// or (with `preview: true`) report exactly what would go.
///
/// The preview is not decoration: the UI confirm dialog names each document and
/// its chunk count, so nothing is ever destroyed on an unlabelled "are you sure".
/// Ids that match no source are reported back as missing rather than silently
/// ignored — the folder-delete bug was a silent no-op that looked like success.
pub async fn delete_sources_bulk(
    State(st): State<AppState>,
    _admin: AdminUser,
    Json(req): Json<DeleteBulkReq>,
) -> ApiResult<Json<DeleteBulkResp>> {
    let ids: Vec<String> = req
        .source_ids
        .into_iter()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect();
    if ids.is_empty() {
        return Err(ApiError::bad_request("no source_ids given"));
    }
    if ids.len() > DELETE_BULK_MAX {
        return Err(ApiError::bad_request(format!(
            "refusing to delete {} documents in one request (max {DELETE_BULK_MAX})",
            ids.len()
        )));
    }

    let targets: Vec<DeleteTarget> = sqlx::query_as(
        "SELECT s.id AS source_id, s.file_name, \
                (SELECT COUNT(*) FROM chunks c WHERE c.source_id = s.id) AS chunk_count \
         FROM sources s WHERE s.id = ANY($1) ORDER BY s.file_name",
    )
    .bind(&ids)
    .fetch_all(&st.pool)
    .await
    .map_err(|e| ApiError::internal(e.to_string()))?;

    if targets.len() != ids.len() {
        let found: std::collections::HashSet<&str> =
            targets.iter().map(|t| t.source_id.as_str()).collect();
        let missing: Vec<&str> =
            ids.iter().map(String::as_str).filter(|i| !found.contains(i)).collect();
        log::warn!("delete_bulk: {} id(s) matched no source: {:?}", missing.len(), missing);
    }

    if req.preview {
        return Ok(Json(DeleteBulkResp { targets, deleted: 0, preview: true }));
    }

    let mut deleted = 0usize;
    for t in &targets {
        match super::documents::hard_delete_source(&st, &t.source_id).await {
            Ok(outcome) => {
                // B1 — `deleted` was incremented unconditionally, so a bulk
                // delete that retained everything still reported a full count.
                if outcome.deleted() { deleted += 1; }
                if let Some(p) = outcome.file_path() {
                    // Best-effort: the row is already gone, so a stale file on
                    // disk must not turn a successful delete into an error.
                    if let Err(e) = std::fs::remove_file(p) {
                        log::warn!("delete_bulk: source {} row deleted, file {p} left: {e}", t.source_id);
                    }
                }
            }
            Err(e) => {
                log::error!("delete_bulk: failed to delete {}: {}", t.source_id, e.message);
                return Err(e);
            }
        }
    }
    log::info!("delete_bulk: deleted {deleted} document(s)");
    Ok(Json(DeleteBulkResp { targets, deleted, preview: false }))
}

/// One document that still needs a derived layer built.
///
/// `owner` is NULL for the shared corpus and the owning username for a user's
/// own document, so the admin can tell "the corpus is missing a summary" from
/// "someone's upload is missing one" without cross-referencing another screen.
#[derive(Serialize, sqlx::FromRow)]
pub struct PendingDoc {
    pub source_id: String,
    pub file_name: String,
    #[sqlx(default)]
    pub owner: Option<String>,
}

/// One `(edge_type, count)` pair for the corpus-wide graph stat.
#[derive(Serialize, sqlx::FromRow)]
pub struct EdgeCount {
    pub edge_type: String,
    pub count: i64,
}

/// GET /api/admin/derived_status — which documents still need each derived
/// layer, so the admin can see what is PENDING instead of firing blind.
/// Summaries and structure are per-document (a list each); the graph overlay is
/// corpus-wide (regenerated whole), so it returns edge counts by type rather
/// than a per-document list. Read-only; safe to poll while a build runs so the
/// UI can show a document dropping off its list as it completes.
pub async fn derived_status(
    State(st): State<AppState>,
    _admin: AdminUser,
) -> ApiResult<Json<serde_json::Value>> {
    // Build summaries: indexed docs with no whole-document (`document_full`) summary.
    //
    // Chat attachments are excluded (`conversation_id IS NULL`). They are private
    // to one conversation, capped at a few pages, and deliberately transient — a
    // pre-baked whole-document summary is never served for them, so listing them
    // here only buried the corpus documents the admin is actually looking for.
    // Note this is a LISTING scope, not the retrieval invariant in
    // `sources::visible_source_ids`; user-owned documents legitimately want
    // summaries (long uploads route through the same builder), so they stay in
    // the list and are labelled with their owner instead of hidden.
    let summaries_pending: Vec<PendingDoc> = sqlx::query_as(
        "SELECT s.id AS source_id, s.file_name, u.username AS owner FROM sources s \
         LEFT JOIN users u ON u.id = s.owner_id \
         WHERE s.indexing_status = 'indexed' \
           AND s.conversation_id IS NULL \
           AND s.disabled_at IS NULL \
           AND NOT EXISTS (SELECT 1 FROM graph_summary gs \
                           WHERE gs.source_id = s.id AND gs.level = 'document_full') \
         ORDER BY (u.username IS NOT NULL), s.file_name",
    )
    .fetch_all(&st.pool)
    .await
    .map_err(|e| ApiError::internal(e.to_string()))?;

    // Re-index structure: the SAME predicate the button's own job uses
    // (`reindex_structure_all`) so the list matches exactly what it will process.
    let structure_pending: Vec<PendingDoc> = sqlx::query_as(
        "SELECT id AS source_id, file_name FROM sources \
         WHERE indexing_status = 'indexed' \
           AND structure_importer_version < $1 \
           AND format = 'pdf' \
         ORDER BY file_name",
    )
    .bind(crate::services::structured_importer::STRUCTURED_IMPORTER_VERSION)
    .fetch_all(&st.pool)
    .await
    .map_err(|e| ApiError::internal(e.to_string()))?;

    // Graph overlay is corpus-wide → a stat, not a per-doc list.
    let graph_edges: Vec<EdgeCount> = sqlx::query_as(
        "SELECT edge_type, COUNT(*) AS count FROM graph_edges GROUP BY edge_type ORDER BY edge_type",
    )
    .fetch_all(&st.pool)
    .await
    .map_err(|e| ApiError::internal(e.to_string()))?;

    Ok(Json(serde_json::json!({
        "summaries_pending": summaries_pending,
        "structure_pending": structure_pending,
        "graph_edges": graph_edges,
    })))
}

/// One document inside a duplicate group.
#[derive(Serialize, sqlx::FromRow)]
pub struct DuplicateDoc {
    pub source_id: String,
    pub file_name: String,
    pub owner: Option<String>,
    pub chunk_count: i64,
    pub created_at: i64,
    pub conversation_id: Option<String>,
}

/// A set of documents that look like the same thing.
#[derive(Serialize)]
pub struct DuplicateGroup {
    /// `content` — byte-identical (same SHA-256). `name` — confusably similar
    /// titles. The distinction matters: content groups are safe to collapse
    /// mechanically, name groups need a human to look, because an amended
    /// instrument beside its base text is two documents, not one.
    pub kind: &'static str,
    pub docs: Vec<DuplicateDoc>,
}

/// GET /api/admin/duplicates — documents already in the corpus that look like
/// duplicates of each other.
///
/// Migration 022 stops NEW duplicates being created; it deliberately does not
/// merge the ones already stored, because merging is destructive and the two
/// kinds below need different judgement. This endpoint is the review surface for
/// that backlog: it reports, it never deletes.
pub async fn duplicates(
    State(st): State<AppState>,
    _admin: AdminUser,
) -> ApiResult<Json<serde_json::Value>> {
    // Everything indexed, with its chunk count, so the admin can see which copy
    // is the substantive one before deciding anything.
    let rows: Vec<DuplicateDoc> = sqlx::query_as(
        "SELECT s.id AS source_id, s.file_name, u.username AS owner, \
                COALESCE(cc.n, 0) AS chunk_count, s.created_at, s.conversation_id \
         FROM sources s \
         LEFT JOIN users u ON u.id = s.owner_id \
         LEFT JOIN (SELECT source_id, COUNT(*) AS n FROM chunks GROUP BY source_id) cc \
              ON cc.source_id = s.id \
         WHERE s.indexing_status = 'indexed' \
         ORDER BY s.created_at ASC",
    )
    .fetch_all(&st.pool)
    .await
    .map_err(|e| ApiError::internal(e.to_string()))?;

    // Byte-identical content. Keyed on the hash the row actually carries, so the
    // pre-022 attachment rows with an empty hash cannot collide with each other:
    // a blank hash is an absence of evidence, not a match.
    let hashes: Vec<(String, String)> = sqlx::query_as(
        "SELECT id, file_hash FROM sources \
         WHERE indexing_status = 'indexed' AND file_hash <> ''",
    )
    .fetch_all(&st.pool)
    .await
    .map_err(|e| ApiError::internal(e.to_string()))?;

    let by_id: std::collections::HashMap<&str, &DuplicateDoc> =
        rows.iter().map(|d| (d.source_id.as_str(), d)).collect();
    let mut by_hash: std::collections::HashMap<String, Vec<&DuplicateDoc>> =
        std::collections::HashMap::new();
    for (id, hash) in &hashes {
        if let Some(doc) = by_id.get(id.as_str()) {
            by_hash.entry(hash.clone()).or_default().push(doc);
        }
    }
    let mut groups: Vec<DuplicateGroup> = Vec::new();
    let mut already_grouped: std::collections::HashSet<&str> = std::collections::HashSet::new();
    let mut content: Vec<Vec<&DuplicateDoc>> =
        by_hash.into_values().filter(|v| v.len() > 1).collect();
    content.sort_by_key(|g| std::cmp::Reverse(g.len()));
    for g in content {
        for d in &g {
            already_grouped.insert(d.source_id.as_str());
        }
        groups.push(DuplicateGroup {
            kind: "content",
            docs: g.into_iter().map(clone_doc).collect(),
        });
    }

    // Confusably similar titles, over what is NOT already an exact-content group
    // (reporting the same twelve rows twice would bury the cases that need a
    // human). Single-link grouping: A joins the group if it matches any member.
    let remaining: Vec<&DuplicateDoc> = rows
        .iter()
        .filter(|d| !already_grouped.contains(d.source_id.as_str()))
        .collect();
    let mut used = vec![false; remaining.len()];
    for i in 0..remaining.len() {
        if used[i] {
            continue;
        }
        let mut group = vec![remaining[i]];
        used[i] = true;
        for j in (i + 1)..remaining.len() {
            if used[j] {
                continue;
            }
            if group.iter().any(|m| {
                crate::api::uploads::title_similarity(&m.file_name, &remaining[j].file_name)
                    >= crate::api::uploads::SIMILAR_TITLE_THRESHOLD
            }) {
                group.push(remaining[j]);
                used[j] = true;
            }
        }
        if group.len() > 1 {
            groups.push(DuplicateGroup {
                kind: "name",
                docs: group.into_iter().map(clone_doc).collect(),
            });
        }
    }

    let total: usize = groups.iter().map(|g| g.docs.len()).sum();
    Ok(Json(serde_json::json!({
        "groups": groups,
        "group_count": groups.len(),
        "document_count": total,
    })))
}

fn clone_doc(d: &DuplicateDoc) -> DuplicateDoc {
    DuplicateDoc {
        source_id: d.source_id.clone(),
        file_name: d.file_name.clone(),
        owner: d.owner.clone(),
        chunk_count: d.chunk_count,
        created_at: d.created_at,
        conversation_id: d.conversation_id.clone(),
    }
}

/// Query bounds for the feedback summary (both optional, epoch ms on created_at).
#[derive(Deserialize)]
pub struct FeedbackSummaryQuery {
    pub from: Option<i64>,
    pub to: Option<i64>,
}

/// GET /api/admin/feedback/summary?from=&to= (Feature 018, FR-009) — distribution
/// of both citation metrics over CURRENT feedback rows, for prioritizing fixes.
pub async fn feedback_summary(
    State(st): State<AppState>,
    _admin: AdminUser,
    Query(q): Query<FeedbackSummaryQuery>,
) -> ApiResult<Json<crate::db::repositories::answer_feedback::FeedbackSummary>> {
    let summary = crate::db::repositories::answer_feedback::summary(&st.pool, q.from, q.to)
        .await
        .map_err(|e| ApiError::internal(e.to_string()))?;
    Ok(Json(summary))
}

/// F021 (FR-010) — GET /api/admin/usage: per-user document storage usage
/// (personal documents + live conversation attachments).
pub async fn usage(
    State(st): State<AppState>,
    _admin: AdminUser,
) -> ApiResult<Json<Vec<serde_json::Value>>> {
    let rows: Vec<(String, String, Option<i64>, i64)> = sqlx::query_as(
        "SELECT u.id, u.username, SUM(s.file_size_bytes)::BIGINT, COUNT(s.id) \
         FROM users u LEFT JOIN sources s ON s.owner_id = u.id \
         GROUP BY u.id, u.username ORDER BY u.username",
    )
    .fetch_all(&st.pool)
    .await
    .map_err(|e| ApiError::internal(e.to_string()))?;
    Ok(Json(
        rows.into_iter()
            .map(|(id, username, bytes, files)| {
                serde_json::json!({
                    "userId": id,
                    "username": username,
                    "usedBytes": bytes.unwrap_or(0),
                    "usedFiles": files,
                })
            })
            .collect(),
    ))
}

// ─── Inference engine toggle (admin) — local GPU (vLLM) ⇄ hosted API (Groq/…) ──

/// GET /api/admin/inference — current engine + remote-API config. Never returns
/// the API key; only whether one is set (`has_api_key`).
pub async fn get_inference(_admin: AdminUser) -> ApiResult<Json<serde_json::Value>> {
    use crate::services::inference::{engine_str, runtime};
    let s = runtime::status();
    Ok(Json(serde_json::json!({
        "engine": engine_str(s.engine),
        "remote_base_url": s.remote_base_url,
        "remote_model": s.remote_model,
        "remote_endpoint": s.remote_endpoint,
        "remote_prefix": s.remote_prefix,
        "has_api_key": s.has_api_key,
    })))
}

#[derive(Deserialize)]
pub struct SetInferenceReq {
    /// `vllm` (local GPU) or `remote`/`api` (hosted API); `llamacpp` also accepted.
    pub engine: String,
    /// Bearer key for the hosted API. Blank/omitted keeps the stored key.
    pub api_key: Option<String>,
    pub model: Option<String>,
    pub base_url: Option<String>,
    /// `chat` (default) or `completions`.
    pub endpoint: Option<String>,
    /// API path prefix between base URL and route. `/v1` for Groq/OpenRouter/
    /// Together; empty for Google Gemini (whose base URL already ends `/v1beta/openai`).
    /// A `None` value leaves the stored prefix untouched; `Some("")` sets it empty.
    pub prefix: Option<String>,
}

/// POST /api/admin/inference — switch engine and/or set the remote-API config.
/// Persists to `db_config` and applies at runtime (no restart). Selecting the
/// remote engine without any stored/provided key is rejected.
pub async fn set_inference(
    State(st): State<AppState>,
    admin: AdminUser,
    Json(req): Json<SetInferenceReq>,
) -> ApiResult<Json<serde_json::Value>> {
    use crate::services::inference::remote::Endpoint;
    use crate::services::inference::{engine_str, parse_engine, runtime, EngineKind};

    let engine = parse_engine(&req.engine);
    // Merge onto the current remote config so a partial update (e.g. just flipping
    // the engine) keeps the previously-saved URL/model/key.
    let mut remote = runtime::current_remote();
    if let Some(v) = req.base_url {
        let v = v.trim();
        if !v.is_empty() {
            remote.base_url = v.trim_end_matches('/').to_string();
        }
    }
    if let Some(v) = req.model {
        let v = v.trim();
        if !v.is_empty() {
            remote.model = v.to_string();
        }
    }
    if let Some(v) = req.endpoint {
        remote.endpoint = Endpoint::from_str_or_default(&v);
    }
    // Unlike URL/model, an *empty* prefix is meaningful (Gemini) — so `Some("")`
    // sets it empty; only `None` (field omitted) keeps the stored prefix.
    if let Some(v) = req.prefix {
        remote.api_prefix = crate::services::inference::remote::normalize_prefix(&v);
    }
    if let Some(v) = req.api_key {
        let v = v.trim().to_string();
        if !v.is_empty() {
            remote.api_key = Some(v);
        }
    }

    if engine == EngineKind::Remote
        && remote.api_key.as_ref().map(|k| k.is_empty()).unwrap_or(true)
    {
        return Err(ApiError::new(
            axum::http::StatusCode::BAD_REQUEST,
            "no_key",
            "Hosted API selected but no API key is set — provide the API key.",
        ));
    }

    runtime::save_to_db(&st.pool, engine, &remote)
        .await
        .map_err(|e| ApiError::internal(e.to_string()))?;
    runtime::apply(engine, remote);

    let _ = crate::db::repositories::audit::insert_user_event(
        &st.pool,
        Some(&admin.0.id),
        "admin_set_inference_engine",
        None,
        Some(engine_str(engine)),
    )
    .await;

    Ok(Json(serde_json::json!({ "ok": true, "engine": engine_str(engine) })))
}

/// GET /api/admin/providers — the provider/model catalogue backing the picker.
///
/// Served from the server (not compiled into the frontend bundle) so a refreshed
/// catalogue can be dropped at `{NOVIQ_DATA_DIR}/provider_catalog.json` without a
/// frontend rebuild — model ids retire on a ~12-month cycle. Contains no secrets:
/// connection shapes, published limits and commercial terms only.
pub async fn get_providers(_admin: AdminUser) -> ApiResult<Json<serde_json::Value>> {
    let c = crate::services::inference::catalog::catalog();
    Ok(Json(serde_json::json!({
        "version": c.version,
        "note": c.note,
        "context_warn_tokens": crate::services::inference::catalog::CONTEXT_WARN_TOKENS,
        "providers": c.providers,
    })))
}

#[derive(Deserialize)]
pub struct ValidateInferenceReq {
    /// Blank/omitted → probe with the stored key (lets an admin re-test an
    /// existing configuration without re-typing the secret).
    pub api_key: Option<String>,
    pub model: Option<String>,
    pub base_url: Option<String>,
    pub prefix: Option<String>,
}

/// POST /api/admin/inference/validate — probe a key/model pair against the live
/// provider *without* saving anything.
///
/// Validating the key alone is not enough: entitlement is per model. The live
/// example is Google's free tier, where one valid key returns 200 on Flash and
/// 404/403 on Pro. Probing the actual selected model is the only way to tell an
/// operator the truth before they save and discover it in production.
///
/// The probe is a 1-token non-streaming completion — the cheapest call that still
/// exercises auth, routing and model entitlement together.
pub async fn validate_inference(
    _admin: AdminUser,
    Json(req): Json<ValidateInferenceReq>,
) -> ApiResult<Json<serde_json::Value>> {
    use crate::services::inference::{remote::normalize_prefix, runtime};

    let cur = runtime::current_remote();
    let base_url = req
        .base_url
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .unwrap_or(&cur.base_url)
        .trim_end_matches('/')
        .to_string();
    let model = req
        .model
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .unwrap_or(&cur.model)
        .to_string();
    // An empty prefix is meaningful (Gemini), so only an omitted field falls back.
    let prefix = match req.prefix.as_deref() {
        Some(p) => normalize_prefix(p),
        None => cur.api_prefix.clone(),
    };
    let key = req
        .api_key
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
        .or_else(|| cur.api_key.clone());

    if base_url.is_empty() {
        return Err(ApiError::new(
            axum::http::StatusCode::BAD_REQUEST,
            "no_base_url",
            "No base URL to probe — pick a provider or enter one under Custom.",
        ));
    }

    let url = format!("{base_url}{prefix}/chat/completions");
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(20))
        .build()
        .map_err(|e| ApiError::internal(e.to_string()))?;
    let mut rb = client.post(&url).json(&serde_json::json!({
        "model": model,
        "messages": [{ "role": "user", "content": "ping" }],
        "max_tokens": 1,
        "stream": false,
    }));
    if let Some(k) = &key {
        rb = rb.bearer_auth(k);
    }

    let started = std::time::Instant::now();
    let resp = match rb.send().await {
        Ok(r) => r,
        // A transport failure is a real, reportable outcome (bad host, DNS, TLS,
        // no egress) — not a 500 on our side.
        Err(e) => {
            return Ok(Json(serde_json::json!({
                "ok": false,
                "status": 0,
                "reason": "unreachable",
                "message": format!("Could not reach {url}: {e}"),
            })));
        }
    };
    let elapsed_ms = started.elapsed().as_millis() as u64;
    let status = resp.status();
    let body = resp.text().await.unwrap_or_default();

    // Map the provider's status onto an explanation an operator can act on. 429
    // is deliberately reported as a *success* for the key: it proves auth and
    // model entitlement both passed, and only the rate limit was hit.
    let (ok, reason, message) = match status.as_u16() {
        200 => (true, "ok", format!("Works — {model} responded in {elapsed_ms} ms.")),
        429 => (
            true,
            "rate_limited",
            format!("Key and model are valid, but the provider is rate-limiting right now ({model})."),
        ),
        401 | 403 => (
            false,
            "unauthorized",
            if key.is_some() {
                format!("Provider rejected the API key for {model} (HTTP {status}).")
            } else {
                "No API key was supplied and the provider requires one.".to_string()
            },
        ),
        404 => (
            false,
            "model_not_found",
            format!(
                "The key is reaching {base_url}, but model \"{model}\" is not available to it \
                 (HTTP 404). This is usually a model that has been retired, renamed, or is not \
                 included in your plan/tier."
            ),
        ),
        400 => (
            false,
            "bad_request",
            format!("Provider rejected the request for {model} (HTTP 400): {}", truncate(&body, 300)),
        ),
        _ => (
            false,
            "upstream_error",
            format!("Provider returned HTTP {status}: {}", truncate(&body, 300)),
        ),
    };

    Ok(Json(serde_json::json!({
        "ok": ok,
        "status": status.as_u16(),
        "reason": reason,
        "message": message,
        "model": model,
        "url": url,
        "elapsed_ms": elapsed_ms,
    })))
}

/// Clip an upstream error body so a provider's HTML error page can't flood the UI.
fn truncate(s: &str, max: usize) -> String {
    let s = s.trim();
    if s.chars().count() <= max {
        return s.to_string();
    }
    let cut: String = s.chars().take(max).collect();
    format!("{cut}…")
}

/// Make a document name safe for a download filename (drop path separators and
/// characters browsers/OSes dislike in a `Content-Disposition` filename).
fn sanitize_filename(name: &str) -> String {
    let stem = name.rsplit(['/', '\\']).next().unwrap_or(name);
    let stem = stem.strip_suffix(".pdf").unwrap_or(stem);
    let cleaned: String = stem
        .chars()
        .map(|c| if c.is_alphanumeric() || c == '-' || c == '_' { c } else { '_' })
        .collect();
    let trimmed = cleaned.trim_matches('_');
    if trimmed.is_empty() { "document".to_string() } else { trimmed.to_string() }
}

#[derive(Deserialize)]
pub struct CreateUserReq {
    pub username: String,
    /// 'user' (default) or 'admin'. Anything else is rejected.
    pub role: Option<String>,
}

#[derive(Serialize)]
pub struct CreateUserResp {
    pub id: String,
    pub username: String,
    pub role: String,
    /// One-time temporary password to hand to the tester; not stored in plaintext.
    pub temp_password: String,
}

/// POST /api/admin/users — create a tester with a generated temp password.
pub async fn create_user(
    State(st): State<AppState>,
    admin: AdminUser,
    Json(req): Json<CreateUserReq>,
) -> ApiResult<Json<CreateUserResp>> {
    let username = req.username.trim().to_string();
    if username.is_empty() {
        return Err(ApiError::new(axum::http::StatusCode::BAD_REQUEST, "invalid", "username required"));
    }
    let role = match req.role.as_deref() {
        None | Some("user") => "user",
        Some("admin") => "admin",
        Some(_) => {
            return Err(ApiError::new(axum::http::StatusCode::BAD_REQUEST, "invalid", "role must be 'user' or 'admin'"));
        }
    };

    // Short 6-char temp password (lowercase hex) so admins can read it out to
    // testers; weak by design — acceptable only because it is a one-time
    // hand-off credential for internal tester accounts.
    let temp_password = Uuid::new_v4().simple().to_string()[..6].to_string();
    let hash = hash_password(&temp_password).map_err(ApiError::internal)?;
    let id = Uuid::new_v4().to_string();

    let res = sqlx::query(
        "INSERT INTO users (id, username, password_hash, created_at, role) VALUES ($1,$2,$3,$4,$5)",
    )
    .bind(&id)
    .bind(&username)
    .bind(&hash)
    .bind(now_secs())
    .bind(role)
    .execute(&st.pool)
    .await;

    if let Err(e) = res {
        // Unique-violation on username → 409 with a clear message.
        if e.to_string().contains("unique") || e.to_string().contains("duplicate") {
            return Err(ApiError::new(axum::http::StatusCode::CONFLICT, "exists", "username already taken"));
        }
        return Err(ApiError::internal(e.to_string()));
    }

    let _ = crate::db::repositories::audit::insert_user_event(
        &st.pool, Some(&admin.0.id), "admin_create_user", Some(&id), None,
    )
    .await;

    Ok(Json(CreateUserResp { id, username, role: role.to_string(), temp_password }))
}

// ── Admin-gated sharing (2026-07-28) ─────────────────────────────────────────

/// GET /api/admin/share_requests — the pending review queue, oldest first.
pub async fn list_share_requests(
    State(st): State<AppState>,
    _admin: AdminUser,
) -> ApiResult<Json<Vec<crate::db::repositories::share_requests::ShareRequestRow>>> {
    let rows = crate::db::repositories::share_requests::list_pending(&st.pool)
        .await
        .map_err(|e| ApiError::internal(e.to_string()))?;
    Ok(Json(rows))
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DecideShareReq {
    pub approve: bool,
}

/// POST /api/admin/share_requests/:id/decide — approve (document becomes
/// shared) or reject (stays private). Idempotent: an already-decided or
/// unknown request 404s rather than double-applying.
pub async fn decide_share_request(
    State(st): State<AppState>,
    admin: AdminUser,
    Path(id): Path<String>,
    Json(req): Json<DecideShareReq>,
) -> ApiResult<Json<serde_json::Value>> {
    let source_id = crate::db::repositories::share_requests::decide(
        &st.pool, &id, req.approve, &admin.0.id,
    )
    .await
    .map_err(|e| ApiError::internal(e.to_string()))?;
    let Some(source_id) = source_id else {
        return Err(ApiError::new(
            axum::http::StatusCode::NOT_FOUND,
            "not_pending",
            "request not found or already decided",
        ));
    };
    if req.approve {
        crate::db::repositories::sources::set_visibility_source(&st.pool, &source_id, "shared")
            .await
            .map_err(|e| ApiError::internal(e.to_string()))?;
    }
    let event = if req.approve { "share_approve" } else { "share_reject" };
    let _ = crate::db::repositories::audit::insert_user_event(
        &st.pool, Some(&admin.0.id), event, Some(&source_id), None,
    )
    .await;
    Ok(Json(serde_json::json!({ "ok": true, "approved": req.approve })))
}

/// GET /api/admin/sources/:id/download — the raw file, so the admin can
/// review a share request's document (and re-upload a canonical copy if
/// they prefer that over approving in place).
pub async fn download_source(
    State(st): State<AppState>,
    _admin: AdminUser,
    Path(id): Path<String>,
) -> Result<axum::response::Response, ApiError> {
    use axum::response::IntoResponse;
    let row: Option<(String, Option<String>)> =
        sqlx::query_as("SELECT file_path, file_name FROM sources WHERE id = $1")
            .bind(&id)
            .fetch_optional(&st.pool)
            .await
            .map_err(|e| ApiError::internal(e.to_string()))?;
    let Some((file_path, file_name)) = row else {
        return Err(ApiError::new(
            axum::http::StatusCode::NOT_FOUND,
            "not_found",
            "unknown source",
        ));
    };
    let bytes = tokio::fs::read(&file_path).await.map_err(|e| {
        ApiError::new(
            axum::http::StatusCode::NOT_FOUND,
            "file_missing",
            format!("file unreadable: {e}"),
        )
    })?;
    // ASCII-sanitized filename for the Content-Disposition header.
    let name = file_name
        .unwrap_or_else(|| "document".into())
        .chars()
        .map(|c| if c.is_ascii_graphic() || c == ' ' { c } else { '_' })
        .collect::<String>()
        .replace('"', "_");
    let headers = [
        (axum::http::header::CONTENT_TYPE, "application/octet-stream".to_string()),
        (
            axum::http::header::CONTENT_DISPOSITION,
            format!("attachment; filename=\"{name}\""),
        ),
    ];
    Ok((headers, bytes).into_response())
}

/// POST /api/admin/users/:id/disable — disable + revoke all sessions.
pub async fn disable_user(
    State(st): State<AppState>,
    admin: AdminUser,
    Path(id): Path<String>,
) -> ApiResult<axum::http::StatusCode> {
    if id == admin.0.id {
        return Err(ApiError::new(axum::http::StatusCode::BAD_REQUEST, "invalid", "cannot disable your own account"));
    }
    sqlx::query("UPDATE users SET disabled_at = $1 WHERE id = $2 AND disabled_at IS NULL")
        .bind(now_secs())
        .bind(&id)
        .execute(&st.pool)
        .await
        .map_err(|e| ApiError::internal(e.to_string()))?;
    // Cut access immediately.
    sqlx::query("DELETE FROM session WHERE user_id = $1")
        .bind(&id)
        .execute(&st.pool)
        .await
        .map_err(|e| ApiError::internal(e.to_string()))?;
    let _ = crate::db::repositories::audit::insert_user_event(
        &st.pool, Some(&admin.0.id), "admin_disable_user", Some(&id), None,
    )
    .await;
    Ok(axum::http::StatusCode::NO_CONTENT)
}

/// POST /api/admin/users/:id/enable — clear the disabled flag.
pub async fn enable_user(
    State(st): State<AppState>,
    admin: AdminUser,
    Path(id): Path<String>,
) -> ApiResult<axum::http::StatusCode> {
    sqlx::query("UPDATE users SET disabled_at = NULL WHERE id = $1")
        .bind(&id)
        .execute(&st.pool)
        .await
        .map_err(|e| ApiError::internal(e.to_string()))?;
    let _ = crate::db::repositories::audit::insert_user_event(
        &st.pool, Some(&admin.0.id), "admin_enable_user", Some(&id), None,
    )
    .await;
    Ok(axum::http::StatusCode::NO_CONTENT)
}

/// GET /api/admin/sources/:id/inspect — does this document hold one legal
/// instrument, or several?
///
/// The pipeline assumes file == instrument. `Compendium Rules under SLGA 2013`
/// is twelve rule-sets in one file, each numbering its provisions from 1, which
/// is why "Rule 4" is ambiguous across instruments and why `query_router`
/// carries a hand-written demotion for that document.
///
/// Read-only and deterministic — it proposes boundaries and never applies them.
/// A wrong split silently truncates an instrument, which is worse than not
/// splitting at all, so the answer is always shown to a human first. Results are
/// not cached: a re-uploaded edition is re-detected, so a compendium that gained
/// an instrument cannot inherit last year's boundaries.
///
/// Costs one `pdftotext` pass (about a second on thirty pages), so it is cheap
/// enough to run from the console on demand.
pub async fn inspect_source(
    State(st): State<AppState>,
    _admin: AdminUser,
    Path(id): Path<String>,
) -> ApiResult<Json<InspectResponse>> {
    let row: Option<(String, Option<String>)> =
        sqlx::query_as("SELECT file_path, file_name FROM sources WHERE id = $1")
            .bind(&id)
            .fetch_optional(&st.pool)
            .await
            .map_err(|e| ApiError::internal(e.to_string()))?;
    let Some((file_path, file_name)) = row else {
        return Err(ApiError::new(
            axum::http::StatusCode::NOT_FOUND,
            "not_found",
            "unknown source",
        ));
    };

    // pdftotext + regex over a whole document: enough work to be worth keeping
    // off the async worker threads.
    let path = std::path::PathBuf::from(&file_path);
    let inspection = tokio::task::spawn_blocking(move || {
        crate::services::instrument_detector::inspect_pdf(&path)
    })
    .await
    .map_err(|e| ApiError::internal(format!("inspect task failed: {e}")))?
    .map_err(|e| {
        ApiError::new(
            axum::http::StatusCode::UNPROCESSABLE_ENTITY,
            "unreadable",
            e,
        )
    })?;

    Ok(Json(InspectResponse {
        source_id: id,
        file_name: file_name.unwrap_or_else(|| "document".into()),
        is_container: inspection.is_container(),
        instrument_count: inspection.instrument_count(),
        inspection,
    }))
}

#[derive(Debug, Serialize)]
pub struct InspectResponse {
    pub source_id: String,
    pub file_name: String,
    /// True when the file == instrument assumption is false for this document.
    pub is_container: bool,
    pub instrument_count: usize,
    #[serde(flatten)]
    pub inspection: crate::services::instrument_detector::Inspection,
}

#[derive(Debug, Deserialize)]
pub struct SplitRequest {
    /// The confirmed instruments, in page order. Not derived here — a human has
    /// checked these against the document.
    pub pieces: Vec<crate::services::instrument_split::PlannedPiece>,
}

#[derive(Debug, Serialize)]
pub struct SplitResponse {
    pub created: Vec<SplitCreated>,
    /// Pages covered by no instrument (front matter, gaps). Reported rather than
    /// refused — dropping them is legitimate, doing so unknowingly is not.
    pub dropped_pages: Vec<[u32; 2]>,
    pub original_retired_to: String,
}

#[derive(Debug, Serialize)]
pub struct SplitCreated {
    pub source_id: String,
    pub title: String,
    pub file_name: String,
    pub page_offset: i64,
}

/// POST /api/admin/sources/:id/split — cut a compendium into one source per
/// legal instrument and index each.
///
/// The plan comes from the operator, who has reviewed it against the document;
/// this endpoint validates and carries it out. Validation refuses overlaps and
/// out-of-range pages outright, because both fail silently: an overlap files one
/// provision under two instruments, and neither shows up anywhere downstream.
///
/// The original source ROW is deleted — leaving it indexed would defeat the
/// split, since its chunks keep competing in retrieval and "Rule 4" stays
/// ambiguous. The original FILE is moved to `_split_originals/` rather than
/// deleted: it is the only record of what was cut, and leaving it in the indexed
/// folder would let the next scan re-create the container.
pub async fn split_source(
    State(st): State<AppState>,
    admin: AdminUser,
    Path(id): Path<String>,
    Json(req): Json<SplitRequest>,
) -> ApiResult<Json<SplitResponse>> {
    let row: Option<(String, Option<String>)> =
        sqlx::query_as("SELECT file_path, file_name FROM sources WHERE id = $1")
            .bind(&id)
            .fetch_optional(&st.pool)
            .await
            .map_err(|e| ApiError::internal(e.to_string()))?;
    let Some((file_path, file_name)) = row else {
        return Err(ApiError::new(
            axum::http::StatusCode::NOT_FOUND,
            "not_found",
            "unknown source",
        ));
    };
    let original_name = file_name.unwrap_or_else(|| "document.pdf".into());
    let src = std::path::PathBuf::from(&file_path);
    let dir = src
        .parent()
        .ok_or_else(|| ApiError::internal("source file has no parent directory"))?
        .to_path_buf();

    let bad = |e: String| ApiError::new(axum::http::StatusCode::BAD_REQUEST, "invalid_plan", e);

    // Cutting reads and rewrites the whole PDF once per piece — blocking work.
    let pieces = req.pieces.clone();
    let (cut, dropped) = {
        let src = src.clone();
        let dir = dir.clone();
        tokio::task::spawn_blocking(move || {
            let total = crate::services::instrument_split::page_count(&src)?;
            crate::services::instrument_split::validate(&pieces, total)?;
            let gaps = crate::services::instrument_split::unassigned_pages(&pieces, total);
            crate::services::instrument_split::cut(&src, &pieces, &dir).map(|c| (c, gaps))
        })
        .await
        .map_err(|e| ApiError::internal(format!("split task failed: {e}")))?
        .map_err(bad)?
    };

    // Index the pieces through the same path an upload takes, so they are
    // ordinary documents from here on.
    let paths: Vec<String> = cut.iter().map(|c| c.path.to_string_lossy().to_string()).collect();
    let dir_str = dir.to_string_lossy().to_string();
    let notebook = match crate::db::repositories::folders::get_by_path(&st.pool, &dir_str)
        .await
        .map_err(|e| ApiError::internal(e.to_string()))?
    {
        Some(f) => format!("__folder__{}", f.id),
        None => "__folder__unknown".to_string(),
    };
    let created = crate::services::indexing::index_sources(
        &st.pool, &st.embedder, &st.tier, &notebook, &paths, None, None, None,
    )
    .await
    .map_err(|e| ApiError::internal(format!("indexing the split pieces failed: {e}")))?;

    // Name each piece the way it will be cited, and record where it came from.
    //
    // Resolved by FILE PATH, not by the returned DTO: for a file it has not seen
    // before `index_sources` hands back `id: String::new()` and leaves the row to
    // the indexer it spawns, so updating by that id would silently write nothing.
    // `sources.file_path` is UNIQUE, and `index_file` inserts the row before it
    // takes the indexing semaphore, so the row appears promptly — but not
    // instantly, hence the bounded wait.
    let _ = &created;
    let mut out = Vec::new();
    for piece in cut.iter() {
        let path_str = piece.path.to_string_lossy().to_string();
        let mut source_id = None;
        for _ in 0..40 {
            match crate::db::repositories::sources::get_by_path(&st.pool, &path_str).await {
                Ok(Some(s)) => {
                    source_id = Some(s.id);
                    break;
                }
                Ok(None) => tokio::time::sleep(std::time::Duration::from_millis(250)).await,
                Err(e) => return Err(ApiError::internal(e.to_string())),
            }
        }
        let Some(source_id) = source_id else {
            // The piece is on disk and indexing; only its citation name is
            // missing. Losing the whole split over that would be worse.
            log::warn!(
                "split: {path_str} did not appear in `sources` within 10s — \
                 display name and page offset not recorded"
            );
            continue;
        };
        sqlx::query(
            "UPDATE sources SET display_name = $1, split_page_offset = $2, split_from_file = $3 \
             WHERE id = $4",
        )
        .bind(&piece.title)
        .bind(piece.page_offset)
        .bind(&original_name)
        .bind(&source_id)
        .execute(&st.pool)
        .await
        .map_err(|e| ApiError::internal(e.to_string()))?;
        out.push(SplitCreated {
            source_id,
            title: piece.title.clone(),
            file_name: piece
                .path
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_default(),
            page_offset: piece.page_offset,
        });
    }

    // Retire the container: the row goes (migration 018 makes the cascade
    // reach the graph), the file is moved aside.
    let retired = tokio::task::spawn_blocking(move || {
        crate::services::instrument_split::retire_original(&src)
    })
    .await
    .map_err(|e| ApiError::internal(format!("retire task failed: {e}")))?
    .map_err(ApiError::internal)?;

    sqlx::query("DELETE FROM sources WHERE id = $1")
        .bind(&id)
        .execute(&st.pool)
        .await
        .map_err(|e| ApiError::internal(e.to_string()))?;

    let _ = crate::db::repositories::audit::insert_user_event(
        &st.pool,
        Some(&admin.0.id),
        "admin_split_source",
        Some(&original_name),
        Some(&format!("{} instruments", out.len())),
    )
    .await;

    Ok(Json(SplitResponse {
        created: out,
        dropped_pages: dropped.into_iter().map(|(a, b)| [a, b]).collect(),
        original_retired_to: retired.to_string_lossy().to_string(),
    }))
}

// ── Corpus organisation (migration 021) ─────────────────────────────────────
//
// The corpus was one flat, unlabelled list. These routes let the admin group it
// into categories ("Finance", "Local Govt") and withdraw individual documents
// from service. Both are LABEL-ONLY: no file is moved, nothing is re-chunked,
// and no re-index is triggered. A disable takes effect on the next turn because
// `visible_source_ids` is recomputed per request.

use crate::db::repositories::corpus_category;

/// GET /api/admin/categories — every category, in display order, with counts.
pub async fn list_categories(
    State(st): State<AppState>,
    _admin: AdminUser,
) -> ApiResult<Json<Vec<corpus_category::CorpusCategory>>> {
    let rows = corpus_category::list(&st.pool)
        .await
        .map_err(|e| ApiError::internal(e.to_string()))?;
    Ok(Json(rows))
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CategoryReq {
    pub name: String,
    #[serde(default)]
    pub sort_order: i32,
}

/// POST /api/admin/categories — create one.
///
/// The unique-name constraint is reported as a 400 rather than a 500: "Finance
/// already exists" is an answer the admin can act on, an internal error is not.
pub async fn create_category(
    State(st): State<AppState>,
    _admin: AdminUser,
    Json(req): Json<CategoryReq>,
) -> ApiResult<Json<corpus_category::CorpusCategory>> {
    let name = req.name.trim();
    if name.is_empty() {
        return Err(ApiError::bad_request("category name is required"));
    }
    match corpus_category::create(&st.pool, name, req.sort_order).await {
        Ok(c) => Ok(Json(c)),
        Err(e) if e.to_string().contains("duplicate key") => {
            Err(ApiError::bad_request(format!("a category named \"{name}\" already exists")))
        }
        Err(e) => Err(ApiError::internal(e.to_string())),
    }
}

/// POST /api/admin/categories/:id — rename and/or reorder.
pub async fn update_category(
    State(st): State<AppState>,
    _admin: AdminUser,
    Path(id): Path<String>,
    Json(req): Json<CategoryReq>,
) -> ApiResult<Json<serde_json::Value>> {
    let name = req.name.trim();
    if name.is_empty() {
        return Err(ApiError::bad_request("category name is required"));
    }
    let found = corpus_category::update(&st.pool, &id, name, req.sort_order)
        .await
        .map_err(|e| ApiError::internal(e.to_string()))?;
    if !found {
        return Err(ApiError::bad_request("no such category"));
    }
    Ok(Json(serde_json::json!({ "updated": true })))
}

/// POST /api/admin/categories/:id/delete — remove the category, keep its documents.
///
/// `sources.category_id` is ON DELETE SET NULL, so the documents fall back to
/// Uncategorised. The count of released documents is returned so the UI can say
/// what happened instead of implying the documents went with it.
pub async fn delete_category(
    State(st): State<AppState>,
    _admin: AdminUser,
    Path(id): Path<String>,
) -> ApiResult<Json<serde_json::Value>> {
    let released = corpus_category::delete(&st.pool, &id)
        .await
        .map_err(|e| ApiError::internal(e.to_string()))?;
    Ok(Json(serde_json::json!({ "released": released })))
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AssignReq {
    pub source_ids: Vec<String>,
    /// `None`/absent files the documents back under Uncategorised.
    #[serde(default)]
    pub category_id: Option<String>,
}

/// POST /api/admin/sources/categorize — file documents into a category.
pub async fn categorize_sources(
    State(st): State<AppState>,
    _admin: AdminUser,
    Json(req): Json<AssignReq>,
) -> ApiResult<Json<serde_json::Value>> {
    let ids: Vec<String> = req
        .source_ids
        .into_iter()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect();
    if ids.is_empty() {
        return Err(ApiError::bad_request("no source_ids given"));
    }
    let moved = corpus_category::assign(&st.pool, &ids, req.category_id.as_deref())
        .await
        .map_err(|e| ApiError::internal(e.to_string()))?;
    Ok(Json(serde_json::json!({ "moved": moved })))
}

#[derive(Deserialize)]
pub struct DisableReq {
    pub disabled: bool,
}

/// POST /api/admin/sources/:id/disabled — withdraw a document from service, or
/// return it.
///
/// A disabled document stays in the corpus listing (greyed, marked unavailable)
/// but `visible_source_ids` excludes it, so it can never be retrieved or cited.
/// Reversible and immediate — no re-index either way.
pub async fn set_source_disabled(
    State(st): State<AppState>,
    _admin: AdminUser,
    Path(id): Path<String>,
    Json(req): Json<DisableReq>,
) -> ApiResult<Json<serde_json::Value>> {
    let found = corpus_category::set_disabled(&st.pool, &id, req.disabled)
        .await
        .map_err(|e| ApiError::internal(e.to_string()))?;
    if !found {
        return Err(ApiError::bad_request("no such document"));
    }
    Ok(Json(serde_json::json!({ "disabled": req.disabled })))
}
