//! Per-user preferences (2026-07-29).
//!
//! Distinct from `db_config`, which holds SERVER-WIDE operator settings
//! (`chat_extractive`, `infer_engine`, …). These are the toggles an individual
//! officer sets for themselves.
//!
//! Convention: **an absent row means the default**, so a new preference needs no
//! backfill and a user who has never opened settings behaves exactly as before.
//! Booleans are stored as `'1'` / `'0'`.

use crate::db::Db;
use anyhow::Result;

/// Whether an answer ends with a "Next steps" follow-up offer. Default ON.
pub const KEY_REFINEMENT_OFFER: &str = "refinement_offer";

/// Read one preference. `None` when the user has never set it — the caller
/// supplies the default rather than this layer guessing one.
pub async fn get(pool: &Db, user_id: &str, key: &str) -> Result<Option<String>> {
    let v: Option<String> =
        sqlx::query_scalar("SELECT value FROM user_preference WHERE user_id = $1 AND key = $2")
            .bind(user_id)
            .bind(key)
            .fetch_optional(pool)
            .await?;
    Ok(v)
}

/// Read a boolean preference, falling back to `default` when unset.
pub async fn get_bool(pool: &Db, user_id: &str, key: &str, default: bool) -> Result<bool> {
    Ok(get(pool, user_id, key).await?.map(|v| v == "1").unwrap_or(default))
}

/// Upsert one preference.
pub async fn set(pool: &Db, user_id: &str, key: &str, value: &str) -> Result<()> {
    sqlx::query(
        "INSERT INTO user_preference (user_id, key, value, updated_at) VALUES ($1, $2, $3, $4) \
         ON CONFLICT (user_id, key) DO UPDATE SET value = EXCLUDED.value, updated_at = EXCLUDED.updated_at",
    )
    .bind(user_id)
    .bind(key)
    .bind(value)
    .bind(chrono::Utc::now().timestamp_millis())
    .execute(pool)
    .await?;
    Ok(())
}

/// Upsert a boolean preference.
pub async fn set_bool(pool: &Db, user_id: &str, key: &str, value: bool) -> Result<()> {
    set(pool, user_id, key, if value { "1" } else { "0" }).await
}
