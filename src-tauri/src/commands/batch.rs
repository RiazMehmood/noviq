//! Tauri commands: batch processing (US5) — T079.

use std::sync::Arc;

use serde::{Deserialize, Serialize};
use crate::db::Db;
use tauri::{AppHandle, State};

use crate::{
    db::repositories::batch_jobs as batch_repo,
    services::{
        batch_processor::{self, BatchHandleMap},
        embedder::EmbedderState,
        hardware_detector::HardwareTier,
        llm::ModelSlot,
        scheduler::Scheduler,
        task_budget::TaskBudget,
    },
};

#[derive(Debug, Serialize, Deserialize)]
pub struct BatchJobDto {
    pub id: String,
    pub folder_path: String,
    pub operation: String,
    pub status: String,
    pub total_files: i64,
    pub processed_files: i64,
    pub failed_files: i64,
    pub created_at: i64,
    pub completed_at: Option<i64>,
}

impl From<batch_repo::BatchJob> for BatchJobDto {
    fn from(j: batch_repo::BatchJob) -> Self {
        BatchJobDto {
            id: j.id,
            folder_path: j.folder_path,
            operation: j.operation,
            status: j.status,
            total_files: j.total_files,
            processed_files: j.processed_files,
            failed_files: j.failed_files,
            created_at: j.created_at,
            completed_at: j.completed_at,
        }
    }
}

#[tauri::command]
pub async fn create_batch_job(
    app: AppHandle,
    pool: State<'_, Db>,
    embedder: State<'_, Arc<EmbedderState>>,
    model_slot: State<'_, Arc<ModelSlot>>,
    tier: State<'_, HardwareTier>,
    task_budget: State<'_, TaskBudget>,
    scheduler: State<'_, Arc<Scheduler>>,
    handle_map: State<'_, Arc<BatchHandleMap>>,
    folder_path: String,
    operation: String,
    operation_params_json: Option<String>,
) -> Result<BatchJobDto, String> {
    if !std::path::Path::new(&folder_path).is_dir() {
        return Err(format!("Directory not found: {folder_path}"));
    }

    // Count files in folder for initial estimate.
    let total = std::fs::read_dir(&folder_path)
        .map(|rd| rd.count() as i64)
        .unwrap_or(0);

    let job = batch_repo::create(
        &pool,
        &folder_path,
        &operation,
        operation_params_json.as_deref(),
        total,
    )
    .await
    .map_err(|e| e.to_string())?;

    // T078: Queue batch job in background processor.
    batch_processor::spawn_batch_job(
        app,
        pool.inner().clone(),
        Arc::clone(&embedder),
        Arc::clone(&model_slot),
        tier.inner().clone(),
        task_budget.inner().clone(),
        Arc::clone(&scheduler),
        Arc::clone(&handle_map),
        job.id.clone(),
        folder_path,
        operation,
        operation_params_json,
    );

    Ok(job.into())
}

#[tauri::command]
pub async fn get_batch_job(
    pool: State<'_, Db>,
    job_id: String,
) -> Result<Option<BatchJobDto>, String> {
    batch_repo::get(&pool, &job_id)
        .await
        .map(|opt| opt.map(Into::into))
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn get_batch_results(
    pool: State<'_, Db>,
    job_id: String,
    limit: Option<i64>,
    offset: Option<i64>,
) -> Result<Vec<batch_repo::BatchResult>, String> {
    batch_repo::list_results(&pool, &job_id, limit.unwrap_or(50), offset.unwrap_or(0))
        .await
        .map_err(|e| e.to_string())
}

/// Export all results for a batch job to a single TXT file at `destination_path`.
#[tauri::command]
pub async fn export_batch_results(
    pool: State<'_, Db>,
    job_id: String,
    destination_path: String,
) -> Result<(), String> {
    let job = batch_repo::get(&pool, &job_id)
        .await
        .map_err(|e| e.to_string())?
        .ok_or_else(|| format!("batch job {job_id} not found"))?;

    let results = batch_repo::list_results(&pool, &job_id, 10_000, 0)
        .await
        .map_err(|e| e.to_string())?;

    let mut out = String::new();
    out.push_str(&format!("Batch Results — {}\n", job.operation.replace('_', " ").to_uppercase()));
    out.push_str(&format!("Folder: {}\n", job.folder_path));
    out.push_str(&format!(
        "Total: {}  |  Processed: {}  |  Failed: {}\n",
        job.total_files, job.processed_files, job.failed_files
    ));
    out.push_str(&"─".repeat(72));
    out.push('\n');

    for r in &results {
        let file_name = std::path::Path::new(&r.file_path)
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or(&r.file_path);

        out.push_str(&format!("\n[{}] {}\n", r.status.to_uppercase(), file_name));
        if let Some(text) = &r.output_text {
            out.push_str(text);
        } else if let Some(err) = &r.error_reason {
            out.push_str(&format!("ERROR: {err}"));
        }
        out.push('\n');
        out.push_str(&"─".repeat(72));
        out.push('\n');
    }

    std::fs::write(&destination_path, out).map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn pause_batch_job(
    handle_map: State<'_, Arc<BatchHandleMap>>,
    job_id: String,
) -> Result<(), String> {
    match handle_map.get(&job_id) {
        Some(h) => { h.pause_job(); Ok(()) }
        None => Err(format!("No active batch job: {job_id}")),
    }
}

#[tauri::command]
pub async fn resume_batch_job(
    handle_map: State<'_, Arc<BatchHandleMap>>,
    job_id: String,
) -> Result<(), String> {
    match handle_map.get(&job_id) {
        Some(h) => { h.resume_job(); Ok(()) }
        None => Err(format!("No active batch job: {job_id}")),
    }
}

#[tauri::command]
pub async fn cancel_batch_job(
    handle_map: State<'_, Arc<BatchHandleMap>>,
    job_id: String,
) -> Result<(), String> {
    match handle_map.get(&job_id) {
        Some(h) => { h.cancel.cancel(); Ok(()) }
        None => Err(format!("No active batch job: {job_id}")),
    }
}
