//! Repository: `indexed_folders` table.

use anyhow::Result;
use chrono::Utc;
use serde::{Deserialize, Serialize};
use crate::db::Db;
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct IndexedFolder {
    pub id: String,
    pub folder_path: String,
    pub indexing_status: String,
    pub file_count: i64,
    pub last_scanned_at: Option<i64>,
    pub created_at: i64,
    // Feature 021 — visibility + owner surfaced in listings (FR-012).
    // `owner_username` is a join artifact (users table), not a column, so it
    // defaults when a query doesn't select it (e.g. `SELECT *` in get_by_path).
    #[serde(default = "default_visibility")]
    #[sqlx(default)]
    pub visibility: String,
    #[serde(default)]
    #[sqlx(default)]
    pub owner_username: Option<String>,
    // Migration 026 — progress of the batch running right now, polled by the UI.
    // `indexing_total = 0` means idle; do NOT infer idle from done == total,
    // which is also true for the instant a batch finishes.
    #[serde(default)]
    #[sqlx(default)]
    pub indexing_total: i64,
    #[serde(default)]
    #[sqlx(default)]
    pub indexing_done: i64,
}

fn default_visibility() -> String {
    "shared".to_string()
}

pub async fn insert(
    pool: &Db,
    folder_path: &str,
    owner_id: Option<&str>,
) -> Result<IndexedFolder> {
    let id = Uuid::new_v4().to_string();
    let now = Utc::now().timestamp_millis();

    // Insert the indexed_folders row AND the matching stub notebook row in
    // one transaction. The folder flow attaches every indexed source to a
    // synthetic notebook id `__folder__<folder_id>` (see commands/folders.rs
    // and lib.rs file-watcher loop); without the parent notebook row,
    // notebook_sources.notebook_id FK rejects the attach with SQLITE_CONSTRAINT
    // and every file fails to index.
    let mut tx = pool.begin().await?;

    // F021: user-owned folders start private; ownerless (admin/global) shared.
    let visibility = if owner_id.is_some() { "private" } else { "shared" };
    sqlx::query(
        "INSERT INTO indexed_folders (id, folder_path, indexing_status, file_count, created_at, owner_id, visibility) \
         VALUES ($1, $2, 'pending', 0, $3, $4, $5)",
    )
    .bind(&id)
    .bind(folder_path)
    .bind(now)
    .bind(owner_id)
    .bind(visibility)
    .execute(&mut *tx)
    .await?;

    let stub_id = format!("__folder__{}", id);
    let stub_name = format!("Indexed folder: {}", folder_path);
    sqlx::query(
        "INSERT INTO notebooks (id, name, created_at, updated_at) \
         VALUES ($1, $2, $3, $4) ON CONFLICT (id) DO NOTHING",
    )
    .bind(&stub_id)
    .bind(&stub_name)
    .bind(now)
    .bind(now)
    .execute(&mut *tx)
    .await?;

    tx.commit().await?;

    Ok(IndexedFolder {
        id,
        folder_path: folder_path.to_string(),
        indexing_status: "pending".to_string(),
        file_count: 0,
        last_scanned_at: None,
        created_at: now,
        visibility: visibility.to_string(),
        owner_username: None,
        indexing_total: 0,
        indexing_done: 0,
    })
}

/// Start a batch: record how many files it holds and reset the counter.
///
/// Called BEFORE the first file is handed to the indexer, so the UI can show
/// "0 of 61" immediately rather than an indeterminate spinner. Also flips the
/// folder to `processing` — for an uploads folder that is already `indexed`
/// from a previous batch, that transition is what tells the poller to start.
/// `category_id` (migration 027) is remembered for the duration of the batch so
/// that a restart mid-batch can still file the files it resumes. Without it, the
/// automatic GPU switch — which restarts the server BY DESIGN, early in every
/// batch — dropped the category on the whole upload.
pub async fn begin_batch(
    pool: &Db,
    id: &str,
    total: i64,
    category_id: Option<&str>,
) -> Result<()> {
    // ADDITIVE, so folders can be QUEUED. Every upload lands in the same
    // `uploads/admin` folder row, so a second upload starting while the first is
    // still indexing used to RESET the counter — losing the first batch's
    // progress and showing a total for only the newest folder. Adding instead
    // means dropping three folders in a row gives one honest bar over all of
    // them; the files themselves already queue behind `indexing_sem`.
    //
    // `indexing_done` is only zeroed when no batch is running, so a queued
    // folder extends the current bar rather than restarting it.
    sqlx::query(
        "UPDATE indexed_folders \
         SET indexing_total = CASE WHEN indexing_total > 0 THEN indexing_total + $1 ELSE $1 END, \
             indexing_done  = CASE WHEN indexing_total > 0 THEN indexing_done ELSE 0 END, \
             indexing_status = 'processing', \
             indexing_category_id = $2 \
         WHERE id = $3",
    )
    .bind(total)
    .bind(category_id)
    .bind(id)
    .execute(pool)
    .await?;
    Ok(())
}

/// Close any batch that can no longer finish.
///
/// Returns how many were closed. A file killed between being marked indexed and
/// its counter bump never increments, so its batch can never reach its total:
/// the bar sticks (seen at 6/7 and 25/27) and — worse — the GPU poller treated
/// the open flag as "work in progress" and held chat down indefinitely.
///
/// Run periodically rather than only at startup, because the stranding happens
/// DURING a run: at startup there was nothing left to fix, and the next restart
/// might be hours away.
///
/// A folder is only closed when nothing under it is pending or processing, and a
/// query error counts as busy, so a live batch is never cut short.
pub async fn close_stranded_batches(pool: &Db) -> Result<usize> {
    let open: Vec<(String, i64, i64)> = sqlx::query_as(
        "SELECT id, indexing_total, indexing_done FROM indexed_folders WHERE indexing_total > 0",
    )
    .fetch_all(pool)
    .await
    .unwrap_or_default();

    let mut closed = 0usize;
    for (folder_id, total, done) in open {
        let stub = format!("__folder__{folder_id}");
        let busy: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM sources s \
             JOIN notebook_sources ns ON ns.source_id = s.id \
             WHERE ns.notebook_id = $1 AND s.indexing_status IN ('pending','processing')",
        )
        .bind(&stub)
        .fetch_one(pool)
        .await
        .unwrap_or(1);
        if busy == 0 {
            log::warn!(
                "closing stranded batch on folder {folder_id} ({done}/{total} counted, \
                 nothing left indexing)"
            );
            finish_batch(pool, &folder_id).await?;
            closed += 1;
        }
    }
    Ok(closed)
}

/// The category the in-flight batch is filing into, if any. Read by the resume
/// path after a restart; `None` once the batch has closed.
pub async fn batch_category(pool: &Db, id: &str) -> Result<Option<String>> {
    let v: Option<Option<String>> =
        sqlx::query_scalar("SELECT indexing_category_id FROM indexed_folders WHERE id = $1")
            .bind(id)
            .fetch_optional(pool)
            .await?;
    Ok(v.flatten())
}

/// One more file finished. Incremented in SQL rather than from a counter held
/// in the spawning task: uploads index through per-file `tokio::spawn`s that
/// share no state, and a read-modify-write from several of them would lose
/// ticks. `+ 1` in the statement is atomic under any interleaving.
/// The statement also CLOSES the batch when this was the last file, because the
/// upload path has nowhere else to do it: those files index in independent
/// `tokio::spawn`s and nothing joins them, so there is no "all done" moment in
/// Rust to hook. Doing it in the same UPDATE means the close happens exactly
/// once, under the row lock, whichever task happens to finish last.
///
/// The `indexing_total > 0` guard matters: without it a stray bump on an idle
/// folder would satisfy `done + 1 >= 0` and mark it indexed. All CASE arms read
/// the pre-UPDATE column values, which is why `indexing_done + 1` is written
/// out rather than referring to the new value.
pub async fn bump_indexed(pool: &Db, id: &str) -> Result<()> {
    let now = Utc::now().timestamp_millis();
    sqlx::query(
        "UPDATE indexed_folders SET \
           indexing_done = indexing_done + 1, \
           indexing_total = CASE WHEN indexing_total > 0 AND indexing_done + 1 >= indexing_total \
                                 THEN 0 ELSE indexing_total END, \
           indexing_status = CASE WHEN indexing_total > 0 AND indexing_done + 1 >= indexing_total \
                                  THEN 'indexed' ELSE indexing_status END, \
           last_scanned_at = CASE WHEN indexing_total > 0 AND indexing_done + 1 >= indexing_total \
                                  THEN $2 ELSE last_scanned_at END, \
           indexing_category_id = CASE WHEN indexing_total > 0 AND indexing_done + 1 >= indexing_total \
                                  THEN NULL ELSE indexing_category_id END \
         WHERE id = $1",
    )
    .bind(id)
    .bind(now)
    .execute(pool)
    .await?;
    Ok(())
}

/// Batch over. Clears `indexing_total` back to 0 (= idle) and marks the folder
/// indexed. `indexing_done` is left as it is: nothing reads it once total is 0,
/// and keeping it makes the last run visible when debugging.
pub async fn finish_batch(pool: &Db, id: &str) -> Result<()> {
    sqlx::query(
        "UPDATE indexed_folders \
         SET indexing_total = 0, indexing_status = 'indexed', last_scanned_at = $1, \
             indexing_category_id = NULL \
         WHERE id = $2",
    )
    .bind(Utc::now().timestamp_millis())
    .bind(id)
    .execute(pool)
    .await?;
    Ok(())
}

/// Backfill stub notebooks for any indexed_folder whose `__folder__<id>`
/// parent is missing. Heals installs created before the stub insert was
/// added to `insert()` (otherwise those folders can never index anything).
pub async fn backfill_stub_notebooks(pool: &Db) -> Result<usize> {
    let rows: Vec<(String, String, i64)> = sqlx::query_as(
        "SELECT f.id, f.folder_path, f.created_at FROM indexed_folders f \
         WHERE NOT EXISTS ( \
            SELECT 1 FROM notebooks n WHERE n.id = '__folder__' || f.id \
         )",
    )
    .fetch_all(pool)
    .await?;

    let mut inserted = 0usize;
    for (folder_id, folder_path, created_at) in &rows {
        let stub_id = format!("__folder__{}", folder_id);
        let stub_name = format!("Indexed folder: {}", folder_path);
        sqlx::query(
            "INSERT INTO notebooks (id, name, created_at, updated_at) \
             VALUES ($1, $2, $3, $4) ON CONFLICT (id) DO NOTHING",
        )
        .bind(&stub_id)
        .bind(&stub_name)
        .bind(created_at)
        .bind(created_at)
        .execute(pool)
        .await?;
        inserted += 1;
    }
    Ok(inserted)
}

pub async fn list(pool: &Db) -> Result<Vec<IndexedFolder>> {
    // Join with sources to get real indexed count (more accurate than stored file_count
    // which can lag when the app is killed mid-scan).
    Ok(sqlx::query_as::<_, IndexedFolder>(
        "SELECT f.id, f.folder_path, f.indexing_status,
                COALESCE((
                    SELECT COUNT(*) FROM sources s
                    WHERE s.file_path LIKE (f.folder_path || '%')
                    AND s.indexing_status = 'indexed'
                ), 0) AS file_count,
                f.last_scanned_at, f.created_at, f.visibility,
                u.username AS owner_username,
                f.indexing_total, f.indexing_done
         FROM indexed_folders f
         LEFT JOIN users u ON u.id = f.owner_id
         ORDER BY f.created_at DESC",
    )
    .fetch_all(pool)
    .await?)
}

/// Folders visible to a specific user: the ones they own, every shared
/// folder (F021: explicit `visibility` replaces the old owner_id-NULL-means-
/// global convention; the migration backfills NULL-owner folders as 'shared',
/// so the pre-021 corpus keeps appearing for everyone), PLUS anyone else's
/// folder that contains at least one per-document-shared source (the My
/// documents toggle flips sources, not the folder). `file_count` counts only
/// the sources this viewer can retrieve from, so a folder never advertises
/// more files than the viewer can see. Admins use this too — a user's private
/// documents are private from admin lists as well; management actions stay
/// owner/admin-gated separately via `can_manage_folder`.
pub async fn list_for_owner(pool: &Db, owner_id: &str) -> Result<Vec<IndexedFolder>> {
    Ok(sqlx::query_as::<_, IndexedFolder>(
        "SELECT f.id, f.folder_path, f.indexing_status,
                COALESCE((
                    SELECT COUNT(*) FROM sources s
                    WHERE s.file_path LIKE (f.folder_path || '%')
                    AND s.indexing_status = 'indexed'
                    AND s.conversation_id IS NULL
                    AND (s.visibility = 'shared' OR s.owner_id = $1)
                ), 0) AS file_count,
                f.last_scanned_at, f.created_at, f.visibility,
                u.username AS owner_username,
                f.indexing_total, f.indexing_done
         FROM indexed_folders f
         LEFT JOIN users u ON u.id = f.owner_id
         WHERE f.owner_id = $1
            OR f.visibility = 'shared'
            OR EXISTS (
                SELECT 1 FROM sources s
                WHERE s.file_path LIKE (f.folder_path || '%')
                AND s.visibility = 'shared'
                AND s.conversation_id IS NULL
            )
         ORDER BY f.created_at DESC",
    )
    .bind(owner_id)
    .fetch_all(pool)
    .await?)
}

/// F021: after the last document in a user's uploads folder is deleted, drop
/// the now-empty folder row and its stub notebook so the folder stops
/// appearing in every listing (it is re-created on the next upload). Only
/// user-owned folders qualify — global corpus folders stay even when empty.
/// Conservative: any remaining source under the path (including conversation
/// attachments, which share the uploads directory) keeps the row, because
/// re-creating the folder later re-scans the whole directory.
pub async fn delete_if_empty_upload_folder(pool: &Db, file_path: &str) -> Result<bool> {
    let folder: Option<(String, String)> = sqlx::query_as(
        "SELECT id, folder_path FROM indexed_folders \
         WHERE owner_id IS NOT NULL AND $1 LIKE (folder_path || '%')",
    )
    .bind(file_path)
    .fetch_optional(pool)
    .await?;
    let Some((folder_id, folder_path)) = folder else { return Ok(false) };
    let remaining: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM sources WHERE file_path LIKE ($1 || '%')",
    )
    .bind(&folder_path)
    .fetch_one(pool)
    .await?;
    if remaining > 0 {
        return Ok(false);
    }
    let stub_id = format!("__folder__{folder_id}");
    let mut tx = pool.begin().await?;
    sqlx::query("DELETE FROM notebook_sources WHERE notebook_id = $1")
        .bind(&stub_id)
        .execute(&mut *tx)
        .await?;
    sqlx::query("DELETE FROM notebooks WHERE id = $1")
        .bind(&stub_id)
        .execute(&mut *tx)
        .await?;
    sqlx::query("DELETE FROM indexed_folders WHERE id = $1")
        .bind(&folder_id)
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    Ok(true)
}

/// F021 — flip a folder's visibility AND all of its child sources in one
/// transaction (invariant I3). Takes effect on the next query — the retrieval
/// scope set is computed per turn, no re-index needed (FR-014).
pub async fn set_visibility_folder(pool: &Db, folder_id: &str, visibility: &str) -> Result<()> {
    let mut tx = pool.begin().await?;
    sqlx::query("UPDATE indexed_folders SET visibility = $1 WHERE id = $2")
        .bind(visibility)
        .bind(folder_id)
        .execute(&mut *tx)
        .await?;
    // Child sources via the stub-notebook link; attachments never live under a
    // __folder__ notebook so conversation rows are untouched.
    sqlx::query(
        "UPDATE sources SET visibility = $1 WHERE id IN \
         (SELECT source_id FROM notebook_sources WHERE notebook_id = $2)",
    )
    .bind(visibility)
    .bind(format!("__folder__{folder_id}"))
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok(())
}

/// Owner of a folder (NULL/global → `None`); `Ok(None)` also when the folder
/// does not exist — callers treat that as "not authorized / not found".
pub async fn owner_of(pool: &Db, folder_id: &str) -> Result<Option<String>> {
    let owner: Option<Option<String>> =
        sqlx::query_scalar("SELECT owner_id FROM indexed_folders WHERE id = $1")
            .bind(folder_id)
            .fetch_optional(pool)
            .await?;
    Ok(owner.flatten())
}

pub async fn get_by_path(pool: &Db, path: &str) -> Result<Option<IndexedFolder>> {
    Ok(sqlx::query_as::<_, IndexedFolder>(
        "SELECT * FROM indexed_folders WHERE folder_path = $1",
    )
    .bind(path)
    .fetch_optional(pool)
    .await?)
}

pub async fn update_status(pool: &Db, id: &str, status: &str) -> Result<()> {
    sqlx::query("UPDATE indexed_folders SET indexing_status = $1 WHERE id = $2")
        .bind(status)
        .bind(id)
        .execute(pool)
        .await?;
    Ok(())
}

pub async fn update_file_count(pool: &Db, id: &str, count: i64) -> Result<()> {
    sqlx::query("UPDATE indexed_folders SET file_count = $1 WHERE id = $2")
        .bind(count)
        .bind(id)
        .execute(pool)
        .await?;
    Ok(())
}

pub async fn update_last_scanned(pool: &Db, id: &str) -> Result<()> {
    let now = Utc::now().timestamp_millis();
    sqlx::query("UPDATE indexed_folders SET last_scanned_at = $1 WHERE id = $2")
        .bind(now)
        .bind(id)
        .execute(pool)
        .await?;
    Ok(())
}

/// What a folder delete removed (or, from `delete_preview`, would remove).
/// Returned so the caller can show an honest confirmation *before* the delete and
/// report the real outcome *after* it, instead of assuming success.
#[derive(Debug, Clone, Default, serde::Serialize)]
pub struct FolderDeleteCounts {
    pub sources: i64,
    pub chunks: i64,
    pub embeddings: i64,
    /// False when no `indexed_folders` row exists for this id — the folder is a
    /// dangling UI entry and there is nothing to cascade from.
    pub folder_found: bool,
}

/// Every source belonging to a folder, by BOTH membership signals.
///
/// This union is the fix for a real data-loss-adjacent bug. The old code matched
/// only `sources.file_path LIKE '<folder_path>%'`, which silently matches nothing
/// when the two disagree — and they do disagree for **uploaded** corpora, whose
/// sources are linked to the folder through the `__folder__<id>` stub notebook
/// rather than by living under the folder's path. The delete then removed zero
/// rows while the caller still dropped the `indexed_folders` row, so the folder
/// vanished from the UI and every chunk and embedding survived. Re-uploading the
/// same corpus afterwards produced two live copies of every document — including
/// two different answers to the same question.
///
/// Matching on either signal makes the delete correct for both folder shapes.
async fn folder_source_ids<'e, E>(ex: E, folder_id: &str, folder_path: &str) -> Result<Vec<String>>
where
    E: sqlx::Executor<'e, Database = sqlx::Postgres>,
{
    let stub = format!("__folder__{folder_id}");
    let path_prefix = format!("{folder_path}%");
    Ok(sqlx::query_scalar::<_, String>(
        "SELECT id FROM sources WHERE file_path LIKE $1 \
         UNION \
         SELECT source_id FROM notebook_sources WHERE notebook_id = $2",
    )
    .bind(&path_prefix)
    .bind(&stub)
    .fetch_all(ex)
    .await?)
}

/// Count what a folder delete would remove, without removing anything.
/// Drives the confirmation dialog so the warning states real numbers.
pub async fn delete_preview(pool: &Db, folder_id: &str) -> Result<FolderDeleteCounts> {
    let folder: Option<IndexedFolder> =
        sqlx::query_as("SELECT * FROM indexed_folders WHERE id = $1")
            .bind(folder_id)
            .fetch_optional(pool)
            .await?;
    let Some(f) = folder else {
        return Ok(FolderDeleteCounts { folder_found: false, ..Default::default() });
    };
    let ids = folder_source_ids(pool, folder_id, &f.folder_path).await?;
    if ids.is_empty() {
        return Ok(FolderDeleteCounts { folder_found: true, ..Default::default() });
    }
    let chunks: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM chunks WHERE source_id = ANY($1)")
            .bind(&ids)
            .fetch_one(pool)
            .await
            .unwrap_or(0);
    let embeddings: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM chunk_embeddings WHERE chunk_id IN \
         (SELECT id FROM chunks WHERE source_id = ANY($1))",
    )
    .bind(&ids)
    .fetch_one(pool)
    .await
    .unwrap_or(0);
    Ok(FolderDeleteCounts {
        sources: ids.len() as i64,
        chunks,
        embeddings,
        folder_found: true,
    })
}

/// Delete a folder and everything indexed under it — sources, chunks, embeddings,
/// graph nodes/edges. Returns what was actually removed so the caller can report
/// it and detect a no-op rather than reporting a silent success.
pub async fn delete(pool: &Db, folder_id: &str) -> Result<FolderDeleteCounts> {
    let folder: Option<IndexedFolder> = sqlx::query_as(
        "SELECT * FROM indexed_folders WHERE id = $1",
    )
    .bind(folder_id)
    .fetch_optional(pool)
    .await?;

    // One transaction for the whole revoke. The caller (revoke_indexed_folder)
    // already drained the indexer via INDEXING_SEM, so this should be uncontested.
    let mut tx = pool.begin().await?;
    let mut counts = FolderDeleteCounts::default();

    if let Some(f) = folder {
        counts.folder_found = true;
        let ids = folder_source_ids(&mut *tx, folder_id, &f.folder_path).await?;

        if !ids.is_empty() {
            counts.sources = ids.len() as i64;
            counts.chunks = sqlx::query_scalar("SELECT COUNT(*) FROM chunks WHERE source_id = ANY($1)")
                .bind(&ids)
                .fetch_one(&mut *tx)
                .await
                .unwrap_or(0);
            counts.embeddings = sqlx::query_scalar(
                "SELECT COUNT(*) FROM chunk_embeddings WHERE chunk_id IN \
                 (SELECT id FROM chunks WHERE source_id = ANY($1))",
            )
            .bind(&ids)
            .fetch_one(&mut *tx)
            .await
            .unwrap_or(0);

            // `chunk_embeddings.chunk_id` and `chunks.source_id` are both
            // ON DELETE CASCADE in Postgres (001_baseline.sql + the runtime
            // `ensure_vector_table` DDL), so deleting sources would carry both
            // away on its own. We still delete them explicitly and in order:
            // the counts above are what we report to the user, and an explicit
            // delete keeps that promise true even if a future migration relaxes
            // a constraint.
            sqlx::query(
                "DELETE FROM chunk_embeddings WHERE chunk_id IN \
                 (SELECT id FROM chunks WHERE source_id = ANY($1))",
            )
            .bind(&ids)
            .execute(&mut *tx)
            .await?;

            // graph_nodes.chunk_id and graph_edges.{from_node,to_node} are FK
            // NO ACTION — nothing cascades to them, so they must go first or the
            // delete of chunks fails on a foreign-key violation and the whole
            // transaction rolls back.
            let node_subq = "SELECT id FROM graph_nodes WHERE chunk_id IN \
                 (SELECT id FROM chunks WHERE source_id = ANY($1))";

            for col in ["from_node", "to_node"] {
                sqlx::query(&format!("DELETE FROM graph_edges WHERE {col} IN ({node_subq})"))
                    .bind(&ids)
                    .execute(&mut *tx)
                    .await?;
            }

            sqlx::query(
                "DELETE FROM graph_nodes WHERE chunk_id IN \
                 (SELECT id FROM chunks WHERE source_id = ANY($1))",
            )
            .bind(&ids)
            .execute(&mut *tx)
            .await?;

            // Drop the stub-notebook links before the sources themselves, so a
            // NO ACTION FK on notebook_sources cannot block the delete.
            sqlx::query("DELETE FROM notebook_sources WHERE source_id = ANY($1)")
                .bind(&ids)
                .execute(&mut *tx)
                .await?;

            // Sources cascade to chunks via FK ON DELETE CASCADE.
            sqlx::query("DELETE FROM sources WHERE id = ANY($1)")
                .bind(&ids)
                .execute(&mut *tx)
                .await?;
        }
    }

    sqlx::query("DELETE FROM indexed_folders WHERE id = $1")
        .bind(folder_id)
        .execute(&mut *tx)
        .await?;

    tx.commit().await?;

    // A folder row that owned no sources is worth a log line: it is either an
    // empty folder (fine) or the symptom of the membership mismatch this
    // function was rewritten to fix (not fine).
    if counts.folder_found && counts.sources == 0 {
        log::warn!(
            "folder delete {folder_id}: folder row removed but matched 0 sources — \
             verify nothing was orphaned"
        );
    } else {
        log::info!(
            "folder delete {folder_id}: removed {} sources, {} chunks, {} embeddings",
            counts.sources, counts.chunks, counts.embeddings
        );
    }

    // Purge KG entities orphaned by the source delete (entities have no FK to
    // sources/chunks; only entity_mentions cascades). Done outside the tx to
    // keep the writer lock window small.
    let _ = sqlx::query(
        "DELETE FROM entities WHERE id NOT IN (SELECT DISTINCT entity_id FROM entity_mentions)",
    )
    .execute(pool)
    .await;

    Ok(counts)
}
