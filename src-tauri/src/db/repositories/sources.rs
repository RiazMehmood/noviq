//! Repository: `sources` and `notebook_sources` tables.

use anyhow::Result;
use chrono::Utc;
use serde::{Deserialize, Serialize};
use crate::db::Db;
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct Source {
    pub id: String,
    pub file_path: String,
    pub file_name: String,
    pub format: String,
    pub file_size_bytes: i64,
    pub file_hash: String,
    pub indexing_status: String,
    pub error_reason: Option<String>,
    pub last_modified: i64,
    pub indexed_at: Option<i64>,
    pub created_at: i64,
    #[serde(default = "default_index_tier")]
    pub index_tier: String,
    // Feature 021 — ownership scope (migration 009). `owner_id` NULL = global
    // corpus; `conversation_id` set = conversation attachment (always private).
    #[serde(default)]
    pub owner_id: Option<String>,
    #[serde(default = "default_visibility")]
    pub visibility: String,
    #[serde(default)]
    pub conversation_id: Option<String>,
    // Migration 021 — corpus organisation. `category_id` NULL = Uncategorised,
    // where every pre-021 document and every new upload starts. `disabled_at`
    // NULL = in service; set = withdrawn, and excluded by `visible_source_ids`
    // below. Both default so a row selected before 021 applied still decodes.
    #[serde(default)]
    pub category_id: Option<String>,
    #[serde(default)]
    pub disabled_at: Option<i64>,
}

fn default_index_tier() -> String {
    "fast".to_string()
}

fn default_visibility() -> String {
    "shared".to_string()
}

/// Feature 021 — who is asking, for retrieval scoping. Built per request on the
/// web (`api/query.rs`); desktop (single operator, no auth) uses
/// `unrestricted()` which preserves the pre-021 behaviour exactly.
#[derive(Debug, Clone, Default)]
pub struct RequestScope {
    pub user_id: Option<String>,
    pub conversation_id: Option<String>,
    /// Feature 027 — `Some` switches `visible_source_ids` into EXCLUSIVE mode:
    /// this project's documents and nothing else. `None` keeps every pre-027
    /// caller on the additive path unchanged.
    ///
    /// ⚠️ Never populate this from a client-supplied value without checking
    /// ownership first (`projects::owned_project`). Scoping to a project the
    /// caller owns can only NARROW; scoping to one they do not own would WIDEN
    /// into another user's documents. On a chat turn the server derives it from
    /// `project_chat` instead, so the client cannot name a project at all.
    pub project_id: Option<String>,
    /// Feature 027 — include the shared legal corpus in EXCLUSIVE mode. Read
    /// only when `project_id` is `Some`; ignored otherwise (the additive path
    /// already returns the corpus). Per-question by decision: it lives on the
    /// request and is never persisted, which is what makes it reset every turn.
    pub include_corpus: bool,
    /// Migration 029 — this request belongs to a FREE-TRIAL user.
    ///
    /// ⚠️⚠️ Switches `visible_source_ids` into DEMO-ONLY mode, and it is checked
    /// BEFORE every other branch. A trial user is an anonymous member of the
    /// public; the corpus is 3,931 Government of Sindh documents. The mode
    /// FAILS CLOSED — an empty `demo_source` table yields an empty allow-list,
    /// meaning "reaches nothing", never "unrestricted".
    ///
    /// Set by the server from the session's role. NEVER from anything a client
    /// sends: a client-settable value here could only ever be set to `false`.
    pub trial: bool,
}

impl RequestScope {
    pub fn unrestricted() -> Self {
        Self {
            user_id: None,
            conversation_id: None,
            project_id: None,
            include_corpus: false,
            trial: false,
        }
    }

    /// ⚠️ `trial` must be part of this test. A trial request is never
    /// unrestricted, and omitting it here would drop the request into the
    /// "everything indexed" branch — publishing the whole corpus to anyone with
    /// a Google account.
    pub fn is_unrestricted(&self) -> bool {
        !self.trial
            && self.user_id.is_none()
            && self.conversation_id.is_none()
            && self.project_id.is_none()
    }

    /// Feature 027 — true when this request is confined to one project.
    pub fn is_project_scoped(&self) -> bool {
        self.project_id.is_some()
    }
}

/// Feature 021 — THE retrieval-privacy invariant, in one place (research D2).
/// A requesting user may retrieve from:
///   1. the shared/global corpus       (visibility='shared', not an attachment)
///   2. their own documents            (owner_id = user, not an attachment)
///   3. the active conversation's attachments — matched on conversation_id **and**
///      owner_id. The owner predicate is not redundant: `conversation_id` arrives
///      from the request body (`api/query.rs`) and is never checked against the
///      caller, so matching on it alone let anyone holding another user's
///      conversation id retrieve that conversation's attachments. Every other
///      query touching attachments (`api/documents.rs`) already pairs the two
///      columns; this one did not.
///
/// Migration 022 — branch 3 now reads `source_link` instead of
/// `sources.conversation_id`, because one indexed document can be attached to
/// several conversations. Both columns of the pair moved together onto the link
/// row, so the owner predicate above still holds exactly as written; the
/// migration backfills a link for every existing attachment, so the first
/// request after deploy sees the same set the column-based scoping saw.
/// `unrestricted()` (desktop) returns every indexed source — today's behaviour.
///
/// Migration 021 — a document the admin has DISABLED (`disabled_at IS NOT NULL`)
/// is excluded from every branch, desktop included. Putting it here rather than
/// in each caller is the whole point: this one function is what chat retrieval,
/// global search and citation opening all scope through, so a disable takes
/// effect everywhere at once and on the next turn, with no re-index. Note that
/// the corpus LISTING reads deliberately still return disabled documents (greyed,
/// marked unavailable) — listing and retrieval are different questions and must
/// not be collapsed into one query.
///
/// ─── Feature 027 (migration 023, ADR-0029) — EXCLUSIVE mode ─────────────────
///
/// **This function is the only legal place to scope retrieval. Filtering the
/// returned Vec at a call site is forbidden.** Six call sites funnel through
/// here — chat (`api/query.rs`), global search, citation→passage,
/// citation→file, the knowledge graph (`api/documents.rs`) and the desktop
/// assistant. That single point is why an admin disable takes effect on every
/// route at once with no re-index, and why project isolation is inherited by
/// global search and citation opening for free. Scope anywhere else and it WILL
/// leak through one of the other five.
///
/// Every branch above ADDS: the three are OR'd, so each only ever widens the
/// set. Project scoping is the first requirement that must SUBTRACT, so it is a
/// MODE rather than a fourth branch — a fourth OR'd branch cannot express "and
/// nothing else".
///
/// In exclusive mode a source is visible **iff** it is a member of the named
/// project, indexed, and not disabled. Note what is NOT consulted:
///
/// - **`owner_id` is ignored, deliberately.** A shared project is a COPY that
///   references the *sharer's* `sources` rows (ADR-0028), so `owner_id` names a
///   different user than the caller and any predicate on it would break sharing.
///   Membership, not ownership, is the authority inside a project. This will
///   read as a missing predicate to anyone without that context; it is not.
/// - **Conversation attachments are excluded.** A project question reaches the
///   project's documents, not whatever happens to be paper-clipped into the
///   chat.
///
/// The caller is responsible for having proved the requester owns the project
/// (`projects::owned_project`) BEFORE building a scope with `project_id` set.
/// This function cannot check that without a second query on every call, so the
/// invariant lives in that helper and in the per-route tests that prove each
/// route calls it.
pub async fn visible_source_ids(pool: &Db, scope: &RequestScope) -> Result<Vec<String>> {
    // Migration 029 — DEMO-ONLY mode, checked before everything else.
    //
    // ⚠️⚠️ This branch must stay first and must stay total: it returns, it never
    // falls through. A trial user may reach the curated demo documents and
    // nothing else — not the corpus, not another user's uploads, not their own
    // attachments. The intersection with `indexing_status`/`disabled_at` is
    // deliberate: an admin withdrawing a document must withdraw it from the
    // demo too, without having to remember `demo_source` exists.
    //
    // An empty `demo_source` yields an empty Vec, which every caller treats as
    // "retrieve nothing" and which produces a refusal, not a corpus dump.
    if scope.trial {
        return Ok(sqlx::query_scalar(
            "SELECT s.id FROM sources s              JOIN demo_source d ON d.source_id = s.id              WHERE s.indexing_status = 'indexed' AND s.disabled_at IS NULL",
        )
        .fetch_all(pool)
        .await?);
    }
    // F027 — EXCLUSIVE mode. Checked first: it is the only path that subtracts,
    // and it must not fall through to the additive branches below.
    if let Some(project_id) = scope.project_id.as_deref() {
        return Ok(sqlx::query_scalar(
            "SELECT id FROM sources \
             WHERE indexing_status = 'indexed' AND disabled_at IS NULL AND ( \
                   EXISTS (SELECT 1 FROM project_source ps \
                           WHERE ps.source_id = sources.id AND ps.project_id = $1) \
                OR ($2::bool AND visibility = 'shared' AND conversation_id IS NULL) \
             )",
        )
        .bind(project_id)
        .bind(scope.include_corpus)
        .fetch_all(pool)
        .await?);
    }
    // Migration 024 — a document UPLOADED INTO a project is never part of a
    // GENERAL retrieval scope, whoever is asking and however they own it. The
    // project branch above reaches it by membership; every branch below must
    // not. Without this the officer's own project files answered their ordinary
    // chat questions, which is the same confusion the project mode exists to
    // remove — only worse, because here it changes the ANSWER, not the rail.
    if scope.is_unrestricted() {
        return Ok(sqlx::query_scalar(
            "SELECT id FROM sources \
             WHERE indexing_status = 'indexed' AND disabled_at IS NULL \
               AND project_id IS NULL",
        )
        .fetch_all(pool)
        .await?);
    }
    Ok(sqlx::query_scalar(
        "SELECT id FROM sources \
         WHERE indexing_status = 'indexed' AND disabled_at IS NULL \
           AND project_id IS NULL AND ( \
               (visibility = 'shared' AND conversation_id IS NULL) \
            OR ($1::text IS NOT NULL AND owner_id = $1 AND conversation_id IS NULL) \
            OR ($1::text IS NOT NULL AND $2::text IS NOT NULL \
                AND EXISTS (SELECT 1 FROM source_link l \
                            WHERE l.source_id = sources.id \
                              AND l.conversation_id = $2 AND l.owner_id = $1)) \
         )",
    )
    .bind(scope.user_id.as_deref())
    .bind(scope.conversation_id.as_deref())
    .fetch_all(pool)
    .await?)
}

/// Feature 027 (migration 023) — is this source still reachable from anywhere?
///
/// ⚠️ **Every delete path MUST consult this before destroying content.** A
/// source's content (chunks, embeddings, graph rows, the file on disk) may now
/// be referenced from three places, and after migration 023 the third can belong
/// to a **different user**:
///
/// | reference | since | cross-user? |
/// |---|---|---|
/// | `notebook_sources` | F001 | no |
/// | `source_link`      | 022  | no — per-user by design |
/// | `project_source`   | 023  | **yes** |
///
/// Before 023, every reference to a source belonged to the same user, so
/// "delete my document" could only ever destroy the deleting user's own data.
/// After it, a delete path that skips this check silently destroys content
/// another user's project depends on — and FR-024 guarantees the opposite: a
/// document shared into someone else's project keeps working there even after
/// the original owner deletes it.
pub async fn is_referenced(pool: &Db, source_id: &str) -> Result<bool> {
    let referenced: bool = sqlx::query_scalar(
        "SELECT EXISTS (SELECT 1 FROM project_source  WHERE source_id = $1) \
             OR EXISTS (SELECT 1 FROM source_link     WHERE source_id = $1) \
             OR EXISTS (SELECT 1 FROM notebook_sources WHERE source_id = $1)",
    )
    .bind(source_id)
    .fetch_one(pool)
    .await?;
    Ok(referenced)
}

/// Feature 021 — per-user allowance usage: (bytes, files) over everything the
/// user owns, live conversation attachments included (FR-019).
///
/// F027 note: a project COPY adds `project_source` rows and never a `sources`
/// row, so a user who receives a shared project is charged nothing for it. That
/// is both correct (copying stores no new bytes) and free — this predicate
/// already keys on the column a copy does not change.
pub async fn usage_for_owner(pool: &Db, user_id: &str) -> Result<(i64, i64)> {
    let row: (Option<i64>, i64) = sqlx::query_as(
        // SUM(bigint) yields NUMERIC in PG; cast back so it decodes as i64.
        "SELECT SUM(file_size_bytes)::BIGINT, COUNT(*) FROM sources WHERE owner_id = $1",
    )
    .bind(user_id)
    .fetch_one(pool)
    .await?;
    Ok((row.0.unwrap_or(0), row.1))
}

/// F021 — flip one source's visibility (FR-013 per-document toggle).
pub async fn set_visibility_source(pool: &Db, source_id: &str, visibility: &str) -> Result<()> {
    sqlx::query("UPDATE sources SET visibility = $1 WHERE id = $2")
        .bind(visibility)
        .bind(source_id)
        .execute(pool)
        .await?;
    Ok(())
}

/// F021 — owner of a source (`None` = global/system). `Ok(None)` also when the
/// source does not exist — callers treat that as not-authorized/not-found.
pub async fn owner_of(pool: &Db, source_id: &str) -> Result<Option<String>> {
    let owner: Option<Option<String>> =
        sqlx::query_scalar("SELECT owner_id FROM sources WHERE id = $1")
            .bind(source_id)
            .fetch_optional(pool)
            .await?;
    Ok(owner.flatten())
}

/// Attachment source ids for a conversation — the *live* attachments the user
/// paper-clipped into THIS chat (still `conversation_id`-scoped; a "Kept"
/// attachment loses its `conversation_id` and becomes an ordinary user doc, so
/// it is intentionally excluded here). Used to resolve "this/the attached
/// document" references to a strict retrieval scope (attachment-only).
pub async fn attachment_source_ids(pool: &Db, conversation_id: &str) -> Result<Vec<String>> {
    Ok(sqlx::query_scalar(
        "SELECT id FROM sources \
         WHERE conversation_id = $1 AND indexing_status = 'indexed'",
    )
    .bind(conversation_id)
    .fetch_all(pool)
    .await?)
}

/// File names of this conversation's attachments — enough to tell a spreadsheet
/// from a notice without opening anything.
pub async fn attachment_file_names(pool: &Db, conversation_id: &str) -> Result<Vec<String>> {
    Ok(sqlx::query_scalar(
        "SELECT file_name FROM sources \
         WHERE conversation_id = $1 AND indexing_status = 'indexed'",
    )
    .bind(conversation_id)
    .fetch_all(pool)
    .await?)
}

/// Feature 021 — ownership stamped on a new source row, resolved from the
/// notebook it is being indexed into (see `scope_for_notebook`).
#[derive(Debug, Clone, Default)]
pub struct SourceScope {
    pub owner_id: Option<String>,
    pub visibility: Option<String>, // None → 'shared' (pre-021 default)
    pub conversation_id: Option<String>,
}

/// Resolve the ownership a brand-new source must carry from its target
/// notebook: `__folder__<id>` inherits the folder's owner + CURRENT visibility;
/// `__conv__<id>` is a conversation attachment (private; owner is stamped by
/// the attach endpoint's pre-created row — this fallback fails closed).
/// Anything else (plain notebooks, desktop) → global/shared, pre-021 behaviour.
pub async fn scope_for_notebook(pool: &Db, notebook_id: &str) -> SourceScope {
    if let Some(folder_id) = notebook_id.strip_prefix("__folder__") {
        let row: Option<(Option<String>, String)> = sqlx::query_as(
            "SELECT owner_id, visibility FROM indexed_folders WHERE id = $1",
        )
        .bind(folder_id)
        .fetch_optional(pool)
        .await
        .ok()
        .flatten();
        if let Some((owner_id, visibility)) = row {
            return SourceScope { owner_id, visibility: Some(visibility), conversation_id: None };
        }
    }
    if let Some(conv_id) = notebook_id.strip_prefix("__conv__") {
        return SourceScope {
            owner_id: None, // attach endpoint pre-creates the row with the real owner
            visibility: Some("private".to_string()),
            conversation_id: Some(conv_id.to_string()),
        };
    }
    SourceScope::default()
}

/// Migration 022 — is this content already indexed FOR THIS USER?
///
/// The dedup key is (owner, sha256), never the file path: attachments land in a
/// per-conversation directory, so the path differs every time even when the
/// bytes are identical, which is why `indexer.rs`'s path-based dedup never fired
/// for them. Scoped to one owner by decision — see `022_source_links.sql`.
///
/// Only `indexed` rows qualify. Linking to a row that is still `processing` or
/// that `failed` would hand the caller a source with a partial or empty chunk
/// set and no way to notice; a fresh index is the safe answer there. Disabled
/// documents (021) are excluded for the same reason `visible_source_ids`
/// excludes them — they are withdrawn from service.
pub async fn find_indexed_by_owner_hash(
    pool: &Db,
    owner_id: &str,
    file_hash: &str,
) -> Result<Option<Source>> {
    if file_hash.is_empty() {
        return Ok(None);
    }
    Ok(sqlx::query_as::<_, Source>(
        "SELECT * FROM sources \
         WHERE owner_id = $1 AND file_hash = $2 \
           AND indexing_status = 'indexed' AND disabled_at IS NULL \
         ORDER BY created_at ASC LIMIT 1",
    )
    .bind(owner_id)
    .bind(file_hash)
    .fetch_optional(pool)
    .await?)
}

/// Migration 022 — make `source_id` reachable from `conversation_id`.
/// Idempotent: re-attaching the same file to the same chat is a no-op, not an
/// error, so the caller never has to check first.
pub async fn link_to_conversation(
    pool: &Db,
    source_id: &str,
    conversation_id: &str,
    owner_id: &str,
) -> Result<()> {
    sqlx::query(
        "INSERT INTO source_link (source_id, conversation_id, owner_id, linked_at) \
         VALUES ($1, $2, $3, $4) ON CONFLICT (source_id, conversation_id) DO NOTHING",
    )
    .bind(source_id)
    .bind(conversation_id)
    .bind(owner_id)
    .bind(Utc::now().timestamp_millis())
    .execute(pool)
    .await?;
    Ok(())
}

/// Migration 022 — drop this user's links for a conversation and return the
/// sources that were linked, so the caller can decide which are now orphaned.
/// Deleting the links is NOT deleting the content: that judgement belongs to
/// `is_orphaned` below, because the same bytes may still be linked elsewhere.
pub async fn unlink_conversation(
    pool: &Db,
    conversation_id: &str,
    owner_id: &str,
) -> Result<Vec<String>> {
    Ok(sqlx::query_scalar(
        "DELETE FROM source_link WHERE conversation_id = $1 AND owner_id = $2 \
         RETURNING source_id",
    )
    .bind(conversation_id)
    .bind(owner_id)
    .fetch_all(pool)
    .await?)
}

/// Migration 022 — may this source's content be destroyed?
///
/// True only when nothing links to it any more. This is the check that stops
/// deleting one chat from destroying a spreadsheet eleven other chats still
/// answer from — the failure mode that dedup INTRODUCES and that did not exist
/// while every attachment owned a private copy.
pub async fn is_orphaned(pool: &Db, source_id: &str) -> Result<bool> {
    let remaining: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM source_link WHERE source_id = $1",
    )
    .bind(source_id)
    .fetch_one(pool)
    .await?;
    Ok(remaining == 0)
}

/// Insert a new source. Returns the inserted row.
/// Pre-021 signature — global/shared row (desktop + plain notebooks).
pub async fn insert(
    pool: &Db,
    file_path: &str,
    file_name: &str,
    format: &str,
    file_size_bytes: i64,
    file_hash: &str,
    last_modified: i64,
) -> Result<Source> {
    insert_scoped(
        pool, file_path, file_name, format, file_size_bytes, file_hash,
        last_modified, &SourceScope::default(),
    )
    .await
}

/// Feature 021 — insert with ownership stamped atomically at creation, so a
/// private document is never visible in the window before a post-hoc UPDATE.
pub async fn insert_scoped(
    pool: &Db,
    file_path: &str,
    file_name: &str,
    format: &str,
    file_size_bytes: i64,
    file_hash: &str,
    last_modified: i64,
    scope: &SourceScope,
) -> Result<Source> {
    let id = Uuid::new_v4().to_string();
    let now = Utc::now().timestamp_millis();
    // Invariant I1: an attachment is always private.
    let visibility = if scope.conversation_id.is_some() {
        "private".to_string()
    } else {
        scope.visibility.clone().unwrap_or_else(|| "shared".to_string())
    };

    sqlx::query(
        "INSERT INTO sources \
         (id, file_path, file_name, format, file_size_bytes, file_hash, \
          indexing_status, last_modified, created_at, owner_id, visibility, conversation_id) \
         VALUES ($1, $2, $3, $4, $5, $6, 'pending', $7, $8, $9, $10, $11)",
    )
    .bind(&id)
    .bind(file_path)
    .bind(file_name)
    .bind(format)
    .bind(file_size_bytes)
    .bind(file_hash)
    .bind(last_modified)
    .bind(now)
    .bind(scope.owner_id.as_deref())
    .bind(&visibility)
    .bind(scope.conversation_id.as_deref())
    .execute(pool)
    .await?;

    Ok(Source {
        id,
        file_path: file_path.to_string(),
        file_name: file_name.to_string(),
        format: format.to_string(),
        file_size_bytes,
        file_hash: file_hash.to_string(),
        indexing_status: "pending".to_string(),
        error_reason: None,
        last_modified,
        indexed_at: None,
        created_at: now,
        index_tier: "fast".to_string(),
        owner_id: scope.owner_id.clone(),
        visibility,
        conversation_id: scope.conversation_id.clone(),
        // Migration 021 — the INSERT above sets neither column, so both are NULL
        // on the row this mirrors: a new document is Uncategorised and in
        // service. The admin files it and can withdraw it afterwards.
        category_id: None,
        disabled_at: None,
    })
}

pub async fn get_by_path(pool: &Db, file_path: &str) -> Result<Option<Source>> {
    Ok(
        sqlx::query_as::<_, Source>("SELECT * FROM sources WHERE file_path = $1")
            .bind(file_path)
            .fetch_optional(pool)
            .await?,
    )
}

pub async fn get_by_id(pool: &Db, id: &str) -> Result<Option<Source>> {
    Ok(
        sqlx::query_as::<_, Source>("SELECT * FROM sources WHERE id = $1")
            .bind(id)
            .fetch_optional(pool)
            .await?,
    )
}

pub async fn list_by_notebook(pool: &Db, notebook_id: &str) -> Result<Vec<Source>> {
    Ok(sqlx::query_as::<_, Source>(
        "SELECT s.* FROM sources s \
         JOIN notebook_sources ns ON ns.source_id = s.id \
         WHERE ns.notebook_id = $1 \
         ORDER BY ns.added_at DESC",
    )
    .bind(notebook_id)
    .fetch_all(pool)
    .await?)
}

/// Attach a source to a notebook (join table entry).
pub async fn attach_to_notebook(
    pool: &Db,
    notebook_id: &str,
    source_id: &str,
) -> Result<()> {
    let now = Utc::now().timestamp_millis();
    sqlx::query(
        "INSERT INTO notebook_sources (notebook_id, source_id, added_at) VALUES ($1, $2, $3) ON CONFLICT (notebook_id, source_id) DO NOTHING",
    )
    .bind(notebook_id)
    .bind(source_id)
    .bind(now)
    .execute(pool)
    .await?;
    Ok(())
}

/// Feature 005 — return the user-set `doc_type` override for a source, or
/// `None` if doc_type_source is `'auto'` / `'llm'` / NULL. Used by the
/// re-index path so user overrides survive re-detection.
pub async fn get_doc_type_override(pool: &Db, source_id: &str) -> Result<Option<String>> {
    let row: Option<Option<String>> = sqlx::query_scalar(
        "SELECT doc_type FROM sources WHERE id = $1 AND doc_type_source = 'user'",
    )
    .bind(source_id)
    .fetch_optional(pool)
    .await?;
    Ok(row.flatten())
}

pub async fn update_status(pool: &Db, id: &str, status: &str) -> Result<()> {
    sqlx::query("UPDATE sources SET indexing_status = $1 WHERE id = $2")
        .bind(status)
        .bind(id)
        .execute(pool)
        .await?;
    Ok(())
}

pub async fn update_error(pool: &Db, id: &str, reason: &str) -> Result<()> {
    sqlx::query("UPDATE sources SET indexing_status = 'failed', error_reason = $1 WHERE id = $2")
        .bind(reason)
        .bind(id)
        .execute(pool)
        .await?;
    Ok(())
}

pub async fn set_indexed_at(
    pool: &Db,
    id: &str,
    extraction_method: &str,
    quality_score: f32,
) -> Result<()> {
    let now = Utc::now().timestamp_millis();
    sqlx::query(
        "UPDATE sources SET indexing_status = 'indexed', indexed_at = $1, \
         extraction_method = $2, quality_score = $3 WHERE id = $4",
    )
    .bind(now)
    .bind(extraction_method)
    .bind(quality_score)
    .bind(id)
    .execute(pool)
    .await?;
    Ok(())
}

/// Remove source from a notebook. Deletes the source row only if it has no other refs.
pub async fn detach_from_notebook(
    pool: &Db,
    notebook_id: &str,
    source_id: &str,
) -> Result<()> {
    sqlx::query(
        "DELETE FROM notebook_sources WHERE notebook_id = $1 AND source_id = $2",
    )
    .bind(notebook_id)
    .bind(source_id)
    .execute(pool)
    .await?;

    // Cascade-delete source if it is now orphaned.
    let ref_count: (i64,) = sqlx::query_as(
        "SELECT COUNT(*) FROM notebook_sources WHERE source_id = $1",
    )
    .bind(source_id)
    .fetch_one(pool)
    .await?;

    if ref_count.0 == 0 {
        sqlx::query("DELETE FROM sources WHERE id = $1")
            .bind(source_id)
            .execute(pool)
            .await?;
    }

    Ok(())
}

/// On app startup, reset any source that is stuck in 'pending'.
/// These were interrupted mid-indexing when the app was previously killed.
/// Marking them 'failed' allows the user to retry via the retry button.
pub async fn reset_stuck_pending(pool: &Db) -> Result<()> {
    let count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM sources WHERE indexing_status = 'pending'",
    )
    .fetch_one(pool)
    .await
    .unwrap_or(0);

    if count > 0 {
        sqlx::query(
            "UPDATE sources SET indexing_status = 'failed',
             error_reason = 'Indexing interrupted — app was closed mid-scan. Click retry to re-index.'
             WHERE indexing_status = 'pending'",
        )
        .execute(pool)
        .await?;
        log::info!("Reset {count} stuck-pending sources to failed on startup");
    }

    Ok(())
}

#[cfg(test)]
mod scope_tests {
    use super::*;

    /// Seed: shared corpus doc, A's private doc, B's private doc, A's
    /// attachment in conversation c1. All 'indexed'.
    async fn seeded_pool() -> Db {
        let pool = crate::db::test_pool().await;
        for (id, owner, vis, conv) in [
            ("s_shared", None::<&str>, "shared", None::<&str>),
            ("s_a_priv", Some("userA"), "private", None),
            ("s_b_priv", Some("userB"), "private", None),
            ("s_a_att", Some("userA"), "private", Some("c1")),
        ] {
            sqlx::query(
                "INSERT INTO sources(id,file_path,file_name,format,file_size_bytes,file_hash,\
                 last_modified,created_at,indexing_status,owner_id,visibility,conversation_id) \
                 VALUES($1,$2,$3,'pdf',10,'h',0,0,'indexed',$4,$5,$6)",
            )
            .bind(id)
            .bind(format!("/{id}"))
            .bind(format!("{id}.pdf"))
            .bind(owner)
            .bind(vis)
            .bind(conv)
            .execute(&pool)
            .await
            .unwrap();
        }
        // ⚠️ Migration 022 moved the attachment branch from `sources.conversation_id`
        // to `source_link`. This fixture seeded only the column, so
        // `conversation_scope_includes_own_attachment` failed for a STALE-FIXTURE
        // reason and was carried as a known failure — leaving the attachment
        // branch's owner predicate, which that test is the ONLY coverage of,
        // effectively untested. Seed the link the way an upload does.
        sqlx::query(
            "INSERT INTO source_link(source_id,conversation_id,owner_id,linked_at) \
             VALUES('s_a_att','c1','userA',0)",
        )
        .execute(&pool)
        .await
        .unwrap();
        pool
    }

    fn scope(user: &str, conv: Option<&str>) -> RequestScope {
        RequestScope {
            trial: false,
            user_id: Some(user.to_string()),
            conversation_id: conv.map(str::to_string),
            project_id: None,
            include_corpus: false,
        }
    }

    /// F027 — exclusive scope. `include_corpus` false unless stated.
    fn project_scope(user: &str, project: &str, include_corpus: bool) -> RequestScope {
        RequestScope {
            trial: false,
            user_id: Some(user.to_string()),
            conversation_id: None,
            project_id: Some(project.to_string()),
            include_corpus,
        }
    }

    /// Extends [`seeded_pool`] with two projects:
    ///   `p_a` (userA's) holding `s_a_priv`
    ///   `p_other` (userA's second project) holding `s_a_att`
    /// plus `s_b_proj`, userB's own document, in userB's project `p_b`.
    ///
    /// The point of the fixture is that every category a project question must
    /// NOT reach is present and non-empty — an isolation test against an empty
    /// database proves nothing.
    async fn seeded_pool_with_projects() -> Db {
        let pool = seeded_pool().await;
        sqlx::query(
            "INSERT INTO sources(id,file_path,file_name,format,file_size_bytes,file_hash,\
             last_modified,created_at,indexing_status,owner_id,visibility,conversation_id) \
             VALUES('s_b_proj','/s_b_proj','s_b_proj.pdf','pdf',10,'h',0,0,'indexed',\
             'userB','private',NULL)",
        )
        .execute(&pool)
        .await
        .unwrap();
        for (id, owner) in [("p_a", "userA"), ("p_other", "userA"), ("p_b", "userB")] {
            sqlx::query(
                "INSERT INTO project(id,owner_id,name,created_at,updated_at) \
                 VALUES($1,$2,$1,0,0)",
            )
            .bind(id)
            .bind(owner)
            .execute(&pool)
            .await
            .unwrap();
        }
        for (project, source) in
            [("p_a", "s_a_priv"), ("p_other", "s_a_att"), ("p_b", "s_b_proj")]
        {
            sqlx::query(
                "INSERT INTO project_source(project_id,source_id,added_at) VALUES($1,$2,0)",
            )
            .bind(project)
            .bind(source)
            .execute(&pool)
            .await
            .unwrap();
        }
        pool
    }

    /// ⭐ **The isolation invariant** (SC-001, FR-010). This is the test the
    /// whole feature rests on: every excluded category is asserted separately,
    /// so a failure names which one leaked rather than only that something did.
    #[tokio::test]
    async fn project_scope_excludes_everything_but_the_project() {
        let pool = seeded_pool_with_projects().await;
        let ids = visible_source_ids(&pool, &project_scope("userA", "p_a", false))
            .await
            .unwrap();

        assert!(ids.contains(&"s_a_priv".to_string()), "the project's own document is missing");
        assert!(!ids.contains(&"s_shared".to_string()), "SHARED CORPUS leaked into a project");
        assert!(!ids.contains(&"s_a_att".to_string()), "ANOTHER PROJECT of the same user leaked");
        assert!(!ids.contains(&"s_b_proj".to_string()), "ANOTHER USER's document leaked");
        assert!(!ids.contains(&"s_b_priv".to_string()), "ANOTHER USER's private document leaked");
        assert_eq!(ids.len(), 1, "exclusive scope returned more than the project's documents");
    }

    /// F027 — `include_corpus` widens to the shared corpus and to nothing else.
    #[tokio::test]
    async fn project_scope_with_corpus_adds_only_the_corpus() {
        let pool = seeded_pool_with_projects().await;
        let ids = visible_source_ids(&pool, &project_scope("userA", "p_a", true))
            .await
            .unwrap();

        assert!(ids.contains(&"s_a_priv".to_string()), "the project's own document is missing");
        assert!(ids.contains(&"s_shared".to_string()), "corpus opt-in did not include the corpus");
        assert!(!ids.contains(&"s_a_att".to_string()), "another project leaked via the corpus tick");
        assert!(!ids.contains(&"s_b_proj".to_string()), "another user leaked via the corpus tick");
        assert_eq!(ids.len(), 2, "corpus opt-in widened further than the shared corpus");
    }

    /// F027 — membership, not ownership, is the authority inside a project
    /// (ADR-0028). A copied project holds the SHARER's `sources` rows, so if
    /// exclusive mode consulted `owner_id` the recipient would see nothing.
    /// Here userB scopes to a project holding userA's document.
    #[tokio::test]
    async fn project_scope_ignores_source_owner() {
        let pool = seeded_pool_with_projects().await;
        // Simulate a copy: userB's project gains userA's document.
        sqlx::query(
            "INSERT INTO project_source(project_id,source_id,added_at) VALUES('p_b','s_a_priv',0)",
        )
        .execute(&pool)
        .await
        .unwrap();

        let ids = visible_source_ids(&pool, &project_scope("userB", "p_b", false))
            .await
            .unwrap();
        assert!(
            ids.contains(&"s_a_priv".to_string()),
            "a copied project must reach the sharer's document — exclusive mode must NOT \
             filter on owner_id (ADR-0028)"
        );
        assert!(ids.contains(&"s_b_proj".to_string()));
        assert_eq!(ids.len(), 2);
    }

    /// F027 — a project holding nothing returns nothing. It must never fall
    /// through to the corpus (FR-012); the caller turns this into "nothing to
    /// search" rather than an answer.
    #[tokio::test]
    async fn empty_project_returns_nothing() {
        let pool = seeded_pool_with_projects().await;
        sqlx::query("INSERT INTO project(id,owner_id,name,created_at,updated_at) \
                     VALUES('p_empty','userA','p_empty',0,0)")
            .execute(&pool)
            .await
            .unwrap();
        let ids = visible_source_ids(&pool, &project_scope("userA", "p_empty", false))
            .await
            .unwrap();
        assert!(ids.is_empty(), "an empty project fell through to another source set");
    }

    /// F027 — a disabled document stays out of project answers too, exactly as
    /// it does on the additive path (migration 021 semantics preserved).
    #[tokio::test]
    async fn project_scope_still_excludes_disabled_documents() {
        let pool = seeded_pool_with_projects().await;
        sqlx::query("UPDATE sources SET disabled_at = 1 WHERE id = 's_a_priv'")
            .execute(&pool)
            .await
            .unwrap();
        let ids = visible_source_ids(&pool, &project_scope("userA", "p_a", false))
            .await
            .unwrap();
        assert!(ids.is_empty(), "an admin-disabled document leaked into a project");
    }

    /// F027 — `is_referenced` must see EACH reference type independently, so no
    /// single one can be forgotten in a delete path. The `project_source` case
    /// is the new and dangerous one: it can belong to another user.
    #[tokio::test]
    async fn is_referenced_sees_each_reference_type() {
        let pool = seeded_pool_with_projects().await;

        // project_source only (s_a_priv is in p_a and nothing else).
        assert!(is_referenced(&pool, "s_a_priv").await.unwrap(), "project membership missed");

        // No reference at all.
        assert!(
            !is_referenced(&pool, "s_b_priv").await.unwrap(),
            "reported a reference for a source held by nothing"
        );

        // source_link only.
        sqlx::query(
            "INSERT INTO source_link(source_id,conversation_id,owner_id,linked_at) \
             VALUES('s_b_priv','c1','userB',0)",
        )
        .execute(&pool)
        .await
        .unwrap();
        assert!(is_referenced(&pool, "s_b_priv").await.unwrap(), "source_link missed");
    }

    /// B1 (2026-08-07) — THE REGRESSION.
    ///
    /// `delete_conversation` asks two different questions about the same source:
    /// `is_orphaned` counts only `source_link`, while `hard_delete_source` guards
    /// on `is_referenced`, which also counts `notebook_sources`. The `__conv__`
    /// stub notebook used to be dropped AFTER the delete loop, so an attachment
    /// looked orphaned to the loop and referenced to the delete — retained, then
    /// stranded as a fully-indexed orphan while the API reported success.
    ///
    /// This pins the disagreement: with the stub present the two predicates must
    /// contradict each other, and dropping the stub first must resolve it.
    #[tokio::test]
    async fn conversation_stub_notebook_makes_orphaned_and_referenced_disagree() {
        let pool = seeded_pool_with_projects().await;

        // s_b_priv is held by nothing; attach it to a chat the way an upload does.
        sqlx::query(
            "INSERT INTO source_link(source_id,conversation_id,owner_id,linked_at) \
             VALUES('s_b_priv','c_del','userB',0)",
        )
        .execute(&pool).await.unwrap();
        // Mirrors the real stub insert in `api/uploads.rs` (id, name, created_at,
        // updated_at) — `notebooks.updated_at` is NOT NULL.
        sqlx::query(
            "INSERT INTO notebooks(id,name,created_at,updated_at) \
             VALUES('__conv__c_del','chat',0,0)",
        )
        .execute(&pool).await.unwrap();
        sqlx::query(
            "INSERT INTO notebook_sources(notebook_id,source_id,added_at) \
             VALUES('__conv__c_del','s_b_priv',0)",
        )
        .execute(&pool).await.unwrap();

        // The user deletes the chat: its links go first.
        let unlinked = unlink_conversation(&pool, "c_del", "userB").await.unwrap();
        assert_eq!(unlinked, vec!["s_b_priv".to_string()]);

        // ⚠️ The bug, reproduced: orphaned says yes, referenced says yes too.
        assert!(is_orphaned(&pool, "s_b_priv").await.unwrap(), "no links remain");
        assert!(
            is_referenced(&pool, "s_b_priv").await.unwrap(),
            "the stub notebook must still make this look referenced — if this \
             assertion fails the stub is no longer the confounder and B1's fix \
             needs re-deriving, not deleting",
        );

        // The fix: drop the stub FIRST. `notebook_sources.notebook_id` is
        // ON DELETE CASCADE, so its rows go with it and the two agree.
        sqlx::query("DELETE FROM notebooks WHERE id = '__conv__c_del'")
            .execute(&pool).await.unwrap();
        assert!(
            !is_referenced(&pool, "s_b_priv").await.unwrap(),
            "after the stub is gone the content must be deletable — otherwise a \
             deleted attachment stays indexed and searchable",
        );
        assert!(is_orphaned(&pool, "s_b_priv").await.unwrap());
    }

    #[tokio::test]
    async fn unrestricted_returns_everything() {
        let pool = seeded_pool().await;
        let ids = visible_source_ids(&pool, &RequestScope::unrestricted()).await.unwrap();
        assert_eq!(ids.len(), 4, "desktop scope must match pre-021 behaviour");
    }

    #[tokio::test]
    async fn user_scope_excludes_foreign_private_and_all_attachments() {
        let pool = seeded_pool().await;
        let ids = visible_source_ids(&pool, &scope("userB", Some("c9"))).await.unwrap();
        assert!(ids.contains(&"s_shared".to_string()));
        assert!(ids.contains(&"s_b_priv".to_string()));
        assert!(!ids.contains(&"s_a_priv".to_string()), "A's private doc leaked to B");
        assert!(!ids.contains(&"s_a_att".to_string()), "foreign attachment leaked");
    }

    #[tokio::test]
    async fn conversation_scope_includes_own_attachment() {
        let pool = seeded_pool().await;
        let ids = visible_source_ids(&pool, &scope("userA", Some("c1"))).await.unwrap();
        assert!(ids.contains(&"s_shared".to_string()));
        assert!(ids.contains(&"s_a_priv".to_string()));
        assert!(ids.contains(&"s_a_att".to_string()), "own conversation attachment missing");
        assert!(!ids.contains(&"s_b_priv".to_string()));
    }

    #[tokio::test]
    async fn shared_flip_becomes_visible_without_reindex() {
        let pool = seeded_pool().await;
        set_visibility_source(&pool, "s_a_priv", "shared").await.unwrap();
        let ids = visible_source_ids(&pool, &scope("userB", None)).await.unwrap();
        assert!(ids.contains(&"s_a_priv".to_string()), "share flip not effective");
    }

    #[tokio::test]
    async fn attachment_insert_is_forced_private() {
        let pool = crate::db::test_pool().await;
        let s = insert_scoped(
            &pool, "/att", "att.pdf", "pdf", 5, "h2", 0,
            &SourceScope {
                owner_id: Some("userA".into()),
                visibility: Some("shared".into()), // hostile input — must be overridden
                conversation_id: Some("c1".into()),
            },
        )
        .await
        .unwrap();
        assert_eq!(s.visibility, "private", "invariant I1: attachment must be private");
    }

    #[tokio::test]
    async fn usage_counts_documents_and_attachments() {
        let pool = seeded_pool().await;
        let (bytes, files) = usage_for_owner(&pool, "userA").await.unwrap();
        assert_eq!(files, 2, "private doc + attachment both count (FR-019)");
        assert_eq!(bytes, 20);
    }
}

/// On startup, reset sources stuck in 'processing' to 'failed' so the user
/// can retry. 'processing' at startup means the app crashed mid-index —
/// we cannot safely auto-re-trigger because the notebook link may be orphaned.
pub async fn reset_stuck_processing(pool: &Db) -> Result<()> {
    let count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM sources WHERE indexing_status = 'processing'",
    )
    .fetch_one(pool)
    .await
    .unwrap_or(0);

    if count > 0 {
        sqlx::query(
            "UPDATE sources SET indexing_status = 'failed', \
             error_reason = 'Indexing interrupted — app was closed mid-index. Click retry to re-index.' \
             WHERE indexing_status = 'processing'",
        )
        .execute(pool)
        .await?;
        log::info!("Reset {count} stuck-processing source(s) to failed on startup");
    }

    Ok(())
}

/// Display names (falling back to `file_name`) for the given source ids, in the
/// order the ids were supplied. Used by web search to build a query from the
/// ATTACHED document's title rather than from the officer's sentence — see
/// `websearch::build_query`.
pub async fn display_names_for(pool: &Db, ids: &[String]) -> anyhow::Result<Vec<String>> {
    if ids.is_empty() {
        return Ok(Vec::new());
    }
    let rows: Vec<(String, Option<String>, String)> = sqlx::query_as(
        "SELECT id, display_name, file_name FROM sources WHERE id = ANY($1)",
    )
    .bind(ids)
    .fetch_all(pool)
    .await?;
    Ok(ids
        .iter()
        .filter_map(|id| rows.iter().find(|(rid, _, _)| rid == id))
        .map(|(_, disp, file)| {
            disp.as_deref()
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .unwrap_or(file.as_str())
                .to_string()
        })
        .collect())
}

/// First `limit` characters of a source's text, in document order. Used by web
/// search to read the instruments an ATTACHED document cites, which make a far
/// better query than its filename — see `websearch::instruments_in`.
pub async fn head_text_for(pool: &Db, ids: &[String], limit: usize) -> anyhow::Result<String> {
    if ids.is_empty() {
        return Ok(String::new());
    }
    let rows: Vec<(String,)> = sqlx::query_as(
        "SELECT content FROM chunks WHERE source_id = ANY($1) ORDER BY chunk_index LIMIT 40",
    )
    .bind(ids)
    .fetch_all(pool)
    .await?;
    let mut out = String::new();
    for (c,) in rows {
        if out.len() >= limit {
            break;
        }
        out.push_str(&c);
        out.push('\n');
    }
    out.truncate(out.char_indices().nth(limit).map(|(i, _)| i).unwrap_or(out.len()));
    Ok(out)
}
