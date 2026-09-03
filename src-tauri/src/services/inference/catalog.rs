//! Provider / model catalogue for the admin API-integration tab.
//!
//! The tab needs more than a list of model ids: it has to tell an operator whether
//! a model is free or metered, whether the provider trains on submitted data,
//! whether the context window is large enough for a Noviq RAG prompt, and whether
//! the model has a published shutdown date. That is *data*, not code — model ids
//! have roughly a 12-month half-life (see `docs/llm-provider-api-analysis-2026-07.pdf`
//! §6.3; fourteen widely-used ids retire between Jul and Dec 2026 alone), so a
//! catalogue compiled into the frontend bundle would be partly dead within a year.
//!
//! Resolution order:
//!   1. `{NOVIQ_DATA_DIR}/provider_catalog.json` — operator override, if present
//!      and parseable. Lets the VM get a refreshed catalogue by dropping a file,
//!      with no frontend rebuild and no server release.
//!   2. The bundled `catalog.json` (compiled in via `include_str!`), so the app
//!      always works offline and a corrupt override can never brick the tab.
//!
//! A malformed override is logged and ignored rather than propagated — losing the
//! provider list would take the whole inference tab down, which is a far worse
//! outcome than serving a slightly stale catalogue.
//!
//! Nothing here is authoritative about whether a key *works*: published limits
//! drift and free tiers are repriced without notice. `POST /api/admin/inference/validate`
//! probes the live endpoint, and that probe — not this file — is what the UI
//! should be trusted on.

use std::sync::OnceLock;

/// Bundled default catalogue, shipped with the binary.
const BUNDLED: &str = include_str!("catalog.json");

/// One selectable model within a provider. Every field beyond `id`/`tier` is
/// optional: the catalogue is a *best-effort* description of a moving target, and
/// a missing price is honestly rendered as "—" rather than guessed at.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct CatalogModel {
    pub id: String,
    /// `free` | `paid`. Drives the tag shown next to the model in the picker.
    pub tier: String,
    /// Maximum context window in tokens. Used to warn when a model cannot hold a
    /// typical Noviq RAG prompt (see `CONTEXT_WARN_TOKENS`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub context: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub input_price: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub output_price: Option<f64>,
    /// Free-tier requests/minute, tokens/minute and requests/day where published.
    /// `tpm` is the one that actually binds RAG traffic — see `effective_rpm`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rpm: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tpm: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rpd: Option<u32>,
    /// Published retirement date (`YYYY-MM-DD`). Drives the "retiring" warning.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub shutdown_date: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
}

/// One provider: connection shape plus the commercial terms an operator must see
/// *before* pasting a key, not buried in documentation.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct CatalogProvider {
    pub id: String,
    pub label: String,
    pub base_url: String,
    /// Path segment between `base_url` and the route. `/v1` for most; empty for
    /// Google's Gemini OpenAI-compat layer, whose base already carries its path.
    pub prefix: String,
    #[serde(default = "default_endpoint")]
    pub endpoint: String,
    #[serde(default)]
    pub key_placeholder: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub console_url: Option<String>,
    /// Plain-language description of the free allowance, shown under the picker.
    #[serde(default)]
    pub free_tier: String,
    /// True when the provider trains on submitted data on the tier a first-time
    /// operator will land on. Surfaced as a prominent warning tag: for a
    /// non-public corpus this is a gating criterion, not a preference.
    #[serde(default)]
    pub trains_on_data: bool,
    /// True when the free tier forbids commercial use (Cohere-style). Such a tier
    /// cannot legally back a customer deployment regardless of its limits.
    #[serde(default)]
    pub non_commercial_only: bool,
    #[serde(default)]
    pub notes: String,
    #[serde(default)]
    pub models: Vec<CatalogModel>,
}

fn default_endpoint() -> String {
    "chat".to_string()
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct Catalog {
    pub version: String,
    #[serde(default)]
    pub note: String,
    pub providers: Vec<CatalogProvider>,
}

/// Below this, a model cannot hold a typical Noviq RAG prompt (~8K tokens of
/// retrieved passages plus the system/grounding prefix) and will silently
/// truncate retrieved context — which is precisely how ungrounded answers get
/// produced. Models under this ceiling are tagged rather than hidden, so the
/// operator sees *why* they are a bad fit.
pub const CONTEXT_WARN_TOKENS: u32 = 16_000;

/// Path of the operator override, if one is configured. Mirrors the
/// `NOVIQ_DATA_DIR` convention used by the other server-side caches.
fn override_path() -> std::path::PathBuf {
    let data_dir = std::env::var("NOVIQ_DATA_DIR").unwrap_or_else(|_| ".noviq-data".to_string());
    std::path::PathBuf::from(data_dir).join("provider_catalog.json")
}

fn parse(src: &str) -> anyhow::Result<Catalog> {
    Ok(serde_json::from_str::<Catalog>(src)?)
}

/// The active catalogue: operator override if it parses, else the bundled default.
/// Resolved once per process — the override is read at startup, so refreshing it
/// takes a server restart (the same lifecycle as every other bundled resource).
pub fn catalog() -> &'static Catalog {
    static C: OnceLock<Catalog> = OnceLock::new();
    C.get_or_init(|| {
        let path = override_path();
        if path.is_file() {
            match std::fs::read_to_string(&path).map_err(anyhow::Error::from).and_then(|s| parse(&s)) {
                Ok(c) => {
                    log::info!(
                        "catalog: using operator override {} (version {}, {} providers)",
                        path.display(),
                        c.version,
                        c.providers.len()
                    );
                    return c;
                }
                // Deliberately non-fatal: a bad override must not empty the
                // provider picker and take the inference tab down with it.
                Err(e) => log::warn!(
                    "catalog: ignoring unreadable override {}: {e} — falling back to the bundled catalogue",
                    path.display()
                ),
            }
        }
        parse(BUNDLED).expect("bundled catalog.json must parse — it is compiled in and covered by tests")
    })
}

/// Look up a provider by id.
pub fn provider(id: &str) -> Option<&'static CatalogProvider> {
    catalog().providers.iter().find(|p| p.id == id)
}

/// Sustainable requests/minute at a given prompt size, given a tokens-per-minute
/// cap — the number an operator actually experiences.
///
/// Free tiers advertise requests/day, which flatters them badly for RAG: our
/// traffic is token-heavy per request, so TPM binds long before RPD does. Groq's
/// headline 14,400 requests/day looks like ~10/minute of headroom, but at 6,000
/// TPM and an 8K-token prompt the real sustained rate is about 0.75/minute. The
/// UI shows this derived figure instead of the headline, so users do not hit 429s
/// they were told could not happen.
///
/// Returns `None` when there is no TPM cap to derive from (paid tiers, or a
/// provider that does not publish one), in which case `rpm` stands unmodified.
pub fn effective_rpm(tpm: Option<u32>, rpm: Option<u32>, prompt_tokens: u32) -> Option<f64> {
    let tpm = tpm? as f64;
    let prompt = prompt_tokens.max(1) as f64;
    let from_tokens = tpm / prompt;
    // The true ceiling is whichever cap binds first.
    Some(match rpm {
        Some(r) => from_tokens.min(r as f64),
        None => from_tokens,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bundled_catalog_parses() {
        let c = parse(BUNDLED).expect("bundled catalog must parse");
        assert!(!c.providers.is_empty());
        assert!(!c.version.is_empty());
    }

    #[test]
    fn bundled_catalog_is_internally_consistent() {
        let c = parse(BUNDLED).unwrap();
        let mut seen = std::collections::HashSet::new();
        for p in &c.providers {
            assert!(seen.insert(p.id.clone()), "duplicate provider id: {}", p.id);
            assert!(!p.label.is_empty(), "{} has no label", p.id);
            // Custom is the only provider allowed an empty base URL (the operator
            // supplies it); everything else must be dialable as shipped.
            if p.id != "custom" {
                assert!(p.base_url.starts_with("https://"), "{} base_url must be https", p.id);
                assert!(!p.models.is_empty(), "{} ships no models", p.id);
            }
            let mut model_ids = std::collections::HashSet::new();
            for m in &p.models {
                assert!(model_ids.insert(m.id.clone()), "duplicate model {} in {}", m.id, p.id);
                assert!(
                    m.tier == "free" || m.tier == "paid",
                    "{}/{} has invalid tier {:?}",
                    p.id, m.id, m.tier
                );
                if let Some(d) = &m.shutdown_date {
                    assert_eq!(d.len(), 10, "{}/{} shutdown_date must be YYYY-MM-DD", p.id, m.id);
                }
            }
        }
    }

    /// Every provider the frontend previously hardcoded must survive the move to
    /// the catalogue, or saved configs stop resolving to a known provider.
    #[test]
    fn catalog_covers_the_previously_hardcoded_providers() {
        let c = parse(BUNDLED).unwrap();
        for id in ["groq", "google", "openrouter", "custom"] {
            assert!(c.providers.iter().any(|p| p.id == id), "missing provider {id}");
        }
    }

    /// The stored base URLs must still match what the old frontend wrote, so an
    /// existing saved config maps back onto the right provider on load.
    #[test]
    fn base_urls_match_the_previously_saved_values() {
        let c = parse(BUNDLED).unwrap();
        let find = |id: &str| c.providers.iter().find(|p| p.id == id).unwrap();
        assert_eq!(find("groq").base_url, "https://api.groq.com/openai");
        assert_eq!(find("groq").prefix, "/v1");
        assert_eq!(
            find("google").base_url,
            "https://generativelanguage.googleapis.com/v1beta/openai"
        );
        // Gemini's base already carries `/v1beta/openai`; a `/v1` here would
        // produce a double version segment and 404 every request.
        assert_eq!(find("google").prefix, "");
        assert_eq!(find("openrouter").base_url, "https://openrouter.ai/api");
    }

    #[test]
    fn effective_rpm_reports_the_binding_cap() {
        // Groq's real shape: 30 RPM / 6,000 TPM. At an 8K RAG prompt the token
        // cap binds far below the request cap.
        let e = effective_rpm(Some(6_000), Some(30), 8_000).unwrap();
        assert!((e - 0.75).abs() < 1e-9, "expected 0.75 rpm, got {e}");
        // With a tiny prompt the request cap binds instead — never report more
        // than the provider's own RPM.
        assert_eq!(effective_rpm(Some(6_000), Some(30), 10).unwrap(), 30.0);
        // No TPM published → nothing to derive.
        assert!(effective_rpm(None, Some(30), 8_000).is_none());
    }

    #[test]
    fn effective_rpm_does_not_divide_by_zero() {
        assert!(effective_rpm(Some(6_000), None, 0).unwrap().is_finite());
    }

    /// Models that cannot hold a RAG prompt must be *tagged*, not silently listed
    /// as equivalent — this is the Cerebras/gemma2 trap from the analysis.
    #[test]
    fn context_limited_models_are_detectable() {
        let c = parse(BUNDLED).unwrap();
        let cerebras = c.providers.iter().find(|p| p.id == "cerebras").unwrap();
        let free = cerebras.models.iter().find(|m| m.tier == "free").unwrap();
        assert!(
            free.context.unwrap() < CONTEXT_WARN_TOKENS,
            "cerebras free tier is context-limited and must be flagged as such"
        );
    }
}
