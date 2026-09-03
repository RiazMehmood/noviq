//! Repository: `notebooks` table.

use anyhow::Result;
use chrono::Utc;
use serde::{Deserialize, Serialize};
use crate::db::Db;
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct Notebook {
    pub id: String,
    pub name: String,
    pub created_at: i64,
    pub updated_at: i64,
}

pub async fn create(pool: &Db, name: &str) -> Result<Notebook> {
    let id = Uuid::new_v4().to_string();
    let now = Utc::now().timestamp_millis();

    sqlx::query(
        "INSERT INTO notebooks (id, name, created_at, updated_at) VALUES ($1, $2, $3, $4)",
    )
    .bind(&id)
    .bind(name)
    .bind(now)
    .bind(now)
    .execute(pool)
    .await?;

    Ok(Notebook { id, name: name.to_string(), created_at: now, updated_at: now })
}

pub async fn get(pool: &Db, id: &str) -> Result<Option<Notebook>> {
    let row = sqlx::query_as::<_, Notebook>("SELECT * FROM notebooks WHERE id = $1")
        .bind(id)
        .fetch_optional(pool)
        .await?;
    Ok(row)
}

pub async fn list(pool: &Db) -> Result<Vec<Notebook>> {
    // Exclude synthetic `__folder__<id>` rows — those are infrastructure
    // stubs that let the folder-index flow satisfy notebook_sources' FK
    // without polluting the user-visible Notebooks list (see
    // folder_repo::insert).
    let rows = sqlx::query_as::<_, Notebook>(
        "SELECT * FROM notebooks \
         WHERE id NOT LIKE '__folder__%' \
           AND id NOT LIKE '__batch__%' \
         ORDER BY updated_at DESC",
    )
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

pub async fn rename(pool: &Db, id: &str, name: &str) -> Result<()> {
    let now = Utc::now().timestamp_millis();
    sqlx::query("UPDATE notebooks SET name = $1, updated_at = $2 WHERE id = $3")
        .bind(name)
        .bind(now)
        .bind(id)
        .execute(pool)
        .await?;
    Ok(())
}

pub async fn delete(pool: &Db, id: &str) -> Result<()> {
    // CASCADE handles notebook_sources → sources (if no other notebook refs).
    sqlx::query("DELETE FROM notebooks WHERE id = $1")
        .bind(id)
        .execute(pool)
        .await?;
    Ok(())
}

pub async fn update_timestamp(pool: &Db, id: &str) -> Result<()> {
    let now = Utc::now().timestamp_millis();
    sqlx::query("UPDATE notebooks SET updated_at = $1 WHERE id = $2")
        .bind(now)
        .bind(id)
        .execute(pool)
        .await?;
    Ok(())
}
