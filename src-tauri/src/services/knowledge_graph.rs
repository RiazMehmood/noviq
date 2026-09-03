//! Knowledge graph — T071 (US4).
//!
//! Entity query builder (natural-language → SQL) and relationship traversal.

use anyhow::Result;
use crate::db::Db;

use crate::db::repositories::entities as entity_repo;

pub use entity_repo::Entity;

// ─── Entity query ─────────────────────────────────────────────────────────────

/// Parse a natural-language entity query and map to SQL filters.
///
/// Patterns:
///   - "emails from acme" → type=email, value LIKE "%acme%"
///   - "people named John" → type=person, value LIKE "%John%"
///   - "decisions about budget" → type=decision, value LIKE "%budget%"
///   - anything else → value FTS5 search
pub async fn query_entities(
    pool: &Db,
    query: &str,
    entity_type: Option<&str>,
) -> Result<Vec<Entity>> {
    // Resolve entity type filter.
    let etype = entity_type
        .map(|t| t.to_string())
        .or_else(|| detect_type_hint(query));

    // Value search fragment (strip type keywords from query).
    let value_term = strip_type_keywords(query);

    if value_term.is_empty() && etype.is_none() {
        // Return top entities by source count when no filter given.
        return entity_repo::list_top(pool, 100).await;
    }

    entity_repo::search_filtered(pool, &value_term, etype.as_deref()).await
}

// ─── Relationship traversal ───────────────────────────────────────────────────

#[derive(Debug, serde::Serialize, serde::Deserialize, Clone)]
pub struct Relationship {
    pub id: String,
    pub from_entity_id: String,
    pub to_entity_id: String,
    pub relation_type: String,
    pub created_at: i64,
}

/// Return all direct relationships for an entity (1-hop BFS).
pub async fn get_relationships(
    pool: &Db,
    entity_id: &str,
) -> Result<Vec<Relationship>> {
    let rows = sqlx::query_as::<_, entity_repo::EntityRelationship>(
        r#"
        SELECT id, from_entity_id, to_entity_id, relationship_type, source_chunk_id, created_at
        FROM entity_relationships
        WHERE from_entity_id = $1 OR to_entity_id = $2
        LIMIT 200
        "#,
    )
    .bind(entity_id)
    .bind(entity_id)
    .fetch_all(pool)
    .await?;

    Ok(rows
        .into_iter()
        .map(|r| Relationship {
            id: r.id,
            from_entity_id: r.from_entity_id,
            to_entity_id: r.to_entity_id,
            relation_type: r.relation_type,
            created_at: r.created_at,
        })
        .collect())
}

/// Return source documents that mention an entity.
pub async fn get_entity_sources(
    pool: &Db,
    entity_id: &str,
) -> Result<Vec<entity_repo::EntityMention>> {
    entity_repo::get_mentions(pool, entity_id).await
}

// ─── Subgraph fetch ───────────────────────────────────────────────────────────

#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub struct KnowledgeGraph {
    pub nodes: Vec<Entity>,
    pub edges: Vec<Relationship>,
}

/// Fetch a paginated subgraph (≤ 500 entities), optionally scoped to specific sources.
pub async fn get_subgraph(
    pool: &Db,
    limit: i64,
    offset: i64,
    source_ids: Option<&[String]>,
) -> Result<KnowledgeGraph> {
    let limit = limit.min(500);
    let nodes = match source_ids {
        Some(ids) if !ids.is_empty() => {
            entity_repo::list_paginated_by_sources(pool, ids, limit, offset).await?
        }
        _ => entity_repo::list_paginated(pool, limit, offset).await?,
    };

    if nodes.is_empty() {
        return Ok(KnowledgeGraph { nodes, edges: vec![] });
    }

    // Fetch edges involving these nodes.
    let node_ids: Vec<String> = nodes.iter().map(|n| n.id.clone()).collect();
    let edges = get_edges_for_nodes(pool, &node_ids).await?;

    Ok(KnowledgeGraph { nodes, edges })
}

// ─── Private helpers ──────────────────────────────────────────────────────────

fn detect_type_hint(query: &str) -> Option<String> {
    let lower = query.to_lowercase();
    if lower.contains("email") { return Some("email".into()); }
    if lower.contains("person") || lower.contains("people") || lower.contains("named") {
        return Some("person".into());
    }
    if lower.contains("org") || lower.contains("company") || lower.contains("organisation") {
        return Some("org".into());
    }
    if lower.contains("project") { return Some("project".into()); }
    if lower.contains("decision") || lower.contains("decided") { return Some("decision".into()); }
    if lower.contains("date") || lower.contains("when") { return Some("date".into()); }
    if lower.contains("amount") || lower.contains("budget") || lower.contains("cost") {
        return Some("amount".into());
    }
    None
}

fn strip_type_keywords(query: &str) -> String {
    let stop_words = [
        "emails", "email", "from", "people", "person", "named",
        "org", "company", "project", "decision", "about", "for",
        "date", "when", "amount", "budget", "cost", "all",
    ];
    query
        .split_whitespace()
        .filter(|w| !stop_words.contains(&w.to_lowercase().as_str()))
        .collect::<Vec<_>>()
        .join(" ")
}

async fn get_edges_for_nodes(
    pool: &Db,
    node_ids: &[String],
) -> Result<Vec<Relationship>> {
    if node_ids.is_empty() {
        return Ok(vec![]);
    }
    // Build parameterised IN clause.
    let ph1 = crate::db::pg_in(node_ids.len(), 1);
    let ph2 = crate::db::pg_in(node_ids.len(), node_ids.len() + 1);
    let sql = format!(
        r#"
        SELECT id, from_entity_id, to_entity_id, relationship_type, source_chunk_id, created_at
        FROM entity_relationships
        WHERE from_entity_id IN ({ph1}) OR to_entity_id IN ({ph2})
        LIMIT 2000
        "#,
    );
    let mut q = sqlx::query_as::<_, entity_repo::EntityRelationship>(&sql);
    // Bind twice (for IN clause on both sides).
    for id in node_ids { q = q.bind(id); }
    for id in node_ids { q = q.bind(id); }

    let rows = q.fetch_all(pool).await?;
    Ok(rows
        .into_iter()
        .map(|r| Relationship {
            id: r.id,
            from_entity_id: r.from_entity_id,
            to_entity_id: r.to_entity_id,
            relation_type: r.relation_type,
            created_at: r.created_at,
        })
        .collect())
}
