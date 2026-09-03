//! Runtime-mutable inference engine selection.
//!
//! Engine choice + remote-API config used to be captured once from env in
//! `OnceLock` generators, so switching required a restart. This module holds the
//! **active** config behind an `RwLock` so the admin dashboard can flip between
//! local-GPU (vLLM) and a hosted API (Groq/…) — inserting an API key — **without
//! a restart**. It is seeded from env on first use (backward compatible), then
//! optionally overridden from `db_config` at startup and by the admin endpoint.
//!
//! Persistence lives in `db_config` (key/value): `infer_engine`, `remote_base_url`,
//! `remote_model`, `remote_endpoint`, `remote_api_key`. The API key is stored in
//! the bundled Postgres (same trust boundary as the corpus) and is **never**
//! returned by the read path — only a `has_api_key` boolean.

use std::sync::{Arc, OnceLock, RwLock};

use super::remote::{Endpoint, RemoteConfig, RemoteGenerator};
use super::vllm::{VllmConfig, VllmGenerator};
use super::{engine_from_env_raw, engine_str, parse_engine, EngineKind, TextGenerator};
use crate::db::Db;

const K_ENGINE: &str = "infer_engine";
const K_URL: &str = "remote_base_url";
const K_MODEL: &str = "remote_model";
const K_ENDPOINT: &str = "remote_endpoint";
const K_PREFIX: &str = "remote_prefix";
const K_KEY: &str = "remote_api_key";

struct RuntimeState {
    engine: EngineKind,
    remote: RemoteConfig,
    /// Cached generator for the active HTTP engine (`None` for llama.cpp), rebuilt
    /// on every `apply` so a new key/model/URL takes effect immediately.
    active: Option<Arc<dyn TextGenerator>>,
}

fn build_active(engine: EngineKind, remote: &RemoteConfig) -> Option<Arc<dyn TextGenerator>> {
    match engine {
        EngineKind::Vllm => {
            Some(Arc::new(VllmGenerator::new(VllmConfig::from_env())) as Arc<dyn TextGenerator>)
        }
        EngineKind::Remote => {
            Some(Arc::new(RemoteGenerator::new(remote.clone())) as Arc<dyn TextGenerator>)
        }
        EngineKind::LlamaCpp => None,
    }
}

fn state() -> &'static RwLock<RuntimeState> {
    static S: OnceLock<RwLock<RuntimeState>> = OnceLock::new();
    S.get_or_init(|| {
        let engine = engine_from_env_raw();
        let remote = RemoteConfig::from_env();
        let active = build_active(engine, &remote);
        RwLock::new(RuntimeState { engine, remote, active })
    })
}

/// The active engine (runtime-mutable, seeded from env).
pub fn current_engine() -> EngineKind {
    state().read().map(|s| s.engine).unwrap_or(EngineKind::LlamaCpp)
}

/// The active HTTP generator, or `None` for the in-process llama.cpp engine.
pub fn http_generator() -> Option<Arc<dyn TextGenerator>> {
    state().read().ok().and_then(|s| s.active.clone())
}

/// The current remote config (used to merge partial admin updates — a blank key
/// keeps the existing one).
pub fn current_remote() -> RemoteConfig {
    state()
        .read()
        .map(|s| s.remote.clone())
        .unwrap_or_else(|_| RemoteConfig::from_env())
}

/// Non-secret snapshot for the admin read path. Never exposes the API key.
#[derive(Debug, Clone)]
pub struct InferenceStatus {
    pub engine: EngineKind,
    pub remote_base_url: String,
    pub remote_model: String,
    pub remote_endpoint: &'static str,
    pub remote_prefix: String,
    pub has_api_key: bool,
}

pub fn status() -> InferenceStatus {
    let s = current_remote();
    InferenceStatus {
        engine: current_engine(),
        remote_base_url: s.base_url,
        remote_model: s.model,
        remote_endpoint: s.endpoint.as_str(),
        remote_prefix: s.api_prefix,
        has_api_key: s.api_key.as_ref().map(|k| !k.is_empty()).unwrap_or(false),
    }
}

/// Replace the active engine + remote config and rebuild the generator. Effective
/// immediately for subsequent turns (no restart).
pub fn apply(engine: EngineKind, remote: RemoteConfig) {
    let active = build_active(engine, &remote);
    if let Ok(mut s) = state().write() {
        s.engine = engine;
        s.remote = remote;
        s.active = active;
    }
}

/// Load a persisted override from `db_config` at startup. No-op (keeps env
/// defaults) when nothing has been saved. Best-effort: DB errors are ignored so a
/// read failure never blocks boot.
pub async fn load_from_db(pool: &Db) {
    let Some(engine_s) = get(pool, K_ENGINE).await else {
        return; // nothing saved → keep the env-seeded defaults
    };
    let engine = parse_engine(&engine_s);
    let mut remote = current_remote();
    if let Some(v) = get(pool, K_URL).await {
        remote.base_url = v.trim_end_matches('/').to_string();
    }
    if let Some(v) = get(pool, K_MODEL).await {
        remote.model = v;
    }
    if let Some(v) = get(pool, K_ENDPOINT).await {
        remote.endpoint = Endpoint::from_str_or_default(&v);
    }
    if let Some(v) = get(pool, K_PREFIX).await {
        remote.api_prefix = super::remote::normalize_prefix(&v);
    }
    if let Some(v) = get(pool, K_KEY).await {
        if !v.is_empty() {
            remote.api_key = Some(v);
        }
    }
    apply(engine, remote);
    log::info!("inference: loaded engine override from db_config ({engine_s})");
}

/// Persist the current selection to `db_config`. The API key is written only when
/// present (a blank update leaves the stored key untouched).
pub async fn save_to_db(pool: &Db, engine: EngineKind, remote: &RemoteConfig) -> anyhow::Result<()> {
    set(pool, K_ENGINE, engine_str(engine)).await?;
    set(pool, K_URL, &remote.base_url).await?;
    set(pool, K_MODEL, &remote.model).await?;
    set(pool, K_ENDPOINT, remote.endpoint.as_str()).await?;
    set(pool, K_PREFIX, &remote.api_prefix).await?;
    if let Some(k) = &remote.api_key {
        if !k.is_empty() {
            set(pool, K_KEY, k).await?;
        }
    }
    Ok(())
}

async fn get(pool: &Db, key: &str) -> Option<String> {
    sqlx::query_scalar::<_, String>("SELECT value FROM db_config WHERE key = $1")
        .bind(key)
        .fetch_optional(pool)
        .await
        .ok()
        .flatten()
}

async fn set(pool: &Db, key: &str, val: &str) -> anyhow::Result<()> {
    sqlx::query(
        "INSERT INTO db_config (key, value) VALUES ($1, $2) \
         ON CONFLICT (key) DO UPDATE SET value = EXCLUDED.value",
    )
    .bind(key)
    .bind(val)
    .execute(pool)
    .await?;
    Ok(())
}
