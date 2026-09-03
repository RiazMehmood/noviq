//! T029 — Tauri command surface for the LLM IR importer.
//!
//! Owns a process-global `CancellationToken` registry keyed by `source_id`.
//! The dispatch path (`services::doc_ir::dispatch::try_llm_ir_path`) registers
//! a token before invoking the builder and drops it on completion; the
//! frontend can call `cancel_import` to flip it, which causes any in-flight
//! `infer_with_cancel` and the chunk loop to return promptly.

use std::collections::HashMap;
use std::sync::OnceLock;

use tokio::sync::RwLock;
use tokio_util::sync::CancellationToken;

/// Process-global token registry. Lazily initialised.
fn registry() -> &'static RwLock<HashMap<String, CancellationToken>> {
    static REG: OnceLock<RwLock<HashMap<String, CancellationToken>>> = OnceLock::new();
    REG.get_or_init(|| RwLock::new(HashMap::new()))
}

/// Register a token for a source. If a stale entry exists (previous import
/// abandoned without cleanup), it's overwritten.
pub async fn register(source_id: &str, token: CancellationToken) {
    registry()
        .write()
        .await
        .insert(source_id.to_string(), token);
}

/// Remove a source's token. Called on dispatch completion (success, failure,
/// or cancellation).
pub async fn unregister(source_id: &str) {
    registry().write().await.remove(source_id);
}

/// Number of in-flight imports (used by tests).
#[cfg(test)]
pub async fn active_count() -> usize {
    registry().read().await.len()
}

/// Tauri command: cancel an in-flight LLM IR import.
///
/// Returns `true` if a token was found and flipped; `false` if no import is
/// active for that source (already finished or never started).
#[tauri::command]
pub async fn cancel_import(source_id: String) -> Result<bool, String> {
    let guard = registry().read().await;
    if let Some(tok) = guard.get(&source_id) {
        if !tok.is_cancelled() {
            tok.cancel();
            log::info!("commands::importer: cancel_import flipped token for {source_id}");
            return Ok(true);
        }
        return Ok(false);
    }
    Ok(false)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn cancel_unknown_returns_false() {
        let r = cancel_import("nope-never-registered".to_string()).await.unwrap();
        assert!(!r);
    }

    #[tokio::test]
    async fn register_then_cancel_flips_token() {
        let tok = CancellationToken::new();
        register("src-xyz", tok.clone()).await;
        assert!(!tok.is_cancelled());

        let r = cancel_import("src-xyz".to_string()).await.unwrap();
        assert!(r);
        assert!(tok.is_cancelled());

        // Second call: already cancelled → false.
        let r2 = cancel_import("src-xyz".to_string()).await.unwrap();
        assert!(!r2);

        unregister("src-xyz").await;
    }

    #[tokio::test]
    async fn unregister_clears_entry() {
        let tok = CancellationToken::new();
        register("src-clear", tok).await;
        unregister("src-clear").await;
        let r = cancel_import("src-clear".to_string()).await.unwrap();
        assert!(!r);
    }
}
