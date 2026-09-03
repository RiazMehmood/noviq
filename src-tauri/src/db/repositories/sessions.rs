//! Repository: `query_sessions` and `query_messages` tables.

use anyhow::Result;
use chrono::Utc;
use serde::{Deserialize, Serialize};
use crate::db::Db;
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct QuerySession {
    pub id: String,
    pub notebook_id: String,
    pub created_at: i64,
    pub updated_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct QueryMessage {
    pub id: String,
    pub session_id: String,
    pub role: String,
    pub content: String,
    pub citations_json: Option<String>,
    pub tier_used: Option<String>,
    pub is_incomplete: i64,
    pub created_at: i64,
}

pub async fn create(pool: &Db, notebook_id: &str) -> Result<QuerySession> {
    let id = Uuid::new_v4().to_string();
    let now = Utc::now().timestamp_millis();

    sqlx::query(
        "INSERT INTO query_sessions (id, notebook_id, created_at, updated_at) VALUES ($1, $2, $3, $4)",
    )
    .bind(&id)
    .bind(notebook_id)
    .bind(now)
    .bind(now)
    .execute(pool)
    .await?;

    Ok(QuerySession {
        id,
        notebook_id: notebook_id.to_string(),
        created_at: now,
        updated_at: now,
    })
}

pub async fn get(pool: &Db, id: &str) -> Result<Option<QuerySession>> {
    Ok(
        sqlx::query_as::<_, QuerySession>(
            "SELECT * FROM query_sessions WHERE id = $1",
        )
        .bind(id)
        .fetch_optional(pool)
        .await?,
    )
}

pub async fn list_by_notebook(pool: &Db, notebook_id: &str) -> Result<Vec<QuerySession>> {
    Ok(sqlx::query_as::<_, QuerySession>(
        "SELECT * FROM query_sessions WHERE notebook_id = $1 ORDER BY updated_at DESC",
    )
    .bind(notebook_id)
    .fetch_all(pool)
    .await?)
}

pub async fn touch_updated_at(pool: &Db, id: &str) -> Result<()> {
    let now = Utc::now().timestamp_millis();
    sqlx::query("UPDATE query_sessions SET updated_at = $1 WHERE id = $2")
        .bind(now)
        .bind(id)
        .execute(pool)
        .await?;
    Ok(())
}

pub async fn insert_message(
    pool: &Db,
    session_id: &str,
    role: &str,
    content: &str,
    citations_json: Option<&str>,
    tier_used: Option<&str>,
    is_incomplete: bool,
) -> Result<QueryMessage> {
    let id = Uuid::new_v4().to_string();
    let now = Utc::now().timestamp_millis();

    sqlx::query(
        "INSERT INTO query_messages \
         (id, session_id, role, content, citations_json, tier_used, is_incomplete, created_at) \
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8)",
    )
    .bind(&id)
    .bind(session_id)
    .bind(role)
    .bind(content)
    .bind(citations_json)
    .bind(tier_used)
    .bind(is_incomplete as i64)
    .bind(now)
    .execute(pool)
    .await?;

    Ok(QueryMessage {
        id,
        session_id: session_id.to_string(),
        role: role.to_string(),
        content: content.to_string(),
        citations_json: citations_json.map(str::to_string),
        tier_used: tier_used.map(str::to_string),
        is_incomplete: is_incomplete as i64,
        created_at: now,
    })
}

pub async fn list_messages(pool: &Db, session_id: &str) -> Result<Vec<QueryMessage>> {
    Ok(sqlx::query_as::<_, QueryMessage>(
        "SELECT * FROM query_messages WHERE session_id = $1 ORDER BY created_at",
    )
    .bind(session_id)
    .fetch_all(pool)
    .await?)
}
