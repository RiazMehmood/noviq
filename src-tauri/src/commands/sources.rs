//! Tauri commands: source management.

use serde::{Deserialize, Serialize};
use crate::db::Db;
use std::sync::Arc;
use tauri::State;

use crate::db::repositories::sources as source_repo;
use crate::services::embedder::EmbedderState;
use crate::services::hardware_detector::HardwareTier;
use crate::services::indexer;
use crate::services::task_budget::TaskBudget;

// F014: SourceDto + the shared `index_sources` orchestration moved to
// `crate::services::indexing` (compiled in both desktop and headless server
// builds). Re-exported here so existing `commands::sources::*` references and
// the Tauri command wrappers below keep working unchanged.
pub use crate::services::indexing::{index_sources, SourceDto};

/// Add one or more source files to a notebook, queueing them for indexing.
#[tauri::command]
pub async fn add_sources(
    app: tauri::AppHandle,
    pool: State<'_, Db>,
    embedder: State<'_, Arc<EmbedderState>>,
    _task_budget: State<'_, TaskBudget>,
    tier: State<'_, HardwareTier>,
    notebook_id: String,
    file_paths: Vec<String>,
) -> Result<Vec<SourceDto>, String> {
    // One implementation, two transports (F014/T024): the desktop command and the
    // headless `POST /api/add_sources` both call `index_sources`. `app` drives
    // the Tauri progress events; headless passes `None` (SSE picks up progress
    // from the indexer broadcast).
    index_sources(
        pool.inner(),
        embedder.inner(),
        tier.inner(),
        &notebook_id,
        &file_paths,
        Some(app),
        None,
        None,
    )
    .await
}

/// Return all sources whose file_path is under the given folder's prefix.
/// Used by the chat scope picker to let users pick specific files within a folder.
#[tauri::command]
pub async fn list_sources_in_folder(
    pool: State<'_, Db>,
    folder_id: String,
) -> Result<Vec<SourceDto>, String> {
    let folder_path: Option<String> = sqlx::query_scalar(
        "SELECT folder_path FROM indexed_folders WHERE id = $1",
    )
    .bind(&folder_id)
    .fetch_optional(pool.inner())
    .await
    .map_err(|e| e.to_string())?;
    let Some(folder_path) = folder_path else { return Ok(vec![]); };
    let prefix = format!("{folder_path}%");
    let rows = sqlx::query_as::<_, source_repo::Source>(
        "SELECT * FROM sources WHERE file_path LIKE $1 AND indexing_status = 'indexed' \
         ORDER BY file_name",
    )
    .bind(&prefix)
    .fetch_all(pool.inner())
    .await
    .map_err(|e| e.to_string())?;
    Ok(rows.into_iter().map(Into::into).collect())
}

#[tauri::command]
pub async fn list_notebook_sources(
    pool: State<'_, Db>,
    notebook_id: String,
) -> Result<Vec<SourceDto>, String> {
    source_repo::list_by_notebook(&pool, &notebook_id)
        .await
        .map(|v| v.into_iter().map(Into::into).collect())
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn remove_source(
    pool: State<'_, Db>,
    notebook_id: String,
    source_id: String,
) -> Result<(), String> {
    source_repo::detach_from_notebook(&pool, &notebook_id, &source_id)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn retry_failed_source(
    pool: State<'_, Db>,
    embedder: State<'_, Arc<EmbedderState>>,
    task_budget: State<'_, TaskBudget>,
    tier: State<'_, HardwareTier>,
    notebook_id: String,
    source_id: String,
) -> Result<(), String> {
    let source = source_repo::get_by_id(&pool, &source_id)
        .await
        .map_err(|e| e.to_string())?
        .ok_or_else(|| "Source not found".to_string())?;

    let pool_ref = pool.inner().clone();
    let embedder_ref = embedder.inner().clone();
    let tier_ref = tier.inner().clone();
    let budget_ref = task_budget.inner().clone();

    tokio::spawn(async move {
        if let Err(e) = indexer::index_file(
            &pool_ref,
            Arc::clone(&embedder_ref),
            &budget_ref,
            &tier_ref,
            &notebook_id,
            &source.file_path,
            None, // retry — no progress events
        )
        .await
        {
            log::error!("Retry indexing failed: {e}");
        }
    });

    Ok(())
}

// ─── KG extraction status ─────────────────────────────────────────────────────

#[derive(Debug, Serialize)]
pub struct KgSourceStatus {
    pub source_id: String,
    pub file_name: String,
    pub ner_status: String,   // "pending" | "running" | "done"
    pub ner_progress: i64,    // 0-100
    pub ner_total: i64,       // total chunks sampled for NER
}

/// Return NER extraction status for all indexed sources (KG is global, not per-notebook).
/// Used by the KG tab to decide whether to show the progress overlay.
#[tauri::command]
pub async fn get_kg_status(
    pool: State<'_, Db>,
) -> Result<Vec<KgSourceStatus>, String> {
    let rows = sqlx::query_as::<_, (String, String, String, i64, i64)>(
        "SELECT id, file_name, \
             CASE WHEN indexing_status != 'indexed' THEN 'pending' ELSE ner_status END, \
             ner_progress, ner_total \
         FROM sources \
         WHERE indexing_status IN ('pending', 'processing', 'indexed') \
           AND indexing_status != 'failed' \
         ORDER BY file_name",
    )
    .fetch_all(pool.inner())
    .await
    .map_err(|e| e.to_string())?;

    Ok(rows
        .into_iter()
        .map(|(source_id, file_name, ner_status, ner_progress, ner_total)| KgSourceStatus {
            source_id,
            file_name,
            ner_status,
            ner_progress,
            ner_total,
        })
        .collect())
}

// ─── Maintenance: clean orphan sources ────────────────────────────────────────

#[derive(Debug, Serialize)]
pub struct CleanOrphansReport {
    pub scanned: i64,
    pub deleted: i64,
    pub deleted_files: Vec<String>,
}

/// Delete `sources` rows whose file no longer exists on disk OR which are not
/// referenced by any folder/notebook. Cascades to chunks/embeddings/entities
/// via FK ON DELETE CASCADE. Added 2026-05-03 (CHANGELOG_DEV.md).
#[tauri::command]
pub async fn clean_orphan_sources(
    pool: State<'_, Db>,
) -> Result<CleanOrphansReport, String> {
    let rows = sqlx::query_as::<_, (String, String)>(
        "SELECT id, file_path FROM sources",
    )
    .fetch_all(pool.inner())
    .await
    .map_err(|e| e.to_string())?;

    let scanned = rows.len() as i64;
    let mut deleted = 0i64;
    let mut deleted_files: Vec<String> = Vec::new();

    // Cache the set of indexed folder paths once — orphan = source not under any
    // indexed folder AND not joined to any notebook AND/OR file missing on disk.
    let folder_paths: Vec<String> = sqlx::query_scalar::<_, String>(
        "SELECT folder_path FROM indexed_folders",
    )
    .fetch_all(pool.inner())
    .await
    .unwrap_or_default();

    for (id, file_path) in rows {
        let missing = !std::path::Path::new(&file_path).exists();
        // Ignore synthetic notebooks (__batch__*, __folder__*) when checking
        // refs — they're orchestration shells, not real user references.
        // Without this, batch + folder deletes leave sources permanently
        // orphaned because the synthetic shells linger in notebook_sources.
        let nb_refs: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM notebook_sources \
             WHERE source_id = $1 \
               AND notebook_id NOT LIKE '\\_\\_batch\\_\\_%' ESCAPE '\\' \
               AND notebook_id NOT LIKE '\\_\\_folder\\_\\_%' ESCAPE '\\'",
        )
        .bind(&id)
        .fetch_one(pool.inner())
        .await
        .unwrap_or(0);
        let under_folder = folder_paths.iter().any(|fp| file_path.starts_with(fp));
        let unreferenced = nb_refs == 0 && !under_folder;

        if missing || unreferenced {
            if let Err(e) = sqlx::query("DELETE FROM sources WHERE id = $1")
                .bind(&id)
                .execute(pool.inner())
                .await
            {
                log::warn!("clean_orphan_sources: delete {id} failed: {e}");
                continue;
            }
            deleted += 1;
            if deleted_files.len() < 50 {
                deleted_files.push(file_path);
            }
        }
    }

    // Purge synthetic notebook shells (__batch__*, __folder__*) that have no
    // remaining sources — they're orchestration scaffolding, not user data.
    let _ = sqlx::query(
        "DELETE FROM notebooks \
         WHERE (id LIKE '\\_\\_batch\\_\\_%' ESCAPE '\\' OR id LIKE '\\_\\_folder\\_\\_%' ESCAPE '\\') \
           AND id NOT IN (SELECT DISTINCT notebook_id FROM notebook_sources)",
    )
    .execute(pool.inner())
    .await;

    // Purge KG residue: entities/relationships have no FK to sources, so
    // deleting sources/chunks cascades only to entity_mentions. Drop entities
    // with zero remaining mentions; relationships then cascade via their FKs.
    let purged_entities: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM entities WHERE id NOT IN (SELECT DISTINCT entity_id FROM entity_mentions)",
    )
    .fetch_one(pool.inner())
    .await
    .unwrap_or(0);
    let _ = sqlx::query(
        "DELETE FROM entities WHERE id NOT IN (SELECT DISTINCT entity_id FROM entity_mentions)",
    )
    .execute(pool.inner())
    .await;

    log::info!(
        "clean_orphan_sources: scanned={scanned} deleted={deleted} purged_entities={purged_entities}"
    );
    Ok(CleanOrphansReport { scanned, deleted, deleted_files })
}

/// Resume NER extraction for any sources stuck in 'pending' or 'running' state.
/// Safe to call while the app is running — resets 'running' → 'pending' first,
/// then spawns recovery tasks for all pending sources.
#[tauri::command]
pub async fn resume_pending_ner(
    pool: State<'_, Db>,
    app: tauri::AppHandle,
) -> Result<u32, String> {
    // Reset any stuck 'running' sources back to 'pending'.
    let _ = sqlx::query(
        "UPDATE sources SET ner_status='pending', ner_progress=0 WHERE ner_status='running'",
    )
    .execute(pool.inner())
    .await;

    let pending: Vec<(String, String)> = sqlx::query_as::<_, (String, String)>(
        "SELECT id, file_name FROM sources WHERE indexing_status='indexed' AND ner_status='pending'",
    )
    .fetch_all(pool.inner())
    .await
    .map_err(|e| e.to_string())?;

    let count = pending.len() as u32;
    if count > 0 {
        let pool2 = pool.inner().clone();
        let app2 = app.clone();
        tauri::async_runtime::spawn(async move {
            for (source_id, file_name) in pending {
                crate::services::indexer::run_ner_for_source(
                    &pool2, &source_id, &file_name, app2.clone(),
                ).await;
            }
        });
    }

    log::info!("resume_pending_ner: kicked off NER for {count} sources");
    Ok(count)
}
