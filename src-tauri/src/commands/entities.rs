//! Tauri commands: entity queries and knowledge graph — T072 (US4).

use serde::{Deserialize, Serialize};
use crate::db::Db;
use tauri::State;

use crate::db::repositories::entities as entity_repo;
use crate::services::knowledge_graph;

// F014: EntityDto moved to `crate::services::indexing` (shared by the headless
// HTTP API). Re-exported so existing `commands::entities::EntityDto` refs work.
pub use crate::services::indexing::EntityDto;

/// Natural-language entity query. Routes to knowledge_graph SQL builder.
/// Target: < 1s (SQL-only path, no LLM).
#[tauri::command]
pub async fn query_entities(
    pool: State<'_, Db>,
    query: String,
    entity_type: Option<String>,
) -> Result<Vec<EntityDto>, String> {
    knowledge_graph::query_entities(&pool, &query, entity_type.as_deref())
        .await
        .map(|v| v.into_iter().map(Into::into).collect())
        .map_err(|e| e.to_string())
}

/// Direct relationships for an entity (1-hop BFS).
#[tauri::command]
pub async fn get_entity_relationships(
    pool: State<'_, Db>,
    entity_id: String,
) -> Result<Vec<knowledge_graph::Relationship>, String> {
    knowledge_graph::get_relationships(&pool, &entity_id)
        .await
        .map_err(|e| e.to_string())
}

/// Source documents that mention an entity (with excerpt).
#[tauri::command]
pub async fn get_entity_sources(
    pool: State<'_, Db>,
    entity_id: String,
) -> Result<Vec<serde_json::Value>, String> {
    // Join mentions → chunks → sources to get rich source info.
    let rows: Vec<(String, String, String, Option<String>)> = sqlx::query_as(
        r#"
        SELECT s.id, s.file_name, c.content, em.context_text
        FROM entity_mentions em
        JOIN chunks c ON c.id = em.chunk_id
        JOIN sources s ON s.id = c.source_id
        WHERE em.entity_id = $1
        LIMIT 20
        "#,
    )
    .bind(&entity_id)
    .fetch_all(pool.inner())
    .await
    .map_err(|e| e.to_string())?;

    Ok(rows
        .into_iter()
        .map(|(sid, fname, content, ctx)| {
            let excerpt: String = content.chars().take(200).collect();
            serde_json::json!({
                "source_id": sid,
                "file_name": fname,
                "excerpt": excerpt,
                "context": ctx,
            })
        })
        .collect())
}

/// Paginated knowledge graph subgraph (≤ 500 entities), optionally filtered by source docs.
#[tauri::command]
pub async fn get_knowledge_graph(
    pool: State<'_, Db>,
    limit: Option<i64>,
    offset: Option<i64>,
    source_ids: Option<Vec<String>>,
) -> Result<serde_json::Value, String> {
    let graph = knowledge_graph::get_subgraph(
        &pool,
        limit.unwrap_or(500),
        offset.unwrap_or(0),
        source_ids.as_deref(),
    )
    .await
    .map_err(|e| e.to_string())?;

    let nodes: Vec<EntityDto> = graph.nodes.into_iter().map(Into::into).collect();

    Ok(serde_json::json!({
        "nodes": nodes,
        "edges": graph.edges,
    }))
}
