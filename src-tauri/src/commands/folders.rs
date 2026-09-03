//! Tauri commands: indexed folder management (US2).

use serde::{Deserialize, Serialize};
use crate::db::Db;
use std::sync::Arc;
use tauri::{Emitter, State};

use crate::db::repositories::{audit, folders as folder_repo};
use crate::services::{
    embedder::EmbedderState,
    file_watcher::FileWatcherState,
    hardware_detector::HardwareTier,
    indexer,
    parsers::FileFormat,
    task_budget::{BudgetState, TaskBudget},
};

// F014: FolderDto + the shared `index_folder` orchestration (and its scan
// helpers FolderProgress/is_temp_file/emit_folder_progress) moved to
// `crate::services::indexing` (compiled in both desktop and headless builds).
// Re-exported so the Tauri command wrappers and other `commands::folders::*`
// references keep working unchanged.
pub use crate::services::indexing::{index_folder, FolderDto};

#[tauri::command]
pub async fn add_indexed_folder(
    app: tauri::AppHandle,
    pool: State<'_, Db>,
    embedder: State<'_, Arc<EmbedderState>>,
    _task_budget: State<'_, TaskBudget>,
    tier: State<'_, HardwareTier>,
    file_watcher: State<'_, FileWatcherState>,
    folder_path: String,
) -> Result<FolderDto, String> {
    if !std::path::Path::new(&folder_path).is_dir() {
        return Err(format!("Directory not found: {folder_path}"));
    }
    // Live file-change watching is a desktop concern (the headless server owns a
    // fixed corpus), so keep it in the command — out of the shared service fn.
    if let Err(e) = file_watcher.watch(&folder_path) {
        log::warn!("Could not watch {folder_path}: {e}");
    }
    index_folder(
        pool.inner(),
        embedder.inner(),
        tier.inner(),
        &folder_path,
        None,
        Some(app),
        None,
    )
    .await
}

#[tauri::command]
pub async fn list_indexed_folders(
    pool: State<'_, Db>,
) -> Result<Vec<FolderDto>, String> {
    folder_repo::list(&pool)
        .await
        .map(|v| v.into_iter().map(Into::into).collect())
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn revoke_indexed_folder(
    pool: State<'_, Db>,
    file_watcher: State<'_, FileWatcherState>,
    folder_id: String,
) -> Result<(), String> {
    // Fetch folder path before deleting so we can unwatch it.
    if let Ok(folders) = folder_repo::list(&pool).await {
        if let Some(folder) = folders.into_iter().find(|f| f.id == folder_id) {
            if let Err(e) = file_watcher.unwatch(&folder.folder_path) {
                log::warn!("Could not unwatch {}: {e}", folder.folder_path);
            }
        }
    }

    // Drain the indexer before touching the writer lock. Without this we race
    // the indexer's per-source transaction and either (a) block it long enough
    // to push it past its own busy_timeout (constitution.pdf "database is
    // locked" incident, 2026-05-16) or (b) get cancelled ourselves. 120s is
    // generous enough for a single PDF batch (typical 5-30s) but short enough
    // that the user sees a real error instead of an infinite spinner.
    let _indexing_permit = match tokio::time::timeout(
        std::time::Duration::from_secs(120),
        indexer::indexing_sem().acquire(),
    )
    .await
    {
        Ok(Ok(p)) => p,
        Ok(Err(_)) => return Err("indexer semaphore closed".to_string()),
        Err(_) => {
            return Err(
                "Indexing is in progress on another file. Please wait a few seconds and try again."
                    .to_string(),
            );
        }
    };

    folder_repo::delete(&pool, &folder_id)
        .await
        .map_err(|e| e.to_string())?;

    audit::insert_event(
        &pool,
        "folder_revoked",
        Some(&folder_id),
        None,
    )
    .await
    .map_err(|e| e.to_string())?;

    Ok(())
}

/// Return all sources whose file_path starts with the given folder path.
/// Used by Settings to show per-file status inside each indexed folder.
#[tauri::command]
pub async fn list_folder_sources(
    pool: State<'_, Db>,
    folder_path: String,
) -> Result<Vec<crate::db::repositories::sources::Source>, String> {
    let path_prefix = format!("{}/", folder_path.trim_end_matches('/'));
    sqlx::query_as::<_, crate::db::repositories::sources::Source>(
        "SELECT * FROM sources WHERE file_path LIKE $1 ORDER BY file_name ASC",
    )
    .bind(format!("{}%", path_prefix))
    .fetch_all(&*pool)
    .await
    .map_err(|e| e.to_string())
}

/// Re-index a failed source from a folder (no notebook context needed).
#[tauri::command]
pub async fn retry_folder_source(
    app: tauri::AppHandle,
    pool: State<'_, Db>,
    embedder: State<'_, Arc<EmbedderState>>,
    task_budget: State<'_, TaskBudget>,
    tier: State<'_, HardwareTier>,
    source_id: String,
    folder_id: String,
) -> Result<(), String> {
    use crate::db::repositories::sources as source_repo;
    // Reset to pending so indexer picks it up fresh.
    source_repo::update_status(&pool, &source_id, "pending")
        .await
        .map_err(|e| e.to_string())?;

    let source = source_repo::get_by_id(&pool, &source_id)
        .await
        .map_err(|e| e.to_string())?
        .ok_or("Source not found")?;

    let pool2 = pool.inner().clone();
    let embedder2 = Arc::clone(&embedder);
    let budget2 = task_budget.inner().clone();
    let tier2 = tier.inner().clone();
    let notebook_id = format!("__folder__{}", folder_id);
    let app2 = app.clone();

    tauri::async_runtime::spawn(async move {
        if let Err(e) = crate::services::indexer::index_file(
            &pool2,
            Arc::clone(&embedder2),
            &budget2,
            &tier2,
            &notebook_id,
            &source.file_path,
            Some(app2),
        )
        .await
        {
            log::error!("Folder source retry failed for {}: {e}", source.file_path);
        }
    });

    Ok(())
}

/// Deep-index a single source: mark it `index_tier = 'deep'`, reset to pending,
/// and re-run indexing. The deep tier enables the heavy LLM IR (Mistral-7B)
/// extraction path inside `do_index`; ordinary (fast) indexing never touches it.
/// This is the per-file "deep index" action in Settings (beside reindex / remove).
#[tauri::command]
pub async fn deep_index_folder_source(
    app: tauri::AppHandle,
    pool: State<'_, Db>,
    embedder: State<'_, Arc<EmbedderState>>,
    task_budget: State<'_, TaskBudget>,
    tier: State<'_, HardwareTier>,
    source_id: String,
    folder_id: String,
) -> Result<(), String> {
    use crate::db::repositories::sources as source_repo;

    // Promote to deep tier + reset to pending so the indexer re-runs the file
    // through the LLM IR path. index_tier is sticky (FR-007): once deep, future
    // re-indexes stay deep until the file is removed/re-added as fast.
    sqlx::query("UPDATE sources SET index_tier = 'deep', indexing_status = 'pending' WHERE id = $1")
        .bind(&source_id)
        .execute(pool.inner())
        .await
        .map_err(|e| e.to_string())?;

    let source = source_repo::get_by_id(&pool, &source_id)
        .await
        .map_err(|e| e.to_string())?
        .ok_or("Source not found")?;

    let pool2 = pool.inner().clone();
    let embedder2 = Arc::clone(&embedder);
    let budget2 = task_budget.inner().clone();
    let tier2 = tier.inner().clone();
    let notebook_id = format!("__folder__{}", folder_id);
    let app2 = app.clone();

    tauri::async_runtime::spawn(async move {
        if let Err(e) = crate::services::indexer::index_file(
            &pool2,
            Arc::clone(&embedder2),
            &budget2,
            &tier2,
            &notebook_id,
            &source.file_path,
            Some(app2),
        )
        .await
        {
            log::error!("Folder source deep-index failed for {}: {e}", source.file_path);
        }
    });

    Ok(())
}

/// Remove a single source from a folder index.
/// Wipes the source's chunks and embeddings (frees space, hides from search)
/// but keeps the source row with status "unindexed" so the user can re-index
/// the file later without re-adding the folder.
#[tauri::command]
pub async fn remove_folder_source(
    pool: State<'_, Db>,
    source_id: String,
    _folder_id: String,
) -> Result<(), String> {
    // Single transaction so the writer lock is grabbed once and the three
    // writes are atomic. Prevents "database is locked" when a concurrent
    // indexer batch is holding the writer momentarily.
    let mut tx = pool.begin().await.map_err(|e| e.to_string())?;

    sqlx::query(
        "DELETE FROM chunk_embeddings WHERE chunk_id IN (SELECT id FROM chunks WHERE source_id = $1)",
    )
    .bind(&source_id)
    .execute(&mut *tx)
    .await
    .map_err(|e| e.to_string())?;

    // graph_edges → graph_nodes → chunks must be deleted in dependency order:
    // graph_nodes.chunk_id and graph_edges.{from_node,to_node} are FK NO ACTION,
    // so deleting chunks while a graph_node still references one throws
    // SQLITE_CONSTRAINT_FOREIGNKEY (code 787) and the whole tx rolls back.
    sqlx::query(
        "DELETE FROM graph_edges WHERE from_node IN \
         (SELECT id FROM graph_nodes WHERE chunk_id IN \
          (SELECT id FROM chunks WHERE source_id = $1))",
    )
    .bind(&source_id)
    .execute(&mut *tx)
    .await
    .map_err(|e| e.to_string())?;

    sqlx::query(
        "DELETE FROM graph_edges WHERE to_node IN \
         (SELECT id FROM graph_nodes WHERE chunk_id IN \
          (SELECT id FROM chunks WHERE source_id = $1))",
    )
    .bind(&source_id)
    .execute(&mut *tx)
    .await
    .map_err(|e| e.to_string())?;

    sqlx::query(
        "DELETE FROM graph_nodes WHERE chunk_id IN \
         (SELECT id FROM chunks WHERE source_id = $1)",
    )
    .bind(&source_id)
    .execute(&mut *tx)
    .await
    .map_err(|e| e.to_string())?;

    // chunks.parent_chunk_id is a self-referential FK NO ACTION. SQLite enforces
    // FKs at end-of-statement, so deleting all of a source's chunks in one
    // statement is safe (every parent_chunk_id either points to another row
    // being deleted in the same statement, or to NULL).
    sqlx::query("DELETE FROM chunks WHERE source_id = $1")
        .bind(&source_id)
        .execute(&mut *tx)
        .await
        .map_err(|e| e.to_string())?;

    sqlx::query(
        "UPDATE sources SET indexing_status = 'unindexed', error_reason = NULL, indexed_at = NULL WHERE id = $1",
    )
    .bind(&source_id)
    .execute(&mut *tx)
    .await
    .map_err(|e| e.to_string())?;

    tx.commit().await.map_err(|e| e.to_string())?;

    log::info!("Unindexed folder source {source_id} — chunks cleared, row kept for re-indexing");
    Ok(())
}
