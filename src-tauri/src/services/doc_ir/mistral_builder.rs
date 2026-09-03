//! T020 / T021 / T022 — `MistralIrBuilder` implementation of the `IrBuilder`
//! trait, slice B (real inference).
//!
//! Pipeline per build():
//!   1. Classify doc type via short LLM call (T020) — fallback to rule-based.
//!   2. Pre-chunk source text on heading boundaries (T017).
//!   3. For each chunk: prompt = system + breadcrumb context + chunk text;
//!      sample with the llguidance IR-schema sampler (T019); parse the JSON
//!      payload; rewrite node ids to a doc-global counter; append nodes.
//!   4. Retry loop (T022): on JSON-parse or validator failure, reprompt with
//!      the validator error message; max 3 chunk retries, after which the
//!      chunk's nodes are emitted with `LowConfidence`.
//!   5. Final whole-doc validation. Return `BuilderOutput`.
//!
//! Feature-gated under `llm` (matches the grammar binding).

use std::path::{Path, PathBuf};
use std::sync::Arc;

use async_trait::async_trait;
use sha2::{Digest, Sha256};
use tokio_util::sync::CancellationToken;

use crate::services::doc_ir::builder::{
    BuilderInput, BuilderOutput, IrBuildError, IrBuilder, RetryCounts,
};
use crate::services::doc_ir::chunker::{self, Chunk};
// Grammar samplers (llguidance) are retained for future re-enable but not used
// in slice B-revised: free-text + retry is more robust against C++ asserts in
// llama-sampler when the grammar can't satisfy any token. See:
// dev-20260530-140131.log "GGML_ASSERT(cur_p.selected ... failed".
#[allow(unused_imports)]
use crate::services::doc_ir::grammar::{build_classifier_sampler, build_ir_sampler};
use crate::services::doc_ir::progress::{ImporterProgress, Phase, ProgressSink};
use crate::services::doc_ir::progress_store::{self, ChunkProgress};
use crate::services::doc_ir::prompts::{
    full_system_prompt, DOC_TYPE_CLASSIFIER_PROMPT,
};
use crate::services::doc_ir::types::{
    DocIr, DocType, QualityFlag, StructureNode,
};
use crate::services::doc_ir::validator::{
    non_whitespace_char_count, validate_doc_ir, Severity,
};
use crate::services::doc_profiles::detect_profile;
use crate::services::hardware_detector::HardwareTier;
use crate::services::llm::ModelSlot;
use crate::services::query_router::resolve_model_path;

/// Subdirectory under `models/` where IR-builder GGUFs are dropped.
/// Adding a new GGUF there + restarting the app is enough to switch model.
pub const IR_BUILDER_MODEL_DIR: &str = "models/ir_builder";

/// Mistral 7B Q4_K_M supports 32k context; we use 6144 to fit the chunker's
/// 3200-token budget + system prompt (~700) + breadcrumb (~50) + generation
/// headroom (~2200). Bumped from 4096 in slice-B-revised after the 100h ETA
/// finding on the 275-chunk Sindh PDF.
const IR_BUILDER_N_CTX: u32 = 6144;
/// Max prompt tokens (sys + breadcrumb + chunk). 700 sys + 50 crumb + 3200
/// chunk = 3950, round up to 4000 for safety.
const IR_BUILDER_MAX_PROMPT_TOKENS: usize = 4000;
/// Max tokens the model is allowed to emit per chunk. Free-text mode means
/// the model may emit a fair bit of node JSON; 2000 is enough for ~10-15
/// nodes per chunk.
const IR_BUILDER_MAX_GEN_TOKENS: u32 = 2000;

/// Classifier needs enough budget for the classifier instructions
/// (~250 tok) + document head (~600 tok) + Mistral [INST] markers.
/// Was 1024/700 — truncated the closing [/INST] and Mistral echoed the
/// document text instead of emitting JSON (see slice-B run 09:40:47 UTC).
const CLASSIFIER_N_CTX: u32 = 2048;
const CLASSIFIER_MAX_PROMPT: usize = 1800;
const CLASSIFIER_MAX_GEN: u32 = 48;

/// Per-chunk retry budget (T022).
const MAX_CHUNK_RETRIES: u32 = 3;

/// Sampler temperature per retry attempt. Attempt 0 stays at the deterministic
/// greedy default (temperature = None). Each subsequent attempt bumps the
/// stochasticity ceiling — without this, the model produced byte-identical
/// output on all 4 attempts of chunk n0008 on 2026-05-31 (see CHANGELOG_DEV).
fn temp_for_attempt(attempt: u32) -> Option<f32> {
    match attempt {
        0 => None,        // greedy + temp 0.1 (current default)
        1 => Some(0.4),
        2 => Some(0.7),
        _ => Some(0.9),
    }
}
/// Whole-doc retry budget (currently informational — chunk-level retries
/// already address cross-chunk breaks).
const MAX_DOC_RETRIES: u32 = 2;

// ────────────────────────────────────────────────────────────────────────────
// Discovery helpers
// ────────────────────────────────────────────────────────────────────────────

/// Scan the configured IR-builder directory for `*.gguf` files and return the
/// first (sorted lexicographically) absolute path. Returns `None` when the
/// directory is missing or contains no GGUF.
pub fn discover_ir_builder_gguf() -> Option<PathBuf> {
    let dir_str = resolve_model_path(IR_BUILDER_MODEL_DIR);
    discover_in(Path::new(&dir_str))
}

/// Pure variant: scan `dir` for `*.gguf` and return the lexically-first.
pub fn discover_in(dir: &Path) -> Option<PathBuf> {
    if !dir.is_dir() {
        return None;
    }
    let mut ggufs: Vec<PathBuf> = std::fs::read_dir(dir)
        .ok()?
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| {
            p.is_file()
                && p.extension()
                    .and_then(|s| s.to_str())
                    .map(|s| s.eq_ignore_ascii_case("gguf"))
                    .unwrap_or(false)
        })
        .collect();
    ggufs.sort();
    ggufs.into_iter().next()
}

/// Derive a stable `llm_model` identifier from a GGUF filename.
pub fn model_id_from_path(path: &Path) -> String {
    path.file_stem()
        .and_then(|s| s.to_str())
        .map(|s| s.to_ascii_lowercase())
        .unwrap_or_else(|| "unknown-ir-model".to_string())
}

// ────────────────────────────────────────────────────────────────────────────
// Builder
// ────────────────────────────────────────────────────────────────────────────

pub struct MistralIrBuilder {
    slot: Option<Arc<ModelSlot>>,
    tier: HardwareTier,
    /// Optional SQLite pool used to persist per-chunk progress (migration 019).
    /// When `Some`, the builder resumes from a saved checkpoint on entry and
    /// saves after every successful chunk. When `None`, builds run in-memory
    /// (used by tests / non-production callers).
    pool: Option<crate::db::Db>,
}

impl MistralIrBuilder {
    /// Production constructor — used by the indexer with the global ModelSlot.
    /// `pool` is None when no DB-backed resume is desired.
    pub fn new(slot: Arc<ModelSlot>, tier: HardwareTier) -> Self {
        Self {
            slot: Some(slot),
            tier,
            pool: None,
        }
    }

    /// Production constructor with optional pool for per-chunk progress
    /// persistence (Feature 006 chunk-progress, 2026-05-31).
    pub fn new_with_pool(
        slot: Arc<ModelSlot>,
        tier: HardwareTier,
        pool: Option<crate::db::Db>,
    ) -> Self {
        Self {
            slot: Some(slot),
            tier,
            pool,
        }
    }

    /// Default constructor for tests / dispatch fallbacks. `available()`
    /// returns false because there's no slot to run inference against.
    pub fn unconfigured() -> Self {
        Self {
            slot: None,
            tier: HardwareTier::default_for_test(),
            pool: None,
        }
    }

    pub fn resolved_model_path(&self) -> Option<PathBuf> {
        discover_ir_builder_gguf()
    }
}

impl Default for MistralIrBuilder {
    fn default() -> Self {
        Self::unconfigured()
    }
}

#[async_trait]
impl IrBuilder for MistralIrBuilder {
    fn name(&self) -> &'static str {
        "mistral_ir_builder"
    }

    fn available(&self) -> bool {
        self.slot.is_some() && self.resolved_model_path().is_some()
    }

    async fn build(
        &self,
        input: BuilderInput<'_>,
        progress: ProgressSink,
        cancel: CancellationToken,
    ) -> Result<BuilderOutput, IrBuildError> {
        let slot = self.slot.as_ref().ok_or_else(|| {
            IrBuildError::ModelUnavailable("ModelSlot not provided to MistralIrBuilder".into())
        })?;
        let model_path = self.resolved_model_path().ok_or_else(|| {
            IrBuildError::ModelUnavailable(format!(
                "no *.gguf found under {IR_BUILDER_MODEL_DIR}"
            ))
        })?;
        let model_path_str = model_path.to_string_lossy().to_string();
        let llm_model = model_id_from_path(&model_path);

        // 1. Classify (T020) ─────────────────────────────────────────────────
        emit_phase(&progress, input.source_id, Phase::Classifying, 0, 1, None);
        let doc_type = input.forced_doc_type.unwrap_or_else(|| {
            classify_doc_type(
                slot.clone(),
                &model_path_str,
                &self.tier,
                input.extracted_text,
                &cancel,
            )
            .unwrap_or_else(|e| {
                log::warn!(
                    "mistral_builder: classifier failed ({e}); falling back to rule-based detect_profile"
                );
                doctype_from_profile_name(detect_profile(input.extracted_text, None).0.doc_type)
            })
        });

        if cancel.is_cancelled() {
            return Err(IrBuildError::Cancelled);
        }

        // 2. Chunk (T017) ────────────────────────────────────────────────────
        emit_phase(&progress, input.source_id, Phase::Chunking, 0, 1, None);
        let (profile, _src) = detect_profile(input.extracted_text, Some(doc_type.as_str()));
        let chunks = chunker::chunk(input.extracted_text, profile, input.page_count);
        if chunks.is_empty() {
            return Err(IrBuildError::InsufficientText {
                chars: input.extracted_text.len(),
            });
        }

        // 2b. Resume from saved checkpoint, if any (migration 019).
        // Only accept a resume row when `total_chunks` matches the current
        // chunker output — a mismatch means the source text changed between
        // runs and the saved nodes belong to a different doc shape.
        let (start_idx, mut all_nodes, mut id_counter) =
            if let Some(pool) = self.pool.as_ref() {
                match progress_store::load(pool, input.source_id).await {
                    Ok(Some(saved))
                        if saved.total_chunks as usize == chunks.len()
                            && (saved.next_chunk_idx as usize) <= chunks.len() =>
                    {
                        log::info!(
                            "mistral_builder: resuming from chunk {} / {} \
                             (id_counter={}, prior_nodes={})",
                            saved.next_chunk_idx,
                            saved.total_chunks,
                            saved.id_counter,
                            saved.nodes.len()
                        );
                        (
                            saved.next_chunk_idx as usize,
                            saved.nodes,
                            saved.id_counter,
                        )
                    }
                    Ok(Some(saved)) => {
                        log::warn!(
                            "mistral_builder: stale progress row for {} \
                             (saved total_chunks={}, current={}) — discarding",
                            input.source_id,
                            saved.total_chunks,
                            chunks.len()
                        );
                        let _ = progress_store::clear(pool, input.source_id).await;
                        (0usize, Vec::<StructureNode>::new(), 0u32)
                    }
                    Ok(None) => (0usize, Vec::<StructureNode>::new(), 0u32),
                    Err(e) => {
                        log::warn!(
                            "mistral_builder: progress_store::load failed: {e} — starting fresh"
                        );
                        (0usize, Vec::<StructureNode>::new(), 0u32)
                    }
                }
            } else {
                (0usize, Vec::<StructureNode>::new(), 0u32)
            };

        // 3. Per-chunk generation (T021) + retry (T022) ──────────────────────
        let system_prompt = full_system_prompt(doc_type);
        let mut retry_counts = RetryCounts::default();

        let n_chunks = chunks.len();
        for (i, chunk) in chunks.iter().enumerate().skip(start_idx) {
            if cancel.is_cancelled() {
                return Err(IrBuildError::Cancelled);
            }
            emit_phase(
                &progress,
                input.source_id,
                Phase::Converting,
                i as u32 + 1,
                n_chunks as u32,
                Some(format!("converting section {}/{n_chunks}", i + 1)),
            );

            let (nodes, attempts) = generate_chunk_with_retry(
                slot.clone(),
                &model_path_str,
                &self.tier,
                &system_prompt,
                chunk,
                i,
                last_seen_id(&all_nodes),
                &mut id_counter,
                &cancel,
            )
            .await?;
            retry_counts.chunk_retries += attempts.saturating_sub(1);
            all_nodes.extend(nodes);

            // Persist progress so a crash, kill, or suspend doesn't lose work.
            if let Some(pool) = self.pool.as_ref() {
                let snapshot = ChunkProgress {
                    doc_type,
                    total_chunks: n_chunks as u32,
                    next_chunk_idx: (i as u32) + 1,
                    id_counter,
                    nodes: all_nodes.clone(),
                };
                if let Err(e) = progress_store::save(
                    pool,
                    input.source_id,
                    input.source_sha256,
                    &snapshot,
                )
                .await
                {
                    log::warn!(
                        "mistral_builder: progress_store::save failed at chunk {}: {e}",
                        i + 1
                    );
                }
            }
        }

        // 4. Assemble + validate ─────────────────────────────────────────────
        emit_phase(&progress, input.source_id, Phase::Validating, 0, 1, None);
        let doc_ir = DocIr {
            doc_id: input.source_sha256.to_string(),
            doc_type,
            title: derive_title(&all_nodes, input.source_pdf_path),
            source_pdf: input.source_pdf_path.to_string_lossy().to_string(),
            extracted_at: chrono::Utc::now(),
            llm_model,
            schema_version: 1,
            nodes: all_nodes,
            doc_metadata: serde_json::Value::Null,
        };

        let source_chars = non_whitespace_char_count(input.extracted_text);
        let issues = validate_doc_ir(&doc_ir, source_chars, Some(input.page_count));
        let errors = issues.iter().filter(|i| i.severity == Severity::Error).count();
        if errors > 0 {
            log::warn!(
                "mistral_builder: post-assembly validation has {errors} errors (will surface as low-confidence nodes downstream)"
            );
        }
        let _ = MAX_DOC_RETRIES; // not yet wired (chunk-level retry covers it)

        // Whole-doc assembly succeeded — clear the progress row so a future
        // re-index of this source doesn't think there's resumable work.
        if let Some(pool) = self.pool.as_ref() {
            if let Err(e) = progress_store::clear(pool, input.source_id).await {
                log::warn!(
                    "mistral_builder: progress_store::clear failed for {}: {e}",
                    input.source_id
                );
            }
        }

        Ok(BuilderOutput {
            doc_ir,
            issues,
            retry_counts,
        })
    }
}

// ────────────────────────────────────────────────────────────────────────────
// Classifier (T020)
// ────────────────────────────────────────────────────────────────────────────

fn classify_doc_type(
    slot: Arc<ModelSlot>,
    model_path: &str,
    tier: &HardwareTier,
    text: &str,
    cancel: &CancellationToken,
) -> anyhow::Result<DocType> {
    let head = take_head(text, 2400); // ~600 tokens of context
    let user = format!("{DOC_TYPE_CLASSIFIER_PROMPT}\n\nDocument head:\n{head}\n\n");
    // Mistral instruct template — without [INST] markers Mistral treats the
    // prompt as completion text and echoes the document back (observed in
    // round 7, dev-20260530-170011.log).
    let prompt = format!("<s>[INST] {user} [/INST]");
    let (raw, _cancelled) = slot.infer_with_cancel(
        model_path,
        &prompt,
        CLASSIFIER_MAX_GEN,
        CLASSIFIER_N_CTX,
        CLASSIFIER_MAX_PROMPT,
        tier,
        Some(cancel.clone()),
        None,
    )?;
    let json = extract_first_json_object(&raw).ok_or_else(|| {
        anyhow::anyhow!("classifier output had no JSON object; raw: {raw:?}")
    })?;
    let v: serde_json::Value = serde_json::from_str(&json)
        .map_err(|e| anyhow::anyhow!("classifier JSON parse failed: {e}; raw: {raw:?}"))?;
    let label = v
        .get("doc_type")
        .and_then(|x| x.as_str())
        .ok_or_else(|| anyhow::anyhow!("classifier output missing doc_type field"))?;
    Ok(doctype_from_str(label))
}

fn doctype_from_str(s: &str) -> DocType {
    match s {
        "constitution" => DocType::Constitution,
        "act" => DocType::Act,
        "rules" => DocType::Rules,
        "sro" => DocType::Sro,
        "contract" => DocType::Contract,
        _ => DocType::Generic,
    }
}

/// Map a rule-based DocProfile.doc_type string to our DocType.
fn doctype_from_profile_name(name: &str) -> DocType {
    match name {
        "constitution" => DocType::Constitution,
        "act" => DocType::Act,
        "rules" => DocType::Rules,
        "manual" => DocType::Generic, // manual profile = generic in IR
        _ => DocType::Generic,
    }
}

// ────────────────────────────────────────────────────────────────────────────
// Chunk generation + retry (T021/T022)
// ────────────────────────────────────────────────────────────────────────────

#[derive(Debug, serde::Deserialize)]
struct ChunkPayload {
    nodes: Vec<StructureNode>,
}

/// Try every (shape × parser) combination until one succeeds. Returns the
/// first error if all fail, so the retry loop has something useful to log.
fn try_parse_chunk(text: &str) -> Result<ChunkPayload, anyhow::Error> {
    // 1. strict serde_json — envelope shape
    if let Ok(p) = serde_json::from_str::<ChunkPayload>(text) {
        return Ok(p);
    }
    // 2. strict serde_json — bare array
    if let Ok(nodes) = serde_json::from_str::<Vec<StructureNode>>(text) {
        return Ok(ChunkPayload { nodes });
    }
    // 3. strict serde_json — single node
    if let Ok(node) = serde_json::from_str::<StructureNode>(text) {
        return Ok(ChunkPayload { nodes: vec![node] });
    }
    // 4. json5 (lenient: unquoted keys, trailing commas, comments) — envelope
    if let Ok(p) = json5::from_str::<ChunkPayload>(text) {
        return Ok(p);
    }
    // 5. json5 — bare array
    if let Ok(nodes) = json5::from_str::<Vec<StructureNode>>(text) {
        return Ok(ChunkPayload { nodes });
    }
    // 6. json5 — single node
    if let Ok(node) = json5::from_str::<StructureNode>(text) {
        return Ok(ChunkPayload { nodes: vec![node] });
    }
    // Last resort: report the strict-envelope error (most informative).
    let strict_err = serde_json::from_str::<ChunkPayload>(text).unwrap_err();
    Err(anyhow::anyhow!("{strict_err}"))
}

/// Persist a failing-chunk payload to a debug dir under temp so we can
/// inspect the FULL bytes without flooding the log. Returns the path.
fn dump_chunk_failure(attempt: u32, json_text: &str, raw: &str) -> Option<PathBuf> {
    let dir = std::env::temp_dir().join("echo-ai-debug");
    std::fs::create_dir_all(&dir).ok()?;
    let ts = chrono::Utc::now().format("%Y%m%dT%H%M%S").to_string();
    let path = dir.join(format!("chunk-fail-{ts}-attempt{attempt}.json"));
    let body = format!("=== EXTRACTED JSON ===\n{json_text}\n\n=== FULL RAW OUTPUT ===\n{raw}\n");
    std::fs::write(&path, body).ok()?;
    Some(path)
}

/// Persist a successfully-parsed chunk's nodes to the same debug dir so the
/// watcher script can show what's being built without waiting for whole-doc
/// validation (graph_nodes / chunks tables only commit at the end). Each file
/// is one chunk; filename includes timestamp + chunk seq + page so the
/// watcher can sort/tail easily.
///
/// Format: one line per node, aligned columns — no JSON parsing needed in the
/// watcher (jq is not installed on the dev box):
///   <id>  <parent>  <kind>  num=<n>  p=<page>  q=<flag>  | <text head (90 chars)>
fn dump_chunk_success(seq: usize, page_start: u32, page_end: u32, nodes: &[StructureNode]) {
    let dir = std::env::temp_dir().join("echo-ai-debug");
    if std::fs::create_dir_all(&dir).is_err() {
        return;
    }
    let ts = chrono::Utc::now().format("%Y%m%dT%H%M%S").to_string();
    let path = dir.join(format!(
        "chunk-ok-{ts}-seq{seq:04}-p{page_start}-{page_end}.txt"
    ));
    let mut body = String::with_capacity(nodes.len() * 120);
    body.push_str(&format!(
        "# chunk seq={seq} pages={page_start}-{page_end} nodes={} extracted_at={ts}Z\n",
        nodes.len()
    ));
    for n in nodes {
        let text_head: String = n
            .text
            .chars()
            .take(90)
            .map(|c| if c == '\n' || c == '\r' || c == '\t' { ' ' } else { c })
            .collect();
        let q = match n.quality_flag {
            QualityFlag::Ok => "ok",
            QualityFlag::LowConfidence => "LOW",
            QualityFlag::UserRepaired => "rep",
        };
        body.push_str(&format!(
            "{id:<6} parent={parent:<6} {kind:<11} num={num:<6} p={page:<3} q={q:<3} | {text}\n",
            id = n.id,
            parent = n.parent_id.as_deref().unwrap_or("-"),
            kind = format!("{:?}", n.kind).to_lowercase(),
            num = n.number.as_deref().unwrap_or("-"),
            page = n.page,
            text = text_head,
        ));
    }
    let _ = std::fs::write(&path, body);
}

/// Run one chunk through the LLM with retries.
/// Returns (nodes_with_rewritten_ids, attempts_made).
async fn generate_chunk_with_retry(
    slot: Arc<ModelSlot>,
    model_path: &str,
    tier: &HardwareTier,
    system_prompt: &str,
    chunk: &Chunk,
    chunk_seq: usize,
    last_id_seen: Option<String>,
    id_counter: &mut u32,
    cancel: &CancellationToken,
) -> Result<(Vec<StructureNode>, u32), IrBuildError> {
    let mut last_error: Option<String> = None;
    let mut attempts = 0u32;

    for attempt in 0..=MAX_CHUNK_RETRIES {
        attempts = attempt + 1;
        if cancel.is_cancelled() {
            return Err(IrBuildError::Cancelled);
        }
        let user = build_user_message(chunk, last_id_seen.as_deref(), last_error.as_deref());
        // Mistral instruct format — required for the model to recognise the
        // prompt as an instruction (round 7 proved free-text format fails).
        let prompt = format!("<s>[INST] {system_prompt}\n\n{user} [/INST]");
        let temperature = temp_for_attempt(attempt);
        // Seed deterministic-per-attempt so a re-run of the same doc lands on
        // the same retry trajectory (debuggability), while still being
        // different across attempts within a single run.
        let seed = temperature.map(|_| (chunk_seq as u32).wrapping_mul(1009).wrapping_add(attempt));
        let result = slot.infer_with_cancel_temp(
            model_path,
            &prompt,
            IR_BUILDER_MAX_GEN_TOKENS,
            IR_BUILDER_N_CTX,
            IR_BUILDER_MAX_PROMPT_TOKENS,
            tier,
            Some(cancel.clone()),
            None,
            temperature,
            seed,
        );
        let (raw, was_cancelled) = match result {
            Ok(v) => v,
            Err(e) => {
                last_error = Some(format!("inference error: {e}"));
                continue;
            }
        };
        if was_cancelled {
            return Err(IrBuildError::Cancelled);
        }

        // Free-text parse: strip markdown fences + find the first balanced
        // {...} block, then try to deserialize as ChunkPayload. Fall back to
        // wrapping a bare array.
        let raw_extracted = match extract_first_json_object(&raw) {
            Some(s) => s,
            None => {
                last_error = Some(format!(
                    "no JSON object in model output (len={})",
                    raw.len()
                ));
                log::warn!(
                    "mistral_builder: no JSON object in chunk output attempt {attempt}; raw head: {:?}",
                    &raw.chars().take(400).collect::<String>()
                );
                continue;
            }
        };
        // Pre-sanitize, in order:
        //   1. Convert Python/json5-style single-quoted strings to double-
        //      quoted JSON (Mistral n0008 fix, 2026-05-31).
        //   2. Escape stray control chars + unescaped `"` inside string bodies.
        let pre_converted = convert_single_quoted_strings(&raw_extracted);
        let json_text = sanitize_json_string_newlines(&pre_converted);

        // Tolerant parser: accept any shape (envelope / bare array / single
        // node) AND parser (strict serde_json / lenient json5). json5 tolerates
        // unquoted keys, trailing commas, comments — common LLM JSON quirks.
        let parsed: Result<ChunkPayload, anyhow::Error> = try_parse_chunk(&json_text);
        let payload = match parsed {
            Ok(p) => p,
            Err(e) => {
                last_error = Some(format!("JSON parse failed: {e}"));
                let dump_path = dump_chunk_failure(attempt, &json_text, &raw);
                log::warn!(
                    "mistral_builder: chunk parse failed attempt {attempt}: {e}; raw len={}; \
                     extracted head={:?}; full dump: {}",
                    raw.len(),
                    &json_text.chars().take(400).collect::<String>(),
                    dump_path
                        .as_ref()
                        .map(|p| p.display().to_string())
                        .unwrap_or_else(|| "<dump failed>".into())
                );
                continue;
            }
        };

        let nodes = rewrite_ids(payload.nodes, id_counter);
        let preview = if let (Some(first), Some(last)) = (nodes.first(), nodes.last()) {
            format!(
                "{} nodes (first={}/{:?}, last={}/{:?}, page={})",
                nodes.len(),
                first.id,
                first.kind,
                last.id,
                last.kind,
                first.page,
            )
        } else {
            "0 nodes".into()
        };
        log::info!(
            "mistral_builder: chunk parsed attempt {attempt} → {preview}"
        );
        dump_chunk_success(chunk_seq, chunk.page_start, chunk.page_end, &nodes);
        return Ok((nodes, attempts));
    }

    // Retries exhausted — emit a single LowConfidence sentinel node for this
    // chunk so the source text is still represented in the index.
    *id_counter += 1;
    let sentinel = StructureNode {
        id: format!("n{:04}", *id_counter),
        parent_id: None,
        kind: crate::services::doc_ir::types::NodeKind::Paragraph,
        number: None,
        title: None,
        text: chunk.text.clone(),
        page: chunk.page_start,
        annotations: vec![],
        footnote_refs: vec![],
        metadata: serde_json::Value::Null,
        quality_flag: QualityFlag::LowConfidence,
        quality_error: last_error,
    };
    Ok((vec![sentinel], attempts))
}

fn build_user_message(
    chunk: &Chunk,
    last_id_seen: Option<&str>,
    retry_reason: Option<&str>,
) -> String {
    let mut s = String::new();
    if let Some(reason) = retry_reason {
        // Strong, distinctive retry preamble — at sampler temp 0.1 + greedy,
        // a short retry hint produced byte-identical re-attempts (observed
        // 2026-05-31 dumps, attempt 0 == attempt 3). This longer preamble
        // perturbs the prompt enough that the model takes a different path,
        // and the explicit rule list addresses the most common failure modes
        // seen in production: single-quoted strings, unescaped `"` mid-string,
        // unknown enum values, unescaped newlines.
        s.push_str(
            "RETRY — your previous output was REJECTED.\n\
             Reason: ",
        );
        s.push_str(reason);
        s.push_str(
            "\n\nFollow these JSON rules STRICTLY this time:\n\
             - Use double quotes (\") for ALL strings. NEVER single quotes (').\n\
             - Escape every literal double quote inside a string value as \\\".\n\
             - Escape every newline inside a string value as \\n.\n\
             - `kind` must be one of: part, chapter, section, subsection, clause, sub_clause, proviso, explanation, schedule, preamble, paragraph, heading, footnote.\n\
             - Annotation `type` must be one of: insertion, substitution, omission, renumbering, proviso_added.\n\
             - Emit ONLY the JSON object. No prose. No markdown fences.\n\n\
             Re-emit the structural extraction for this chunk now.\n\n",
        );
    }
    if !chunk.breadcrumb.is_empty() {
        s.push_str("You are inside: ");
        let crumbs: Vec<String> = chunk
            .breadcrumb
            .iter()
            .map(|(lvl, num)| format!("{lvl:?} {num}"))
            .collect();
        s.push_str(&crumbs.join(" › "));
        s.push('\n');
    }
    if let Some(last) = last_id_seen {
        s.push_str(&format!(
            "Latest node id from previous chunks: {last}. Continue numbering from there.\n"
        ));
    }
    s.push_str(&format!("\nChunk text (page {}–{}):\n", chunk.page_start, chunk.page_end));
    s.push_str(&chunk.text);
    s
}

/// Rewrite per-chunk ids (n0001..) into doc-global ids continuing `id_counter`.
/// Also rewrites `parent_id` references when they point into this chunk.
fn rewrite_ids(mut nodes: Vec<StructureNode>, id_counter: &mut u32) -> Vec<StructureNode> {
    use std::collections::HashMap;
    let mut id_map: HashMap<String, String> = HashMap::new();
    for n in &nodes {
        *id_counter += 1;
        id_map.insert(n.id.clone(), format!("n{:04}", *id_counter));
    }
    for n in nodes.iter_mut() {
        if let Some(new_id) = id_map.get(&n.id) {
            n.id = new_id.clone();
        }
        if let Some(pid) = &n.parent_id {
            if let Some(new_pid) = id_map.get(pid) {
                n.parent_id = Some(new_pid.clone());
            }
            // If parent_id points outside this chunk's id_map, leave as-is —
            // it must reference a previously-emitted (already-rewritten) node.
        }
    }
    nodes
}

fn last_seen_id(nodes: &[StructureNode]) -> Option<String> {
    nodes.last().map(|n| n.id.clone())
}

/// Convert Python/json5-style single-quoted JSON string values to standard
/// double-quoted form. Mistral occasionally emits values like
/// `"text": '"Provincial Selection Board" means a Board…'` (preserved at
/// `chunk-fail-20260531T132932-attempt0.json`) — the inner unescaped `"` then
/// breaks the downstream sanitizer's stray-quote heuristic and the chunk
/// fails to parse for 4 retries (all byte-identical at temp=0.1).
///
/// Strategy: walk the input with a 3-state machine (outside / in-double /
/// in-single). Inside a single-quoted string:
///   - emit an opening `"` instead of the `'`
///   - escape every literal `"` as `\"`
///   - convert `\'` (the json5 escape for a literal apostrophe) to a bare `'`
///   - preserve every other backslash escape verbatim (`\"`, `\\`, `\n`, …)
///   - emit a closing `"` when an unescaped `'` is reached
/// Outside any string the text is untouched. Inside a double-quoted string
/// the text is untouched so `it's` apostrophes don't trigger the state flip.
fn convert_single_quoted_strings(s: &str) -> String {
    let chars: Vec<char> = s.chars().collect();
    let mut out = String::with_capacity(s.len() + 16);
    #[derive(PartialEq)]
    enum State {
        Outside,
        InDouble,
        InSingle,
    }
    let mut state = State::Outside;
    let mut esc = false;
    let mut i = 0;
    while i < chars.len() {
        let ch = chars[i];
        match state {
            State::Outside => {
                if ch == '"' {
                    state = State::InDouble;
                    out.push('"');
                } else if ch == '\'' {
                    state = State::InSingle;
                    out.push('"');
                } else {
                    out.push(ch);
                }
            }
            State::InDouble => {
                if esc {
                    out.push(ch);
                    esc = false;
                } else if ch == '\\' {
                    out.push(ch);
                    esc = true;
                } else if ch == '"' {
                    state = State::Outside;
                    out.push(ch);
                } else {
                    out.push(ch);
                }
            }
            State::InSingle => {
                if esc {
                    // We already swallowed the leading backslash. Translate
                    // json5/python escapes into the equivalent JSON form.
                    if ch == '\'' {
                        // `\'` → literal apostrophe (no escape needed in JSON
                        // strings).
                        out.push('\'');
                    } else {
                        // Preserve any other escape verbatim — `\"` stays as
                        // `\"`, `\\` stays as `\\`, `\n` stays as `\n`, etc.
                        out.push('\\');
                        out.push(ch);
                    }
                    esc = false;
                } else if ch == '\\' {
                    esc = true;
                } else if ch == '\'' {
                    state = State::Outside;
                    out.push('"');
                } else if ch == '"' {
                    // Bare double quote inside a single-quoted string body
                    // becomes an escaped double quote in the converted JSON.
                    out.push('\\');
                    out.push('"');
                } else {
                    out.push(ch);
                }
            }
        }
        i += 1;
    }
    out
}

/// Escape literal control characters (newline, carriage return, tab) AND stray
/// unescaped `"` that appear *inside JSON string values*. LLMs frequently
/// embed raw newlines inside `"text": "..."` fields (serde_json rejects as
/// malformed) AND occasionally copy a literal `"` from the source PDF without
/// escaping (observed 2026-05-31 fail-rate-50% run on chunk-2:
/// `...column 2".",` — the close-bracket of an original quote in the doc).
///
/// Strategy: walk the input tracking string-vs-not-string state. Inside a
/// string:
///   - control chars (`\n`, `\r`, `\t`) → escape.
///   - bare `"` → look ahead past whitespace; if the next non-whitespace
///     char is a structural delimiter (`,`, `}`, `]`, `:`) or EOF, treat as
///     terminator. Otherwise it's a stray inside the string body → escape.
///
/// Outside strings, the text is unchanged.
fn sanitize_json_string_newlines(s: &str) -> String {
    let chars: Vec<char> = s.chars().collect();
    let mut out = String::with_capacity(s.len() + 32);
    let mut in_str = false;
    let mut esc = false;
    let mut idx = 0;
    while idx < chars.len() {
        let ch = chars[idx];
        if in_str {
            if esc {
                out.push(ch);
                esc = false;
                idx += 1;
                continue;
            }
            match ch {
                '\\' => {
                    esc = true;
                    out.push(ch);
                }
                '"' => {
                    // Look ahead past whitespace.
                    let mut j = idx + 1;
                    while j < chars.len() && chars[j].is_whitespace() {
                        j += 1;
                    }
                    let next = chars.get(j).copied();
                    let is_terminator = match next {
                        None => true,
                        Some(c) => matches!(c, ',' | '}' | ']' | ':'),
                    };
                    if is_terminator {
                        in_str = false;
                        out.push('"');
                    } else {
                        // Stray quote inside the string body — escape it.
                        out.push('\\');
                        out.push('"');
                    }
                }
                '\n' => out.push_str("\\n"),
                '\r' => out.push_str("\\r"),
                '\t' => out.push_str("\\t"),
                _ => out.push(ch),
            }
        } else {
            if ch == '"' {
                in_str = true;
            }
            out.push(ch);
        }
        idx += 1;
    }
    out
}

/// Extract the first balanced `{...}` or `[...]` JSON value from free-text
/// model output. Strips common wrappers (```json fences, leading prose). Returns
/// `None` if no opening brace/bracket is found or braces don't balance.
fn extract_first_json_object(raw: &str) -> Option<String> {
    let bytes = raw.as_bytes();
    let mut i = 0;
    // Find first `{` or `[`.
    while i < bytes.len() && bytes[i] != b'{' && bytes[i] != b'[' {
        i += 1;
    }
    if i >= bytes.len() {
        return None;
    }
    let open = bytes[i];
    let close = if open == b'{' { b'}' } else { b']' };
    let start = i;
    let mut depth: i32 = 0;
    let mut in_str = false;
    let mut esc = false;
    while i < bytes.len() {
        let b = bytes[i];
        if in_str {
            if esc {
                esc = false;
            } else if b == b'\\' {
                esc = true;
            } else if b == b'"' {
                in_str = false;
            }
        } else if b == b'"' {
            in_str = true;
        } else if b == open {
            depth += 1;
        } else if b == close {
            depth -= 1;
            if depth == 0 {
                return Some(raw[start..=i].to_string());
            }
        }
        i += 1;
    }
    None
}

fn take_head(s: &str, max_chars: usize) -> String {
    if s.len() <= max_chars {
        return s.to_string();
    }
    let mut end = max_chars;
    while end > 0 && !s.is_char_boundary(end) {
        end -= 1;
    }
    s[..end].to_string()
}

fn derive_title(nodes: &[StructureNode], pdf_path: &Path) -> String {
    nodes
        .iter()
        .find(|n| n.title.is_some())
        .and_then(|n| n.title.clone())
        .unwrap_or_else(|| {
            pdf_path
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("Untitled")
                .to_string()
        })
}

fn emit_phase(
    sink: &ProgressSink,
    source_id: &str,
    phase: Phase,
    current: u32,
    total: u32,
    message: Option<String>,
) {
    sink.emit(&ImporterProgress {
        source_id: source_id.to_string(),
        phase,
        current,
        total,
        retries: 0,
        eta_seconds: None,
        message,
    });
}

/// Compute SHA-256 of a file as lowercase hex. Used by the indexer wiring.
pub fn sha256_file(path: &Path) -> std::io::Result<String> {
    let bytes = std::fs::read(path)?;
    let mut h = Sha256::new();
    h.update(&bytes);
    Ok(format!("{:x}", h.finalize()))
}

// ────────────────────────────────────────────────────────────────────────────
// Tests
// ────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn model_id_from_path_strips_extension_and_lowercases() {
        let p = Path::new("/x/y/Mistral-7B-Instruct-v0.3-Q4_K_M.gguf");
        assert_eq!(
            model_id_from_path(p),
            "mistral-7b-instruct-v0.3-q4_k_m".to_string()
        );
    }

    #[test]
    fn discover_in_returns_none_when_dir_missing() {
        assert!(discover_in(Path::new("/no/such/dir/ever/exists/xyz")).is_none());
    }

    #[test]
    fn discover_in_returns_none_when_dir_has_no_gguf() {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::write(tmp.path().join("README.md"), b"x").unwrap();
        std::fs::write(tmp.path().join("notes.txt"), b"x").unwrap();
        assert!(discover_in(tmp.path()).is_none());
    }

    #[test]
    fn discover_in_picks_first_gguf_sorted() {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::write(tmp.path().join("Z-last.gguf"), b"x").unwrap();
        std::fs::write(tmp.path().join("A-first.gguf"), b"x").unwrap();
        std::fs::write(tmp.path().join("README.md"), b"x").unwrap();
        std::fs::write(tmp.path().join("M-middle.gguf"), b"x").unwrap();

        let got = discover_in(tmp.path()).expect("expected a hit");
        assert_eq!(
            got.file_name().and_then(|s| s.to_str()),
            Some("A-first.gguf")
        );
    }

    #[test]
    fn discover_in_is_case_insensitive_on_extension() {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::write(tmp.path().join("model.GGUF"), b"x").unwrap();
        assert!(discover_in(tmp.path()).is_some());
    }

    #[test]
    fn unconfigured_builder_is_unavailable() {
        let b = MistralIrBuilder::unconfigured();
        assert!(!b.available());
    }

    #[test]
    fn doctype_from_str_maps_known_and_falls_back_to_generic() {
        assert_eq!(doctype_from_str("constitution"), DocType::Constitution);
        assert_eq!(doctype_from_str("act"), DocType::Act);
        assert_eq!(doctype_from_str("contract"), DocType::Contract);
        assert_eq!(doctype_from_str("zzz_unknown"), DocType::Generic);
    }

    #[test]
    fn rewrite_ids_remaps_self_and_parent_within_chunk() {
        use crate::services::doc_ir::types::{NodeKind, QualityFlag};
        let mut counter = 0u32;
        let nodes = vec![
            StructureNode {
                id: "n0001".into(),
                parent_id: None,
                kind: NodeKind::Section,
                number: Some("1".into()),
                title: None,
                text: String::new(),
                page: 1,
                annotations: vec![],
                footnote_refs: vec![],
                metadata: serde_json::Value::Null,
                quality_flag: QualityFlag::Ok,
                quality_error: None,
            },
            StructureNode {
                id: "n0002".into(),
                parent_id: Some("n0001".into()),
                kind: NodeKind::Clause,
                number: Some("a".into()),
                title: None,
                text: "leaf".into(),
                page: 1,
                annotations: vec![],
                footnote_refs: vec![],
                metadata: serde_json::Value::Null,
                quality_flag: QualityFlag::Ok,
                quality_error: None,
            },
        ];
        let rewritten = rewrite_ids(nodes, &mut counter);
        assert_eq!(counter, 2);
        assert_eq!(rewritten[0].id, "n0001");
        assert_eq!(rewritten[1].id, "n0002");
        assert_eq!(rewritten[1].parent_id.as_deref(), Some("n0001"));

        // Second chunk starts from counter=2.
        let nodes2 = vec![StructureNode {
            id: "n0001".into(),
            parent_id: Some("nXXXX".into()), // external ref — kept as-is
            kind: crate::services::doc_ir::types::NodeKind::Paragraph,
            number: None,
            title: None,
            text: "p".into(),
            page: 2,
            annotations: vec![],
            footnote_refs: vec![],
            metadata: serde_json::Value::Null,
            quality_flag: QualityFlag::Ok,
            quality_error: None,
        }];
        let rewritten2 = rewrite_ids(nodes2, &mut counter);
        assert_eq!(counter, 3);
        assert_eq!(rewritten2[0].id, "n0003");
        assert_eq!(rewritten2[0].parent_id.as_deref(), Some("nXXXX"));
    }

    #[test]
    fn extract_first_json_object_strips_markdown_fences_and_prose() {
        let s = "Here you go:\n```json\n{\"doc_type\":\"act\"}\n```\nThanks!";
        let j = extract_first_json_object(s).expect("expected hit");
        assert_eq!(j, "{\"doc_type\":\"act\"}");
    }

    #[test]
    fn extract_first_json_object_handles_nested_braces() {
        let s = r#"prefix {"nodes":[{"id":"n1"},{"id":"n2","metadata":{"k":"v"}}]} trailing"#;
        let j = extract_first_json_object(s).expect("expected hit");
        assert!(j.contains("n2"));
        assert!(j.contains("metadata"));
        assert!(j.ends_with("}]}"));
    }

    #[test]
    fn extract_first_json_object_handles_arrays() {
        let s = r#"[{"id":"x"}]"#;
        let j = extract_first_json_object(s).expect("expected hit");
        assert_eq!(j, r#"[{"id":"x"}]"#);
    }

    #[test]
    fn extract_first_json_object_ignores_braces_in_strings() {
        let s = r#"{"text":"some text with } in it","kind":"clause"}"#;
        let j = extract_first_json_object(s).expect("expected hit");
        assert_eq!(j, s);
    }

    #[test]
    fn extract_first_json_object_none_when_no_opening_brace() {
        assert!(extract_first_json_object("no json here").is_none());
    }

    #[test]
    fn sanitize_escapes_newlines_inside_strings_only() {
        let raw = "{\n\"id\":\"x\",\n\"text\":\"line1\nline2\"\n}";
        // newlines between fields stay; newline inside "text" gets escaped.
        let out = sanitize_json_string_newlines(raw);
        assert!(out.contains("\"text\":\"line1\\nline2\""));
        // structural newlines preserved (outside strings)
        assert!(out.contains("\n\"id\""));
    }

    #[test]
    fn sanitize_handles_escaped_quote_inside_string() {
        let raw = r#"{"text":"he said \"hi\""}"#;
        let out = sanitize_json_string_newlines(raw);
        assert_eq!(out, raw); // no control chars to escape
    }

    #[test]
    fn sanitize_escapes_tabs_and_crs() {
        let raw = "{\"k\":\"a\tb\rc\"}";
        let out = sanitize_json_string_newlines(raw);
        assert_eq!(out, r#"{"k":"a\tb\rc"}"#);
    }

    #[test]
    fn temp_for_attempt_schedule() {
        // Attempt 0 stays deterministic (greedy default).
        assert_eq!(temp_for_attempt(0), None);
        // Attempts 1..=3 escalate stochasticity.
        let t1 = temp_for_attempt(1).unwrap();
        let t2 = temp_for_attempt(2).unwrap();
        let t3 = temp_for_attempt(3).unwrap();
        assert!(t1 > 0.0 && t1 < t2);
        assert!(t2 < t3);
        assert!(t3 <= 1.0);
        // Beyond the budget, stay capped (defence-in-depth).
        assert_eq!(temp_for_attempt(99), Some(0.9));
    }

    #[test]
    fn convert_single_quoted_handles_n0008_actual_dump() {
        // Verbatim Mistral output that caused the 4× retry loop on
        // 2026-05-31 at 18:58 PKT (preserved at chunk-fail-…-attempt0.json).
        let raw = r#" {
  "nodes": [
    {
      "id": "n0008",
      "parent_id": "n0007",
      "kind": "sub_clause",
      "number": "(b)",
      "title": null,
      "text": '"Provincial Selection Board" means a Board constituted under rule 6-A;',
      "page": 1,
      "quality_flag": "ok"
    }
  ]
}"#;
        let converted = convert_single_quoted_strings(raw);
        // After conversion + the existing newline sanitizer, the doc must parse
        // through the tolerant chain — currently it fails.
        let sanitized = sanitize_json_string_newlines(&converted);
        let parsed: Result<ChunkPayload, _> = try_parse_chunk(&sanitized);
        assert!(
            parsed.is_ok(),
            "expected n0008 dump to parse after single-quote conversion; got: {:?}\nsanitized:\n{sanitized}",
            parsed.err()
        );
        let payload = parsed.unwrap();
        assert_eq!(payload.nodes.len(), 1);
        assert_eq!(payload.nodes[0].id, "n0008");
        assert!(
            payload.nodes[0].text.contains("Provincial Selection Board"),
            "text should preserve the original content"
        );
        // The literal double-quotes from the source PDF must survive.
        assert!(payload.nodes[0].text.contains('"'));
    }

    #[test]
    fn convert_single_quoted_leaves_double_quoted_strings_alone() {
        let s = r#"{"id":"n0001","text":"normal value"}"#;
        assert_eq!(convert_single_quoted_strings(s), s);
    }

    #[test]
    fn convert_single_quoted_basic_value() {
        let s = r#"{"k": 'hello'}"#;
        assert_eq!(convert_single_quoted_strings(s), r#"{"k": "hello"}"#);
    }

    #[test]
    fn convert_single_quoted_escapes_internal_double_quote() {
        let s = r#"{"k": 'he said "hi"'}"#;
        assert_eq!(
            convert_single_quoted_strings(s),
            r#"{"k": "he said \"hi\""}"#
        );
    }

    #[test]
    fn convert_single_quoted_unescapes_backslash_apostrophe() {
        // `\'` inside `'...'` means literal apostrophe — after converting to
        // a double-quoted string, the apostrophe needs no escape.
        let s = r#"{"k": 'don\'t'}"#;
        assert_eq!(convert_single_quoted_strings(s), r#"{"k": "don't"}"#);
    }

    #[test]
    fn convert_single_quoted_preserves_apostrophes_inside_double_quoted_strings() {
        // The `'` characters here are content, not delimiters — must not flip
        // the state machine.
        let s = r#"{"text": "it's fine"}"#;
        assert_eq!(convert_single_quoted_strings(s), s);
    }

    #[test]
    fn sanitize_escapes_stray_unescaped_quote_inside_string_value() {
        // Real failure from 2026-05-31 chunk-2: a raw `"` from the source PDF
        // appears mid-string, followed by `.` then a proper terminator.
        let dirty = r#"{"text":"column 2".","page":4}"#;
        let cleaned = sanitize_json_string_newlines(dirty);
        assert_eq!(cleaned, r#"{"text":"column 2\".","page":4}"#);
        let _: serde_json::Value =
            serde_json::from_str(&cleaned).expect("sanitized JSON must parse");
    }

    #[test]
    fn sanitize_makes_dirty_json_parse_cleanly() {
        let dirty = "{\n\"id\":\"n1\",\n\"text\":\"multi\nline\nbody\"\n}";
        let cleaned = sanitize_json_string_newlines(dirty);
        let _: serde_json::Value = serde_json::from_str(&cleaned).expect("should parse");
    }

    #[test]
    fn extract_first_json_object_none_when_unbalanced() {
        assert!(extract_first_json_object(r#"{"a":1"#).is_none());
    }

    #[test]
    fn build_user_message_includes_breadcrumb_and_last_id_and_retry() {
        use crate::services::structure_parser::NodeLevel;
        let chunk = Chunk {
            text: "body".into(),
            breadcrumb: vec![(NodeLevel::Part, "III".into()), (NodeLevel::Chapter, "1".into())],
            offset: 0,
            page_start: 12,
            page_end: 13,
        };
        let s = build_user_message(&chunk, Some("n0042"), Some("missing parent"));
        assert!(s.contains("missing parent"));
        assert!(s.contains("Part III"));
        assert!(s.contains("n0042"));
        assert!(s.contains("page 12–13"));
        assert!(s.contains("body"));
    }
}
