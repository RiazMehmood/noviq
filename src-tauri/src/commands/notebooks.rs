//! Tauri commands: notebook CRUD.

use serde::{Deserialize, Serialize};
use crate::db::Db;
use tauri::State;

use crate::db::repositories::{audit, notebooks as nb_repo};

#[derive(Debug, Serialize, Deserialize)]
pub struct NotebookDto {
    pub id: String,
    pub name: String,
    pub created_at: i64,
    pub updated_at: i64,
}

impl From<nb_repo::Notebook> for NotebookDto {
    fn from(n: nb_repo::Notebook) -> Self {
        NotebookDto {
            id: n.id,
            name: n.name,
            created_at: n.created_at,
            updated_at: n.updated_at,
        }
    }
}

#[tauri::command]
pub async fn create_notebook(
    pool: State<'_, Db>,
    name: String,
) -> Result<NotebookDto, String> {
    let nb = nb_repo::create(&pool, &name)
        .await
        .map_err(|e| e.to_string())?;
    Ok(nb.into())
}

#[tauri::command]
pub async fn list_notebooks(pool: State<'_, Db>) -> Result<Vec<NotebookDto>, String> {
    let notebooks = nb_repo::list(&pool)
        .await
        .map_err(|e| e.to_string())?;
    Ok(notebooks.into_iter().map(Into::into).collect())
}

#[tauri::command]
pub async fn get_notebook(
    pool: State<'_, Db>,
    id: String,
) -> Result<Option<NotebookDto>, String> {
    nb_repo::get(&pool, &id)
        .await
        .map(|opt| opt.map(Into::into))
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn rename_notebook(
    pool: State<'_, Db>,
    id: String,
    name: String,
) -> Result<(), String> {
    nb_repo::rename(&pool, &id, &name)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn delete_notebook(
    pool: State<'_, Db>,
    id: String,
) -> Result<(), String> {
    // Collect sources linked only to this notebook before deletion,
    // so we can clean up their chunks and embeddings after.
    let orphan_ids: Vec<String> = sqlx::query_scalar(
        "SELECT source_id FROM notebook_sources
         WHERE notebook_id = $1
           AND source_id NOT IN (
               SELECT source_id FROM notebook_sources WHERE notebook_id != $2
           )",
    )
    .bind(&id)
    .bind(&id)
    .fetch_all(&*pool)
    .await
    .unwrap_or_default();

    nb_repo::delete(&pool, &id)
        .await
        .map_err(|e| e.to_string())?;

    // Delete orphaned sources (cascade removes their chunks + embeddings).
    for source_id in &orphan_ids {
        let _ = sqlx::query("DELETE FROM sources WHERE id = $1")
            .bind(source_id)
            .execute(&*pool)
            .await;
    }
    if !orphan_ids.is_empty() {
        log::info!(
            "delete_notebook {id}: removed {} orphaned source(s)",
            orphan_ids.len()
        );
        // Purge KG entities with no remaining mentions (entities have no FK to
        // sources/chunks). Relationships cascade via their FKs to entities.
        let _ = sqlx::query(
            "DELETE FROM entities WHERE id NOT IN (SELECT DISTINCT entity_id FROM entity_mentions)",
        )
        .execute(&*pool)
        .await;
    }

    audit::insert_event(&pool, "data_deleted", Some(&id), Some(r#"{"type":"notebook"}"#))
        .await
        .map_err(|e| e.to_string())?;

    Ok(())
}
