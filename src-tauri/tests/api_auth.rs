//! T021 (US2) — DB-backed HTTP auth integration test.
//!
//! Proves over the real axum router + bundled PostgreSQL:
//!   - wrong password → 401; correct → token + `/me` identity,
//!   - logout revokes the session (same token then 401),
//!   - a garbage bearer token → 401,
//!   - 5 failed logins lock the account → 429.
//!
//! DB-backed, so it boots the bundled engine like `pg_spike` and is `#[ignore]`d
//! by default. Run on the GPU VM (or any box with the vendored pgvector tree):
//!   POSTGRESQL_VERSION="=17.10.0" \
//!   cargo test --features server --test api_auth -- --ignored --nocapture
#![cfg(feature = "server")]

use std::sync::Arc;

use axum::body::{to_bytes, Body};
use axum::http::{Request, StatusCode};
use serde_json::{json, Value};
use tower::ServiceExt;

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
        req.header("content-type", "application/json").body(Body::from(b.to_string())).unwrap()
    } else {
        req.body(Body::empty()).unwrap()
    };
    let resp = router.clone().oneshot(req).await.unwrap();
    let status = resp.status();
    let bytes = to_bytes(resp.into_body(), usize::MAX).await.unwrap();
    let value: Value = serde_json::from_slice(&bytes).unwrap_or(Value::Null);
    (status, value)
}

async fn login(router: &axum::Router, user: &str, pass: &str) -> (StatusCode, Value) {
    send(router, "POST", "/api/login", None, Some(json!({"username": user, "password": pass}))).await
}

#[tokio::test]
#[ignore = "boots bundled PostgreSQL; run on the VM with POSTGRESQL_VERSION set"]
async fn auth_flow_login_me_logout_lockout() {
    let tmp = tempfile::tempdir().unwrap();
    let pg_root = tmp.path().join("pg");
    let pgvector_resources = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("resources")
        .join("pgvector");
    let tier = HardwareTier::detect();

    let (_engine, pool) = db::schema::init_db(&pg_root, &pgvector_resources, &tier)
        .await
        .expect("init bundled PG");

    let hash = echo_ai_lib::api::auth::hash_password("right-pw").unwrap();
    let now = chrono::Utc::now().timestamp();
    sqlx::query("INSERT INTO users (id, username, password_hash, created_at, role) VALUES ($1,$2,$3,$4,'user')")
        .bind("u1").bind("alice").bind(&hash).bind(now)
        .execute(&pool).await.unwrap();

    let embedder = Arc::new(EmbedderState::new(&tier));
    let model_slot = Arc::new(ModelSlot::new().expect("model slot"));
    let reranker_slot = Arc::new(RerankerSlot::new(
        "models/bge-reranker-base/model.onnx",
        "models/bge-reranker-base/tokenizer.json",
    ));
    let scheduler = Scheduler::new(None);
    let gen_gate = Arc::new(tokio::sync::Semaphore::new(1));
    let state = AppState { pool: pool.clone(), tier, embedder, model_slot, reranker_slot, scheduler, gen_gate };
    let router = build_router(state);

    // Wrong password → 401.
    let (st, _) = login(&router, "alice", "wrong").await;
    assert_eq!(st, StatusCode::UNAUTHORIZED);

    // Correct → token; /me echoes identity.
    let (st, body) = login(&router, "alice", "right-pw").await;
    assert_eq!(st, StatusCode::OK, "login: {body}");
    let token = body["token"].as_str().unwrap().to_string();

    let (st, me) = send(&router, "GET", "/api/me", Some(&token), None).await;
    assert_eq!(st, StatusCode::OK);
    assert_eq!(me["username"], "alice");
    assert_eq!(me["role"], "user");

    // Garbage token → 401.
    let (st, _) = send(&router, "GET", "/api/me", Some("not-a-real-token"), None).await;
    assert_eq!(st, StatusCode::UNAUTHORIZED);

    // Logout revokes → same token now 401.
    let (st, _) = send(&router, "POST", "/api/logout", Some(&token), None).await;
    assert_eq!(st, StatusCode::NO_CONTENT);
    let (st, _) = send(&router, "GET", "/api/me", Some(&token), None).await;
    assert_eq!(st, StatusCode::UNAUTHORIZED);

    // 5 failed logins lock the account → 429 (uses a fresh username to avoid
    // interfering with the successful path above).
    sqlx::query("INSERT INTO users (id, username, password_hash, created_at, role) VALUES ($1,$2,$3,$4,'user')")
        .bind("u2").bind("bob").bind(&hash).bind(now)
        .execute(&pool).await.unwrap();
    for _ in 0..5 {
        let (st, _) = login(&router, "bob", "wrong").await;
        assert_eq!(st, StatusCode::UNAUTHORIZED);
    }
    let (st, _) = login(&router, "bob", "right-pw").await;
    assert_eq!(st, StatusCode::TOO_MANY_REQUESTS, "should lock after 5 fails");
}
