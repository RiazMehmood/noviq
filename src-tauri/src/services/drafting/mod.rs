//! Feature 009 — LGD Summary & Note Drafting.
//!
//! Composition over existing services: corpus retrieval (`query_router`), the
//! deterministic citation selector (`intent_router::decompose`, ADR-0008), the
//! indexer/file-watcher, and the exporters. This module owns only the drafting
//! orchestration + the template slot-schema store.
//!
//! Pipeline: template (slot schema) + intent → propose doc set → officer curates
//! (= scope) → gap detection → render blocks (cited = verbatim+citation, fact =
//! extractive, prose = number-free, flagged = refuse-or-flag) → export.

pub mod slot;
pub mod template;
pub mod gold;
pub mod ingest; // T023: uploaded format → proposed slot schema
pub mod docset; // T028: intent → candidate/confirmed corpus doc set
pub mod gaps; // T032: required-source category → present/missing detection
pub mod render; // T015–T018: fill slots + assemble blocks
pub mod export; // T020: serialise DraftOutput to text/markdown + write to disk
pub mod session; // T044: persistent, revisable draft session (US5)
pub mod chat; // Phase A: turn → action parser (conversational session)
pub mod conversation; // Phase A: durable chat transcript (append + load)
pub mod one_ask; // Feature 010: one-ask drafting pipeline (ADR-0013)
pub mod retrieval; // Feature 010: single vec0 KNN seam (ADR-0014, R1)

pub use slot::{Slot, SlotKind};
pub use template::{TemplateHeader, TemplateSchema};

use serde::{Deserialize, Serialize};

/// Role of a corpus document within a draft (FR-008): rules supply citations,
/// case documents supply facts. Heuristic + officer-correctable (research R2).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DocRole {
    Rules,
    Case,
}

/// A reference to an existing indexed document (a view, not a new table).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CorpusDocRef {
    pub doc_id: String,
    pub title: String,
    pub source_path: String,
    pub role: DocRole,
}

/// Officer intent captured before document proposal (FR-017).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct IntentAnswers {
    /// "summary" | "note".
    pub instrument_type: String,
    pub addressee: String,
    pub purpose: String,
    pub scheme: String,
}

/// An uncovered required-source category for a cited slot (FR-019, two-tier —
/// Phase E). Coverage is judged against document CONTENT (semantic match of the
/// category vs indexed chunks), never titles (research R11).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Gap {
    pub slot_ordering: i64,
    pub required_source_category: String,
    #[serde(flatten)]
    pub tier: GapTier,
}

/// The two-tier gap shape (FR-019) + the officer-supplied fact channel (FR-008a).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", rename_all_fields = "camelCase", tag = "tier")]
pub enum GapTier {
    /// Covered by indexed document(s) NOT yet in the confirmed set — reported as
    /// an add-this-document suggestion the officer accepts in one turn.
    Suggestion { suggested_docs: Vec<CorpusDocRef> },
    /// Covered by NO indexed document — the true missing-document gap, with the
    /// existing watched corpus folder the officer can drop the doc into.
    Missing { destination_folder: String },
    /// A `fact:` slot with no covering case document — case-specific facts live
    /// in no corpus; the officer points to a case doc or types the value
    /// (rendered with an explicit "officer-supplied" attribution, FR-008a).
    OfficerInput,
}

/// Per-gap officer decision (FR-020/021 + Phase E FR-008a/FR-019a).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", tag = "decision")]
pub enum GapResolution {
    /// A document was supplied and indexed for this category.
    Supplied { new_doc_id: String },
    /// Officer accepted a tier-a suggestion; the docs joined the confirmed set.
    AcceptedSuggestion { doc_ids: Vec<String> },
    /// Officer typed the fact value directly (fact slots only). Rendered with an
    /// explicit "officer-supplied" attribution — never as a citation; the model
    /// never generates or alters it (FR-008a).
    OfficerSupplied { value: String },
    /// Officer declared the document unavailable; affected slot will be flagged.
    ProceedWithout,
}

/// One rendered block of a draft (FR-002/003). Serialised with a `type` tag for
/// the frontend (camelCase, matches contracts/drafting-commands.md).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", tag = "type")]
pub enum Block {
    Boilerplate {
        label: Option<String>,
        text: String,
    },
    /// Verbatim quote + deterministic hierarchical citation (ADR-0008).
    Cited {
        label: Option<String>,
        quote: String,
        citation: String,
    },
    /// Extractive value + case-document attribution (FR-008).
    Fact {
        label: Option<String>,
        value: String,
        attribution: String,
    },
    /// Model-written connective narrative; contains no number/citation (FR-007).
    Prose {
        label: Option<String>,
        text: String,
    },
    /// Refuse-or-flag: unanchored cited slot (FR-006).
    Flagged {
        label: Option<String>,
        reason: String,
    },
}

/// The produced instrument: ordered blocks, one per slot.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DraftOutput {
    pub blocks: Vec<Block>,
}

/// The proposed document set for a draft (US3 `start_draft` output). The officer
/// curates `candidate_docs` (add/remove/retag) and confirms via `update_doc_set`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DraftSession {
    pub draft_id: String,
    pub candidate_docs: Vec<CorpusDocRef>,
}

/// Result of curating the working set (US3 `update_doc_set` output). Gap logic is
/// a placeholder here (empty) and is filled by US4 (`gaps::detect`).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DocSetUpdate {
    pub gaps: Vec<Gap>,
}
