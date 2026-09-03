//! Intent router public types — shared shape consumed by the frontend
//! `ChatBubble`/`Chat.tsx` components via the `chat_query` Tauri command.
//!
//! Canonical contract lives at
//! `specs/007-legal-rag-pakistan/contracts/tauri-commands.md` C4 and
//! `contracts/intent-classifier.schema.json`.
//!
//! Filled with handler logic across Slices B/C/D. Slice 0 ships the types
//! only — every type below is `Serialize + Deserialize` so the existing
//! Tauri command machinery can move these across the IPC boundary.

use crate::services::citation::LegalCitation;
use serde::{Deserialize, Serialize};

/// Eight intents plus an explicit `Fallback`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Intent {
    Lookup,
    ListChildren,
    Summarize,
    Difference,
    CrossCompare,
    ActionItems,
    Timeline,
    Procedure,
    Fallback,
}

/// Active query scope (FR-025). Discriminated union — frontend matches on
/// `kind`. `Both` is the default.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Scope {
    Both,
    Folder,
    FolderDoc { doc_id: String },
    Notebook,
    NotebookDoc { doc_id: String },
}

impl Default for Scope {
    fn default() -> Self {
        Scope::Both
    }
}

/// Slot extraction output (subset present per intent).
///
/// `section_paths` holds ALL named-provision matches in the order they
/// appear in the query — e.g. "Articles 9, 10, and 10A" becomes three
/// entries. When the dispatcher sees `section_paths.len() >= 2` it must
/// switch the Lookup handler to its multi-target prompt, because the
/// single-target anti-adjacency rule actively hurts multi-hop queries
/// (2026-06-03 Q3 regression — confirmed against prior Groq 70B traces).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Slots {
    pub instrument: Option<String>,
    pub section_paths: Vec<String>,
    pub topic: Option<String>,
    pub comparands: Vec<String>,
    pub timeline_anchor: Option<String>,
}

impl Slots {
    /// Convenience: first named section path, if any. Most intents only
    /// care about the primary target.
    pub fn section_path(&self) -> Option<&str> {
        self.section_paths.first().map(|s| s.as_str())
    }
}

/// One officer query as it enters the dispatcher.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatQuery {
    pub text: String,
    #[serde(default)]
    pub scope: Scope,
    /// Resolved from the persistent "use cloud LLM" Settings toggle (FR-029).
    /// Backend MUST route to cloud only when this is `true` and MUST never
    /// set it to `true` for any other reason.
    #[serde(default)]
    pub use_cloud: bool,
    pub thread_id: Option<String>,
}

/// Classification result — rule-first or small-LLM tiebreak.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IntentClassification {
    pub intent: Intent,
    pub confidence: f32,
    pub slots: Slots,
}

/// Top-level response. One `chat_query` may produce several bubbles
/// (Difference / CrossCompare split into per-comparand bubbles).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatResponse {
    pub intent: Intent,
    pub intent_confidence: f32,
    pub bubbles: Vec<ChatBubble>,
    pub upgrade_prompt: Option<UpgradePrompt>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatBubble {
    pub headline: String,
    pub body: String,
    pub citations: Vec<Citation>,
    pub see_also: Vec<Citation>,
    pub flow: Option<FlowDiagram>,
}

/// Wire-format citation. Wraps `LegalCitation` and pre-renders both forms
/// via `services::citation` (constitution §XI — single canonical formatter).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Citation {
    pub officer_format: String,
    pub short_format: String,
    pub doc_id: String,
    pub node_id: Option<String>,
    pub page: Option<u32>,
}

impl Citation {
    /// Build a wire-format citation from a `LegalCitation` + the source
    /// document/page anchors. Always routes through
    /// `services::citation::LegalCitation::format_*` — never construct the
    /// `officer_format` / `short_format` strings inline anywhere else.
    pub fn from_legal(
        legal: &LegalCitation,
        doc_id: String,
        node_id: Option<String>,
        page: Option<u32>,
    ) -> Self {
        Citation {
            officer_format: legal.format_officer(),
            short_format: legal.format_short(),
            doc_id,
            node_id,
            page,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpgradePrompt {
    pub doc_ids: Vec<String>,
    pub reason: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FlowDiagram {
    pub process_name: String,
    pub steps: Vec<FlowStep>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FlowStep {
    pub order: u32,
    pub actor: String,
    pub action: String,
    pub condition: Option<String>,
    pub timeline: Option<String>,
    pub citation: Citation,
    /// Conditional branches hanging off this step. Empty by default; populated
    /// when the officer expands a step via the FlowDiagram UI (Slice D T081).
    pub branches: Vec<FlowStep>,
}
