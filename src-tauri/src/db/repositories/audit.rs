//! Repository: `audit_log` table.

use anyhow::Result;
use chrono::Utc;
use serde::{Deserialize, Serialize};
use crate::db::Db;
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct AuditEvent {
    pub id: String,
    pub event_type: String,
    pub entity_id: Option<String>,
    pub details_json: Option<String>,
    pub created_at: i64,
}

pub async fn insert_event(
    pool: &Db,
    event_type: &str,
    entity_id: Option<&str>,
    details_json: Option<&str>,
) -> Result<()> {
    let id = Uuid::new_v4().to_string();
    let now = Utc::now().timestamp_millis();

    sqlx::query(
        "INSERT INTO audit_log (id, event_type, entity_id, details_json, created_at) \
         VALUES ($1, $2, $3, $4, $5)",
    )
    .bind(&id)
    .bind(event_type)
    .bind(entity_id)
    .bind(details_json)
    .bind(now)
    .execute(pool)
    .await?;

    Ok(())
}

/// F014 (US5, T038, FR-011): multi-user traceability. Writes the same row as
/// `insert_event` plus the actor (`user_id`), the `action` performed, and the
/// `target` it acted on (migration 005 added these nullable columns). Used by
/// the HTTP API where a request carries an authenticated `AuthUser`.
pub async fn insert_user_event(
    pool: &Db,
    user_id: Option<&str>,
    action: &str,
    target: Option<&str>,
    details_json: Option<&str>,
) -> Result<()> {
    let id = Uuid::new_v4().to_string();
    let now = Utc::now().timestamp_millis();

    sqlx::query(
        "INSERT INTO audit_log (id, event_type, entity_id, details_json, created_at, user_id, action, target) \
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8)",
    )
    .bind(&id)
    .bind(action) // event_type mirrors action for backward-compatible reads
    .bind(target)
    .bind(details_json)
    .bind(now)
    .bind(user_id)
    .bind(action)
    .bind(target)
    .execute(pool)
    .await?;

    Ok(())
}

pub async fn list(pool: &Db, limit: i64, offset: i64) -> Result<Vec<AuditEvent>> {
    Ok(sqlx::query_as::<_, AuditEvent>(
        "SELECT * FROM audit_log ORDER BY created_at DESC LIMIT $1 OFFSET $2",
    )
    .bind(limit)
    .bind(offset)
    .fetch_all(pool)
    .await?)
}

// ── Admin audit browser (2026-08-13) ────────────────────────────────────────
//
// `list` above returns the pre-F014 shape only (no actor, no action), and its
// single caller is the vestigial Tauri command. The admin console needs the
// full row, the actor's username, and the document title behind `target` — plus
// filtering, because an audit trail nobody can narrow is an audit trail nobody
// reads. Everything below is read-only.

/// One audit row as the console renders it. `username` and `target_name` are
/// resolved by LEFT JOIN, so a deleted user or document still yields a row —
/// losing the actor's name must never lose the event.
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct AuditRecord {
    pub id: String,
    pub created_at: i64,
    pub user_id: Option<String>,
    pub username: Option<String>,
    pub action: Option<String>,
    pub target: Option<String>,
    /// Document title when `target` is a source id; NULL for every other target.
    pub target_name: Option<String>,
    pub details_json: Option<String>,
}

/// Filters for the audit browser and its export. All optional and ANDed —
/// `AuditFilter::default()` means "everything", which is what the whole-log
/// download uses.
#[derive(Debug, Clone, Default)]
pub struct AuditFilter {
    /// Inclusive lower bound, ms epoch.
    pub from_ms: Option<i64>,
    /// EXCLUSIVE upper bound, ms epoch — callers pass midnight after the last
    /// wanted day, so a whole day is `[day, day+1)` with no off-by-one on the
    /// final millisecond.
    pub to_ms: Option<i64>,
    pub user_id: Option<String>,
    /// Empty = every action.
    pub actions: Vec<String>,
    /// Exact source/entity id.
    pub target: Option<String>,
    /// Case-insensitive substring over username, action, target and details.
    pub search: Option<String>,
}

/// Shared WHERE clause for the browser, the counter and the export, so a filter
/// can never mean one thing on screen and another in the downloaded file.
fn push_filters(qb: &mut sqlx::QueryBuilder<'_, sqlx::Postgres>, f: &AuditFilter) {
    qb.push(" WHERE 1=1");
    if let Some(from) = f.from_ms {
        qb.push(" AND a.created_at >= ").push_bind(from);
    }
    if let Some(to) = f.to_ms {
        qb.push(" AND a.created_at < ").push_bind(to);
    }
    if let Some(uid) = f.user_id.as_deref().filter(|s| !s.is_empty()) {
        qb.push(" AND a.user_id = ").push_bind(uid.to_string());
    }
    if let Some(t) = f.target.as_deref().filter(|s| !s.is_empty()) {
        qb.push(" AND a.target = ").push_bind(t.to_string());
    }
    if !f.actions.is_empty() {
        qb.push(" AND COALESCE(a.action, a.event_type) = ANY(")
            .push_bind(f.actions.clone())
            .push(")");
    }
    if let Some(q) = f.search.as_deref().map(str::trim).filter(|s| !s.is_empty()) {
        // ILIKE with the wildcards bound as data, not concatenated into SQL.
        let pat = format!("%{}%", q.replace('\\', "\\\\").replace('%', "\\%").replace('_', "\\_"));
        qb.push(" AND (COALESCE(u.username,'') ILIKE ").push_bind(pat.clone());
        qb.push(" OR COALESCE(a.action, a.event_type) ILIKE ").push_bind(pat.clone());
        qb.push(" OR COALESCE(a.target,'') ILIKE ").push_bind(pat.clone());
        qb.push(" OR COALESCE(s.display_name, s.file_name, '') ILIKE ").push_bind(pat.clone());
        qb.push(" OR COALESCE(a.details_json,'') ILIKE ").push_bind(pat);
        qb.push(")");
    }
}

const SELECT_WITH_JOINS: &str = "SELECT a.id, a.created_at, a.user_id, u.username, \
     COALESCE(a.action, a.event_type) AS action, a.target, \
     COALESCE(s.display_name, s.file_name) AS target_name, a.details_json \
     FROM audit_log a \
     LEFT JOIN users u ON u.id = a.user_id \
     LEFT JOIN sources s ON s.id = a.target";

const COUNT_WITH_JOINS: &str = "SELECT COUNT(*) FROM audit_log a \
     LEFT JOIN users u ON u.id = a.user_id \
     LEFT JOIN sources s ON s.id = a.target";

/// One page of the audit trail, newest first.
pub async fn query(
    pool: &Db,
    f: &AuditFilter,
    limit: i64,
    offset: i64,
) -> Result<Vec<AuditRecord>> {
    let mut qb = sqlx::QueryBuilder::<sqlx::Postgres>::new(SELECT_WITH_JOINS);
    push_filters(&mut qb, f);
    qb.push(" ORDER BY a.created_at DESC, a.id DESC LIMIT ")
        .push_bind(limit)
        .push(" OFFSET ")
        .push_bind(offset);
    Ok(qb.build_query_as::<AuditRecord>().fetch_all(pool).await?)
}

/// Total matching rows, so the console can page and the export can warn.
pub async fn count(pool: &Db, f: &AuditFilter) -> Result<i64> {
    let mut qb = sqlx::QueryBuilder::<sqlx::Postgres>::new(COUNT_WITH_JOINS);
    push_filters(&mut qb, f);
    Ok(qb.build_query_scalar::<i64>().fetch_one(pool).await?)
}

/// Every matching row, for the download. Bounded by `cap` so a filterless
/// export on a large deployment cannot pull the table into memory.
pub async fn export(pool: &Db, f: &AuditFilter, cap: i64) -> Result<Vec<AuditRecord>> {
    query(pool, f, cap, 0).await
}

/// Distinct action names present in the log — populates the filter dropdown, so
/// it always offers exactly what the deployment has actually recorded.
pub async fn distinct_actions(pool: &Db) -> Result<Vec<String>> {
    Ok(sqlx::query_scalar::<_, String>(
        "SELECT DISTINCT COALESCE(action, event_type) AS a FROM audit_log \
         WHERE COALESCE(action, event_type) IS NOT NULL ORDER BY a",
    )
    .fetch_all(pool)
    .await?)
}
