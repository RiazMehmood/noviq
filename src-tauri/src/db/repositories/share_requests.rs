//! Share requests (2026-07-28) — admin-gated sharing.
//!
//! A non-admin owner's "share" action files a request here instead of flipping
//! `sources.visibility`; the admin reviews the queue in the Admin console and
//! approve/reject decides the outcome. `visibility = 'shared'` is thereby only
//! ever written by admin code paths (the F021 retrieval invariant itself is
//! unchanged — see `sources::visible_source_ids`).

use crate::db::Db;
use anyhow::Result;
use uuid::Uuid;

/// One pending-queue row, joined with the requester and document names the
/// admin needs to review it.
#[derive(Debug, sqlx::FromRow, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ShareRequestRow {
    pub id: String,
    pub source_id: String,
    pub requester_user_id: String,
    pub requester_username: Option<String>,
    pub file_name: Option<String>,
    pub display_name: Option<String>,
    pub status: String,
    pub created_at: i64,
}

/// File a pending request. Returns `false` when one already exists for this
/// source (the partial unique index makes the insert a no-op) — the caller
/// reports "already requested" instead of stacking duplicates.
pub async fn create_pending(pool: &Db, source_id: &str, requester: &str) -> Result<bool> {
    let res = sqlx::query(
        "INSERT INTO share_requests (id, source_id, requester_user_id, status, created_at) \
         VALUES ($1, $2, $3, 'pending', EXTRACT(EPOCH FROM now())::bigint) \
         ON CONFLICT DO NOTHING",
    )
    .bind(Uuid::new_v4().to_string())
    .bind(source_id)
    .bind(requester)
    .execute(pool)
    .await?;
    Ok(res.rows_affected() > 0)
}

/// The admin's pending queue, oldest first.
pub async fn list_pending(pool: &Db) -> Result<Vec<ShareRequestRow>> {
    Ok(sqlx::query_as(
        "SELECT r.id, r.source_id, r.requester_user_id, \
                u.username AS requester_username, \
                s.file_name, s.display_name, \
                r.status, r.created_at \
         FROM share_requests r \
         LEFT JOIN users u ON u.id = r.requester_user_id \
         LEFT JOIN sources s ON s.id = r.source_id \
         WHERE r.status = 'pending' \
         ORDER BY r.created_at ASC",
    )
    .fetch_all(pool)
    .await?)
}

/// Decide a pending request. Returns the request's `source_id` when it existed
/// and was still pending; `None` means unknown id or already decided (the
/// caller 404s rather than double-applying).
pub async fn decide(
    pool: &Db,
    request_id: &str,
    approve: bool,
    decided_by: &str,
) -> Result<Option<String>> {
    let row: Option<(String,)> = sqlx::query_as(
        "UPDATE share_requests \
         SET status = $1, decided_at = EXTRACT(EPOCH FROM now())::bigint, decided_by = $2 \
         WHERE id = $3 AND status = 'pending' \
         RETURNING source_id",
    )
    .bind(if approve { "approved" } else { "rejected" })
    .bind(decided_by)
    .bind(request_id)
    .fetch_optional(pool)
    .await?;
    Ok(row.map(|r| r.0))
}
