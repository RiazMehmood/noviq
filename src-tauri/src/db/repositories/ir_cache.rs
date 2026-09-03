//! T014 — `ir_cache_index` table operations. SQL-only; JSON file I/O lives
//! in `services::doc_ir::cache`.
//!
//! See data-model.md §3 + §4.

use crate::db::Db;

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct CacheRow {
    pub sha256: String,
    pub schema_version: i64,
    pub doc_type: String,
    pub nodes_total: i64,
    pub nodes_ok: i64,
    pub nodes_low_confidence: i64,
    pub nodes_user_repaired: i64,
    pub quality_score: f64,
    pub orphaned_at: Option<i64>,
    pub created_at: i64,
    pub updated_at: i64,
}

pub struct InsertOrUpdate<'a> {
    pub sha256: &'a str,
    pub schema_version: i64,
    pub doc_type: &'a str,
    pub nodes_total: i64,
    pub nodes_ok: i64,
    pub nodes_low_confidence: i64,
    pub nodes_user_repaired: i64,
    pub quality_score: f64,
}

pub async fn insert_or_update(pool: &Db, row: InsertOrUpdate<'_>) -> anyhow::Result<()> {
    let now = unix_now();
    sqlx::query(
        "INSERT INTO ir_cache_index
            (sha256, schema_version, doc_type, nodes_total, nodes_ok,
             nodes_low_confidence, nodes_user_repaired, quality_score,
             orphaned_at, created_at, updated_at)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, NULL, $9, $10)
         ON CONFLICT(sha256) DO UPDATE SET
            schema_version       = excluded.schema_version,
            doc_type             = excluded.doc_type,
            nodes_total          = excluded.nodes_total,
            nodes_ok             = excluded.nodes_ok,
            nodes_low_confidence = excluded.nodes_low_confidence,
            nodes_user_repaired  = excluded.nodes_user_repaired,
            quality_score        = excluded.quality_score,
            orphaned_at          = NULL,
            updated_at           = excluded.updated_at",
    )
    .bind(row.sha256)
    .bind(row.schema_version)
    .bind(row.doc_type)
    .bind(row.nodes_total)
    .bind(row.nodes_ok)
    .bind(row.nodes_low_confidence)
    .bind(row.nodes_user_repaired)
    .bind(row.quality_score)
    .bind(now)
    .bind(now)
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn mark_orphaned(pool: &Db, sha256: &str) -> anyhow::Result<()> {
    let now = unix_now();
    sqlx::query("UPDATE ir_cache_index SET orphaned_at = $1, updated_at = $2 WHERE sha256 = $3")
        .bind(now)
        .bind(now)
        .bind(sha256)
        .execute(pool)
        .await?;
    Ok(())
}

pub async fn clear_orphan(pool: &Db, sha256: &str) -> anyhow::Result<()> {
    let now = unix_now();
    sqlx::query("UPDATE ir_cache_index SET orphaned_at = NULL, updated_at = $1 WHERE sha256 = $2")
        .bind(now)
        .bind(sha256)
        .execute(pool)
        .await?;
    Ok(())
}

pub async fn find_orphans_older_than(
    pool: &Db,
    cutoff_unix: i64,
) -> anyhow::Result<Vec<String>> {
    let rows: Vec<(String,)> = sqlx::query_as(
        "SELECT sha256 FROM ir_cache_index WHERE orphaned_at IS NOT NULL AND orphaned_at < $1",
    )
    .bind(cutoff_unix)
    .fetch_all(pool)
    .await?;
    Ok(rows.into_iter().map(|(s,)| s).collect())
}

pub async fn delete(pool: &Db, sha256: &str) -> anyhow::Result<()> {
    sqlx::query("DELETE FROM ir_cache_index WHERE sha256 = $1")
        .bind(sha256)
        .execute(pool)
        .await?;
    Ok(())
}

pub async fn get(pool: &Db, sha256: &str) -> anyhow::Result<Option<CacheRow>> {
    let row = sqlx::query_as::<_, CacheRow>("SELECT * FROM ir_cache_index WHERE sha256 = $1")
        .bind(sha256)
        .fetch_optional(pool)
        .await?;
    Ok(row)
}

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct DocReviewItem {
    pub source_id: String,
    pub title: String,
    pub nodes_low_confidence: i64,
    pub quality_score: f64,
}

pub async fn list_needing_review(pool: &Db) -> anyhow::Result<Vec<DocReviewItem>> {
    let rows = sqlx::query_as::<_, DocReviewItem>(
        "SELECT s.id AS source_id, COALESCE(s.file_name, '') AS title,
                COALESCE(s.nodes_low_confidence, 0) AS nodes_low_confidence,
                COALESCE(s.ir_quality_score, 1.0) AS quality_score
           FROM sources s
          WHERE COALESCE(s.nodes_low_confidence, 0) > 0
          ORDER BY s.nodes_low_confidence DESC",
    )
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

fn unix_now() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    async fn fresh_pool() -> Db {
        crate::db::test_pool().await
    }

    #[tokio::test]
    async fn insert_then_get_roundtrip() {
        let pool = fresh_pool().await;
        let sha = "a".repeat(64);
        insert_or_update(
            &pool,
            InsertOrUpdate {
                sha256: &sha,
                schema_version: 1,
                doc_type: "act",
                nodes_total: 10,
                nodes_ok: 9,
                nodes_low_confidence: 1,
                nodes_user_repaired: 0,
                quality_score: 0.9,
            },
        )
        .await
        .unwrap();
        let got = get(&pool, &sha).await.unwrap().unwrap();
        assert_eq!(got.doc_type, "act");
        assert_eq!(got.nodes_total, 10);
        assert!(got.orphaned_at.is_none());
    }

    #[tokio::test]
    async fn mark_orphan_then_clear() {
        let pool = fresh_pool().await;
        let sha = "b".repeat(64);
        insert_or_update(
            &pool,
            InsertOrUpdate {
                sha256: &sha,
                schema_version: 1,
                doc_type: "rules",
                nodes_total: 5,
                nodes_ok: 5,
                nodes_low_confidence: 0,
                nodes_user_repaired: 0,
                quality_score: 1.0,
            },
        )
        .await
        .unwrap();
        mark_orphaned(&pool, &sha).await.unwrap();
        let after_orphan = get(&pool, &sha).await.unwrap().unwrap();
        assert!(after_orphan.orphaned_at.is_some());

        clear_orphan(&pool, &sha).await.unwrap();
        let after_clear = get(&pool, &sha).await.unwrap().unwrap();
        assert!(after_clear.orphaned_at.is_none());
    }

    #[tokio::test]
    async fn find_orphans_older_than_filters_correctly() {
        let pool = fresh_pool().await;
        let sha = "c".repeat(64);
        insert_or_update(
            &pool,
            InsertOrUpdate {
                sha256: &sha,
                schema_version: 1,
                doc_type: "act",
                nodes_total: 1,
                nodes_ok: 1,
                nodes_low_confidence: 0,
                nodes_user_repaired: 0,
                quality_score: 1.0,
            },
        )
        .await
        .unwrap();
        mark_orphaned(&pool, &sha).await.unwrap();

        // far-future cutoff → orphan included.
        let stale = find_orphans_older_than(&pool, i64::MAX).await.unwrap();
        assert_eq!(stale, vec![sha.clone()]);

        // far-past cutoff → orphan excluded.
        let none = find_orphans_older_than(&pool, 0).await.unwrap();
        assert!(none.is_empty());
    }
}
