//! LLM service — single `ModelSlot` with 90-second idle eviction.
//!
//! Feature-gated behind "llm" — compile without `--features llm` for a fast
//! checkpoint build (all LLM paths return stub responses).

use anyhow::{Context, Result};

// ─── LLM tier selection (Feature 007 §FR-027/028/029, constitution §I/II) ───
//
// Every Feature 007 call site MUST pass an explicit `LlmTier`. The router
// uses this to pick the model class (size + cloud vs offline) before
// reaching `ModelSlot`. Default routing rules (research §R1, §R11):
//
//   - OfflineSmall: Qwen2.5-0.5B-FT / 3B for rule-tiebreak intent
//     classification, citation rendering helpers, summarize, action_items.
//   - OfflineMid:   Mistral-7B-Instruct-Q4_K_M for IR build, procedure
//     extraction, Difference (instrument), CrossCompare.
//   - CloudEval:    Groq / Cerebras. Reserved for eval baselines and the
//     officer-toggled "use cloud LLM" Settings switch (FR-029). NEVER
//     selected silently.
//
// Existing Feature 003/004/005/006 call sites are not migrated in Slice 0 —
// each Slice B/C/D handler that uses this enum will pass the explicit tier
// at its call site. See task T020 in tasks.md.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LlmTier {
    OfflineSmall,
    OfflineMid,
    CloudEval,
}

impl LlmTier {
    /// Constitution §I gate. The router MUST NOT dispatch a `CloudEval`
    /// tier unless the officer has explicitly opted in via the Settings
    /// toggle (FR-029). Helper to make accidental silent-cloud impossible
    /// at the call site.
    pub fn requires_cloud_opt_in(self) -> bool {
        matches!(self, LlmTier::CloudEval)
    }
}

// ─── Cancellation-aware public API ───────────────────────────────────────────

#[derive(Debug, thiserror::Error)]
pub enum LlmError {
    #[error("cancelled")]
    Cancelled,
    #[error("{0}")]
    Other(String),
}

/// Cancellation-aware generation. Checks the token before any inference work.
/// When the token is already cancelled on entry, returns `Err(LlmError::Cancelled)`
/// immediately without loading or touching the model.
pub async fn generate_with_cancel(
    _prompt: &str,
    _max_tokens: usize,
    cancel: Option<tokio_util::sync::CancellationToken>,
) -> Result<String, LlmError> {
    if let Some(t) = &cancel {
        if t.is_cancelled() {
            return Err(LlmError::Cancelled);
        }
    }
    // Full decode (mid-token cancellation) happens inside ModelSlot::infer_with_cancel
    // when the "llm" feature is enabled. The stub path never reaches here for a
    // pre-cancelled token.
    Err(LlmError::Other("generate_with_cancel: no model loaded".into()))
}

#[cfg(test)]
mod cancel_tests {
    use super::*;
    use tokio_util::sync::CancellationToken;

    #[tokio::test]
    async fn decode_aborts_when_token_cancelled_before_call() {
        let token = CancellationToken::new();
        token.cancel();
        let result = generate_with_cancel("hello", 32, Some(token)).await;
        assert!(matches!(result, Err(LlmError::Cancelled)));
    }
}

// ─── Grounding system prompt (AD-7) ──────────────────────────────────────────
//
// Versioned constant — never generate this string at runtime. Changing the
// prompt requires bumping GROUNDING_SYSTEM_PROMPT_VERSION so audit_log entries
// remain traceable to the exact prompt that produced each answer.

/// Semantic version of the grounding system prompt.
/// Logged in audit_log on every LLM call for full answer traceability.
pub const GROUNDING_SYSTEM_PROMPT_VERSION: &str = "5.4.0";

/// The LLM system prompt used for ALL document Q&A queries.
///
/// v3.0.0: Removed "1-3 sentences" cap; model answers short for simple facts,
///         long for complex questions.
/// v3.1.0 (2026-05-03): Pushes for detailed answers + ALL relevant citations.
/// v4.0.0 (2026-05-09): Two-part output — actionable human-readable answer
///         (with inline [filename, page N] markers), then a dedicated
///         "References:" block listing each unique source.
/// v4.1.0 (2026-05-09): Trimmed worked example (~250 tokens) to free context
///         budget for the answer; small models were truncating.
/// v4.2.0 (2026-05-11): References template used `FILENAME.pdf` placeholder —
///         small models (Qwen3-1.7B) copied that word literally, producing
///         "[filename, page 51]" in answers. Replaced with a concrete example.
/// v4.3.0 (2026-05-11): Concrete worked example for the References block so the
///         model substitutes real filenames from the passages instead of echoing
///         the placeholder word.
/// v5.0.0 (2026-05-11): "Grounded Legal Document Assistant" — strict grounding
///         rules, explicit insufficient-info clause, conflict resolution,
///         silent internal reasoning. Removes the in-prompt worked example
///         (avoids hardcoded page numbers leaking into outputs).
/// v5.2.0 (2026-05-12): Instruct-only single prompt. Dropped PART 1/PART 2
///         framing and the inline `[filename.pdf, page N]` citations — the
///         frontend already renders a numbered references panel from the
///         retrieved chunks (see ChatPanel / CitationChip). The model now
///         emits compact `[N]` markers tied to the bracketed indices in the
///         context block; backend resolves N → (filename, page) via
///         extract_indexed_citations(). Removes the PART 1 anchor (was a
///         CoT-leak shield for thinking models — no longer used). Compressed
///         from ~275 → ~180 tokens. Summary budgets bumped in
///         query_router.rs (512→1280 tokens, 2048→3584 n_ctx).
///         updated CRITICAL for not hallucination.
/// v5.3.0 (2026-05-26): Replaced "explicitly present / Do not infer" with
///         "directly stated or directly implied" — the prior wording blocked
///         trivial numerical/logical deductions (e.g. "Grade 16 and above"
///         covers Grade 17) causing false "not mentioned" refusals when the
///         answer was present in the retrieved chunks.
pub const GROUNDING_SYSTEM_PROMPT: &str = "\
Grounded Document Assistant v5.4\n\n\
You answer the user's question using ONLY the retrieved passages in the Context block.\n\
NEVER use outside KNOWLEDGE, PRIOR TRAINING, ASSUMPTIONS, or INTERPRETATIONS.\n\
Examine each passage carefully before deciding it is irrelevant, IF irrelevant reply with ONLY this single line and nothing else:\n\
\"The retrieved documents do not contain enough information to answer this question.\n\
RULES\n\
- Cite every factual sentence with a bracketed source index, e.g. [1] or [2], matching the number shown in the Context block. Place the marker at the end of the sentence, before the period.\n\
- CRITICAL: Only use numbers that appear in the Context passages provided. If you see [1], [2], [3] in the context, you MUST NOT use [4] or higher. Incorrect numbering will result in your answer being rejected.\n\
- Use only information directly stated or directly implied by the passages. Direct implications include straightforward numerical ranges (e.g. \"Grade 16 and above\" covers Grade 17) and logical containment. Do not invent facts or extrapolate beyond what the text says.\n\
- Preserve all conditions, deadlines, exceptions, and limitations exactly as written.\n\
- If passages conflict, present both versions and cite both indices; do not resolve.\n\
- Use plain language, short sentences, and numbered steps where appropriate. Neutral, factual tone.\n\
- Format with Markdown: for answers longer than a short paragraph use ## headings and ### subheadings to organise topics, bullet or numbered lists for steps/conditions, and **bold** for key terms, amounts, and deadlines. Short factual answers stay as plain sentences.\n\
- Do not output a References section or list sources at the end — the UI renders them separately.\n\
- Think silently. Do not reveal reasoning or chain-of-thought. Output only the final answer.";

/// Version of the compact grounding prompt (audit-logged like the full one).
pub const GROUNDING_SYSTEM_PROMPT_COMPACT_VERSION: &str = "5.5.0-compact";

/// Compact grounding prompt for small models (`NOVIQ_PROMPT_PROFILE=compact`).
///
/// v5.5.0-compact (2026-07-06): Gemma-3-1B on prod refused answerable queries
/// under the ~530-token persona+guidance+v5.4 stack while answering the same
/// context correctly under a short instruction (live A/B against the vLLM
/// sidecar; see CHANGELOG_DEV 2026-07-06). This single prompt absorbs the
/// persona voice and the guidance refine-offer, keeps the literal `[n]` (the
/// citation-demo injection trigger in query_router), keeps the canonical
/// refusal sentence byte-identical to `INSUFFICIENT_INFO_MSG`, and demotes the
/// refusal to a last-resort final line — the v5.4 up-front refusal rule is
/// what a 1B over-triggers on.
pub const GROUNDING_SYSTEM_PROMPT_COMPACT: &str = "\
You are a Pakistan government legal assistant. Answer the officer's question using only the Context passages below.\n\n\
How to answer:\n\
1. Find the passages that mention the topic and copy their rules, numbers, and conditions exactly.\n\
2. End every factual sentence with its source index [n], e.g. [1]. Use only index numbers shown in the Context.\n\
3. Use short sentences. Use a numbered list for steps or conditions.\n\
4. After the answer, add one short line offering to refine or tailor it.\n\n\
If no passage mentions the topic, reply only: \"The retrieved documents do not contain enough information to answer this question.\"";

/// True when the operator selected the small-model prompt profile
/// (`NOVIQ_PROMPT_PROFILE=compact` in the server `.env`). Checked per call so
/// tests and the desktop app need no restart-order coupling; the env var is
/// static in production.
pub fn compact_prompt_profile() -> bool {
    std::env::var("NOVIQ_PROMPT_PROFILE")
        .map(|v| v.trim().eq_ignore_ascii_case("compact"))
        .unwrap_or(false)
}

/// The active grounding prompt for regular (non-summary) queries.
pub fn grounding_system_prompt() -> &'static str {
    if compact_prompt_profile() {
        GROUNDING_SYSTEM_PROMPT_COMPACT
    } else {
        GROUNDING_SYSTEM_PROMPT
    }
}

/// The active grounding-prompt version (audit_log / trace traceability).
pub fn grounding_prompt_version() -> &'static str {
    if compact_prompt_profile() {
        GROUNDING_SYSTEM_PROMPT_COMPACT_VERSION
    } else {
        GROUNDING_SYSTEM_PROMPT_VERSION
    }
}

#[allow(dead_code)]
pub const SUMMARY_SYSTEM_PROMPT_VERSION: &str = "2.0.0";

/// Summary/outline system prompt — used when classify_query() returns Summary.
/// v2.0.0 (2026-05-09): Citations explicitly NOT required; ends with a
/// 'Summarization mode — citations not required' footer instead of a
/// References block, per user request.
#[allow(dead_code)]
pub const SUMMARY_SYSTEM_PROMPT: &str = "\
You are a document assistant. The user wants a thorough summary or overview.\n\n\
Rules:\n\
1. Synthesise information across ALL provided passages into a coherent narrative.\n\
2. Structure with clear headings or bullet points; group related points together.\n\
3. Be comprehensive — cover every major theme, section, and key point in the passages.\n\
4. Do NOT include inline [filename, page N] citation markers — this is summary mode.\n\
5. Base the summary entirely on the passages; never use outside knowledge.\n\
6. End your response with this literal line on its own:\n\
   Summarization mode — citations not required.\n\n\
Write a detailed, well-organised summary now.";

/// F015 T028 (FR-009, research R7) — strip chain-of-thought reasoning spans
/// (`<think>…</think>`) from generated text **at the generation boundary**, so
/// every consumer (the user-facing answer AND the citation-gate input) sees only
/// the final answer. Thinking models (Qwen3, DeepSeek-R1, LFM2.5) wrap reasoning
/// in `<think>` tags; the streaming filter already suppresses them token-by-token
/// from the live `chat://token` stream, but the returned `output` string still
/// accumulates them — this is where they are removed once and for all. Tolerates
/// an unclosed/orphan tag (drops everything from an open `<think>` with no close,
/// and any stray `</think>`). Not feature-gated so it is unit-testable.
pub fn strip_reasoning_spans(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    loop {
        if let Some(open) = rest.find("<think>") {
            out.push_str(&rest[..open]);
            let after = &rest[open + "<think>".len()..];
            if let Some(close) = after.find("</think>") {
                rest = &after[close + "</think>".len()..];
            } else {
                // Unclosed think block — drop the remainder.
                break;
            }
        } else if let Some(orphan) = rest.find("</think>") {
            // Orphan close tag — drop it and keep what follows.
            rest = &rest[orphan + "</think>".len()..];
        } else {
            out.push_str(rest);
            break;
        }
    }
    out.trim().to_string()
}

/// Strip leaked chat-template / special control tokens from generated text so
/// they never reach the user or the citation gate. Covers ChatML markers
/// (`<|im_start|>`, `<|im_end|>` and any `<|…|>` control token) plus the bare
/// `<im_start>` / `<im_end>` variants some quantized models emit, and the role
/// word (`system`/`user`/`assistant`) that follows an `im_start` marker, plus
/// Mistral `[INST]`/`[/INST]` and `</s>` markers. Idempotent; not feature-gated
/// so it is unit-testable. Applied AFTER `strip_reasoning_spans`.
pub fn strip_special_tokens(text: &str) -> String {
    static RE: std::sync::OnceLock<regex_lite::Regex> = std::sync::OnceLock::new();
    let re = RE.get_or_init(|| {
        // `<|...|>` ChatML control tokens; bare `<|im_*|>`/`<im_*>` variants with
        // an optional trailing role word; Mistral `[INST]`/`[/INST]`; EOS `</s>`.
        regex_lite::Regex::new(
            r"(?i)<\|[^>]*\|>|</?im_(?:start|end)>|\[/?INST\]|</s>",
        )
        .expect("special-token regex compiles")
    });
    let cleaned = re.replace_all(text, "");
    // Drop a leading role word left over from a stripped `im_start` marker.
    static ROLE: std::sync::OnceLock<regex_lite::Regex> = std::sync::OnceLock::new();
    let role = ROLE.get_or_init(|| {
        regex_lite::Regex::new(r"(?i)^\s*(system|user|assistant)\b[:\s]*").expect("role regex compiles")
    });
    role.replace(cleaned.trim(), "").trim().to_string()
}

/// Final cleanup applied to every generated answer: drop reasoning spans, then
/// any leaked chat-template/control tokens (FR-009, research R7).
pub fn clean_model_output(text: &str) -> String {
    strip_special_tokens(&strip_reasoning_spans(text))
}

// ─── GPU layer calculation helpers (no feature gate — used in tests + real path) ─

/// Calculate how many transformer layers to offload to GPU.
/// Takes explicit model_layers and model_size_mb to allow unit testing without a real file.
pub(crate) fn calculate_gpu_layers(vram_gb: f32, model_layers: u32, model_size_mb: u64) -> u32 {
    if vram_gb < 0.1 || model_layers == 0 || model_size_mb == 0 {
        return 0;
    }
    let mb_per_layer = model_size_mb / model_layers as u64;
    if mb_per_layer == 0 {
        return model_layers;
    }
    let usable_mb = ((vram_gb * 1024.0) as u64).saturating_sub(512);
    let n = usable_mb / mb_per_layer;
    n.min(model_layers as u64) as u32
}

/// Estimate layer count from model filename (e.g. "qwen2.5-3b-..." → 36).
pub(crate) fn estimate_model_layers_from_name(filename: &str) -> u32 {
    let lower = filename.to_lowercase();
    if lower.contains("3b") { 36 }
    else if lower.contains("7b") { 32 }
    else if lower.contains("13b") { 40 }
    else if lower.contains("70b") { 80 }
    else { 32 }
}

/// Get model file size in MB and layer count from path.
fn model_stats_from_path(path: &std::path::Path) -> (u32, u64) {
    let size_mb = std::fs::metadata(path)
        .map(|m| m.len() / 1_048_576)
        .unwrap_or(2400);
    let name = path.file_name()
        .unwrap_or_default()
        .to_string_lossy();
    let layers = estimate_model_layers_from_name(&name);
    (layers, size_mb)
}

use std::sync::{Arc, Mutex};
use std::time::Duration;

use crate::services::hardware_detector::HardwareTier;

/// Choose a thread count for llama.cpp.
///
/// llama.cpp scales sub-linearly past ~physical-core count and pays
/// context-switch overhead when threads exceed available logical cores.
/// `available_parallelism()` returns logical cores; we cap at 8 because past
/// 8 the gains taper sharply and cooler runs are more responsive on laptops.
/// Override with the `ECHO_LLM_THREADS` env var (1..=N) for benchmarking.
fn pick_threads() -> i32 {
    if let Ok(v) = std::env::var("ECHO_LLM_THREADS") {
        if let Ok(n) = v.parse::<i32>() {
            if n >= 1 { return n; }
        }
    }
    let logical = std::thread::available_parallelism()
        .map(|n| n.get() as i32)
        .unwrap_or(4);
    // Use logical/2 as a proxy for physical cores on Intel hyper-threaded
    // CPUs. On 2-core HT laptops the previous .max(4) forced 4 threads,
    // making HT siblings fight over the same SIMD units (10-30% slower on
    // matmul-heavy LLM kernels). Floor at 1, cap at 8.
    (logical / 2).max(1).min(8)
}

// ─── Stub types used when the "llm" feature is disabled ──────────────────────

#[cfg(not(feature = "llm"))]
pub struct ModelSlot;

#[cfg(not(feature = "llm"))]
impl ModelSlot {
    pub fn new() -> Result<Self> { Ok(ModelSlot) }
    pub fn evict(&self) {}
    pub fn request_abort(&self) {}
    pub fn infer(&self, _path: &str, _prompt: &str, _max: u32, _tier: &HardwareTier) -> Result<String> {
        Ok("[LLM feature not compiled — enable with --features llm]".to_string())
    }
    pub fn infer_with_cancel(
        &self,
        _model_path: &str,
        _prompt: &str,
        _max_tokens: u32,
        _n_ctx: u32,
        _max_prompt_tokens: usize,
        _tier: &HardwareTier,
        _cancel: Option<tokio_util::sync::CancellationToken>,
        _app: Option<crate::compat::AppHandle>,
    ) -> Result<(String, bool)> {
        Ok(("[LLM feature not compiled]".to_string(), false))
    }
    /// Stochastic variant — accepts a sampler temperature override and a seed.
    /// When `temperature` is `None`, behaves exactly like `infer_with_cancel`.
    #[allow(clippy::too_many_arguments)]
    pub fn infer_with_cancel_temp(
        &self,
        _model_path: &str,
        _prompt: &str,
        _max_tokens: u32,
        _n_ctx: u32,
        _max_prompt_tokens: usize,
        _tier: &HardwareTier,
        _cancel: Option<tokio_util::sync::CancellationToken>,
        _app: Option<crate::compat::AppHandle>,
        _temperature: Option<f32>,
        _seed: Option<u32>,
    ) -> Result<(String, bool)> {
        Ok(("[LLM feature not compiled]".to_string(), false))
    }
    pub fn set_gpu_enabled(&self, _enabled: bool) {}
    /// F015 T029 — window info stub: returns the fallback trained length and the
    /// `ECHO_N_CTX` override (or fallback) as the resolved window.
    pub fn window_info(&self, _model_path: &str, _tier: &HardwareTier) -> Result<(u32, u32)> {
        let resolved = crate::services::assistant::n_ctx::env_override()
            .unwrap_or(crate::services::assistant::n_ctx::FALLBACK_TRAINED);
        Ok((crate::services::assistant::n_ctx::FALLBACK_TRAINED, resolved))
    }
}

// Note: infer_with_grammar is only available behind the `llm` feature — it
// takes a sampler factory that depends on `LlamaModel`. The IR builder
// (Feature 006) is itself feature-gated under `llm`, so no stub is needed.

#[cfg(not(feature = "llm"))]
pub struct VisionSlot;

#[cfg(not(feature = "llm"))]
impl VisionSlot {
    pub fn new() -> Result<Self> { Ok(VisionSlot) }
    pub fn describe_image(&self, _model: &str, _image: &str) -> Result<String> {
        Ok("[Vision feature not compiled]".to_string())
    }
}

// ─── Full implementation (requires llama-cpp-2) ───────────────────────────────

#[cfg(feature = "llm")]
mod full {
    use super::*;
    use llama_cpp_2::{
        context::params::LlamaContextParams,
        llama_backend::LlamaBackend,
        model::{params::LlamaModelParams, LlamaModel},
        sampling::LlamaSampler,
    };
    use tokio::task::JoinHandle;
    use tokio::time::sleep;
    use encoding_rs::UTF_8;

    struct LoadedModel {
        model: LlamaModel,
        model_path: String,
    }

    pub struct ModelSlot {
        backend: Arc<LlamaBackend>,
        inner: Mutex<SlotInner>,
        gpu_enabled: std::sync::atomic::AtomicBool,
        /// Set true to break out of the active infer() decode loop on next token.
        /// Checked every iteration; cleared automatically when infer() starts.
        abort_flag: std::sync::atomic::AtomicBool,
    }

    struct SlotInner {
        current: Option<LoadedModel>,
        idle_handle: Option<JoinHandle<()>>,
    }

    impl ModelSlot {
        pub fn new() -> Result<Self> {
            let backend = LlamaBackend::init().context("llama backend init")?;
            Ok(ModelSlot {
                backend: Arc::new(backend),
                inner: Mutex::new(SlotInner { current: None, idle_handle: None }),
                gpu_enabled: std::sync::atomic::AtomicBool::new(true),
                abort_flag: std::sync::atomic::AtomicBool::new(false),
            })
        }

        pub fn evict(&self) {
            if let Ok(mut g) = self.inner.lock() {
                g.current = None;
                if let Some(h) = g.idle_handle.take() { h.abort(); }
            }
            log::info!("LLM evicted");
        }

        /// Signal the active infer() decode loop to stop after the current token.
        /// The loop checks this flag each iteration; it clears the flag and breaks,
        /// releasing the inner mutex so subsequent calls can proceed normally.
        pub fn request_abort(&self) {
            self.abort_flag.store(true, std::sync::atomic::Ordering::Relaxed);
        }

        /// Toggle GPU acceleration. Evicts current model so next load picks up new setting.
        pub fn set_gpu_enabled(&self, enabled: bool) {
            self.gpu_enabled.store(enabled, std::sync::atomic::Ordering::Relaxed);
            if let Ok(mut g) = self.inner.lock() {
                g.current = None;
            }
            log::info!("GPU acceleration {}", if enabled { "enabled" } else { "disabled" });
        }

        /// F015 T029 (FR-019/FR-010) — resolve the active window for the model at
        /// `model_path`: returns `(trained_ctx, resolved_n_ctx)`. Loads (and
        /// caches) the model to read its trained context length, then runs the
        /// RAM/VRAM-aware clamp (same path generation uses). The caller derives
        /// `ram_clamped`/`source` from these + `ECHO_N_CTX`.
        pub fn window_info(&self, model_path: &str, tier: &HardwareTier) -> Result<(u32, u32)> {
            self.load(model_path, tier)?;
            let g = self.inner.lock().map_err(|_| anyhow::anyhow!("lock"))?;
            let loaded = g.current.as_ref().ok_or_else(|| anyhow::anyhow!("no model"))?;
            let trained = loaded.model.n_ctx_train();
            let resolved = crate::services::assistant::n_ctx::for_model(
                &loaded.model,
                tier.ram_gb,
                tier.vram_gb,
                tier.has_discrete_gpu,
            );
            Ok((trained, resolved))
        }

        pub fn load(&self, model_path: &str, tier: &HardwareTier) -> Result<()> {
            // Guard: llama-cpp-2 hard-panics when the file is missing — check
            // existence here so callers get a clean anyhow::Error instead.
            if !std::path::Path::new(model_path).exists() {
                anyhow::bail!(
                    "LLM model not found: {model_path} — \
                     download the model file and place it in the models/ directory"
                );
            }
            let mut g = self.inner.lock().map_err(|_| anyhow::anyhow!("lock poisoned"))?;
            if g.current.as_ref().map(|m| m.model_path.as_str()) == Some(model_path) {
                self.reset_timer(&mut g);
                return Ok(());
            }
            g.current = None;
            let gpu_on = self.gpu_enabled.load(std::sync::atomic::Ordering::Relaxed);
            // F014/T027: `NOVIQ_GPU_LAYERS` overrides auto-sizing on the headless
            // GPU box (12 GB VRAM → a hand-tuned offload count). Set it to `0` to
            // force the CPU fallback (FR-009) regardless of the detected GPU;
            // unset → keep the desktop auto-calc behavior.
            let gpu_layers_env = std::env::var("NOVIQ_GPU_LAYERS").ok();
            let gpu: u32 = match gpu_layers_env.as_deref().and_then(|v| v.trim().parse::<u32>().ok()) {
                Some(n) => n,
                None if tier.has_discrete_gpu && gpu_on => {
                    let (layers, size_mb) = model_stats_from_path(std::path::Path::new(model_path));
                    calculate_gpu_layers(tier.vram_gb, layers, size_mb)
                }
                None => 0,
            };
            log::info!(
                "GPU layers: {gpu} (gpu_enabled={gpu_on}, vram={:.1}GB, NOVIQ_GPU_LAYERS={})",
                tier.vram_gb,
                gpu_layers_env.as_deref().unwrap_or("unset")
            );
            let params = LlamaModelParams::default().with_n_gpu_layers(gpu);
            let model = LlamaModel::load_from_file(&self.backend, model_path, &params)
                .with_context(|| format!("load model: {model_path}"))?;
            g.current = Some(LoadedModel { model, model_path: model_path.to_string() });
            self.reset_timer(&mut g);
            log::info!("Loaded LLM: {model_path}");
            Ok(())
        }

        pub fn infer(&self, model_path: &str, prompt: &str, max_tokens: u32, tier: &HardwareTier) -> Result<String> {
            self.load(model_path, tier)?;
            let g = self.inner.lock().map_err(|_| anyhow::anyhow!("lock"))?;
            let loaded = g.current.as_ref().ok_or_else(|| anyhow::anyhow!("no model"))?;
            // n_batch must be ≥ the largest batch we ever submit to decode();
            // otherwise llama.cpp asserts (GGML_ASSERT n_tokens_all <= n_batch).
            // We feed the full prompt in a single batch below, so n_batch must
            // match n_ctx. n_ubatch (default 512) keeps the compute buffer
            // bounded — llama.cpp iterates internally in ubatch-sized slices.
            // n_ctx=2048 for NER/analyze/batch: analyze needs up to 928+768=1696 tokens
            // (multi-source prompts); NER needs 928+256=1184; batch needs 928+512=1440.
            // 1536 was still too tight for analyze. Chat uses infer_with_cancel (n_ctx=1024).
            let ctx_params = LlamaContextParams::default()
                .with_n_ctx(Some(std::num::NonZeroU32::new(2048).unwrap()))
                .with_n_batch(2048)
                .with_n_threads(super::pick_threads())
                .with_n_threads_batch(super::pick_threads());
            let mut ctx = loaded.model.new_context(&self.backend, ctx_params)?;
            let tokens = loaded.model.str_to_token(prompt, llama_cpp_2::model::AddBos::Always)?;
            // n_ctx=1536. Reserve 608 for response tokens; prompt gets remaining 928.
            // Previously 768 which truncated the end of the prompt (dropping the
            // <|im_start|>assistant token), causing the model to echo input instead of answering.
            const MAX_PROMPT_TOKENS: usize = 928;
            let n_prompt = tokens.len().min(MAX_PROMPT_TOKENS);
            let mut batch = llama_cpp_2::llama_batch::LlamaBatch::new(n_prompt.max(1), 1);
            for (i, t) in tokens.iter().take(n_prompt).enumerate() {
                // The last prompt token must have logits=true so the sampler can read from it.
                // All others are false (prefill only, no logits needed).
                let is_last = i == n_prompt - 1;
                batch.add(*t, i as i32, &[0], is_last)?;
            }
            ctx.decode(&mut batch)?;
            // Clear any stale abort signal from a previous timed-out call before
            // starting the decode loop, so we don't abort instantly.
            self.abort_flag.store(false, std::sync::atomic::Ordering::Relaxed);
            let mut output = String::new();
            let mut i = n_prompt;
            let mut n_generated: u32 = 0;
            let mut decoder = UTF_8.new_decoder();
            // temperature = 0.1 for determinism (AD-7).
            // penalties: look back 256 tokens (was 64 — too small to catch
            // multi-sentence repetition observed on Qwen2.5-1.5B benchmark
            // run, P2/P5/P10/P15). repeat_penalty=1.3 retained.
            let mut sampler = LlamaSampler::chain_simple([
                LlamaSampler::penalties(256, 1.3, 0.0, 0.0),
                LlamaSampler::temp(0.1),
                LlamaSampler::greedy(),
            ]);
            while n_generated < max_tokens {
                // Check abort flag — set by request_abort() when a 90s NER timer fires.
                // Clearing and breaking here releases the inner mutex cleanly so the
                // next infer() call isn't blocked by a zombie thread.
                if self.abort_flag.swap(false, std::sync::atomic::Ordering::Relaxed) {
                    log::warn!("infer() aborted by external signal after {n_generated} tokens — releasing mutex");
                    break;
                }
                let token = sampler.sample(&ctx, -1);
                if loaded.model.is_eog_token(token) { break; }
                output.push_str(&loaded.model.token_to_piece(token, &mut decoder, false, None)?);
                // Stop on ChatML turn markers — repeat_penalty can suppress the EOS
                // token when <|im_end|> appears in the prompt's prior-turn history,
                // causing the model to overshoot and reproduce the system prompt.
                if let Some(pos) = output.rfind("<|im_end|>").or_else(|| output.rfind("<|im_start|>")) {
                    output.truncate(pos);
                    break;
                }
                batch.clear();
                batch.add(token, i as i32, &[0], true)?;
                ctx.decode(&mut batch)?;
                i += 1;
                n_generated += 1;
            }
            Ok(output)
        }

        /// Cancellation-aware streaming infer.
        /// Returns (output, was_cancelled) — partial text is preserved on cancel.
        /// Emits chat://token per generated token when app handle is provided.
        ///
        /// Uses the default sampler chain (temp 0.1 + greedy) — deterministic
        /// given identical prompts. Callers that need stochasticity (e.g. the
        /// LLM IR builder retry loop) must call `infer_with_cancel_temp`.
        pub fn infer_with_cancel(
            &self,
            model_path: &str,
            prompt: &str,
            max_tokens: u32,
            n_ctx: u32,
            max_prompt_tokens: usize,
            tier: &HardwareTier,
            cancel: Option<tokio_util::sync::CancellationToken>,
            app: Option<crate::compat::AppHandle>,
        ) -> Result<(String, bool)> {
            self.infer_with_cancel_temp(
                model_path, prompt, max_tokens, n_ctx, max_prompt_tokens, tier,
                cancel, app, None, None,
            )
        }

        /// Like `infer_with_cancel` but accepts an optional temperature
        /// override and seed. When `temperature` is `Some(t)` with `t > 0`,
        /// switches the sampler chain to `penalties → top_p(0.95) → temp(t)
        /// → dist(seed)` for genuine stochasticity (the default greedy chain
        /// is deterministic — see CHANGELOG_DEV entry on the n0008 retry loop
        /// where 4 retries produced byte-identical output).
        #[allow(clippy::too_many_arguments)]
        pub fn infer_with_cancel_temp(
            &self,
            model_path: &str,
            prompt: &str,
            max_tokens: u32,
            n_ctx: u32,
            max_prompt_tokens: usize,
            tier: &HardwareTier,
            cancel: Option<tokio_util::sync::CancellationToken>,
            app: Option<crate::compat::AppHandle>,
            temperature: Option<f32>,
            seed: Option<u32>,
        ) -> Result<(String, bool)> {
            if let Some(t) = &cancel {
                if t.is_cancelled() {
                    return Ok(("".to_string(), true));
                }
            }
            self.load(model_path, tier)?;
            let g = self.inner.lock().map_err(|_| anyhow::anyhow!("lock"))?;
            let loaded = g.current.as_ref().ok_or_else(|| anyhow::anyhow!("no model"))?;
            // F015 T011: dynamic window. `n_ctx == 0` means "resolve from the
            // loaded model's trained context length, clamped to available
            // RAM/VRAM" — replacing the historical hardcoded 1024/2048/3584
            // per call-site. Explicit non-zero values still pass through, so
            // every legacy path keeps its exact behaviour. With the F015 context
            // allocator guaranteeing `prompt ≤ window` BEFORE generation, the
            // head+tail truncation below becomes an unreachable safety net.
            let n_ctx = if n_ctx == 0 {
                let hw = crate::services::hardware_detector::HardwareTier::detect();
                crate::services::assistant::n_ctx::for_model(
                    &loaded.model,
                    hw.ram_gb,
                    hw.vram_gb,
                    hw.has_discrete_gpu,
                )
            } else {
                n_ctx
            };
            let n_ctx_nz = std::num::NonZeroU32::new(n_ctx.max(64)).unwrap();
            let ctx_params = LlamaContextParams::default()
                .with_n_ctx(Some(n_ctx_nz))
                .with_n_batch(n_ctx)
                .with_n_threads(super::pick_threads())
                .with_n_threads_batch(super::pick_threads());
            let mut ctx = loaded.model.new_context(&self.backend, ctx_params)?;
            let tokens = loaded.model.str_to_token(prompt, llama_cpp_2::model::AddBos::Always)?;
            // Over-budget truncation must NEVER drop the prompt tail. The RAG
            // prompt ends with `Question: …` + the `<|im_start|>assistant` cue;
            // a naive head-only `take(max_prompt_tokens)` silently removed both
            // on long prompts (~2.4k tok vs an 1800 cap), so the model generated
            // a continuation of the context with no question and no answer cue —
            // a primary driver of confabulation. Instead keep the HEAD (system
            // prompt + start of context) AND the TAIL (question + cue), dropping
            // the MIDDLE of the context. See CHANGELOG_DEV 2026-06-06.
            let kept: Vec<_> = if tokens.len() > max_prompt_tokens {
                let tail_keep = (max_prompt_tokens / 4).max(256).min(max_prompt_tokens);
                let head_keep = max_prompt_tokens - tail_keep;
                let mut v = Vec::with_capacity(max_prompt_tokens);
                v.extend_from_slice(&tokens[..head_keep]);
                v.extend_from_slice(&tokens[tokens.len() - tail_keep..]);
                log::warn!(
                    "Prompt {} tok > cap {} — kept head {} + tail {} (dropped {} mid context tokens)",
                    tokens.len(), max_prompt_tokens, head_keep, tail_keep,
                    tokens.len() - max_prompt_tokens
                );
                v
            } else {
                tokens
            };
            let n_prompt = kept.len();
            let mut batch = llama_cpp_2::llama_batch::LlamaBatch::new(n_prompt.max(1), 1);
            for (i, t) in kept.iter().enumerate() {
                let is_last = i == n_prompt - 1;
                batch.add(*t, i as i32, &[0], is_last)?;
            }
            ctx.decode(&mut batch)?;
            let mut output = String::new();
            let mut i = n_prompt;
            let mut n_generated: u32 = 0;
            let mut decoder = UTF_8.new_decoder();
            // Default chain is deterministic (greedy). Override with a
            // stochastic chain only when the caller requested a non-zero
            // temperature — this lets the IR-builder retry loop produce
            // different output on each attempt.
            let mut sampler = match temperature {
                Some(t) if t > 0.0 => {
                    let s = seed.unwrap_or_else(|| {
                        // Fall back to a per-call wall-clock seed when none was
                        // provided. Cast to u32 from nanos modulo wrap.
                        let now = std::time::SystemTime::now()
                            .duration_since(std::time::UNIX_EPOCH)
                            .map(|d| d.subsec_nanos())
                            .unwrap_or(0);
                        now ^ ((t * 1_000_000.0) as u32)
                    });
                    LlamaSampler::chain_simple([
                        LlamaSampler::penalties(256, 1.3, 0.0, 0.0),
                        LlamaSampler::top_p(0.95, 1),
                        LlamaSampler::temp(t),
                        LlamaSampler::dist(s),
                    ])
                }
                _ => LlamaSampler::chain_simple([
                    // repeat_penalty kept low (1.05): grounded/extractive answers
                    // must reproduce the verbatim source text. A high penalty
                    // (was 1.3) penalises tokens already present in the retrieved
                    // context tail, pushing the model OFF the source wording into
                    // paraphrase/confabulation. See CHANGELOG_DEV 2026-06-06.
                    LlamaSampler::penalties(256, 1.05, 0.0, 0.0),
                    LlamaSampler::temp(0.1),
                    LlamaSampler::greedy(),
                ]),
            };
            // Streaming <think>…</think> filter: thinking models (Qwen3,
            // DeepSeek-R1, LFM2.5) emit chain-of-thought wrapped in <think>
            // tags. The post-hoc cleaner strips them before storage, but the
            // user still sees them tick by in the UI. Buffer the tail so we
            // can recognise the tag across token boundaries, and skip emit
            // while inside a think block.
            let mut in_think = false;
            let mut tail = String::new();
            while n_generated < max_tokens {
                if let Some(t) = &cancel {
                    if t.is_cancelled() {
                        // Clean reasoning spans + leaked control tokens even on
                        // cancellation (FR-009 T028).
                        return Ok((clean_model_output(&output), true));
                    }
                }
                let token = sampler.sample(&ctx, -1);
                if loaded.model.is_eog_token(token) { break; }
                let token_str = loaded.model.token_to_piece(token, &mut decoder, false, None)?;
                output.push_str(&token_str);

                tail.push_str(&token_str);
                if tail.len() > 32 {
                    let mut cut = tail.len() - 32;
                    while cut < tail.len() && !tail.is_char_boundary(cut) { cut += 1; }
                    tail.drain(..cut);
                }
                if !in_think && tail.contains("<think>") {
                    in_think = true;
                    tail.clear();
                } else if in_think && tail.contains("</think>") {
                    in_think = false;
                    tail.clear();
                } else if !in_think {
                    // Don't flash leaked chat-template control tokens into the live
                    // stream (`<|im_*|>`, bare `<im_*>`, `[INST]`, `</s>`).
                    let is_marker = token_str.contains("<|")
                        || token_str.contains("<im")
                        || token_str.contains("[INST")
                        || token_str.contains("</s>");
                    if !is_marker {
                        if let Some(a) = &app {
                            use crate::compat::Emitter;
                            let _ = a.emit("chat://token", serde_json::json!({ "token": &token_str }));
                        }
                    }
                }
                // Stop on ChatML turn markers — repeat_penalty can suppress the EOS
                // token when <|im_end|> appears in the prompt's prior-turn history,
                // causing the model to overshoot and reproduce the system prompt.
                if tail.contains("<|im_end|>") || tail.contains("<|im_start|>") {
                    if let Some(pos) = output.rfind("<|im_end|>").or_else(|| output.rfind("<|im_start|>")) {
                        output.truncate(pos);
                    }
                    break;
                }
                batch.clear();
                batch.add(token, i as i32, &[0], true)?;
                ctx.decode(&mut batch)?;
                i += 1;
                n_generated += 1;
            }
            // FR-009 T028: strip reasoning spans + any leaked chat-template
            // control tokens at the generation boundary so the user-facing answer
            // and the citation-gate input are both clean.
            Ok((clean_model_output(&output), false))
        }

        /// Grammar-constrained streaming infer. Used by the LLM IR builder
        /// (Feature 006) with an llguidance JSON-schema sampler so the output
        /// is guaranteed to be valid JSON conforming to the IR schema.
        ///
        /// `sampler_factory` is called once after the model is loaded; it
        /// receives the loaded model and returns the configured sampler. This
        /// indirection lets callers build samplers that depend on model
        /// vocab (llguidance does).
        ///
        /// Returns (output, was_cancelled). No ChatML stripping — IR builder
        /// uses Mistral instruct format which doesn't emit ChatML markers.
        pub fn infer_with_grammar<F>(
            &self,
            model_path: &str,
            prompt: &str,
            max_tokens: u32,
            n_ctx: u32,
            max_prompt_tokens: usize,
            tier: &HardwareTier,
            sampler_factory: F,
            cancel: Option<tokio_util::sync::CancellationToken>,
        ) -> Result<(String, bool)>
        where
            F: FnOnce(&LlamaModel) -> Result<LlamaSampler>,
        {
            if let Some(t) = &cancel {
                if t.is_cancelled() {
                    return Ok(("".to_string(), true));
                }
            }
            self.load(model_path, tier)?;
            let g = self.inner.lock().map_err(|_| anyhow::anyhow!("lock"))?;
            let loaded = g.current.as_ref().ok_or_else(|| anyhow::anyhow!("no model"))?;

            let n_ctx_nz = std::num::NonZeroU32::new(n_ctx.max(64)).unwrap();
            let ctx_params = LlamaContextParams::default()
                .with_n_ctx(Some(n_ctx_nz))
                .with_n_batch(n_ctx)
                .with_n_threads(super::pick_threads())
                .with_n_threads_batch(super::pick_threads());
            let mut ctx = loaded.model.new_context(&self.backend, ctx_params)?;
            let tokens = loaded.model.str_to_token(prompt, llama_cpp_2::model::AddBos::Always)?;
            let n_prompt = tokens.len().min(max_prompt_tokens);
            let mut batch = llama_cpp_2::llama_batch::LlamaBatch::new(n_prompt.max(1), 1);
            for (i, t) in tokens.iter().take(n_prompt).enumerate() {
                let is_last = i == n_prompt - 1;
                batch.add(*t, i as i32, &[0], is_last)?;
            }
            ctx.decode(&mut batch)?;

            let mut sampler = sampler_factory(&loaded.model)?;
            let mut output = String::new();
            let mut i = n_prompt;
            let mut n_generated: u32 = 0;
            let mut decoder = UTF_8.new_decoder();
            while n_generated < max_tokens {
                if let Some(t) = &cancel {
                    if t.is_cancelled() {
                        return Ok((output, true));
                    }
                }
                let token = sampler.sample(&ctx, -1);
                if loaded.model.is_eog_token(token) { break; }
                let token_str = loaded.model.token_to_piece(token, &mut decoder, false, None)?;
                output.push_str(&token_str);
                sampler.accept(token);
                batch.clear();
                batch.add(token, i as i32, &[0], true)?;
                ctx.decode(&mut batch)?;
                i += 1;
                n_generated += 1;
            }
            Ok((output, false))
        }

        fn reset_timer(&self, g: &mut SlotInner) {
            if let Some(h) = g.idle_handle.take() { h.abort(); }
            let handle = tokio::spawn(async {
                sleep(Duration::from_secs(90)).await;
            });
            g.idle_handle = Some(handle);
        }
    }

    pub struct VisionSlot {
        backend: Arc<LlamaBackend>,
    }

    impl VisionSlot {
        pub fn new() -> Result<Self> {
            let backend = LlamaBackend::init()?;
            Ok(VisionSlot { backend: Arc::new(backend) })
        }

        pub fn describe_image(&self, model_path: &str, image_path: &str) -> Result<String> {
            let params = LlamaModelParams::default();
            let model = LlamaModel::load_from_file(&self.backend, model_path, &params)?;
            let prompt = format!("[IMAGE: {image_path}]\nDescribe this image in detail:");
            let ctx_params = LlamaContextParams::default()
                .with_n_ctx(Some(std::num::NonZeroU32::new(2048).unwrap()));
            let mut ctx = model.new_context(&self.backend, ctx_params)?;
            let tokens = model.str_to_token(&prompt, llama_cpp_2::model::AddBos::Always)?;
            let mut batch = llama_cpp_2::llama_batch::LlamaBatch::new(512, 1);
            for (i, t) in tokens.iter().enumerate() { batch.add(*t, i as i32, &[0], false)?; }
            ctx.decode(&mut batch)?;
            let mut out = String::new();
            let mut i = tokens.len();
            let mut decoder = UTF_8.new_decoder();
            // Vision descriptions can be slightly warmer — 0.3 allows descriptive variety.
            let mut sampler = LlamaSampler::chain_simple([
                LlamaSampler::temp(0.3),
                LlamaSampler::greedy(),
            ]);
            loop {
                let token = sampler.sample(&ctx, -1);
                if model.is_eog_token(token) || out.len() > 1024 { break; }
                out.push_str(&model.token_to_piece(token, &mut decoder, false, None)?);
                batch.clear();
                batch.add(token, i as i32, &[0], true)?;
                ctx.decode(&mut batch)?;
                i += 1;
            }
            Ok(out)
        }
    }
}

#[cfg(feature = "llm")]
pub use full::{ModelSlot, VisionSlot};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strip_reasoning_spans_removes_closed_think_block() {
        let raw = "<think>let me reason\nover lines</think>File the appeal within 30 days [1].";
        let out = strip_reasoning_spans(raw);
        assert!(!out.contains("<think>"), "tag removed: {out}");
        assert!(!out.contains("reason"), "content removed: {out}");
        assert_eq!(out, "File the appeal within 30 days [1].");
    }

    #[test]
    fn strip_reasoning_spans_drops_unclosed_block_and_orphan_close() {
        assert_eq!(strip_reasoning_spans("<think>never closed and runs on"), "");
        // Orphan close tag — everything before it is leaked reasoning; keep what
        // follows (mirrors query_router::clean_llm_output).
        assert_eq!(strip_reasoning_spans("reasoning leaked</think>answer body"), "answer body");
        // think block mid-text with content before and after
        assert_eq!(
            strip_reasoning_spans("Apply in writing. <think>hmm</think> Cite Section 18."),
            "Apply in writing.  Cite Section 18.".trim()
        );
    }

    #[test]
    fn strip_reasoning_spans_passes_clean_text_through() {
        let raw = "Under Section 18 you apply in writing [1].";
        assert_eq!(strip_reasoning_spans(raw), raw);
    }

    #[test]
    fn strip_special_tokens_removes_chatml_and_variants() {
        // Full ChatML markers + a leading role word.
        assert_eq!(
            strip_special_tokens("<|im_start|>assistant\nUnder Section 18 you apply.<|im_end|>"),
            "Under Section 18 you apply.",
        );
        // Bare (pipe-less) variant some quantized models emit.
        assert_eq!(strip_special_tokens("answer body<im_end>"), "answer body");
        assert_eq!(strip_special_tokens("<im_start>user question"), "question");
        // Mistral instruct + EOS markers.
        assert_eq!(strip_special_tokens("[INST] hi [/INST] reply</s>"), "hi  reply");
        // Clean text is untouched.
        let clean = "Under Section 18 you apply in writing [1].";
        assert_eq!(strip_special_tokens(clean), clean);
    }

    #[test]
    fn clean_model_output_strips_reasoning_then_control_tokens() {
        let raw = "<think>let me think</think>The answer is X.<|im_end|>";
        assert_eq!(clean_model_output(raw), "The answer is X.");
    }

    #[test]
    fn test_calculate_gpu_layers_full_offload() {
        // 6 GB VRAM, 3B model (2400 MB, 36 layers) → full offload
        // usable = 6144 - 512 = 5632 MB; per_layer = 2400/36 = 66 MB; 5632/66 = 85 → capped at 36
        let layers = calculate_gpu_layers(6.0, 36, 2400);
        assert_eq!(layers, 36, "6 GB VRAM should fully offload 36-layer 3B model");
    }

    #[test]
    fn test_calculate_gpu_layers_partial_offload() {
        // 4 GB VRAM, 7B model (4100 MB, 32 layers) → partial offload
        // usable = 4096 - 512 = 3584 MB; per_layer = 4100/32 = 128 MB; 3584/128 = 28
        let layers = calculate_gpu_layers(4.0, 32, 4100);
        assert_eq!(layers, 28, "4 GB VRAM should partially offload 28/32 layers of 7B model");
    }

    #[test]
    fn test_calculate_gpu_layers_zero_vram() {
        let layers = calculate_gpu_layers(0.0, 36, 2400);
        assert_eq!(layers, 0, "zero VRAM should return 0 layers");
    }

    #[test]
    fn test_estimate_model_layers_by_name() {
        assert_eq!(estimate_model_layers_from_name("qwen2.5-3b-instruct-q4_k_m.gguf"), 36);
        assert_eq!(estimate_model_layers_from_name("qwen2.5-7b-instruct-q4_k_m.gguf"), 32);
        assert_eq!(estimate_model_layers_from_name("unknown-model.gguf"), 32);
    }
}
