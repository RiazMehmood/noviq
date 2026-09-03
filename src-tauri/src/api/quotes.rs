//! "Request a quote" (landing page) — the public intake and its admin inbox.
//!
//!   POST /api/quotes                      PUBLIC — an organisation asks to be contacted
//!   GET  /api/admin/quotes                admin — the inbox
//!   POST /api/admin/quotes/:id/status     admin — triage one request
//!
//! ⚠️ `POST /api/quotes` is the ONLY unauthenticated write in this system.
//! Everything else lives behind `require_auth`. The consequences that shaped
//! this module:
//!
//!  * **The payload is hostile text.** It is trimmed, length-bounded, stored,
//!    listed and mailed — never interpolated into a prompt, a query or a shell.
//!  * **It is rate limited twice**: per source IP, and globally per hour. The
//!    global cap is the one that matters — an attacker has many IPs but the
//!    inbox has one owner, and a flooded inbox is a denial of service against
//!    the person who has to read it.
//!  * **Mail is best-effort and the row is authoritative.** If mail is not
//!    configured, or the provider is down, the request is still stored and still
//!    appears in the admin console. Notification failure must never lose a lead.

use std::time::{SystemTime, UNIX_EPOCH};

use axum::extract::{Path, Query, State};
use axum::http::{HeaderMap, StatusCode};
use axum::Json;
use serde::Deserialize;

use super::session::AdminUser;
use super::{ApiError, ApiResult, AppState};
use crate::db::repositories::quote_requests::{self, NewQuoteRequest};

/// Per-IP ceiling and its window. Generous: a real organisation may legitimately
/// send a second request after a typo.
const IP_LIMIT: i64 = 5;
const IP_WINDOW_MS: i64 = 60 * 60 * 1000; // 1 hour

/// Global ceiling per hour. This protects the READER, not the database.
const GLOBAL_LIMIT: usize = 60;

/// The five tracks the landing page offers. An unknown value is not an error —
/// it is stored as NULL rather than refusing an otherwise good lead because a
/// stale bundle sent an old track name.
const SECTORS: &[&str] = &["government", "academia", "industry", "developers", "policy"];

fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

#[derive(Deserialize)]
pub struct QuoteReq {
    pub organisation: Option<String>,
    #[serde(alias = "contactName")]
    pub contact_name: Option<String>,
    pub email: Option<String>,
    pub phone: Option<String>,
    pub sector: Option<String>,
    #[serde(alias = "orgSize")]
    pub org_size: Option<String>,
    pub message: Option<String>,
    /// Honeypot. A real form leaves this empty because the field is hidden from
    /// humans; a naive bot fills every input it finds. Cheap, and it costs a
    /// legitimate visitor nothing.
    #[serde(default)]
    pub website: Option<String>,
}

/// Trim, collapse an empty string to `None`, and cap the length.
fn clean(v: &Option<String>, max: usize) -> Option<String> {
    let t = v.as_deref()?.trim();
    if t.is_empty() {
        return None;
    }
    Some(t.chars().take(max).collect())
}

/// Deliberately permissive. This is a contact form, not an authentication
/// boundary: rejecting a valid unusual address costs a customer, while accepting
/// an invalid one costs a bounced email.
fn looks_like_email(s: &str) -> bool {
    let mut parts = s.split('@');
    let (local, domain) = match (parts.next(), parts.next(), parts.next()) {
        (Some(l), Some(d), None) => (l, d),
        _ => return false,
    };
    !local.is_empty()
        && domain.contains('.')
        && !domain.starts_with('.')
        && !domain.ends_with('.')
        && !s.chars().any(char::is_whitespace)
}

/// Best-effort client address.
///
/// ⚠️ Both headers are attacker-controlled unless a trusted proxy overwrites
/// them. noviq.pk sits behind a Cloudflare tunnel, which does set
/// `CF-Connecting-IP`, so that header is trustworthy in THIS deployment and
/// `X-Forwarded-For` is only a fallback for other topologies. Treat the per-IP
/// limit as a courtesy that stops accidents and naive scripts — the global cap
/// is the control that actually holds.
fn client_ip(headers: &HeaderMap) -> Option<String> {
    headers
        .get("cf-connecting-ip")
        .or_else(|| headers.get("x-forwarded-for"))
        .and_then(|v| v.to_str().ok())
        .map(|v| v.split(',').next().unwrap_or(v).trim().to_string())
        .filter(|v| !v.is_empty())
}

/// POST /api/quotes — public.
pub async fn submit(
    State(st): State<AppState>,
    headers: HeaderMap,
    Json(req): Json<QuoteReq>,
) -> ApiResult<Json<serde_json::Value>> {
    // Honeypot: answer 200 so a bot learns nothing from the response, but store
    // nothing. Never surfaced as an error — a human never trips it.
    if clean(&req.website, 200).is_some() {
        return Ok(Json(serde_json::json!({ "ok": true })));
    }

    let organisation = clean(&req.organisation, 200)
        .ok_or_else(|| ApiError::bad_request("organisation is required"))?;
    let email = clean(&req.email, 254).ok_or_else(|| ApiError::bad_request("email is required"))?;
    if !looks_like_email(&email) {
        return Err(ApiError::bad_request("that does not look like an email address"));
    }

    let sector = clean(&req.sector, 40)
        .map(|s| s.to_lowercase())
        .filter(|s| SECTORS.contains(&s.as_str()));

    let ip = client_ip(&headers);
    let since = now_ms() - IP_WINDOW_MS;

    if let Some(ip) = ip.as_deref() {
        let n = quote_requests::count_from_ip_since(&st.pool, ip, since)
            .await
            .map_err(|e| ApiError::internal(e.to_string()))?;
        if n >= IP_LIMIT {
            return Err(ApiError::new(
                StatusCode::TOO_MANY_REQUESTS,
                "rate_limited",
                "You have already sent us a few requests. We will be in touch — \
                 or write to admin@noviq.pk directly.",
            ));
        }
    }

    // Global hourly cap, counted over every row in the window regardless of IP.
    let recent_total = quote_requests::list(&st.pool, None, 500)
        .await
        .map_err(|e| ApiError::internal(e.to_string()))?
        .iter()
        .filter(|r| r.created_at >= since)
        .count();
    if recent_total >= GLOBAL_LIMIT {
        return Err(ApiError::new(
            StatusCode::TOO_MANY_REQUESTS,
            "rate_limited",
            "We are receiving an unusual number of requests. Please write to admin@noviq.pk.",
        ));
    }

    let contact_name = clean(&req.contact_name, 120);
    let phone = clean(&req.phone, 40);
    let org_size = clean(&req.org_size, 40);
    let message = clean(&req.message, 4000);

    let id = quote_requests::insert(
        &st.pool,
        NewQuoteRequest {
            organisation: &organisation,
            contact_name: contact_name.as_deref(),
            email: &email,
            phone: phone.as_deref(),
            sector: sector.as_deref(),
            org_size: org_size.as_deref(),
            message: message.as_deref(),
            source_ip: ip.as_deref(),
        },
    )
    .await
    .map_err(|e| ApiError::internal(e.to_string()))?;

    // Notification is fire-and-forget: the visitor must not wait on a
    // third-party API, and a mail failure must not fail a stored request.
    {
        let pool = st.pool.clone();
        let id2 = id.clone();
        let summary = QuoteSummary {
            organisation,
            contact_name,
            email,
            phone,
            sector,
            org_size,
            message,
        };
        tokio::spawn(async move {
            match notify_admin(&summary).await {
                Ok(true) => {
                    if let Err(e) = quote_requests::mark_emailed(&pool, &id2).await {
                        log::warn!("quote {id2}: stored but could not stamp emailed_at: {e}");
                    }
                }
                // Not configured — normal, and not a problem: the inbox has it.
                Ok(false) => {}
                Err(e) => log::warn!("quote {id2}: notification email failed: {e}"),
            }
        });
    }

    Ok(Json(serde_json::json!({ "ok": true, "id": id })))
}

struct QuoteSummary {
    organisation: String,
    contact_name: Option<String>,
    email: String,
    phone: Option<String>,
    sector: Option<String>,
    org_size: Option<String>,
    message: Option<String>,
}

/// Mail the admin, if and only if mail is configured.
///
/// Uses ZeptoMail's HTTPS API rather than SMTP because the server already
/// carries `reqwest` and carries no SMTP client — adding one for a single
/// notification is not worth a new dependency in this build.
///
/// Configure on the VM (all four, or none):
///   NOVIQ_MAIL_API_URL    https://api.zeptomail.com/v1.1/email
///   NOVIQ_MAIL_API_TOKEN  Zoho-enczapikey <key>      ⚠️ secret — never in git
///   NOVIQ_MAIL_FROM       noreply@noviq.pk           (a verified sender)
///   NOVIQ_MAIL_TO         admin@noviq.pk
///
/// Returns `Ok(false)` when unconfigured — the common case, and not an error.
async fn notify_admin(q: &QuoteSummary) -> anyhow::Result<bool> {
    let (url, token, from, to) = match (
        std::env::var("NOVIQ_MAIL_API_URL").ok(),
        std::env::var("NOVIQ_MAIL_API_TOKEN").ok(),
        std::env::var("NOVIQ_MAIL_FROM").ok(),
        std::env::var("NOVIQ_MAIL_TO").ok(),
    ) {
        (Some(u), Some(t), Some(f), Some(r))
            if !u.is_empty() && !t.is_empty() && !f.is_empty() && !r.is_empty() =>
        {
            (u, t, f, r)
        }
        _ => return Ok(false),
    };

    fn field(label: &str, v: Option<&str>) -> String {
        match v {
            Some(v) => format!("{label}: {v}\n"),
            None => String::new(),
        }
    }
    let body = format!(
        "New quote request from the Noviq landing page.\n\n\
         Organisation: {}\nEmail: {}\n{}{}{}{}\n{}\n\n\
         Open the Admin console -> Inbox to reply or triage.",
        q.organisation,
        q.email,
        field("Contact", q.contact_name.as_deref()),
        field("Phone", q.phone.as_deref()),
        field("Sector", q.sector.as_deref()),
        field("Size", q.org_size.as_deref()),
        q.message.as_deref().unwrap_or("(no message)"),
    );

    let payload = serde_json::json!({
        "from": { "address": from },
        "to": [ { "email_address": { "address": to } } ],
        "subject": format!("Noviq — quote request from {}", q.organisation),
        "textbody": body,
    });

    let resp = reqwest::Client::new()
        .post(&url)
        .header("Authorization", token)
        .header("Content-Type", "application/json")
        .json(&payload)
        .send()
        .await?;
    if !resp.status().is_success() {
        anyhow::bail!("mail provider returned {}", resp.status());
    }
    Ok(true)
}

// ── Admin inbox ─────────────────────────────────────────────────────────────

#[derive(Deserialize)]
pub struct ListParams {
    pub status: Option<String>,
    pub limit: Option<i64>,
}

/// GET /api/admin/quotes?status=&limit=
pub async fn list(
    State(st): State<AppState>,
    _admin: AdminUser,
    Query(p): Query<ListParams>,
) -> ApiResult<Json<serde_json::Value>> {
    let limit = p.limit.unwrap_or(200).clamp(1, 500);
    let status = p.status.as_deref().filter(|s| !s.is_empty());
    let rows = quote_requests::list(&st.pool, status, limit)
        .await
        .map_err(|e| ApiError::internal(e.to_string()))?;
    let unread = quote_requests::count_new(&st.pool)
        .await
        .map_err(|e| ApiError::internal(e.to_string()))?;
    Ok(Json(serde_json::json!({ "requests": rows, "unread": unread })))
}

#[derive(Deserialize)]
pub struct StatusReq {
    pub status: String,
    pub note: Option<String>,
}

/// POST /api/admin/quotes/:id/status
pub async fn set_status(
    State(st): State<AppState>,
    _admin: AdminUser,
    Path(id): Path<String>,
    Json(req): Json<StatusReq>,
) -> ApiResult<Json<serde_json::Value>> {
    const ALLOWED: &[&str] = &["new", "read", "replied", "archived"];
    if !ALLOWED.contains(&req.status.as_str()) {
        return Err(ApiError::bad_request(format!(
            "invalid status '{}' (expected one of {ALLOWED:?})",
            req.status
        )));
    }
    let note = clean(&req.note, 4000);
    let found = quote_requests::set_status(&st.pool, &id, &req.status, note.as_deref())
        .await
        .map_err(|e| ApiError::internal(e.to_string()))?;
    if !found {
        return Err(ApiError::bad_request("no such request"));
    }
    Ok(Json(serde_json::json!({ "ok": true })))
}
