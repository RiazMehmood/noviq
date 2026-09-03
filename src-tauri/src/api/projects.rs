//! Feature 027 — project workspace routes.
//!
//! Every handler in this module MUST call [`require_projects_enabled`] as its
//! **first act**, before any argument is examined, and every handler that takes
//! a client-supplied `project_id` MUST then call
//! `repositories::projects::owned_project`. Those two calls are the feature's
//! whole authorisation story; skipping either fails open.
//!
//! Routes land per user story (contracts/api.md).

use crate::api::auth::AuthUser;
use crate::api::{ApiError, ApiResult, AppState};
use crate::db::repositories::chat_shares as share_repo;
use crate::db::repositories::projects as project_repo;
use crate::db::repositories::sources::{self as source_repo, RequestScope};
use crate::services::projects as project_svc;
use axum::extract::State;
use axum::http::StatusCode;
use axum::Json;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

fn err<E: std::fmt::Display>(e: E) -> ApiError {
    ApiError::internal(e.to_string())
}

/// Project name bounds (FR-001). Rejected rather than silently truncated: a
/// renamed project the user cannot recognise is worse than a refusal.
const MAX_NAME_LEN: usize = 200;

fn validate_name(name: &str) -> Result<String, ApiError> {
    let trimmed = name.trim();
    if trimmed.is_empty() {
        return Err(ApiError::new(
            StatusCode::BAD_REQUEST,
            "invalid_name",
            "A project needs a name.",
        ));
    }
    if trimmed.chars().count() > MAX_NAME_LEN {
        return Err(ApiError::new(
            StatusCode::BAD_REQUEST,
            "invalid_name",
            format!("Project names are limited to {MAX_NAME_LEN} characters."),
        ));
    }
    Ok(trimmed.to_string())
}

/// FR-038 — the deployment-wide availability gate, enforced at the server.
///
/// Returns the same shape whether the feature is off or the caller is simply
/// not permitted, and is checked before arguments are parsed so a request for a
/// project that does not exist and a request while the feature is off are
/// indistinguishable.
///
/// ⚠️ Hiding the nav entry is presentation. This is the enforcement.
pub async fn require_projects_enabled(st: &AppState) -> Result<(), ApiError> {
    let enabled = project_svc::my_projects_enabled(&st.pool)
        .await
        .map_err(|e| ApiError::internal(e.to_string()))?;
    if enabled {
        Ok(())
    } else {
        Err(ApiError::new(
            StatusCode::NOT_FOUND,
            "not_found",
            "My Projects is not available on this deployment",
        ))
    }
}

/// The single not-found response for "no such project" **and** "not yours".
///
/// Whether a project the caller cannot see EXISTS is itself information, so the
/// two cases must be indistinguishable — the rule `api/documents.rs` already
/// follows for sources.
pub fn project_not_found() -> ApiError {
    ApiError::new(StatusCode::NOT_FOUND, "not_found", "project not found")
}

/// Query string for the GET routes that cannot carry a JSON body
/// (`/sources/:id/file`).
#[derive(Debug, Default, Deserialize)]
pub struct ProjectQuery {
    pub project_id: Option<String>,
}

/// **The one place a client-supplied `project_id` becomes a retrieval scope.**
///
/// Four routes — `global_search`, `source_chunks`, `source_file` and
/// `get_knowledge_graph` — take the project from the request because, unlike a
/// chat turn, they have no conversation from which the server could derive it.
/// That makes them the only place FR-013 is enforced by a check rather than by
/// construction, so the check lives here once and every one of them calls it.
///
/// ⚠️ Scoping to a project the caller owns can only NARROW what is retrievable.
/// Scoping to one they do NOT own would WIDEN into another user's documents,
/// turning citation opening and global search into a way to read any project in
/// the database. `owned_project` returning `None` is therefore a hard refusal,
/// not a fallback to the unscoped path.
///
/// `project_id: None` yields exactly today's scope, so every pre-027 caller and
/// every request that names no project is byte-identical to current behaviour
/// (FR-040).
pub async fn scope_for(
    st: &AppState,
    user_id: &str,
    project_id: Option<&str>,
    include_corpus: bool,
) -> Result<RequestScope, ApiError> {
    let Some(project_id) = project_id else {
        return Ok(RequestScope {
            user_id: Some(user_id.to_string()),
            conversation_id: None,
            project_id: None,
            include_corpus: false,
            // Projects are not offered to trial users at all (the API refuses
            // before reaching here), so this can only ever be a paying user.
            trial: false,
        });
    };

    // Gate first, before the project is looked at: with the feature off, a
    // request naming a project must be indistinguishable from one naming a
    // project that does not exist.
    require_projects_enabled(st).await?;

    project_repo::owned_project(&st.pool, user_id, project_id)
        .await
        .map_err(|e| ApiError::internal(e.to_string()))?
        .ok_or_else(project_not_found)?;

    Ok(RequestScope {
        user_id: Some(user_id.to_string()),
        conversation_id: None,
        project_id: Some(project_id.to_string()),
        include_corpus,
        trial: false,
    })
}

// ════════════════════════════════════════════════════════════════════════════
// US1 — projects and their documents
// ════════════════════════════════════════════════════════════════════════════

#[derive(Serialize)]
pub struct ProjectOut {
    pub id: String,
    pub name: String,
    pub document_count: i64,
    pub chat_count: i64,
    pub created_at: i64,
    pub updated_at: i64,
    /// Received from another user by copy (ADR-0028).
    pub is_copy: bool,
}

async fn to_out(st: &AppState, p: project_repo::Project) -> Result<ProjectOut, ApiError> {
    let document_count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM project_source WHERE project_id = $1")
            .bind(&p.id)
            .fetch_one(&st.pool)
            .await
            .map_err(err)?;
    let chat_count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM project_chat WHERE project_id = $1")
            .bind(&p.id)
            .fetch_one(&st.pool)
            .await
            .map_err(err)?;
    Ok(ProjectOut {
        is_copy: p.copied_from.is_some(),
        id: p.id,
        name: p.name,
        document_count,
        chat_count,
        created_at: p.created_at,
        updated_at: p.updated_at,
    })
}

#[derive(Deserialize)]
pub struct ProjectIdReq {
    pub project_id: String,
}

#[derive(Deserialize)]
pub struct CreateProjectReq {
    pub name: String,
}

pub async fn list_projects(
    State(st): State<AppState>,
    user: AuthUser,
) -> ApiResult<Json<Vec<ProjectOut>>> {
    require_projects_enabled(&st).await?;
    let rows = project_repo::list_for_owner(&st.pool, &user.id).await.map_err(err)?;
    let mut out = Vec::with_capacity(rows.len());
    for p in rows {
        out.push(to_out(&st, p).await?);
    }
    Ok(Json(out))
}

pub async fn create_project(
    State(st): State<AppState>,
    user: AuthUser,
    Json(req): Json<CreateProjectReq>,
) -> ApiResult<Json<ProjectOut>> {
    require_projects_enabled(&st).await?;
    let name = validate_name(&req.name)?;
    let p = project_repo::create(&st.pool, &user.id, &name).await.map_err(err)?;
    Ok(Json(to_out(&st, p).await?))
}

#[derive(Deserialize)]
pub struct RenameProjectReq {
    pub project_id: String,
    pub name: String,
}

pub async fn rename_project(
    State(st): State<AppState>,
    user: AuthUser,
    Json(req): Json<RenameProjectReq>,
) -> ApiResult<Json<ProjectOut>> {
    require_projects_enabled(&st).await?;
    let name = validate_name(&req.name)?;
    project_repo::owned_project(&st.pool, &user.id, &req.project_id)
        .await
        .map_err(err)?
        .ok_or_else(project_not_found)?;
    project_repo::rename(&st.pool, &req.project_id, &name).await.map_err(err)?;
    let p = project_repo::owned_project(&st.pool, &user.id, &req.project_id)
        .await
        .map_err(err)?
        .ok_or_else(project_not_found)?;
    Ok(Json(to_out(&st, p).await?))
}

/// Deletes the project, its memberships and its chats.
///
/// Documents are NOT deleted outright: each is offered to `hard_delete_source`,
/// which retains any still referenced by another project — possibly ANOTHER
/// USER's copy (FR-007, FR-024).
pub async fn delete_project(
    State(st): State<AppState>,
    user: AuthUser,
    Json(req): Json<ProjectIdReq>,
) -> ApiResult<Json<Value>> {
    require_projects_enabled(&st).await?;
    project_repo::owned_project(&st.pool, &user.id, &req.project_id)
        .await
        .map_err(err)?
        .ok_or_else(project_not_found)?;

    let chats_deleted: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM project_chat WHERE project_id = $1")
            .bind(&req.project_id)
            .fetch_one(&st.pool)
            .await
            .map_err(err)?;
    let members: Vec<String> =
        sqlx::query_scalar("SELECT source_id FROM project_source WHERE project_id = $1")
            .bind(&req.project_id)
            .fetch_all(&st.pool)
            .await
            .map_err(err)?;

    // Drop the project FIRST, so its own memberships are gone before we ask
    // whether anything still references each document. Asking the other way
    // round would always answer "yes" — this project.
    project_repo::delete(&st.pool, &req.project_id).await.map_err(err)?;

    let mut documents_deleted = 0u32;
    for source_id in &members {
        // Only ever reclaim documents this user OWNS. A copied document belongs
        // to the sharer; `hard_delete_source` would retain it anyway on the
        // reference check, but skipping it here keeps the intent explicit.
        if source_repo::owner_of(&st.pool, source_id).await.map_err(err)?.as_deref()
            != Some(user.id.as_str())
        {
            continue;
        }
        // B1 — this counted only documents that HAD a file path, so a deleted
        // source with no file on disk went unreported. Count the outcome.
        let outcome = super::documents::hard_delete_source(&st, source_id).await?;
        if let Some(path) = outcome.file_path() {
            let _ = std::fs::remove_file(path);
        }
        if outcome.deleted() { documents_deleted += 1; }
    }

    Ok(Json(json!({
        "deleted": true,
        "chats_deleted": chats_deleted,
        "documents_deleted": documents_deleted,
    })))
}

#[derive(Serialize)]
pub struct ProjectDocumentOut {
    pub source_id: String,
    pub file_name: String,
    pub display_name: Option<String>,
    /// `pending` | `ready` | `failed` — FR-008. A document that is not yet
    /// searchable must LOOK pending, never simply be missing from the list.
    pub status: String,
    pub added_at: i64,
    /// Arrived by copy rather than by this user's own upload.
    pub added_by_copy: bool,
}

pub async fn get_project(
    State(st): State<AppState>,
    user: AuthUser,
    Json(req): Json<ProjectIdReq>,
) -> ApiResult<Json<Value>> {
    require_projects_enabled(&st).await?;
    let p = project_repo::owned_project(&st.pool, &user.id, &req.project_id)
        .await
        .map_err(err)?
        .ok_or_else(project_not_found)?;

    let docs = project_repo::documents(&st.pool, &req.project_id).await.map_err(err)?;
    let mut out = Vec::with_capacity(docs.len());
    for d in docs {
        let owner = source_repo::owner_of(&st.pool, &d.source_id).await.map_err(err)?;
        out.push(ProjectDocumentOut {
            status: match d.indexing_status.as_str() {
                "indexed" => "ready",
                "failed" => "failed",
                _ => "pending",
            }
            .to_string(),
            added_by_copy: owner.as_deref() != Some(user.id.as_str()),
            source_id: d.source_id,
            file_name: d.file_name,
            display_name: d.display_name,
            added_at: d.added_at,
        });
    }

    let mut body = serde_json::to_value(to_out(&st, p).await?).map_err(err)?;
    body["documents"] = serde_json::to_value(out).map_err(err)?;
    Ok(Json(body))
}

#[derive(Deserialize)]
pub struct AddDocumentsReq {
    pub project_id: String,
    pub source_ids: Vec<String>,
}

pub async fn add_project_documents(
    State(st): State<AppState>,
    user: AuthUser,
    Json(req): Json<AddDocumentsReq>,
) -> ApiResult<Json<Value>> {
    require_projects_enabled(&st).await?;
    project_repo::owned_project(&st.pool, &user.id, &req.project_id)
        .await
        .map_err(err)?
        .ok_or_else(project_not_found)?;

    // A user may only add documents they can ALREADY see. Without this the route
    // would be a way to pull any source id in the database into a project and
    // then read it — membership is precisely what exclusive-mode scoping trusts,
    // so an unchecked insert here would defeat the whole isolation model.
    let visible = source_repo::visible_source_ids(
        &st.pool,
        &RequestScope {
            user_id: Some(user.id.clone()),
            conversation_id: None,
            project_id: None,
            include_corpus: false,
            trial: false,
        },
    )
    .await
    .map_err(err)?;
    let own: Vec<String> = req
        .source_ids
        .iter()
        .filter(|id| visible.contains(id))
        .cloned()
        .collect();
    let requested = req.source_ids.len() as u64;
    let eligible = own.len() as u64;

    let added = project_repo::add_sources(&st.pool, &req.project_id, &own).await.map_err(err)?;
    Ok(Json(json!({
        "added": added,
        "already_present": eligible.saturating_sub(added),
        "refused": requested.saturating_sub(eligible),
    })))
}

#[derive(Deserialize)]
pub struct RemoveDocumentReq {
    pub project_id: String,
    pub source_id: String,
}

/// Removes ONE membership, then offers the document for deletion.
///
/// `content_retained` reports what actually happened rather than assuming:
/// another project — possibly another user's copy — may still hold it, in which
/// case the content stays and keeps answering there (FR-007, FR-024).
pub async fn remove_project_document(
    State(st): State<AppState>,
    user: AuthUser,
    Json(req): Json<RemoveDocumentReq>,
) -> ApiResult<Json<Value>> {
    require_projects_enabled(&st).await?;
    project_repo::owned_project(&st.pool, &user.id, &req.project_id)
        .await
        .map_err(err)?
        .ok_or_else(project_not_found)?;

    project_repo::remove_source(&st.pool, &req.project_id, &req.source_id)
        .await
        .map_err(err)?;

    // Never reclaim someone else's document out of our own project.
    let is_own = source_repo::owner_of(&st.pool, &req.source_id).await.map_err(err)?.as_deref()
        == Some(user.id.as_str());
    let mut content_retained = true;
    if is_own {
        // B1 — keyed on the file path, so a source deleted with no file on disk
        // reported `content_retained: true` while its rows were in fact gone.
        let outcome = super::documents::hard_delete_source(&st, &req.source_id).await?;
        if let Some(path) = outcome.file_path() {
            let _ = std::fs::remove_file(path);
        }
        content_retained = !outcome.deleted();
    }

    Ok(Json(json!({ "removed": true, "content_retained": content_retained })))
}

// ════════════════════════════════════════════════════════════════════════════
// US2 — chats inside a project
// ════════════════════════════════════════════════════════════════════════════

#[derive(Serialize)]
pub struct ProjectChatOut {
    pub conversation_id: String,
    pub title: String,
    pub updated_at: i64,
    pub is_shared: bool,
}

pub async fn list_project_chats(
    State(st): State<AppState>,
    user: AuthUser,
    Json(req): Json<ProjectIdReq>,
) -> ApiResult<Json<Vec<ProjectChatOut>>> {
    require_projects_enabled(&st).await?;
    project_repo::owned_project(&st.pool, &user.id, &req.project_id)
        .await
        .map_err(err)?
        .ok_or_else(project_not_found)?;

    let chats = project_repo::chats_for_project(&st.pool, &req.project_id).await.map_err(err)?;
    let mut out = Vec::with_capacity(chats.len());
    for c in chats {
        // Title and recency come from `query_log`: that is where chat history
        // actually lives (migration 020), and `project_chat` deliberately holds
        // only the mapping.
        let title: Option<String> = sqlx::query_scalar(
            "SELECT question FROM query_log WHERE conversation_id = $1 AND user_id = $2 \
             ORDER BY created_at ASC LIMIT 1",
        )
        .bind(&c.conversation_id)
        .bind(&user.id)
        .fetch_optional(&st.pool)
        .await
        .map_err(err)?;
        let updated_at: Option<i64> =
            sqlx::query_scalar("SELECT MAX(created_at) FROM query_log WHERE conversation_id = $1")
                .bind(&c.conversation_id)
                .fetch_one(&st.pool)
                .await
                .map_err(err)?;
        out.push(ProjectChatOut {
            title: title.unwrap_or_else(|| "New chat".to_string()),
            updated_at: updated_at.unwrap_or(c.created_at),
            is_shared: share_repo::is_shared(&st.pool, &c.conversation_id).await.map_err(err)?,
            conversation_id: c.conversation_id,
        });
    }
    Ok(Json(out))
}

/// The ONLY place a chat becomes a project chat.
///
/// Written server-side, which is what makes the chat turn path unwidenable: the
/// client thereafter sends only `conversation_id`, and the server derives the
/// project from it (ADR-0029).
pub async fn start_project_chat(
    State(st): State<AppState>,
    user: AuthUser,
    Json(req): Json<ProjectIdReq>,
) -> ApiResult<Json<Value>> {
    require_projects_enabled(&st).await?;
    project_repo::owned_project(&st.pool, &user.id, &req.project_id)
        .await
        .map_err(err)?
        .ok_or_else(project_not_found)?;

    let conversation_id = uuid::Uuid::new_v4().to_string();
    project_repo::create_chat(&st.pool, &conversation_id, &req.project_id, &user.id)
        .await
        .map_err(err)?;
    Ok(Json(json!({ "conversation_id": conversation_id })))
}

/// Delete one chat from a project (FR-033 companion to `start_project_chat`).
///
/// ⚠️ `project_chat.conversation_id` is a bare TEXT primary key — there is no
/// conversation table for it to reference, so it cascades only from `project`.
/// Nothing else removes this row: `delete_conversation` cleans attachments and
/// the `__conv__` stub but has never touched `project_chat`. Without this the
/// chat stays in the project's list forever, pointing at a conversation whose
/// content is gone.
///
/// The transcript itself is hidden by the caller's existing `hide_conversation`
/// (migration 020, a SOFT delete — `query_log` keeps the audit trail), and the
/// attachments by `delete_conversation`. This route owns exactly one thing: the
/// project membership.
pub async fn delete_project_chat(
    State(st): State<AppState>,
    user: AuthUser,
    Json(req): Json<ConversationIdReq>,
) -> ApiResult<Json<Value>> {
    require_projects_enabled(&st).await?;
    // Ownership is checked on the CHAT, not on a project id the client supplies:
    // the membership row already carries `owner_id`, so this cannot be widened by
    // naming someone else's project (ADR-0029).
    let chat = project_repo::chat_for_conversation(&st.pool, &req.conversation_id)
        .await
        .map_err(err)?;
    let Some(chat) = chat else {
        // Not a project chat (or already gone) — nothing to unlink. Report success
        // so a double-delete is not an error the officer has to think about.
        return Ok(Json(json!({ "removed": false })));
    };
    if chat.owner_id != user.id {
        return Err(project_not_found());
    }
    let removed = project_repo::delete_chat(&st.pool, &req.conversation_id)
        .await
        .map_err(err)?;
    Ok(Json(json!({ "removed": removed })))
}

/// Does this conversation belong to a project of the caller's?
///
/// Presentation support only: it tells the composer whether to show the "also
/// search the law" tick. The server derives the real project from the
/// conversation id on every turn and never consults this, so a client that lies
/// about the answer changes nothing about what is retrieved (ADR-0029).
pub async fn is_project_chat(
    State(st): State<AppState>,
    user: AuthUser,
    Json(req): Json<ConversationIdReq>,
) -> ApiResult<Json<Value>> {
    require_projects_enabled(&st).await?;
    let chat = project_repo::chat_for_conversation(&st.pool, &req.conversation_id)
        .await
        .map_err(err)?
        .filter(|c| c.owner_id == user.id);
    Ok(Json(json!({
        "in_project": chat.is_some(),
        "project_id": chat.map(|c| c.project_id),
    })))
}

// ════════════════════════════════════════════════════════════════════════════
// US5 — sharing a project (COPY, not access)
// ════════════════════════════════════════════════════════════════════════════

#[derive(Deserialize)]
pub struct ShareProjectReq {
    pub project_id: String,
    /// Username of the recipient on this deployment. Sharing never leaves Noviq.
    pub recipient: String,
}

/// Shares a project by giving the recipient **their own copy** (ADR-0028).
///
/// The response says `new_project_id`, not "access granted", deliberately: the
/// caller should see that a copy was made, because FR-024's consequence — that
/// they can no longer recall those documents — follows from it.
pub async fn share_project(
    State(st): State<AppState>,
    user: AuthUser,
    Json(req): Json<ShareProjectReq>,
) -> ApiResult<Json<Value>> {
    require_projects_enabled(&st).await?;
    let source = project_repo::owned_project(&st.pool, &user.id, &req.project_id)
        .await
        .map_err(err)?
        .ok_or_else(project_not_found)?;

    let recipient: Option<(String,)> =
        sqlx::query_as("SELECT id FROM users WHERE username = $1 AND disabled_at IS NULL")
            .bind(req.recipient.trim())
            .fetch_optional(&st.pool)
            .await
            .map_err(err)?;
    let Some((recipient_id,)) = recipient else {
        return Err(ApiError::new(
            StatusCode::NOT_FOUND,
            "no_such_user",
            "No active user with that name on this deployment.",
        ));
    };
    if recipient_id == user.id {
        return Err(ApiError::new(
            StatusCode::BAD_REQUEST,
            "self_share",
            "That project is already yours.",
        ));
    }

    let new_project_id =
        project_svc::copy_project(&st.pool, &source.id, &recipient_id, None).await.map_err(err)?;
    Ok(Json(json!({
        "shared_with": req.recipient.trim(),
        "new_project_id": new_project_id,
    })))
}

// ════════════════════════════════════════════════════════════════════════════
// US6 — chat share links: view by link, fork to continue
// ════════════════════════════════════════════════════════════════════════════

#[derive(Deserialize)]
pub struct ConversationIdReq {
    pub conversation_id: String,
}

#[derive(Deserialize)]
pub struct TokenReq {
    pub token: String,
}

/// Does the caller own this conversation?
///
/// There is no conversation table, so ownership is "I have turns in it" — read
/// from `query_log`, which is where chat history lives (migration 020).
async fn owns_conversation(
    st: &AppState,
    user_id: &str,
    conversation_id: &str,
) -> Result<bool, ApiError> {
    let mine: bool = sqlx::query_scalar(
        "SELECT EXISTS (SELECT 1 FROM query_log WHERE conversation_id = $1 AND user_id = $2)",
    )
    .bind(conversation_id)
    .bind(user_id)
    .fetch_one(&st.pool)
    .await
    .map_err(err)?;
    Ok(mine)
}

pub async fn share_chat(
    State(st): State<AppState>,
    user: AuthUser,
    Json(req): Json<ConversationIdReq>,
) -> ApiResult<Json<Value>> {
    require_projects_enabled(&st).await?;
    if !owns_conversation(&st, &user.id, &req.conversation_id).await? {
        return Err(project_not_found());
    }
    let token = share_repo::mint(&st.pool, &req.conversation_id, &user.id).await.map_err(err)?;
    Ok(Json(json!({ "token": token, "url": format!("/shared/{token}") })))
}

pub async fn revoke_chat_share(
    State(st): State<AppState>,
    user: AuthUser,
    Json(req): Json<ConversationIdReq>,
) -> ApiResult<Json<Value>> {
    require_projects_enabled(&st).await?;
    if !owns_conversation(&st, &user.id, &req.conversation_id).await? {
        return Err(project_not_found());
    }
    share_repo::revoke(&st.pool, &req.conversation_id, &user.id).await.map_err(err)?;
    Ok(Json(json!({ "revoked": true })))
}

/// Reads a shared chat.
///
/// ⚠️ Takes `AuthUser`, so a signed-out caller is refused by the extractor
/// before the token is ever examined (FR-027). The token SELECTS a chat; it does
/// not authenticate anyone.
///
/// Read-only by ABSENCE: no route anywhere accepts a question for a conversation
/// the caller does not own, so a viewer cannot continue this chat (FR-029). The
/// enforcement is that the capability does not exist.
pub async fn get_shared_chat(
    State(st): State<AppState>,
    _user: AuthUser,
    Json(req): Json<TokenReq>,
) -> ApiResult<Json<Value>> {
    require_projects_enabled(&st).await?;
    let share = share_repo::resolve_live(&st.pool, &req.token)
        .await
        .map_err(err)?
        .ok_or_else(project_not_found)?;

    let turns: Vec<(String, Option<String>, Option<String>, i64)> = sqlx::query_as(
        "SELECT question, answer, citations_json, created_at FROM query_log \
         WHERE conversation_id = $1 ORDER BY created_at ASC",
    )
    .bind(&share.conversation_id)
    .fetch_all(&st.pool)
    .await
    .map_err(err)?;

    let turns: Vec<Value> = turns
        .into_iter()
        .map(|(question, answer, citations_json, created_at)| {
            json!({
                "question": question,
                "answer": answer.unwrap_or_default(),
                "citations": citations_json
                    .and_then(|c| serde_json::from_str::<Value>(&c).ok())
                    .unwrap_or_else(|| Value::Array(vec![])),
                "created_at": created_at,
            })
        })
        .collect();

    Ok(Json(json!({ "conversation_id": share.conversation_id, "turns": turns })))
}

#[derive(Deserialize)]
pub struct SharedChunkReq {
    pub token: String,
    pub chunk_id: String,
}

/// One cited passage from a shared chat.
///
/// ⭐ **Authorised per CHUNK, not per document, and `visible_source_ids` is
/// neither consulted nor widened** (ADR-0029, R5). Had a share link added the
/// chat's sources to the viewer's visible set, the viewer could immediately
/// global-search the owner's entire project — because global search scopes
/// through that same function — and the link would become project access by the
/// back door. The question asked here is the narrowest one that still lets a
/// reader check the answer: *is this chunk cited by a turn of this live share?*
pub async fn get_shared_chunk(
    State(st): State<AppState>,
    _user: AuthUser,
    Json(req): Json<SharedChunkReq>,
) -> ApiResult<Json<Value>> {
    require_projects_enabled(&st).await?;
    let share = share_repo::resolve_live(&st.pool, &req.token)
        .await
        .map_err(err)?
        .ok_or_else(project_not_found)?;

    if !share_repo::is_chunk_cited_by(&st.pool, &share.conversation_id, &req.chunk_id)
        .await
        .map_err(err)?
    {
        return Err(ApiError::new(StatusCode::NOT_FOUND, "not_found", "passage not found"));
    }

    let row: Option<(String, String, Option<String>, Option<i64>)> = sqlx::query_as(
        "SELECT c.content, s.file_name, c.unit_label, c.page_number \
         FROM chunks c JOIN sources s ON s.id = c.source_id WHERE c.id = $1",
    )
    .bind(&req.chunk_id)
    .fetch_optional(&st.pool)
    .await
    .map_err(err)?;
    let (content, file_name, unit_label, page_number) = row
        .ok_or_else(|| ApiError::new(StatusCode::NOT_FOUND, "not_found", "passage not found"))?;

    Ok(Json(json!({
        "chunk_id": req.chunk_id,
        "content": content,
        "file_name": file_name,
        "unit_label": unit_label,
        "page_number": page_number,
    })))
}

/// Takes the viewer's own copy of a shared chat: their own project holding the
/// same documents, plus their own copy of the conversation (FR-030).
///
/// The original is untouched, and the copy survives revocation of the link it
/// came from, because it is an independent project with no reference back.
pub async fn fork_shared_chat(
    State(st): State<AppState>,
    user: AuthUser,
    Json(req): Json<TokenReq>,
) -> ApiResult<Json<Value>> {
    require_projects_enabled(&st).await?;
    let share = share_repo::resolve_live(&st.pool, &req.token)
        .await
        .map_err(err)?
        .ok_or_else(project_not_found)?;
    if share.owner_id == user.id {
        return Err(ApiError::new(
            StatusCode::BAD_REQUEST,
            "self_fork",
            "That chat is already yours.",
        ));
    }

    // The chat's project, if it had one. A chat outside any project forks into a
    // fresh empty project, so the viewer still has somewhere to continue.
    let origin = project_repo::chat_for_conversation(&st.pool, &share.conversation_id)
        .await
        .map_err(err)?;

    let new_project_id = match origin {
        Some(c) => project_svc::copy_project(&st.pool, &c.project_id, &user.id, None)
            .await
            .map_err(err)?,
        None => {
            project_repo::create(&st.pool, &user.id, "Copy of shared chat")
                .await
                .map_err(err)?
                .id
        }
    };

    let new_conversation_id = uuid::Uuid::new_v4().to_string();
    project_repo::create_chat(&st.pool, &new_conversation_id, &new_project_id, &user.id)
        .await
        .map_err(err)?;
    project_svc::copy_conversation_turns(
        &st.pool,
        &share.conversation_id,
        &new_conversation_id,
        &user.id,
    )
    .await
    .map_err(err)?;

    Ok(Json(json!({
        "project_id": new_project_id,
        "conversation_id": new_conversation_id,
    })))
}
