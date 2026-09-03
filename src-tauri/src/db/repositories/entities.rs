//! Repository: `entities`, `entity_mentions`, `entity_relationships` tables.

use anyhow::Result;
use chrono::Utc;
use serde::{Deserialize, Serialize};
use crate::db::Db;
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct Entity {
    pub id: String,
    pub canonical_value: String,
    pub entity_type: String,
    pub normalized_value: Option<String>,
    pub source_count: i64,
    pub created_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct EntityMention {
    pub entity_id: String,
    pub chunk_id: String,
    pub context_text: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct EntityRelationship {
    pub id: String,
    pub from_entity_id: String,
    pub to_entity_id: String,
    /// Alias exposed to the knowledge graph layer as `relation_type`.
    #[sqlx(rename = "relationship_type")]
    pub relation_type: String,
    pub source_chunk_id: Option<String>,
    pub created_at: i64,
}

/// Upsert an entity. Returns the existing or newly created entity.
pub async fn upsert(
    pool: &Db,
    canonical_value: &str,
    entity_type: &str,
    normalized_value: Option<&str>,
) -> Result<Entity> {
    let now = Utc::now().timestamp_millis();
    let id = Uuid::new_v4().to_string();

    sqlx::query(
        "INSERT INTO entities (id, canonical_value, entity_type, normalized_value, source_count, created_at) \
         VALUES ($1, $2, $3, $4, 0, $5) \
         ON CONFLICT(canonical_value, entity_type) DO NOTHING",
    )
    .bind(&id)
    .bind(canonical_value)
    .bind(entity_type)
    .bind(normalized_value)
    .bind(now)
    .execute(pool)
    .await?;

    let entity = sqlx::query_as::<_, Entity>(
        "SELECT * FROM entities WHERE canonical_value = $1 AND entity_type = $2",
    )
    .bind(canonical_value)
    .bind(entity_type)
    .fetch_one(pool)
    .await?;

    Ok(entity)
}

pub async fn get(pool: &Db, id: &str) -> Result<Option<Entity>> {
    Ok(
        sqlx::query_as::<_, Entity>("SELECT * FROM entities WHERE id = $1")
            .bind(id)
            .fetch_optional(pool)
            .await?,
    )
}

pub async fn list_by_type(pool: &Db, entity_type: &str) -> Result<Vec<Entity>> {
    Ok(sqlx::query_as::<_, Entity>(
        "SELECT * FROM entities WHERE entity_type = $1 ORDER BY source_count DESC",
    )
    .bind(entity_type)
    .fetch_all(pool)
    .await?)
}

pub async fn search_by_value(pool: &Db, query: &str) -> Result<Vec<Entity>> {
    let pattern = format!("%{}%", query);
    Ok(sqlx::query_as::<_, Entity>(
        "SELECT * FROM entities WHERE canonical_value LIKE $1 ORDER BY source_count DESC LIMIT 50",
    )
    .bind(pattern)
    .fetch_all(pool)
    .await?)
}

pub async fn increment_source_count(pool: &Db, id: &str) -> Result<()> {
    sqlx::query("UPDATE entities SET source_count = source_count + 1 WHERE id = $1")
        .bind(id)
        .execute(pool)
        .await?;
    Ok(())
}

pub async fn insert_mention(
    pool: &Db,
    entity_id: &str,
    chunk_id: &str,
    context_text: Option<&str>,
) -> Result<()> {
    sqlx::query(
        "INSERT INTO entity_mentions (entity_id, chunk_id, context_text) VALUES ($1, $2, $3) ON CONFLICT (entity_id, chunk_id) DO NOTHING",
    )
    .bind(entity_id)
    .bind(chunk_id)
    .bind(context_text)
    .execute(pool)
    .await?;
    Ok(())
}

/// Insert a relationship only if an identical (from, to, type, chunk) row does
/// not already exist. Returns `true` if a new row was inserted.
///
/// Prevents duplicate edges accumulating when a file is re-indexed — Pass 2
/// would otherwise emit the same edge for the same chunk every run.
pub async fn insert_relationship_if_new(
    pool: &Db,
    from_entity_id: &str,
    to_entity_id: &str,
    relationship_type: &str,
    source_chunk_id: Option<&str>,
) -> Result<bool> {
    let existing: Option<String> = sqlx::query_scalar(
        "SELECT id FROM entity_relationships \
         WHERE from_entity_id = $1 AND to_entity_id = $2 AND relationship_type = $3 \
           AND ((source_chunk_id IS NULL AND $4 IS NULL) OR source_chunk_id = $5) \
         LIMIT 1",
    )
    .bind(from_entity_id)
    .bind(to_entity_id)
    .bind(relationship_type)
    .bind(source_chunk_id)
    .bind(source_chunk_id)
    .fetch_optional(pool)
    .await?;

    if existing.is_some() {
        return Ok(false);
    }

    insert_relationship(pool, from_entity_id, to_entity_id, relationship_type, source_chunk_id).await?;
    Ok(true)
}

pub async fn insert_relationship(
    pool: &Db,
    from_entity_id: &str,
    to_entity_id: &str,
    relationship_type: &str,
    source_chunk_id: Option<&str>,
) -> Result<EntityRelationship> {
    let id = Uuid::new_v4().to_string();
    let now = Utc::now().timestamp_millis();

    sqlx::query(
        "INSERT INTO entity_relationships \
         (id, from_entity_id, to_entity_id, relationship_type, source_chunk_id, created_at) \
         VALUES ($1, $2, $3, $4, $5, $6)",
    )
    .bind(&id)
    .bind(from_entity_id)
    .bind(to_entity_id)
    .bind(relationship_type)
    .bind(source_chunk_id)
    .bind(now)
    .execute(pool)
    .await?;

    Ok(EntityRelationship {
        id,
        from_entity_id: from_entity_id.to_string(),
        to_entity_id: to_entity_id.to_string(),
        relation_type: relationship_type.to_string(),
        source_chunk_id: source_chunk_id.map(str::to_string),
        created_at: now,
    })
}

pub async fn get_relationships(pool: &Db, entity_id: &str) -> Result<Vec<EntityRelationship>> {
    Ok(sqlx::query_as::<_, EntityRelationship>(
        "SELECT id, from_entity_id, to_entity_id, relationship_type, source_chunk_id, created_at \
         FROM entity_relationships WHERE from_entity_id = $1 OR to_entity_id = $2",
    )
    .bind(entity_id)
    .bind(entity_id)
    .fetch_all(pool)
    .await?)
}

/// List top N entities by source count.
pub async fn list_top(pool: &Db, limit: i64) -> Result<Vec<Entity>> {
    Ok(sqlx::query_as::<_, Entity>(
        "SELECT * FROM entities ORDER BY source_count DESC LIMIT $1",
    )
    .bind(limit)
    .fetch_all(pool)
    .await?)
}

/// Paginated entity list (for knowledge graph subgraph).
pub async fn list_paginated(pool: &Db, limit: i64, offset: i64) -> Result<Vec<Entity>> {
    Ok(sqlx::query_as::<_, Entity>(
        "SELECT * FROM entities ORDER BY source_count DESC LIMIT $1 OFFSET $2",
    )
    .bind(limit)
    .bind(offset)
    .fetch_all(pool)
    .await?)
}

/// Paginated entities scoped to specific source documents.
pub async fn list_paginated_by_sources(
    pool: &Db,
    source_ids: &[String],
    limit: i64,
    offset: i64,
) -> Result<Vec<Entity>> {
    let n = source_ids.len();
    let placeholders = crate::db::pg_in(n, 1);
    let (p_limit, p_offset) = (n + 1, n + 2);
    let sql = format!(
        "SELECT DISTINCT e.id, e.canonical_value, e.entity_type, e.normalized_value, \
                        e.source_count, e.created_at \
         FROM entities e \
         JOIN entity_mentions em ON em.entity_id = e.id \
         JOIN chunks c ON c.id = em.chunk_id \
         WHERE c.source_id IN ({placeholders}) \
         ORDER BY e.source_count DESC \
         LIMIT ${p_limit} OFFSET ${p_offset}",
    );
    let mut q = sqlx::query_as::<_, Entity>(&sql);
    for id in source_ids {
        q = q.bind(id);
    }
    q = q.bind(limit).bind(offset);
    Ok(q.fetch_all(pool).await?)
}

/// Search entities with optional type filter.
pub async fn search_filtered(
    pool: &Db,
    value_term: &str,
    entity_type: Option<&str>,
) -> Result<Vec<Entity>> {
    let pattern = format!("%{}%", value_term);
    match entity_type {
        Some(et) => Ok(sqlx::query_as::<_, Entity>(
            "SELECT * FROM entities WHERE canonical_value LIKE $1 AND entity_type = $2 \
             ORDER BY source_count DESC LIMIT 100",
        )
        .bind(&pattern)
        .bind(et)
        .fetch_all(pool)
        .await?),
        None => Ok(sqlx::query_as::<_, Entity>(
            "SELECT * FROM entities WHERE canonical_value LIKE $1 \
             ORDER BY source_count DESC LIMIT 100",
        )
        .bind(&pattern)
        .fetch_all(pool)
        .await?),
    }
}

/// Get all chunk mentions for an entity.
pub async fn get_mentions(pool: &Db, entity_id: &str) -> Result<Vec<EntityMention>> {
    Ok(sqlx::query_as::<_, EntityMention>(
        "SELECT entity_id, chunk_id, context_text FROM entity_mentions WHERE entity_id = $1 LIMIT 200",
    )
    .bind(entity_id)
    .fetch_all(pool)
    .await?)
}
