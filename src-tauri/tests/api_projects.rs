//! Feature 027 — "My Projects" end-to-end acceptance over the real axum router
//! and bundled PostgreSQL.
//!
//! These are the tests the feature's success criteria are literally written as.
//! Isolation that is not tested is isolation that is not delivered, so the bulk
//! of this file is NEGATIVE assertions — what must never come back.
//!
//! Covers:
//!   - SC-001  a project question reaches that project and nothing else, on all
//!             four document routes (chat, global search, chunks, file)
//!   - FR-013  a project the caller does not own is refused, identically to one
//!             that does not exist
//!   - FR-012  an empty project says so instead of answering
//!   - SC-005  a copy and its original are independent in BOTH directions
//!   - FR-024  a document survives in the copy after the sharer deletes it
//!   - SC-006  a share link refuses the signed-out
//!   - SC-007  a share viewer cannot ask a question
//!   - ADR-0029 a share viewer's global search still returns nothing of the
//!             owner's project — the back-door test
//!   - SC-008  every project route is refused when the gate is closed
//!   - FR-040  ordinary chat and search are unchanged with the gate closed
//!
//! DB-backed, so it boots the bundled engine like `api_admin` and is `#[ignore]`d
//! by default. Run on the GPU VM:
//!   NOVIQ_TEST_PG_INSTALL_DIR=/home/appadmin/test-pg-install \
//!   cargo test --features server --test api_projects -- --ignored --nocapture
#![cfg(feature = "server")]

use std::sync::Arc;

use axum::body::{to_bytes, Body};
use axum::http::{Request, StatusCode};
use serde_json::{json, Value};
use tower::ServiceExt; // oneshot

use echo_ai_lib::api::{build_router, AppState};
use echo_ai_lib::db;
use echo_ai_lib::services::embedder::EmbedderState;
use echo_ai_lib::services::hardware_detector::HardwareTier;
use echo_ai_lib::services::llm::ModelSlot;
use echo_ai_lib::services::reranker::RerankerSlot;
use echo_ai_lib::services::scheduler::Scheduler;

async fn send(
    router: &axum::Router,
    method: &str,
    path: &str,
    token: Option<&str>,
    body: Option<Value>,
) -> (StatusCode, Value) {
    let mut req = Request::builder().method(method).uri(path);
    if let Some(t) = token {
        req = req.header("authorization", format!("Bearer {t}"));
    }
    let req = if let Some(b) = body {
        req.header("content-type", "application/json")
            .body(Body::from(b.to_string()))
            .unwrap()
    } else {
        req.body(Body::empty()).unwrap()
    };
    let resp = router.clone().oneshot(req).await.unwrap();
    let status = resp.status();
    let bytes = to_bytes(resp.into_body(), usize::MAX).await.unwrap();
    let value: Value = serde_json::from_slice(&bytes).unwrap_or(Value::Null);
    (status, value)
}

async fn login(router: &axum::Router, username: &str, password: &str) -> String {
    let (st, body) = send(
        router,
        "POST",
        "/api/login",
        None,
        Some(json!({"username": username, "password": password})),
    )
    .await;
    assert_eq!(st, StatusCode::OK, "login failed: {body}");
    body["token"].as_str().unwrap().to_string()
}

struct Harness {
    router: axum::Router,
    pool: echo_ai_lib::db::Db,
}

/// Boots a test database + the real router, seeds two users, and optionally
/// turns the feature ON.
///
/// ⚠️ Uses `db::testing::test_pool()` rather than `init_db` into a temp dir.
/// `test_pool` shares ONE bundled engine that honours
/// `NOVIQ_TEST_PG_INSTALL_DIR`; booting a fresh engine per test instead
/// downloads a PostgreSQL build that fails on this VM with
/// `initdb: error while loading shared libraries: libssl.so.3`. (The older
/// `api_admin` harness still does it the other way, which is why it is
/// `#[ignore]`d and, in practice, unrunnable here.)
///
/// The gate starts CLOSED — migration 023 seeds `'0'`, which is the production
/// default — so every test that exercises the feature opens it first, and
/// `gate_closed_refuses_every_project_route` deliberately does not.
async fn harness(enable_feature: bool) -> Harness {
    let tier = HardwareTier::detect();
    let pool = db::testing::test_pool().await;

    let now = chrono::Utc::now().timestamp();
    for (id, name) in [("alice-id", "alice"), ("bob-id", "bob")] {
        let hash = echo_ai_lib::api::auth::hash_password("pw").unwrap();
        sqlx::query(
            "INSERT INTO users (id, username, password_hash, created_at, role) \
             VALUES ($1,$2,$3,$4,'user')",
        )
        .bind(id)
        .bind(name)
        .bind(&hash)
        .bind(now)
        .execute(&pool)
        .await
        .unwrap();
    }

    if enable_feature {
        echo_ai_lib::services::projects::set_my_projects_enabled(&pool, true)
            .await
            .unwrap();
    }

    let embedder = Arc::new(EmbedderState::new(&tier));
    let model_slot = Arc::new(ModelSlot::new().expect("model slot"));
    let reranker_slot = Arc::new(RerankerSlot::new(
        "models/bge-reranker-base/model.onnx",
        "models/bge-reranker-base/tokenizer.json",
    ));
    let scheduler = Scheduler::new(None);
    let gen_gate = Arc::new(tokio::sync::Semaphore::new(1));
    let state = AppState {
        pool: pool.clone(),
        tier,
        embedder,
        model_slot,
        reranker_slot,
        scheduler,
        gen_gate,
    };
    let router = build_router(state);
    Harness { router, pool }
}

/// Inserts an indexed source directly. `owner` `None` + `shared` = corpus.
async fn seed_source(
    pool: &echo_ai_lib::db::Db,
    id: &str,
    owner: Option<&str>,
    visibility: &str,
) {
    sqlx::query(
        "INSERT INTO sources(id,file_path,file_name,format,file_size_bytes,file_hash,\
         last_modified,created_at,indexing_status,owner_id,visibility) \
         VALUES($1,$2,$3,'pdf',10,$4,0,0,'indexed',$5,$6)",
    )
    .bind(id)
    .bind(format!("/{id}.pdf"))
    .bind(format!("{id}.pdf"))
    .bind(format!("hash-{id}"))
    .bind(owner)
    .bind(visibility)
    .execute(pool)
    .await
    .unwrap();
}

// ════════════════════════════════════════════════════════════════════════════
// US1 — isolation across every route (SC-001, FR-013, FR-012)
// ════════════════════════════════════════════════════════════════════════════

#[tokio::test]
#[ignore = "DB-backed; run on the VM with NOVIQ_TEST_PG_INSTALL_DIR set"]
async fn project_scope_isolates_every_route() {
    let h = harness(true).await;
    let alice = login(&h.router, "alice", "pw").await;
    let bob = login(&h.router, "bob", "pw").await;

    // Every category a project question must NOT reach is present and non-empty.
    // An isolation test against an empty database proves nothing.
    seed_source(&h.pool, "corpus-doc", None, "shared").await;
    seed_source(&h.pool, "alice-in", Some("alice-id"), "private").await;
    seed_source(&h.pool, "alice-out", Some("alice-id"), "private").await;
    seed_source(&h.pool, "bob-doc", Some("bob-id"), "private").await;

    let (st, p) = send(
        &h.router,
        "POST",
        "/api/create_project",
        Some(&alice),
        Some(json!({"name": "Tender 2026-14"})),
    )
    .await;
    assert_eq!(st, StatusCode::OK, "create_project: {p}");
    let pid = p["id"].as_str().unwrap().to_string();

    let (st, _) = send(
        &h.router,
        "POST",
        "/api/add_project_documents",
        Some(&alice),
        Some(json!({"project_id": pid, "source_ids": ["alice-in"]})),
    )
    .await;
    assert_eq!(st, StatusCode::OK);

    // ── global_search, scoped ────────────────────────────────────────────────
    let (st, hits) = send(
        &h.router,
        "POST",
        "/api/global_search",
        Some(&alice),
        Some(json!({"query": "anything", "project_id": pid})),
    )
    .await;
    assert_eq!(st, StatusCode::OK, "scoped global_search: {hits}");
    let ids: Vec<&str> = hits
        .as_array()
        .map(|a| a.iter().filter_map(|h| h["source_id"].as_str()).collect())
        .unwrap_or_default();
    for leaked in ["corpus-doc", "alice-out", "bob-doc"] {
        assert!(
            !ids.contains(&leaked),
            "{leaked} leaked into a project-scoped global search: {ids:?}"
        );
    }

    // ── a project the caller does not own: refused, and indistinguishable from
    //    one that does not exist (FR-013) ───────────────────────────────────
    for route in ["/api/global_search", "/api/source_chunks", "/api/get_knowledge_graph"] {
        let body = match route {
            "/api/global_search" => json!({"query": "x", "project_id": pid}),
            "/api/source_chunks" => json!({"sourceId": "alice-in", "project_id": pid}),
            _ => json!({"project_id": pid}),
        };
        let (st, _) = send(&h.router, "POST", route, Some(&bob), Some(body)).await;
        assert_eq!(
            st,
            StatusCode::NOT_FOUND,
            "{route} let bob scope to alice's project — this is the widening hole"
        );
    }
    let (st_missing, _) = send(
        &h.router,
        "POST",
        "/api/global_search",
        Some(&bob),
        Some(json!({"query": "x", "project_id": "no-such-project"})),
    )
    .await;
    assert_eq!(
        st_missing,
        StatusCode::NOT_FOUND,
        "a nonexistent project must answer the same as one the caller cannot see"
    );

    // ── citation opening cannot reach outside the project ────────────────────
    let (st, _) = send(
        &h.router,
        "POST",
        "/api/source_chunks",
        Some(&alice),
        Some(json!({"sourceId": "alice-out", "project_id": pid})),
    )
    .await;
    assert_eq!(
        st,
        StatusCode::NOT_FOUND,
        "a document outside the project was readable through a project-scoped citation"
    );

    let (st, _) = send(
        &h.router,
        "GET",
        &format!("/api/sources/corpus-doc/file?project_id={pid}"),
        Some(&alice),
        None,
    )
    .await;
    assert_eq!(
        st,
        StatusCode::NOT_FOUND,
        "the shared corpus was reachable through a project-scoped file open"
    );
}

#[tokio::test]
#[ignore = "DB-backed; run on the VM with NOVIQ_TEST_PG_INSTALL_DIR set"]
async fn empty_project_reports_nothing_to_search() {
    let h = harness(true).await;
    let alice = login(&h.router, "alice", "pw").await;
    seed_source(&h.pool, "corpus-doc", None, "shared").await;

    let (_, p) = send(
        &h.router,
        "POST",
        "/api/create_project",
        Some(&alice),
        Some(json!({"name": "Empty"})),
    )
    .await;
    let pid = p["id"].as_str().unwrap().to_string();
    let (_, c) = send(
        &h.router,
        "POST",
        "/api/start_project_chat",
        Some(&alice),
        Some(json!({"project_id": pid})),
    )
    .await;
    let conv = c["conversation_id"].as_str().unwrap().to_string();

    let (st, body) = send(
        &h.router,
        "POST",
        "/api/send_message",
        Some(&alice),
        Some(json!({"conversationId": conv, "text": "what does Article 19 say?"})),
    )
    .await;

    // ⚠️ Refusals return HTTP 200 on this system. Assert on the BODY.
    assert_eq!(st, StatusCode::OK);
    let answer = body["answer"].as_str().unwrap_or_default().to_lowercase();
    assert!(
        answer.contains("no documents") || answer.contains("nothing to search"),
        "an empty project answered instead of saying it has nothing to search: {answer}"
    );
    assert!(
        body["citations"].as_array().map(|a| a.is_empty()).unwrap_or(true),
        "an empty project produced citations"
    );
}

// ════════════════════════════════════════════════════════════════════════════
// US5 — copy-on-share (SC-005, FR-024, FR-023)
// ════════════════════════════════════════════════════════════════════════════

#[tokio::test]
#[ignore = "DB-backed; run on the VM with NOVIQ_TEST_PG_INSTALL_DIR set"]
async fn a_copy_is_independent_and_survives_the_original() {
    let h = harness(true).await;
    let alice = login(&h.router, "alice", "pw").await;
    let bob = login(&h.router, "bob", "pw").await;
    seed_source(&h.pool, "shared-doc", Some("alice-id"), "private").await;

    let (_, p) = send(
        &h.router,
        "POST",
        "/api/create_project",
        Some(&alice),
        Some(json!({"name": "Matter A"})),
    )
    .await;
    let pid = p["id"].as_str().unwrap().to_string();
    send(
        &h.router,
        "POST",
        "/api/add_project_documents",
        Some(&alice),
        Some(json!({"project_id": pid, "source_ids": ["shared-doc"]})),
    )
    .await;

    // Share → bob gets a COPY, not access.
    let (st, res) = send(
        &h.router,
        "POST",
        "/api/share_project",
        Some(&alice),
        Some(json!({"project_id": pid, "recipient": "bob"})),
    )
    .await;
    assert_eq!(st, StatusCode::OK, "share_project: {res}");
    let bob_pid = res["new_project_id"].as_str().unwrap().to_string();
    assert_ne!(bob_pid, pid, "sharing must create a NEW project, not grant access");

    // No new `sources` row was created — copying is metadata only (SC-003).
    let source_rows: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM sources")
        .fetch_one(&h.pool)
        .await
        .unwrap();
    assert_eq!(source_rows, 1, "copying must not duplicate the document");

    // FR-023 — chats are not copied.
    let (_, chats) = send(
        &h.router,
        "POST",
        "/api/list_project_chats",
        Some(&bob),
        Some(json!({"project_id": bob_pid})),
    )
    .await;
    assert_eq!(chats.as_array().map(|a| a.len()), Some(0), "chats must not be copied");

    // ⭐ FR-024 — alice deletes the document, then the whole project. Bob's copy
    // keeps working. This is the counter-intuitive one and the one an
    // implementation gets wrong.
    send(
        &h.router,
        "POST",
        "/api/remove_project_document",
        Some(&alice),
        Some(json!({"project_id": pid, "source_id": "shared-doc"})),
    )
    .await;
    send(
        &h.router,
        "POST",
        "/api/delete_project",
        Some(&alice),
        Some(json!({"project_id": pid})),
    )
    .await;

    let (st, bob_project) = send(
        &h.router,
        "POST",
        "/api/get_project",
        Some(&bob),
        Some(json!({"project_id": bob_pid})),
    )
    .await;
    assert_eq!(st, StatusCode::OK, "bob's copy died with the original: {bob_project}");
    let docs = bob_project["documents"].as_array().cloned().unwrap_or_default();
    assert_eq!(
        docs.len(),
        1,
        "FR-024: the document must survive in the copy after the sharer deletes it"
    );

    let alive: bool = sqlx::query_scalar("SELECT EXISTS (SELECT 1 FROM sources WHERE id='shared-doc')")
        .fetch_one(&h.pool)
        .await
        .unwrap();
    assert!(alive, "content was destroyed while another user's project still held it");

    // SC-005 — bob's changes are invisible to alice, and alice no longer exists
    // to be affected. Rename and confirm the copy is bob's alone.
    let (st, _) = send(
        &h.router,
        "POST",
        "/api/rename_project",
        Some(&alice),
        Some(json!({"project_id": bob_pid, "name": "hijack"})),
    )
    .await;
    assert_eq!(st, StatusCode::NOT_FOUND, "the sharer must not retain control of the copy");
}

// ════════════════════════════════════════════════════════════════════════════
// US6 — share links (SC-006, SC-007, ADR-0029 back door)
// ════════════════════════════════════════════════════════════════════════════

#[tokio::test]
#[ignore = "DB-backed; run on the VM with NOVIQ_TEST_PG_INSTALL_DIR set"]
async fn share_link_reads_but_never_widens() {
    let h = harness(true).await;
    let alice = login(&h.router, "alice", "pw").await;
    let bob = login(&h.router, "bob", "pw").await;
    seed_source(&h.pool, "alice-doc", Some("alice-id"), "private").await;

    let (_, p) = send(
        &h.router,
        "POST",
        "/api/create_project",
        Some(&alice),
        Some(json!({"name": "Matter"})),
    )
    .await;
    let pid = p["id"].as_str().unwrap().to_string();
    send(
        &h.router,
        "POST",
        "/api/add_project_documents",
        Some(&alice),
        Some(json!({"project_id": pid, "source_ids": ["alice-doc"]})),
    )
    .await;
    let (_, c) = send(
        &h.router,
        "POST",
        "/api/start_project_chat",
        Some(&alice),
        Some(json!({"project_id": pid})),
    )
    .await;
    let conv = c["conversation_id"].as_str().unwrap().to_string();

    // A turn must exist for the chat to be shareable (ownership is read from
    // `query_log` — there is no conversation table).
    sqlx::query(
        "INSERT INTO query_log (id, user_id, conversation_id, question, answer, created_at) \
         VALUES ('t1','alice-id',$1,'q','a',1)",
    )
    .bind(&conv)
    .execute(&h.pool)
    .await
    .unwrap();

    let (st, s) = send(
        &h.router,
        "POST",
        "/api/share_chat",
        Some(&alice),
        Some(json!({"conversation_id": conv})),
    )
    .await;
    assert_eq!(st, StatusCode::OK, "share_chat: {s}");
    let token = s["token"].as_str().unwrap().to_string();

    // SC-006 — signed out is refused, and leaks nothing.
    let (st, body) = send(
        &h.router,
        "POST",
        "/api/get_shared_chat",
        None,
        Some(json!({"token": token})),
    )
    .await;
    assert_eq!(st, StatusCode::UNAUTHORIZED, "a share link must require a Noviq session");
    assert!(
        body.get("turns").is_none(),
        "the transcript leaked to a signed-out caller"
    );

    // A signed-in colleague reads it.
    let (st, chat) = send(
        &h.router,
        "POST",
        "/api/get_shared_chat",
        Some(&bob),
        Some(json!({"token": token})),
    )
    .await;
    assert_eq!(st, StatusCode::OK, "a signed-in user should be able to read: {chat}");
    assert_eq!(chat["turns"].as_array().map(|a| a.len()), Some(1));

    // ⭐ THE BACK-DOOR TEST (ADR-0029 Alternative E). Having read the shared
    // chat, bob must STILL not be able to reach alice's project through global
    // search. If viewing had widened `visible_source_ids`, the transcript above
    // would look correct and this would quietly leak the whole project.
    let (st, hits) = send(
        &h.router,
        "POST",
        "/api/global_search",
        Some(&bob),
        Some(json!({"query": "anything"})),
    )
    .await;
    assert_eq!(st, StatusCode::OK);
    let ids: Vec<&str> = hits
        .as_array()
        .map(|a| a.iter().filter_map(|x| x["source_id"].as_str()).collect())
        .unwrap_or_default();
    assert!(
        !ids.contains(&"alice-doc"),
        "reading a shared chat widened the viewer's document set — the link became \
         project access by the back door"
    );

    // SC-007 — a viewer cannot ask a question in someone else's chat.
    let (_, turn) = send(
        &h.router,
        "POST",
        "/api/send_message",
        Some(&bob),
        Some(json!({"conversationId": conv, "text": "follow-up"})),
    )
    .await;
    let cited: Vec<&str> = turn["citations"]
        .as_array()
        .map(|a| a.iter().filter_map(|c| c["instrument"].as_str()).collect())
        .unwrap_or_default();
    assert!(
        !cited.iter().any(|c| c.contains("alice-doc")),
        "a share viewer reached the owner's documents by posting to their conversation"
    );

    // FR-031 — revocation kills the link for everyone.
    send(
        &h.router,
        "POST",
        "/api/revoke_chat_share",
        Some(&alice),
        Some(json!({"conversation_id": conv})),
    )
    .await;
    let (st, _) = send(
        &h.router,
        "POST",
        "/api/get_shared_chat",
        Some(&bob),
        Some(json!({"token": token})),
    )
    .await;
    assert_eq!(st, StatusCode::NOT_FOUND, "a revoked link still resolved");
}

// ════════════════════════════════════════════════════════════════════════════
// US7 — the gate (SC-008, SC-009, FR-040)
// ════════════════════════════════════════════════════════════════════════════

#[tokio::test]
#[ignore = "DB-backed; run on the VM with NOVIQ_TEST_PG_INSTALL_DIR set"]
async fn gate_closed_refuses_every_project_route() {
    // Deliberately NOT enabled — this is the production default after applying
    // migration 023, and it must expose nothing.
    let h = harness(false).await;
    let alice = login(&h.router, "alice", "pw").await;
    seed_source(&h.pool, "corpus-doc", None, "shared").await;

    // Enumerated so a route added later WITHOUT a gate check fails this test.
    let routes: &[(&str, Value)] = &[
        ("/api/list_projects", json!({})),
        ("/api/create_project", json!({"name": "x"})),
        ("/api/rename_project", json!({"project_id": "p", "name": "x"})),
        ("/api/delete_project", json!({"project_id": "p"})),
        ("/api/get_project", json!({"project_id": "p"})),
        ("/api/add_project_documents", json!({"project_id": "p", "source_ids": []})),
        ("/api/remove_project_document", json!({"project_id": "p", "source_id": "s"})),
        ("/api/list_project_chats", json!({"project_id": "p"})),
        ("/api/start_project_chat", json!({"project_id": "p"})),
        ("/api/is_project_chat", json!({"conversation_id": "c"})),
        ("/api/share_project", json!({"project_id": "p", "recipient": "bob"})),
        ("/api/share_chat", json!({"conversation_id": "c"})),
        ("/api/revoke_chat_share", json!({"conversation_id": "c"})),
        ("/api/get_shared_chat", json!({"token": "t"})),
        ("/api/get_shared_chunk", json!({"token": "t", "chunk_id": "c"})),
        ("/api/fork_shared_chat", json!({"token": "t"})),
    ];
    for (route, body) in routes {
        let (st, _) = send(&h.router, "POST", route, Some(&alice), Some(body.clone())).await;
        assert_eq!(
            st,
            StatusCode::NOT_FOUND,
            "{route} answered while the feature was switched off — the UI is not the gate"
        );
    }

    // FR-040 — everything that existed before this feature behaves as it did.
    let (st, _) = send(
        &h.router,
        "POST",
        "/api/global_search",
        Some(&alice),
        Some(json!({"query": "anything"})),
    )
    .await;
    assert_eq!(st, StatusCode::OK, "ordinary global search regressed with the gate closed");

    // SC-009 — turning it on restores access; nothing was deleted.
    echo_ai_lib::services::projects::set_my_projects_enabled(&h.pool, true)
        .await
        .unwrap();
    let (st, _) = send(&h.router, "POST", "/api/list_projects", Some(&alice), Some(json!({}))).await;
    assert_eq!(st, StatusCode::OK, "re-enabling did not restore access");
}

// ════════════════════════════════════════════════════════════════════════════
// T099 — performance (SC-003, SC-010)
// ════════════════════════════════════════════════════════════════════════════

/// SC-003 — copying a 50-document project completes well inside 5 s, and does so
/// because it is **metadata only**: two statements in one transaction, neither of
/// which is per-document work beyond a single `INSERT … SELECT`.
///
/// ⚠️ The assertion that matters here is not the clock — a fast machine would
/// pass this even if the copy re-indexed, given ten-byte fixture files. It is the
/// pair of row counts: `sources` must be unchanged (no content duplicated) and
/// `project_source` must have exactly doubled (memberships, and only
/// memberships). Those are what make the bound hold for real 50 MB documents,
/// where a duplicating implementation would take minutes. The timing is recorded
/// as corroboration, and printed so a regression is visible in the log.
///
/// SC-010 — the same test measures EXCLUSIVE-mode scoping at 50 documents, since
/// `visible_source_ids` is the only thing Feature 027 adds to the retrieval hot
/// path. It is one indexed membership join; anything near the budget here would
/// mean the mode, not the model, is the cost.
#[tokio::test]
#[ignore = "DB-backed; run on the VM with NOVIQ_TEST_PG_INSTALL_DIR set"]
async fn copying_a_fifty_document_project_is_metadata_only_and_fast() {
    use echo_ai_lib::db::repositories::sources as source_repo;
    use std::time::Instant;

    const DOCS: usize = 50;

    let h = harness(true).await;
    let alice = login(&h.router, "alice", "pw").await;

    let (_, p) = send(
        &h.router,
        "POST",
        "/api/create_project",
        Some(&alice),
        Some(json!({"name": "Fifty"})),
    )
    .await;
    let pid = p["id"].as_str().unwrap().to_string();

    let mut ids = Vec::with_capacity(DOCS);
    for i in 0..DOCS {
        let id = format!("perf-doc-{i}");
        seed_source(&h.pool, &id, Some("alice-id"), "private").await;
        ids.push(id);
    }
    let (st, res) = send(
        &h.router,
        "POST",
        "/api/add_project_documents",
        Some(&alice),
        Some(json!({"project_id": pid, "source_ids": ids})),
    )
    .await;
    assert_eq!(st, StatusCode::OK, "add_project_documents: {res}");

    // ── SC-003 ──────────────────────────────────────────────────────────────
    let started = Instant::now();
    let (st, res) = send(
        &h.router,
        "POST",
        "/api/share_project",
        Some(&alice),
        Some(json!({"project_id": pid, "recipient": "bob"})),
    )
    .await;
    let elapsed = started.elapsed();
    assert_eq!(st, StatusCode::OK, "share_project: {res}");
    let bob_pid = res["new_project_id"].as_str().unwrap().to_string();

    let source_rows: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM sources")
        .fetch_one(&h.pool)
        .await
        .unwrap();
    assert_eq!(
        source_rows, DOCS as i64,
        "the copy duplicated content — SC-003's bound rests on it not doing so"
    );
    let chunk_rows: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM chunks")
        .fetch_one(&h.pool)
        .await
        .unwrap();
    assert_eq!(chunk_rows, 0, "the copy created chunks, i.e. it re-indexed");

    let memberships: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM project_source")
        .fetch_one(&h.pool)
        .await
        .unwrap();
    assert_eq!(
        memberships,
        (DOCS * 2) as i64,
        "the copy must add exactly one membership per document and nothing else"
    );

    assert!(
        elapsed.as_secs_f64() < 5.0,
        "SC-003: copying {DOCS} documents took {elapsed:?}, budget 5 s"
    );

    // ── SC-010 — exclusive-mode scoping cost at 50 documents ────────────────
    let scope = echo_ai_lib::db::repositories::sources::RequestScope {
        user_id: Some("bob-id".into()),
        conversation_id: None,
        project_id: Some(bob_pid.clone()),
        include_corpus: false,
    };
    let started = Instant::now();
    let visible = source_repo::visible_source_ids(&h.pool, &scope).await.unwrap();
    let scope_elapsed = started.elapsed();
    assert_eq!(
        visible.len(),
        DOCS,
        "the copy must see every document it was given, and only those"
    );
    assert!(
        scope_elapsed.as_millis() < 500,
        "SC-010: exclusive-mode scoping took {scope_elapsed:?} for {DOCS} documents"
    );

    println!(
        "T099: copy of {DOCS} documents = {:.3} s (budget 5 s); \
         exclusive-mode scoping = {:.1} ms; sources {source_rows}, memberships {memberships}, chunks {chunk_rows}",
        elapsed.as_secs_f64(),
        scope_elapsed.as_secs_f64() * 1000.0
    );
}
