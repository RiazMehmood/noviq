//! Cloud LLM inference — opt-in via Settings.
//!
//! When enabled, chat queries are routed through an OpenAI-compatible
//! streaming chat-completions API instead of the local llama.cpp model.
//! Emits the same `chat://token` events as the local path so the frontend
//! is unchanged.
//!
//! Multi-provider (2026-05-23): the endpoint is selected by `settings.provider`
//! ("groq", "cerebras", "openrouter", "together", "deepseek", "openai",
//! "fireworks") or an explicit `settings.endpoint_url` override. All supported
//! providers expose the same `/v1/chat/completions` shape (OpenAI-compatible),
//! so the request/response code is provider-agnostic.
//!
//! Settings are stored as JSON at `<app_data>/cloud_settings.json` to avoid
//! a DB migration. See CHANGELOG_DEV.md → "Cloud inference multi-provider".

use anyhow::{anyhow, Result};
use futures::StreamExt;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::{Mutex, OnceLock};
// F014: AppHandle/Emitter via the compat shim so this compiles in the headless
// server build. `Manager` (path resolver) is desktop-only — see settings_path.
use crate::compat::{AppHandle, Emitter};
#[cfg(feature = "desktop")]
use tauri::Manager;
use tokio_util::sync::CancellationToken;

/// Process-wide cached cloud settings — populated on first load and refreshed
/// via `save_settings`. Lets non-Tauri callers (indexer, batch processor) read
/// the user's cloud config without an AppHandle. Added 2026-05-03.
static CACHED: OnceLock<Mutex<CloudSettings>> = OnceLock::new();

fn cached_slot() -> &'static Mutex<CloudSettings> {
    CACHED.get_or_init(|| Mutex::new(CloudSettings::default()))
}

/// Read the cached cloud settings (no AppHandle required).
pub fn load_settings_cached() -> CloudSettings {
    cached_slot().lock().map(|g| g.clone()).unwrap_or_default()
}

fn write_cache(s: &CloudSettings) {
    if let Ok(mut g) = cached_slot().lock() { *g = s.clone(); }
}

const DEFAULT_MODEL: &str = "llama-3.1-8b-instant";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CloudSettings {
    pub enabled: bool,
    /// One of: "groq", "cerebras", "openrouter", "together", "deepseek",
    /// "openai", "fireworks", or "custom" (when `endpoint_url` is set).
    pub provider: String,
    pub api_key: String,
    pub model: String,
    /// Optional override — when set, takes precedence over the provider's
    /// default endpoint. Required when `provider == "custom"`. Empty string
    /// means "use the provider default."
    #[serde(default)]
    pub endpoint_url: String,
}

impl Default for CloudSettings {
    fn default() -> Self {
        Self {
            enabled: false,
            provider: "groq".to_string(),
            api_key: std::env::var("ECHO_GROQ_API_KEY").unwrap_or_default(),
            model: DEFAULT_MODEL.to_string(),
            endpoint_url: String::new(),
        }
    }
}

/// Resolve the chat-completions endpoint for these settings. An explicit
/// `endpoint_url` always wins so users can point at internal proxies or
/// providers we haven't built-in. Unknown providers fall back to the Groq
/// URL for backwards compatibility with pre-multi-provider configs.
fn endpoint(settings: &CloudSettings) -> &str {
    let custom = settings.endpoint_url.trim();
    if !custom.is_empty() {
        return custom;
    }
    match settings.provider.as_str() {
        "groq"       => "https://api.groq.com/openai/v1/chat/completions",
        "cerebras"   => "https://api.cerebras.ai/v1/chat/completions",
        "openrouter" => "https://openrouter.ai/api/v1/chat/completions",
        "together"   => "https://api.together.xyz/v1/chat/completions",
        "deepseek"   => "https://api.deepseek.com/v1/chat/completions",
        "openai"     => "https://api.openai.com/v1/chat/completions",
        "fireworks"  => "https://api.fireworks.ai/inference/v1/chat/completions",
        _ => "https://api.groq.com/openai/v1/chat/completions",
    }
}

fn settings_path(app: &AppHandle) -> Result<PathBuf> {
    // Desktop: resolve via the Tauri path API. Headless server: the compat
    // AppHandle is a stub with no path resolver, so derive the data dir from
    // NOVIQ_DATA_DIR (same convention as the other server-side caches). On the
    // server every caller passes `None` and uses load_settings_cached(), so this
    // path is effectively dead there — it only needs to type-check.
    #[cfg(feature = "desktop")]
    let dir = app.path().app_data_dir().map_err(|e| anyhow!("app_data_dir: {e}"))?;
    #[cfg(not(feature = "desktop"))]
    let dir = {
        let _ = app;
        PathBuf::from(std::env::var("NOVIQ_DATA_DIR").unwrap_or_else(|_| ".noviq-data".to_string()))
    };
    std::fs::create_dir_all(&dir)?;
    Ok(dir.join("cloud_settings.json"))
}

pub fn load_settings(app: &AppHandle) -> CloudSettings {
    let Ok(path) = settings_path(app) else {
        let d = CloudSettings::default();
        write_cache(&d);
        return d;
    };
    let s = match std::fs::read_to_string(&path) {
        Ok(body) => serde_json::from_str::<CloudSettings>(&body).unwrap_or_default(),
        Err(_) => CloudSettings::default(),
    };
    write_cache(&s);
    s
}

pub fn save_settings(app: &AppHandle, s: &CloudSettings) -> Result<()> {
    let path = settings_path(app)?;
    let body = serde_json::to_string_pretty(s)?;
    std::fs::write(&path, body)?;
    write_cache(s);
    Ok(())
}

/// Stream a chat completion from Groq. Returns (full_text, was_cancelled).
pub async fn groq_chat_streaming(
    settings: &CloudSettings,
    system_prompt: &str,
    context: &str,
    query: &str,
    history: &[String],
    max_tokens: u32,
    cancel: Option<CancellationToken>,
    app: Option<AppHandle>,
) -> Result<(String, bool)> {
    if settings.api_key.trim().is_empty() {
        return Err(anyhow!("Cloud API key not set in Settings ({} provider)", settings.provider));
    }

    // Build OpenAI-style messages: system + alternating prior turns + final user.
    let mut messages: Vec<serde_json::Value> = Vec::new();
    let combined_system = if context.is_empty() {
        system_prompt.to_string()
    } else {
        format!("{system_prompt}\n\nContext:\n{context}")
    };
    messages.push(serde_json::json!({ "role": "system", "content": combined_system }));
    for (i, turn) in history.iter().enumerate() {
        let role = if i % 2 == 0 { "user" } else { "assistant" };
        messages.push(serde_json::json!({ "role": role, "content": turn }));
    }
    messages.push(serde_json::json!({ "role": "user", "content": query }));

    let body = serde_json::json!({
        "model": settings.model,
        "messages": messages,
        "max_tokens": max_tokens,
        "temperature": 0.1,
        "stream": true,
    });

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(120))
        .build()?;
    let url = endpoint(settings);
    let resp = client
        .post(url)
        .header("Authorization", format!("Bearer {}", settings.api_key))
        .header("Content-Type", "application/json")
        .json(&body)
        .send()
        .await?;

    if !resp.status().is_success() {
        let status = resp.status();
        let text = resp.text().await.unwrap_or_default();
        return Err(anyhow!("{} HTTP {status}: {text}", settings.provider));
    }

    let mut stream = resp.bytes_stream();
    let mut full = String::new();
    let mut buf: String = String::new();
    let mut was_cancelled = false;

    while let Some(chunk) = stream.next().await {
        if let Some(ref tok) = cancel {
            if tok.is_cancelled() { was_cancelled = true; break; }
        }
        let bytes = match chunk {
            Ok(b) => b,
            Err(e) => return Err(anyhow!("stream read: {e}")),
        };
        buf.push_str(&String::from_utf8_lossy(&bytes));

        // SSE events are separated by \n; each starts with "data: "
        while let Some(nl) = buf.find('\n') {
            let line = buf[..nl].trim().to_string();
            buf.drain(..=nl);
            let Some(payload) = line.strip_prefix("data: ") else { continue; };
            if payload == "[DONE]" { break; }
            let v: serde_json::Value = match serde_json::from_str(payload) {
                Ok(v) => v,
                Err(_) => continue,
            };
            let delta = v
                .get("choices")
                .and_then(|c| c.get(0))
                .and_then(|c| c.get("delta"))
                .and_then(|d| d.get("content"))
                .and_then(|c| c.as_str())
                .unwrap_or("");
            if !delta.is_empty() {
                full.push_str(delta);
                if let Some(ref a) = app {
                    let _ = a.emit("chat://token", serde_json::json!({ "token": delta }));
                }
            }
        }
    }

    Ok((full, was_cancelled))
}

/// Synchronous-style Groq call for non-chat callers (analyze, KG, batch).
/// Non-streaming, no chat://token events. Returns the assistant text.
pub async fn groq_complete(
    settings: &CloudSettings,
    system_prompt: &str,
    user_prompt: &str,
    max_tokens: u32,
) -> Result<String> {
    if settings.api_key.trim().is_empty() {
        return Err(anyhow!("Cloud API key not set ({} provider)", settings.provider));
    }
    let body = serde_json::json!({
        "model": settings.model,
        "messages": [
            { "role": "system", "content": system_prompt },
            { "role": "user", "content": user_prompt }
        ],
        "max_tokens": max_tokens,
        "temperature": 0.1,
        "stream": false,
    });
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(120))
        .build()?;
    let url = endpoint(settings);
    let resp = client
        .post(url)
        .header("Authorization", format!("Bearer {}", settings.api_key))
        .header("Content-Type", "application/json")
        .json(&body)
        .send()
        .await?;
    if !resp.status().is_success() {
        let status = resp.status();
        let text = resp.text().await.unwrap_or_default();
        return Err(anyhow!("{} HTTP {status}: {text}", settings.provider));
    }
    let v: serde_json::Value = resp.json().await?;
    let content = v
        .get("choices")
        .and_then(|c| c.get(0))
        .and_then(|c| c.get("message"))
        .and_then(|m| m.get("content"))
        .and_then(|c| c.as_str())
        .unwrap_or("")
        .to_string();
    Ok(content)
}

/// Helper: blocking-friendly Groq call. Spawns a runtime in a separate thread
/// so it can be invoked from sync code (e.g. inside `model_slot.infer()` paths
/// that don't already have a tokio context).
pub fn groq_complete_blocking(
    settings: &CloudSettings,
    system_prompt: &str,
    user_prompt: &str,
    max_tokens: u32,
) -> Result<String> {
    let s = settings.clone();
    let sys = system_prompt.to_string();
    let user = user_prompt.to_string();
    std::thread::spawn(move || {
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .map_err(|e| anyhow!("rt build: {e}"))?;
        rt.block_on(groq_complete(&s, &sys, &user, max_tokens))
    })
    .join()
    .map_err(|_| anyhow!("cloud blocking thread panicked"))?
}
