//! Repository: `chunks` table.

use anyhow::Result;
use chrono::Utc;
use serde::{Deserialize, Serialize};
use crate::db::Db;
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct Chunk {
    pub id: String,
    pub source_id: String,
    pub chunk_index: i64,
    pub content: String,
    pub char_start: i64,
    pub char_end: i64,
    pub page_number: Option<i64>,
    pub token_count: i64,
    pub created_at: i64,
    pub parent_chunk_id: Option<String>,
    pub unit_type: String,
    pub unit_label: Option<String>,
    pub unit_path: Option<String>,
    pub domain_metadata: Option<String>,
}

#[derive(Debug)]
pub struct NewChunk {
    pub source_id: String,
    pub chunk_index: i64,
    pub content: String,
    pub char_start: i64,
    pub char_end: i64,
    pub page_number: Option<i64>,
    pub token_count: i64,
    pub parent_chunk_id: Option<String>,
    pub unit_type: String,
    pub unit_label: Option<String>,
    pub unit_path: Option<String>,
    pub domain_metadata: Option<String>,
}

/// Bulk-insert chunks for a source inside a single transaction.
/// Returns the inserted chunks with their (freshly generated) IDs.
pub async fn bulk_insert(pool: &Db, chunks: Vec<NewChunk>) -> Result<Vec<Chunk>> {
    let prepared: Vec<(String, NewChunk)> = chunks
        .into_iter()
        .map(|c| (Uuid::new_v4().to_string(), c))
        .collect();
    bulk_insert_with_ids(pool, prepared).await
}

/// Bulk-insert chunks that already carry caller-assigned IDs, in one
/// transaction, in the order given. Used to persist a hierarchy tree where
/// each child's `parent_chunk_id` must reference an ID fixed before insert —
/// pass the rows in parent-before-child (pre-order) sequence.
pub async fn bulk_insert_with_ids(
    pool: &Db,
    prepared: Vec<(String, NewChunk)>,
) -> Result<Vec<Chunk>> {
    let now = Utc::now().timestamp_millis();
    let mut result = Vec::with_capacity(prepared.len());

    let mut tx = pool.begin().await?;
    for (id, c) in &prepared {
        sqlx::query(
            "INSERT INTO chunks \
             (id, source_id, chunk_index, content, char_start, char_end, page_number, token_count, created_at, \
              parent_chunk_id, unit_type, unit_label, unit_path, domain_metadata) \
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14)",
        )
        .bind(id)
        .bind(&c.source_id)
        .bind(c.chunk_index)
        .bind(&c.content)
        .bind(c.char_start)
        .bind(c.char_end)
        .bind(c.page_number)
        .bind(c.token_count)
        .bind(now)
        .bind(&c.parent_chunk_id)
        .bind(&c.unit_type)
        .bind(&c.unit_label)
        .bind(&c.unit_path)
        .bind(&c.domain_metadata)
        .execute(&mut *tx)
        .await?;
    }
    tx.commit().await?;

    for (id, c) in prepared {
        result.push(Chunk {
            id, source_id: c.source_id, chunk_index: c.chunk_index, content: c.content,
            char_start: c.char_start, char_end: c.char_end, page_number: c.page_number,
            token_count: c.token_count, created_at: now,
            parent_chunk_id: c.parent_chunk_id, unit_type: c.unit_type,
            unit_label: c.unit_label, unit_path: c.unit_path, domain_metadata: c.domain_metadata,
        });
    }
    Ok(result)
}

pub async fn list_by_source(pool: &Db, source_id: &str) -> Result<Vec<Chunk>> {
    Ok(sqlx::query_as::<_, Chunk>(
        "SELECT * FROM chunks WHERE source_id = $1 ORDER BY chunk_index",
    )
    .bind(source_id)
    .fetch_all(pool)
    .await?)
}

pub async fn get_by_id(pool: &Db, id: &str) -> Result<Option<Chunk>> {
    Ok(
        sqlx::query_as::<_, Chunk>("SELECT * FROM chunks WHERE id = $1")
            .bind(id)
            .fetch_optional(pool)
            .await?,
    )
}

/// Wipe a source's chunks and every row that references them, in FK dependency
/// order, atomically.
///
/// A bare `DELETE FROM chunks` throws SQLITE_CONSTRAINT_FOREIGNKEY (787) once a
/// source has had its legal graph / Pass-2 NER built, because
/// `graph_nodes.chunk_id` (and `graph_edges.{from,to}_node`) are FK NO ACTION.
/// That fired on re-index/retry of an already-graphed source and aborted the
/// whole operation, leaving the UI stuck at the early "clearing" phase.
/// `chunk_embeddings` is also cleared so old vectors are not orphaned on
/// re-index (it has no FK, but stale rows would pollute search). This mirrors
/// the ordered deletion in `commands::folders::remove_folder_source`.
pub async fn delete_by_source(pool: &Db, source_id: &str) -> Result<()> {
    let mut tx = pool.begin().await?;

    // chunk_embeddings is a vec0 virtual table created at runtime (schema.rs),
    // so it may be absent in migration-only pools — guard before touching it.
    let emb_exists: (i64,) = sqlx::query_as(
        "SELECT COUNT(*) FROM information_schema.tables WHERE table_schema='public' AND table_name='chunk_embeddings'",
    )
    .fetch_one(&mut *tx)
    .await?;
    if emb_exists.0 > 0 {
        sqlx::query(
            "DELETE FROM chunk_embeddings WHERE chunk_id IN (SELECT id FROM chunks WHERE source_id = $1)",
        )
        .bind(source_id)
        .execute(&mut *tx)
        .await?;
    }

    // graph_edges → graph_nodes → chunks: NO ACTION FKs, so delete leaves first.
    for col in ["from_node", "to_node"] {
        sqlx::query(&format!(
            "DELETE FROM graph_edges WHERE {col} IN \
             (SELECT id FROM graph_nodes WHERE chunk_id IN \
              (SELECT id FROM chunks WHERE source_id = $1))",
        ))
        .bind(source_id)
        .execute(&mut *tx)
        .await?;
    }

    sqlx::query(
        "DELETE FROM graph_nodes WHERE chunk_id IN (SELECT id FROM chunks WHERE source_id = $1)",
    )
    .bind(source_id)
    .execute(&mut *tx)
    .await?;

    sqlx::query("DELETE FROM chunks WHERE source_id = $1")
        .bind(source_id)
        .execute(&mut *tx)
        .await?;

    tx.commit().await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    async fn fresh_pool() -> Db {
        let pool = crate::db::test_pool().await;
        sqlx::query("INSERT INTO sources(id,file_path,file_name,format,file_size_bytes,file_hash,last_modified,created_at) VALUES('s1','/x','x.pdf','pdf',1,'h',0,0)")
            .execute(&pool).await.unwrap();
        pool
    }

    #[tokio::test]
    async fn bulk_insert_persists_unit_type_and_parent() {
        let pool = fresh_pool().await;
        let parent = NewChunk {
            source_id: "s1".into(), chunk_index: 0, content: "Doc rollup".into(),
            char_start: 0, char_end: 10, page_number: None, token_count: 3,
            parent_chunk_id: None,
            unit_type: "document".into(),
            unit_label: Some("x.pdf".into()),
            unit_path: Some("".into()),
            domain_metadata: None,
        };
        let inserted = bulk_insert(&pool, vec![parent]).await.unwrap();
        let parent_id = inserted[0].id.clone();
        let leaf = NewChunk {
            source_id: "s1".into(), chunk_index: 1, content: "leaf text".into(),
            char_start: 0, char_end: 9, page_number: Some(1), token_count: 2,
            parent_chunk_id: Some(parent_id.clone()),
            unit_type: "leaf".into(),
            unit_label: Some("Section 1".into()),
            unit_path: Some("Section 1".into()),
            domain_metadata: Some(r#"{"domain":"legal"}"#.into()),
        };
        let l = bulk_insert(&pool, vec![leaf]).await.unwrap();
        assert_eq!(l[0].unit_type, "leaf");
        assert_eq!(l[0].parent_chunk_id.as_deref(), Some(parent_id.as_str()));
        assert_eq!(l[0].unit_label.as_deref(), Some("Section 1"));
    }

    /// Migration 018: deleting the SOURCE must take the graph with it, without
    /// the caller knowing the ordering.
    ///
    /// `delete_by_source` below hand-deletes edges → nodes → chunks, and three
    /// other call sites repeat that sequence. Two do NOT — `clean_orphan_sources`
    /// (which swallows the FK error and skips the source, so orphans accumulate)
    /// and the "source is now unreferenced" path in `repositories::sources`.
    /// Those delete `sources` directly and rely on the cascade, which stopped
    /// dead at `graph_nodes`. This asserts the cascade now runs the whole chain.
    #[tokio::test]
    async fn deleting_a_source_directly_cascades_through_the_graph() {
        let pool = fresh_pool().await;
        let chunk = NewChunk {
            source_id: "s1".into(), chunk_index: 0, content: "Article 1".into(),
            char_start: 0, char_end: 9, page_number: Some(1), token_count: 2,
            parent_chunk_id: None,
            unit_type: "article".into(), unit_label: Some("Article 1".into()),
            unit_path: Some("Article 1".into()), domain_metadata: None,
        };
        let chunk_id = bulk_insert(&pool, vec![chunk]).await.unwrap()[0].id.clone();
        sqlx::query("INSERT INTO graph_nodes(id, chunk_id, node_type, citation) VALUES('n1', $1, 'article', 'Article 1')")
            .bind(&chunk_id).execute(&pool).await.unwrap();
        sqlx::query("INSERT INTO graph_edges(id, from_node, to_node, edge_type) VALUES('e1','n1','n1','contains')")
            .execute(&pool).await.unwrap();

        // No pre-clearing: exactly what the two unprotected call sites do.
        sqlx::query("DELETE FROM sources WHERE id = 's1'")
            .execute(&pool).await
            .expect("deleting a graphed source must not violate a foreign key");

        for (table, sql) in [
            ("chunks", "SELECT COUNT(*) FROM chunks WHERE source_id='s1'"),
            ("graph_nodes", "SELECT COUNT(*) FROM graph_nodes WHERE id='n1'"),
            ("graph_edges", "SELECT COUNT(*) FROM graph_edges WHERE id='e1'"),
        ] {
            let n: (i64,) = sqlx::query_as(sql).fetch_one(&pool).await.unwrap();
            assert_eq!(n.0, 0, "{table} rows survived the source delete");
        }
    }

    /// Regression: re-indexing a source that already had its legal graph built
    /// used to throw SQLITE_CONSTRAINT_FOREIGNKEY (787) on the bare
    /// `DELETE FROM chunks`, because graph_nodes/graph_edges still referenced
    /// the chunks (FK NO ACTION). delete_by_source must clear them in order.
    #[tokio::test]
    async fn delete_by_source_clears_dependent_graph_rows() {
        let pool = fresh_pool().await;
        let chunk = NewChunk {
            source_id: "s1".into(), chunk_index: 0, content: "Article 1".into(),
            char_start: 0, char_end: 9, page_number: Some(1), token_count: 2,
            parent_chunk_id: None,
            unit_type: "article".into(), unit_label: Some("Article 1".into()),
            unit_path: Some("Article 1".into()), domain_metadata: None,
        };
        let inserted = bulk_insert(&pool, vec![chunk]).await.unwrap();
        let chunk_id = inserted[0].id.clone();

        // Simulate Pass-2/legal-graph build: a graph_node + self edge.
        sqlx::query("INSERT INTO graph_nodes(id, chunk_id, node_type, citation) VALUES('n1', $1, 'article', 'Article 1')")
            .bind(&chunk_id).execute(&pool).await.unwrap();
        sqlx::query("INSERT INTO graph_edges(id, from_node, to_node, edge_type) VALUES('e1','n1','n1','contains')")
            .execute(&pool).await.unwrap();

        // Old code panicked here with FK 787; fixed code deletes in order.
        delete_by_source(&pool, "s1").await.unwrap();

        let chunks: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM chunks WHERE source_id='s1'")
            .fetch_one(&pool).await.unwrap();
        let nodes: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM graph_nodes WHERE chunk_id=$1")
            .bind(&chunk_id).fetch_one(&pool).await.unwrap();
        let edges: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM graph_edges WHERE id='e1'")
            .fetch_one(&pool).await.unwrap();
        assert_eq!(chunks.0, 0, "chunks not cleared");
        assert_eq!(nodes.0, 0, "graph_nodes not cleared");
        assert_eq!(edges.0, 0, "graph_edges not cleared");
    }
}
