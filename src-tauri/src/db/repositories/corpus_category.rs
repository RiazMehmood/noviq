//! Repository: `corpus_category` + the two `sources` columns migration 021 adds.
//!
//! Admin-controlled corpus organisation. Both operations here are **label-only**:
//! assigning a category writes one column, disabling writes one timestamp.
//! Neither moves a file, re-chunks, re-embeds or re-indexes anything — which is
//! why re-filing the whole corpus is instant and reversible.
//!
//! Disabling is ENFORCED in [`super::sources::visible_source_ids`], not here.
//! This module only records the intent.

use anyhow::Result;
use chrono::Utc;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::db::Db;

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct CorpusCategory {
    pub id: String,
    pub name: String,
    pub sort_order: i32,
    pub created_at: i64,
    /// Documents filed here. A join artifact, not a column — defaults to 0 for
    /// queries that don't compute it.
    #[serde(default)]
    #[sqlx(default)]
    pub document_count: i64,
}

/// Every category, in display order, with how many documents each holds.
///
/// The count is over ALL documents in the category including disabled ones: the
/// admin needs to see that a category still holds something before deleting it,
/// and a count that silently omitted withdrawn documents would understate that.
pub async fn list(pool: &Db) -> Result<Vec<CorpusCategory>> {
    Ok(sqlx::query_as::<_, CorpusCategory>(
        "SELECT c.id, c.name, c.sort_order, c.created_at, \
                (SELECT COUNT(*) FROM sources s WHERE s.category_id = c.id) AS document_count \
         FROM corpus_category c \
         ORDER BY c.sort_order, c.name",
    )
    .fetch_all(pool)
    .await?)
}

/// Categories as an OFFICER sees them: names and display order, no counts.
///
/// The count in [`list`] is computed over every row in `sources`, including
/// documents this viewer may not retrieve (another user's private uploads, or a
/// withdrawn instrument). Publishing it on the officer-facing route would
/// disclose the size of a corpus they have no access to, so it is left at zero
/// here rather than filtered — the chat sidebar renders its own counts from the
/// documents it was actually given.
pub async fn list_names(pool: &Db) -> Result<Vec<CorpusCategory>> {
    Ok(sqlx::query_as::<_, CorpusCategory>(
        "SELECT id, name, sort_order, created_at, 0::bigint AS document_count \
         FROM corpus_category ORDER BY sort_order, name",
    )
    .fetch_all(pool)
    .await?)
}

/// Create a category. Names are unique (DB constraint) and compared trimmed, so
/// "Finance " cannot shadow "Finance".
pub async fn create(pool: &Db, name: &str, sort_order: i32) -> Result<CorpusCategory> {
    let id = Uuid::new_v4().to_string();
    let now = Utc::now().timestamp_millis();
    let name = name.trim();
    sqlx::query(
        "INSERT INTO corpus_category (id, name, sort_order, created_at) VALUES ($1,$2,$3,$4)",
    )
    .bind(&id)
    .bind(name)
    .bind(sort_order)
    .bind(now)
    .execute(pool)
    .await?;
    Ok(CorpusCategory {
        id,
        name: name.to_string(),
        sort_order,
        created_at: now,
        document_count: 0,
    })
}

/// Rename a category and/or move it in the display order. Returns false when no
/// such category exists, so the caller reports a miss instead of a false success.
pub async fn update(pool: &Db, id: &str, name: &str, sort_order: i32) -> Result<bool> {
    let res = sqlx::query("UPDATE corpus_category SET name = $1, sort_order = $2 WHERE id = $3")
        .bind(name.trim())
        .bind(sort_order)
        .bind(id)
        .execute(pool)
        .await?;
    Ok(res.rows_affected() > 0)
}

/// Delete a category. Its documents are NOT deleted — `sources.category_id` is
/// `ON DELETE SET NULL`, so they fall back to Uncategorised and can be re-filed.
/// Returns how many documents were released, so the UI can say so plainly.
pub async fn delete(pool: &Db, id: &str) -> Result<i64> {
    let released: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM sources WHERE category_id = $1")
            .bind(id)
            .fetch_one(pool)
            .await?;
    sqlx::query("DELETE FROM corpus_category WHERE id = $1")
        .bind(id)
        .execute(pool)
        .await?;
    Ok(released)
}

/// File documents into a category (or, with `category_id = None`, back to
/// Uncategorised). Returns the number of documents actually moved.
///
/// One statement for the whole batch: re-filing a corpus is an admin action that
/// should either land or not, never half-apply because the request was long.
pub async fn assign(pool: &Db, source_ids: &[String], category_id: Option<&str>) -> Result<u64> {
    if source_ids.is_empty() {
        return Ok(0);
    }
    let res = sqlx::query("UPDATE sources SET category_id = $1 WHERE id = ANY($2)")
        .bind(category_id)
        .bind(source_ids)
        .execute(pool)
        .await?;
    Ok(res.rows_affected())
}

/// Withdraw a document from service, or return it. Takes effect on the next turn
/// — `visible_source_ids` is recomputed per request, so no re-index is involved
/// and re-enabling restores retrieval immediately.
pub async fn set_disabled(pool: &Db, source_id: &str, disabled: bool) -> Result<bool> {
    let ts = disabled.then(|| Utc::now().timestamp_millis());
    let res = sqlx::query("UPDATE sources SET disabled_at = $1 WHERE id = $2")
        .bind(ts)
        .bind(source_id)
        .execute(pool)
        .await?;
    Ok(res.rows_affected() > 0)
}
