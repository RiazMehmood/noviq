//! Tauri commands: global semantic search (US2).

use serde::{Deserialize, Serialize};
use crate::db::Db;
use std::sync::Arc;
use tauri::State;

use crate::services::embedder::EmbedderState;

#[derive(Debug, Serialize, Deserialize)]
pub struct SearchResult {
    pub source_id: String,
    pub chunk_id: String,
    pub file_path: String,
    pub file_name: String,
    pub excerpt: String,
    pub last_modified: i64,
    pub relevance_score: f64,
    pub page_number: Option<i64>,
}

#[tauri::command]
pub async fn global_search(
    pool: State<'_, Db>,
    embedder: State<'_, Arc<EmbedderState>>,
    query: String,
) -> Result<Vec<SearchResult>, String> {
    let query_embedding = embedder.embed_one(&query).map_err(|e| e.to_string())?;
    let embedding_json = serde_json::to_string(&query_embedding).map_err(|e| e.to_string())?;

    let d = crate::db::schema::embedding_dim();
    let rows: Vec<(String, String, String, String, String, i64, f64, Option<i64>)> =
        sqlx::query_as(&format!(
            "SELECT c.id, c.source_id, s.file_path, s.file_name, c.content, \
                    s.last_modified, (ce.embedding::halfvec({d}) <=> $1::vector::halfvec({d})) as distance, \
                    c.page_number \
             FROM chunk_embeddings ce \
             JOIN chunks c ON c.id = ce.chunk_id \
             JOIN sources s ON s.id = c.source_id \
             WHERE s.indexing_status = 'indexed' \
               AND EXISTS ( \
                 SELECT 1 FROM indexed_folders f \
                 WHERE s.file_path LIKE (f.folder_path || '%') \
               ) \
             ORDER BY distance ASC LIMIT 20",
        ))
        .bind(&embedding_json)
        .fetch_all(pool.inner())
        .await
        .map_err(|e| e.to_string())?;

    Ok(rows
        .into_iter()
        .map(|(chunk_id, source_id, file_path, file_name, content, last_modified, distance, page)| {
            SearchResult {
                source_id,
                chunk_id,
                file_path,
                file_name,
                excerpt: content,
                last_modified,
                relevance_score: 1.0 - distance,
                page_number: page,
            }
        })
        .collect())
}
