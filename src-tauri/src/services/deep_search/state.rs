//! Feature 026 (Part 1) — the serializable state of one deep-search turn.
//!
//! The bounded loop is modelled as a small graph whose progress is a single
//! [`DeepState`] value, checkpointed after each [`Node`] completes (see
//! `checkpoint.rs`). Because the phases are strictly linear, RESUME is just
//! "load the last state, skip every phase already completed" (`Node` is ordered,
//! so `state.node < Node::Foo` means "phase Foo has NOT run yet"). TIME-TRAVEL
//! loads an *earlier* checkpoint and re-runs forward, branching via `parent_id`.

use serde::{Deserialize, Serialize};

use super::{CoverageItem, DeepTrace, PassageSet, SubQuestion};

/// The completion marker for each super-step of the loop. Ordered by declaration
/// so `state.node < Node::Expanded` reads as "expand has not completed yet". A
/// fresh turn starts at `Start`; a fully-finished turn ends at `Done`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum Node {
    Start,
    Expanded,
    RetrievedR1,
    CoverageChecked,
    RetrievedR2,
    Synthesized,
    Done,
}

impl Node {
    /// Stable text for the `deep_checkpoint.node` column / the trace.
    pub fn as_str(self) -> &'static str {
        match self {
            Node::Start => "start",
            Node::Expanded => "expand",
            Node::RetrievedR1 => "retrieve_r1",
            Node::CoverageChecked => "coverage",
            Node::RetrievedR2 => "retrieve_r2",
            Node::Synthesized => "synthesize",
            Node::Done => "done",
        }
    }
}

/// Everything needed to (a) run the rest of the loop and (b) reconstruct the
/// final answer — persisted verbatim as `deep_checkpoint.state_json`. The inputs
/// (`query_text`, `system_prompt`, `scope_source_ids`) are stored so a resume
/// after a process restart needs nothing but the checkpoint row.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeepState {
    /// `conversation_id + ':' + turn_seq` — one deep turn's thread.
    pub thread_id: String,
    pub query_text: String,
    pub system_prompt: String,
    pub scope_source_ids: Option<Vec<String>>,
    /// Last completed node (drives the resume skip-guards).
    pub node: Node,
    pub subs: Vec<SubQuestion>,
    pub passages: PassageSet,
    pub coverage: Vec<CoverageItem>,
    /// Sub-question ids the coverage-check left pending → round 2 targets.
    pub pending: Vec<u8>,
    /// The synthesized (sanitized) answer, once produced. Citations are rebuilt
    /// deterministically from `passages` on finalize, so they need no storage.
    pub answer: String,
    pub trace: DeepTrace,
}

impl DeepState {
    /// A brand-new turn, before any node has run.
    pub fn fresh(
        thread_id: String,
        query_text: String,
        system_prompt: String,
        scope_source_ids: Option<Vec<String>>,
    ) -> Self {
        Self {
            thread_id,
            query_text,
            system_prompt,
            scope_source_ids,
            node: Node::Start,
            subs: Vec::new(),
            passages: PassageSet::default(),
            coverage: Vec::new(),
            pending: Vec::new(),
            answer: String::new(),
            trace: DeepTrace::default(),
        }
    }
}
