//! The free trial: Google sign-in, and the limits that make it a trial.
//!
//!   POST /api/auth/google   PUBLIC — exchange a Google ID token for a session
//!   GET  /api/trial/status  the signed-in trial user's remaining quota
//!
//! ─── Why the ID token is verified at Google rather than locally ────────────
//! Verifying an RS256 JWT here means fetching Google's JWKS, caching it, picking
//! the key by `kid`, and checking the signature — four places to get crypto
//! subtly wrong, in a crate we would have to add. Google's `tokeninfo` endpoint
//! does that validation and returns the claims. It costs one round trip per
//! sign-in, which at trial volumes is free.
//!
//! ⚠️ tokeninfo validates the SIGNATURE and EXPIRY. It does NOT decide the token
//! was minted for us — every Google ID token in the world comes back "valid".
//! The `aud` and `iss` checks below are what make this authentication rather
//! than a way for anyone with any Google token to become a Noviq trial user.

use axum::extract::State;
use axum::http::StatusCode;
use axum::Json;
use serde::Deserialize;

use super::auth::{issue_session, AuthUser};
use super::{ApiError, ApiResult, AppState};
use crate::db::repositories::trial;

/// Set on the VM. Absent ⇒ Google sign-in is OFF and the endpoint refuses
/// cleanly rather than accepting unverifiable tokens.
const CLIENT_ID_ENV: &str = "NOVIQ_GOOGLE_CLIENT_ID";

const TOKENINFO: &str = "https://oauth2.googleapis.com/tokeninfo";

/// The claims we consume. Google returns many more; these are the ones that
/// carry identity, and `aud`/`iss` are the ones that carry trust.
#[derive(Deserialize)]
struct TokenInfo {
    /// Stable subject id — the identity key.
    sub: String,
    /// Audience: MUST equal our client id.
    aud: String,
    /// Issuer: MUST be Google.
    iss: String,
    email: Option<String>,
    email_verified: Option<String>,
    name: Option<String>,
}

#[derive(Deserialize)]
pub struct GoogleReq {
    /// The credential returned by Google Identity Services in the browser.
    #[serde(alias = "credential", alias = "idToken", alias = "id_token")]
    pub token: String,
}

/// POST /api/auth/google
pub async fn google_signin(
    State(st): State<AppState>,
    Json(req): Json<GoogleReq>,
) -> ApiResult<Json<serde_json::Value>> {
    let client_id = std::env::var(CLIENT_ID_ENV).unwrap_or_default();
    if client_id.is_empty() {
        return Err(ApiError::new(
            StatusCode::SERVICE_UNAVAILABLE,
            "google_disabled",
            "Google sign-in is not configured on this deployment.",
        ));
    }
    if req.token.trim().is_empty() {
        return Err(ApiError::bad_request("missing credential"));
    }

    // ⚠️ The token goes in the QUERY STRING because that is the interface Google
    // publishes for this endpoint. It is a short-lived, single-audience token
    // and never touches our logs — do not log `url`.
    let url = format!("{TOKENINFO}?id_token={}", urlencoding_minimal(&req.token));
    let resp = reqwest::Client::new()
        .get(&url)
        .send()
        .await
        .map_err(|e| ApiError::internal(format!("could not reach Google: {e}")))?;
    if !resp.status().is_success() {
        return Err(ApiError::unauthorized("that Google sign-in could not be verified"));
    }
    let info: TokenInfo = resp
        .json()
        .await
        .map_err(|e| ApiError::internal(format!("unreadable response from Google: {e}")))?;

    // ── The three checks that make this authentication ──────────────────────
    // 1. Minted for US. Without this, a token issued to ANY Google app would be
    //    accepted here and its holder would become a Noviq user.
    if info.aud != client_id {
        return Err(ApiError::unauthorized("that sign-in was not issued for Noviq"));
    }
    // 2. Issued by Google.
    if info.iss != "accounts.google.com" && info.iss != "https://accounts.google.com" {
        return Err(ApiError::unauthorized("unexpected token issuer"));
    }
    // 3. A verified address. An unverified one can be anybody's.
    let email = info.email.unwrap_or_default();
    let verified = info.email_verified.as_deref() == Some("true");
    if email.is_empty() || !verified {
        return Err(ApiError::unauthorized(
            "your Google account has no verified email address",
        ));
    }

    // Find or create. `sub` is the key, never the email.
    let existing = trial::find_by_google_sub(&st.pool, &info.sub)
        .await
        .map_err(|e| ApiError::internal(e.to_string()))?;
    let user_id = match existing {
        Some(id) => id,
        None => trial::create(&st.pool, &info.sub, &email, info.name.as_deref())
            .await
            .map_err(|e| ApiError::internal(e.to_string()))?,
    };

    // A revoked or expired trial gets no session at all — refusing at sign-in is
    // clearer than handing out a token that fails on the first question.
    let status = trial::status(&st.pool, &user_id)
        .await
        .map_err(|e| ApiError::internal(e.to_string()))?;
    if let Some(s) = &status {
        if s.revoked {
            return Err(ApiError::unauthorized("this trial has been ended."));
        }
        if s.expired {
            return Err(ApiError::new(
                StatusCode::FORBIDDEN,
                "trial_expired",
                "Your 3-day trial has finished. Request a quote and we will set you up properly.",
            ));
        }
    }

    let (token, expires) = issue_session(&st.pool, &user_id).await?;
    let _ = crate::db::repositories::audit::insert_user_event(
        &st.pool,
        Some(&user_id),
        "trial_login",
        None,
        None,
    )
    .await;

    Ok(Json(serde_json::json!({
        "token": token,
        "expires_at": expires,
        "role": "trial",
        "trial": status,
    })))
}

/// GET /api/trial/status — what the banner in the UI reads.
///
/// Returns `null` for anyone who is not a trial user, so the frontend can ask
/// unconditionally without branching on role first.
pub async fn status(State(st): State<AppState>, user: AuthUser) -> ApiResult<Json<serde_json::Value>> {
    if user.role != "trial" {
        return Ok(Json(serde_json::json!({ "trial": null })));
    }
    let s = trial::status(&st.pool, &user.id)
        .await
        .map_err(|e| ApiError::internal(e.to_string()))?;
    Ok(Json(serde_json::json!({ "trial": s })))
}

// ── Admin: who has tried the product ────────────────────────────────────────

/// GET /api/admin/trials — everyone who has started a trial, newest first.
///
/// ⚠️ This is a list of real people's names and email addresses. It exists so
/// the team can follow up about Noviq, which the privacy policy states plainly.
/// It must not be exported to, or joined with, anything else without saying so
/// there first.
pub async fn list_trials(
    State(st): State<AppState>,
    _admin: super::session::AdminUser,
) -> ApiResult<Json<serde_json::Value>> {
    let rows = trial::list(&st.pool, 500)
        .await
        .map_err(|e| ApiError::internal(e.to_string()))?;
    Ok(Json(serde_json::json!({ "trials": rows })))
}

/// POST /api/admin/trials/:id/revoke — end one trial now.
///
/// Revokes the sessions too: leaving a live bearer token behind would let the
/// holder keep asking until it expired, which is not what "revoke" means.
pub async fn revoke_trial(
    State(st): State<AppState>,
    _admin: super::session::AdminUser,
    axum::extract::Path(user_id): axum::extract::Path<String>,
) -> ApiResult<Json<serde_json::Value>> {
    let found = trial::revoke(&st.pool, &user_id)
        .await
        .map_err(|e| ApiError::internal(e.to_string()))?;
    sqlx::query("DELETE FROM session WHERE user_id = $1")
        .bind(&user_id)
        .execute(&st.pool)
        .await
        .map_err(|e| ApiError::internal(e.to_string()))?;
    Ok(Json(serde_json::json!({ "ok": found })))
}

/// Percent-encode the few characters that matter in a JWT-bearing query string.
///
/// A JWT is base64url + '.', all of which are already URL-safe; this exists so
/// a malformed input cannot break out of the query parameter. Hand-rolled to
/// avoid adding a dependency for one call site.
fn urlencoding_minimal(s: &str) -> String {
    s.chars()
        .map(|c| match c {
            'A'..='Z' | 'a'..='z' | '0'..='9' | '-' | '_' | '.' | '~' => c.to_string(),
            _ => format!("%{:02X}", c as u32 as u8),
        })
        .collect()
}
