//! Repository: the free trial (migration 029).
//!
//! A trial user is an ordinary `users` row with `role = 'trial'`, so sessions,
//! `AuthUser`, the audit trail and every ownership check work unchanged. What is
//! NOT unchanged is authorisation: `role != 'admin'` no longer means "a real
//! customer", and every surface that assumed it must now exclude 'trial'.
//!
//! Three limits live here:
//!   * 3 questions per UTC day        — `consume`
//!   * 3 days from first sign-in      — `status`
//!   * a curated corpus, failing shut — `demo_source_ids`

use anyhow::Result;
use chrono::{TimeZone, Utc};
use serde::Serialize;
use uuid::Uuid;

use crate::db::Db;

/// Questions per UTC day.
pub const DAILY_QUOTA: i32 = 3;
/// Days the trial stays open, counted from first sign-in.
pub const TRIAL_DAYS: i64 = 3;

/// ⚠️ Stored in `users.password_hash`, which is NOT NULL. It is deliberately not
/// a valid PHC string: `verify_password` parses the stored hash and returns
/// false when it cannot, so password login can never succeed for a trial
/// account no matter what is posted to `/api/login`.
const NO_PASSWORD_SENTINEL: &str = "!google-only-no-password";

#[derive(Debug, Clone, Serialize)]
pub struct TrialStatus {
    /// Whole UTC days since the trial began (0 on the first day).
    pub day_index: i64,
    pub used_today: i32,
    pub remaining_today: i32,
    /// Past its 3 days.
    pub expired: bool,
    /// Cut short by an admin.
    pub revoked: bool,
}

impl TrialStatus {
    /// The one question the query path asks.
    pub fn may_ask(&self) -> bool {
        !self.expired && !self.revoked && self.remaining_today > 0
    }
}

/// Today in UTC as `YYYY-MM-DD` — the quota bucket key.
///
/// UTC, not local: the reset instant has to be the same for every trial user
/// regardless of where they are, and a client-supplied timezone would let one
/// user reset their own quota by changing a clock.
fn today_utc() -> String {
    Utc::now().format("%Y-%m-%d").to_string()
}

/// Look up a trial by Google's stable subject id.
///
/// `sub`, not email: Google documents `sub` as the only claim that never
/// changes, while an email can be reassigned inside a Workspace domain.
pub async fn find_by_google_sub(pool: &Db, sub: &str) -> Result<Option<String>> {
    let row: Option<(String,)> =
        sqlx::query_as("SELECT user_id FROM trial_account WHERE google_sub = $1")
            .bind(sub)
            .fetch_optional(pool)
            .await?;
    Ok(row.map(|r| r.0))
}

/// Create the `users` row and its `trial_account` in one transaction.
///
/// Returns the new user id. The username is derived from the email but made
/// unique with a short suffix — `users.username` is UNIQUE and two Google
/// accounts can legitimately present the same local part on different domains.
pub async fn create(pool: &Db, google_sub: &str, email: &str, display_name: Option<&str>) -> Result<String> {
    let mut tx = pool.begin().await?;
    let id = Uuid::new_v4().to_string();
    let now = Utc::now().timestamp_millis();

    let stem: String = email
        .split('@')
        .next()
        .unwrap_or("trial")
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || *c == '.' || *c == '-' || *c == '_')
        .take(24)
        .collect();
    let username = format!("{}~{}", if stem.is_empty() { "trial".into() } else { stem }, &id[..6]);

    sqlx::query(
        "INSERT INTO users (id, username, password_hash, created_at, role) \
         VALUES ($1, $2, $3, $4, 'trial')",
    )
    .bind(&id)
    .bind(&username)
    // `users.created_at` is SECONDS in migration 005, unlike the millisecond
    // columns elsewhere. Do not "fix" this to millis.
    .bind(NO_PASSWORD_SENTINEL)
    .bind(Utc::now().timestamp())
    .execute(&mut *tx)
    .await?;

    sqlx::query(
        "INSERT INTO trial_account (user_id, google_sub, email, display_name, started_at) \
         VALUES ($1, $2, $3, $4, $5)",
    )
    .bind(&id)
    .bind(google_sub)
    .bind(email)
    .bind(display_name)
    .bind(now) // MILLISECONDS — see the migration header
    .execute(&mut *tx)
    .await?;

    tx.commit().await?;
    Ok(id)
}

/// Where this trial stands right now.
pub async fn status(pool: &Db, user_id: &str) -> Result<Option<TrialStatus>> {
    let row: Option<(i64, Option<i64>)> =
        sqlx::query_as("SELECT started_at, revoked_at FROM trial_account WHERE user_id = $1")
            .bind(user_id)
            .fetch_optional(pool)
            .await?;
    let Some((started_ms, revoked_at)) = row else {
        return Ok(None);
    };

    let used: Option<(i32,)> =
        sqlx::query_as("SELECT used FROM trial_usage WHERE user_id = $1 AND day_utc = $2")
            .bind(user_id)
            .bind(today_utc())
            .fetch_optional(pool)
            .await?;
    let used_today = used.map(|r| r.0).unwrap_or(0);

    // Whole days elapsed, measured on the UTC calendar rather than in
    // milliseconds: someone who signs up at 23:50 should not lose a day at
    // midnight ten minutes later, and 3 * 86_400_000 would do exactly that.
    let started_day = Utc
        .timestamp_millis_opt(started_ms)
        .single()
        .map(|d| d.date_naive())
        .unwrap_or_else(|| Utc::now().date_naive());
    let day_index = (Utc::now().date_naive() - started_day).num_days();

    Ok(Some(TrialStatus {
        day_index,
        used_today,
        remaining_today: (DAILY_QUOTA - used_today).max(0),
        expired: day_index >= TRIAL_DAYS,
        revoked: revoked_at.is_some(),
    }))
}

/// Spend one question. Returns `Ok(false)` when the quota is already gone.
///
/// ⚠️ The check and the increment are ONE statement. A `SELECT` followed by an
/// `UPDATE` lets two concurrent turns both read `used = 2` and both proceed, so
/// the conditional lives in the `WHERE` clause and the row count is the answer.
/// The daily bucket is created by the same statement via `ON CONFLICT`.
pub async fn consume(pool: &Db, user_id: &str) -> Result<bool> {
    let res = sqlx::query(
        "INSERT INTO trial_usage (user_id, day_utc, used) VALUES ($1, $2, 1) \
         ON CONFLICT (user_id, day_utc) DO UPDATE SET used = trial_usage.used + 1 \
         WHERE trial_usage.used < $3",
    )
    .bind(user_id)
    .bind(today_utc())
    .bind(DAILY_QUOTA)
    .execute(pool)
    .await?;
    Ok(res.rows_affected() > 0)
}

/// The documents a trial user may retrieve from.
///
/// ⚠️⚠️ FAILS CLOSED. An empty result means "this user may reach NOTHING" and
/// callers must treat it as an explicit empty allow-list. Everywhere else in the
/// retrieval path an absent scope means "everything"; if that convention leaks
/// in here, the whole 3,931-document corpus becomes public to anyone with a
/// Google account. Never map an empty vector to `None`.
pub async fn demo_source_ids(pool: &Db) -> Result<Vec<String>> {
    let rows: Vec<(String,)> = sqlx::query_as("SELECT source_id FROM demo_source")
        .fetch_all(pool)
        .await?;
    Ok(rows.into_iter().map(|r| r.0).collect())
}

/// Admin: put a document into, or take it out of, the demo corpus.
pub async fn set_demo_source(pool: &Db, source_id: &str, included: bool) -> Result<()> {
    if included {
        sqlx::query(
            "INSERT INTO demo_source (source_id, added_at) VALUES ($1, $2) \
             ON CONFLICT (source_id) DO NOTHING",
        )
        .bind(source_id)
        .bind(Utc::now().timestamp_millis())
        .execute(pool)
        .await?;
    } else {
        sqlx::query("DELETE FROM demo_source WHERE source_id = $1")
            .bind(source_id)
            .execute(pool)
            .await?;
    }
    Ok(())
}

/// One row of the admin's trial list.
#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct TrialRow {
    pub user_id: String,
    pub email: String,
    pub display_name: Option<String>,
    pub started_at: i64,
    pub revoked_at: Option<i64>,
    pub used_today: i32,
}

pub async fn list(pool: &Db, limit: i64) -> Result<Vec<TrialRow>> {
    let rows = sqlx::query_as::<_, TrialRow>(
        "SELECT t.user_id, t.email, t.display_name, t.started_at, t.revoked_at, \
                COALESCE(u.used, 0) AS used_today \
         FROM trial_account t \
         LEFT JOIN trial_usage u ON u.user_id = t.user_id AND u.day_utc = $1 \
         ORDER BY t.started_at DESC LIMIT $2",
    )
    .bind(today_utc())
    .bind(limit)
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

/// Admin: end a trial now. Sessions are revoked separately by the caller.
pub async fn revoke(pool: &Db, user_id: &str) -> Result<bool> {
    let res = sqlx::query("UPDATE trial_account SET revoked_at = $1 WHERE user_id = $2 AND revoked_at IS NULL")
        .bind(Utc::now().timestamp_millis())
        .bind(user_id)
        .execute(pool)
        .await?;
    Ok(res.rows_affected() > 0)
}
