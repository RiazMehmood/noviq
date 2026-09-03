//! Tauri commands: knowledge-graph relation-type preferences.
//!
//! Stores a user-configurable list of snake_case relation verbs in the
//! existing `db_config` key-value table under key `kg_relation_types`.
//! Pass-2 entity extraction reads this list and injects it into the LLM
//! prompt so users can broaden the graph vocabulary (academic, legal,
//! medical, code, etc.) without a rebuild.

use serde::{Deserialize, Serialize};
use crate::db::Db;
use tauri::State;

const DB_KEY: &str = "kg_relation_types";

/// Built-in defaults — mirror the verbs originally hard-coded in the
/// Pass-2 prompt. Kept in sync so a fresh install behaves identically to
/// pre-feature builds.
pub const DEFAULT_RELATIONS: &[&str] = &[
    "works_at",
    "reports_to",
    "part_of",
    "signed_with",
    "owns",
    "authored",
    "located_in",
    "mentions",
];

#[derive(Debug, Serialize, Deserialize)]
pub struct KgPreferences {
    pub relation_types: Vec<String>,
}

/// Load the active relation-type list, falling back to `DEFAULT_RELATIONS`
/// if the key is absent or the stored value is malformed.
pub async fn load_relation_types(pool: &Db) -> Vec<String> {
    let row: Option<(String,)> = sqlx::query_as(
        "SELECT value FROM db_config WHERE key = $1",
    )
    .bind(DB_KEY)
    .fetch_optional(pool)
    .await
    .ok()
    .flatten();

    if let Some((val,)) = row {
        if let Ok(list) = serde_json::from_str::<Vec<String>>(&val) {
            let cleaned: Vec<String> = list
                .into_iter()
                .map(|s| s.trim().to_lowercase())
                .filter(|s| !s.is_empty())
                .collect();
            if !cleaned.is_empty() {
                return cleaned;
            }
        }
    }

    DEFAULT_RELATIONS.iter().map(|s| s.to_string()).collect()
}

fn is_valid_snake_case(s: &str) -> bool {
    !s.is_empty()
        && s.len() <= 40
        && s.chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
        && !s.starts_with('_')
        && !s.ends_with('_')
}

#[tauri::command]
pub async fn get_kg_preferences(
    pool: State<'_, Db>,
) -> Result<KgPreferences, String> {
    let relation_types = load_relation_types(pool.inner()).await;
    Ok(KgPreferences { relation_types })
}

#[tauri::command]
pub async fn set_kg_preferences(
    pool: State<'_, Db>,
    relation_types: Vec<String>,
) -> Result<KgPreferences, String> {
    let mut cleaned: Vec<String> = Vec::new();
    for raw in relation_types {
        let norm = raw.trim().to_lowercase().replace(' ', "_").replace('-', "_");
        if norm.is_empty() {
            continue;
        }
        if !is_valid_snake_case(&norm) {
            return Err(format!(
                "invalid relation type '{raw}' — use snake_case letters, digits, underscores (max 40 chars)"
            ));
        }
        if !cleaned.contains(&norm) {
            cleaned.push(norm);
        }
    }

    if cleaned.is_empty() {
        return Err("at least one relation type is required".into());
    }
    if cleaned.len() > 40 {
        return Err("at most 40 relation types allowed".into());
    }

    let encoded = serde_json::to_string(&cleaned).map_err(|e| e.to_string())?;
    sqlx::query(
        "INSERT INTO db_config (key, value) VALUES ($1, $2) \
         ON CONFLICT(key) DO UPDATE SET value = excluded.value",
    )
    .bind(DB_KEY)
    .bind(&encoded)
    .execute(pool.inner())
    .await
    .map_err(|e| e.to_string())?;

    Ok(KgPreferences {
        relation_types: cleaned,
    })
}
