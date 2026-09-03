//! Tauri commands: foreground scheduling + index job management.

use std::sync::Arc;
use crate::db::Db;
use tauri::State;
use uuid::Uuid;

use crate::services::scheduler::{Scheduler, WorkloadKind};
use crate::db::repositories::index_jobs as repo;

#[tauri::command]
pub async fn set_foreground(
    scheduler: State<'_, Arc<Scheduler>>,
    kind: WorkloadKind,
) -> Result<(), String> {
    scheduler.set_foreground(kind).await;
    Ok(())
}

#[tauri::command]
pub async fn cancel_index_job(
    pool: State<'_, Db>,
    scheduler: State<'_, Arc<Scheduler>>,
    job_id: String,
    workload_id: String,
) -> Result<(), String> {
    let id = Uuid::parse_str(&workload_id).map_err(|e| e.to_string())?;
    scheduler.request_cancel(id);
    repo::set_job_status(&pool, &job_id, "cancelled", None)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn discard_index_job(
    pool: State<'_, Db>,
    job_id: String,
) -> Result<(), String> {
    repo::delete_job(&pool, &job_id).await.map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn list_active_index_jobs(
    pool: State<'_, Db>,
) -> Result<Vec<repo::IndexJob>, String> {
    repo::list_active_jobs(&pool).await.map_err(|e| e.to_string())
}
