//! SearXNG client — the self-hosted, credit-free web-search backend.
//!
//! SearXNG is an open-source metasearch engine running on this host
//! (`127.0.0.1:8888`, loopback only, `systemctl --user searxng`). It aggregates
//! several upstream engines and returns JSON. There is no API key, no per-query
//! credit and no vendor: the requirement was a web search that does not meter.
//!
//! Two consequences worth stating, because they are not obvious:
//!
//!  * **Snippets, not full pages.** SearXNG returns each engine's result snippet
//!    in `content`, not the extracted article body. That is deliberate here — a
//!    fetch-and-extract stage would mean this server crawling arbitrary pages,
//!    which is a much larger surface than reading one loopback JSON endpoint.
//!    Snippets are enough to decide relevance and to cite a URL the officer can
//!    open; they are NOT enough to quote as authority, which is exactly why web
//!    results never enter the grounded legal context.
//!
//!  * **Self-hosting removes the vendor, not the visibility.** The upstream
//!    engines still see the query text, from this VM's IP rather than the
//!    officer's. Self-hosting means no third party bills us or retains a
//!    per-customer query history; it does not make the query private.
//!
//! `unresponsive_engines` is surfaced in the log rather than swallowed: a
//! blocked upstream engine degrades results silently otherwise, and "the web
//! search got worse" is very hard to diagnose after the fact.

use std::time::Duration;

use async_trait::async_trait;

use super::{SearchError, WebResult, WebSearcher};

/// Where SearXNG listens. Overridable for a non-default deployment; the default
/// matches the unit installed on this host.
pub const DEFAULT_BASE_URL: &str = "http://127.0.0.1:8888";
/// A search the officer is waiting on. SearXNG fans out to several engines, so
/// this is more generous than a single-provider call would need.
const TIMEOUT_SECS: u64 = 25;

pub struct SearxngSearcher {
    base_url: String,
    client: reqwest::Client,
}

impl SearxngSearcher {
    pub fn new(base_url: impl Into<String>) -> Self {
        let mut base_url = base_url.into().trim().trim_end_matches('/').to_string();
        if base_url.is_empty() {
            base_url = DEFAULT_BASE_URL.to_string();
        }
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(TIMEOUT_SECS))
            .build()
            .unwrap_or_else(|_| reqwest::Client::new());
        Self { base_url, client }
    }

    /// Read the configured base URL (`ECHO_WEBSEARCH_URL`), falling back to the
    /// loopback default.
    pub fn from_env() -> Self {
        Self::new(std::env::var("ECHO_WEBSEARCH_URL").unwrap_or_default())
    }
}

#[async_trait]
impl WebSearcher for SearxngSearcher {
    fn name(&self) -> &'static str {
        "searxng"
    }

    async fn search(&self, query: &str, limit: usize) -> Result<Vec<WebResult>, SearchError> {
        let url = format!("{}/search", self.base_url);
        let resp = self
            .client
            .get(&url)
            .query(&[("q", query), ("format", "json")])
            .send()
            .await
            .map_err(|e| {
                // The most likely cause by far is the service being down, and
                // "connection refused" alone sends the reader hunting in the app.
                SearchError::Transport(format!(
                    "{e} (is SearXNG running? `systemctl --user status searxng`)"
                ))
            })?;

        let status = resp.status();
        if !status.is_success() {
            let detail = resp.text().await.unwrap_or_default();
            let detail = detail.chars().take(300).collect::<String>();
            // A 403 here almost always means `formats: [json]` is missing from
            // settings.yml — JSON is not enabled upstream by default.
            return Err(SearchError::Rejected(format!("HTTP {status}: {detail}")));
        }

        let parsed: SearxResponse = resp
            .json()
            .await
            .map_err(|e| SearchError::Transport(format!("malformed response: {e}")))?;

        if !parsed.unresponsive_engines.is_empty() {
            log::warn!(
                "websearch: {} upstream engine(s) did not respond: {:?}",
                parsed.unresponsive_engines.len(),
                parsed.unresponsive_engines,
            );
        }

        Ok(map_results(parsed.results, limit))
    }
}

/// Split out so the mapping rules are testable without a live SearXNG.
fn map_results(results: Vec<SearxResult>, limit: usize) -> Vec<WebResult> {
    results
        .into_iter()
        // A result with no URL cannot be cited or opened, so it is not a source.
        .filter(|r| !r.url.trim().is_empty())
        .take(limit)
        .map(|r| WebResult {
            title: if r.title.trim().is_empty() { r.url.clone() } else { r.title },
            url: r.url,
            content: r.content.unwrap_or_default(),
            score: r.score,
            // SearXNG spells it `publishedDate`; `pubdate` appears on some
            // engines. Either is useful — the age of a web source is part of
            // judging it, unlike a statute, which is dated by its own text.
            published: r.published_date.or(r.pubdate),
        })
        .collect()
}

#[derive(serde::Deserialize)]
struct SearxResponse {
    #[serde(default)]
    results: Vec<SearxResult>,
    #[serde(default)]
    unresponsive_engines: Vec<serde_json::Value>,
}

#[derive(serde::Deserialize)]
struct SearxResult {
    #[serde(default)]
    title: String,
    #[serde(default)]
    url: String,
    #[serde(default)]
    content: Option<String>,
    #[serde(default)]
    score: Option<f32>,
    #[serde(default, rename = "publishedDate")]
    published_date: Option<String>,
    #[serde(default)]
    pubdate: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn base_url_is_normalised_and_defaulted() {
        assert_eq!(SearxngSearcher::new("").base_url, DEFAULT_BASE_URL);
        assert_eq!(SearxngSearcher::new("   ").base_url, DEFAULT_BASE_URL);
        // A trailing slash would produce `//search`, which some proxies 404.
        assert_eq!(SearxngSearcher::new("http://h:8888/").base_url, "http://h:8888");
    }

    /// Mapping against the REAL response shape (keys taken from a live call to
    /// the installed instance), including the two rows that would otherwise
    /// surface as an unusable entry in the UI.
    #[test]
    fn maps_live_shape_and_drops_unusable_rows() {
        let raw = serde_json::json!({
            "results": [
                { "title": "Sindh Act No.XLII of 2013", "url": "https://pas.gov.pk/a.pdf",
                  "content": "The Sindh Local Government Bill, 2013 …",
                  "score": 2.5, "publishedDate": "2013-08-19", "engines": ["google cse"] },
                { "title": "", "url": "https://lgdsindh.gov.pk/downloads", "content": null },
                { "title": "no url", "url": "  ", "content": "dropped" }
            ],
            "unresponsive_engines": []
        });
        let parsed: SearxResponse = serde_json::from_value(raw).expect("parse");
        let out = map_results(parsed.results, 10);

        assert_eq!(out.len(), 2, "a result with no URL is not a source");
        assert_eq!(out[0].published.as_deref(), Some("2013-08-19"));
        assert_eq!(out[1].title, "https://lgdsindh.gov.pk/downloads", "blank title falls back to URL");
        assert_eq!(out[1].content, "", "null content must not panic");
    }

    /// The limit is applied AFTER dropping unusable rows, so a page of junk
    /// cannot squeeze out the real results.
    #[test]
    fn limit_applies_after_filtering() {
        let rows: Vec<SearxResult> = serde_json::from_value(serde_json::json!([
            { "url": "" }, { "url": "" },
            { "title": "a", "url": "https://x/1" },
            { "title": "b", "url": "https://x/2" }
        ]))
        .expect("parse");
        let out = map_results(rows, 2);
        assert_eq!(out.len(), 2);
        assert_eq!(out[0].url, "https://x/1");
    }
}
