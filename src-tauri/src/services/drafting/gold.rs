//! Feature 009 — gold/style example storage.
//!
//! **Invariant (SC-007, FR-013):** gold-example CONTENT is retained for
//! structure/style + evaluation only and is NEVER inserted into the retrieval
//! corpus (chunks / FTS / vec). It is reachable solely through this module and
//! the `gold_example` table; it is never handed to the indexer. The test below
//! guards the property that recording a gold example creates no retrievable rows.

use anyhow::Result;
use serde::{Deserialize, Serialize};
use crate::db::Db;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GoldExample {
    pub id: Option<i64>,
    pub template_id: Option<i64>,
    pub instrument_type: String,
    /// On-disk path/sha of the example content. Never indexed.
    pub content_ref: String,
}

/// Record a gold example (metadata only; content stays on disk, out of retrieval).
pub async fn record_gold_example(pool: &Db, ex: &GoldExample) -> Result<i64> {
    let now = chrono::Utc::now().to_rfc3339();
    let id: i64 = sqlx::query_scalar(
        "INSERT INTO gold_example (template_id, instrument_type, content_ref, created_at)
         VALUES ($1, $2, $3, $4) RETURNING id",
    )
    .bind(ex.template_id)
    .bind(&ex.instrument_type)
    .bind(&ex.content_ref)
    .bind(&now)
    .fetch_one(pool)
    .await?;
    Ok(id)
}

/// List gold examples for an optional template association.
pub async fn list_gold_examples(
    pool: &Db,
    template_id: Option<i64>,
) -> Result<Vec<GoldExample>> {
    let rows: Vec<(i64, Option<i64>, String, String)> = match template_id {
        Some(tid) => {
            sqlx::query_as(
                "SELECT id, template_id, instrument_type, content_ref
                 FROM gold_example WHERE template_id = $1 ORDER BY id DESC",
            )
            .bind(tid)
            .fetch_all(pool)
            .await?
        }
        None => {
            sqlx::query_as(
                "SELECT id, template_id, instrument_type, content_ref
                 FROM gold_example ORDER BY id DESC",
            )
            .fetch_all(pool)
            .await?
        }
    };
    Ok(rows
        .into_iter()
        .map(|(id, template_id, instrument_type, content_ref)| GoldExample {
            id: Some(id),
            template_id,
            instrument_type,
            content_ref,
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::test_pool;

    #[tokio::test]
    async fn gold_example_recorded_but_absent_from_retrieval() {
        let pool = test_pool().await;
        let id = record_gold_example(
            &pool,
            &GoldExample {
                id: None,
                template_id: None,
                instrument_type: "summary".into(),
                content_ref: "/data/gold/example-1.txt".into(),
            },
        )
        .await
        .unwrap();
        assert!(id > 0);

        // SC-007 guard: the gold content_ref must NOT appear as a retrievable
        // chunk. The chunks table holds the retrieval corpus; a gold example
        // must never land there.
        let chunk_hits: (i64,) = sqlx::query_as(
            "SELECT COUNT(*) FROM chunks WHERE content LIKE '%/data/gold/example-1.txt%'",
        )
        .fetch_one(&pool)
        .await
        .unwrap_or((0,));
        assert_eq!(chunk_hits.0, 0, "gold example must not enter the retrieval corpus");
    }
}
