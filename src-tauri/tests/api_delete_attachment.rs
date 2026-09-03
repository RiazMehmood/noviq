//! B1 (2026-08-07) — deleting a chat attachment must actually delete it.
//!
//! The bug this file exists for: `delete_conversation` reported
//! `{"ok":true,"removed":1}` and removed **nothing**. The `sources` row, its
//! `chunks` and the file all survived, still indexed and still searchable, while
//! the user was told their attachment was gone.
//!
//! Cause was an ordering disagreement between two predicates:
//!   * `is_orphaned`    counts only `source_link`
//!   * `is_referenced`  counts `source_link` **and** `notebook_sources`
//!
//! The `__conv__<id>` stub notebook was dropped AFTER the delete loop, so every
//! attachment looked orphaned to the loop and referenced to the delete.
//!
//! ⚠️ A repository-level test on the two predicates (in `sources.rs`) pins the
//! mechanism but would NOT catch a regression: moving the stub-drop back after
//! the loop leaves those predicates untouched. Only an end-to-end call through
//! the real router observes the ordering, which is why this file exists.
//!
//! DB-backed, so `#[ignore]`d by default. Run on the GPU VM:
//!   NOVIQ_TEST_PG_INSTALL_DIR=/home/appadmin/test-pg-install \
//!   cargo test --features server --test api_delete_attachment -- --ignored --nocapture
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
    path: &str,
    token: &str,
    body: Value,
) -> (StatusCode, Value) {
    let req = Request::builder()
        .method("POST")
        .uri(path)
        .header("authorization", format!("Bearer {token}"))
        .header("content-type", "application/json")
        .body(Body::from(body.to_string()))
        .unwrap();
    let resp = router.clone().oneshot(req).await.unwrap();
    let status = resp.status();
    let bytes = to_bytes(resp.into_body(), usize::MAX).await.unwrap();
    (status, serde_json::from_slice(&bytes).unwrap_or(Value::Null))
}

struct Harness {
    router: axum::Router,
    pool: echo_ai_lib::db::Db,
}

/// Same shape as `api_projects::harness` — `test_pool()` shares one bundled
/// engine honouring `NOVIQ_TEST_PG_INSTALL_DIR`; booting a fresh one per test
/// fails on this VM with `initdb: libssl.so.3`.
async fn harness() -> (Harness, String) {
    let tier = HardwareTier::detect();
    let pool = db::testing::test_pool().await;
    let now = chrono::Utc::now().timestamp();

    let uid = format!("b1-user-{}", uuid::Uuid::new_v4());
    let uname = format!("b1user{}", &uid[8..16]);
    let hash = echo_ai_lib::api::auth::hash_password("pw").unwrap();
    sqlx::query(
        "INSERT INTO users (id, username, password_hash, created_at, role) \
         VALUES ($1,$2,$3,$4,'user')",
    )
    .bind(&uid).bind(&uname).bind(&hash).bind(now)
    .execute(&pool).await.unwrap();

    let embedder = Arc::new(EmbedderState::new(&tier));
    let model_slot = Arc::new(ModelSlot::new().expect("model slot"));
    let reranker_slot = Arc::new(RerankerSlot::new(
        "models/bge-reranker-base/model.onnx",
        "models/bge-reranker-base/tokenizer.json",
    ));
    let state = AppState {
        pool: pool.clone(),
        tier,
        embedder,
        model_slot,
        reranker_slot,
        scheduler: Scheduler::new(None),
        gen_gate: Arc::new(tokio::sync::Semaphore::new(1)),
    };
    let router = build_router(state);

    // Log in for a real bearer token.
    let req = Request::builder()
        .method("POST").uri("/api/login")
        .header("content-type", "application/json")
        .body(Body::from(json!({"username": uname, "password": "pw"}).to_string()))
        .unwrap();
    let resp = router.clone().oneshot(req).await.unwrap();
    let bytes = to_bytes(resp.into_body(), usize::MAX).await.unwrap();
    let v: Value = serde_json::from_slice(&bytes).unwrap();
    let token = v["token"].as_str().expect("login token").to_string();

    (Harness { router, pool }, format!("{uid}|{token}"))
}

/// Seeds an attachment exactly the way `api/uploads.rs` does: a private source
/// stamped with `conversation_id`, a `source_link` row, the `__conv__` stub
/// notebook, a `notebook_sources` row, and one chunk so "still searchable" is
/// observable.
async fn seed_attachment(
    pool: &echo_ai_lib::db::Db,
    source_id: &str,
    conv_id: &str,
    owner_id: &str,
) {
    sqlx::query(
        "INSERT INTO sources(id,file_path,file_name,format,file_size_bytes,file_hash,\
         last_modified,created_at,indexing_status,owner_id,visibility,conversation_id) \
         VALUES($1,$2,$3,'pdf',10,$4,0,0,'indexed',$5,'private',$6)",
    )
    .bind(source_id)
    .bind(format!("/{source_id}.pdf"))
    .bind(format!("{source_id}.pdf"))
    .bind(format!("hash-{source_id}"))
    .bind(owner_id)
    .bind(conv_id)
    .execute(pool).await.unwrap();

    sqlx::query(
        "INSERT INTO source_link(source_id,conversation_id,owner_id,linked_at) \
         VALUES($1,$2,$3,0)",
    )
    .bind(source_id).bind(conv_id).bind(owner_id)
    .execute(pool).await.unwrap();

    let nb = format!("__conv__{conv_id}");
    sqlx::query(
        "INSERT INTO notebooks(id,name,created_at,updated_at) VALUES($1,$2,0,0)",
    )
    .bind(&nb).bind(format!("Conversation attachments: {conv_id}"))
    .execute(pool).await.unwrap();

    sqlx::query(
        "INSERT INTO notebook_sources(notebook_id,source_id,added_at) VALUES($1,$2,0)",
    )
    .bind(&nb).bind(source_id)
    .execute(pool).await.unwrap();

    // `char_start` / `char_end` are NOT NULL on `chunks`.
    sqlx::query(
        "INSERT INTO chunks(id,source_id,chunk_index,content,char_start,char_end,\
         token_count,created_at) \
         VALUES($1,$2,0,'the attachment body the user wants gone',0,38,9,0)",
    )
    .bind(format!("{source_id}-c0")).bind(source_id)
    .execute(pool).await.unwrap();
}

async fn count(pool: &echo_ai_lib::db::Db, sql: &str, id: &str) -> i64 {
    sqlx::query_scalar(sql).bind(id).fetch_one(pool).await.unwrap()
}

/// THE REGRESSION. Deleting the conversation must destroy the attachment's rows
/// AND report honestly.
#[tokio::test]
#[ignore = "DB-backed; run on the VM with NOVIQ_TEST_PG_INSTALL_DIR set"]
async fn deleting_a_conversation_actually_removes_its_attachment() {
    let (h, who) = harness().await;
    let (uid, token) = who.split_once('|').unwrap();
    let conv = format!("conv-{}", uuid::Uuid::new_v4());
    let sid = format!("src-{}", uuid::Uuid::new_v4());
    seed_attachment(&h.pool, &sid, &conv, uid).await;

    // Precondition — an assertion against an already-empty table proves nothing.
    assert_eq!(count(&h.pool, "SELECT COUNT(*) FROM sources WHERE id=$1", &sid).await, 1);
    assert_eq!(count(&h.pool, "SELECT COUNT(*) FROM chunks WHERE source_id=$1", &sid).await, 1);

    let (st, body) = send(
        &h.router, "/api/delete_conversation", token,
        json!({"conversationId": conv}),
    )
    .await;
    assert_eq!(st, StatusCode::OK, "delete_conversation: {body}");

    // The rows must be GONE — this is the assertion the bug failed.
    assert_eq!(
        count(&h.pool, "SELECT COUNT(*) FROM sources WHERE id=$1", &sid).await, 0,
        "the attachment survived the delete and is still indexed",
    );
    assert_eq!(
        count(&h.pool, "SELECT COUNT(*) FROM chunks WHERE source_id=$1", &sid).await, 0,
        "the attachment's chunks survived — it is still searchable",
    );

    // ...and the report must match reality, not the number of attempts.
    assert_eq!(body["removed"], json!(1), "removed must count actual deletions");
    assert_eq!(body["retained"], json!(0), "nothing was legitimately retained here");
}

/// The other half of the contract: content another chat still links to must
/// SURVIVE, and be reported as retained rather than removed. Without this, a fix
/// that simply deletes unconditionally would pass the test above.
#[tokio::test]
#[ignore = "DB-backed; run on the VM with NOVIQ_TEST_PG_INSTALL_DIR set"]
async fn an_attachment_shared_with_another_chat_is_retained_not_removed() {
    let (h, who) = harness().await;
    let (uid, token) = who.split_once('|').unwrap();
    let conv_a = format!("convA-{}", uuid::Uuid::new_v4());
    let conv_b = format!("convB-{}", uuid::Uuid::new_v4());
    let sid = format!("src-{}", uuid::Uuid::new_v4());
    seed_attachment(&h.pool, &sid, &conv_a, uid).await;

    // The same indexed document also attached to a second chat (migration 022).
    sqlx::query(
        "INSERT INTO source_link(source_id,conversation_id,owner_id,linked_at) \
         VALUES($1,$2,$3,0)",
    )
    .bind(&sid).bind(&conv_b).bind(uid)
    .execute(&h.pool).await.unwrap();

    let (st, body) = send(
        &h.router, "/api/delete_conversation", token,
        json!({"conversationId": conv_a}),
    )
    .await;
    assert_eq!(st, StatusCode::OK, "delete_conversation: {body}");

    assert_eq!(
        count(&h.pool, "SELECT COUNT(*) FROM sources WHERE id=$1", &sid).await, 1,
        "deleting one chat destroyed a document another chat still answers from",
    );
    assert_eq!(body["removed"], json!(0), "nothing was removed, so removed must be 0");
}

/// `delete_my_document` must state what happened. It used to answer
/// `{"ok":true}` unconditionally, which is how a retained document read as a
/// successful delete.
#[tokio::test]
#[ignore = "DB-backed; run on the VM with NOVIQ_TEST_PG_INSTALL_DIR set"]
async fn delete_my_document_reports_whether_it_actually_deleted() {
    let (h, who) = harness().await;
    let (uid, token) = who.split_once('|').unwrap();
    let conv = format!("conv-{}", uuid::Uuid::new_v4());
    let sid = format!("src-{}", uuid::Uuid::new_v4());
    seed_attachment(&h.pool, &sid, &conv, uid).await;

    // The stub notebook still references it, so this delete legitimately retains.
    let (st, body) = send(
        &h.router, "/api/delete_my_document", token,
        json!({"sourceId": sid}),
    )
    .await;
    assert_eq!(st, StatusCode::OK, "delete_my_document: {body}");
    assert_eq!(body["deleted"], json!(false), "a retained document must not report deleted");
    assert_eq!(
        count(&h.pool, "SELECT COUNT(*) FROM sources WHERE id=$1", &sid).await, 1,
        "reported retained but the row is gone",
    );

    // Drop every reference, then it must delete AND say so.
    sqlx::query("DELETE FROM notebooks WHERE id=$1")
        .bind(format!("__conv__{conv}")).execute(&h.pool).await.unwrap();
    sqlx::query("DELETE FROM source_link WHERE source_id=$1")
        .bind(&sid).execute(&h.pool).await.unwrap();

    let (st, body) = send(
        &h.router, "/api/delete_my_document", token,
        json!({"sourceId": sid}),
    )
    .await;
    assert_eq!(st, StatusCode::OK, "delete_my_document: {body}");
    assert_eq!(body["deleted"], json!(true), "an unreferenced document must delete");
    assert_eq!(
        count(&h.pool, "SELECT COUNT(*) FROM sources WHERE id=$1", &sid).await, 0,
        "reported deleted but the row survived",
    );
}
