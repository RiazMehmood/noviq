//! Audit-trail endpoints (2026-08-13) — admin-only browse + export.
//!
//!   GET /api/admin/audit          one filtered page + total + facet lists
//!   GET /api/admin/audit/export   the same filter, as a CSV or JSON download
//!
//! Why this module exists: `audit_log` has been written since F014 (adds,
//! deletes, logins, admin actions) and read by **nothing** on the server. The
//! only reader was `commands/audit.rs`, a Tauri command, and Tauri is vestigial
//! in the client-server deployment — so the trail was only reachable by opening
//! psql on the VM. A trail that cannot be produced on request is not an audit
//! trail, which is the whole point of the claim it backs.
//!
//! Both routes take the SAME filter and both build it through
//! `filter_from_query`, so what an admin sees on screen and what lands in the
//! downloaded file cannot drift apart. Read-only throughout: nothing here
//! mutates `audit_log`, and there is deliberately no delete route — an audit log
//! an admin can prune on demand is worth little in an inspection.

use axum::extract::{Query, State};
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde::{Deserialize, Serialize};

use super::session::AdminUser;
use super::{ApiError, ApiResult, AppState};
use crate::db::repositories::audit;

/// Page size ceiling for the browser. The console asks for 100.
const MAX_PAGE: i64 = 500;

/// Hard ceiling on a single download. Chosen so an unfiltered export on a busy
/// deployment truncates rather than exhausting memory; the response says so
/// explicitly (`truncated` + the header row count) instead of silently cutting,
/// because a short audit file that looks complete is worse than an error.
const EXPORT_CAP: i64 = 100_000;

/// Query string shared by the browser and the export.
///
/// Dates are **calendar days** (`YYYY-MM-DD`), not timestamps: this is how an
/// officer asks for logs ("March 3rd to March 9th"), and it keeps the caller out
/// of epoch-millisecond arithmetic. `to` is inclusive of the whole named day —
/// see `day_bounds_ms`.
#[derive(Debug, Deserialize, Default)]
pub struct AuditQuery {
    /// Inclusive first day, `YYYY-MM-DD`.
    pub from: Option<String>,
    /// Inclusive last day, `YYYY-MM-DD`.
    pub to: Option<String>,
    pub user_id: Option<String>,
    /// Comma-separated action names; omitted or empty = every action.
    pub actions: Option<String>,
    /// Exact document/entity id.
    pub target: Option<String>,
    /// Free-text substring over actor, action, target, document title, details.
    pub search: Option<String>,
    pub limit: Option<i64>,
    pub offset: Option<i64>,
    /// Export only: `csv` (default) or `json`.
    pub format: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct AuditPage {
    pub total: i64,
    pub offset: i64,
    pub limit: i64,
    pub rows: Vec<audit::AuditRecord>,
    /// Every action name present in the log — drives the filter dropdown.
    pub actions: Vec<String>,
}

/// Midnight-to-midnight UTC bounds for a `YYYY-MM-DD` day, in ms epoch.
///
/// Returns `[start_of_day, start_of_NEXT_day)`. The upper bound is exclusive on
/// purpose: the alternative — an inclusive `23:59:59.999` — drops any event
/// landing in that final millisecond, which is the classic way a "complete"
/// export quietly isn't.
///
/// UTC, matching `Utc::now().timestamp_millis()` at every write site. A
/// deployment in PKT will see a day boundary at 05:00 local; that is a known,
/// consistent choice rather than a mix of zones, and the exported timestamps are
/// labelled UTC so the file is self-describing.
fn day_bounds_ms(day: &str) -> Option<(i64, i64)> {
    use chrono::{NaiveDate, TimeZone, Utc};
    let d = NaiveDate::parse_from_str(day.trim(), "%Y-%m-%d").ok()?;
    let start = Utc.from_utc_datetime(&d.and_hms_opt(0, 0, 0)?).timestamp_millis();
    let next = d.succ_opt()?;
    let end = Utc.from_utc_datetime(&next.and_hms_opt(0, 0, 0)?).timestamp_millis();
    Some((start, end))
}

/// Build the repository filter, rejecting malformed dates loudly.
///
/// A silently-ignored `from=2026-13-45` would hand back the whole log while the
/// screen still showed the date box filled in — the caller must know their
/// filter did not apply.
fn filter_from_query(q: &AuditQuery) -> Result<audit::AuditFilter, ApiError> {
    let bad = |what: &str| {
        ApiError::new(
            axum::http::StatusCode::BAD_REQUEST,
            "bad_date",
            format!("{what} must be a date in YYYY-MM-DD form"),
        )
    };

    let from_ms = match q.from.as_deref().map(str::trim).filter(|s| !s.is_empty()) {
        Some(d) => Some(day_bounds_ms(d).ok_or_else(|| bad("from"))?.0),
        None => None,
    };
    let to_ms = match q.to.as_deref().map(str::trim).filter(|s| !s.is_empty()) {
        // `.1` = start of the following day, so the named day is included whole.
        Some(d) => Some(day_bounds_ms(d).ok_or_else(|| bad("to"))?.1),
        None => None,
    };
    if let (Some(f), Some(t)) = (from_ms, to_ms) {
        if f >= t {
            return Err(ApiError::new(
                axum::http::StatusCode::BAD_REQUEST,
                "bad_range",
                "'from' must not be later than 'to'",
            ));
        }
    }

    let actions = q
        .actions
        .as_deref()
        .unwrap_or("")
        .split(',')
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
        .collect::<Vec<_>>();

    Ok(audit::AuditFilter {
        from_ms,
        to_ms,
        user_id: q.user_id.clone().filter(|s| !s.trim().is_empty()),
        actions,
        target: q.target.clone().filter(|s| !s.trim().is_empty()),
        search: q.search.clone().filter(|s| !s.trim().is_empty()),
    })
}

/// `GET /api/admin/audit` — one page of the trail, newest first.
pub async fn browse(
    State(st): State<AppState>,
    _admin: AdminUser,
    Query(q): Query<AuditQuery>,
) -> ApiResult<Json<AuditPage>> {
    let f = filter_from_query(&q)?;
    let limit = q.limit.unwrap_or(100).clamp(1, MAX_PAGE);
    let offset = q.offset.unwrap_or(0).max(0);

    let rows = audit::query(&st.pool, &f, limit, offset)
        .await
        .map_err(|e| ApiError::internal(e.to_string()))?;
    let total = audit::count(&st.pool, &f)
        .await
        .map_err(|e| ApiError::internal(e.to_string()))?;
    // Facets come from the UNFILTERED log on purpose: filtering the dropdown by
    // the current filter would let one selection hide the others and strand the
    // admin with no way back.
    let actions = audit::distinct_actions(&st.pool)
        .await
        .map_err(|e| ApiError::internal(e.to_string()))?;

    Ok(Json(AuditPage { total, offset, limit, rows, actions }))
}

/// `GET /api/admin/audit/export` — the current filter as a download.
///
/// Same filter as `browse`, no pagination: whatever the filter selects comes out
/// whole, up to `EXPORT_CAP`. Filenames carry the date range so a folder of
/// exports stays self-describing months later.
pub async fn export(
    State(st): State<AppState>,
    _admin: AdminUser,
    Query(q): Query<AuditQuery>,
) -> ApiResult<Response> {
    let f = filter_from_query(&q)?;
    let total = audit::count(&st.pool, &f)
        .await
        .map_err(|e| ApiError::internal(e.to_string()))?;
    let rows = audit::export(&st.pool, &f, EXPORT_CAP)
        .await
        .map_err(|e| ApiError::internal(e.to_string()))?;
    let truncated = total > rows.len() as i64;

    let span = match (q.from.as_deref(), q.to.as_deref()) {
        (Some(a), Some(b)) if !a.is_empty() && !b.is_empty() => format!("{a}_to_{b}"),
        (Some(a), _) if !a.is_empty() => format!("from_{a}"),
        (_, Some(b)) if !b.is_empty() => format!("until_{b}"),
        _ => "all".to_string(),
    };

    if q.format.as_deref() == Some("json") {
        let body = serde_json::json!({
            "exported_at_utc": chrono::Utc::now().to_rfc3339(),
            "row_count": rows.len(),
            "matching_total": total,
            "truncated": truncated,
            "rows": rows,
        });
        let json = serde_json::to_string_pretty(&body)
            .map_err(|e| ApiError::internal(e.to_string()))?;
        return Ok(download(json, "application/json", &format!("noviq-audit-{span}.json")));
    }

    // CSV. A leading comment line carries the same provenance the JSON envelope
    // does — a spreadsheet opened next year should state what it is and whether
    // it is complete, without needing the request that produced it.
    let mut body = format!(
        "# Noviq audit log · exported {} UTC · {} row(s) of {} matching{}\n",
        chrono::Utc::now().format("%Y-%m-%d %H:%M:%S"),
        rows.len(),
        total,
        if truncated { format!(" · TRUNCATED at {EXPORT_CAP}") } else { String::new() },
    );
    body.push_str("timestamp_utc,epoch_ms,user_id,username,action,target,document,details\n");
    for r in &rows {
        body.push_str(&format!(
            "{},{},{},{},{},{},{},{}\n",
            csv_field(&fmt_ts(r.created_at)),
            r.created_at,
            csv_field(r.user_id.as_deref().unwrap_or("")),
            csv_field(r.username.as_deref().unwrap_or("")),
            csv_field(r.action.as_deref().unwrap_or("")),
            csv_field(r.target.as_deref().unwrap_or("")),
            csv_field(r.target_name.as_deref().unwrap_or("")),
            csv_field(r.details_json.as_deref().unwrap_or("")),
        ));
    }
    Ok(download(body, "text/csv", &format!("noviq-audit-{span}.csv")))
}

fn download(body: String, mime: &str, filename: &str) -> Response {
    (
        [
            (axum::http::header::CONTENT_TYPE, mime.to_string()),
            (
                axum::http::header::CONTENT_DISPOSITION,
                format!("attachment; filename=\"{filename}\""),
            ),
        ],
        body,
    )
        .into_response()
}

/// ms epoch → `YYYY-MM-DD HH:MM:SS` UTC, for the human column of the CSV.
fn fmt_ts(ms: i64) -> String {
    chrono::DateTime::from_timestamp_millis(ms)
        .map(|d| d.format("%Y-%m-%d %H:%M:%S").to_string())
        .unwrap_or_else(|| ms.to_string())
}

/// Escape one CSV field: strip newlines, quote, double inner quotes.
///
/// Mirrors `admin::csv_field`. A leading `=`/`+`/`-`/`@` is additionally
/// prefixed with `'` so Excel treats a crafted value as text rather than a
/// formula — audit details contain user-supplied search terms, and an export is
/// exactly the file someone opens in a spreadsheet.
fn csv_field(s: &str) -> String {
    let cleaned = s.replace('\n', " ").replace('\r', " ");
    let guarded = match cleaned.chars().next() {
        Some('=') | Some('+') | Some('-') | Some('@') => format!("'{cleaned}"),
        _ => cleaned,
    };
    format!("\"{}\"", guarded.replace('"', "\"\""))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn q(from: Option<&str>, to: Option<&str>) -> AuditQuery {
        AuditQuery {
            from: from.map(str::to_string),
            to: to.map(str::to_string),
            ..Default::default()
        }
    }

    /// `ApiError` deliberately carries no `Debug` (it is a response type, not a
    /// diagnostic), so `Result::unwrap` is unavailable here — go through
    /// `Option` instead of widening the error type just to suit tests.
    fn must(q: &AuditQuery) -> audit::AuditFilter {
        filter_from_query(q).ok().expect("filter should build")
    }

    #[test]
    fn a_day_range_covers_the_whole_final_day() {
        // The bug this guards: an inclusive end-of-day bound at 23:59:59.999
        // silently drops events in the last millisecond. `to` must resolve to
        // midnight of the FOLLOWING day, compared with `<`.
        let f = must(&q(Some("2026-03-03"), Some("2026-03-03")));
        let (start, next) = day_bounds_ms("2026-03-03").expect("valid day");
        assert_eq!(f.from_ms, Some(start));
        assert_eq!(f.to_ms, Some(next));
        assert_eq!(next - start, 86_400_000, "one whole day");
    }

    #[test]
    fn a_multi_day_range_spans_both_endpoints() {
        let f = must(&q(Some("2026-03-03"), Some("2026-03-09")));
        assert_eq!(
            f.to_ms.expect("to") - f.from_ms.expect("from"),
            7 * 86_400_000,
            "3rd through 9th inclusive is seven whole days"
        );
    }

    #[test]
    fn a_malformed_date_is_rejected_not_ignored() {
        // Failing open here would hand back the entire log while the UI still
        // showed a date filter — the caller must be told it did not apply.
        assert!(filter_from_query(&q(Some("03-03-2026"), None)).is_err());
        assert!(filter_from_query(&q(None, Some("2026-13-45"))).is_err());
        assert!(filter_from_query(&q(Some(""), Some("  "))).is_ok(), "blank = no filter");
    }

    #[test]
    fn an_inverted_range_is_rejected() {
        assert!(filter_from_query(&q(Some("2026-03-09"), Some("2026-03-03"))).is_err());
    }

    #[test]
    fn actions_parse_from_a_comma_list_and_tolerate_blanks() {
        let mut a = q(None, None);
        a.actions = Some("document_open, document_denied ,,".into());
        assert_eq!(must(&a).actions, vec!["document_open", "document_denied"]);

        let mut b = q(None, None);
        b.actions = Some("  ".into());
        assert!(must(&b).actions.is_empty(), "blank = every action");
    }

    #[test]
    fn csv_fields_are_quoted_and_formula_guarded() {
        assert_eq!(csv_field("a,b"), "\"a,b\"");
        assert_eq!(csv_field("say \"hi\""), "\"say \"\"hi\"\"\"");
        assert_eq!(csv_field("line\nbreak"), "\"line break\"");
        // Spreadsheet formula injection via a logged search term.
        assert_eq!(csv_field("=cmd|'/c calc'!A1"), "\"'=cmd|'/c calc'!A1\"");
    }

    #[test]
    fn timestamps_render_as_readable_utc() {
        assert_eq!(fmt_ts(0), "1970-01-01 00:00:00");
        assert!(fmt_ts(1_772_000_000_000).starts_with("2026-"));
    }
}
