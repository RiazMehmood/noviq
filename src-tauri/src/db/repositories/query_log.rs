//! Repository: `query_log` table (F014 follow-on).
//!
//! One row per chat turn over the HTTP API — who asked, the question, the answer,
//! citations, latency, and the peak process/GPU resource use sampled during the
//! turn. Powers the admin user-management usage panel + per-user export.

use anyhow::Result;
use serde::Serialize;
use uuid::Uuid;

use crate::db::Db;
use crate::services::usage_sampler::ResourcePeak;

/// One persisted query record.
#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct QueryLogRow {
    pub id: String,
    pub user_id: Option<String>,
    pub conversation_id: Option<String>,
    pub question: String,
    pub answer: Option<String>,
    pub citations_json: Option<String>,
    pub latency_ms: Option<i64>,
    pub cpu_pct: Option<f32>,
    pub ram_mb: Option<f32>,
    pub gpu_util_pct: Option<f32>,
    pub gpu_mem_mb: Option<f32>,
    pub created_at: i64,
}

/// Per-user usage rollup (joined into the admin user list).
#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct UserUsage {
    pub user_id: String,
    pub query_count: i64,
    pub last_active: Option<i64>,
    pub avg_cpu_pct: Option<f64>,
    pub peak_ram_mb: Option<f64>,
    pub avg_gpu_util_pct: Option<f64>,
    pub avg_latency_ms: Option<f64>,
}

/// Insert one turn's record. Best-effort — callers ignore the result so logging
/// never fails a user's chat turn.
#[allow(clippy::too_many_arguments)]
pub async fn insert(
    pool: &Db,
    user_id: Option<&str>,
    conversation_id: &str,
    question: &str,
    answer: &str,
    citations_json: Option<&str>,
    // Full serialized `TurnResponse` (migration 020) — what `history::turns`
    // replays so a chat reopened on another device renders like the original,
    // not like a question/answer pair.
    response_json: Option<&str>,
    latency_ms: i64,
    peak: &ResourcePeak,
) -> Result<()> {
    let id = Uuid::new_v4().to_string();
    let now = chrono::Utc::now().timestamp_millis();
    sqlx::query(
        "INSERT INTO query_log \
         (id, user_id, conversation_id, question, answer, citations_json, response_json, \
          latency_ms, cpu_pct, ram_mb, gpu_util_pct, gpu_mem_mb, created_at) \
         VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13)",
    )
    .bind(&id)
    .bind(user_id)
    .bind(conversation_id)
    .bind(question)
    .bind(answer)
    .bind(citations_json)
    .bind(response_json)
    .bind(latency_ms)
    .bind(peak.cpu_pct)
    .bind(peak.ram_mb)
    .bind(peak.gpu_util_pct)
    .bind(peak.gpu_mem_mb)
    .bind(now)
    .execute(pool)
    .await?;
    Ok(())
}

/// Latest turn's `citations_json` for a conversation — the transform (rewrite)
/// capability reuses the prior answer's citations instead of re-retrieving.
pub async fn latest_citations_for_conversation(
    pool: &Db,
    conversation_id: &str,
) -> Result<Option<String>> {
    let row: Option<(Option<String>,)> = sqlx::query_as(
        "SELECT citations_json FROM query_log \
         WHERE conversation_id = $1 ORDER BY created_at DESC LIMIT 1",
    )
    .bind(conversation_id)
    .fetch_optional(pool)
    .await?;
    Ok(row.and_then(|(c,)| c))
}

/// Newest completed turn for one user + conversation recorded at/after
/// `since_ms` — the `poll_turn` recovery read (F022 tunnel-timeout fix). The
/// user filter keeps one user from reading another's turns even if they guess
/// a conversation id (ids are client-generated).
pub async fn latest_since(
    pool: &Db,
    user_id: &str,
    conversation_id: &str,
    since_ms: i64,
) -> Result<Option<QueryLogRow>> {
    Ok(sqlx::query_as::<_, QueryLogRow>(
        "SELECT id, user_id, conversation_id, question, answer, citations_json, \
                latency_ms, cpu_pct, ram_mb, gpu_util_pct, gpu_mem_mb, created_at \
         FROM query_log \
         WHERE user_id = $1 AND conversation_id = $2 AND created_at >= $3 \
         ORDER BY created_at DESC LIMIT 1",
    )
    .bind(user_id)
    .bind(conversation_id)
    .bind(since_ms)
    .fetch_optional(pool)
    .await?)
}

/// Recent queries for one user (newest first).
pub async fn list_for_user(pool: &Db, user_id: &str, limit: i64) -> Result<Vec<QueryLogRow>> {
    Ok(sqlx::query_as::<_, QueryLogRow>(
        "SELECT id, user_id, conversation_id, question, answer, citations_json, \
                latency_ms, cpu_pct, ram_mb, gpu_util_pct, gpu_mem_mb, created_at \
         FROM query_log WHERE user_id = $1 ORDER BY created_at DESC LIMIT $2",
    )
    .bind(user_id)
    .bind(limit)
    .fetch_all(pool)
    .await?)
}

/// All queries for one user (full export, newest first).
pub async fn export_for_user(pool: &Db, user_id: &str) -> Result<Vec<QueryLogRow>> {
    Ok(sqlx::query_as::<_, QueryLogRow>(
        "SELECT id, user_id, conversation_id, question, answer, citations_json, \
                latency_ms, cpu_pct, ram_mb, gpu_util_pct, gpu_mem_mb, created_at \
         FROM query_log WHERE user_id = $1 ORDER BY created_at DESC",
    )
    .bind(user_id)
    .fetch_all(pool)
    .await?)
}

// ── Account-bound chat history (migration 020) ──────────────────────────────
//
// These three are the OFFICER-facing reads: everything is filtered by `user_id`
// AND by `hidden_at IS NULL`. The admin reads above deliberately do NOT filter
// on `hidden_at` — hiding a chat removes it from the user's rail, never from the
// audit trail.

/// One conversation in the history rail.
#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct ConversationRow {
    pub conversation_id: String,
    /// The conversation's FIRST question — the same title the local rail derived,
    /// so a synced rail reads identically to the one it replaces.
    pub title: String,
    /// Newest turn's timestamp, for most-recent-first ordering.
    pub updated_at: i64,
    pub turn_count: i64,
}

/// One replayable turn (question + the assistant response that answered it).
#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct TurnRow {
    pub id: String,
    pub question: String,
    pub answer: Option<String>,
    pub citations_json: Option<String>,
    /// NULL for turns recorded before migration 020 — the client falls back to
    /// rendering `answer` + `citations_json`.
    pub response_json: Option<String>,
    pub created_at: i64,
}

/// This user's conversations, most recently active first.
///
/// `DISTINCT ON` picks the title row per conversation in one pass; the outer
/// query then reorders by recency. Postgres requires the `ORDER BY` of a
/// `DISTINCT ON` to lead with the distinct expression, hence the subquery.
pub async fn list_conversations(
    pool: &Db,
    user_id: &str,
    limit: i64,
) -> Result<Vec<ConversationRow>> {
    Ok(sqlx::query_as::<_, ConversationRow>(
        // `MAX(created_at)` and `COUNT(*)` are already bigint, so no casts are
        // needed — and `agg() OVER (...)::int8` is exactly the shape whose
        // precedence is worth not depending on.
        "SELECT conversation_id, title, updated_at, turn_count FROM ( \
             SELECT DISTINCT ON (conversation_id) \
                    conversation_id, \
                    LEFT(question, 60)                                  AS title, \
                    MAX(created_at) OVER (PARTITION BY conversation_id) AS updated_at, \
                    COUNT(*)        OVER (PARTITION BY conversation_id) AS turn_count \
             FROM query_log \
             WHERE user_id = $1 AND conversation_id IS NOT NULL AND hidden_at IS NULL \
             ORDER BY conversation_id, created_at ASC \
         ) c ORDER BY updated_at DESC LIMIT $2",
    )
    .bind(user_id)
    .bind(limit)
    .fetch_all(pool)
    .await?)
}

/// Every turn of one conversation, oldest first — the transcript replay.
///
/// Filtered by `user_id` as well as `conversation_id`: ids are generated by the
/// client, so without the owner check a guessed id would read another officer's
/// chat. Same invariant `latest_since` already enforces.
pub async fn conversation_turns(
    pool: &Db,
    user_id: &str,
    conversation_id: &str,
) -> Result<Vec<TurnRow>> {
    Ok(sqlx::query_as::<_, TurnRow>(
        "SELECT id, question, answer, citations_json, response_json, created_at \
         FROM query_log \
         WHERE user_id = $1 AND conversation_id = $2 AND hidden_at IS NULL \
         ORDER BY created_at ASC",
    )
    .bind(user_id)
    .bind(conversation_id)
    .fetch_all(pool)
    .await?)
}

/// Hide one conversation from this user's history on every device.
///
/// Returns the number of turns hidden. Idempotent — the `hidden_at IS NULL`
/// guard means re-hiding an already-hidden chat is a no-op rather than a
/// timestamp rewrite.
pub async fn hide_conversation(pool: &Db, user_id: &str, conversation_id: &str) -> Result<u64> {
    let now = chrono::Utc::now().timestamp_millis();
    let res = sqlx::query(
        "UPDATE query_log SET hidden_at = $1 \
         WHERE user_id = $2 AND conversation_id = $3 AND hidden_at IS NULL",
    )
    .bind(now)
    .bind(user_id)
    .bind(conversation_id)
    .execute(pool)
    .await?;
    Ok(res.rows_affected())
}

/// Per-user usage rollups keyed by `user_id`, for the admin list join.
pub async fn usage_by_user(pool: &Db) -> Result<Vec<UserUsage>> {
    Ok(sqlx::query_as::<_, UserUsage>(
        "SELECT user_id, \
                COUNT(*)::int8                 AS query_count, \
                MAX(created_at)::int8          AS last_active, \
                AVG(cpu_pct)::float8           AS avg_cpu_pct, \
                MAX(ram_mb)::float8            AS peak_ram_mb, \
                AVG(gpu_util_pct)::float8      AS avg_gpu_util_pct, \
                AVG(latency_ms)::float8        AS avg_latency_ms \
         FROM query_log WHERE user_id IS NOT NULL GROUP BY user_id",
    )
    .fetch_all(pool)
    .await?)
}
