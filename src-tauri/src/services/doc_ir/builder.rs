//! T011 — `IrBuilder` trait + transport types.
//!
//! See specs/006-llm-pdf-importer/contracts/ir_builder_trait.md.

use std::path::Path;

use async_trait::async_trait;
use tokio_util::sync::CancellationToken;

use crate::services::doc_ir::progress::ProgressSink;
use crate::services::doc_ir::types::{DocIr, DocType};
use crate::services::doc_ir::validator::ValidationIssue;

#[async_trait]
pub trait IrBuilder: Send + Sync {
    /// Stable identifier surfaced in `DocIr.llm_model` and logs.
    fn name(&self) -> &'static str;

    /// True iff the builder is ready (model file present, hardware tier OK).
    fn available(&self) -> bool;

    /// Convert a parsed document into a `DocIr`. Streams progress and respects
    /// the cancellation token.
    async fn build(
        &self,
        input: BuilderInput<'_>,
        progress: ProgressSink,
        cancel: CancellationToken,
    ) -> Result<BuilderOutput, IrBuildError>;
}

pub struct BuilderInput<'a> {
    pub source_id: &'a str,
    pub source_sha256: &'a str,
    pub source_pdf_path: &'a Path,
    pub extracted_text: &'a str,
    pub page_count: u32,
    pub forced_doc_type: Option<DocType>,
}

#[derive(Debug)]
pub struct BuilderOutput {
    pub doc_ir: DocIr,
    pub issues: Vec<ValidationIssue>,
    pub retry_counts: RetryCounts,
}

#[derive(Debug, Clone, Copy, Default)]
pub struct RetryCounts {
    pub chunk_retries: u32,
    pub doc_retries: u32,
}

#[derive(thiserror::Error, Debug)]
pub enum IrBuildError {
    #[error("model unavailable: {0}")]
    ModelUnavailable(String),
    #[error("source text too small ({chars} chars)")]
    InsufficientText { chars: usize },
    #[error("cancelled")]
    Cancelled,
    #[error("validator exhausted retries on {failed_chunks} chunk(s)")]
    ValidationExhausted { failed_chunks: usize },
    #[error("internal: {0}")]
    Internal(#[from] anyhow::Error),
}
