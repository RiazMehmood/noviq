//! Repository for index_jobs + index_job_files. Spec §6.

use anyhow::Result;
use crate::db::Db;
use uuid::Uuid;

#[derive(Clone, Debug, sqlx::FromRow, serde::Serialize)]
pub struct IndexJob {
    pub id: String,
    pub notebook_id: String,
    pub root_path: String,
    pub status: String,
    pub total_files: i64,
    pub processed_files: i64,
    pub started_at: i64,
    pub updated_at: i64,
    pub error: Option<String>,
}

#[derive(Clone, Debug, sqlx::FromRow)]
pub struct IndexJobFile {
    pub job_id: String,
    pub file_path: String,
    pub mtime: i64,
    pub status: String,
    pub chunks_written: i64,
    pub error: Option<String>,
}

pub async fn create_job(
    pool: &Db,
    notebook_id: &str,
    root_path: &str,
    file_list: &[(String, i64)], // (path, mtime)
) -> Result<IndexJob> {
    let id = Uuid::new_v4().to_string();
    let now = chrono::Utc::now().timestamp_millis();
    let total = file_list.len() as i64;
    let mut tx = pool.begin().await?;

    sqlx::query(
        "INSERT INTO index_jobs (id, notebook_id, root_path, status,
         total_files, processed_files, started_at, updated_at)
         VALUES ($1, $2, $3, 'running', $4, 0, $5, $6)",
    )
    .bind(&id).bind(notebook_id).bind(root_path)
    .bind(total).bind(now).bind(now)
    .execute(&mut *tx).await?;

    for (path, mtime) in file_list {
        sqlx::query(
            "INSERT INTO index_job_files (job_id, file_path, mtime, status)
             VALUES ($1, $2, $3, 'pending')",
        ).bind(&id).bind(path).bind(mtime).execute(&mut *tx).await?;
    }
    tx.commit().await?;

    Ok(IndexJob {
        id, notebook_id: notebook_id.into(), root_path: root_path.into(),
        status: "running".into(), total_files: total, processed_files: 0,
        started_at: now, updated_at: now, error: None,
    })
}

pub async fn set_job_status(pool: &Db, job_id: &str, status: &str, error: Option<&str>) -> Result<()> {
    let now = chrono::Utc::now().timestamp_millis();
    sqlx::query("UPDATE index_jobs SET status = $1, error = $2, updated_at = $3 WHERE id = $4")
        .bind(status).bind(error).bind(now).bind(job_id)
        .execute(pool).await?;
    Ok(())
}

pub async fn mark_file_done(tx: &mut sqlx::Transaction<'_, sqlx::Postgres>, job_id: &str, path: &str, chunks: i64) -> Result<()> {
    sqlx::query(
        "UPDATE index_job_files SET status = 'done', chunks_written = $1 WHERE job_id = $2 AND file_path = $3",
    ).bind(chunks).bind(job_id).bind(path).execute(&mut **tx).await?;
    Ok(())
}

pub async fn mark_file_failed(pool: &Db, job_id: &str, path: &str, error: &str) -> Result<()> {
    sqlx::query(
        "UPDATE index_job_files SET status = 'failed', error = $1 WHERE job_id = $2 AND file_path = $3",
    ).bind(error).bind(job_id).bind(path).execute(pool).await?;
    Ok(())
}

pub async fn increment_processed(pool: &Db, job_id: &str) -> Result<()> {
    sqlx::query("UPDATE index_jobs SET processed_files = processed_files + 1, updated_at = $1 WHERE id = $2")
        .bind(chrono::Utc::now().timestamp_millis()).bind(job_id)
        .execute(pool).await?;
    Ok(())
}

pub async fn find_completed_files_for_root(
    pool: &Db,
    notebook_id: &str,
    root_path: &str,
) -> Result<Vec<IndexJobFile>> {
    let rows = sqlx::query_as::<_, IndexJobFile>(
        "SELECT f.* FROM index_job_files f
         JOIN index_jobs j ON j.id = f.job_id
         WHERE j.notebook_id = $1 AND j.root_path = $2 AND f.status = 'done'",
    ).bind(notebook_id).bind(root_path).fetch_all(pool).await?;
    Ok(rows)
}

pub async fn list_active_jobs(pool: &Db) -> Result<Vec<IndexJob>> {
    let rows = sqlx::query_as::<_, IndexJob>(
        "SELECT * FROM index_jobs WHERE status IN ('running','paused') ORDER BY started_at DESC",
    ).fetch_all(pool).await?;
    Ok(rows)
}

pub async fn delete_job(pool: &Db, job_id: &str) -> Result<()> {
    sqlx::query("DELETE FROM index_jobs WHERE id = $1").bind(job_id).execute(pool).await?;
    Ok(())
}

pub async fn reconcile_orphaned_running_jobs(pool: &Db) -> Result<u64> {
    let res = sqlx::query(
        "UPDATE index_jobs SET status = 'paused', updated_at = $1 WHERE status = 'running'",
    ).bind(chrono::Utc::now().timestamp_millis()).execute(pool).await?;
    Ok(res.rows_affected())
}

#[cfg(test)]
mod tests {
    use super::*;

    async fn fresh_pool() -> Db {
        crate::db::test_pool().await
    }

    #[tokio::test]
    async fn create_and_read_job() {
        let pool = fresh_pool().await;
        let files = vec![("/a.pdf".into(), 1i64), ("/b.pdf".into(), 2i64)];
        let j = create_job(&pool, "nb1", "/root", &files).await.unwrap();
        assert_eq!(j.total_files, 2);
        assert_eq!(j.status, "running");
    }

    #[tokio::test]
    async fn reconcile_running_to_paused() {
        let pool = fresh_pool().await;
        let _ = create_job(&pool, "nb1", "/root", &[("/a".into(), 1)]).await.unwrap();
        let n = reconcile_orphaned_running_jobs(&pool).await.unwrap();
        assert_eq!(n, 1);
        let actives = list_active_jobs(&pool).await.unwrap();
        assert_eq!(actives[0].status, "paused");
    }

    #[tokio::test]
    async fn find_completed_files() {
        let pool = fresh_pool().await;
        let j = create_job(&pool, "nb1", "/root", &[
            ("/a".into(), 1), ("/b".into(), 2),
        ]).await.unwrap();
        let mut tx = pool.begin().await.unwrap();
        mark_file_done(&mut tx, &j.id, "/a", 5).await.unwrap();
        tx.commit().await.unwrap();
        let done = find_completed_files_for_root(&pool, "nb1", "/root").await.unwrap();
        assert_eq!(done.len(), 1);
        assert_eq!(done[0].file_path, "/a");
    }
}
