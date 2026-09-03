//! T005/T006 — dynamic `n_ctx` resolver + real-token counter (FR-010, R1/R2).
//!
//! Replaces the hardcoded per-call-site `n_ctx` (llm.rs 2048/1024, query_router
//! 3584). The usable window is derived from the loaded model's **trained**
//! context length, clamped to what RAM/VRAM can afford for the KV cache, with an
//! `ECHO_N_CTX` override for testing. The KV-math and clamp are pure (and
//! unit-tested); only the model-dependent helpers are `llm`-feature-gated.

use crate::services::assistant::context_budget::TokenCounter;

/// Lower bound for any resolved window.
pub const MIN_N_CTX: u32 = 1024;
/// Used when the model reports no trained length.
pub const FALLBACK_TRAINED: u32 = 4096;
/// Fraction of available RAM/VRAM the KV cache may consume.
const KV_BUDGET_FRACTION: f64 = 0.35;
/// KV-cache element size in bytes (f16 cache).
const KV_BYTES_PER_ELEM: u64 = 2;
const BYTES_PER_GB: f64 = 1_073_741_824.0;

/// Per-token KV-cache bytes: `layers × 2 (K+V) × kv_embd × elem_size`, where
/// `kv_embd = n_embd × n_head_kv / n_head` (GQA-aware; falls back to `n_embd`
/// when head counts are unknown).
pub fn kv_bytes_per_token(n_layer: u32, n_embd: u32, n_head: u32, n_head_kv: u32) -> u64 {
    let kv_embd = if n_head > 0 && n_head_kv > 0 {
        (n_embd as u64 * n_head_kv as u64) / n_head as u64
    } else {
        n_embd as u64
    };
    (n_layer as u64) * 2 * kv_embd.max(1) * KV_BYTES_PER_ELEM
}

/// Pure clamp: resolve the window from the model's trained length, the available
/// RAM/VRAM, the per-token KV cost, and an optional `ECHO_N_CTX` override.
/// Result is always within `[MIN_N_CTX, trained]`.
pub fn resolve_n_ctx(
    trained: u32,
    available_bytes: u64,
    kv_bytes_per_token: u64,
    env_override: Option<u32>,
) -> u32 {
    let trained = if trained == 0 { FALLBACK_TRAINED } else { trained };
    if let Some(o) = env_override {
        return o.clamp(MIN_N_CTX, trained.max(MIN_N_CTX));
    }
    let budget = (available_bytes as f64 * KV_BUDGET_FRACTION) as u64;
    let by_ram = if kv_bytes_per_token == 0 {
        trained
    } else {
        (budget / kv_bytes_per_token).min(u32::MAX as u64) as u32
    };
    by_ram.min(trained).max(MIN_N_CTX)
}

/// `ECHO_N_CTX` override if set and parseable.
pub fn env_override() -> Option<u32> {
    std::env::var("ECHO_N_CTX").ok().and_then(|v| v.trim().parse::<u32>().ok())
}

// ── Model-dependent helpers (require the loaded model) ───────────────────────

/// Resolve the window for a loaded model on the given hardware. Uses VRAM when a
/// discrete GPU is present (model + KV live there), else system RAM.
#[cfg(feature = "llm")]
pub fn for_model(
    model: &llama_cpp_2::model::LlamaModel,
    ram_gb: f32,
    vram_gb: f32,
    has_discrete_gpu: bool,
) -> u32 {
    let trained = model.n_ctx_train();
    let kv = kv_bytes_per_token(
        model.n_layer(),
        model.n_embd() as u32,
        model.n_head(),
        model.n_head_kv(),
    );
    let gb = if has_discrete_gpu && vram_gb >= 2.0 { vram_gb } else { ram_gb };
    let available = (gb as f64 * BYTES_PER_GB) as u64;
    let resolved = resolve_n_ctx(trained, available, kv, env_override());
    log::info!(
        "n_ctx resolved: trained={trained} kv/tok={kv}B avail={gb:.1}GB → n_ctx={resolved}{}",
        if env_override().is_some() { " (ECHO_N_CTX override)" } else { "" }
    );
    resolved
}

/// T006 — real `TokenCounter` backed by the loaded model's tokenizer. Falls back
/// to a whitespace estimate only if tokenization errors (never panics).
#[cfg(feature = "llm")]
pub struct ModelTokenCounter<'a> {
    pub model: &'a llama_cpp_2::model::LlamaModel,
}

#[cfg(feature = "llm")]
impl TokenCounter for ModelTokenCounter<'_> {
    fn count(&self, text: &str) -> usize {
        self.model
            .str_to_token(text, llama_cpp_2::model::AddBos::Never)
            .map(|t| t.len())
            .unwrap_or_else(|_| text.split_whitespace().count())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn override_clamps_within_trained_and_floor() {
        // override above trained → clamped to trained
        assert_eq!(resolve_n_ctx(8192, u64::MAX, 1, Some(99999)), 8192);
        // override below floor → MIN_N_CTX
        assert_eq!(resolve_n_ctx(8192, u64::MAX, 1, Some(100)), MIN_N_CTX);
        // valid override passes through
        assert_eq!(resolve_n_ctx(8192, u64::MAX, 1, Some(4096)), 4096);
    }

    #[test]
    fn zero_trained_uses_fallback() {
        // plenty of RAM, no override, trained=0 → fallback 4096 (RAM not limiting)
        assert_eq!(resolve_n_ctx(0, u64::MAX, 1, None), FALLBACK_TRAINED);
    }

    #[test]
    fn ram_limits_below_trained() {
        // kv=1MB/token, budget fraction 0.35 of 8GB ≈ 3006 tokens < trained 32768
        let avail = (8.0 * BYTES_PER_GB) as u64;
        let kv = 1_048_576; // 1 MiB/token
        let n = resolve_n_ctx(32768, avail, kv, None);
        assert!(n < 32768 && n >= MIN_N_CTX, "got {n}");
    }

    #[test]
    fn ample_ram_uses_full_trained_window() {
        // tiny KV cost + huge RAM → trained is the binding limit
        let avail = (64.0 * BYTES_PER_GB) as u64;
        assert_eq!(resolve_n_ctx(8192, avail, 4096, None), 8192);
    }

    #[test]
    fn never_below_floor_even_on_tiny_ram() {
        let n = resolve_n_ctx(8192, 1, 1_000_000_000, None); // 1 byte RAM
        assert_eq!(n, MIN_N_CTX);
    }

    #[test]
    fn kv_bytes_gqa_aware() {
        // n_embd=4096, 32 heads, 8 kv heads → kv_embd=1024; 32 layers
        // = 32 * 2 * 1024 * 2 = 131072 bytes/token
        assert_eq!(kv_bytes_per_token(32, 4096, 32, 8), 131_072);
        // unknown heads → falls back to n_embd
        assert_eq!(kv_bytes_per_token(1, 100, 0, 0), 1 * 2 * 100 * 2);
    }
}
