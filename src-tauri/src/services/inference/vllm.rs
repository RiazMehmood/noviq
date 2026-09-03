//! F016 — `VllmGenerator`: HTTP client to the loopback vLLM OpenAI-compatible
//! sidecar. Uses raw `/v1/completions` (so the hand-built ChatML + no-think
//! prompt is sent byte-identical — no server-side chat-template re-render),
//! `stream=true`, mapping SSE `data:` deltas onto `on_token`. See
//! specs/016-vllm-serving/contracts/vllm-openai-api.md.

use std::time::Duration;

use futures_util::StreamExt;
use tokio_util::sync::CancellationToken;

use super::{EngineHealth, GenError, GenOutcome, GenParams, ResponseFormat, TextGenerator};

/// Turn-marker dialect of the raw prompt sent to `/v1/completions`. All upstream
/// prompt builders emit ChatML (Qwen); when the served model is a Gemma the
/// operator sets `NOVIQ_VLLM_PROMPT_FORMAT=gemma` and this seam rewrites the
/// markers — one choke point instead of touching every builder.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PromptFormat {
    ChatMl,
    Gemma,
}

impl PromptFormat {
    fn from_env() -> Self {
        match std::env::var("NOVIQ_VLLM_PROMPT_FORMAT")
            .unwrap_or_default()
            .trim()
            .to_ascii_lowercase()
            .as_str()
        {
            "gemma" => PromptFormat::Gemma,
            _ => PromptFormat::ChatMl,
        }
    }
}

/// Connection config, resolved from `NOVIQ_VLLM_*` env (data-model.md).
#[derive(Debug, Clone)]
pub struct VllmConfig {
    pub base_url: String,
    pub model: String,
    pub timeout: Duration,
    pub prompt_format: PromptFormat,
}

impl VllmConfig {
    pub fn from_env() -> Self {
        let base_url = std::env::var("NOVIQ_VLLM_URL")
            .unwrap_or_else(|_| "http://127.0.0.1:8000".to_string());
        let model = std::env::var("NOVIQ_VLLM_MODEL")
            .unwrap_or_else(|_| "Qwen/Qwen3-8B-AWQ".to_string());
        let timeout_ms = std::env::var("NOVIQ_VLLM_TIMEOUT_MS")
            .ok()
            .and_then(|v| v.trim().parse::<u64>().ok())
            .unwrap_or(60_000);
        Self {
            base_url: base_url.trim_end_matches('/').to_string(),
            model,
            timeout: Duration::from_millis(timeout_ms),
            prompt_format: PromptFormat::from_env(),
        }
    }
}

/// Rewrite a hand-built ChatML prompt into Gemma 3's turn format. Gemma has no
/// system role — its official chat template prepends the system text to the
/// first user message, so the leading system block is folded into the first
/// user turn. Qwen3's `NOVIQ_NO_THINK` prefill block is dropped: Gemma never
/// emits reasoning tags, and the stray literals would leak into its context.
fn chatml_to_gemma(prompt: &str) -> String {
    const SYS_OPEN: &str = "<|im_start|>system\n";
    const END_NL: &str = "<|im_end|>\n";
    let (sys, rest) = match prompt.strip_prefix(SYS_OPEN) {
        Some(r) => match r.find(END_NL) {
            Some(i) => (&r[..i], &r[i + END_NL.len()..]),
            None => ("", prompt),
        },
        None => ("", prompt),
    };
    let s = rest
        .replace("<|im_start|>user\n", "<start_of_turn>user\n")
        .replace("<|im_start|>assistant\n", "<start_of_turn>model\n")
        // No mid-prompt system blocks exist today; fold any future one to user.
        .replace("<|im_start|>system\n", "<start_of_turn>user\n")
        .replace("<|im_end|>", "<end_of_turn>")
        .replace("<think>\n\n</think>\n\n", "");
    if sys.is_empty() {
        s
    } else if let Some(tail) = s.strip_prefix("<start_of_turn>user\n") {
        format!("<start_of_turn>user\n{sys}\n\n{tail}")
    } else {
        format!("<start_of_turn>user\n{sys}<end_of_turn>\n{s}")
    }
}

/// Map the ChatML stop markers onto Gemma's end-of-turn token so vLLM trims the
/// answer server-side (the Rust output scrubbers only know ChatML markers).
fn stops_for_gemma(stop: &[String]) -> Vec<String> {
    let mut out: Vec<String> = stop
        .iter()
        .map(|s| {
            if s == "<|im_end|>" {
                "<end_of_turn>".to_string()
            } else {
                s.clone()
            }
        })
        .collect();
    if !out.iter().any(|s| s == "<end_of_turn>") {
        out.push("<end_of_turn>".to_string());
    }
    out
}

pub struct VllmGenerator {
    client: reqwest::Client,
    cfg: VllmConfig,
}

impl VllmGenerator {
    pub fn new(cfg: VllmConfig) -> Self {
        let client = reqwest::Client::builder()
            .build()
            .unwrap_or_else(|_| reqwest::Client::new());
        Self { client, cfg }
    }

    /// Best-effort queue depth from vLLM's Prometheus `/metrics`
    /// (`vllm:num_requests_running` / `vllm:num_requests_waiting`). Returns
    /// `(0, 0)` if metrics are unavailable or unparseable.
    async fn fetch_queue(&self) -> (u32, u32) {
        let url = format!("{}/metrics", self.cfg.base_url);
        let body = match self
            .client
            .get(&url)
            .timeout(Duration::from_secs(2))
            .send()
            .await
        {
            Ok(r) if r.status().is_success() => r.text().await.unwrap_or_default(),
            _ => return (0, 0),
        };
        (
            parse_metric(&body, "vllm:num_requests_running").unwrap_or(0.0) as u32,
            parse_metric(&body, "vllm:num_requests_waiting").unwrap_or(0.0) as u32,
        )
    }
}

#[async_trait::async_trait]
impl TextGenerator for VllmGenerator {
    async fn generate_stream(
        &self,
        prompt: &str,
        params: &GenParams,
        cancel: CancellationToken,
        on_token: &mut (dyn for<'a> FnMut(&'a str) + Send),
    ) -> Result<GenOutcome, GenError> {
        let (prompt, stop) = match self.cfg.prompt_format {
            PromptFormat::ChatMl => (prompt.to_string(), params.stop.clone()),
            PromptFormat::Gemma => (chatml_to_gemma(prompt), stops_for_gemma(&params.stop)),
        };
        let mut body = serde_json::json!({
            "model": self.cfg.model,
            "prompt": prompt,
            "max_tokens": params.max_tokens,
            "temperature": params.temperature,
            "top_p": params.top_p,
            "stop": stop,
            "stream": true,
        });
        // Structured-output mode (F017): vLLM's OpenAI server accepts
        // `response_format:{type:json_object}` and the `guided_json` extra for a
        // schema (guided decoding). No-op for `Text`.
        match &params.response_format {
            ResponseFormat::Text => {}
            ResponseFormat::JsonObject => {
                body["response_format"] = serde_json::json!({ "type": "json_object" });
            }
            ResponseFormat::JsonSchema { schema, .. } => {
                body["guided_json"] = schema.clone();
            }
        }

        let resp = self
            .client
            .post(format!("{}/v1/completions", self.cfg.base_url))
            .json(&body)
            .send()
            .await
            .map_err(|e| if e.is_timeout() { GenError::Timeout } else { GenError::Unavailable })?;

        if !resp.status().is_success() {
            return Err(GenError::Upstream(format!("HTTP {}", resp.status())));
        }

        let mut stream = resp.bytes_stream();
        // Buffer raw bytes; decode only complete lines (each ends in '\n', an
        // ASCII byte that never splits a multibyte UTF-8 char) so a chunk
        // boundary mid-character can't corrupt the text.
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
                let (events, _rest) = parse_sse(&region); // region is all complete lines
                for ev in events {
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
        let ready = self
            .client
            .get(format!("{}/health", self.cfg.base_url))
            .timeout(Duration::from_secs(2))
            .send()
            .await
            .map(|r| r.status().is_success())
            .unwrap_or(false);
        let (num_running, num_waiting) = self.fetch_queue().await;
        EngineHealth { ready, num_running, num_waiting }
    }
}

/// A parsed SSE record from the `/v1/completions` stream.
#[derive(Debug, PartialEq, Eq)]
enum SseEvent {
    Delta(String),
    Done,
}

/// Parse complete `data:` lines out of `region` (which must contain only whole
/// lines). Pure + side-effect-free so it is unit-testable without a network.
/// Returns the events plus any leftover partial tail (empty when `region` ends
/// in a newline, which is how the streaming caller invokes it).
fn parse_sse(region: &str) -> (Vec<SseEvent>, String) {
    let mut events = Vec::new();
    let (complete, rest) = match region.rfind('\n') {
        Some(i) => (&region[..=i], region[i + 1..].to_string()),
        None => return (events, region.to_string()),
    };
    for line in complete.lines() {
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
        if let Ok(v) = serde_json::from_str::<serde_json::Value>(payload) {
            if let Some(t) = v
                .get("choices")
                .and_then(|c| c.get(0))
                .and_then(|c| c.get("text"))
                .and_then(|t| t.as_str())
            {
                events.push(SseEvent::Delta(t.to_string()));
            }
        }
    }
    (events, rest)
}

/// Extract the value of a Prometheus metric line `name <value>` (ignores `# HELP`
/// / `# TYPE` and any labels are not expected on these gauges).
fn parse_metric(body: &str, name: &str) -> Option<f64> {
    body.lines()
        .filter(|l| !l.starts_with('#'))
        .find_map(|l| {
            let l = l.trim();
            let rest = l.strip_prefix(name)?;
            rest.trim().split_whitespace().next()?.parse::<f64>().ok()
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_two_deltas_then_done() {
        let region = "data: {\"choices\":[{\"text\":\"The\",\"finish_reason\":null}]}\n\
                      data: {\"choices\":[{\"text\":\" Secretary\",\"finish_reason\":null}]}\n\
                      data: [DONE]\n";
        let (ev, rest) = parse_sse(region);
        assert_eq!(
            ev,
            vec![
                SseEvent::Delta("The".to_string()),
                SseEvent::Delta(" Secretary".to_string()),
                SseEvent::Done,
            ]
        );
        assert_eq!(rest, "");
    }

    #[test]
    fn buffers_incomplete_trailing_line() {
        let region = "data: {\"choices\":[{\"text\":\"A\"}]}\ndata: {\"choi";
        let (ev, rest) = parse_sse(region);
        assert_eq!(ev, vec![SseEvent::Delta("A".to_string())]);
        assert_eq!(rest, "data: {\"choi");
    }

    #[test]
    fn final_empty_text_delta_is_kept_but_skipped_by_caller() {
        // vLLM's terminal chunk before [DONE] carries an empty text + stop reason.
        let region = "data: {\"choices\":[{\"text\":\"\",\"finish_reason\":\"stop\"}]}\n";
        let (ev, _rest) = parse_sse(region);
        assert_eq!(ev, vec![SseEvent::Delta(String::new())]);
    }

    #[test]
    fn ignores_blank_and_non_data_lines() {
        let region = ": keep-alive comment\n\ndata: {\"choices\":[{\"text\":\"x\"}]}\n";
        let (ev, _rest) = parse_sse(region);
        assert_eq!(ev, vec![SseEvent::Delta("x".to_string())]);
    }

    #[test]
    fn gemma_translation_folds_system_and_maps_roles() {
        // Shape mirrors build_chat_prompt: system + citation demo + history +
        // final user turn + assistant cue with the Qwen no-think prefill.
        let chatml = "<|im_start|>system\nYou are a legal assistant. Cite [n].<|im_end|>\n\
<|im_start|>user\nContext passages:\n[1] rules.pdf\nQuestion: Who?<|im_end|>\n\
<|im_start|>assistant\n<think>\n\n</think>\n\n";
        let g = chatml_to_gemma(chatml);
        assert!(!g.contains("<|im_start|>") && !g.contains("<|im_end|>"), "{g}");
        assert!(!g.contains("<think>"), "{g}");
        assert!(g.starts_with(
            "<start_of_turn>user\nYou are a legal assistant. Cite [n].\n\nContext passages:"
        ), "{g}");
        assert!(g.ends_with("<start_of_turn>model\n"), "{g}");
        // Exactly one user turn: system folded, not emitted as its own turn.
        assert_eq!(g.matches("<start_of_turn>user\n").count(), 1);
        assert_eq!(g.matches("<end_of_turn>").count(), 1);
    }

    #[test]
    fn gemma_translation_preserves_multi_turn_history() {
        let chatml = "<|im_start|>system\nsys<|im_end|>\n\
<|im_start|>user\nq1<|im_end|>\n<|im_start|>assistant\na1<|im_end|>\n\
<|im_start|>user\nq2<|im_end|>\n<|im_start|>assistant\n";
        let g = chatml_to_gemma(chatml);
        assert_eq!(g.matches("<start_of_turn>user\n").count(), 2); // sys folded into q1
        assert_eq!(g.matches("<start_of_turn>model\n").count(), 2);
        assert!(g.contains("<start_of_turn>model\na1<end_of_turn>\n"), "{g}");
        assert!(g.ends_with("<start_of_turn>model\n"), "{g}");
    }

    #[test]
    fn gemma_translation_without_system_block_is_pure_role_mapping() {
        let chatml = "<|im_start|>user\nhello<|im_end|>\n<|im_start|>assistant\n";
        assert_eq!(
            chatml_to_gemma(chatml),
            "<start_of_turn>user\nhello<end_of_turn>\n<start_of_turn>model\n"
        );
    }

    #[test]
    fn gemma_stop_mapping_swaps_im_end() {
        let stops = stops_for_gemma(&["<|im_end|>".to_string(), "</s>".to_string()]);
        assert_eq!(stops, vec!["<end_of_turn>".to_string(), "</s>".to_string()]);
        // No ChatML marker present -> end_of_turn still appended.
        let stops = stops_for_gemma(&["</s>".to_string()]);
        assert!(stops.contains(&"<end_of_turn>".to_string()));
    }

    #[test]
    fn parse_metric_reads_gauge_value() {
        let body = "# HELP vllm:num_requests_running n\n# TYPE vllm:num_requests_running gauge\n\
                    vllm:num_requests_running 3.0\nvllm:num_requests_waiting 2.0\n";
        assert_eq!(parse_metric(body, "vllm:num_requests_running"), Some(3.0));
        assert_eq!(parse_metric(body, "vllm:num_requests_waiting"), Some(2.0));
        assert_eq!(parse_metric(body, "vllm:absent"), None);
    }
}
