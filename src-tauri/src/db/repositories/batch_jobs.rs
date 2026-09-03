//! Repository: `batch_jobs` and `batch_results` tables.

use anyhow::Result;
use chrono::Utc;
use serde::{Deserialize, Serialize};
use crate::db::Db;
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct BatchJob {
    pub id: String,
    pub folder_path: String,
    pub operation: String,
    pub operation_params_json: Option<String>,
    pub status: String,
    pub total_files: i64,
    pub processed_files: i64,
    pub failed_files: i64,
    pub created_at: i64,
    pub completed_at: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct BatchResult {
    pub id: String,
    pub job_id: String,
    pub source_id: Option<String>,
    pub file_path: String,
    pub status: String,
    pub output_text: Option<String>,
    pub error_reason: Option<String>,
    pub created_at: i64,
}

pub async fn create(
    pool: &Db,
    folder_path: &str,
    operation: &str,
    operation_params_json: Option<&str>,
    total_files: i64,
) -> Result<BatchJob> {
    let id = Uuid::new_v4().to_string();
    let now = Utc::now().timestamp_millis();

    sqlx::query(
        "INSERT INTO batch_jobs \
         (id, folder_path, operation, operation_params_json, status, total_files, \
          processed_files, failed_files, created_at) \
         VALUES ($1, $2, $3, $4, 'queued', $5, 0, 0, $6)",
    )
    .bind(&id)
    .bind(folder_path)
    .bind(operation)
    .bind(operation_params_json)
    .bind(total_files)
    .bind(now)
    .execute(pool)
    .await?;

    Ok(BatchJob {
        id,
        folder_path: folder_path.to_string(),
        operation: operation.to_string(),
        operation_params_json: operation_params_json.map(str::to_string),
        status: "queued".to_string(),
        total_files,
        processed_files: 0,
        failed_files: 0,
        created_at: now,
        completed_at: None,
    })
}

pub async fn get(pool: &Db, id: &str) -> Result<Option<BatchJob>> {
    Ok(
        sqlx::query_as::<_, BatchJob>("SELECT * FROM batch_jobs WHERE id = $1")
            .bind(id)
            .fetch_optional(pool)
            .await?,
    )
}

pub async fn update_status(pool: &Db, id: &str, status: &str) -> Result<()> {
    sqlx::query("UPDATE batch_jobs SET status = $1 WHERE id = $2")
        .bind(status)
        .bind(id)
        .execute(pool)
        .await?;
    Ok(())
}

pub async fn increment_counters(
    pool: &Db,
    id: &str,
    processed: i64,
    failed: i64,
) -> Result<()> {
    sqlx::query(
        "UPDATE batch_jobs \
         SET processed_files = processed_files + $1, failed_files = failed_files + $2 \
         WHERE id = $3",
    )
    .bind(processed)
    .bind(failed)
    .bind(id)
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn set_completed_at(pool: &Db, id: &str) -> Result<()> {
    let now = Utc::now().timestamp_millis();
    sqlx::query("UPDATE batch_jobs SET completed_at = $1 WHERE id = $2")
        .bind(now)
        .bind(id)
        .execute(pool)
        .await?;
    Ok(())
}

pub async fn insert_result(
    pool: &Db,
    job_id: &str,
    source_id: Option<&str>,
    file_path: &str,
    status: &str,
    output_text: Option<&str>,
    error_reason: Option<&str>,
) -> Result<BatchResult> {
    let id = Uuid::new_v4().to_string();
    let now = Utc::now().timestamp_millis();

    sqlx::query(
        "INSERT INTO batch_results \
         (id, job_id, source_id, file_path, status, output_text, error_reason, created_at) \
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8)",
    )
    .bind(&id)
    .bind(job_id)
    .bind(source_id)
    .bind(file_path)
    .bind(status)
    .bind(output_text)
    .bind(error_reason)
    .bind(now)
    .execute(pool)
    .await?;

    Ok(BatchResult {
        id,
        job_id: job_id.to_string(),
        source_id: source_id.map(str::to_string),
        file_path: file_path.to_string(),
        status: status.to_string(),
        output_text: output_text.map(str::to_string),
        error_reason: error_reason.map(str::to_string),
        created_at: now,
    })
}

pub async fn list_results(
    pool: &Db,
    job_id: &str,
    limit: i64,
    offset: i64,
) -> Result<Vec<BatchResult>> {
    Ok(sqlx::query_as::<_, BatchResult>(
        "SELECT * FROM batch_results WHERE job_id = $1 \
         ORDER BY created_at DESC LIMIT $2 OFFSET $3",
    )
    .bind(job_id)
    .bind(limit)
    .bind(offset)
    .fetch_all(pool)
    .await?)
}
