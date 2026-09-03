//! Per-source persistent progress for the LLM IR builder (migration 019).
//!
//! Lets a long-running build survive process restarts, transient stalls, and
//! lid-close suspends without re-doing the chunks that already produced nodes.
//! Written-to after every successful chunk inside
//! `mistral_builder::generate_chunk_with_retry`; deleted on successful
//! whole-doc validation in `mistral_builder::build`.
//!
//! The shape mirrors what the builder needs to resume:
//!   - `total_chunks` so we can detect a chunker output change (source text
//!     was edited between runs) and ignore the stale row.
//!   - `next_chunk_idx` so the loop knows where to resume.
//!   - `id_counter` so doc-global node ids stay monotone across resumes.
//!   - `nodes` (full vector) so the in-memory `all_nodes` rebuilds without
//!     rereading the partial graph from `graph_nodes` (which is only written
//!     atomically at the end of dispatch::persist_and_finalize).

use crate::services::doc_ir::types::{DocType, StructureNode};
use crate::db::Db;

#[derive(Debug, Clone)]
pub struct ChunkProgress {
    pub doc_type: DocType,
    pub total_chunks: u32,
    pub next_chunk_idx: u32,
    pub id_counter: u32,
    pub nodes: Vec<StructureNode>,
}

/// Load a saved progress row, if any. Returns `Ok(None)` when no row exists
/// or when the stored JSON can't be deserialized (a corrupted/stale row is
/// treated as no progress so the build can still proceed).
pub async fn load(
    pool: &Db,
    source_id: &str,
) -> sqlx::Result<Option<ChunkProgress>> {
    let row: Option<(String, i64, i64, i64, String)> = sqlx::query_as(
        "SELECT doc_type, total_chunks, next_chunk_idx, id_counter, nodes_json
           FROM doc_ir_progress
          WHERE source_id = $1",
    )
    .bind(source_id)
    .fetch_optional(pool)
    .await?;

    let Some((doc_type_str, total, next, counter, nodes_json)) = row else {
        return Ok(None);
    };
    let doc_type = match doc_type_str.as_str() {
        "constitution" => DocType::Constitution,
        "act" => DocType::Act,
        "rules" => DocType::Rules,
        "sro" => DocType::Sro,
        "contract" => DocType::Contract,
        _ => DocType::Generic,
    };
    let nodes: Vec<StructureNode> = match serde_json::from_str(&nodes_json) {
        Ok(v) => v,
        Err(e) => {
            log::warn!(
                "doc_ir_progress: stale row for {source_id} (nodes_json parse: {e}); ignoring"
            );
            return Ok(None);
        }
    };
    Ok(Some(ChunkProgress {
        doc_type,
        total_chunks: total as u32,
        next_chunk_idx: next as u32,
        id_counter: counter as u32,
        nodes,
    }))
}

/// Upsert progress for `source_id`. Caller passes `next_chunk_idx` = the
/// index of the chunk that should run NEXT (so a value equal to total_chunks
/// means "all chunks done, awaiting whole-doc validation").
pub async fn save(
    pool: &Db,
    source_id: &str,
    source_sha256: &str,
    progress: &ChunkProgress,
) -> sqlx::Result<()> {
    let nodes_json = serde_json::to_string(&progress.nodes)
        .map_err(|e| sqlx::Error::Decode(Box::new(e)))?;
    let now = chrono::Utc::now().timestamp_millis();
    sqlx::query(
        "INSERT INTO doc_ir_progress
            (source_id, source_sha256, doc_type, total_chunks,
             next_chunk_idx, id_counter, nodes_json, updated_at)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
         ON CONFLICT(source_id) DO UPDATE SET
             source_sha256  = excluded.source_sha256,
             doc_type       = excluded.doc_type,
             total_chunks   = excluded.total_chunks,
             next_chunk_idx = excluded.next_chunk_idx,
             id_counter     = excluded.id_counter,
             nodes_json     = excluded.nodes_json,
             updated_at     = excluded.updated_at",
    )
    .bind(source_id)
    .bind(source_sha256)
    .bind(progress.doc_type.as_str())
    .bind(progress.total_chunks as i64)
    .bind(progress.next_chunk_idx as i64)
    .bind(progress.id_counter as i64)
    .bind(nodes_json)
    .bind(now)
    .execute(pool)
    .await?;
    Ok(())
}

/// Remove the progress row — called after whole-doc validation succeeds.
pub async fn clear(pool: &Db, source_id: &str) -> sqlx::Result<()> {
    sqlx::query("DELETE FROM doc_ir_progress WHERE source_id = $1")
        .bind(source_id)
        .execute(pool)
        .await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    async fn fresh_pool() -> Db {
        crate::db::test_pool().await
    }

    fn sample_node(id: &str) -> StructureNode {
        use crate::services::doc_ir::types::{NodeKind, QualityFlag};
        StructureNode {
            id: id.into(),
            parent_id: None,
            kind: NodeKind::Section,
            number: Some("1".into()),
            title: None,
            text: "body".into(),
            page: 1,
            annotations: vec![],
            footnote_refs: vec![],
            metadata: serde_json::Value::Null,
            quality_flag: QualityFlag::Ok,
            quality_error: None,
        }
    }

    #[tokio::test]
    async fn save_then_load_roundtrips() {
        let pool = fresh_pool().await;
        let progress = ChunkProgress {
            doc_type: DocType::Rules,
            total_chunks: 10,
            next_chunk_idx: 4,
            id_counter: 7,
            nodes: vec![sample_node("n0001"), sample_node("n0007")],
        };
        save(&pool, "src-1", "sha-1", &progress).await.unwrap();
        let loaded = load(&pool, "src-1").await.unwrap().expect("row");
        assert_eq!(loaded.doc_type, DocType::Rules);
        assert_eq!(loaded.total_chunks, 10);
        assert_eq!(loaded.next_chunk_idx, 4);
        assert_eq!(loaded.id_counter, 7);
        assert_eq!(loaded.nodes.len(), 2);
        assert_eq!(loaded.nodes[1].id, "n0007");
    }

    #[tokio::test]
    async fn load_returns_none_when_missing() {
        let pool = fresh_pool().await;
        assert!(load(&pool, "no-such-source").await.unwrap().is_none());
    }

    #[tokio::test]
    async fn save_then_save_upserts_same_key() {
        let pool = fresh_pool().await;
        let mut p = ChunkProgress {
            doc_type: DocType::Act,
            total_chunks: 5,
            next_chunk_idx: 1,
            id_counter: 1,
            nodes: vec![sample_node("n0001")],
        };
        save(&pool, "s", "sha", &p).await.unwrap();
        p.next_chunk_idx = 3;
        p.id_counter = 3;
        p.nodes.push(sample_node("n0003"));
        save(&pool, "s", "sha", &p).await.unwrap();
        let row: (i64,) =
            sqlx::query_as("SELECT COUNT(*) FROM doc_ir_progress WHERE source_id='s'")
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(row.0, 1);
        let loaded = load(&pool, "s").await.unwrap().unwrap();
        assert_eq!(loaded.next_chunk_idx, 3);
        assert_eq!(loaded.nodes.len(), 2);
    }

    #[tokio::test]
    async fn clear_deletes_row() {
        let pool = fresh_pool().await;
        let p = ChunkProgress {
            doc_type: DocType::Generic,
            total_chunks: 1,
            next_chunk_idx: 1,
            id_counter: 1,
            nodes: vec![],
        };
        save(&pool, "s", "sha", &p).await.unwrap();
        clear(&pool, "s").await.unwrap();
        assert!(load(&pool, "s").await.unwrap().is_none());
    }

    #[tokio::test]
    async fn load_returns_none_for_corrupt_nodes_json() {
        let pool = fresh_pool().await;
        sqlx::query(
            "INSERT INTO doc_ir_progress
                (source_id, source_sha256, doc_type, total_chunks,
                 next_chunk_idx, id_counter, nodes_json, updated_at)
             VALUES ('s', 'sha', 'rules', 5, 2, 2, 'not json', 0)",
        )
        .execute(&pool)
        .await
        .unwrap();
        // Stale/corrupt rows are treated as "no progress" instead of erroring.
        assert!(load(&pool, "s").await.unwrap().is_none());
    }
}
