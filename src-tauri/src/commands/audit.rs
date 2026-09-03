//! Tauri commands: audit log.

use serde::{Deserialize, Serialize};
use crate::db::Db;
use tauri::State;

use crate::db::repositories::audit as audit_repo;

#[derive(Debug, Serialize, Deserialize)]
pub struct AuditEventDto {
    pub id: String,
    pub event_type: String,
    pub entity_id: Option<String>,
    pub details_json: Option<String>,
    pub created_at: i64,
}

#[tauri::command]
pub async fn get_audit_log(
    pool: State<'_, Db>,
    limit: Option<i64>,
    offset: Option<i64>,
) -> Result<Vec<AuditEventDto>, String> {
    let rows = audit_repo::list(&pool, limit.unwrap_or(50), offset.unwrap_or(0))
        .await
        .map_err(|e| e.to_string())?;

    Ok(rows
        .into_iter()
        .map(|e| AuditEventDto {
            id: e.id,
            event_type: e.event_type,
            entity_id: e.entity_id,
            details_json: e.details_json,
            created_at: e.created_at,
        })
        .collect())
}
