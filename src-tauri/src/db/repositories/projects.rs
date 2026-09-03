//! Repository: `project`, `project_source` and `project_chat` (migration 023).
//!
//! Feature 027 — user-owned document workspaces. A project is a named container
//! holding its own documents and many chats, where retrieval is confined to that
//! project's documents (ADR-0029) and sharing produces an independent COPY
//! rather than shared access (ADR-0028).
//!
//! These are siblings of `notebooks`, not a replacement. `notebook_sources` has
//! the same shape and is not free: it is live, already means two different
//! things (per-conversation stub and per-folder stub), and four delete paths
//! interpret its rows.

use anyhow::Result;
use chrono::Utc;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::db::Db;

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct Project {
    pub id: String,
    pub owner_id: String,
    pub name: String,
    pub created_at: i64,
    pub updated_at: i64,
    /// Provenance only — never a live relationship. See migration 023.
    #[serde(default)]
    pub copied_from: Option<String>,
}

/// One document's membership in a project, with the indexing state the user
/// needs in order to know whether the set backing an answer is complete
/// (FR-008). A pending document must be visibly pending, never silently absent.
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct ProjectDocument {
    pub source_id: String,
    pub file_name: String,
    pub display_name: Option<String>,
    /// `sources.indexing_status`, surfaced verbatim so the UI can distinguish
    /// pending from failed rather than collapsing both to "missing".
    pub indexing_status: String,
    pub added_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct ProjectChat {
    pub conversation_id: String,
    pub project_id: String,
    pub owner_id: String,
    pub created_at: i64,
}

fn now_ms() -> i64 {
    Utc::now().timestamp_millis()
}

// ─── Ownership ──────────────────────────────────────────────────────────────

/// **The ownership check every client-supplied `project_id` must pass.**
///
/// Returns the project only when it exists AND the requesting user owns it;
/// `Ok(None)` covers both "no such project" and "not yours", because whether a
/// project the caller cannot see EXISTS is itself information — the same rule
/// `api/documents.rs::source_chunks` already follows.
///
/// ⚠️ Call this before building a `RequestScope` with `project_id` set. Scoping
/// to a project the caller owns can only NARROW what is retrievable; scoping to
/// one they do not own would WIDEN into another user's documents, turning
/// citation opening and global search into a way to read any project in the
/// database (FR-013, ADR-0029).
///
/// The chat turn path does NOT use this: it derives the project from
/// `conversation_id` via [`chat_for_conversation`], so the client never names a
/// project and there is nothing to check.
pub async fn owned_project(
    pool: &Db,
    user_id: &str,
    project_id: &str,
) -> Result<Option<Project>> {
    Ok(sqlx::query_as::<_, Project>(
        "SELECT id, owner_id, name, created_at, updated_at, copied_from \
         FROM project WHERE id = $1 AND owner_id = $2",
    )
    .bind(project_id)
    .bind(user_id)
    .fetch_optional(pool)
    .await?)
}

// ─── Project CRUD ───────────────────────────────────────────────────────────

pub async fn create(pool: &Db, owner_id: &str, name: &str) -> Result<Project> {
    let now = now_ms();
    let project = Project {
        id: Uuid::new_v4().to_string(),
        owner_id: owner_id.to_string(),
        name: name.trim().to_string(),
        created_at: now,
        updated_at: now,
        copied_from: None,
    };
    sqlx::query(
        "INSERT INTO project (id, owner_id, name, created_at, updated_at, copied_from) \
         VALUES ($1, $2, $3, $4, $5, NULL)",
    )
    .bind(&project.id)
    .bind(&project.owner_id)
    .bind(&project.name)
    .bind(project.created_at)
    .bind(project.updated_at)
    .execute(pool)
    .await?;
    Ok(project)
}

pub async fn rename(pool: &Db, project_id: &str, name: &str) -> Result<()> {
    sqlx::query("UPDATE project SET name = $1, updated_at = $2 WHERE id = $3")
        .bind(name.trim())
        .bind(now_ms())
        .bind(project_id)
        .execute(pool)
        .await?;
    Ok(())
}

/// Deletes the project, its memberships and its chat mappings (both cascade).
///
/// Documents are NOT deleted here. Removing a membership only makes the content
/// *eligible* for deletion; whether it is actually destroyed is decided by
/// `sources::is_referenced`, because another project — possibly another user's
/// copy — may still hold it (FR-007, FR-024).
pub async fn delete(pool: &Db, project_id: &str) -> Result<()> {
    sqlx::query("DELETE FROM project WHERE id = $1")
        .bind(project_id)
        .execute(pool)
        .await?;
    Ok(())
}

/// FR-004 — the caller's projects, most recently active first.
pub async fn list_for_owner(pool: &Db, owner_id: &str) -> Result<Vec<Project>> {
    Ok(sqlx::query_as::<_, Project>(
        "SELECT id, owner_id, name, created_at, updated_at, copied_from \
         FROM project WHERE owner_id = $1 ORDER BY updated_at DESC",
    )
    .bind(owner_id)
    .fetch_all(pool)
    .await?)
}

/// Bump last activity. Called when a chat is started or a document is added, so
/// the FR-004 ordering reflects work rather than creation date.
pub async fn touch(pool: &Db, project_id: &str) -> Result<()> {
    sqlx::query("UPDATE project SET updated_at = $1 WHERE id = $2")
        .bind(now_ms())
        .bind(project_id)
        .execute(pool)
        .await?;
    Ok(())
}

// ─── Membership ─────────────────────────────────────────────────────────────

/// Adds documents to a project. Idempotent — re-adding one already present is
/// not an error (the duplicate-upload edge case). Returns how many rows were
/// genuinely new.
pub async fn add_sources(pool: &Db, project_id: &str, source_ids: &[String]) -> Result<u64> {
    let now = now_ms();
    let mut added = 0u64;
    for source_id in source_ids {
        let result = sqlx::query(
            "INSERT INTO project_source (project_id, source_id, added_at) \
             VALUES ($1, $2, $3) ON CONFLICT (project_id, source_id) DO NOTHING",
        )
        .bind(project_id)
        .bind(source_id)
        .bind(now)
        .execute(pool)
        .await?;
        added += result.rows_affected();
    }
    touch(pool, project_id).await?;
    Ok(added)
}

/// Removes ONE membership. Never touches content — see [`delete`].
pub async fn remove_source(pool: &Db, project_id: &str, source_id: &str) -> Result<()> {
    sqlx::query("DELETE FROM project_source WHERE project_id = $1 AND source_id = $2")
        .bind(project_id)
        .bind(source_id)
        .execute(pool)
        .await?;
    touch(pool, project_id).await?;
    Ok(())
}

/// The project's documents with their indexing state (FR-008).
pub async fn documents(pool: &Db, project_id: &str) -> Result<Vec<ProjectDocument>> {
    Ok(sqlx::query_as::<_, ProjectDocument>(
        "SELECT s.id AS source_id, s.file_name, s.display_name, \
                s.indexing_status, ps.added_at \
         FROM project_source ps JOIN sources s ON s.id = ps.source_id \
         WHERE ps.project_id = $1 ORDER BY ps.added_at DESC",
    )
    .bind(project_id)
    .fetch_all(pool)
    .await?)
}

/// How many documents in this project are ready to answer from.
///
/// FR-012 needs this rather than a plain count: a project holding only pending
/// or failed documents has nothing to search, and must say so rather than
/// answering from somewhere else.
pub async fn ready_document_count(pool: &Db, project_id: &str) -> Result<i64> {
    Ok(sqlx::query_scalar(
        "SELECT COUNT(*) FROM project_source ps JOIN sources s ON s.id = ps.source_id \
         WHERE ps.project_id = $1 AND s.indexing_status = 'indexed' AND s.disabled_at IS NULL",
    )
    .bind(project_id)
    .fetch_one(pool)
    .await?)
}

// ─── Project ↔ conversation mapping ─────────────────────────────────────────

/// Binds a conversation to a project. Written SERVER-SIDE only — this is the
/// single place a chat becomes a project chat, and the reason a chat turn never
/// carries a client-supplied `project_id`.
pub async fn create_chat(
    pool: &Db,
    conversation_id: &str,
    project_id: &str,
    owner_id: &str,
) -> Result<()> {
    sqlx::query(
        "INSERT INTO project_chat (conversation_id, project_id, owner_id, created_at) \
         VALUES ($1, $2, $3, $4) ON CONFLICT (conversation_id) DO NOTHING",
    )
    .bind(conversation_id)
    .bind(project_id)
    .bind(owner_id)
    .bind(now_ms())
    .execute(pool)
    .await?;
    touch(pool, project_id).await?;
    Ok(())
}

/// Resolves a conversation to its project, if it has one.
///
/// The caller MUST compare `owner_id` against the authenticated user before
/// using the result to build a scope. A forged or guessed `conversation_id`
/// yields either `None` or another user's row, and it is that comparison which
/// rejects the second case (ADR-0029).
/// Drop one chat's project membership. Returns whether a row went.
///
/// Only the membership — the transcript is soft-deleted via `hide_conversation`
/// (migration 020 keeps `query_log` for the audit trail) and the attachments by
/// `delete_conversation`. Nothing else deletes from `project_chat`: it cascades
/// from `project` alone, so a chat deleted without this would linger in the
/// project's listing forever.
pub async fn delete_chat(pool: &Db, conversation_id: &str) -> Result<bool> {
    let res = sqlx::query("DELETE FROM project_chat WHERE conversation_id = $1")
        .bind(conversation_id)
        .execute(pool)
        .await?;
    Ok(res.rows_affected() > 0)
}

pub async fn chat_for_conversation(
    pool: &Db,
    conversation_id: &str,
) -> Result<Option<ProjectChat>> {
    Ok(sqlx::query_as::<_, ProjectChat>(
        "SELECT conversation_id, project_id, owner_id, created_at \
         FROM project_chat WHERE conversation_id = $1",
    )
    .bind(conversation_id)
    .fetch_optional(pool)
    .await?)
}

/// FR-033 — the conversations belonging to one project, newest first.
pub async fn chats_for_project(pool: &Db, project_id: &str) -> Result<Vec<ProjectChat>> {
    Ok(sqlx::query_as::<_, ProjectChat>(
        "SELECT conversation_id, project_id, owner_id, created_at \
         FROM project_chat WHERE project_id = $1 ORDER BY created_at DESC",
    )
    .bind(project_id)
    .fetch_all(pool)
    .await?)
}
