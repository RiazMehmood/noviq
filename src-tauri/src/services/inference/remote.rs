//! Remote OpenAI-compatible LLM API generator.
//!
//! A second HTTP `TextGenerator` (beside `VllmGenerator`) that targets a *hosted*
//! OpenAI-compatible endpoint — OpenRouter, Groq, Together, Fireworks, DeepInfra,
//! DeepSeek, or any gateway that speaks `/v1/chat/completions` (+ optionally
//! `/v1/completions`). Selected with `NOVIQ_INFER_ENGINE=remote` (or `api`).
//!
//! Design mirrors `vllm.rs`: raw-byte SSE line buffering (a chunk boundary can't
//! split a multibyte char because every complete line ends in `\n`), per-delta
//! `on_token`, cancel via `tokio::select!`. Two differences from the loopback
//! vLLM path:
//!   1. **Auth** — a `NOVIQ_REMOTE_API_KEY` bearer token (from `.env`, never
//!      hardcoded).
//!   2. **Endpoint shape** — most free hosted APIs expose only
//!      `/v1/chat/completions` (`messages` array + `choices[].delta.content`),
//!      not raw `/v1/completions`. Default is `chat`; operators whose provider
//!      supports text completion can set `NOVIQ_REMOTE_ENDPOINT=completions` to
//!      send the hand-built ChatML prompt byte-identically (like the vLLM path).
//!
//! In `chat` mode the hand-built ChatML prompt is parsed back into role/content
//! messages so the provider's own chat template wraps it correctly (double
//! templating would leak `<|im_start|>` literals into the model's context).

use std::time::Duration;

use futures_util::StreamExt;
use tokio_util::sync::CancellationToken;

use super::{EngineHealth, GenError, GenOutcome, GenParams, ResponseFormat, TextGenerator};

/// Which OpenAI-compatible route to call.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Endpoint {
    /// `/v1/chat/completions` — `messages` array; deltas at `choices[].delta.content`.
    Chat,
    /// `/v1/completions` — raw `prompt`; deltas at `choices[].text` (like vLLM).
    Completions,
}

impl Endpoint {
    fn from_env() -> Self {
        match std::env::var("NOVIQ_REMOTE_ENDPOINT")
            .unwrap_or_default()
            .trim()
            .to_ascii_lowercase()
            .as_str()
        {
            "completions" | "completion" | "text" => Endpoint::Completions,
            _ => Endpoint::Chat,
        }
    }

    /// Route *relative to the API prefix* (see `RemoteConfig::api_prefix`). The
    /// version segment (`/v1`) is NOT baked in here — it comes from the prefix —
    /// because not every provider nests under `/v1` (Google's Gemini OpenAI-compat
    /// layer is `/v1beta/openai/chat/completions`, no `/v1`).
    fn route(self) -> &'static str {
        match self {
            Endpoint::Chat => "/chat/completions",
            Endpoint::Completions => "/completions",
        }
    }

    /// Stable wire name (admin GET / persistence).
    pub fn as_str(self) -> &'static str {
        match self {
            Endpoint::Chat => "chat",
            Endpoint::Completions => "completions",
        }
    }

    /// Parse a stored/admin value; anything unrecognised → `Chat` (broadest support).
    pub fn from_str_or_default(s: &str) -> Self {
        match s.trim().to_ascii_lowercase().as_str() {
            "completions" | "completion" | "text" => Endpoint::Completions,
            _ => Endpoint::Chat,
        }
    }
}

/// Normalise an API path prefix: trim, drop a trailing `/`, and ensure a leading
/// `/` when non-empty. Empty stays empty (Gemini's base URL already carries its
/// full path). `"v1"` → `"/v1"`, `"/v1/"` → `"/v1"`, `""` → `""`.
pub fn normalize_prefix(s: &str) -> String {
    let t = s.trim().trim_end_matches('/');
    if t.is_empty() {
        String::new()
    } else if t.starts_with('/') {
        t.to_string()
    } else {
        format!("/{t}")
    }
}

/// Connection config, resolved from `NOVIQ_REMOTE_*` env.
#[derive(Debug, Clone)]
pub struct RemoteConfig {
    /// Host root up to (but excluding) the API prefix, e.g. `https://openrouter.ai/api`,
    /// `https://api.groq.com/openai`, `https://generativelanguage.googleapis.com/v1beta/openai`.
    pub base_url: String,
    pub model: String,
    /// Bearer token; `None` sends no `Authorization` header (some local gateways).
    pub api_key: Option<String>,
    pub timeout: Duration,
    pub endpoint: Endpoint,
    /// Path segment between `base_url` and the route (`/chat/completions`), default
    /// `/v1`. Providers that nest under `/v1` (Groq, OpenRouter, Together) keep the
    /// default; Google's Gemini OpenAI-compat layer sets this **empty** because its
    /// base URL is already `…/v1beta/openai`. Requests go to
    /// `{base_url}{api_prefix}{route}`; the health probe to `{base_url}{api_prefix}/models`.
    pub api_prefix: String,
    /// Optional `HTTP-Referer` / `X-Title` (OpenRouter attribution; ignored elsewhere).
    pub referer: Option<String>,
    pub title: Option<String>,
}

impl RemoteConfig {
    pub fn from_env() -> Self {
        // Defaults target Groq (the provider in use) so a first-time admin who
        // only pastes the API key gets a working config. Override per-provider via
        // NOVIQ_REMOTE_URL / NOVIQ_REMOTE_MODEL (or the admin dashboard fields).
        let base_url = std::env::var("NOVIQ_REMOTE_URL")
            .unwrap_or_else(|_| "https://api.groq.com/openai".to_string());
        let model = std::env::var("NOVIQ_REMOTE_MODEL")
            .unwrap_or_else(|_| "llama-3.1-8b-instant".to_string());
        let api_key = std::env::var("NOVIQ_REMOTE_API_KEY")
            .ok()
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty());
        let timeout_ms = std::env::var("NOVIQ_REMOTE_TIMEOUT_MS")
            .ok()
            .and_then(|v| v.trim().parse::<u64>().ok())
            .unwrap_or(60_000);
        // Default `/v1` keeps every currently-configured provider working; set
        // `NOVIQ_REMOTE_PREFIX=` (empty) for Gemini's `…/v1beta/openai` base.
        let api_prefix = std::env::var("NOVIQ_REMOTE_PREFIX")
            .map(|s| normalize_prefix(&s))
            .unwrap_or_else(|_| "/v1".to_string());
        let referer = std::env::var("NOVIQ_REMOTE_REFERER").ok().filter(|s| !s.is_empty());
        let title = std::env::var("NOVIQ_REMOTE_TITLE").ok().filter(|s| !s.is_empty());
        Self {
            base_url: base_url.trim_end_matches('/').to_string(),
            model,
            api_key,
            timeout: Duration::from_millis(timeout_ms),
            endpoint: Endpoint::from_env(),
            api_prefix,
            referer,
            title,
        }
    }
}

/// One chat message parsed out of a hand-built ChatML prompt.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
struct ChatMessage {
    role: String,
    content: String,
}

/// Parse a hand-built ChatML prompt (`<|im_start|>role\n…<|im_end|>`) into
/// role/content messages for `/v1/chat/completions`.
///
/// - The Qwen `NOVIQ_NO_THINK` prefill (`<think>\n\n</think>\n\n`) is stripped —
///   hosted models never emit reasoning tags and the literals would leak.
/// - A trailing `<|im_start|>assistant\n` with no closing `<|im_end|>` is the
///   *generation cue*: it's dropped (the API generates the assistant turn), not
///   sent as an empty assistant message (some providers reject a trailing
///   assistant turn).
fn chatml_to_messages(prompt: &str) -> Vec<ChatMessage> {
    const OPEN: &str = "<|im_start|>";
    const END: &str = "<|im_end|>";
    let mut out = Vec::new();
    let mut rest = prompt;
    while let Some(i) = rest.find(OPEN) {
        rest = &rest[i + OPEN.len()..];
        // role is up to the first newline.
        let Some(nl) = rest.find('\n') else { break };
        let role = rest[..nl].trim().to_string();
        let body = &rest[nl + 1..];
        match body.find(END) {
            Some(j) => {
                let content = body[..j].replace("<think>\n\n</think>\n\n", "");
                if !role.is_empty() {
                    out.push(ChatMessage { role, content: content.trim_end().to_string() });
                }
                rest = &body[j + END.len()..];
            }
            None => {
                // Unterminated final block = the assistant generation cue. If it
                // carries real content (rare) keep it; if it's just the cue (or a
                // no-think prefill), drop it so the API generates the turn.
                let content = body.replace("<think>\n\n</think>\n\n", "");
                let content = content.trim();
                if role != "assistant" && !content.is_empty() {
                    out.push(ChatMessage { role, content: content.to_string() });
                }
                break;
            }
        }
    }
    // Robustness: a caller that passes a plain prompt with no ChatML markers (e.g.
    // an ad-hoc control call) would otherwise yield an empty `messages` array, which
    // chat APIs reject. Fall back to one user message carrying the whole prompt.
    if out.is_empty() {
        let content = prompt.replace("<think>\n\n</think>\n\n", "").trim().to_string();
        if !content.is_empty() {
            out.push(ChatMessage { role: "user".to_string(), content });
        }
    }
    out
}

/// Map a `ResponseFormat` onto the OpenAI `response_format` JSON (or `None` for
/// free-form text). `strict:true` on schema mode makes conforming providers
/// hard-constrain the output.
fn response_format_json(rf: &ResponseFormat) -> Option<serde_json::Value> {
    match rf {
        ResponseFormat::Text => None,
        ResponseFormat::JsonObject => Some(serde_json::json!({ "type": "json_object" })),
        ResponseFormat::JsonSchema { name, schema } => Some(serde_json::json!({
            "type": "json_schema",
            "json_schema": { "name": name, "schema": schema, "strict": true },
        })),
    }
}

pub struct RemoteGenerator {
    client: reqwest::Client,
    cfg: RemoteConfig,
}

impl RemoteGenerator {
    pub fn new(cfg: RemoteConfig) -> Self {
        let client = reqwest::Client::builder()
            .build()
            .unwrap_or_else(|_| reqwest::Client::new());
        Self { client, cfg }
    }

    fn request_body(&self, prompt: &str, params: &GenParams) -> serde_json::Value {
        let mut body = match self.cfg.endpoint {
            Endpoint::Chat => serde_json::json!({
                "model": self.cfg.model,
                "messages": chatml_to_messages(prompt),
                "max_tokens": params.max_tokens,
                "temperature": params.temperature,
                "top_p": params.top_p,
                "stop": params.stop,
                "stream": true,
            }),
            Endpoint::Completions => serde_json::json!({
                "model": self.cfg.model,
                "prompt": prompt,
                "max_tokens": params.max_tokens,
                "temperature": params.temperature,
                "top_p": params.top_p,
                "stop": params.stop,
                "stream": true,
            }),
        };
        // Structured-output mode (F017): attach `response_format` when requested so
        // agentic control calls get valid JSON. No-op for `Text`.
        if let Some(rf) = response_format_json(&params.response_format) {
            body["response_format"] = rf;
        }
        body
    }

    fn apply_headers(&self, mut rb: reqwest::RequestBuilder) -> reqwest::RequestBuilder {
        if let Some(key) = &self.cfg.api_key {
            rb = rb.bearer_auth(key);
        }
        if let Some(r) = &self.cfg.referer {
            rb = rb.header("HTTP-Referer", r);
        }
        if let Some(t) = &self.cfg.title {
            rb = rb.header("X-Title", t);
        }
        rb
    }
}

#[async_trait::async_trait]
impl TextGenerator for RemoteGenerator {
    async fn generate_stream(
        &self,
        prompt: &str,
        params: &GenParams,
        cancel: CancellationToken,
        on_token: &mut (dyn for<'a> FnMut(&'a str) + Send),
    ) -> Result<GenOutcome, GenError> {
        let body = self.request_body(prompt, params);
        let url = format!("{}{}{}", self.cfg.base_url, self.cfg.api_prefix, self.cfg.endpoint.route());
        let rb = self.apply_headers(self.client.post(&url).json(&body));

        let resp = rb
            .send()
            .await
            .map_err(|e| if e.is_timeout() { GenError::Timeout } else { GenError::Unavailable })?;

        if !resp.status().is_success() {
            let status = resp.status();
            // Body often carries the provider's error reason (bad key, no credit,
            // model not found) — surface it truncated to aid diagnosis.
            let detail = resp.text().await.unwrap_or_default();
            let detail: String = detail.chars().take(300).collect();
            return Err(GenError::Upstream(format!("HTTP {status}: {detail}")));
        }

        let endpoint = self.cfg.endpoint;
        let mut stream = resp.bytes_stream();
        let mut bytebuf: Vec<u8> = Vec::new();
        let mut out = String::new();

        loop {
            let chunk = tokio::select! {
                _ = cancel.cancelled() => {
                    return Ok(GenOutcome { text: out, was_cancelled: true });
                }
                r = tokio::time::timeout(self.cfg.timeout, stream.next()) => match r {
                    Err(_) => return Err(GenError::Timeout),
                    Ok(None) => break, // stream ended without an explicit [DONE]
                    Ok(Some(Ok(bytes))) => bytes,
                    Ok(Some(Err(e))) => return Err(GenError::Upstream(e.to_string())),
                }
            };

            bytebuf.extend_from_slice(&chunk);
            if let Some(nl) = bytebuf.iter().rposition(|&b| b == b'\n') {
                let region = String::from_utf8_lossy(&bytebuf[..=nl]).into_owned();
                bytebuf.drain(..=nl);
                for ev in parse_sse(&region, endpoint) {
                    match ev {
                        SseEvent::Done => {
                            return Ok(GenOutcome { text: out, was_cancelled: false })
                        }
                        SseEvent::Delta(d) => {
                            if !d.is_empty() {
                                on_token(&d);
                                out.push_str(&d);
                            }
                        }
                    }
                }
            }
        }

        Ok(GenOutcome { text: out, was_cancelled: false })
    }

    async fn health(&self) -> EngineHealth {
        // Remote gateways have no cheap universal readiness probe; a `GET /v1/models`
        // both liveness-checks and validates the key. Queue depth is provider-side
        // and not exposed → 0/0.
        let url = format!("{}{}/models", self.cfg.base_url, self.cfg.api_prefix);
        let ready = self
            .apply_headers(self.client.get(&url))
            .timeout(Duration::from_secs(4))
            .send()
            .await
            .map(|r| r.status().is_success())
            .unwrap_or(false);
        EngineHealth { ready, num_running: 0, num_waiting: 0 }
    }
}

/// A parsed SSE record.
#[derive(Debug, PartialEq, Eq)]
enum SseEvent {
    Delta(String),
    Done,
}

/// Parse complete `data:` lines out of `region` (must contain only whole lines).
/// Delta location depends on the endpoint: chat → `choices[0].delta.content`,
/// completions → `choices[0].text`. Pure + unit-testable without a network.
fn parse_sse(region: &str, endpoint: Endpoint) -> Vec<SseEvent> {
    let mut events = Vec::new();
    for line in region.lines() {
        let Some(payload) = line.trim().strip_prefix("data:") else {
            continue;
        };
        let payload = payload.trim();
        if payload.is_empty() {
            continue;
        }
        if payload == "[DONE]" {
            events.push(SseEvent::Done);
            continue;
        }
        let Ok(v) = serde_json::from_str::<serde_json::Value>(payload) else {
            continue;
        };
        let choice = v.get("choices").and_then(|c| c.get(0));
        let text = match endpoint {
            Endpoint::Chat => choice
                .and_then(|c| c.get("delta"))
                .and_then(|d| d.get("content"))
                .and_then(|t| t.as_str()),
            Endpoint::Completions => {
                choice.and_then(|c| c.get("text")).and_then(|t| t.as_str())
            }
        };
        if let Some(t) = text {
            events.push(SseEvent::Delta(t.to_string()));
        }
    }
    events
}

#[cfg(test)]
mod tests {
    use super::*;

    fn params() -> GenParams {
        GenParams::default()
    }

    #[test]
    fn chatml_parses_system_user_and_drops_gen_cue() {
        let chatml = "<|im_start|>system\nYou are a legal assistant. Cite [n].<|im_end|>\n\
<|im_start|>user\nContext:\n[1] rules.pdf\nQuestion: Who?<|im_end|>\n\
<|im_start|>assistant\n<think>\n\n</think>\n\n";
        let msgs = chatml_to_messages(chatml);
        assert_eq!(msgs.len(), 2, "gen cue must not become an empty assistant msg: {msgs:?}");
        assert_eq!(msgs[0].role, "system");
        assert_eq!(msgs[0].content, "You are a legal assistant. Cite [n].");
        assert_eq!(msgs[1].role, "user");
        assert!(msgs[1].content.starts_with("Context:"));
    }

    #[test]
    fn chatml_preserves_multi_turn_history() {
        let chatml = "<|im_start|>system\nsys<|im_end|>\n\
<|im_start|>user\nq1<|im_end|>\n<|im_start|>assistant\na1<|im_end|>\n\
<|im_start|>user\nq2<|im_end|>\n<|im_start|>assistant\n";
        let msgs = chatml_to_messages(chatml);
        let roles: Vec<&str> = msgs.iter().map(|m| m.role.as_str()).collect();
        assert_eq!(roles, vec!["system", "user", "assistant", "user"]);
        assert_eq!(msgs[2].content, "a1");
    }

    #[test]
    fn chat_body_uses_messages_not_prompt() {
        let g = RemoteGenerator::new(RemoteConfig {
            base_url: "https://x".into(),
            model: "m".into(),
            api_key: None,
            timeout: Duration::from_secs(1),
            endpoint: Endpoint::Chat,
            api_prefix: "/v1".into(),
            referer: None,
            title: None,
        });
        let body = g.request_body("<|im_start|>user\nhi<|im_end|>\n<|im_start|>assistant\n", &params());
        assert!(body.get("messages").is_some());
        assert!(body.get("prompt").is_none());
        assert_eq!(body["stream"], true);
    }

    #[test]
    fn completions_body_sends_raw_prompt() {
        let g = RemoteGenerator::new(RemoteConfig {
            base_url: "https://x".into(),
            model: "m".into(),
            api_key: None,
            timeout: Duration::from_secs(1),
            endpoint: Endpoint::Completions,
            api_prefix: "/v1".into(),
            referer: None,
            title: None,
        });
        let body = g.request_body("RAW PROMPT", &params());
        assert_eq!(body["prompt"], "RAW PROMPT");
        assert!(body.get("messages").is_none());
    }

    #[test]
    fn parse_chat_deltas_then_done() {
        let region = "data: {\"choices\":[{\"delta\":{\"content\":\"The\"}}]}\n\
data: {\"choices\":[{\"delta\":{\"content\":\" Secretary\"}}]}\n\
data: [DONE]\n";
        let ev = parse_sse(region, Endpoint::Chat);
        assert_eq!(
            ev,
            vec![
                SseEvent::Delta("The".into()),
                SseEvent::Delta(" Secretary".into()),
                SseEvent::Done,
            ]
        );
    }

    #[test]
    fn parse_completions_deltas() {
        let region = "data: {\"choices\":[{\"text\":\"A\"}]}\ndata: {\"choices\":[{\"text\":\"B\"}]}\n";
        let ev = parse_sse(region, Endpoint::Completions);
        assert_eq!(ev, vec![SseEvent::Delta("A".into()), SseEvent::Delta("B".into())]);
    }

    #[test]
    fn chat_roleless_delta_and_keepalive_ignored() {
        // First chat delta often carries only {"role":"assistant"} with no content.
        let region = ": keep-alive\n\ndata: {\"choices\":[{\"delta\":{\"role\":\"assistant\"}}]}\n\
data: {\"choices\":[{\"delta\":{\"content\":\"x\"}}]}\n";
        let ev = parse_sse(region, Endpoint::Chat);
        assert_eq!(ev, vec![SseEvent::Delta("x".into())]);
    }

    #[test]
    fn json_object_attaches_response_format() {
        let g = RemoteGenerator::new(RemoteConfig {
            base_url: "https://x".into(),
            model: "m".into(),
            api_key: None,
            timeout: Duration::from_secs(1),
            endpoint: Endpoint::Chat,
            api_prefix: "/v1".into(),
            referer: None,
            title: None,
        });
        let mut p = params();
        p.response_format = ResponseFormat::JsonObject;
        let body = g.request_body("<|im_start|>user\nhi<|im_end|>\n", &p);
        assert_eq!(body["response_format"]["type"], "json_object");
    }

    #[test]
    fn json_schema_attaches_named_schema() {
        let g = RemoteGenerator::new(RemoteConfig {
            base_url: "https://x".into(),
            model: "m".into(),
            api_key: None,
            timeout: Duration::from_secs(1),
            endpoint: Endpoint::Chat,
            api_prefix: "/v1".into(),
            referer: None,
            title: None,
        });
        let mut p = params();
        p.response_format = ResponseFormat::JsonSchema {
            name: "action".into(),
            schema: serde_json::json!({"type":"object"}),
        };
        let body = g.request_body("<|im_start|>user\nhi<|im_end|>\n", &p);
        assert_eq!(body["response_format"]["type"], "json_schema");
        assert_eq!(body["response_format"]["json_schema"]["name"], "action");
        assert_eq!(body["response_format"]["json_schema"]["strict"], true);
    }

    #[test]
    fn text_format_omits_response_format() {
        let g = RemoteGenerator::new(RemoteConfig {
            base_url: "https://x".into(),
            model: "m".into(),
            api_key: None,
            timeout: Duration::from_secs(1),
            endpoint: Endpoint::Chat,
            api_prefix: "/v1".into(),
            referer: None,
            title: None,
        });
        let body = g.request_body("<|im_start|>user\nhi<|im_end|>\n", &params());
        assert!(body.get("response_format").is_none());
    }

    #[test]
    fn plain_prompt_falls_back_to_single_user_message() {
        let msgs = chatml_to_messages("just a plain question with no markers");
        assert_eq!(msgs.len(), 1);
        assert_eq!(msgs[0].role, "user");
        assert_eq!(msgs[0].content, "just a plain question with no markers");
    }

    #[test]
    fn endpoint_routes_have_no_version_prefix() {
        // The `/v1` (or none) comes from `RemoteConfig::api_prefix`, not the route.
        assert_eq!(Endpoint::Chat.route(), "/chat/completions");
        assert_eq!(Endpoint::Completions.route(), "/completions");
    }

    #[test]
    fn normalize_prefix_covers_provider_shapes() {
        assert_eq!(normalize_prefix("/v1"), "/v1"); // Groq/OpenRouter default
        assert_eq!(normalize_prefix("v1"), "/v1");
        assert_eq!(normalize_prefix("/v1/"), "/v1");
        assert_eq!(normalize_prefix(""), ""); // Gemini (base already …/v1beta/openai)
        assert_eq!(normalize_prefix("  "), "");
    }

    #[test]
    fn gemini_url_has_no_double_version_segment() {
        // Gemini: base carries `/v1beta/openai`, prefix empty → correct route.
        let g = RemoteGenerator::new(RemoteConfig {
            base_url: "https://generativelanguage.googleapis.com/v1beta/openai".into(),
            model: "gemma-4-12b-it".into(),
            api_key: Some("k".into()),
            timeout: Duration::from_secs(1),
            endpoint: Endpoint::Chat,
            api_prefix: normalize_prefix(""),
            referer: None,
            title: None,
        });
        let url = format!("{}{}{}", g.cfg.base_url, g.cfg.api_prefix, g.cfg.endpoint.route());
        assert_eq!(
            url,
            "https://generativelanguage.googleapis.com/v1beta/openai/chat/completions"
        );
    }

    /// Live smoke test against a real OpenAI-compatible provider. `#[ignore]` — it
    /// needs network + `NOVIQ_REMOTE_*` env (URL/MODEL/API_KEY), so it never runs
    /// in normal/offline builds. Run explicitly:
    ///   NOVIQ_REMOTE_URL=https://api.groq.com/openai NOVIQ_REMOTE_MODEL=llama-3.1-8b-instant \
    ///   NOVIQ_REMOTE_API_KEY=... cargo test -- --ignored live_provider_stream --nocapture
    #[tokio::test]
    #[ignore]
    async fn live_provider_stream() {
        let g = RemoteGenerator::new(RemoteConfig::from_env());
        let p = GenParams { max_tokens: 40, ..GenParams::default() };
        let prompt = "<|im_start|>user\nReply with exactly: hello from the remote engine<|im_end|>\n<|im_start|>assistant\n";
        let mut streamed = String::new();
        let mut on = |t: &str| streamed.push_str(t);
        let out = g
            .generate_stream(prompt, &p, CancellationToken::new(), &mut on)
            .await
            .expect("remote provider stream should succeed");
        eprintln!("REMOTE_ANSWER: {:?}", out.text);
        assert!(!out.text.trim().is_empty(), "provider returned empty text");
        assert!(!streamed.is_empty(), "no tokens were streamed to on_token");
    }
}
