//! Document + knowledge-graph read endpoints (US1, T013).
//!
//! Command-named POST routes so `transport.call("list_indexed_folders", …)`
//! maps 1:1 (args are camelCase, matching the Tauri invoke boundary). These are
//! AppHandle-free read paths over the shared pool — they reuse the same
//! repository/service functions the desktop commands call (one implementation,
//! two transports, FR-001). The indexing **write** path (add_sources /
//! add_indexed_folder) stays server-side admin work and is out of scope here.

use axum::extract::{Path, State};
use axum::Json;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::auth::AuthUser;
use super::{ApiError, ApiResult, AppState};
use crate::services::indexing::{index_folder, index_sources, EntityDto, FolderDto, SourceDto};
use crate::db::repositories::{folders as folder_repo, sources as source_repo};
use crate::services::knowledge_graph;

fn err(e: impl std::fmt::Display) -> ApiError {
    ApiError::internal(e.to_string())
}

/// Record a document-ACCESS event (2026-08-13).
///
/// Mutations were already audited; reads were not, so "who opened this file?"
/// had no answer. The call sites sit immediately after the `visible_source_ids`
/// decision — the one place in the codebase where "this user may see this
/// document" is settled — so a new read route cannot quietly skip the trail
/// without also skipping the permission check.
///
/// Best-effort, exactly like every other audit write here: a failed insert is
/// logged and swallowed rather than failing an officer's read. That is a
/// deliberate trade — losing one row is preferable to a database hiccup making
/// documents unreadable. Pass `""` for `target` when the event is not about one
/// specific document.
async fn audit_access(st: &AppState, user: &AuthUser, action: &str, target: &str, details: &str) {
    let target = if target.is_empty() { None } else { Some(target) };
    if let Err(e) = crate::db::repositories::audit::insert_user_event(
        &st.pool,
        Some(&user.id),
        action,
        target,
        Some(details),
    )
    .await
    {
        log::warn!("audit_access({action}) failed to record: {e}");
    }
}

// ── Indexing triggers (US4/T024 — progress streams over /api/index/events) ────

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AddSourcesReq {
    pub notebook_id: String,
    pub file_paths: Vec<String>,
}

pub async fn add_sources(
    State(st): State<AppState>,
    user: AuthUser,
    Json(req): Json<AddSourcesReq>,
) -> ApiResult<Json<Vec<SourceDto>>> {
    let _ = crate::db::repositories::audit::insert_user_event(
        &st.pool, Some(&user.id), "index", Some(&req.notebook_id), None,
    )
    .await;
    // Headless: app = None — per-file progress flows to SSE, not Tauri events.
    let out = index_sources(
        &st.pool, &st.embedder, &st.tier, &req.notebook_id, &req.file_paths, None, None, None,
    )
    .await
    .map_err(err)?;
    Ok(Json(out))
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AddFolderReq {
    pub folder_path: String,
    /// File every document from this scan into a category as it finishes.
    ///
    /// ⚠️ `rename_all = "camelCase"` + `Option<T>` FAILS OPEN: a client sending
    /// `category_id` would deserialize to `None` and silently file the whole
    /// batch as Uncategorised instead of erroring. The `alias` accepts both
    /// spellings so that cannot happen — same defect shape as the F027 scope
    /// fields (ADR-0029).
    #[serde(default, alias = "category_id")]
    pub category_id: Option<String>,
}

pub async fn add_indexed_folder(
    State(st): State<AppState>,
    user: AuthUser,
    Json(req): Json<AddFolderReq>,
) -> ApiResult<Json<FolderDto>> {
    let _ = crate::db::repositories::audit::insert_user_event(
        &st.pool, Some(&user.id), "index", Some(&req.folder_path), None,
    )
    .await;
    // Admin-indexed folders are shared with everyone (owner_id NULL = global);
    // a regular user's folder stays private to them.
    let owner = if user.role == "admin" { None } else { Some(user.id.clone()) };
    let dto = index_folder(
        &st.pool,
        &st.embedder,
        &st.tier,
        &req.folder_path,
        owner,
        None,
        req.category_id.as_deref(),
    )
    .await
    .map_err(err)?;
    Ok(Json(dto))
}

// ── Folders / sources ─────────────────────────────────────────────────────────

/// True if `user` may view/manage `folder_id` — its owner, or any admin.
async fn can_manage_folder(st: &AppState, user: &AuthUser, folder_id: &str) -> Result<bool, ApiError> {
    if user.role == "admin" {
        return Ok(true);
    }
    let owner = folder_repo::owner_of(&st.pool, folder_id).await.map_err(err)?;
    Ok(owner.as_deref() == Some(user.id.as_str()))
}

fn forbidden() -> ApiError {
    ApiError::new(
        axum::http::StatusCode::FORBIDDEN,
        "forbidden",
        "you don't have access to this folder",
    )
}

pub async fn list_indexed_folders(
    State(st): State<AppState>,
    user: AuthUser,
) -> ApiResult<Json<Vec<FolderDto>>> {
    // Every viewer — admin included — sees their own folders plus anything
    // shared. A user's private documents are private from admin lists too;
    // per-user storage totals remain visible via /admin/usage.
    let rows = folder_repo::list_for_owner(&st.pool, &user.id).await.map_err(err)?;
    Ok(Json(
        rows.into_iter()
            // F027 — a project's own upload folder is not part of the general
            // document tree. Its SOURCES are already hidden by `project_id`
            // (migration 024); without this the folder would still be listed,
            // and an empty folder that cannot be explained is worse than no
            // folder at all. Settings still reaches these files through
            // `list_folder_sources`, which is deliberately NOT filtered — a
            // project upload must stay visible where documents are managed and
            // deleted, or it becomes unreachable content that still bills
            // against the officer's storage allowance.
            .filter(|f| !f.folder_path.contains("__project__"))
            .map(Into::into)
            .collect::<Vec<FolderDto>>(),
    ))
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FolderIdReq {
    pub folder_id: String,
}

pub async fn list_sources_in_folder(
    State(st): State<AppState>,
    user: AuthUser,
    Json(req): Json<FolderIdReq>,
) -> ApiResult<Json<Vec<SourceDto>>> {
    let folder_path: Option<String> =
        sqlx::query_scalar("SELECT folder_path FROM indexed_folders WHERE id = $1")
            .bind(&req.folder_id)
            .fetch_optional(&st.pool)
            .await
            .map_err(err)?;
    let Some(folder_path) = folder_path else {
        return Ok(Json(vec![]));
    };
    let prefix = format!("{folder_path}%");
    // F021 (FR-017): conversation attachments live under the owner's uploads
    // path but must never surface in folder listings. Rows are also filtered
    // to what THIS viewer may retrieve from (own + shared) — admin included —
    // so private file names never leak through folder-id enumeration.
    // Migration 024: a document UPLOADED INTO a project is hidden for the same
    // reason and by the same shape of test. ⚠️ `project_id`, never a membership
    // check — a corpus document ADDED to a project must keep appearing here.
    let rows = sqlx::query_as::<_, source_repo::Source>(
        // Newest first, matching `list_folder_sources` — see the note there.
        "SELECT * FROM sources WHERE file_path LIKE $1 AND indexing_status = 'indexed' \
         AND conversation_id IS NULL AND project_id IS NULL \
         AND (visibility = 'shared' OR owner_id = $2) \
         ORDER BY created_at DESC, file_name ASC",
    )
    .bind(&prefix)
    .bind(&user.id)
    .fetch_all(&st.pool)
    .await
    .map_err(err)?;
    Ok(Json(rows.into_iter().map(Into::into).collect()))
}

/// `revoke_indexed_folder {folderId}` — delete a folder index (owner/admin only).
pub async fn revoke_indexed_folder(
    State(st): State<AppState>,
    user: AuthUser,
    Json(req): Json<FolderIdReq>,
) -> ApiResult<Json<serde_json::Value>> {
    if !can_manage_folder(&st, &user, &req.folder_id).await? {
        return Err(forbidden());
    }
    let counts = folder_repo::delete(&st.pool, &req.folder_id).await.map_err(err)?;
    let _ = crate::db::repositories::audit::insert_user_event(
        &st.pool,
        Some(&user.id),
        "folder_revoked",
        Some(&req.folder_id),
        // Record what was actually removed. A revoke that matched 0 sources used
        // to be indistinguishable from a clean one in the audit trail.
        Some(&format!(
            "sources={} chunks={} embeddings={}",
            counts.sources, counts.chunks, counts.embeddings
        )),
    )
    .await;
    Ok(Json(serde_json::json!({
        "ok": true,
        "sources": counts.sources,
        "chunks": counts.chunks,
        "embeddings": counts.embeddings,
        "folderFound": counts.folder_found,
    })))
}

/// `folder_delete_preview {folderId}` — how much a delete would remove.
///
/// Read-only. Backs the confirmation dialog so the warning quotes real numbers
/// rather than a generic "are you sure": deleting an indexed folder destroys
/// every chunk and embedding derived from it, and that is not recoverable
/// without a full re-index.
pub async fn folder_delete_preview(
    State(st): State<AppState>,
    user: AuthUser,
    Json(req): Json<FolderIdReq>,
) -> ApiResult<Json<serde_json::Value>> {
    if !can_manage_folder(&st, &user, &req.folder_id).await? {
        return Err(forbidden());
    }
    let counts = folder_repo::delete_preview(&st.pool, &req.folder_id).await.map_err(err)?;
    Ok(Json(serde_json::json!({
        "sources": counts.sources,
        "chunks": counts.chunks,
        "embeddings": counts.embeddings,
        "folderFound": counts.folder_found,
    })))
}

/// `GET /api/categories` — the corpus categories, for ANY signed-in user.
///
/// The chat sidebar groups the document picker by category, so an officer needs
/// the names; `/admin/categories` is admin-gated and would 403 them. Names and
/// display order only — see `corpus_category::list_names` for why the counts are
/// deliberately not published here.
pub async fn list_categories(
    State(st): State<AppState>,
    _user: AuthUser,
) -> ApiResult<Json<Vec<crate::db::repositories::corpus_category::CorpusCategory>>> {
    let rows = crate::db::repositories::corpus_category::list_names(&st.pool)
        .await
        .map_err(err)?;
    Ok(Json(rows))
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FolderPathReq {
    pub folder_path: String,
}

/// `list_folder_sources {folderPath}` — per-file rows inside one folder
/// (owner/admin only). Mirrors the desktop command of the same name.
pub async fn list_folder_sources(
    State(st): State<AppState>,
    user: AuthUser,
    Json(req): Json<FolderPathReq>,
) -> ApiResult<Json<Vec<SourceDto>>> {
    let folder = folder_repo::get_by_path(&st.pool, &req.folder_path).await.map_err(err)?;
    let Some(folder) = folder else { return Ok(Json(vec![])) };
    if !can_manage_folder(&st, &user, &folder.id).await? {
        return Err(forbidden());
    }
    let prefix = format!("{}/%", req.folder_path.trim_end_matches('/'));
    // F021 (FR-017): exclude conversation attachments from folder listings.
    // Visibility-filtered like list_sources_in_folder: owners still see all
    // of their own files, but an admin managing someone's folder sees only
    // the shared ones (private stays private from admin).
    let rows = sqlx::query_as::<_, source_repo::Source>(
        // Newest first: a document you just added is the one you want to watch
        // index and inspect. Alphabetical order buried a new upload in the middle
        // of the list, where it had to be hunted for. `file_name` breaks ties so
        // the order stays stable for documents added in the same second.
        "SELECT * FROM sources WHERE file_path LIKE $1 AND conversation_id IS NULL \
         AND (visibility = 'shared' OR owner_id = $2) \
         ORDER BY created_at DESC, file_name ASC",
    )
    .bind(&prefix)
    .bind(&user.id)
    .fetch_all(&st.pool)
    .await
    .map_err(err)?;
    Ok(Json(rows.into_iter().map(Into::into).collect()))
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FolderSourceReq {
    pub source_id: String,
    pub folder_id: String,
}

/// Re-run indexing for one file in a folder. `tier` optionally moves it to a
/// different index tier first: `Some("deep")` promotes to the LLM-IR pipeline,
/// `Some("fast")` reverts to standard indexing, `None` keeps the current tier
/// (plain retry). Shared by `retry_folder_source`, `deep_index_folder_source`
/// and `undeep_index_folder_source`.
async fn reindex_source(
    st: &AppState,
    user: &AuthUser,
    req: &FolderSourceReq,
    tier: Option<&str>,
) -> Result<(), ApiError> {
    if !can_manage_folder(st, user, &req.folder_id).await? {
        return Err(forbidden());
    }
    match tier {
        Some(t) => {
            sqlx::query("UPDATE sources SET index_tier = $1, indexing_status = 'pending' WHERE id = $2")
                .bind(t)
                .bind(&req.source_id)
                .execute(&st.pool)
                .await
                .map_err(err)?;
        }
        None => {
            source_repo::update_status(&st.pool, &req.source_id, "pending").await.map_err(err)?;
        }
    }
    let source = source_repo::get_by_id(&st.pool, &req.source_id)
        .await
        .map_err(err)?
        .ok_or_else(|| ApiError::new(axum::http::StatusCode::NOT_FOUND, "not_found", "source not found"))?;

    let pool = st.pool.clone();
    let embedder = std::sync::Arc::clone(&st.embedder);
    let tier = st.tier.clone();
    let notebook_id = format!("__folder__{}", req.folder_id);
    let path = source.file_path.clone();
    tokio::spawn(async move {
        let budget = crate::services::task_budget::TaskBudget::new();
        if let Err(e) = crate::services::indexer::index_file(
            &pool, embedder, &budget, &tier, &notebook_id, &path, None,
        )
        .await
        {
            log::error!("Folder source reindex failed for {path}: {e}");
        }
    });
    Ok(())
}

pub async fn retry_folder_source(
    State(st): State<AppState>,
    user: AuthUser,
    Json(req): Json<FolderSourceReq>,
) -> ApiResult<Json<serde_json::Value>> {
    reindex_source(&st, &user, &req, None).await?;
    Ok(Json(serde_json::json!({ "ok": true })))
}

pub async fn deep_index_folder_source(
    State(st): State<AppState>,
    user: AuthUser,
    Json(req): Json<FolderSourceReq>,
) -> ApiResult<Json<serde_json::Value>> {
    reindex_source(&st, &user, &req, Some("deep")).await?;
    Ok(Json(serde_json::json!({ "ok": true })))
}

/// `undeep_index_folder_source {sourceId, folderId}` — revert a deep-indexed
/// file back to the standard (`fast`) tier and re-index it, replacing the
/// LLM-IR chunks with the deterministic pipeline's output (owner/admin only).
pub async fn undeep_index_folder_source(
    State(st): State<AppState>,
    user: AuthUser,
    Json(req): Json<FolderSourceReq>,
) -> ApiResult<Json<serde_json::Value>> {
    reindex_source(&st, &user, &req, Some("fast")).await?;
    Ok(Json(serde_json::json!({ "ok": true })))
}

/// `remove_folder_source {sourceId, folderId}` — wipe a file's chunks/embeddings
/// and mark it unindexed (owner/admin only).
pub async fn remove_folder_source(
    State(st): State<AppState>,
    user: AuthUser,
    Json(req): Json<FolderSourceReq>,
) -> ApiResult<Json<serde_json::Value>> {
    if !can_manage_folder(&st, &user, &req.folder_id).await? {
        return Err(forbidden());
    }
    let mut tx = st.pool.begin().await.map_err(err)?;
    sqlx::query("DELETE FROM chunk_embeddings WHERE chunk_id IN (SELECT id FROM chunks WHERE source_id = $1)")
        .bind(&req.source_id).execute(&mut *tx).await.map_err(err)?;
    sqlx::query("DELETE FROM graph_edges WHERE from_node IN (SELECT id FROM graph_nodes WHERE chunk_id IN (SELECT id FROM chunks WHERE source_id = $1))")
        .bind(&req.source_id).execute(&mut *tx).await.map_err(err)?;
    sqlx::query("DELETE FROM graph_edges WHERE to_node IN (SELECT id FROM graph_nodes WHERE chunk_id IN (SELECT id FROM chunks WHERE source_id = $1))")
        .bind(&req.source_id).execute(&mut *tx).await.map_err(err)?;
    sqlx::query("DELETE FROM graph_nodes WHERE chunk_id IN (SELECT id FROM chunks WHERE source_id = $1)")
        .bind(&req.source_id).execute(&mut *tx).await.map_err(err)?;
    sqlx::query("DELETE FROM chunks WHERE source_id = $1")
        .bind(&req.source_id).execute(&mut *tx).await.map_err(err)?;
    sqlx::query("UPDATE sources SET indexing_status = 'unindexed', error_reason = NULL, indexed_at = NULL WHERE id = $1")
        .bind(&req.source_id).execute(&mut *tx).await.map_err(err)?;
    tx.commit().await.map_err(err)?;
    Ok(Json(serde_json::json!({ "ok": true })))
}

/// `delete_folder_source {sourceId, folderId}` — PERMANENTLY delete a file:
/// wipe its chunks/embeddings/graph, drop the `sources` row (and notebook
/// links), and remove the underlying file from disk (owner/admin only). Unlike
/// `remove_folder_source` this is irreversible — the file must be re-uploaded
/// to index it again.
pub async fn delete_folder_source(
    State(st): State<AppState>,
    user: AuthUser,
    Json(req): Json<FolderSourceReq>,
) -> ApiResult<Json<serde_json::Value>> {
    if !can_manage_folder(&st, &user, &req.folder_id).await? {
        return Err(forbidden());
    }
    // Capture the on-disk path before the row is gone.
    let file_path = source_repo::get_by_id(&st.pool, &req.source_id)
        .await
        .map_err(err)?
        .map(|s| s.file_path);

    let mut tx = st.pool.begin().await.map_err(err)?;
    sqlx::query("DELETE FROM chunk_embeddings WHERE chunk_id IN (SELECT id FROM chunks WHERE source_id = $1)")
        .bind(&req.source_id).execute(&mut *tx).await.map_err(err)?;
    sqlx::query("DELETE FROM graph_edges WHERE from_node IN (SELECT id FROM graph_nodes WHERE chunk_id IN (SELECT id FROM chunks WHERE source_id = $1))")
        .bind(&req.source_id).execute(&mut *tx).await.map_err(err)?;
    sqlx::query("DELETE FROM graph_edges WHERE to_node IN (SELECT id FROM graph_nodes WHERE chunk_id IN (SELECT id FROM chunks WHERE source_id = $1))")
        .bind(&req.source_id).execute(&mut *tx).await.map_err(err)?;
    sqlx::query("DELETE FROM graph_nodes WHERE chunk_id IN (SELECT id FROM chunks WHERE source_id = $1)")
        .bind(&req.source_id).execute(&mut *tx).await.map_err(err)?;
    sqlx::query("DELETE FROM chunks WHERE source_id = $1")
        .bind(&req.source_id).execute(&mut *tx).await.map_err(err)?;
    sqlx::query("DELETE FROM notebook_sources WHERE source_id = $1")
        .bind(&req.source_id).execute(&mut *tx).await.map_err(err)?;
    sqlx::query("DELETE FROM sources WHERE id = $1")
        .bind(&req.source_id).execute(&mut *tx).await.map_err(err)?;
    tx.commit().await.map_err(err)?;

    // Best-effort file removal — the DB row is already gone, so a missing or
    // permission-denied file must not fail the request.
    if let Some(path) = file_path {
        if let Err(e) = std::fs::remove_file(&path) {
            log::warn!("delete_folder_source: DB row removed but file {path} not deleted: {e}");
        }
    }
    Ok(Json(serde_json::json!({ "ok": true })))
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NotebookIdReq {
    pub notebook_id: String,
}

pub async fn list_notebook_sources(
    State(st): State<AppState>,
    _user: AuthUser,
    Json(req): Json<NotebookIdReq>,
) -> ApiResult<Json<Vec<SourceDto>>> {
    let rows = source_repo::list_by_notebook(&st.pool, &req.notebook_id).await.map_err(err)?;
    Ok(Json(rows.into_iter().map(Into::into).collect()))
}

// ── Entities / knowledge graph ────────────────────────────────────────────────

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct QueryEntitiesReq {
    pub query: String,
    pub entity_type: Option<String>,
}

/// F021 — of the given entity ids, keep only those with at least one mention in
/// a source the requesting user may see (shared corpus or their own documents;
/// attachments are excluded from entity browsing). Same rule for admins (their
/// ANSWER scope is global + own — management surfaces are separate).
async fn retain_visible_entities(
    st: &AppState,
    user: &AuthUser,
    entity_ids: &[String],
) -> Result<std::collections::HashSet<String>, ApiError> {
    if entity_ids.is_empty() {
        return Ok(Default::default());
    }
    let placeholders = crate::db::pg_in(entity_ids.len(), 2);
    let sql = format!(
        "SELECT DISTINCT em.entity_id FROM entity_mentions em \
         JOIN chunks c ON c.id = em.chunk_id \
         JOIN sources s ON s.id = c.source_id \
         WHERE ((s.visibility = 'shared' AND s.conversation_id IS NULL) OR s.owner_id = $1) \
           AND em.entity_id IN ({placeholders})",
    );
    let mut q = sqlx::query_scalar::<_, String>(&sql).bind(&user.id);
    for id in entity_ids {
        q = q.bind(id);
    }
    Ok(q.fetch_all(&st.pool).await.map_err(err)?.into_iter().collect())
}

pub async fn query_entities(
    State(st): State<AppState>,
    user: AuthUser,
    Json(req): Json<QueryEntitiesReq>,
) -> ApiResult<Json<Vec<EntityDto>>> {
    let rows = knowledge_graph::query_entities(&st.pool, &req.query, req.entity_type.as_deref())
        .await
        .map_err(err)?;
    // F021: drop entities mentioned only in sources this user can't see.
    let ids: Vec<String> = rows.iter().map(|e| e.id.clone()).collect();
    let visible = retain_visible_entities(&st, &user, &ids).await?;
    Ok(Json(
        rows.into_iter()
            .filter(|e| visible.contains(&e.id))
            .map(Into::into)
            .collect(),
    ))
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EntityIdReq {
    pub entity_id: String,
}

pub async fn get_entity_relationships(
    State(st): State<AppState>,
    user: AuthUser,
    Json(req): Json<EntityIdReq>,
) -> ApiResult<Json<Vec<knowledge_graph::Relationship>>> {
    let rows = knowledge_graph::get_relationships(&st.pool, &req.entity_id).await.map_err(err)?;
    // F021: keep only relationships whose BOTH endpoints are visible entities.
    let mut ids: Vec<String> = Vec::new();
    for r in &rows {
        ids.push(r.from_entity_id.clone());
        ids.push(r.to_entity_id.clone());
    }
    ids.sort();
    ids.dedup();
    let visible = retain_visible_entities(&st, &user, &ids).await?;
    Ok(Json(
        rows.into_iter()
            .filter(|r| visible.contains(&r.from_entity_id) && visible.contains(&r.to_entity_id))
            .collect(),
    ))
}

pub async fn get_entity_sources(
    State(st): State<AppState>,
    user: AuthUser,
    Json(req): Json<EntityIdReq>,
) -> ApiResult<Json<Vec<Value>>> {
    // F021: mention excerpts are answer-grade content — visible sources only.
    let rows: Vec<(String, String, String, Option<String>)> = sqlx::query_as(
        r#"
        SELECT s.id, s.file_name, c.content, em.context_text
        FROM entity_mentions em
        JOIN chunks c ON c.id = em.chunk_id
        JOIN sources s ON s.id = c.source_id
        WHERE em.entity_id = $1
          AND ((s.visibility = 'shared' AND s.conversation_id IS NULL) OR s.owner_id = $2)
        LIMIT 20
        "#,
    )
    .bind(&req.entity_id)
    .bind(&user.id)
    .fetch_all(&st.pool)
    .await
    .map_err(err)?;

    let out = rows
        .into_iter()
        .map(|(sid, fname, content, ctx)| {
            let excerpt: String = content.chars().take(200).collect();
            serde_json::json!({
                "source_id": sid,
                "file_name": fname,
                "excerpt": excerpt,
                "context": ctx,
            })
        })
        .collect();
    Ok(Json(out))
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct KnowledgeGraphReq {
    pub limit: Option<i64>,
    pub offset: Option<i64>,
    pub source_ids: Option<Vec<String>>,
    /// F027 — a project's subgraph is that project's documents (Constitution
    /// XIII: the graph is a retrieval surface, so it obeys the same scope).
    // ⚠️ These request structs are `rename_all = "camelCase"`, so the wire key
    // is `projectId`. The alias accepts `project_id` as well, because
    // contracts/api.md documents the snake_case form and a mismatch here fails
    // SILENTLY in the worst possible direction: serde yields `None`, the scope
    // falls back to unscoped, and the route answers 200 with the caller's whole
    // visible corpus instead of refusing. Same alias pattern `QueryReq` already
    // uses for `conversationId`.
    #[serde(default, alias = "project_id")]
    pub project_id: Option<String>,
}

pub async fn get_knowledge_graph(
    State(st): State<AppState>,
    user: AuthUser,
    Json(req): Json<KnowledgeGraphReq>,
) -> ApiResult<Json<Value>> {
    // F021: clamp the subgraph to sources this user may see. A requested
    // source_ids filter is intersected with the visible set; no filter means
    // "everything visible to me". Empty intersection returns an empty graph
    // (get_subgraph treats Some([]) as "no filter", which would fail open).
    let scope =
        crate::api::projects::scope_for(&st, &user.id, req.project_id.as_deref(), false).await?;
    let visible = source_repo::visible_source_ids(&st.pool, &scope).await.map_err(err)?;
    let effective: Vec<String> = match &req.source_ids {
        Some(requested) => {
            let vis: std::collections::HashSet<&str> =
                visible.iter().map(String::as_str).collect();
            requested.iter().filter(|s| vis.contains(s.as_str())).cloned().collect()
        }
        None => visible,
    };
    if effective.is_empty() {
        return Ok(Json(serde_json::json!({ "nodes": [], "edges": [] })));
    }
    let graph = knowledge_graph::get_subgraph(
        &st.pool,
        req.limit.unwrap_or(500),
        req.offset.unwrap_or(0),
        Some(&effective),
    )
    .await
    .map_err(err)?;
    let nodes: Vec<EntityDto> = graph.nodes.into_iter().map(Into::into).collect();
    Ok(Json(serde_json::json!({ "nodes": nodes, "edges": graph.edges })))
}

// ── F021: visibility, My-documents management, conversation attachments ──────

/// Full FK-safe delete of one source (embeddings → graph → chunks → notebook
/// links → row). Returns the on-disk path for best-effort file removal, or
/// `None` when nothing was deleted.
///
/// ⚠️ **Feature 027 (migration 023) — this now refuses to delete content that
/// anything still references, and that "anything" may belong to ANOTHER USER.**
///
/// Before 023, every reference to a source belonged to the same user, so this
/// function could only ever destroy the caller's own data. After it, a project
/// copied to a colleague holds the *sharer's* `sources` row, and FR-024
/// guarantees that copy keeps working even after the original owner deletes the
/// document. Deleting here regardless would silently destroy someone else's
/// project contents.
///
/// The guard lives INSIDE this function rather than at its four call sites
/// (`admin.rs:494`, `:1054`, `:1293`, and project document removal) precisely so
/// that it cannot be forgotten by the fifth. Callers remove their own reference
/// FIRST and then call this, so `is_referenced` is asking exactly the right
/// question: does anyone *else* still hold it?
///
/// ⚠️ **This used to return `Option<String>`, conflating "retained, still in
/// use" with "deleted, but had no file on disk" (B1, 2026-08-07).** Three of the
/// five call sites mis-counted as a result — `delete_conversation` and
/// `delete_bulk` counted every attempt as a deletion, and project deletion
/// counted only sources that happened to have a file path. The ambiguity was
/// documented as a caveat and then went wrong anyway, so it is now impossible to
/// express: the outcome is a type, and reporting it correctly is the default.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum DeleteOutcome {
    /// Rows were removed. `file_path` is `Some` when there was a file to unlink.
    Deleted { file_path: Option<String> },
    /// Nothing was removed — a project, conversation link or notebook still
    /// references this content, possibly another user's (FR-024).
    Retained,
}

impl DeleteOutcome {
    /// True only when rows were actually removed.
    pub(crate) fn deleted(&self) -> bool {
        matches!(self, DeleteOutcome::Deleted { .. })
    }
    /// The on-disk path to unlink, if this delete removed rows and had a file.
    pub(crate) fn file_path(&self) -> Option<&str> {
        match self {
            DeleteOutcome::Deleted { file_path } => file_path.as_deref(),
            DeleteOutcome::Retained => None,
        }
    }
}

pub(crate) async fn hard_delete_source(st: &AppState, source_id: &str) -> Result<DeleteOutcome, ApiError> {
    if source_repo::is_referenced(&st.pool, source_id).await.map_err(err)? {
        log::info!(
            "hard_delete_source: retaining {source_id} — still referenced by a project, \
             conversation link or notebook (possibly another user's; FR-024)"
        );
        return Ok(DeleteOutcome::Retained);
    }

    let file_path = source_repo::get_by_id(&st.pool, source_id)
        .await
        .map_err(err)?
        .map(|s| s.file_path);

    let mut tx = st.pool.begin().await.map_err(err)?;
    // F027 — memberships for THIS source. `project_source` cascades from
    // `sources`, but deleting explicitly inside the transaction keeps the
    // ordering readable alongside the other reference tables.
    sqlx::query("DELETE FROM project_source WHERE source_id = $1")
        .bind(source_id).execute(&mut *tx).await.map_err(err)?;
    sqlx::query("DELETE FROM chunk_embeddings WHERE chunk_id IN (SELECT id FROM chunks WHERE source_id = $1)")
        .bind(source_id).execute(&mut *tx).await.map_err(err)?;
    sqlx::query("DELETE FROM graph_edges WHERE from_node IN (SELECT id FROM graph_nodes WHERE chunk_id IN (SELECT id FROM chunks WHERE source_id = $1))")
        .bind(source_id).execute(&mut *tx).await.map_err(err)?;
    sqlx::query("DELETE FROM graph_edges WHERE to_node IN (SELECT id FROM graph_nodes WHERE chunk_id IN (SELECT id FROM chunks WHERE source_id = $1))")
        .bind(source_id).execute(&mut *tx).await.map_err(err)?;
    sqlx::query("DELETE FROM graph_nodes WHERE chunk_id IN (SELECT id FROM chunks WHERE source_id = $1)")
        .bind(source_id).execute(&mut *tx).await.map_err(err)?;
    sqlx::query("DELETE FROM chunks WHERE source_id = $1")
        .bind(source_id).execute(&mut *tx).await.map_err(err)?;
    sqlx::query("DELETE FROM notebook_sources WHERE source_id = $1")
        .bind(source_id).execute(&mut *tx).await.map_err(err)?;
    sqlx::query("DELETE FROM sources WHERE id = $1")
        .bind(source_id).execute(&mut *tx).await.map_err(err)?;
    tx.commit().await.map_err(err)?;
    Ok(DeleteOutcome::Deleted { file_path })
}

/// One chunk as the source viewer shows it.
#[derive(Serialize, sqlx::FromRow)]
pub struct SourceChunkRow {
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
#[serde(rename_all = "camelCase")]
pub struct SourceChunksReq {
    pub source_id: String,
    /// Centre the returned window on this chunk (a clicked citation).
    pub around_chunk_id: Option<String>,
    #[serde(default)]
    pub offset: i64,
    pub limit: Option<i64>,
    /// F027 — opening a citation from inside a project scopes to that project,
    /// so a citation link cannot become a way to read a document outside it.
    // ⚠️ These request structs are `rename_all = "camelCase"`, so the wire key
    // is `projectId`. The alias accepts `project_id` as well, because
    // contracts/api.md documents the snake_case form and a mismatch here fails
    // SILENTLY in the worst possible direction: serde yields `None`, the scope
    // falls back to unscoped, and the route answers 200 with the caller's whole
    // visible corpus instead of refusing. Same alias pattern `QueryReq` already
    // uses for `conversationId`.
    #[serde(default, alias = "project_id")]
    pub project_id: Option<String>,
}

#[derive(Serialize)]
pub struct SourceChunksResp {
    pub total: i64,
    pub offset: i64,
    pub chunks: Vec<SourceChunkRow>,
}

/// `GET /api/sources/:id/file` — the ORIGINAL document behind a citation
/// (2026-07-28, tester request: "document download option in citation").
///
/// Scoped exactly like `source_chunks` below — through `visible_source_ids`,
/// so this cannot become a way to read a document the caller may not see.
/// Served **inline** with a real content type so the browser renders a PDF in
/// a new tab (the caller appends `#page=N` to land on the cited page); the
/// viewer's own button still saves it. Unknown types fall back to a download.
pub async fn source_file(
    State(st): State<AppState>,
    user: AuthUser,
    Path(source_id): Path<String>,
    // F027 — this route is a GET with a path param, so the project arrives as
    // `?project_id=…` rather than in a body. Same verification either way.
    axum::extract::Query(q): axum::extract::Query<crate::api::projects::ProjectQuery>,
) -> Result<axum::response::Response, ApiError> {
    use axum::response::IntoResponse;
    let scope =
        crate::api::projects::scope_for(&st, &user.id, q.project_id.as_deref(), false).await?;
    let visible = source_repo::visible_source_ids(&st.pool, &scope).await.map_err(err)?;
    let not_found = || {
        ApiError::new(axum::http::StatusCode::NOT_FOUND, "not_found", "document not found")
    };
    if !visible.iter().any(|id| id == &source_id) {
        // Same response as genuinely absent: whether a document the caller
        // cannot see EXISTS is itself information. The ATTEMPT is recorded even
        // though the response hides the outcome — a refused read is exactly the
        // event an audit is for, and the caller cannot tell it was logged.
        audit_access(&st, &user, "document_denied", &source_id, r#"{"route":"source_file"}"#).await;
        return Err(not_found());
    }

    let row: Option<(String, Option<String>, Option<String>)> = sqlx::query_as(
        "SELECT file_path, file_name, display_name FROM sources WHERE id = $1",
    )
    .bind(&source_id)
    .fetch_optional(&st.pool)
    .await
    .map_err(err)?;
    let (file_path, file_name, display_name) = row.ok_or_else(not_found)?;
    let bytes = tokio::fs::read(&file_path).await.map_err(|e| {
        log::warn!("source_file: {file_path} unreadable: {e}");
        ApiError::new(
            axum::http::StatusCode::NOT_FOUND,
            "file_missing",
            "the original file is no longer on disk",
        )
    })?;

    let lower = file_path.to_lowercase();
    let (mime, inline) = if lower.ends_with(".pdf") {
        ("application/pdf", true)
    } else if lower.ends_with(".txt") || lower.ends_with(".md") {
        ("text/plain; charset=utf-8", true)
    } else {
        ("application/octet-stream", false)
    };
    // Prefer the human title for the saved name; sanitize to ASCII so the
    // header stays valid whatever the source was called.
    let stem = display_name.or(file_name).unwrap_or_else(|| "document".into());
    let ext = std::path::Path::new(&file_path)
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("");
    let mut name: String = stem
        .chars()
        .map(|c| if c.is_ascii_graphic() || c == ' ' { c } else { '_' })
        .collect::<String>()
        .replace('"', "_");
    if !ext.is_empty() && !name.to_lowercase().ends_with(&format!(".{}", ext.to_lowercase())) {
        name.push('.');
        name.push_str(ext);
    }
    // Logged only once the bytes are in hand, so the trail records documents
    // actually served — not requests that died on a missing file.
    audit_access(
        &st,
        &user,
        "document_open",
        &source_id,
        &format!(
            r#"{{"route":"source_file","bytes":{},"disposition":"{}"}}"#,
            bytes.len(),
            if inline { "inline" } else { "attachment" }
        ),
    )
    .await;

    let disposition = if inline { "inline" } else { "attachment" };
    let headers = [
        (axum::http::header::CONTENT_TYPE, mime.to_string()),
        (
            axum::http::header::CONTENT_DISPOSITION,
            format!("{disposition}; filename=\"{name}\""),
        ),
    ];
    Ok((headers, bytes).into_response())
}

const SOURCE_CHUNKS_MAX_LIMIT: i64 = 200;

/// `source_chunks {sourceId, aroundChunkId?, offset?, limit?}` — read a window of
/// one document's chunks, for the officer-facing source viewer opened by clicking
/// an inline citation.
///
/// This is the non-admin twin of `admin::browse_chunks`, and the difference is
/// the point: it is scoped through `visible_source_ids`, so a user can only read
/// chunks of documents they may already see — the shared corpus, their own
/// uploads, and this conversation's attachments. Without that a citation link
/// would become a way to read any document in the database.
pub async fn source_chunks(
    State(st): State<AppState>,
    user: AuthUser,
    Json(req): Json<SourceChunksReq>,
) -> ApiResult<Json<SourceChunksResp>> {
    let scope =
        crate::api::projects::scope_for(&st, &user.id, req.project_id.as_deref(), false).await?;
    let visible = source_repo::visible_source_ids(&st.pool, &scope).await.map_err(err)?;
    if !visible.iter().any(|id| id == &req.source_id) {
        // Same response as a genuinely absent document: whether a document the
        // caller cannot see EXISTS is itself information. Recorded regardless —
        // see the identical guard in `source_file`.
        audit_access(
            &st,
            &user,
            "document_denied",
            &req.source_id,
            r#"{"route":"source_chunks"}"#,
        )
        .await;
        return Err(ApiError::new(
            axum::http::StatusCode::NOT_FOUND,
            "not_found",
            "document not found",
        ));
    }
    // Every window is recorded, not just the first. Scrolling a long document
    // produces several rows, which is the honest trail: `offset` in the details
    // distinguishes the opening read from paging through the rest, and skipping
    // later pages would let a caller read a whole document off the record by
    // supplying its own offsets.
    audit_access(
        &st,
        &user,
        "document_read",
        &req.source_id,
        &format!(r#"{{"route":"source_chunks","offset":{}}}"#, req.offset.max(0)),
    )
    .await;

    let limit = req.limit.unwrap_or(50).clamp(1, SOURCE_CHUNKS_MAX_LIMIT);
    let total: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM chunks WHERE source_id = $1")
        .bind(&req.source_id)
        .fetch_one(&st.pool)
        .await
        .map_err(err)?;

    // Centre the window on the cited chunk so the passage lands with context
    // around it rather than at the very top of the drawer.
    let offset = match &req.around_chunk_id {
        Some(cid) => {
            let idx: Option<i64> = sqlx::query_scalar(
                "SELECT chunk_index FROM chunks WHERE id = $1 AND source_id = $2",
            )
            .bind(cid)
            .bind(&req.source_id)
            .fetch_optional(&st.pool)
            .await
            .map_err(err)?;
            match idx {
                Some(i) => (i - limit / 2).max(0),
                None => req.offset.max(0),
            }
        }
        None => req.offset.max(0),
    };

    let chunks: Vec<SourceChunkRow> = sqlx::query_as(
        "SELECT id, chunk_index, unit_type, unit_label, unit_path, page_number, \
                token_count, label_status, content \
         FROM chunks WHERE source_id = $1 ORDER BY chunk_index LIMIT $2 OFFSET $3",
    )
    .bind(&req.source_id)
    .bind(limit)
    .bind(offset)
    .fetch_all(&st.pool)
    .await
    .map_err(err)?;

    Ok(Json(SourceChunksResp { total, offset, chunks }))
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GlobalSearchReq {
    pub query: String,
    /// F027 — confine this search to one project the caller owns. Absent =
    /// today's behaviour exactly (FR-040). Verified through
    /// `projects::scope_for`, never trusted as given.
    // ⚠️ These request structs are `rename_all = "camelCase"`, so the wire key
    // is `projectId`. The alias accepts `project_id` as well, because
    // contracts/api.md documents the snake_case form and a mismatch here fails
    // SILENTLY in the worst possible direction: serde yields `None`, the scope
    // falls back to unscoped, and the route answers 200 with the caller's whole
    // visible corpus instead of refusing. Same alias pattern `QueryReq` already
    // uses for `conversationId`.
    #[serde(default, alias = "project_id")]
    pub project_id: Option<String>,
}

/// One hit, shaped exactly like `commands::search::SearchResult` so the single
/// frontend `SearchResult` type serves both transports.
#[derive(Serialize)]
pub struct GlobalSearchHit {
    pub source_id: String,
    pub chunk_id: String,
    pub file_path: String,
    pub file_name: String,
    pub excerpt: String,
    pub last_modified: i64,
    pub relevance_score: f64,
    pub page_number: Option<i64>,
}

/// Max hits returned — same 20 the desktop command uses.
const GLOBAL_SEARCH_LIMIT: i64 = 20;

/// `global_search {query}` — semantic search across the documents the caller may
/// see. The web twin of the `search::global_search` Tauri command.
///
/// The Search tab had never worked on the web build: `global_search` was only
/// ever registered as a Tauri command, so `/api/global_search` did not exist and
/// the browser got a 405 it surfaced as "API error".
///
/// **The desktop query is not safe to expose verbatim.** It selects across every
/// indexed chunk, which is correct for a single-user desktop app but on a
/// multi-user server would let any officer read excerpts from every document in
/// the database — including other users' private uploads. So this is scoped
/// through `visible_source_ids`, the same invariant `source_chunks` and
/// `admin::browse_chunks`' officer-facing twin use: the shared corpus plus the
/// caller's own documents. Conversation attachments are deliberately NOT in
/// scope here — this is a corpus search, not a chat turn, so there is no
/// conversation to attribute them to.
///
/// The desktop `indexed_folders` predicate is dropped: on the server, corpus
/// documents are not necessarily under a registered desktop folder, and
/// visibility is already the access rule that matters.
pub async fn global_search(
    State(st): State<AppState>,
    user: AuthUser,
    Json(req): Json<GlobalSearchReq>,
) -> ApiResult<Json<Vec<GlobalSearchHit>>> {
    let q = req.query.trim();
    if q.is_empty() {
        return Ok(Json(Vec::new()));
    }

    // F027 — project isolation is inherited here because global search scopes
    // through the same function as chat retrieval. `scope_for` proves ownership
    // before the project can narrow anything (FR-011, FR-013).
    let scope =
        crate::api::projects::scope_for(&st, &user.id, req.project_id.as_deref(), false).await?;
    let visible = source_repo::visible_source_ids(&st.pool, &scope).await.map_err(err)?;
    // Nothing visible ⇒ no hits. Returning early also keeps the `= ANY($2)` bind
    // off an empty array, which would scan the whole table for nothing.
    if visible.is_empty() {
        return Ok(Json(Vec::new()));
    }

    // Corpus-wide search is a document-access surface in its own right: it
    // returns excerpts. `query_log` covers chat turns only, so without this the
    // Search tab would be the one way to read across the corpus unrecorded.
    // The term is serialised through `serde_json` — it is user input and must
    // not be pasted into a hand-built JSON string.
    audit_access(
        &st,
        &user,
        "global_search",
        "",
        &serde_json::json!({ "route": "global_search", "query": q }).to_string(),
    )
    .await;

    // Embedding runs on a blocking thread: `embed_one` is CPU-bound (ONNX) and
    // would otherwise stall the async runtime for every concurrent searcher.
    let embedder = st.embedder.clone();
    let query_owned = q.to_string();
    let query_embedding = tokio::task::spawn_blocking(move || embedder.embed_one(&query_owned))
        .await
        .map_err(err)?
        .map_err(err)?;
    let embedding_json = serde_json::to_string(&query_embedding).map_err(err)?;

    let d = crate::db::schema::embedding_dim();
    let rows: Vec<(String, String, String, String, String, i64, f64, Option<i64>)> =
        sqlx::query_as(&format!(
            "SELECT c.id, c.source_id, s.file_path, s.file_name, c.content, \
                    s.last_modified, (ce.embedding::halfvec({d}) <=> $1::vector::halfvec({d})) as distance, \
                    c.page_number \
             FROM chunk_embeddings ce \
             JOIN chunks c ON c.id = ce.chunk_id \
             JOIN sources s ON s.id = c.source_id \
             WHERE s.indexing_status = 'indexed' \
               AND s.id = ANY($2) \
             ORDER BY distance ASC LIMIT $3",
        ))
        .bind(&embedding_json)
        .bind(&visible)
        .bind(GLOBAL_SEARCH_LIMIT)
        .fetch_all(&st.pool)
        .await
        .map_err(err)?;

    Ok(Json(
        rows.into_iter()
            .map(
                |(chunk_id, source_id, file_path, file_name, content, last_modified, distance, page)| {
                    GlobalSearchHit {
                        source_id,
                        chunk_id,
                        file_path,
                        file_name,
                        excerpt: content,
                        last_modified,
                        // Cosine distance → similarity, same as the desktop command.
                        relevance_score: 1.0 - distance,
                        page_number: page,
                    }
                },
            )
            .collect(),
    ))
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SetVisibilityReq {
    pub folder_id: Option<String>,
    pub source_id: Option<String>,
    pub visibility: String,
}

/// `set_visibility {folderId | sourceId, visibility}` — Private ↔ Shared flip,
/// owner or admin only (FR-013). Folder form cascades to child sources in one
/// transaction; effective on the next query, no re-index (FR-014).
pub async fn set_visibility(
    State(st): State<AppState>,
    user: AuthUser,
    Json(req): Json<SetVisibilityReq>,
) -> ApiResult<Json<serde_json::Value>> {
    if !matches!(req.visibility.as_str(), "private" | "shared") {
        return Err(ApiError::new(
            axum::http::StatusCode::BAD_REQUEST,
            "bad_visibility",
            "visibility must be 'private' or 'shared'",
        ));
    }
    let event = if req.visibility == "shared" { "share" } else { "unshare" };

    match (&req.folder_id, &req.source_id) {
        (Some(folder_id), None) => {
            if !can_manage_folder(&st, &user, folder_id).await? {
                return Err(forbidden());
            }
            // Admin-gated sharing (2026-07-28): 'shared' is no longer a
            // non-admin's to set. Taking things PRIVATE stays self-serve.
            if user.role != "admin" && req.visibility == "shared" {
                return Err(ApiError::new(
                    axum::http::StatusCode::FORBIDDEN,
                    "admin_gated",
                    "Sharing now goes through the administrator — use the share button on a document to send a request",
                ));
            }
            folder_repo::set_visibility_folder(&st.pool, folder_id, &req.visibility)
                .await
                .map_err(err)?;
            let _ = crate::db::repositories::audit::insert_user_event(
                &st.pool, Some(&user.id), event, Some(folder_id), None,
            )
            .await;
        }
        (None, Some(source_id)) => {
            let owner = source_repo::owner_of(&st.pool, source_id).await.map_err(err)?;
            let is_owner = owner.as_deref() == Some(user.id.as_str());
            if user.role != "admin" && !is_owner {
                return Err(forbidden());
            }
            // Admin-gated sharing (2026-07-28): a non-admin owner's "share"
            // files a review request instead of flipping visibility — the
            // corpus gains a document only through an admin decision. The
            // document stays private until approval. Dedupe: a second click
            // while a request is pending reports it rather than stacking.
            if user.role != "admin" && req.visibility == "shared" {
                let created = crate::db::repositories::share_requests::create_pending(
                    &st.pool, source_id, &user.id,
                )
                .await
                .map_err(err)?;
                let _ = crate::db::repositories::audit::insert_user_event(
                    &st.pool, Some(&user.id), "share_request", Some(source_id), None,
                )
                .await;
                return Ok(Json(serde_json::json!({
                    "ok": true,
                    "visibility": "private",
                    "requested": true,
                    "alreadyPending": !created,
                })));
            }
            source_repo::set_visibility_source(&st.pool, source_id, &req.visibility)
                .await
                .map_err(err)?;
            let _ = crate::db::repositories::audit::insert_user_event(
                &st.pool, Some(&user.id), event, Some(source_id), None,
            )
            .await;
        }
        _ => {
            return Err(ApiError::new(
                axum::http::StatusCode::BAD_REQUEST,
                "bad_target",
                "pass exactly one of folderId or sourceId",
            ));
        }
    }
    Ok(Json(serde_json::json!({ "ok": true, "visibility": req.visibility })))
}

/// `my_usage` — the requesting user's allowance meter (FR-009).
pub async fn my_usage(
    State(st): State<AppState>,
    user: AuthUser,
) -> ApiResult<Json<serde_json::Value>> {
    let (used_bytes, used_files) =
        source_repo::usage_for_owner(&st.pool, &user.id).await.map_err(err)?;
    let (limit_bytes, limit_files) = super::uploads::quota_limits();
    Ok(Json(serde_json::json!({
        "usedBytes": used_bytes,
        "usedFiles": used_files,
        "limitBytes": limit_bytes,
        "limitFiles": limit_files,
    })))
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SourceIdReq {
    pub source_id: String,
}

/// `delete_my_document {sourceId}` — owner (or admin) permanently deletes a
/// personal document: cascade + file removal, audited (FR-008/FR-010).
pub async fn delete_my_document(
    State(st): State<AppState>,
    user: AuthUser,
    Json(req): Json<SourceIdReq>,
) -> ApiResult<Json<serde_json::Value>> {
    let owner = source_repo::owner_of(&st.pool, &req.source_id).await.map_err(err)?;
    let is_owner = owner.as_deref() == Some(user.id.as_str());
    if !is_owner && user.role != "admin" {
        return Err(forbidden());
    }
    let outcome = hard_delete_source(&st, &req.source_id).await?;
    if let Some(p) = outcome.file_path() {
        let p = p.to_string();
        if let Err(e) = std::fs::remove_file(&p) {
            log::warn!("delete_my_document: DB rows removed but file {p} not deleted: {e}");
        }
        // If that was the last source under the owner's uploads folder, drop
        // the empty folder row so it stops appearing in listings (re-created
        // on the next upload).
        match folder_repo::delete_if_empty_upload_folder(&st.pool, &p).await {
            Ok(true) => log::info!("delete_my_document: removed now-empty uploads folder for {p}"),
            Ok(false) => {}
            Err(e) => log::warn!("delete_my_document: empty-folder cleanup failed for {p}: {e}"),
        }
    }
    let event = if is_owner { "delete_document" } else { "admin_delete_document" };
    let _ = crate::db::repositories::audit::insert_user_event(
        &st.pool, Some(&user.id), event, Some(&req.source_id), None,
    )
    .await;
    // B1 — this used to answer `{"ok":true}` whether or not anything was removed.
    // `ok` still means "the request was handled" so existing callers are
    // unaffected, but `deleted` now states what actually happened. A retained
    // document is legitimate (FR-024: another project still holds it), yet the
    // user must never be told it is gone while it remains searchable.
    Ok(Json(serde_json::json!({
        "ok": true,
        "deleted": outcome.deleted(),
        "retained": !outcome.deleted(),
    })))
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConversationIdReq {
    pub conversation_id: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct IndexProgressReq {
    pub source_ids: Vec<String>,
}

/// `index_progress {sourceIds}` — latest indexing tick per source.
///
/// The tunnel-safe twin of `GET /api/index/events` (SSE): the Cloudflare quick
/// tunnel buffers that stream indefinitely, so a browser behind it never sees a
/// tick and its progress ring could only spin. This returns the last recorded
/// percent/stage for the ids the caller owns. Ids with no tick yet are simply
/// absent from the response — the client shows an indeterminate ring rather than
/// a misleading 0%.
pub async fn index_progress(
    State(st): State<AppState>,
    user: AuthUser,
    Json(req): Json<IndexProgressReq>,
) -> ApiResult<Json<Vec<serde_json::Value>>> {
    if req.source_ids.is_empty() {
        return Ok(Json(Vec::new()));
    }
    // Only report on sources this user may see (their own docs / attachments).
    let placeholders = crate::db::pg_in(req.source_ids.len(), 2);
    let sql = format!(
        "SELECT id FROM sources WHERE (owner_id = $1 OR owner_id IS NULL) AND id IN ({placeholders})"
    );
    let mut q = sqlx::query_scalar::<_, String>(&sql).bind(&user.id);
    for id in &req.source_ids {
        q = q.bind(id);
    }
    let allowed: Vec<String> = q.fetch_all(&st.pool).await.map_err(err)?;

    let ticks = crate::services::indexer::progress_for(&allowed);
    Ok(Json(
        ticks
            .into_iter()
            .map(|t| {
                serde_json::json!({
                    "source_id": t.source_id,
                    "percent": t.percent,
                    "stage": t.stage,
                })
            })
            .collect(),
    ))
}

/// `list_conversation_attachments {conversationId}` — the requesting user's
/// attachments in that conversation (drives the chat chips UI). Never lists
/// other users' attachments even if a conversation id were guessed.
pub async fn list_conversation_attachments(
    State(st): State<AppState>,
    user: AuthUser,
    Json(req): Json<ConversationIdReq>,
) -> ApiResult<Json<Vec<serde_json::Value>>> {
    // Migration 022 — scoped through `source_link`, so a document deduped into
    // this chat shows as a chip here even though it was first indexed in another
    // one. Pairs conversation_id with owner_id on the link row for the same
    // reason `visible_source_ids` does: conversation ids are client-supplied.
    let rows: Vec<(String, String, String, Option<String>)> = sqlx::query_as(
        "SELECT s.id, s.file_name, s.indexing_status, s.error_reason FROM sources s \
         JOIN source_link l ON l.source_id = s.id \
         WHERE l.conversation_id = $1 AND l.owner_id = $2 \
         ORDER BY l.linked_at ASC",
    )
    .bind(&req.conversation_id)
    .bind(&user.id)
    .fetch_all(&st.pool)
    .await
    .map_err(err)?;
    Ok(Json(
        rows.into_iter()
            .map(|(id, name, status, reason)| {
                serde_json::json!({
                    "sourceId": id,
                    "fileName": name,
                    "status": status,
                    "failReason": reason,
                })
            })
            .collect(),
    ))
}

/// `promote_attachment {sourceId}` — owner turns a conversation attachment into
/// a normal private "My documents" entry (FR-020): move the file into the
/// user's uploads dir, clear `conversation_id`, and re-link the notebook row to
/// the uploads folder's stub notebook. Chunks/embeddings untouched — no
/// re-index. The scope set is a set, so the promoted doc keeps answering inside
/// the original conversation without double-retrieval.
pub async fn promote_attachment(
    State(st): State<AppState>,
    user: AuthUser,
    Json(req): Json<SourceIdReq>,
) -> ApiResult<Json<serde_json::Value>> {
    let source = source_repo::get_by_id(&st.pool, &req.source_id)
        .await
        .map_err(err)?
        .ok_or_else(|| ApiError::new(axum::http::StatusCode::NOT_FOUND, "not_found", "attachment not found"))?;
    if source.owner_id.as_deref() != Some(user.id.as_str()) {
        return Err(forbidden());
    }
    let Some(conv_id) = source.conversation_id.clone() else {
        return Err(ApiError::new(
            axum::http::StatusCode::BAD_REQUEST,
            "not_attachment",
            "this document is not a conversation attachment",
        ));
    };

    // Get-or-create the user's uploads folder (same layout as upload_documents).
    let dir = super::uploads::user_upload_dir(&user.username);
    std::fs::create_dir_all(&dir).map_err(err)?;
    let dir_str = dir.to_string_lossy().to_string();
    let folder = match folder_repo::get_by_path(&st.pool, &dir_str).await.map_err(err)? {
        Some(f) => f,
        None => folder_repo::insert(&st.pool, &dir_str, Some(&user.id)).await.map_err(err)?,
    };

    // Move the file out of the __conv__ dir; keep a unique name on collision.
    let file_name = std::path::Path::new(&source.file_path)
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("attachment")
        .to_string();
    let mut dest = dir.join(&file_name);
    if dest.exists() {
        dest = dir.join(format!("{}-{}", &req.source_id[..8.min(req.source_id.len())], file_name));
    }
    if let Err(e) = std::fs::rename(&source.file_path, &dest) {
        // Cross-device or already-moved: fall back to copy+remove, else keep
        // the old path (promotion still works — retrieval reads chunks).
        log::warn!("promote_attachment: rename failed ({e}), trying copy");
        if std::fs::copy(&source.file_path, &dest).is_ok() {
            let _ = std::fs::remove_file(&source.file_path);
        } else {
            dest = std::path::PathBuf::from(&source.file_path);
        }
    }

    let mut tx = st.pool.begin().await.map_err(err)?;
    sqlx::query(
        "UPDATE sources SET conversation_id = NULL, visibility = 'private', file_path = $1 WHERE id = $2",
    )
    .bind(dest.to_string_lossy().to_string())
    .bind(&req.source_id)
    .execute(&mut *tx)
    .await
    .map_err(err)?;
    sqlx::query("DELETE FROM notebook_sources WHERE notebook_id = $1 AND source_id = $2")
        .bind(format!("__conv__{conv_id}"))
        .bind(&req.source_id)
        .execute(&mut *tx)
        .await
        .map_err(err)?;
    sqlx::query(
        "INSERT INTO notebook_sources (notebook_id, source_id, added_at) VALUES ($1, $2, $3) \
         ON CONFLICT (notebook_id, source_id) DO NOTHING",
    )
    .bind(format!("__folder__{}", folder.id))
    .bind(&req.source_id)
    .bind(chrono::Utc::now().timestamp_millis())
    .execute(&mut *tx)
    .await
    .map_err(err)?;
    tx.commit().await.map_err(err)?;

    let _ = crate::db::repositories::audit::insert_user_event(
        &st.pool, Some(&user.id), "promote", Some(&req.source_id), None,
    )
    .await;
    Ok(Json(serde_json::json!({ "ok": true })))
}

/// `delete_conversation {conversationId}` — remove the requesting user's
/// attachments for that conversation (chunks/embeddings/files) via the FK-safe
/// cascade (FR-019). Called by the web UI when a chat session is deleted.
pub async fn delete_conversation(
    State(st): State<AppState>,
    user: AuthUser,
    Json(req): Json<ConversationIdReq>,
) -> ApiResult<Json<serde_json::Value>> {
    // Migration 022 — drop this conversation's links first, then delete only the
    // content nothing links to any more. Since one indexed document can now be
    // attached to several chats, deleting by `conversation_id` alone would
    // destroy chunks that other chats still answer from — a failure mode that
    // did not exist while every attachment owned a private copy.
    let ids = source_repo::unlink_conversation(&st.pool, &req.conversation_id, &user.id)
        .await
        .map_err(err)?;

    // ⚠️ B1 (2026-08-07) — this MUST happen before the loop, not after it.
    //
    // `is_orphaned` (used below) counts only `source_link`, but
    // `hard_delete_source` guards on `is_referenced`, which ALSO counts
    // `notebook_sources`. The `__conv__` stub notebook still held a row there, so
    // every attachment looked orphaned to the loop and referenced to the delete —
    // `hard_delete_source` retained it, the stub was dropped a moment later, and
    // the content survived as a fully-indexed orphan while the API reported
    // success. A user was told their attachment was deleted and it remained
    // searchable. `notebook_sources.notebook_id` is ON DELETE CASCADE, so
    // dropping the stub here clears its rows and both predicates finally agree.
    let _ = sqlx::query("DELETE FROM notebooks WHERE id = $1")
        .bind(format!("__conv__{}", req.conversation_id))
        .execute(&st.pool)
        .await;

    let mut removed = 0usize;
    let mut retained = 0usize;
    for id in &ids {
        if !source_repo::is_orphaned(&st.pool, id).await.map_err(err)? {
            continue; // still attached elsewhere — keep the content
        }
        // A promoted attachment (FR-020) has had `conversation_id` cleared and is
        // a normal "My documents" entry now; unlinking it from a deleted chat
        // must not delete the document the user deliberately kept.
        let still_owned = sqlx::query_scalar::<_, Option<String>>(
            "SELECT conversation_id FROM sources WHERE id = $1 AND owner_id = $2",
        )
        .bind(id)
        .bind(&user.id)
        .fetch_optional(&st.pool)
        .await
        .map_err(err)?;
        if !matches!(still_owned, Some(Some(_))) {
            continue;
        }
        let outcome = hard_delete_source(&st, id).await?;
        if let Some(p) = outcome.file_path() {
            let _ = std::fs::remove_file(p);
        }
        // Count what happened, not what was attempted. `removed` used to be
        // incremented unconditionally, which is how a delete that removed
        // nothing still reported `{"removed":1}`.
        if outcome.deleted() { removed += 1; } else { retained += 1; }
    }

    let _ = crate::db::repositories::audit::insert_user_event(
        &st.pool, Some(&user.id), "delete_conversation", Some(&req.conversation_id), None,
    )
    .await;
    // `retained` is not a failure — FR-024 keeps a document alive while another
    // project or chat still references it. It is surfaced so the caller can say
    // so honestly instead of implying everything went.
    Ok(Json(serde_json::json!({ "ok": true, "removed": removed, "retained": retained })))
}
