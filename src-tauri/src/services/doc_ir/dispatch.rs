//! T024 / T026 / T027 / T028 — dispatcher that owns the LLM-IR import path.
//!
//! Sits between `indexer::do_index` (production) or a test driver (acceptance)
//! and the rest of the doc_ir machinery. Handles:
//!   - Cache hit short-circuit (FR-019, T050/T051).
//!   - Unprocessable short-circuit (FR-028, T027).
//!   - Builder availability + one-time "LLM unavailable" notification (FR-027, T026).
//!   - Progress events (FR-025, T028).
//!   - Validator + partial-success accounting (FR-007 / FR-009 / FR-010).
//!   - DocIr → `structure_parser::DocNode` adaptation + downstream
//!     `persist_tree` + `graph_builder::build_graph` reuse (T024 / T025).
//!   - `sources.extraction_method / ir_cache_sha / ir_quality_score /
//!     nodes_low_confidence` writeback (FR-029, FR-030).

use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::OnceLock;

use serde::Serialize;
use crate::db::Db;
use crate::compat::{AppHandle, Emitter};
use tokio_util::sync::CancellationToken;

use crate::db::repositories::{chunks as chunk_repo, graph as graph_repo};
use crate::services::doc_ir::builder::{BuilderInput, IrBuildError, IrBuilder};
use crate::services::doc_ir::cache;
use crate::services::doc_ir::progress::ProgressSink;
use crate::services::doc_ir::types::DocIr;
use crate::services::doc_ir::validator::{non_whitespace_char_count, validate_doc_ir, Severity};
use crate::services::doc_ir::{to_doc_node_tree, LLM_IR_IMPORTER_VERSION, MIN_TEXT_CHARS};
use crate::services::{graph_builder, structure_parser};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DispatchOutcome {
    /// LLM IR path completed (or replayed from cache).
    Indexed,
    /// Source text too small — flagged as `unprocessable`; caller should
    /// stop further processing.
    Unprocessable,
    /// Builder unavailable; caller should fall through to next pipeline
    /// stage (structured_importer / flat hierarchical chunking).
    SkippedBuilderUnavailable,
}

#[derive(Debug, Clone, Serialize)]
pub struct ImporterDone {
    pub source_id: String,
    pub outcome: &'static str,
    pub nodes_total: u32,
    pub nodes_ok: u32,
    pub nodes_low_confidence: u32,
    pub extraction_method: &'static str,
    pub quality_score: f64,
}

#[derive(Debug, Clone, Serialize)]
pub struct LlmUnavailable {
    pub reason: &'static str,
    pub remediation: &'static str,
}

/// Run the LLM-IR import path for `source_id`. Returns the dispatch outcome
/// so the caller knows whether to continue with fallback pipelines.
///
/// `builder` is dyn-dispatched so production can pass `MistralIrBuilder` and
/// acceptance tests can pass a stub.
pub async fn try_llm_ir_path(
    pool: &Db,
    app: &AppHandle,
    source_id: &str,
    source_sha256: &str,
    source_pdf_path: &Path,
    extracted_text: &str,
    page_count: u32,
    builder: &dyn IrBuilder,
    cancel: CancellationToken,
) -> anyhow::Result<DispatchOutcome> {
    // FR-028: too-small text → unprocessable.
    if extracted_text.chars().count() < MIN_TEXT_CHARS {
        sqlx::query(
            "UPDATE sources SET extraction_method = 'unprocessable' WHERE id = $1",
        )
        .bind(source_id)
        .execute(pool)
        .await?;
        let _ = app.emit(
            "importer://done",
            &ImporterDone {
                source_id: source_id.to_string(),
                outcome: "failed",
                nodes_total: 0,
                nodes_ok: 0,
                nodes_low_confidence: 0,
                extraction_method: "unprocessable",
                quality_score: 0.0,
            },
        );
        return Ok(DispatchOutcome::Unprocessable);
    }

    // FR-019, T050/T051: cache hit short-circuit.
    if let Some(cached) = cache::lookup_by_sha(pool, app, source_sha256, LLM_IR_IMPORTER_VERSION).await? {
        log::info!(
            "doc_ir::dispatch: cache hit for {source_sha256} — skipping LLM"
        );
        return persist_and_finalize(pool, app, source_id, source_sha256, &cached).await;
    }

    // FR-027: builder unavailable → emit one-time notification + fall back.
    if !builder.available() {
        emit_llm_unavailable_once(app, "model_missing");
        return Ok(DispatchOutcome::SkippedBuilderUnavailable);
    }

    let progress = ProgressSink::Tauri(app.clone());
    let input = BuilderInput {
        source_id,
        source_sha256,
        source_pdf_path,
        extracted_text,
        page_count,
        forced_doc_type: None,
    };

    let output = match builder.build(input, progress, cancel.clone()).await {
        Ok(o) => o,
        Err(IrBuildError::ModelUnavailable(_)) => {
            emit_llm_unavailable_once(app, "model_load_failed");
            return Ok(DispatchOutcome::SkippedBuilderUnavailable);
        }
        Err(IrBuildError::InsufficientText { .. }) => {
            return Ok(DispatchOutcome::Unprocessable);
        }
        Err(IrBuildError::Cancelled) => {
            log::info!("doc_ir::dispatch: import cancelled for {source_id}");
            return Err(anyhow::anyhow!("import cancelled"));
        }
        Err(e) => return Err(anyhow::anyhow!("LLM IR build failed: {e}")),
    };

    // Validate one more time at dispatch (defense in depth). The builder
    // already validated; we re-check the assembled IR for cross-chunk
    // structural issues that surface only at assembly time.
    let source_chars = non_whitespace_char_count(extracted_text);
    let issues = validate_doc_ir(&output.doc_ir, source_chars, Some(page_count));
    let errors = issues.iter().filter(|i| i.severity == Severity::Error).count();
    if errors > 0 && output.issues.is_empty() {
        log::warn!(
            "doc_ir::dispatch: post-assembly validation found {errors} new errors"
        );
    }

    // Persist to cache, then finalize the index.
    let _ = cache::write_doc_ir(pool, app, &output.doc_ir).await?;
    persist_and_finalize(pool, app, source_id, source_sha256, &output.doc_ir).await
}

/// Adapt the IR into the `structure_parser::DocNode` tree, persist via the
/// existing path, build the legal graph, and write `sources` provenance.
async fn persist_and_finalize(
    pool: &Db,
    app: &AppHandle,
    source_id: &str,
    source_sha256: &str,
    ir: &DocIr,
) -> anyhow::Result<DispatchOutcome> {
    // Replace existing chunks/graph for this source.
    graph_repo::delete_for_source(pool, source_id).await?;
    chunk_repo::delete_by_source(pool, source_id).await?;

    let tree = to_doc_node_tree(ir);
    let inserted = structure_parser::persist_tree(pool, source_id, &tree).await?;

    let (nodes, edges) = graph_builder::build_graph(&tree, &inserted);
    graph_repo::insert_nodes(pool, &nodes).await?;
    graph_repo::insert_edges(pool, &edges).await?;

    let nodes_total = ir.nodes.len() as i64;
    let nodes_ok = ir
        .nodes
        .iter()
        .filter(|n| matches!(n.quality_flag, crate::services::doc_ir::types::QualityFlag::Ok))
        .count() as i64;
    let nodes_low_confidence = ir
        .nodes
        .iter()
        .filter(|n| {
            matches!(
                n.quality_flag,
                crate::services::doc_ir::types::QualityFlag::LowConfidence
            )
        })
        .count() as i64;
    let quality = cache::quality_score(nodes_total as usize, nodes_ok as usize);

    let now = chrono::Utc::now().timestamp_millis();
    sqlx::query(
        "UPDATE sources
            SET extraction_method     = 'llm_ir',
                ir_cache_sha          = $1,
                ir_quality_score      = $2,
                nodes_low_confidence  = $3,
                doc_type              = $4,
                doc_type_source       = 'llm',
                structure_status      = 'structured',
                structure_importer_version = $5,
                indexing_status       = 'indexed',
                indexed_at            = $6,
                quality_score         = $7
          WHERE id = $8",
    )
    .bind(source_sha256)
    .bind(quality)
    .bind(nodes_low_confidence)
    .bind(ir.doc_type.as_str())
    .bind(LLM_IR_IMPORTER_VERSION)
    .bind(now)
    .bind(quality as f32)
    .bind(source_id)
    .execute(pool)
    .await?;

    let outcome = if nodes_low_confidence > 0 {
        "partial"
    } else {
        "ok"
    };
    let _ = app.emit(
        "importer://done",
        &ImporterDone {
            source_id: source_id.to_string(),
            outcome,
            nodes_total: nodes_total as u32,
            nodes_ok: nodes_ok as u32,
            nodes_low_confidence: nodes_low_confidence as u32,
            extraction_method: "llm_ir",
            quality_score: quality,
        },
    );

    log::info!(
        "doc_ir::dispatch: indexed source {source_id} via LLM IR — \
         {nodes_total} nodes ({nodes_ok} ok, {nodes_low_confidence} low-conf, \
         {} edges)",
        edges.len()
    );

    Ok(DispatchOutcome::Indexed)
}

/// Emit `importer://llm-unavailable` at most once per session.
fn emit_llm_unavailable_once(app: &AppHandle, reason: &'static str) {
    static EMITTED: OnceLock<AtomicBool> = OnceLock::new();
    let flag = EMITTED.get_or_init(|| AtomicBool::new(false));
    if flag
        .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
        .is_ok()
    {
        let remediation = match reason {
            "model_missing" => {
                "Place mistral-7b-instruct-v0.3.Q4_K_M.gguf in models/ to enable LLM extraction. \
                 Falling back to rule-based parser."
            }
            "model_load_failed" => {
                "Model file present but failed to load. Falling back to rule-based parser."
            }
            "hardware_tier" => {
                "Mistral 7B IR builder needs ≥12 GB RAM (Tier 3)."
            }
            _ => "Falling back to rule-based parser.",
        };
        let _ = app.emit(
            "importer://llm-unavailable",
            &LlmUnavailable { reason, remediation },
        );
    }
}
