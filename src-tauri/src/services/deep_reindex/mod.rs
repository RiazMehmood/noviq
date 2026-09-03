//! Deep re-index orchestrator (Feature 007 Slice A T050).
//!
//! On officer-initiated Deep re-index of a document: classify the PDF as a
//! Pakistan legal instrument → run the Mistral legal-IR build per chunk →
//! amendment_parser pass → xref_extractor pass → procedure_extractor pass
//! → atomic chunks-replace → re-embed. The document's `sources.index_tier`
//! flips to `'deep'` only after every pass succeeds.
//!
//! This module is a Slice-0 skeleton: `run` returns `unimplemented!()` so
//! that the Tauri command surface (Slice A T053) and the indexer dispatch
//! branch (T021) can be wired up before the pipeline body lands.

pub mod chunks_replace;

use anyhow::Result;
use serde::{Deserialize, Serialize};

/// Handle returned to the frontend when a Deep re-index is dispatched.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeepReindexHandle {
    pub doc_id: String,
    pub job_id: String,
    pub started_at: String,
}

/// Slice 0 stub. Filled in by Slice A (T050). See
/// `specs/007-legal-rag-pakistan/contracts/tauri-commands.md` C1.
pub async fn run(_doc_id: &str, _force: bool) -> Result<DeepReindexHandle> {
    anyhow::bail!("deep_reindex::run not implemented in Slice 0 — lands in Slice A T050")
}
