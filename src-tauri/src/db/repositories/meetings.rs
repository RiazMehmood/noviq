//! Repository: `meetings` and `meeting_segments`.

use anyhow::Result;
use chrono::Utc;
use serde::{Deserialize, Serialize};
use crate::db::Db;

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct Meeting {
    pub id: i64,
    pub title: String,
    pub started_at: String,
    pub ended_at: Option<String>,
    pub language: String,
    pub model_path: String,
    pub status: String,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct MeetingSegment {
    pub id: i64,
    pub meeting_id: i64,
    pub t0_ms: i64,
    pub t1_ms: i64,
    pub text: String,
    pub created_at: String,
}

pub async fn start(
    pool: &Db,
    title: &str,
    language: &str,
    model_path: &str,
) -> Result<i64> {
    let started_at = Utc::now().to_rfc3339();
    let row = sqlx::query(
        "INSERT INTO meetings (title, started_at, language, model_path, status)
         VALUES ($1, $2, $3, $4, 'active') RETURNING id",
    )
    .bind(title).bind(&started_at).bind(language).bind(model_path)
    .fetch_one(pool).await?;
    use sqlx::Row;
    Ok(row.get::<i64, _>("id"))
}

pub async fn stop(pool: &Db, id: i64) -> Result<()> {
    let ended_at = Utc::now().to_rfc3339();
    sqlx::query(
        "UPDATE meetings SET ended_at = $1, status = 'stopped'
         WHERE id = $2 AND status = 'active'",
    )
    .bind(&ended_at).bind(id)
    .execute(pool).await?;
    Ok(())
}

pub async fn mark_error(pool: &Db, id: i64, err: &str) -> Result<()> {
    let ended_at = Utc::now().to_rfc3339();
    sqlx::query(
        "UPDATE meetings SET ended_at = $1, status = 'error', error = $2 WHERE id = $3",
    )
    .bind(&ended_at).bind(err).bind(id)
    .execute(pool).await?;
    Ok(())
}

pub async fn append_segment(
    pool: &Db,
    meeting_id: i64,
    t0_ms: i64,
    t1_ms: i64,
    text: &str,
) -> Result<i64> {
    let row = sqlx::query(
        "INSERT INTO meeting_segments (meeting_id, t0_ms, t1_ms, text)
         VALUES ($1, $2, $3, $4) RETURNING id",
    )
    .bind(meeting_id).bind(t0_ms).bind(t1_ms).bind(text)
    .fetch_one(pool).await?;
    use sqlx::Row;
    Ok(row.get::<i64, _>("id"))
}

pub async fn get(pool: &Db, id: i64) -> Result<Option<Meeting>> {
    Ok(sqlx::query_as::<_, Meeting>("SELECT * FROM meetings WHERE id = $1")
        .bind(id).fetch_optional(pool).await?)
}

pub async fn list_segments(pool: &Db, meeting_id: i64) -> Result<Vec<MeetingSegment>> {
    Ok(sqlx::query_as::<_, MeetingSegment>(
        "SELECT * FROM meeting_segments WHERE meeting_id = $1 ORDER BY t0_ms",
    )
    .bind(meeting_id).fetch_all(pool).await?)
}

#[cfg(test)]
mod tests {
    use super::*;

    async fn pool() -> Db {
        crate::db::test_pool().await
    }

    #[tokio::test]
    async fn start_append_stop_roundtrip() {
        let p = pool().await;
        let id = start(&p, "Test", "en", "/abs/model.bin").await.unwrap();
        assert!(id > 0);

        let s1 = append_segment(&p, id, 0, 1500, "hello world").await.unwrap();
        let s2 = append_segment(&p, id, 1500, 3200, "second segment").await.unwrap();
        assert!(s2 > s1);

        let segs = list_segments(&p, id).await.unwrap();
        assert_eq!(segs.len(), 2);
        assert_eq!(segs[0].text, "hello world");
        assert_eq!(segs[1].t0_ms, 1500);

        stop(&p, id).await.unwrap();
        let m = get(&p, id).await.unwrap().unwrap();
        assert_eq!(m.status, "stopped");
        assert!(m.ended_at.is_some());
    }

    #[tokio::test]
    async fn mark_error_sets_status_and_text() {
        let p = pool().await;
        let id = start(&p, "T", "en", "/abs/m").await.unwrap();
        mark_error(&p, id, "boom").await.unwrap();
        let m = get(&p, id).await.unwrap().unwrap();
        assert_eq!(m.status, "error");
        assert_eq!(m.error.as_deref(), Some("boom"));
    }
}
