//! Feature 027 — "My Projects" availability gate and sharing orchestration.
//!
//! The gate is deployment-wide (`db_config` key `my_projects_enabled`), read
//! server-side on every project request and applied at runtime with no restart.
//! It follows the shape `services::deep_search` established: the config read is
//! separated from the decision so the decision is unit-testable without a
//! database.
//!
//! ⚠️ **The gate is the SERVER refusal, not the hidden nav item.** Removing the
//! entry point from the UI is presentation; a request that bypasses the
//! interface must still be refused (FR-038). Deep search learned this once
//! already.

use anyhow::Result;

use crate::db::Db;

/// The `db_config` key. Seeded to `'0'` by migration 023, so applying the
/// migration exposes nothing until an administrator turns the feature on.
pub const GATE_KEY: &str = "my_projects_enabled";

/// Pure gate decision, with the config read lifted out for testability.
///
/// Deliberately has no role parameter: Feature 027's gate is deployment-wide
/// on/off for everyone, unlike deep search which is admin-always. A per-user
/// allowlist is explicitly out of scope and can be added here later without
/// touching any project schema.
pub fn gate_open(enabled: bool) -> bool {
    enabled
}

/// Reads the gate. A missing row reads as **off** — the safe direction, and the
/// state a deployment is in between applying migration 023 and an administrator
/// enabling the feature.
pub async fn my_projects_enabled(pool: &Db) -> Result<bool> {
    let raw: Option<String> =
        sqlx::query_scalar("SELECT value FROM db_config WHERE key = $1")
            .bind(GATE_KEY)
            .fetch_optional(pool)
            .await?;
    Ok(gate_open(raw.map(|v| v == "1").unwrap_or(false)))
}

/// Sets the gate. Applied at runtime; no restart, which is what FR-039 (off,
/// then on again, everything intact) depends on.
pub async fn set_my_projects_enabled(pool: &Db, enabled: bool) -> Result<()> {
    sqlx::query(
        "INSERT INTO db_config (key, value) VALUES ($1, $2) \
         ON CONFLICT (key) DO UPDATE SET value = excluded.value",
    )
    .bind(GATE_KEY)
    .bind(if enabled { "1" } else { "0" })
    .execute(pool)
    .await?;
    Ok(())
}

// ─── Copy-on-share (ADR-0028) ───────────────────────────────────────────────

/// Copies a project to another user. **This is what "sharing" means here.**
///
/// The recipient gets a NEW project that they own, holding the same documents.
/// The original is untouched, now and forever: nothing either side does — adding,
/// removing, renaming, deleting — is visible to the other (FR-021).
///
/// ⭐ **Metadata only.** One `project` row and N `project_source` rows. Zero
/// `sources` rows, zero chunks, zero embeddings, zero files copied. Two
/// consequences, both load-bearing:
///
/// 1. It completes in milliseconds rather than the minutes a re-index takes
///    (SC-003), because there is nothing to re-extract or re-embed.
/// 2. **Independence is a property of the data model, not of discipline.** Two
///    projects pointing at the same immutable content cannot affect each other
///    because neither owns it. No future code has to remember to preserve this.
///
/// The membership rows this writes are the first in the system to cross users —
/// migration 022 deferred exactly this, pending "a spec + ADR of its own". The
/// hazard it named was *implicit* dedup leaking a file's prior existence; this
/// is explicit and owner-initiated, so nothing is disclosed that the sharer did
/// not choose to disclose.
///
/// ⚠️ Chats are NOT copied (FR-023): the recipient starts with an empty
/// conversation list.
///
/// ⚠️ Because content now survives while ANY project holds it, the sharer can no
/// longer recall these documents from the copy (FR-024). The caller must have
/// said so before reaching here.
pub async fn copy_project(
    pool: &Db,
    source_project_id: &str,
    recipient_id: &str,
    name_override: Option<&str>,
) -> Result<String> {
    use crate::db::repositories::projects as project_repo;

    let source: Option<(String,)> = sqlx::query_as("SELECT name FROM project WHERE id = $1")
        .bind(source_project_id)
        .fetch_optional(pool)
        .await?;
    let Some((source_name,)) = source else {
        anyhow::bail!("project {source_project_id} not found");
    };
    let name = name_override.unwrap_or(&source_name);
    let now = chrono::Utc::now().timestamp_millis();
    let new_id = uuid::Uuid::new_v4().to_string();

    let mut tx = pool.begin().await?;
    sqlx::query(
        "INSERT INTO project (id, owner_id, name, created_at, updated_at, copied_from) \
         VALUES ($1, $2, $3, $4, $4, $5)",
    )
    .bind(&new_id)
    .bind(recipient_id)
    .bind(name)
    .bind(now)
    .bind(source_project_id)
    .execute(&mut *tx)
    .await?;

    // The memberships, in one statement. Note there is no `owner_id` to carry:
    // `project_source` deliberately has none, because the only owner it could
    // record is the SOURCE's — a different user for a copy — and any predicate
    // on it would break precisely this operation.
    sqlx::query(
        "INSERT INTO project_source (project_id, source_id, added_at) \
         SELECT $1, source_id, $2 FROM project_source WHERE project_id = $3 \
         ON CONFLICT (project_id, source_id) DO NOTHING",
    )
    .bind(&new_id)
    .bind(now)
    .bind(source_project_id)
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;

    let _ = project_repo::touch(pool, &new_id).await;
    Ok(new_id)
}

/// Copies one conversation's turns to a new conversation owned by `recipient_id`
/// (FR-030 — "fork to continue").
///
/// Chat history lives in `query_log` (migration 020), so a forked chat is a set
/// of new `query_log` rows under a new `conversation_id`. The original is not
/// modified, and the fork is not linked back to it: revoking the share link the
/// fork came from leaves the fork working (FR-031).
pub async fn copy_conversation_turns(
    pool: &Db,
    from_conversation_id: &str,
    to_conversation_id: &str,
    recipient_id: &str,
) -> Result<u64> {
    let rows: Vec<(String, Option<String>, Option<String>, i64)> = sqlx::query_as(
        "SELECT question, answer, citations_json, created_at FROM query_log \
         WHERE conversation_id = $1 ORDER BY created_at ASC",
    )
    .bind(from_conversation_id)
    .fetch_all(pool)
    .await?;

    let mut copied = 0u64;
    for (question, answer, citations_json, created_at) in rows {
        sqlx::query(
            "INSERT INTO query_log \
             (id, user_id, conversation_id, question, answer, citations_json, created_at) \
             VALUES ($1, $2, $3, $4, $5, $6, $7)",
        )
        .bind(uuid::Uuid::new_v4().to_string())
        .bind(recipient_id)
        .bind(to_conversation_id)
        .bind(&question)
        .bind(&answer)
        .bind(&citations_json)
        .bind(created_at)
        .execute(pool)
        .await?;
        copied += 1;
    }
    Ok(copied)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gate_closed_by_default() {
        assert!(!gate_open(false), "the gate must fail closed");
    }

    #[test]
    fn gate_open_when_enabled() {
        assert!(gate_open(true));
    }
}
