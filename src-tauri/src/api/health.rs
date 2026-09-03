//! GET /api/health (T007) — unauthenticated liveness + GPU/version probe.
//!
//! Consumed by two callers:
//!   - the deploy post-restart health probe (FR-017): a 200 with `status:"ok"`
//!     means the freshly-built binary came up; anything else trips the
//!     auto-rollback in deploy.sh.
//!   - the browser thin-client: while a deploy is mid-flight the still-running
//!     old server reports `status:"deploying"` so the UI can show an "updating"
//!     banner instead of a broken page (deploy.sh writes/clears the sentinel;
//!     see deploy/deploy.sh + deployment-and-updates.md).

use axum::Json;
use std::path::PathBuf;

/// Sentinel file `deploy.sh` touches at the start of a deploy and removes before
/// the restart. Lives under the server's data dir so this process and the deploy
/// script agree on the path. Default mirrors `noviq-server.rs` (`.noviq-data`).
fn deploy_sentinel() -> PathBuf {
    let data_dir =
        std::env::var("NOVIQ_DATA_DIR").unwrap_or_else(|_| ".noviq-data".to_string());
    PathBuf::from(data_dir).join("DEPLOYING")
}

/// How long a sentinel may claim "deploying" before we stop believing it.
/// `NOVIQ_DEPLOY_SENTINEL_MAX_AGE_MIN`, default 120 min.
fn sentinel_max_age() -> std::time::Duration {
    let mins: u64 = std::env::var("NOVIQ_DEPLOY_SENTINEL_MAX_AGE_MIN")
        .ok()
        .and_then(|v| v.parse().ok())
        .filter(|m| *m > 0)
        .unwrap_or(120);
    std::time::Duration::from_secs(mins * 60)
}

/// `true` only while a deploy is *plausibly* in flight.
///
/// Existence alone is not enough (2026-07-24). `deploy.sh` removes the sentinel from a
/// `trap cleanup EXIT`, which does NOT run on SIGKILL — an OOM-killed or
/// `systemctl kill`ed deploy therefore leaves the file behind and every later request
/// reports `status:"deploying"` forever. That pins the browser's "updating" banner with
/// zero builds happening, and it also makes the deploy health probe (FR-017) roll back a
/// perfectly good binary. Ageing the sentinel out by mtime bounds the damage.
///
/// The threshold MUST exceed a worst-case CUDA rebuild; too low and we would clear the
/// banner mid-build and show users a broken page instead. 120 min is deliberately
/// generous versus observed build times — override per box if that box is slower.
fn deploy_in_progress() -> bool {
    sentinel_fresh(&deploy_sentinel(), sentinel_max_age())
}

/// Planned-maintenance sentinel — a DIFFERENT state from a deploy.
///
/// Written by operator tooling (`gpu-mode.sh`) when a task deliberately takes chat
/// offline: GPU-embedding mode stops vLLM to free VRAM, so chat is genuinely
/// unavailable, but nothing is being rebuilt. Reusing `DEPLOYING` for this told
/// users "Noviq is updating to a new version", which was simply false and cost a
/// round of "is it still building?" — the same class of mistake as 2026-07-24.
/// Never claim a deploy we cannot see.
///
/// The file's contents (first line, trimmed) are surfaced to the browser as the
/// banner text, so the operator says why chat is down rather than the UI guessing.
fn maintenance_sentinel() -> PathBuf {
    let data_dir =
        std::env::var("NOVIQ_DATA_DIR").unwrap_or_else(|_| ".noviq-data".to_string());
    PathBuf::from(data_dir).join("MAINTENANCE")
}

/// Reason string when planned maintenance is in flight, else `None`.
/// Aged out on the same rule as the deploy sentinel: a tool killed with SIGKILL
/// never runs its cleanup trap, and a stale file must not pin the banner forever.
fn maintenance_reason() -> Option<String> {
    let path = maintenance_sentinel();
    if !sentinel_fresh(&path, sentinel_max_age()) {
        return None;
    }
    let text = std::fs::read_to_string(&path).unwrap_or_default();
    let line = text.lines().next().unwrap_or("").trim().to_string();
    Some(if line.is_empty() {
        "Noviq is temporarily paused for maintenance.".to_string()
    } else {
        line
    })
}

/// Pure core of [`deploy_in_progress`], split out so it is testable without touching
/// process-wide env vars (`NOVIQ_DATA_DIR` / the max-age override).
fn sentinel_fresh(path: &std::path::Path, max_age: std::time::Duration) -> bool {
    let Ok(meta) = std::fs::metadata(path) else {
        return false;
    };
    // No readable mtime (exotic FS) → fall back to trusting existence.
    let Ok(modified) = meta.modified() else {
        return true;
    };
    match modified.elapsed() {
        // Fresh sentinel → a deploy really is running.
        Ok(age) => age < max_age,
        // mtime in the future (clock skew) → treat as fresh rather than ignore a
        // real deploy.
        Err(_) => true,
    }
}

/// `{ status, gpu, layers, version, engine }`. `status` is `"deploying"` while a
/// rebuild is in progress, else `"ok"`. `gpu`/`layers` reflect the configured
/// `NOVIQ_GPU_LAYERS` (0 = CPU). Actual offload confirmation is logged at model
/// load (US3, T027).
///
/// `engine` (F016 / FR-013) reports the active inference backend:
///   - `{ kind:"vllm", ready, num_running, num_waiting }` — `ready`/queue depth
///     come from the sidecar's `/health` + `/metrics` (cheap, 2s-timeout probe;
///     never fails the endpoint — an unreachable sidecar reports `ready:false`).
///   - `{ kind:"llamacpp", ready:true }` — the in-process slot is always resident;
///     queue depth is N/A (the width-1 gen_gate, not vLLM's batcher).
pub async fn health() -> Json<serde_json::Value> {
    use crate::services::inference::{
        engine_from_env, engine_str, http_generator, EngineKind, TextGenerator,
    };

    let layers: i64 = std::env::var("NOVIQ_GPU_LAYERS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(0);
    // Deploy wins over maintenance: if a rebuild really is in flight the page will
    // need reloading for new assets, and that instruction matters more.
    let maint = maintenance_reason();
    let status = if deploy_in_progress() {
        "deploying"
    } else if maint.is_some() {
        "maintenance"
    } else {
        "ok"
    };

    let active = engine_from_env();
    let engine = match active {
        EngineKind::LlamaCpp => serde_json::json!({
            "kind": "llamacpp",
            "ready": true,
            "num_running": serde_json::Value::Null,
            "num_waiting": serde_json::Value::Null,
        }),
        // HTTP engine (vLLM sidecar / remote API): probe the runtime-active generator.
        e => {
            let (ready, running, waiting) = match http_generator() {
                Some(g) => {
                    let h = g.health().await;
                    (h.ready, h.num_running, h.num_waiting)
                }
                None => (false, 0, 0),
            };
            // Only vLLM exposes queue depth; the remote API does not → null there.
            let (nr, nw) = if e == EngineKind::Vllm {
                (serde_json::json!(running), serde_json::json!(waiting))
            } else {
                (serde_json::Value::Null, serde_json::Value::Null)
            };
            serde_json::json!({ "kind": engine_str(e), "ready": ready, "num_running": nr, "num_waiting": nw })
        }
    };

    Json(serde_json::json!({
        "status": status,
        // Operator-supplied explanation, present only under `maintenance`. The
        // browser renders this verbatim rather than inventing a reason.
        "message": maint,
        "gpu": layers > 0,
        "layers": layers,
        "version": env!("CARGO_PKG_VERSION"),
        "engine": engine,
    }))
}

#[cfg(test)]
mod tests {
    use super::sentinel_fresh;
    use std::time::Duration;

    /// No sentinel → not deploying. The everyday case.
    #[test]
    fn absent_sentinel_is_not_deploying() {
        let dir = tempfile::tempdir().unwrap();
        assert!(!sentinel_fresh(&dir.path().join("DEPLOYING"), Duration::from_secs(60)));
    }

    /// Just-written sentinel → a deploy really is in flight.
    #[test]
    fn fresh_sentinel_is_deploying() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("DEPLOYING");
        std::fs::write(&p, "2026-07-24T00:00:00+05:00").unwrap();
        assert!(sentinel_fresh(&p, Duration::from_secs(60)));
    }

    /// The regression this guards: a sentinel orphaned by a SIGKILLed deploy must NOT
    /// pin `status:"deploying"` forever.
    ///
    /// Backdating the mtime would need an extra crate (`filetime`), so we shrink the
    /// window to zero instead — any real file is older than that by the time it is
    /// stat'ed, which exercises the same `age < max_age` comparison.
    #[test]
    fn stale_sentinel_is_ignored() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("DEPLOYING");
        std::fs::write(&p, "orphaned by SIGKILL").unwrap();
        assert!(!sentinel_fresh(&p, Duration::ZERO));
    }
}
