//! F016 — engine-agnostic text-generation seam.
//!
//! The single abstraction the chat hot-path uses to produce tokens. Everything
//! upstream (retrieval, prompt building incl. no-think, grounding, citation
//! gating) is unchanged and calls **only** this trait at the generation step.
//! Two impls: `VllmGenerator` (HTTP client to the loopback vLLM sidecar) and —
//! added in the wiring stage — `LlamaCppGenerator` (wraps today's in-process
//! `ModelSlot`, the FR-017 fallback). The impl is selected once at startup from
//! `NOVIQ_INFER_ENGINE`. See specs/016-vllm-serving/contracts/text-generator-trait.md.
//!
//! NOTE: until the call-site rewiring lands (query_router/mapreduce → trait,
//! gen_gate lift, /api/health.engine), the items here are not yet referenced;
//! `dead_code` is allowed at the module level to keep `cargo check` clean.
#![allow(dead_code)]

pub mod catalog;
pub mod remote;
pub mod runtime;
pub mod vllm;

use std::sync::Arc;
use tokio_util::sync::CancellationToken;

/// Constrain the shape of the generated text. `Text` is free-form (chat); the JSON
/// variants request the engine's structured-output mode so agentic control steps
/// (F017 FR-003) get valid JSON without brittle prose parsing. Honored per engine:
/// remote chat → OpenAI `response_format`; vLLM → `response_format`/`guided_json`.
/// llama.cpp ignores it (agent runs on the HTTP engines).
#[derive(Debug, Clone, PartialEq)]
pub enum ResponseFormat {
    /// Free-form text (default chat behaviour).
    Text,
    /// Any syntactically-valid JSON object (OpenAI `{"type":"json_object"}`).
    JsonObject,
    /// JSON constrained to a named schema (guided decoding). `schema` is a JSON
    /// Schema object; the model output is guaranteed to validate against it.
    JsonSchema { name: String, schema: serde_json::Value },
}

impl Default for ResponseFormat {
    fn default() -> Self {
        ResponseFormat::Text
    }
}

/// Sampling/generation parameters carried from the current chat defaults.
#[derive(Debug, Clone)]
pub struct GenParams {
    pub max_tokens: u32,
    pub temperature: f32,
    pub top_p: f32,
    pub stop: Vec<String>,
    /// Structured-output constraint (F017). Default `Text` = unchanged chat behaviour.
    pub response_format: ResponseFormat,
}

impl Default for GenParams {
    fn default() -> Self {
        // Mirrors the deterministic chat defaults (temp ~0, ChatML stop markers).
        Self {
            max_tokens: 768,
            temperature: 0.0,
            top_p: 1.0,
            stop: vec!["<|im_end|>".to_string(), "</s>".to_string()],
            response_format: ResponseFormat::Text,
        }
    }
}

/// Result of one streaming generation. `text` is the full decoded answer (the
/// caller still applies the existing `clean_model_output` + grounding/citation
/// gate); `was_cancelled` mirrors the llama.cpp path's partial-on-cancel contract.
#[derive(Debug, Clone)]
pub struct GenOutcome {
    pub text: String,
    pub was_cancelled: bool,
}

/// Generation failure modes. The caller renders the FR-011 friendly message (and
/// may fall back to the llama.cpp engine) on `Unavailable`/`Timeout`.
#[derive(Debug, thiserror::Error)]
pub enum GenError {
    #[error("inference engine unavailable")]
    Unavailable,
    #[error("inference engine timed out")]
    Timeout,
    #[error("inference engine error: {0}")]
    Upstream(String),
}

/// Readiness + load, surfaced through `/api/health.engine` (FR-013).
#[derive(Debug, Clone, Default)]
pub struct EngineHealth {
    pub ready: bool,
    pub num_running: u32,
    pub num_waiting: u32,
}

/// One streaming generation. Implemented by the vLLM HTTP client and (wiring
/// stage) the in-process llama.cpp path.
#[async_trait::async_trait]
pub trait TextGenerator: Send + Sync {
    /// Generate from an already-built raw prompt, streaming token deltas.
    /// `on_token` is invoked per delta so the caller can emit `chat://` events.
    async fn generate_stream(
        &self,
        prompt: &str,
        params: &GenParams,
        cancel: CancellationToken,
        // Higher-ranked `for<'a>` is required: under async_trait a plain
        // `dyn FnMut(&str)` binds the arg lifetime to the trait-object lifetime,
        // forcing every token borrow to outlive the whole future (E0597).
        on_token: &mut (dyn for<'a> FnMut(&'a str) + Send),
    ) -> Result<GenOutcome, GenError>;

    /// One **non-streaming** generation — run to completion and return the full
    /// text. Used by agentic control steps (F017), which want a whole structured
    /// JSON answer, not token deltas. Default impl drains `generate_stream` with a
    /// no-op sink so every engine gets it free; engines needing different
    /// non-streaming semantics may override.
    async fn complete(
        &self,
        prompt: &str,
        params: &GenParams,
        cancel: CancellationToken,
    ) -> Result<GenOutcome, GenError> {
        let mut sink = |_: &str| {};
        self.generate_stream(prompt, params, cancel, &mut sink).await
    }

    /// Cheap readiness probe (drives `/api/health.engine`).
    async fn health(&self) -> EngineHealth;
}

/// Which engine the server generates through. Selected once at startup.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EngineKind {
    LlamaCpp,
    Vllm,
    /// Hosted OpenAI-compatible LLM API (OpenRouter/Groq/Together/… — see `remote.rs`).
    Remote,
}

impl EngineKind {
    /// True for the out-of-process **HTTP streaming** engines (vLLM sidecar,
    /// remote API) as opposed to the in-process llama.cpp `ModelSlot`. Call sites
    /// that gate behaviour on "not the GGUF slot" (concurrency permit, prompt
    /// budgeting, streaming dispatch, health probe) test this, not `== Vllm`, so
    /// the remote engine inherits the same path.
    pub fn is_http(self) -> bool {
        matches!(self, EngineKind::Vllm | EngineKind::Remote)
    }
}

/// Process-wide vLLM client, built once from env on first use. Reusing the one
/// `reqwest::Client` keeps its connection pool warm across concurrent turns.
pub fn vllm_generator() -> &'static vllm::VllmGenerator {
    static VLLM: std::sync::OnceLock<vllm::VllmGenerator> = std::sync::OnceLock::new();
    VLLM.get_or_init(|| vllm::VllmGenerator::new(vllm::VllmConfig::from_env()))
}

/// Process-wide remote-API client, built once from env. Reuses one `reqwest::Client`
/// so its TLS connection pool stays warm across concurrent turns.
pub fn remote_generator() -> &'static remote::RemoteGenerator {
    static REMOTE: std::sync::OnceLock<remote::RemoteGenerator> = std::sync::OnceLock::new();
    REMOTE.get_or_init(|| remote::RemoteGenerator::new(remote::RemoteConfig::from_env()))
}

/// The active HTTP `TextGenerator` for the selected engine (runtime-mutable via the
/// admin toggle). Returns `None` for the in-process llama.cpp engine (driven
/// directly through `ModelSlot`). Single dispatch point so call sites need not
/// match per engine. The `Arc` (not `&'static`) lets the admin swap the generator
/// at runtime when the engine/key/model changes.
pub fn http_generator() -> Option<Arc<dyn TextGenerator>> {
    runtime::http_generator()
}

/// Parse an engine name: `vllm`; `remote`/`api`; anything else (incl. unset/
/// `llamacpp`) → LlamaCpp.
pub fn parse_engine(s: &str) -> EngineKind {
    match s.trim().to_ascii_lowercase().as_str() {
        "vllm" => EngineKind::Vllm,
        "remote" | "api" => EngineKind::Remote,
        _ => EngineKind::LlamaCpp,
    }
}

/// Stable wire name for an engine (admin GET / persistence).
pub fn engine_str(e: EngineKind) -> &'static str {
    match e {
        EngineKind::Vllm => "vllm",
        EngineKind::Remote => "remote",
        EngineKind::LlamaCpp => "llamacpp",
    }
}

/// Raw env-configured engine — the seed for the runtime state, before any admin/DB
/// override. Read this ONLY from `runtime` seeding; everything else calls
/// `engine_from_env()` (which returns the runtime-active engine).
pub(crate) fn engine_from_env_raw() -> EngineKind {
    parse_engine(&std::env::var("NOVIQ_INFER_ENGINE").unwrap_or_default())
}

/// The **active** engine — runtime-mutable (admin toggle), seeded from
/// `NOVIQ_INFER_ENGINE` and optionally overridden from `db_config`. Every hot-path
/// gate (`is_http()`, dispatch, health, budgeting) reads this, so a dashboard flip
/// takes effect without a restart.
pub fn engine_from_env() -> EngineKind {
    runtime::current_engine()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn engine_defaults_to_llamacpp_when_unset_or_unknown() {
        // We can't safely mutate process env in parallel tests; assert the pure
        // mapping via a tiny local replica of the match.
        let map = |s: &str| match s.trim().to_ascii_lowercase().as_str() {
            "vllm" => EngineKind::Vllm,
            "remote" | "api" => EngineKind::Remote,
            _ => EngineKind::LlamaCpp,
        };
        assert_eq!(map("vllm"), EngineKind::Vllm);
        assert_eq!(map("VLLM"), EngineKind::Vllm);
        assert_eq!(map("remote"), EngineKind::Remote);
        assert_eq!(map("api"), EngineKind::Remote);
        assert_eq!(map(""), EngineKind::LlamaCpp);
        assert_eq!(map("llamacpp"), EngineKind::LlamaCpp);
        assert_eq!(map("something"), EngineKind::LlamaCpp);
    }

    #[test]
    fn http_engines_share_the_http_predicate() {
        assert!(EngineKind::Vllm.is_http());
        assert!(EngineKind::Remote.is_http());
        assert!(!EngineKind::LlamaCpp.is_http());
    }
}
