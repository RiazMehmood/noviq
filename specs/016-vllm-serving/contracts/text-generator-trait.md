# Contract: `TextGenerator` (internal Rust generation seam)

The single abstraction the chat hot-path uses to produce tokens. Everything upstream
(retrieval, prompt building incl. no-think, grounding, citation gating) is unchanged and
calls **only** this trait at the generation step.

## Trait (illustrative — finalized in implementation)

```rust
/// One streaming generation. Implemented by the in-process llama.cpp path (fallback)
/// and the vLLM HTTP client. Selected once at startup from `NOVIQ_INFER_ENGINE`.
#[async_trait::async_trait]
pub trait TextGenerator: Send + Sync {
    /// Generate from an already-built raw prompt, streaming token deltas.
    /// `on_token` is invoked per delta so the caller can emit `chat://` events.
    /// Returns the full decoded text (post any existing cleanup) and a done flag.
    async fn generate_stream(
        &self,
        prompt: &str,                 // exact hand-built ChatML incl. no-think prefix
        params: &GenParams,           // max_tokens, temperature, top_p, stop
        cancel: CancellationToken,    // existing cancel registry
        on_token: &mut (dyn FnMut(&str) + Send),
    ) -> Result<GenOutcome, GenError>;

    /// Cheap readiness probe (drives /api/health.engine).
    async fn health(&self) -> EngineHealth; // { ready, num_running, num_waiting }
}

pub struct GenParams { pub max_tokens: u32, pub temperature: f32, pub top_p: f32, pub stop: Vec<String> }
pub struct GenOutcome { pub text: String, pub was_cancelled: bool }
pub enum GenError { Unavailable, Timeout, Upstream(String) }
```

## Impl A — `LlamaCppGenerator` (fallback, default until cutover)

- Wraps today's in-process `ModelSlot` + `infer_with_cancel*`. Behavior **identical to
  current**; this is the rollback target (FR-017).
- `health()` → always-ready (in-process), `num_running` reflects the `gen_gate` (width 1).

## Impl B — `VllmGenerator`

- HTTP client (`reqwest`) to `NOVIQ_VLLM_URL`. Uses **raw `/v1/completions`** with the
  exact `prompt` string (so no-think + ChatML are byte-identical to the llama.cpp path —
  no server-side chat template surprises). `stream=true` → parse **SSE** `data:` lines →
  call `on_token(delta)` → reassemble full text.
- Maps SSE deltas onto the existing `chat://` token events (no frontend change).
- Errors: connection refused / 5xx → `GenError::Unavailable`; exceeded
  `NOVIQ_VLLM_TIMEOUT_MS` → `GenError::Timeout`. Caller renders the FR-011 friendly message
  (and may fall back to `LlamaCppGenerator` if configured).
- `health()` → GET sidecar `/health` (+ `/metrics` for queue depth).

## Behavioral guarantees (must hold for both impls)

1. Output text passes the **same** downstream cleanup (`strip_reasoning_spans`,
   `strip_special_tokens`) and **grounding/citation gating** — no answer-quality regression
   (FR-003/FR-004/FR-014).
2. Streaming: first token surfaces well before completion (SC-006).
3. Cancellation honored via the existing `CancellationToken` registry.
4. No engine-specific logic leaks above the trait (query_router stays engine-agnostic).
