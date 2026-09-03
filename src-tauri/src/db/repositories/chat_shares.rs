//! Repository: `chat_share` (migration 023).
//!
//! Feature 027 (US6) — a read-only link to one chat.
//!
//! ⚠️ **A share link never widens `visible_source_ids`.** The token selects a
//! chat; it does not authenticate, and it does not add the chat's documents to
//! the viewer's visible set. Had it done so, the viewer could immediately
//! global-search the owner's entire project, because global search scopes
//! through the same function — the link would become project access by the back
//! door. Viewers are authorised **per cited chunk** instead (ADR-0029, R5).
//!
use anyhow::Result;
use chrono::Utc;
use serde::{Deserialize, Serialize};

use crate::db::Db;

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct ChatShare {
    pub token: String,
    pub conversation_id: String,
    pub owner_id: String,
    pub created_at: i64,
    pub revoked_at: Option<i64>,
}

/// Mints a share link, or returns the existing live one.
///
/// Idempotent by design: re-sharing a chat must not accumulate credentials that
/// would then all have to be revoked separately. The partial unique index
/// `idx_chat_share_live` makes "at most one live link per chat" a schema
/// guarantee rather than a convention.
///
/// The token is the whole credential, so it comes from a v4 UUID pair (≥128 bits
/// of entropy) rather than anything derived from the conversation id — a token a
/// reader could guess from a chat they already knew about would be no protection
/// at all.
pub async fn mint(pool: &Db, conversation_id: &str, owner_id: &str) -> Result<String> {
    if let Some(existing) = live_for_conversation(pool, conversation_id).await? {
        return Ok(existing.token);
    }
    let token = format!(
        "{}{}",
        uuid::Uuid::new_v4().simple(),
        uuid::Uuid::new_v4().simple()
    );
    sqlx::query(
        "INSERT INTO chat_share (token, conversation_id, owner_id, created_at) \
         VALUES ($1, $2, $3, $4)",
    )
    .bind(&token)
    .bind(conversation_id)
    .bind(owner_id)
    .bind(Utc::now().timestamp_millis())
    .execute(pool)
    .await?;
    Ok(token)
}

/// Resolves a token to its LIVE share. A revoked token resolves to `None`, so
/// revocation kills the link for everyone at once (FR-031).
pub async fn resolve_live(pool: &Db, token: &str) -> Result<Option<ChatShare>> {
    Ok(sqlx::query_as::<_, ChatShare>(
        "SELECT token, conversation_id, owner_id, created_at, revoked_at \
         FROM chat_share WHERE token = $1 AND revoked_at IS NULL",
    )
    .bind(token)
    .fetch_optional(pool)
    .await?)
}

pub async fn live_for_conversation(
    pool: &Db,
    conversation_id: &str,
) -> Result<Option<ChatShare>> {
    Ok(sqlx::query_as::<_, ChatShare>(
        "SELECT token, conversation_id, owner_id, created_at, revoked_at \
         FROM chat_share WHERE conversation_id = $1 AND revoked_at IS NULL",
    )
    .bind(conversation_id)
    .fetch_optional(pool)
    .await?)
}

/// Soft revocation — the row is retained as history, matching how chat deletion
/// already works. Copies already taken from the link keep working, because a
/// copy is an independent project with no reference back to it.
pub async fn revoke(pool: &Db, conversation_id: &str, owner_id: &str) -> Result<()> {
    sqlx::query(
        "UPDATE chat_share SET revoked_at = $1 \
         WHERE conversation_id = $2 AND owner_id = $3 AND revoked_at IS NULL",
    )
    .bind(Utc::now().timestamp_millis())
    .bind(conversation_id)
    .bind(owner_id)
    .execute(pool)
    .await?;
    Ok(())
}

/// **The share-link authorisation predicate**: is this chunk cited by a turn of
/// this conversation?
///
/// This is what a share-link viewer is granted instead of a widened document
/// set. It is narrower than a document — a viewer can read the passages the
/// answer actually rests on and nothing else in the same file, let alone
/// anything else in the owner's project.
///
/// Citations are stored as `citations_json` on each `query_log` turn, so the
/// question is answered by scanning this conversation's turns for the chunk id.
/// A JSON containment test on text is deliberate: `citations_json` is TEXT (it
/// predates any structured citation table), and inventing one here would be a
/// larger change than this feature needs.
pub async fn is_chunk_cited_by(
    pool: &Db,
    conversation_id: &str,
    chunk_id: &str,
) -> Result<bool> {
    let rows: Vec<Option<String>> = sqlx::query_scalar(
        "SELECT citations_json FROM query_log WHERE conversation_id = $1",
    )
    .bind(conversation_id)
    .fetch_all(pool)
    .await?;

    for raw in rows.into_iter().flatten() {
        let Ok(value) = serde_json::from_str::<serde_json::Value>(&raw) else {
            continue;
        };
        if json_mentions_chunk(&value, chunk_id) {
            return Ok(true);
        }
    }
    Ok(false)
}

/// Walks a citation payload looking for an exact `chunk_id` match.
///
/// Compares whole string values rather than doing a substring search on the raw
/// JSON: a substring test would match a chunk id that merely happens to be a
/// prefix of another, which is a way in rather than merely a false positive.
fn json_mentions_chunk(value: &serde_json::Value, chunk_id: &str) -> bool {
    match value {
        serde_json::Value::String(s) => s == chunk_id,
        serde_json::Value::Array(items) => {
            items.iter().any(|v| json_mentions_chunk(v, chunk_id))
        }
        serde_json::Value::Object(map) => {
            map.values().any(|v| json_mentions_chunk(v, chunk_id))
        }
        _ => false,
    }
}

/// Is this conversation currently shared? Used by the chat list to show a
/// "shared" marker (FR-033) before the full US6 surface exists.
pub async fn is_shared(pool: &Db, conversation_id: &str) -> Result<bool> {
    let shared: bool = sqlx::query_scalar(
        "SELECT EXISTS (SELECT 1 FROM chat_share \
         WHERE conversation_id = $1 AND revoked_at IS NULL)",
    )
    .bind(conversation_id)
    .fetch_one(pool)
    .await?;
    Ok(shared)
}
