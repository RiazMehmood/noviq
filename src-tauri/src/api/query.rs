//! `POST /api/query` (US1, T011) — one chat turn over HTTP.
//!
//! Authenticated (the `AuthUser` extractor enforces a valid session). Drives the
//! exact same pipeline as the desktop `send_message` command via
//! `assistant::run_turn`, passing a conversation-scoped compat `AppHandle` so
//! answer tokens + phases stream live over `GET /api/chat/events` (SSE); the
//! full `TurnResponse` with answer + citations is still returned in the body.
//!
//! The turn runs in a DETACHED task (`tokio::spawn`): reverse proxies in front
//! of the server (the Cloudflare quick tunnel) kill long-lived HTTP requests at
//! ~100 s, and axum cancels a handler future when the client disconnects — which
//! used to abort a map-reduce turn minutes into its work with nothing persisted.
//! Detaching lets the turn finish and land in `query_log` regardless of the
//! connection; the browser recovers the answer via `POST /api/poll_turn`.

use axum::extract::State;
use axum::http::StatusCode;
use axum::Json;
use serde::Deserialize;

use super::auth::AuthUser;
use super::{ApiError, ApiResult, AppState};
use crate::services::assistant::{run_turn, TurnResponse};

#[derive(Deserialize)]
pub struct QueryReq {
    // `conversationId` is the Tauri invoke arg name; accept it too so the shared
    // frontend `transport.call("send_message", {conversationId, text})` works
    // unchanged over the web transport (T010).
    #[serde(alias = "conversationId")]
    pub conversation_id: String,
    pub text: String,
    // Composer document scope (document-tree checkboxes + @-mention chips). Omit
    // / empty = no restriction; a non-empty set narrows retrieval to those
    // source ids (intersected server-side with what this user may see).
    #[serde(alias = "sourceIds", default)]
    pub source_ids: Option<Vec<String>>,
    // Deep-search toggle (chat composer). When true this turn runs one much
    // richer retrieval+generation pass (wider pool, graph + summary recall,
    // larger budget, comprehensive prompt) — slower but more thorough.
    #[serde(alias = "deep", default)]
    pub deep: bool,
    // "Search all documents" widen button (scope-routing 2026-07). When true this
    // turn ignores the default narrow-to-attachment scoping and searches the full
    // visible corpus — the escape hatch from a file-scoped chat.
    #[serde(alias = "widen", default)]
    pub widen: bool,
    // Web-search toggle (chat composer, 2026-07-29). When true this turn ALSO
    // runs a public web search and shows the results as a separate, separately
    // cited section. It never changes retrieval over the legal corpus.
    #[serde(alias = "web", default)]
    pub web: bool,
    // F027 — include the shared legal corpus in THIS question, alongside the
    // project's own documents. Per-question by decision: it lives here on the
    // request and is persisted nowhere, which is exactly what makes it return to
    // excluded on the next turn (FR-017). If you ever find yourself adding a
    // column for it, the requirement is being lost.
    //
    // Ignored outside a project chat — ordinary chat already searches the
    // corpus, so there is nothing for it to widen.
    #[serde(alias = "includeCorpus", default)]
    pub include_corpus: bool,
}

/// Longest question accepted, in characters (`NOVIQ_MAX_QUERY_CHARS` overrides).
///
/// 2026-08-11 — nothing capped this, so a pasted chapter became the question.
/// That is not merely rude to the box: the allocator carves the prompt out of a
/// fixed window, and an enormous question evicts the retrieved LAW to make room
/// for it — the answer then comes back as "I don't have enough information"
/// after two minutes of GPU time. Refusing up front is the honest failure.
///
/// Sized against the DEPLOYED window, not a guess: `NOVIQ_VLLM_MAX_MODEL_LEN`
/// is 8,192 and `gen_reserve_qa` reserves 1,024 for the answer, so persona +
/// history + retrieved law + the question share ~7,168 tokens. 2,000 characters
/// ≈ 500 tokens ≈ **7%** of that — a paragraph and a half, longer than any
/// question anyone types by hand, and cheap enough that the law keeps the rest.
/// Text longer than this is a document, and attaching it is both supported and
/// better: an attachment is retrieved from, not crammed into the prompt.
///
/// Enforced HERE, not only in the composer, because the composer is not a gate —
/// a crafted request or a stale bundle bypasses it.
fn max_query_chars() -> usize {
    std::env::var("NOVIQ_MAX_QUERY_CHARS")
        .ok()
        .and_then(|v| v.parse().ok())
        .filter(|n| *n > 0)
        .unwrap_or(2_000)
}

pub async fn query(
    State(st): State<AppState>,
    user: AuthUser, // requires a valid session; identity hook for FR-010
    Json(req): Json<QueryReq>,
) -> ApiResult<Json<TurnResponse>> {
    // Length first — before the audit row, the spawn and the generation permit.
    // `chars()`, not `len()`: Urdu and em-dashes are multi-byte, and a byte cap
    // would refuse a legitimate Urdu question at a third of the stated limit.
    let limit = max_query_chars();
    let asked = req.text.chars().count();
    if asked > limit {
        return Err(ApiError::new(
            StatusCode::BAD_REQUEST,
            "query_too_long",
            format!(
                "That question is {asked} characters — the limit is {limit}. \
                 Shorten it, or attach the text as a document and ask about it."
            ),
        ));
    }

    // ── Free-trial quota (migration 029) ───────────────────────────────────
    // Spent BEFORE the audit row, the spawn and the generation permit: a turn
    // that will be refused must not occupy the single-permit generation gate,
    // and must not be recorded as a query that happened.
    //
    // ⚠️ `consume` is a single conditional UPDATE, so two turns racing for the
    // last question cannot both win. It is spent on ASKING, not on answering —
    // a user who cancels has still used their question, which is the only
    // reading that cannot be gamed by aborting the stream.
    if user.role == "trial" {
        let st_trial = crate::db::repositories::trial::status(&st.pool, &user.id)
            .await
            .map_err(|e| ApiError::internal(e.to_string()))?;
        match st_trial {
            None => {
                return Err(ApiError::new(
                    StatusCode::FORBIDDEN,
                    "trial_missing",
                    "This trial account is not set up correctly. Please request a quote.",
                ))
            }
            Some(t) if t.revoked => {
                return Err(ApiError::new(
                    StatusCode::FORBIDDEN,
                    "trial_revoked",
                    "This trial has been ended.",
                ))
            }
            Some(t) if t.expired => {
                return Err(ApiError::new(
                    StatusCode::FORBIDDEN,
                    "trial_expired",
                    "Your 3-day trial has finished. Request a quote and we will set you up properly.",
                ))
            }
            Some(_) => {
                let ok = crate::db::repositories::trial::consume(&st.pool, &user.id)
                    .await
                    .map_err(|e| ApiError::internal(e.to_string()))?;
                if !ok {
                    return Err(ApiError::new(
                        StatusCode::TOO_MANY_REQUESTS,
                        "trial_quota",
                        "You have used your 3 questions for today. They reset at midnight UTC.",
                    ));
                }
            }
        }
    }

    // Multi-user traceability (US5/T038, FR-011): who queried which conversation.
    let _ = crate::db::repositories::audit::insert_user_event(
        &st.pool,
        Some(&user.id),
        "query",
        Some(&req.conversation_id),
        None,
    )
    .await;

    // Run the whole turn in a detached task so a client disconnect (tunnel
    // timeout, closed tab) cannot cancel it mid-pipeline — the answer still
    // completes and persists to `query_log`, where `poll_turn` can recover it.
    let user_id = user.id.clone();
    let conversation_id = req.conversation_id.clone();
    let text = req.text.clone();
    let source_ids = req.source_ids.clone();
    // Audience gate: Deep search is admin-only while it's being smoothed
    // (ECHO_DEEP_AUDIENCE=all opens it to everyone). A non-admin's `deep:true` —
    // whether from a stale UI or a crafted request — is silently downgraded to a
    // normal search here, on the same line the flag is read, so the UI toggle is
    // never the enforcement point. Same rule the `/me` response advertises.
    let deep = req.deep && crate::services::deep_search::deep_enabled_for_role(&user.role);
    // Migration 029 — a free-trial session. Read from the session role on the
    // same line it is used, exactly like `deep` above, so the UI is never the
    // enforcement point. Drives BOTH the daily quota and the demo-only corpus.
    let is_trial = user.role == "trial";
    let widen = req.widen;
    let web = req.web;
    // F027 — per-turn only; never stored, never carried to the next question.
    let include_corpus = req.include_corpus;
    let task = tokio::spawn(async move {
        // Serialize generation across concurrent requests for the in-process
        // llama.cpp engine (US4/T023, FR-008). F016: when generating through the
        // vLLM sidecar, vLLM owns batching + admission, so we DON'T serialize
        // here — that width-1 gate is exactly what blocks the 10-concurrent-user
        // requirement. Acquired INSIDE the task so the permit is held for the
        // turn's real duration even after the HTTP client goes away.
        let _permit = if crate::services::inference::engine_from_env().is_http() {
            None
        } else {
            Some(
                st.gen_gate
                    .acquire()
                    .await
                    .map_err(|_| "generation gate closed".to_string())?,
            )
        };

        // Sample process + GPU resource use for the duration of this turn.
        // Because the gate above serializes generation, the peak attributes
        // cleanly to this user's query (admin usage visibility).
        let started = std::time::Instant::now();
        let sampler = crate::services::usage_sampler::Sampler::start();

        // Conversation-scoped compat handle: the shared services' `app.emit(...)`
        // calls (answer tokens + pipeline phases) flow into the `chat_stream`
        // broadcast, which `GET /api/chat/events` streams to this conversation's
        // browser subscriber. The full TurnResponse is still returned in the body.
        let stream_handle = crate::compat::AppHandle::for_conversation(conversation_id.clone());
        // F021 (US1): retrieval for this turn is scoped to what THIS user may
        // see — shared corpus + their own documents + this conversation's
        // attachments.
        //
        // F027 (ADR-0029): if this conversation belongs to a project, that scope
        // becomes EXCLUSIVE — the project's documents and nothing else.
        //
        // ⭐ Note what is NOT read here: the request body carries no project id,
        // and deliberately cannot. The project is DERIVED from the conversation
        // the client is already sending, then checked against the authenticated
        // user. A forged or guessed `conversation_id` finds either no row or
        // another user's row, and the `owner_id` comparison rejects the second.
        // The chat path therefore cannot be widened by a crafted request at all
        // — unlike the four document routes, which must take the project from
        // the client and verify it (`api/projects.rs::scope_for`).
        let project = crate::db::repositories::projects::chat_for_conversation(
            &st.pool,
            &conversation_id,
        )
        .await
        .ok()
        .flatten()
        .filter(|c| c.owner_id == user_id);
        let project_id = project.map(|c| c.project_id);
        let in_project = project_id.is_some();
        let scope = crate::db::repositories::sources::RequestScope {
            user_id: Some(user_id.clone()),
            conversation_id: Some(conversation_id.clone()),
            project_id,
            // Only meaningful inside a project; ordinary chat already sees the
            // corpus, so the flag has nothing to widen there.
            include_corpus: in_project && include_corpus,
            // Migration 029. ⚠️ Derived from the SESSION's role, never from the
            // request body — a client-supplied value could only ever be set to
            // `false`, which is the direction that leaks the corpus.
            trial: is_trial,
        };
        // Deep search is carried as a per-request task-local for the whole turn
        // tree (retrieval knobs + prompt read it), so it stays concurrency-safe
        // across simultaneous users without threading a flag through every call.
        // Web search is carried the same way as deep search: a per-request
        // task-local scoping the whole turn tree, so concurrent users cannot
        // read each other's flag.
        let result = crate::services::query_router::with_web_search(
            web,
            crate::services::query_router::with_deep_search(
            deep,
            run_turn(
                &st.pool,
                &st.embedder,
                &st.model_slot,
                &st.reranker_slot,
                &st.tier,
                &st.scheduler,
                Some(&stream_handle),
                &conversation_id,
                &text,
                &scope,
                source_ids.as_deref(),
                widen,
            ),
        ),
        )
        .await;

        let peak = sampler.finish().await;
        let latency_ms = started.elapsed().as_millis() as i64;
        let resp = result?;

        // Record the turn (best-effort — never fails the user's chat). This is
        // also the recovery record `poll_turn` serves after a dropped connection.
        let citations_json = serde_json::to_string(&resp.citations).ok();
        // Migration 020: the WHOLE response, so `/api/conversation_turns` can
        // replay this turn on another device exactly as it rendered here — web
        // results, capability, widen flag and export handles included, none of
        // which survive in `citations_json`.
        let response_json = serde_json::to_string(&resp).ok();
        let _ = crate::db::repositories::query_log::insert(
            &st.pool,
            Some(&user_id),
            &conversation_id,
            &text,
            &resp.answer,
            citations_json.as_deref(),
            response_json.as_deref(),
            latency_ms,
            &peak,
        )
        .await;

        Ok::<TurnResponse, String>(resp)
    });

    let resp = task
        .await
        .map_err(|e| ApiError::internal(format!("turn task failed: {e}")))?
        .map_err(ApiError::internal)?;
    Ok(Json(resp))
}

#[derive(Deserialize)]
pub struct CancelTurnReq {
    #[serde(alias = "conversationId")]
    pub conversation_id: String,
}

/// `POST /api/cancel_turn` — stop button (F026).
///
/// The chat turn runs in a detached task; a client disconnect deliberately does
/// NOT cancel it (so the answer still persists — see the module header). This is
/// the *explicit* stop: the officer typed the wrong query, or no longer wants a
/// slow deep run. We look up the conversation's live workload and signal its
/// cancellation token; the turn unwinds at its next cooperative boundary. Auth is
/// required; the conversation id is an unguessable per-user UUID. Idempotent —
/// `{cancelled:false}` simply means no live turn was found (already finished).
pub async fn cancel_turn(
    State(st): State<AppState>,
    _user: AuthUser,
    Json(req): Json<CancelTurnReq>,
) -> ApiResult<Json<serde_json::Value>> {
    let cancelled = st.scheduler.cancel_conversation(&req.conversation_id);
    Ok(Json(serde_json::json!({ "cancelled": cancelled })))
}

#[derive(Deserialize)]
pub struct AssistStatusReq {
    #[serde(alias = "conversationId")]
    pub conversation_id: String,
}

/// `POST /api/assist_status` — is a Notice Assist session open for this conversation?
///
/// The banner is a *mirror* of server state, and the frontend cannot infer that
/// state on its own: opening a past conversation from the history list says nothing
/// about whether `/help` was ever typed in it. Without this, reopening a conversation
/// with a live session showed no banner while `run_turn` still routed every question
/// into the notice flow — assist mode with no sign of it.
///
/// Cheap: one indexed lookup by primary key, no model, no retrieval.
pub async fn assist_status(
    State(st): State<AppState>,
    _user: AuthUser,
    Json(req): Json<AssistStatusReq>,
) -> ApiResult<Json<serde_json::Value>> {
    use crate::services::assistant::capabilities::notice_assist::session;
    let open = session::load(&st.pool, &req.conversation_id)
        .await
        .map_err(ApiError::internal)?
        .is_some();
    Ok(Json(serde_json::json!({ "open": open })))
}

#[derive(Deserialize)]
pub struct AssistExitReq {
    #[serde(alias = "conversationId")]
    pub conversation_id: String,
}

/// `POST /api/assist_exit` — leave Notice Assist (F022) WITHOUT running a chat turn.
///
/// The assist session is server state (`notice_assist_session`), and while a row
/// exists `run_turn` routes every turn in the conversation into the notice flow
/// (`assist_open`) — the frontend's banner is only a mirror of it. So "Leave" cannot
/// be a client-side flag flip: that would hide the banner while the session stayed
/// open, and the officer's next ordinary question would be silently hijacked into
/// the notice flow.
///
/// Before this endpoint, the Leave button submitted the text `/cancel` through the
/// normal chat pipe to reach `session::delete`. That worked, but it posted a visible
/// user bubble, spun the composer, and appended an assistant reply — the officer saw
/// a chat query run when they had asked to leave. Typing `/cancel` still works and is
/// unchanged; the button no longer needs to fake it.
///
/// Idempotent: `{"left":false}` means there was no open session to leave.
pub async fn assist_exit(
    State(st): State<AppState>,
    _user: AuthUser,
    Json(req): Json<AssistExitReq>,
) -> ApiResult<Json<serde_json::Value>> {
    use crate::services::assistant::capabilities::notice_assist::session;
    let had = session::load(&st.pool, &req.conversation_id)
        .await
        .map_err(ApiError::internal)?
        .is_some();
    session::delete(&st.pool, &req.conversation_id)
        .await
        .map_err(ApiError::internal)?;
    Ok(Json(serde_json::json!({ "left": had })))
}

#[derive(Deserialize)]
pub struct PollTurnReq {
    #[serde(alias = "conversationId")]
    pub conversation_id: String,
    /// Only turns recorded at/after this timestamp (ms since epoch) count —
    /// the browser passes the moment it sent the query, so a recovered answer
    /// is always THE answer to that query, never a stale earlier turn.
    #[serde(alias = "sinceMs")]
    pub since_ms: i64,
}

/// `POST /api/poll_turn` — recover a turn whose HTTP response was lost.
///
/// The Cloudflare quick tunnel kills requests after ~100 s; map-reduce turns
/// legitimately take minutes. The detached task above still finishes and writes
/// `query_log`, so the browser polls here (same conversation + its own user id)
/// until the row appears. Returns `{done:false}` while the turn is in flight.
pub async fn poll_turn(
    State(st): State<AppState>,
    user: AuthUser,
    Json(req): Json<PollTurnReq>,
) -> ApiResult<Json<serde_json::Value>> {
    let row = crate::db::repositories::query_log::latest_since(
        &st.pool,
        &user.id,
        &req.conversation_id,
        req.since_ms,
    )
    .await
    .map_err(|e| ApiError::internal(e.to_string()))?;

    let Some(row) = row else {
        return Ok(Json(serde_json::json!({ "done": false })));
    };
    let citations: serde_json::Value = row
        .citations_json
        .as_deref()
        .and_then(|c| serde_json::from_str(c).ok())
        .unwrap_or_else(|| serde_json::json!([]));
    Ok(Json(serde_json::json!({
        "done": true,
        "message_id": row.id,
        "question": row.question,
        "answer": row.answer.unwrap_or_default(),
        "citations": citations,
        "created_at": row.created_at,
    })))
}
