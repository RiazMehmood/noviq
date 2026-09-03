//! Repository: `quote_request` (migration 028) — organisations asking to be
//! contacted, submitted from the public landing page.
//!
//! ⚠️ The insert path is reachable WITHOUT a session — the only such write in
//! this system. Everything here therefore treats the payload as hostile text:
//! it is bounded, stored, listed and mailed, and never interpolated anywhere
//! that executes. See the migration header.

use anyhow::Result;
use chrono::Utc;
use serde::Serialize;
use uuid::Uuid;

use crate::db::Db;

/// A row as the admin console sees it.
///
/// ⚠️ `source_ip` is deliberately ABSENT. It is collected for rate limiting and
/// abuse triage only; putting it in the serialised row would ship a visitor's IP
/// to every admin browser for no operational gain.
#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct QuoteRequest {
    pub id: String,
    pub organisation: String,
    pub contact_name: Option<String>,
    pub email: String,
    pub phone: Option<String>,
    pub sector: Option<String>,
    pub org_size: Option<String>,
    pub message: Option<String>,
    pub status: String,
    pub admin_note: Option<String>,
    /// Epoch MILLISECONDS (migration 028, matching `sources` and 023).
    pub created_at: i64,
    pub updated_at: i64,
    pub emailed_at: Option<i64>,
}

/// What the API accepts. Trimmed and length-checked by the caller before it
/// arrives; the schema bounds it again.
pub struct NewQuoteRequest<'a> {
    pub organisation: &'a str,
    pub contact_name: Option<&'a str>,
    pub email: &'a str,
    pub phone: Option<&'a str>,
    pub sector: Option<&'a str>,
    pub org_size: Option<&'a str>,
    pub message: Option<&'a str>,
    pub source_ip: Option<&'a str>,
}

/// Store one request. Returns the new id.
pub async fn insert(pool: &Db, req: NewQuoteRequest<'_>) -> Result<String> {
    let id = Uuid::new_v4().to_string();
    let now = Utc::now().timestamp_millis();
    sqlx::query(
        "INSERT INTO quote_request \
         (id, organisation, contact_name, email, phone, sector, org_size, message, \
          status, admin_note, created_at, updated_at, source_ip, emailed_at) \
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, 'new', NULL, $9, $9, $10, NULL)",
    )
    .bind(&id)
    .bind(req.organisation)
    .bind(req.contact_name)
    .bind(req.email)
    .bind(req.phone)
    .bind(req.sector)
    .bind(req.org_size)
    .bind(req.message)
    .bind(now)
    .bind(req.source_ip)
    .execute(pool)
    .await?;
    Ok(id)
}

/// How many requests this IP has filed since `since_ms`.
///
/// The rate limit's whole job is to stop one machine filling the inbox, so it
/// counts by IP and NOT by email — an abuser controls the email field and would
/// simply vary it.
pub async fn count_from_ip_since(pool: &Db, ip: &str, since_ms: i64) -> Result<i64> {
    let (n,): (i64,) = sqlx::query_as(
        "SELECT COUNT(*) FROM quote_request WHERE source_ip = $1 AND created_at >= $2",
    )
    .bind(ip)
    .bind(since_ms)
    .fetch_one(pool)
    .await?;
    Ok(n)
}

/// The inbox: newest first. `status_filter` of `None` returns every status
/// except `archived`, which is what an inbox means.
pub async fn list(pool: &Db, status_filter: Option<&str>, limit: i64) -> Result<Vec<QuoteRequest>> {
    let rows = match status_filter {
        Some(s) => {
            sqlx::query_as::<_, QuoteRequest>(
                "SELECT id, organisation, contact_name, email, phone, sector, org_size, \
                        message, status, admin_note, created_at, updated_at, emailed_at \
                 FROM quote_request WHERE status = $1 ORDER BY created_at DESC LIMIT $2",
            )
            .bind(s)
            .bind(limit)
            .fetch_all(pool)
            .await?
        }
        None => {
            sqlx::query_as::<_, QuoteRequest>(
                "SELECT id, organisation, contact_name, email, phone, sector, org_size, \
                        message, status, admin_note, created_at, updated_at, emailed_at \
                 FROM quote_request WHERE status <> 'archived' ORDER BY created_at DESC LIMIT $1",
            )
            .bind(limit)
            .fetch_all(pool)
            .await?
        }
    };
    Ok(rows)
}

/// Count of unread requests — the Admin nav badge.
pub async fn count_new(pool: &Db) -> Result<i64> {
    let (n,): (i64,) = sqlx::query_as("SELECT COUNT(*) FROM quote_request WHERE status = 'new'")
        .fetch_one(pool)
        .await?;
    Ok(n)
}

/// Move a request through triage, optionally attaching/replacing the note.
///
/// `note = None` LEAVES the existing note alone rather than clearing it — an
/// admin marking a lead "replied" must not silently lose what they wrote about
/// it earlier.
pub async fn set_status(pool: &Db, id: &str, status: &str, note: Option<&str>) -> Result<bool> {
    let now = Utc::now().timestamp_millis();
    let res = sqlx::query(
        "UPDATE quote_request \
         SET status = $1, admin_note = COALESCE($2, admin_note), updated_at = $3 \
         WHERE id = $4",
    )
    .bind(status)
    .bind(note)
    .bind(now)
    .bind(id)
    .execute(pool)
    .await?;
    Ok(res.rows_affected() > 0)
}

/// Record that the notification email went out.
pub async fn mark_emailed(pool: &Db, id: &str) -> Result<()> {
    sqlx::query("UPDATE quote_request SET emailed_at = $1 WHERE id = $2")
        .bind(Utc::now().timestamp_millis())
        .bind(id)
        .execute(pool)
        .await?;
    Ok(())
}
