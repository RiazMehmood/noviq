//! T048 (US7, SC-010) — admin authorization + disabled-login integration test.
//!
//! Proves, end-to-end over the real axum router + bundled PostgreSQL:
//!   - every `/api/admin/*` endpoint 403s a non-admin session,
//!   - an admin can create a tester (gets a one-time temp password) and the
//!     tester can then log in,
//!   - disabling the tester revokes access: their next login is rejected (401).
//!
//! DB-backed, so it boots the bundled engine like `pg_spike` and is `#[ignore]`d
//! by default. Run on the GPU VM (or any box with the vendored pgvector tree):
//!   POSTGRESQL_VERSION="=17.10.0" \
//!   cargo test --features server --test api_admin -- --ignored --nocapture
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

/// Send one request through a fresh clone of the router; return (status, json).
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

async fn login(router: &axum::Router, username: &str, password: &str) -> (StatusCode, Value) {
    send(router, "POST", "/api/login", None, Some(json!({"username": username, "password": password}))).await
}

#[tokio::test]
#[ignore = "boots bundled PostgreSQL; run on the VM with POSTGRESQL_VERSION set"]
async fn admin_authz_and_disabled_login() {
    let tmp = tempfile::tempdir().unwrap();
    let pg_root = tmp.path().join("pg");
    let pgvector_resources = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("resources")
        .join("pgvector");
    let tier = HardwareTier::detect();

    let (_engine, pool) = db::schema::init_db(&pg_root, &pgvector_resources, &tier)
        .await
        .expect("init bundled PG");

    // Seed an admin and a plain user directly (passwords known to the test).
    let admin_hash = echo_ai_lib::api::auth::hash_password("admin-pw").unwrap();
    let user_hash = echo_ai_lib::api::auth::hash_password("user-pw").unwrap();
    let now = chrono::Utc::now().timestamp();
    sqlx::query("INSERT INTO users (id, username, password_hash, created_at, role) VALUES ($1,$2,$3,$4,'admin')")
        .bind("admin-id").bind("rootadmin").bind(&admin_hash).bind(now)
        .execute(&pool).await.unwrap();
    sqlx::query("INSERT INTO users (id, username, password_hash, created_at, role) VALUES ($1,$2,$3,$4,'user')")
        .bind("user-id").bind("plainuser").bind(&user_hash).bind(now)
        .execute(&pool).await.unwrap();

    // Build the real router with the same singletons the bin constructs.
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

    // ── Log in both principals ────────────────────────────────────────────────
    let (st, body) = login(&router, "rootadmin", "admin-pw").await;
    assert_eq!(st, StatusCode::OK, "admin login: {body}");
    let admin_token = body["token"].as_str().unwrap().to_string();

    let (st, body) = login(&router, "plainuser", "user-pw").await;
    assert_eq!(st, StatusCode::OK, "user login: {body}");
    let user_token = body["token"].as_str().unwrap().to_string();

    // ── Non-admin → 403 on every admin endpoint ───────────────────────────────
    let (st, _) = send(&router, "GET", "/api/admin/users", Some(&user_token), None).await;
    assert_eq!(st, StatusCode::FORBIDDEN, "non-admin must not list users");
    let (st, _) = send(&router, "POST", "/api/admin/users", Some(&user_token),
        Some(json!({"username": "x"}))).await;
    assert_eq!(st, StatusCode::FORBIDDEN, "non-admin must not create users");
    let (st, _) = send(&router, "POST", "/api/admin/users/user-id/disable", Some(&user_token), None).await;
    assert_eq!(st, StatusCode::FORBIDDEN, "non-admin must not disable users");

    // ── Missing token → 401 (auth middleware) ─────────────────────────────────
    let (st, _) = send(&router, "GET", "/api/admin/users", None, None).await;
    assert_eq!(st, StatusCode::UNAUTHORIZED);

    // ── Admin lists, then creates a tester ────────────────────────────────────
    let (st, body) = send(&router, "GET", "/api/admin/users", Some(&admin_token), None).await;
    assert_eq!(st, StatusCode::OK, "admin list: {body}");
    assert!(body.as_array().unwrap().len() >= 2);

    let (st, body) = send(&router, "POST", "/api/admin/users", Some(&admin_token),
        Some(json!({"username": "tester1", "role": "user"}))).await;
    assert_eq!(st, StatusCode::OK, "create: {body}");
    let temp_pw = body["temp_password"].as_str().unwrap().to_string();
    let tester_id = body["id"].as_str().unwrap().to_string();

    // Tester can log in with the temp password.
    let (st, _) = login(&router, "tester1", &temp_pw).await;
    assert_eq!(st, StatusCode::OK, "tester first login");

    // ── Admin disables tester → next login rejected (SC-010) ──────────────────
    let (st, _) = send(&router, "POST", &format!("/api/admin/users/{tester_id}/disable"),
        Some(&admin_token), None).await;
    assert_eq!(st, StatusCode::NO_CONTENT);

    let (st, body) = login(&router, "tester1", &temp_pw).await;
    assert_eq!(st, StatusCode::UNAUTHORIZED, "disabled user must be refused: {body}");

    // ── Re-enable restores access ─────────────────────────────────────────────
    let (st, _) = send(&router, "POST", &format!("/api/admin/users/{tester_id}/enable"),
        Some(&admin_token), None).await;
    assert_eq!(st, StatusCode::NO_CONTENT);
    let (st, _) = login(&router, "tester1", &temp_pw).await;
    assert_eq!(st, StatusCode::OK, "re-enabled user logs in again");
}
