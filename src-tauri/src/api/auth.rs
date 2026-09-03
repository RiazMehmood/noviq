//! Authentication (US2, T016–T019, FR-004) + admin bootstrap (US7, FR-019).
//!
//! - argon2id password hashing/verify.
//! - `POST /api/login` → opaque 256-bit bearer token, stored **hashed** in
//!   `session`; `POST /api/logout` revokes; `GET /api/me` echoes identity.
//! - login rate-limit / lockout (in-memory, per username).
//! - disabled accounts (US7) are refused at login.

use std::sync::LazyLock;
use std::time::{SystemTime, UNIX_EPOCH};

use argon2::password_hash::{rand_core::OsRng, PasswordHash, PasswordHasher, PasswordVerifier, SaltString};
use argon2::Argon2;
use axum::extract::State;
use axum::http::StatusCode;
use axum::Json;
use dashmap::DashMap;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use uuid::Uuid;

use super::{ApiError, ApiResult, AppState};
use crate::db::Db;

/// Session lifetime (seconds). 7 days for a low-stakes pilot.
const SESSION_TTL_SECS: i64 = 7 * 24 * 3600;
/// Lockout: max failed logins per window before refusal.
const MAX_FAILS: u32 = 5;
const FAIL_WINDOW_SECS: i64 = 15 * 60;

fn now_secs() -> i64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs() as i64).unwrap_or(0)
}

fn sha256_hex(s: &str) -> String {
    let mut h = Sha256::new();
    h.update(s.as_bytes());
    format!("{:x}", h.finalize())
}

/// argon2id hash for storage. Public so the admin-create path (US7) reuses it.
pub fn hash_password(password: &str) -> Result<String, String> {
    let salt = SaltString::generate(&mut OsRng);
    Argon2::default()
        .hash_password(password.as_bytes(), &salt)
        .map(|h| h.to_string())
        .map_err(|e| e.to_string())
}

fn verify_password(password: &str, stored_hash: &str) -> bool {
    match PasswordHash::new(stored_hash) {
        Ok(parsed) => Argon2::default().verify_password(password.as_bytes(), &parsed).is_ok(),
        Err(_) => false,
    }
}

// ── Rate-limit / lockout (in-memory, per username) ───────────────────────────
struct FailRecord { count: u32, window_start: i64 }
static FAILS: LazyLock<DashMap<String, FailRecord>> = LazyLock::new(DashMap::new);

fn is_locked(username: &str) -> bool {
    if let Some(r) = FAILS.get(username) {
        return now_secs() - r.window_start < FAIL_WINDOW_SECS && r.count >= MAX_FAILS;
    }
    false
}

fn record_fail(username: &str) {
    let mut e = FAILS
        .entry(username.to_string())
        .or_insert(FailRecord { count: 0, window_start: now_secs() });
    if now_secs() - e.window_start >= FAIL_WINDOW_SECS {
        e.window_start = now_secs();
        e.count = 0;
    }
    e.count += 1;
}

fn clear_fails(username: &str) {
    FAILS.remove(username);
}

// ── DTOs ──────────────────────────────────────────────────────────────────────
#[derive(Deserialize)]
pub struct LoginReq { pub username: String, pub password: String }

#[derive(Serialize)]
pub struct LoginResp { pub token: String, pub expires_at: i64 }

#[derive(Serialize)]
pub struct MeResp {
    pub id: String,
    pub username: String,
    pub role: String,
    /// Whether this user may use Deep search (admin-only while it's being smoothed;
    /// opened to all via `ECHO_DEEP_AUDIENCE=all`). The single source of truth the
    /// UI reads to show/hide the toggle — the query handler enforces the same rule.
    pub deep_search_enabled: bool,
}

/// The authenticated principal, injected into request extensions by the
/// session middleware (the FR-010 identity hook).
#[derive(Clone)]
pub struct AuthUser { pub id: String, pub username: String, pub role: String }

/// Mint a bearer session for an already-authenticated user id.
///
/// ⚠️ The ONE place a session token is created. Password login and Google trial
/// sign-in both call this: a second copy of this logic would be a second place
/// to get token lifetime, hashing or storage wrong, and the two would drift.
/// The raw token is returned to the caller and never stored — only its sha256.
pub(crate) async fn issue_session(pool: &Db, user_id: &str) -> Result<(String, i64), ApiError> {
    // Opaque token (≈244 bits from two v4 uuids); only its sha256 is stored.
    let token = format!("{}{}", Uuid::new_v4().simple(), Uuid::new_v4().simple());
    let created = now_secs();
    let expires = created + SESSION_TTL_SECS;
    sqlx::query(
        "INSERT INTO session (id, user_id, token_hash, created_at, expires_at) VALUES ($1,$2,$3,$4,$5)",
    )
    .bind(Uuid::new_v4().to_string())
    .bind(user_id)
    .bind(sha256_hex(&token))
    .bind(created)
    .bind(expires)
    .execute(pool)
    .await
    .map_err(|e| ApiError::internal(e.to_string()))?;
    Ok((token, expires))
}

// ── Handlers ────────────────────────────────────────────────────────────────
pub async fn login(State(st): State<AppState>, Json(req): Json<LoginReq>) -> ApiResult<Json<LoginResp>> {
    if is_locked(&req.username) {
        return Err(ApiError::new(StatusCode::TOO_MANY_REQUESTS, "locked", "too many attempts; try later"));
    }

    let row: Option<(String, String, String, Option<i64>)> = sqlx::query_as(
        "SELECT id, password_hash, role, disabled_at FROM users WHERE username = $1",
    )
    .bind(&req.username)
    .fetch_optional(&st.pool)
    .await
    .map_err(|e| ApiError::internal(e.to_string()))?;

    let Some((id, password_hash, role, disabled_at)) = row else {
        record_fail(&req.username);
        return Err(ApiError::unauthorized("invalid credentials"));
    };
    if disabled_at.is_some() {
        return Err(ApiError::unauthorized("account disabled"));
    }
    if !verify_password(&req.password, &password_hash) {
        record_fail(&req.username);
        return Err(ApiError::unauthorized("invalid credentials"));
    }
    clear_fails(&req.username);

    let (token, expires) = issue_session(&st.pool, &id).await?;

    // Multi-user traceability (US5/T038, FR-011).
    let _ = crate::db::repositories::audit::insert_user_event(
        &st.pool, Some(&id), "login", None, None,
    )
    .await;

    let _ = role; // role surfaced via /me; not needed here
    Ok(Json(LoginResp { token, expires_at: expires }))
}

pub async fn logout(State(st): State<AppState>, user: AuthUser) -> ApiResult<StatusCode> {
    // Revoke all of this user's sessions (simple + safe for the pilot).
    sqlx::query("DELETE FROM session WHERE user_id = $1")
        .bind(&user.id)
        .execute(&st.pool)
        .await
        .map_err(|e| ApiError::internal(e.to_string()))?;
    Ok(StatusCode::NO_CONTENT)
}

pub async fn me(user: AuthUser) -> Json<MeResp> {
    let deep_search_enabled =
        crate::services::deep_search::deep_enabled_for_role(&user.role);
    Json(MeResp { id: user.id, username: user.username, role: user.role, deep_search_enabled })
}

/// Resolve a bearer token to its (id, username, role), enforcing expiry and the
/// disabled flag. Returns None if the session is invalid. Used by the middleware.
pub async fn resolve_token(pool: &Db, token: &str) -> Option<AuthUser> {
    let token_hash = sha256_hex(token);
    let (id, username, role, disabled_at, expires_at): (String, String, String, Option<i64>, i64) =
        sqlx::query_as(
            "SELECT u.id, u.username, u.role, u.disabled_at, s.expires_at \
             FROM session s JOIN users u ON u.id = s.user_id WHERE s.token_hash = $1",
        )
        .bind(&token_hash)
        .fetch_optional(pool)
        .await
        .ok()
        .flatten()?;
    if disabled_at.is_some() || expires_at < now_secs() {
        return None;
    }
    Some(AuthUser { id, username, role })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn password_round_trip() {
        let h = hash_password("s3cret-pilot!").unwrap();
        assert!(verify_password("s3cret-pilot!", &h));
        assert!(!verify_password("wrong", &h));
        assert!(h.starts_with("$argon2id$"), "should be argon2id: {h}");
    }

    #[test]
    fn lockout_after_max_fails() {
        let u = "lockout-unit-test-user";
        clear_fails(u);
        for _ in 0..MAX_FAILS {
            assert!(!is_locked(u));
            record_fail(u);
        }
        assert!(is_locked(u), "should lock after {MAX_FAILS} fails");
        clear_fails(u);
        assert!(!is_locked(u), "cleared on success");
    }
}

/// US7 (FR-019): seed a single admin from env on first boot if none exists.
pub async fn seed_admin_if_configured(pool: &Db) {
    let (Ok(user), Ok(pass)) = (std::env::var("NOVIQ_ADMIN_USER"), std::env::var("NOVIQ_ADMIN_PASS")) else {
        return;
    };
    let admin_exists: Option<(String,)> = sqlx::query_as("SELECT id FROM users WHERE role = 'admin' LIMIT 1")
        .fetch_optional(pool)
        .await
        .ok()
        .flatten();
    if admin_exists.is_some() {
        return;
    }
    let hash = match hash_password(&pass) {
        Ok(h) => h,
        Err(e) => { log::error!("admin seed: hash failed: {e}"); return; }
    };
    let res = sqlx::query(
        "INSERT INTO users (id, username, password_hash, created_at, role) VALUES ($1,$2,$3,$4,'admin') \
         ON CONFLICT (username) DO NOTHING",
    )
    .bind(Uuid::new_v4().to_string())
    .bind(&user)
    .bind(&hash)
    .bind(now_secs())
    .execute(pool)
    .await;
    match res {
        Ok(_) => log::info!("admin user '{user}' seeded from env"),
        Err(e) => log::error!("admin seed failed: {e}"),
    }
}
