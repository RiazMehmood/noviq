//! Tauri commands: notebook export/import (.echoai archive).

use std::sync::Arc;
use crate::db::Db;
use tauri::State;

use crate::services::{archive, embedder::EmbedderState, hardware_detector::HardwareTier, indexer, task_budget::TaskBudget};

/// Export a notebook to a `.echoai` ZIP archive at the given destination path.
#[tauri::command]
pub async fn export_notebook(
    pool: State<'_, Db>,
    notebook_id: String,
    destination_path: String,
) -> Result<String, String> {
    archive::export_notebook(&pool, &notebook_id, &destination_path)
        .await
        .map_err(|e| e.to_string())?;
    Ok(destination_path)
}

/// Import a `.echoai` archive, create a new notebook, and queue all source files
/// for re-indexing.  Returns the new notebook id.
#[tauri::command]
pub async fn import_notebook(
    app: tauri::AppHandle,
    pool: State<'_, Db>,
    embedder: State<'_, Arc<EmbedderState>>,
    task_budget: State<'_, TaskBudget>,
    tier: State<'_, HardwareTier>,
    archive_path: String,
) -> Result<String, String> {
    let (notebook_id, file_paths) = archive::import_notebook(&pool, &archive_path)
        .await
        .map_err(|e| e.to_string())?;

    // Queue all restored files for indexing in the background.
    for path in file_paths {
        let pool_ref = pool.inner().clone();
        let embedder_ref = embedder.inner().clone();
        let tier_ref = tier.inner().clone();
        let budget_ref = task_budget.inner().clone();
        let nb_id = notebook_id.clone();
        let app_ref = app.clone();
        tokio::spawn(async move {
            if let Err(e) = indexer::index_file(
                &pool_ref,
                Arc::clone(&embedder_ref),
                &budget_ref,
                &tier_ref,
                &nb_id,
                &path,
                Some(app_ref),
            )
            .await
            {
                log::error!("Import re-index failed for {path}: {e}");
            }
        });
    }

    Ok(notebook_id)
}
