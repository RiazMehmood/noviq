//! Feature 015 — Unified Bureaucrat Chat Assistant.
//!
//! One intent-routed conversational surface. Every capability shares four
//! invariants: the bureaucrat persona, the verbatim `[CITE:n]` citation gate,
//! the real-token **context allocator** (`prompt ≤ window` guaranteed before
//! generation — so the `llm.rs` head+tail truncation never fires), and
//! deterministic-where-truth-is-structural.
//!
//! This module is built incrementally per `specs/015-bureaucrat-chat/tasks.md`.
//! Phase 2 foundational spine lands first; capabilities ride on top.

/// T006/T007 — real-token priority context allocator (FR-010..FR-018).
pub mod context_budget;
/// T005/T006 — dynamic `n_ctx` resolver + real-token counter (FR-010).
pub mod n_ctx;
/// T009 — model-independent `[CITE:n]` verbatim citation gate (FR-006/007).
pub mod citation_gate;

/// S2 — output-side guard against prompt disclosure and instruction echo
/// (red-team 2026-08-05). Sibling of `citation_gate`: both check what the model
/// actually produced rather than trusting it to follow instructions.
pub mod disclosure_gate;
/// T010 — deterministic capability classification + document-scope parse (FR-005b).
pub mod intent;
/// T012 — persona + per-capability prompt loading (Constitution VI).
pub mod persona;
/// T013 — conversation memory base for the allocator (FR-017/FR-018).
pub mod memory;
/// T016–T019 — per-capability handlers (Phase 3 lands guidance; later phases
/// add draft/summarize/compare/conflict/amendment-chain/action-items).
pub mod capabilities;
/// T032 — map-reduce driver for whole-/cross-document capabilities (FR-005d).
pub mod mapreduce;

use context_budget::{allocate, BudgetInputs, BudgetPlan, TokenCounter};
use intent::Capability;
use memory::MemoryContext;
use persona::PersonaConfig;
use serde::{Deserialize, Serialize};

// ─── Turn response shape (contracts/assistant-commands.md) ───────────────────

/// One resolved citation surfaced to the officer.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct CitationOut {
    pub n: i64,
    /// Clean document display name (no ordering prefix / extension).
    pub instrument: String,
    /// Provision label — Rule/Article/Section (+ sub-clause), e.g. `Rule 16(1)(b)`.
    pub path: String,
    pub verbatim: String,
    /// Page in the source document the cited passage appears on, when known.
    pub page: Option<i64>,
    /// The chunk this citation was drawn from, so the UI can open the source at
    /// that passage on click (the NotebookLM behaviour). `None` for citations
    /// that are not backed by a single chunk — a whole-document summary, for
    /// instance — and the UI simply does not linkify those.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub chunk_id: Option<String>,
    /// The document the chunk belongs to, so the viewer knows what to open.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_id: Option<String>,
    /// F027 (FR-016) — where this passage came from: `"project"` (a document the
    /// user put in their project) or `"corpus"` (enacted law from the shared
    /// legal corpus).
    ///
    /// Only set on project turns, where an answer can draw on both at once. A
    /// reader must never mistake their own uploaded draft for enacted law —
    /// that is exactly the failure Constitution XI (Citation Discipline) exists
    /// to prevent, and mixing the two in one answer is a new way to cause it.
    ///
    /// `None` outside a project, where every citation is corpus law already and
    /// a label would be noise.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provenance: Option<String>,
}

/// Short intro shown for greeting / "introduce yourself" turns. Officer-facing
/// (Principle X); no citations (nothing is retrieved). Ends with a follow-up
/// question and a few concrete example actions.
const NOVIQ_INTRO: &str = "\
I'm **Noviq**, an AI assistant for Pakistan government officers working with legal \
instruments — the Constitution, Acts, Ordinances, Rules, Regulations, and Policies. \
I answer strictly from your indexed documents, so every substantive answer is grounded \
and cited.\n\n\
Here's how I can help:\n\
- **Find & explain a provision** — e.g. \"What are the penalties under the E&D Rules 1973?\"\n\
- **Summarise a whole document** — e.g. \"Summarize the SPPRA Rules 2010\"\n\
- **Trace links & amendments** — e.g. \"Which rules are made under the Local Government Act 2013?\"\n\
- **Scope to one document** — type **@** and pick a document to ask about only that instrument.\n\n\
**What can I help you with?**";

/// Short nudge for a content-free turn (a stray character, "test", digits, or
/// punctuation) that carries no answerable question. Steers the officer to a real
/// query without spending an inference pass. See `intent::is_low_signal`.
const NOVIQ_NUDGE: &str = "\
I didn't catch a question there. Ask me about a legal instrument in your indexed \
documents — for example:\n\
- \"What are the penalties under the E&D Rules 1973?\"\n\
- \"Summarize the SPPRA Rules 2010\"\n\
- Type **@** to scope your question to a single document.\n\n\
**What would you like to know?**";

/// Turn a stored file name into a clean display name for citations: drop the
/// extension, strip a leading ordering prefix (`01_`, `09 - `), and replace
/// underscores with spaces. `01_Sindh_Rules_of_Business_1986.pdf`
/// → `Sindh Rules of Business 1986`.
pub fn clean_document_name(file_name: &str) -> String {
    let mut s = file_name.trim();
    // Drop a short alphanumeric extension (.pdf/.docx/.txt/.md/…).
    if let Some(dot) = s.rfind('.') {
        let ext = &s[dot + 1..];
        if (1..=4).contains(&ext.len()) && ext.chars().all(|c| c.is_ascii_alphanumeric()) {
            s = &s[..dot];
        }
    }
    // Strip a leading numeric ordering prefix like "01_", "9 - ", "12.".
    let digits = s.chars().take_while(|c| c.is_ascii_digit()).count();
    let body = if digits > 0 {
        let rest = s[digits..].trim_start_matches([' ', '_', '-', '.']);
        if rest.is_empty() { s } else { rest }
    } else {
        s
    };
    body.replace('_', " ").split_whitespace().collect::<Vec<_>>().join(" ")
}

/// An ungrounded claim that was flagged (not shipped as fact) with an offer to
/// index the missing document (FR-007).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct FlagOut {
    pub missing: String,
    pub offer_to_index: bool,
}

/// Window-utilization notice (FR-018a): how full the context window is and
/// whether the officer should be nudged to a fresh chat.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ContextFill {
    pub utilization: f32,
    pub notice: bool,
}

/// The auditable budget plan for a turn (FR-010..FR-016), surfaced on the
/// response so the orchestrator can emit it into the per-turn query trace (T047)
/// — the proof that `Σ prompt + gen_reserve ≤ n_ctx` and that law was kept whole.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct TurnAudit {
    pub n_ctx: usize,
    pub gen_reserve: usize,
    pub prompt_tokens: usize,
    pub kept_law: usize,
    pub law_overflow: bool,
    pub kept_turns: usize,
    pub summary_included: bool,
    pub must_fit_failed: bool,
}

impl TurnAudit {
    fn from_plan(plan: &BudgetPlan) -> Self {
        Self {
            n_ctx: plan.n_ctx,
            gen_reserve: plan.gen_reserve,
            prompt_tokens: plan.prompt_tokens,
            kept_law: plan.kept_law,
            law_overflow: plan.law_overflow,
            kept_turns: plan.kept_turns,
            summary_included: plan.summary_included,
            must_fit_failed: plan.must_fit_failed,
        }
    }
    /// The core invariant, recomputed from the audited numbers.
    pub fn fits(&self) -> bool {
        self.prompt_tokens + self.gen_reserve <= self.n_ctx
    }
}

/// The full response for one assistant turn.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TurnResponse {
    pub conversation_id: String,
    /// The persisted `chat_message.id` of this assistant turn — the handle the
    /// `export_draft` command (T027) uses to render a produced draft to text.
    /// Empty until the orchestrator persists the turn and stamps it.
    pub message_id: String,
    /// The query-trace id for this turn (Track 3) — the handle the admin
    /// "Debug this answer" affordance uses to fetch the step-by-step trace.
    /// Empty until `run_turn` stamps it after writing the trace.
    pub trace_id: String,
    pub capability: String,
    pub answer: String,
    pub citations: Vec<CitationOut>,
    /// `Some("tailor")` when the guidance answer invites tailoring (FR-004).
    pub offer: Option<String>,
    pub flags: Vec<FlagOut>,
    pub context_fill: ContextFill,
    /// Budget audit for the turn (T047) — emitted into the query trace.
    pub audit: TurnAudit,
    /// scope-routing 2026-07: retrieval for this turn was scoped to the chat's
    /// live attachment by default (narrow-first). The UI shows a "Search all
    /// documents" widen button when true. `#[serde(default)]` so older/desktop
    /// callers that don't set it still deserialize.
    #[serde(default)]
    pub scoped_to_attachment: bool,
    /// Web results retrieved for this turn, when the officer switched web search
    /// on (2026-07-29). Empty otherwise.
    ///
    /// Returned SEPARATELY from `citations` and rendered by the UI in its own
    /// section, deliberately: the first cut relied on the model to surface web
    /// sources in its prose, and the model — correctly following "answer from
    /// the law first, do not pad" — simply did not mention them, so switching
    /// web search on produced no visible evidence it had run (trace 9059103b).
    /// A feature whose output depends on the model deciding it is worth
    /// mentioning cannot be verified. These are shown deterministically.
    ///
    /// They are NOT citations and must never be rendered as such: they are
    /// unverified web sources, and `CitationOut` means an indexed instrument.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub web_results: Vec<crate::services::websearch::WebResult>,
}

/// F027 (FR-016) — stamp each citation as the user's own document or as law.
///
/// Runs once per project turn, after the capability has produced its citations,
/// because this is the layer that knows which project the turn ran in — the
/// capability handlers receive only chunks.
///
/// **Why it matters more than it looks.** A project answer with the corpus
/// included draws on two things a reader must never confuse: a draft the officer
/// uploaded, and enacted law. Constitution XI exists to stop paraphrased legal
/// text circulating without provenance, and mixing sources in one answer is a
/// new way to break it — a citation that reads "Section 9" is worthless if the
/// reader cannot tell whether it is the Procurement Rules or their own contract.
///
/// Membership decides, not ownership: a document in the project is `"project"`
/// even when the underlying `sources` row belongs to the user who shared it.
pub async fn label_citation_provenance(
    pool: &crate::db::Db,
    project_id: &str,
    citations: &mut [CitationOut],
) {
    let members: Vec<String> = match sqlx::query_scalar(
        "SELECT source_id FROM project_source WHERE project_id = $1",
    )
    .bind(project_id)
    .fetch_all(pool)
    .await
    {
        Ok(v) => v,
        // A failed lookup must not mislabel law as the user's own document, so
        // leave every label unset rather than guess.
        Err(_) => return,
    };
    let members: std::collections::HashSet<&str> =
        members.iter().map(String::as_str).collect();

    for c in citations.iter_mut() {
        let in_project = c
            .source_id
            .as_deref()
            .map(|id| members.contains(id))
            .unwrap_or(false);
        c.provenance = Some(if in_project { "project" } else { "corpus" }.to_string());
    }
}

impl TurnResponse {
    /// F027 (FR-012) — the answer for a project that has nothing to search.
    ///
    /// A project holding no ready documents must SAY so. It must never fall
    /// through to the shared corpus or to the user's other documents, which is
    /// the most likely way corpus leakage would be discovered late: an empty
    /// project that answers at all is answering from somewhere it should not.
    ///
    /// ⚠️ This is a refusal returned with HTTP 200, like every other refusal on
    /// this system. Tests must assert on the body, never the status code.
    pub fn nothing_to_search(conversation_id: &str, pending: i64) -> Self {
        let answer = if pending > 0 {
            format!(
                "This project's documents are still being processed ({pending} pending), so \
                 there is nothing to search yet. Once they finish indexing, ask again."
            )
        } else {
            "This project has no documents yet, so there is nothing to search. Add documents \
             to the project and ask again."
                .to_string()
        };
        Self {
            conversation_id: conversation_id.to_string(),
            message_id: String::new(),
            trace_id: String::new(),
            capability: "guidance".to_string(),
            answer,
            citations: Vec::new(),
            offer: None,
            flags: Vec::new(),
            context_fill: ContextFill { utilization: 0.0, notice: false },
            audit: TurnAudit::default(),
            scoped_to_attachment: false,
            web_results: Vec::new(),
        }
    }

    /// 2026-08-12 — the interim answer for a cross-document **conflict** or
    /// **compare** question, while that capability is being rebuilt onto Deep
    /// search.
    ///
    /// WHY THIS EXISTS. Both capabilities used to run `run_mapreduce_capability`,
    /// which since 2026-07-06 sweeps only the doubled-K *retrieval slice* — on a
    /// live Constitution-vs-SLGA question that was **62 provisions of the two
    /// documents' 2,083 chunks (~3%)**. The model correctly answered "not enough
    /// information", which reads to an officer as a broken product: they named two
    /// documents, waited, and got nothing. A relevance slice cannot answer a
    /// relational question, because a conflict can sit anywhere and there is no
    /// topical signal to rank on (trace `a8ab8eac`, CHANGELOG 2026-08-12 (b)).
    ///
    /// So: say plainly that it is not built yet, rather than attempt it and fail.
    /// An honest "not yet" is a better product than a confident-looking refusal.
    ///
    /// ⚠️ The copy deliberately does NOT tell the officer to switch Deep search
    /// on. Deep search does not implement these capabilities **today** (it is
    /// wired into `guidance` only, and is admin-gated besides), so sending them
    /// there would be a dead end that looks even more broken. It names Deep search
    /// as where the feature is going, not as a workaround available now.
    ///
    /// ⚠️ Returned with HTTP 200, like every other refusal here. Assert on the
    /// body, never the status code.
    pub fn cross_document_unavailable(conversation_id: &str, cap: Capability) -> Self {
        let (what, capability) = match cap {
            Capability::Conflict => ("check two documents against each other for conflicts", "conflict"),
            _ => ("compare two documents side by side", "compare"),
        };
        let answer = format!(
            "I can't {what} yet.\n\n\
             Answering that properly means reading both documents in full, not just the \
             passages that look most relevant to the question — so it is being built as part \
             of **Deep search**. That work is still in development.\n\n\
             In the meantime I can answer about each document on its own — for example what a \
             particular section or article says, or what one of them provides on a specific \
             topic — and every answer comes with citations you can open."
        );
        Self {
            conversation_id: conversation_id.to_string(),
            message_id: String::new(),
            trace_id: String::new(),
            capability: capability.to_string(),
            answer,
            citations: Vec::new(),
            offer: None,
            flags: Vec::new(),
            context_fill: ContextFill { utilization: 0.0, notice: false },
            audit: TurnAudit::default(),
            scoped_to_attachment: false,
            web_results: Vec::new(),
        }
    }
}

/// 2026-08-12 — how `Conflict` / `Compare` turns are served while the Deep-search
/// implementation is pending.
///
/// * `notice` (default) — return [`TurnResponse::cross_document_unavailable`].
/// * `mapreduce` — run the old `run_mapreduce_capability` path. Kept so the
///   capability can still be exercised on the VM while it is being developed;
///   it is NOT fit for officers (see the constructor's note).
///
/// Flag rather than a deletion so the real path stays one env flip away, the same
/// discipline as `ECHO_HYBRID` / `ECHO_SOURCE_CAP`.
/// Should the map-reduce kill-switch demote this capability to `Guidance`?
///
/// Pure so it can be tested — the reason this is a function at all is that the
/// inline version shipped a bug on 2026-08-12: `Conflict`/`Compare` were demoted
/// here BEFORE the dispatch arm that serves the not-yet notice, so with
/// `NOVIQ_MAPREDUCE=off` (the shipped default) the notice was **unreachable** and
/// officers still got the "not enough information" refusal it exists to replace.
/// Verified live, missed by the unit tests, which is what made it worth extracting.
///
/// ⚠️ This same switch is the original defect (trace `a8ab8eac`): a CORRECT
/// `Conflict` classification demoted to `Guidance`, which then answered a
/// two-document question from one document. Anything added here must be checked
/// against "does the capability still reach the handler that serves it?".
fn resolve_map_reduce_demotion(
    cap: Capability,
    map_reduce_on: bool,
    cross_doc_notice: bool,
) -> Capability {
    if !cap.is_map_reduce() || map_reduce_on {
        return cap;
    }
    // Served by the notice, which is NOT a map-reduce run — must survive.
    if cross_doc_notice && matches!(cap, Capability::Compare | Capability::Conflict) {
        return cap;
    }
    Capability::Guidance
}

fn cross_document_mode_is_notice() -> bool {
    !matches!(
        std::env::var("ECHO_CROSSDOC_CAPABILITY")
            .unwrap_or_default()
            .trim()
            .to_ascii_lowercase()
            .as_str(),
        "mapreduce" | "map_reduce" | "run" | "on"
    )
}

/// Resolve the **active model's** real context window (RAM/VRAM-clamped) for the
/// budget allocator. This is the model the chat actually uses
/// (`select_model_for_tier` → `window_info`), so the budget plan scales with the
/// model (FR-010/FR-019) instead of pinning to the 4096 fallback. Falls back to
/// `ECHO_N_CTX`/`FALLBACK_TRAINED` only when the model can't be inspected.
pub fn resolve_active_window(
    model_slot: &crate::services::llm::ModelSlot,
    tier: &crate::services::hardware_detector::HardwareTier,
) -> usize {
    // F016: when generating through an HTTP engine (vLLM sidecar or remote API),
    // the window is the configured max-model-len; do NOT load the GGUF here (that
    // would put llama.cpp weights in VRAM alongside vLLM on the 12 GB card).
    if crate::services::inference::engine_from_env().is_http() {
        return std::env::var("NOVIQ_REMOTE_MAX_MODEL_LEN")
            .or_else(|_| std::env::var("NOVIQ_VLLM_MAX_MODEL_LEN"))
            .ok()
            .and_then(|v| v.trim().parse::<usize>().ok())
            .unwrap_or(8192);
    }
    let (rel, _label, _notice) = crate::services::query_router::select_model_for_tier(tier);
    let path = crate::services::query_router::resolve_model_path(&rel);
    match model_slot.window_info(&path, tier) {
        Ok((_trained, resolved)) if resolved > 0 => resolved as usize,
        _ => n_ctx::env_override().unwrap_or(n_ctx::FALLBACK_TRAINED) as usize,
    }
}

// ─── Budget planning (the allocator, fed from persona + memory) ──────────────

/// Allocate the window for a turn (classify → allocate). Pure given a
/// [`TokenCounter`]: persona + pinned facts + question are must-keeps; retrieved
/// law fills before recent turns (law outranks history); the summary fills last.
/// The plan is the auditable proof that `Σ prompt + gen_reserve ≤ window`.
#[allow(clippy::too_many_arguments)]
pub fn plan_budget<C: TokenCounter>(
    counter: &C,
    window: usize,
    cap: Capability,
    cfg: &PersonaConfig,
    compact: bool,
    mem: &MemoryContext,
    question: &str,
    law: &[String],
) -> BudgetPlan {
    let inputs = BudgetInputs {
        n_ctx: window,
        gen_reserve: cfg.gen_reserve(cap),
        persona: cfg.persona_text(compact),
        pinned_facts: &mem.pinned_facts,
        question,
        retrieved_law: law,
        recent_turns: &mem.recent_turns,
        summary: mem.summary.as_deref(),
    };
    allocate(counter, &inputs)
}

// ─── Response assembly (generate → gate → assemble) ──────────────────────────

/// Inputs to [`assemble_response`]: the produced answer plus everything the gate
/// and the context-fill notice need.
pub struct AssembleInput<'a> {
    pub conversation_id: String,
    pub capability: Capability,
    pub answer: String,
    /// Retrieved context the answer must be grounded in (gate haystack).
    pub context: &'a str,
    pub citations: Vec<CitationOut>,
    pub plan: &'a BudgetPlan,
    pub fill_threshold: f32,
    /// MEASURED chat fullness from the router, when this turn assembled one
    /// grounded prompt. Takes precedence over `plan` for the fill notice.
    ///
    /// `plan` is a post-hoc audit built from the CITATION EXCERPTS — the handful of
    /// passages the model quoted — not the ~33 chunks it actually read. Its
    /// utilization therefore lands around a third when the real prompt is at 94%,
    /// never crosses `fill_threshold`, and **the "context full" notice never fired**.
    /// `None` keeps the old estimate for paths with nothing measured (deep search,
    /// map-reduce).
    pub chat_fill: Option<crate::services::query_router::ChatFill>,
    /// Guidance offers to tailor after the general answer (FR-004).
    pub offer_tailor: bool,
}

/// Whether the citation gate redacts ungrounded provision references
/// (FR-006/FR-007). Default ON. `ECHO_CITATION_GATE=0` ships the full answer
/// un-redacted — used while the verbatim matcher's false positives on compound /
/// spaced references are fixed (it over-strips correct refs, e.g. the answer's
/// "Rule 16(1)(b)" vs the source's "Rule 16 (1) … (b)").
fn citation_gate_enabled() -> bool {
    !matches!(
        std::env::var("ECHO_CITATION_GATE").ok().as_deref(),
        Some("0") | Some("false") | Some("off")
    )
}

/// Run the citation gate over the answer, derive flags + the context-fill
/// notice, and assemble the wire response. Every provision reference the answer
/// states but the context does not support verbatim becomes a `flag`
/// (offer-to-index) rather than being shipped as fact (FR-006/FR-007).
pub fn assemble_response(input: AssembleInput<'_>) -> TurnResponse {
    // Suppress every sentence asserting a provision the context cannot support
    // verbatim (FR-007) — the answer that ships re-gates clean by construction.
    // Skippable via ECHO_CITATION_GATE=0 (ships the full, un-redacted answer).
    let redaction = if citation_gate_enabled() {
        citation_gate::redact_ungrounded(&input.answer, input.context)
    } else {
        citation_gate::Redaction { answer: input.answer.clone(), ungrounded: Vec::new() }
    };
    let flags = redaction
        .ungrounded
        .iter()
        .map(|u| FlagOut { missing: u.reference.clone(), offer_to_index: true })
        .collect();

    // Prefer the MEASURED chat fullness. `used/limit` is how much of the room for
    // conversation history has been spent — NOT how full the prompt is. The prompt
    // is ~always ~100% full (law expands to fill it), so prompt-fullness would warn
    // on turn one, every turn. See `query_router::ChatFill`.
    let utilization = match input.chat_fill {
        Some(f) if f.limit > 0 => (f.used as f32 / f.limit as f32).clamp(0.0, 1.0),
        // Fallback: the old post-hoc estimate (deep search / map-reduce). Known to
        // read low — it counts quoted citations, not the law actually read.
        _ if input.plan.n_ctx > 0 => {
            ((input.plan.prompt_tokens + input.plan.gen_reserve) as f32 / input.plan.n_ctx as f32)
                .clamp(0.0, 1.0)
        }
        _ => 0.0,
    };
    let notice = utilization >= input.fill_threshold;

    let offer = if input.offer_tailor && input.capability == Capability::Guidance {
        Some("tailor".to_string())
    } else {
        None
    };

    TurnResponse {
        conversation_id: input.conversation_id,
        message_id: String::new(), // stamped by the orchestrator after persistence
        trace_id: String::new(),   // stamped by run_turn after the trace is written
        capability: input.capability.as_str().to_string(),
        answer: redaction.answer,
        citations: input.citations,
        offer,
        flags,
        context_fill: ContextFill { utilization, notice },
        audit: TurnAudit::from_plan(input.plan),
        scoped_to_attachment: false, // set by run_turn after scope resolution
        web_results: Vec::new(),     // set by the capability handler when web search ran
    }
}

// ─── Turn orchestrator (shared by Tauri command + headless HTTP, F014 T011) ──
//
// The single turn entrypoint: classify → persist → load memory → dispatch to the
// capability handler → persist + audit. Extracted from `commands/assistant.rs`
// so both transports call ONE implementation (no logic duplicated). `app` is
// `None` on the headless HTTP path (no Tauri event stream; full answer+citations
// still returned). T009b made the whole pipeline `Option<&AppHandle>`-clean.
#[allow(clippy::too_many_arguments)]
/// Map-reduce kill-switch. Default ON; set `NOVIQ_MAPREDUCE` to
/// `off`/`0`/`false`/`no` to route the map-reduce capabilities through the
/// single-pass Guidance path (avoids the multi-batch provision sweep).
fn map_reduce_enabled() -> bool {
    !matches!(
        std::env::var("NOVIQ_MAPREDUCE")
            .unwrap_or_default()
            .trim()
            .to_ascii_lowercase()
            .as_str(),
        "off" | "0" | "false" | "no"
    )
}

pub async fn run_turn(
    pool: &crate::db::Db,
    embedder: &std::sync::Arc<crate::services::embedder::EmbedderState>,
    model_slot: &std::sync::Arc<crate::services::llm::ModelSlot>,
    reranker_slot: &std::sync::Arc<crate::services::reranker::RerankerSlot>,
    tier: &crate::services::hardware_detector::HardwareTier,
    scheduler: &std::sync::Arc<crate::services::scheduler::Scheduler>,
    app: Option<&crate::compat::AppHandle>,
    conversation_id: &str,
    text: &str,
    scope: &crate::db::repositories::sources::RequestScope,
    // User-chosen document scope from the composer (document-tree checkboxes and
    // @-mention chips resolve to source ids). `None`/empty = no restriction
    // ("every visible document"); a non-empty set NARROWS retrieval to those ids
    // (always intersected with the privacy set — never widens it).
    selected_source_ids: Option<&[String]>,
    // scope-routing 2026-07: the "Search all documents" widen button. When true,
    // this turn ignores the default narrow-to-attachment scoping and searches the
    // full visible corpus (see the scope block below).
    widen: bool,
) -> Result<TurnResponse, String> {
    use crate::services::assistant::capabilities::{
        action_items, additive, amendment_chain, compare, conflict, count, draft, guidance,
        summarize_doc, tabular, transform,
    };
    use crate::services::query_router::HistoryTurn;
    use crate::compat::Emitter;

    let emit_phase = |phase: &str| {
        if let Some(a) = app {
            let _ = a.emit("assistant://progress", serde_json::json!({ "phase": phase }));
        }
    };

    // F027 (FR-012) — an empty project answers "nothing to search" and stops.
    //
    // Checked HERE, before any retrieval runs, rather than by inspecting an
    // empty result set afterwards: a zero-candidate retrieval falls through to
    // whatever the pipeline does with no evidence, and "what the pipeline does
    // with no evidence" is not a guarantee — it is an accident that could change
    // under any later tuning. The requirement is that an empty project cannot
    // answer, so the check must be a gate, not an observation.
    if let Some(project_id) = scope.project_id.as_deref() {
        let ready = crate::db::repositories::projects::ready_document_count(pool, project_id)
            .await
            .unwrap_or(0);
        if ready == 0 {
            let pending: i64 = sqlx::query_scalar(
                "SELECT COUNT(*) FROM project_source ps JOIN sources s ON s.id = ps.source_id \
                 WHERE ps.project_id = $1 AND s.indexing_status IN ('pending','processing')",
            )
            .bind(project_id)
            .fetch_one(pool)
            .await
            .unwrap_or(0);
            return Ok(TurnResponse::nothing_to_search(conversation_id, pending));
        }
    }

    // Track 3: ONE query trace per turn, owned here and written once at the end so
    // the whole turn (classify → scope → capability pipeline) lands in a single
    // file. Each capability receives `&mut trace` and records its own stage steps;
    // the guidance/retrieval path records its full ~30-step pipeline into it (it
    // no longer writes its own file — `run_turn` owns the write).
    // 2026-07-29 — per-user "Next steps" follow-up offer. Resolved once per turn
    // here because this is the layer that holds the RequestScope; the capability
    // handlers receive a plain bool. Absent preference (and an unauthenticated /
    // unrestricted scope, e.g. the desktop build) = ON, preserving prior behaviour.
    let show_offer = match scope.user_id.as_deref() {
        Some(uid) => crate::db::repositories::user_preference::get_bool(
            pool,
            uid,
            crate::db::repositories::user_preference::KEY_REFINEMENT_OFFER,
            true,
        )
        .await
        .unwrap_or(true),
        None => true,
    };
    let mut trace = crate::services::query_trace::QueryTrace::new(text, conversation_id, "");
    trace.step("User question received", &[
        ("Question", format!("\"{}\"", text.chars().take(200).collect::<String>())),
        ("Length", format!("{} characters | {} words",
            text.chars().count(), text.split_whitespace().count())),
    ]);

    // ── classify ────────────────────────────────────────────────────────────
    emit_phase("classify");
    let mut cap = intent::classify_capability(text);
    // Whether the turn ASKED for a document summary — captured before the
    // kill-switch below can rewrite `cap` to Guidance. Drives the pre-baked
    // whole-document summary intercept (migration 012), which serves an
    // ingest-time summary deterministically regardless of the map-reduce flag.
    // Also accept a bare "summarise …" the classifier routed to Guidance because
    // the named document carries no doc-type keyword (e.g. "Delegation of
    // Financial Powers 2019"); `try_prebaked` still requires a real named match.
    let requested_summarize =
        cap == Capability::SummarizeDoc || intent::is_summarize_request(text);
    // Kill-switch: route the map-reduce capabilities (SummarizeDoc, Compare,
    // Conflict, ActionItems) through the single-pass Guidance path instead of
    // the multi-batch provision sweep. That sweep gathers dozens of provisions
    // (e.g. 67 on a heavy SPPRA "responsibilities" query → ~80 s turns) and
    // over-builds context; Guidance uses one bounded top-K retrieval. Set
    // NOVIQ_MAPREDUCE=off to enable this. Default (unset) keeps map-reduce on.
    {
        let resolved = resolve_map_reduce_demotion(
            cap,
            map_reduce_enabled(),
            cross_document_mode_is_notice(),
        );
        if resolved != cap {
            log::info!(
                "map-reduce disabled by NOVIQ_MAPREDUCE — routing {} through Guidance",
                cap.as_str()
            );
        }
        cap = resolved;
    }
    trace.set_capability(cap.as_str());
    trace.step("Classify capability", &[
        ("Capability", cap.as_str().to_string()),
        ("Map-reduce", if map_reduce_enabled() { "on".into() } else { "off".into() }),
    ]);

    // Persist the officer's turn before generation (FR-021 durability).
    let (user_seq, _user_id) = memory::append_turn(pool, conversation_id, "user", text)
        .await
        .map_err(|e| e.to_string())?;
    memory::capture_pinned_facts(pool, conversation_id, user_seq, text)
        .await
        .map_err(|e| e.to_string())?;

    // ── load persona + conversation memory ──────────────────────────────────
    let cfg = persona::PersonaConfig::get();
    let max_turns = cfg.settings.recent_turns_verbatim;
    let mem = memory::load_for_turn(pool, conversation_id, max_turns)
        .await
        .map_err(|e| e.to_string())?;

    let history: Vec<HistoryTurn> = mem
        .recent_turns
        .iter()
        .rev()
        .filter_map(|t| {
            t.split_once(": ").map(|(role, content)| HistoryTurn {
                role: role.to_string(),
                content: content.to_string(),
            })
        })
        .collect();

    // ── dispatch to the capability handler ──────────────────────────────────
    // F021: every capability retrieves ONLY within this per-request id set —
    // shared corpus ∪ requester's own documents ∪ this conversation's
    // attachments. Desktop passes RequestScope::unrestricted() (all indexed).
    let scope_ids = crate::db::repositories::sources::visible_source_ids(pool, scope)
        .await
        .map_err(|e| e.to_string())?;

    // ── narrow the retrieval scope to the user's chosen document(s) ───────────
    // NARROW-FIRST precedence (each step only NARROWS, never widens past
    // `scope_ids`); dead-simple and wording-independent (scope-routing 2026-07):
    //   1. an explicit composer selection (checkbox subset / @-mention chips in
    //      `selected_source_ids`) → scope to that selection. Chosen deliberately,
    //      so it WINS even over a live attachment (tick doc A while a file is
    //      attached ⇒ search A, not the attachment).
    //   2. else a live attachment on this conversation → scope strictly to it by
    //      DEFAULT, regardless of the question's wording. Previously this was
    //      gated on `intent::wants_attachment_only(text)`, so any legal word
    //      ("which rules apply?") disabled it and the query LEAKED to the whole
    //      corpus — the reported bug. The UI offers a "Search all documents"
    //      widen button whenever this fires (`scoped_to_attachment`).
    //   3. else the @-mention server-side safety net (frontend failed to send the
    //      id), else the full visible set.
    // Intersecting with `scope_ids` keeps the F021 privacy invariant intact: a
    // stale/forged id can never reach an unauthorized document.
    //
    // TWO exemptions skip the attachment default (still honour an explicit
    // selection): (a) `widen` — the user clicked "Search all documents" — and
    // (b) the Notice-Assist (`/help`) flow, whose officer-defence deliberately
    // needs the STATUTE from the corpus, not just the attached notice. An open
    // assist session makes the officer's plain replies part of that flow too.
    let assist_open = crate::services::assistant::capabilities::notice_assist::session::load(
        pool, conversation_id,
    )
    .await
    .ok()
    .flatten()
    .is_some();
    let notice_flow = intent::is_slash_command(text) || assist_open;
    let has_explicit = selected_source_ids.map(|s| !s.is_empty()).unwrap_or(false);
    let (scope_ids, scoped_to_attachment): (Vec<String>, bool) = if widen || notice_flow || has_explicit {
        (narrow_to_selection(scope_ids, selected_source_ids), false)
    } else {
        let att = crate::db::repositories::sources::attachment_source_ids(pool, conversation_id)
            .await
            .map_err(|e| e.to_string())?;
        let att: Vec<String> = att.into_iter().filter(|id| scope_ids.contains(id)).collect();
        // A SPREADSHEET attachment plus a question that reaches for a rule must
        // NOT narrow to the attachment. A workbook is data, never a legal
        // instrument, so the rule that reads it is always in the corpus —
        // "which forum approves this project?" searched only the sheet and
        // answered "the retrieved documents do not contain information about
        // the forum" while the Manual for Development Projects sat indexed and
        // citable (prod, 2026-08-03). The computed figures are supplied
        // separately, by `tabular::facts`.
        let sheets_only = !att.is_empty()
            && crate::db::repositories::sources::attachment_file_names(pool, conversation_id)
                .await
                .unwrap_or_default()
                .iter()
                .all(|n| tabular::is_spreadsheet(n));
        if sheets_only && tabular::needs_corpus_rules(text) {
            log::info!("spreadsheet attachment + rules question → keeping the corpus in scope");
            (scope_ids, false)
        } else if !att.is_empty() {
            log::info!("live attachment → default scope to {} attachment(s)", att.len());
            (att, true)
        } else if text.contains('@') {
            // Backend safety net: no explicit selection, but the question
            // `@`-mentions a visible document → scope to it server-side.
            let m = resolve_at_mentions(pool, &scope_ids, text).await;
            if m.is_empty() {
                (scope_ids, false)
            } else {
                log::info!("@-mention resolved server-side → scoping to {} document(s)", m.len());
                (m, false)
            }
        } else {
            (scope_ids, false)
        }
    };
    trace.step("Resolve document scope", &[
        ("Sources in scope", scope_ids.len().to_string()),
        ("Scoped to attachment", scoped_to_attachment.to_string()),
        ("Explicit selection",
            selected_source_ids.map(|s| s.len()).unwrap_or(0).to_string()),
    ]);

    // Transform and additive intercept before capability dispatch, both guarded
    // on a prior assistant answer existing. Transform first ("make it more
    // formal" / "expand on this" — restyle, no retrieval); additive second
    // ("also cover…" / "expand on <topic>" — extend the prior answer with a
    // newly-retrieved aspect, citations merged). `history` is oldest→newest,
    // so the last assistant entry is the most recent answer.
    let prior_answer = history
        .iter()
        .rev()
        .find(|t| t.role == "assistant")
        .map(|t| t.content.clone());
    // Notice Assist (/help) — Feature 022. A slash command, or an already-open
    // assist session (`assist_open`, computed above for scope resolution), routes
    // the turn to the officer-defense flow BEFORE ordinary capability dispatch (an
    // open session captures the officer's plain replies as answers to the current
    // question). Sits alongside transform/additive so the common persist/stamp/emit
    // tail below still runs.
    // Pre-baked whole-document summary (migration 012). A "summarize <named doc>"
    // turn is served from the ingest-time `document_full` summary, re-presented by a
    // light prose-model pass grounded strictly in that stored text (no citations),
    // and intercepted BEFORE the map-reduce kill-switch would otherwise reroute
    // SummarizeDoc through Guidance. Falls through (None) when the turn names no
    // in-scope document that has a pre-baked summary. Skipped for slash/assist turns
    // so `/help` is never hijacked.
    let prebaked_summary = if requested_summarize
        && !intent::is_slash_command(text)
        && !assist_open
    {
        summarize_doc::try_prebaked(pool, model_slot, tier, cfg, conversation_id, text, &scope_ids, &mut trace).await?
    } else {
        None
    };

    // Spreadsheet arithmetic (2026-08-03). Resolved BEFORE the routing chain
    // because deciding whether this lane applies means opening the attached
    // workbook — a boolean guard cannot answer it. `None` whenever there is no
    // workbook, the question wants no number, or no plan fits the sheet, so
    // every other lane keeps its existing behaviour.
    let tabular_turn: Option<TurnResponse> = if tabular::wants_computation(text) {
        match tabular::run(
            pool, cfg, model_slot, tier, app.clone(), conversation_id, text, &mut trace,
        )
        .await
        {
            Ok(r) => r,
            Err(e) => {
                log::warn!("tabular lane failed, falling through: {e}");
                None
            }
        }
    } else {
        None
    };

    // A slash command is an instruction to the app, never a question for the
    // corpus. Routing it here guarantees `/cancel` (or any unknown command) can
    // never reach retrieval + generation — it used to answer "/cancel" from the
    // SPPRA rules about cancelling a bidding process, after 38 s of inference.
    let mut resp = if intent::is_slash_command(text) || assist_open {
        emit_phase("notice_assist");
        crate::services::assistant::capabilities::notice_assist::run(
            pool, embedder, model_slot, tier, scheduler, app, cfg, &mem, conversation_id, text,
            &scope_ids, &mut trace,
        )
        .await?
    } else if let Some(pb) = prebaked_summary {
        emit_phase("summarize_doc");
        pb
    } else if intent::is_greeting_or_intro(text) && !intent::is_slash_command(text) {
        // Greeting / "introduce yourself" — a short intro to Noviq + suggested
        // actions. Not a corpus question: no retrieval, no citations.
        emit_phase("greeting");
        trace.step("Greeting / intro", &[("Matched", "true".to_string())]);
        TurnResponse {
            conversation_id: conversation_id.to_string(),
            message_id: String::new(),
            trace_id: String::new(),
            capability: "greeting".to_string(),
            answer: NOVIQ_INTRO.to_string(),
            citations: Vec::new(),
            offer: None,
            flags: Vec::new(),
            context_fill: ContextFill { utilization: 0.0, notice: false },
            audit: TurnAudit::default(),
            scoped_to_attachment: false,
            web_results: Vec::new(),
        }
    } else if intent::is_low_signal(text) && !intent::is_slash_command(text) {
        // Content-free probe ("a", "test", "123", "..."): no answerable question.
        // Nudge instead of a ~30 s retrieval + inference that would answer junk
        // with junk citations. No corpus hit, no citations.
        emit_phase("nudge");
        trace.step("Low-signal / probe", &[("Matched", "true".to_string())]);
        TurnResponse {
            conversation_id: conversation_id.to_string(),
            message_id: String::new(),
            trace_id: String::new(),
            capability: "nudge".to_string(),
            answer: NOVIQ_NUDGE.to_string(),
            citations: Vec::new(),
            offer: None,
            flags: Vec::new(),
            context_fill: ContextFill { utilization: 0.0, notice: false },
            audit: TurnAudit::default(),
            scoped_to_attachment: false,
            web_results: Vec::new(),
        }
    } else if let Some(answer) =
        memory::direct_recall_answer(pool, conversation_id, text).await.map_err(|e| e.to_string())?
    {
        // Conversation-recall short-circuit (graph-memory tier 2). "what was my
        // first question?", "read me your last answer" — a question about THIS
        // chat's structure, not the corpus. Routed through the grounded RAG path it
        // fails two ways (prod 2026-07-17): the model REFUSES (no legal passage
        // supports it) or reads "the first one" as a retrieved DOCUMENT. The exact
        // turn is already in `chat_message`; answer it deterministically here — no
        // retrieval, no model, no refusal — the same shape as Count (ADR-0019).
        // Only fires for PURE recall (`is_pure_recall`): a legal "the first
        // schedule" and the compound "compare the first one with the third one"
        // fall through to normal retrieval.
        emit_phase("recall");
        trace.step("Conversation recall", &[("Matched", "true".to_string())]);
        TurnResponse {
            conversation_id: conversation_id.to_string(),
            message_id: String::new(),
            trace_id: String::new(),
            capability: "recall".to_string(),
            answer,
            citations: Vec::new(),
            offer: None,
            flags: Vec::new(),
            context_fill: ContextFill { utilization: 0.0, notice: false },
            audit: TurnAudit::default(),
            scoped_to_attachment: false,
            web_results: Vec::new(),
        }
    } else if intent::is_transform_request(text) && prior_answer.is_some() {
        emit_phase("transform");
        transform::run(
            pool, model_slot, tier, scheduler, app, cfg, &mem, conversation_id, text,
            prior_answer.as_deref().unwrap(), &mut trace,
        )
        .await?
    } else if intent::is_additive_request(text) && prior_answer.is_some() {
        emit_phase("additive");
        additive::run(
            pool, embedder, model_slot, reranker_slot, tier, scheduler, app, cfg, &mem,
            &history, conversation_id, text, &scope_ids,
            prior_answer.as_deref().unwrap(), scoped_to_attachment, show_offer, &mut trace,
        )
        .await?
    } else if let Some(resp) = tabular_turn {
        // SPREADSHEET ARITHMETIC ⇒ COMPUTE IT (2026-08-03).
        //
        // Ahead of the web lane and of ordinary retrieval because a computed
        // figure is not a matter of opinion, and because the alternative is
        // measured: on this exact question set the model scored 19/53 and
        // presented wrong totals in bold with a citation mark. `tabular_turn`
        // is None whenever this lane cannot help, so nothing else is displaced.
        emit_phase("tabular");
        resp
    } else if crate::services::query_router::web_search_active() {
        // WEB SEARCH ON ⇒ ANSWER FROM THE WEB, whatever the question looks like.
        //
        // Web search lived in `guidance` alone, so every other capability
        // silently ignored the switch the user had just turned on. Prod
        // 2026-07-30: "what are new amendments IN sindh public procurement rules
        // in 2024" matched an amendment-chain cue, went to a SQL/graph lookup
        // that reads neither the corpus nor the web, and answered "No amendments
        // are recorded … in the indexed documents" — while the SAME question
        // phrased "amendments OCCURED in …" missed the cue and was answered from
        // the web. One intervening word decided whether the toggle applied.
        //
        // The toggle is the user's explicit instruction for the turn and now
        // outranks classification. `guidance` is the only lane that searches, so
        // the turn goes there and keeps its classified capability for the trace.
        // Attachment-scoped turns still read the attachment — that is handled
        // inside `guidance`, which treats an attached file as in scope alongside
        // the web (a file the officer put in this conversation is not the
        // indexed corpus).
        emit_phase("web");
        trace.step("Routing", &[
            ("Web search", "on — routed to the web lane".to_string()),
            ("Classified capability", cap.as_str().to_string()),
            ("Overridden", (!matches!(cap, Capability::Guidance)).to_string()),
        ]);
        guidance::run(
            pool, embedder, model_slot, reranker_slot, tier, scheduler, app, cfg, &mem,
            &history, conversation_id, text, cap, &scope_ids, scoped_to_attachment,
            show_offer, &mut trace,
        )
        .await?
    } else {
        match cap {
        Capability::Draft => {
            draft::run(pool, embedder, model_slot, reranker_slot, tier, scheduler, app, cfg, &mem, &history, conversation_id, text, &scope_ids, scoped_to_attachment, show_offer, &mut trace).await?
        }
        Capability::SummarizeDoc => {
            summarize_doc::run(pool, embedder, model_slot, tier, scheduler, app, cfg, &mem, conversation_id, text, &scope_ids, &mut trace).await?
        }
        // Cross-document conflict / compare: served with an honest "not built
        // yet" while the capability is rebuilt onto Deep search. The retrieval
        // slice these used to run on covered ~3% of the named documents, so the
        // attempt produced a refusal that read as a broken product. See
        // `TurnResponse::cross_document_unavailable`. `ECHO_CROSSDOC_CAPABILITY=mapreduce`
        // restores the old path for development on the VM.
        Capability::Compare | Capability::Conflict if cross_document_mode_is_notice() => {
            emit_phase("cross_document_unavailable");
            trace.step("Cross-document capability", &[
                ("Capability", cap.as_str().to_string()),
                ("Served", "not-yet-available notice (no retrieval, no model)".to_string()),
                ("Reason", "map-reduce covers only the retrieval slice (~3% of the named documents)".to_string()),
                ("Planned", "Deep search (decompose → 2 retrieval rounds → grounded synthesis)".to_string()),
                ("Override", "ECHO_CROSSDOC_CAPABILITY=mapreduce".to_string()),
            ]);
            TurnResponse::cross_document_unavailable(conversation_id, cap)
        }
        Capability::Compare => {
            compare::run(pool, embedder, model_slot, tier, scheduler, app, cfg, &mem, conversation_id, text, &scope_ids, &mut trace).await?
        }
        Capability::Conflict => {
            conflict::run(pool, embedder, model_slot, tier, scheduler, app, cfg, &mem, conversation_id, text, &scope_ids, &mut trace).await?
        }
        Capability::ActionItems => {
            action_items::run(pool, embedder, model_slot, tier, scheduler, app, cfg, &mem, conversation_id, text, &scope_ids, &mut trace).await?
        }
        Capability::Count => {
            // ADR-0019 — deterministic SQL count/list; no retrieval, no model.
            emit_phase("count");
            count::run(pool, cfg, conversation_id, text, &scope_ids, &mut trace).await?
        }
        Capability::AmendmentChain => {
            // An amendment chain is a lookup ON A PROVISION. With no provision to
            // look up there is nothing for this capability to do, and the old
            // `unwrap_or_else(|| text.to_string())` fallback handed it the WHOLE
            // QUESTION as the thing to find — producing, in prod 2026-07-30,
            // "No amendments are recorded for what are new amendments in sindh
            // public procurement rules in 2024 in the indexed documents."
            //
            // Worse than the phrasing: this lane reads neither the corpus nor the
            // web, so that sentence claimed the documents had been checked when
            // nothing had been. A question with no provision reference belongs on
            // the normal path, which does read them.
            match intent::extract_provision_ref(text) {
                Some(provision_ref) => {
                    amendment_chain::run(pool, cfg, conversation_id, &provision_ref, &scope_ids, &mut trace).await?
                }
                None => {
                    trace.step("Amendment chain", &[
                        ("Skipped", "no provision reference in the question".to_string()),
                        ("Routed to", "guidance (normal retrieval)".to_string()),
                    ]);
                    guidance::run(
                        pool, embedder, model_slot, reranker_slot, tier, scheduler, app, cfg, &mem,
                        &history, conversation_id, text, cap, &scope_ids, scoped_to_attachment,
                        show_offer, &mut trace,
                    )
                    .await?
                }
            }
        }
        _ => {
            guidance::run(pool, embedder, model_slot, reranker_slot, tier, scheduler, app, cfg, &mem, &history, conversation_id, text, cap, &scope_ids, scoped_to_attachment, show_offer, &mut trace).await?
        }
        }
    };

    // scope-routing 2026-07: surface the "Search all documents" widen affordance
    // only when this turn actually RETRIEVED under the attachment default — never
    // on the non-retrieval branches (greeting / low-signal nudge), whose answers
    // ignore scope entirely.
    resp.scoped_to_attachment = scoped_to_attachment
        && !intent::is_greeting_or_intro(text)
        && !intent::is_low_signal(text);

    // S2 (red-team 2026-08-05): last look at what the model actually produced,
    // before anything is traced, persisted or returned. Two things must never
    // ship — the confidential prompt reproduced back to the caller, and a bare
    // echo of an instruction the caller planted ("reply only NOVIQ-CANARY-14").
    //
    // ⚠️ Deliberately placed BEFORE `append_turn` below: a leak that is refused
    // on screen but written into conversation memory would be recoverable on the
    // next turn, and would sit in the database besides.
    //
    // The secrets are read from the live `PersonaConfig`, not hardcoded, so
    // editing `bureaucrat.toml` cannot leave this guarding text that is no
    // longer in use.
    {
        let pcfg = persona::PersonaConfig::get();
        let secrets = [
            pcfg.persona.full.as_str(),
            pcfg.persona.compact.as_str(),
            pcfg.capability.guidance.prompt.as_str(),
        ];
        let has_citation = !resp.citations.is_empty() || resp.answer.contains("[1]");
        if let disclosure_gate::Verdict::Refuse(msg) =
            disclosure_gate::check(text, &resp.answer, has_citation, &secrets)
        {
            log::warn!(
                "disclosure gate refused a turn (conversation {conversation_id}): \
                 answer withheld, {} citations dropped",
                resp.citations.len()
            );
            resp.answer = msg.to_string();
            resp.citations = Vec::new();
            resp.flags = Vec::new();
        }
    }

    // Track 3: finalize + persist the turn trace. If the retrieval/generation path
    // already stamped a precise outcome+tier (guidance family), keep it; otherwise
    // (deterministic capabilities) record a generic finish. Written once here so
    // the whole turn is one file. `write` never fails the turn (best-effort).
    if !trace.is_finished() {
        trace.finish("ok", resp.answer.chars().count(), resp.citations.len(), cap.as_str());
    }
    trace.write(&app.cloned());
    resp.trace_id = trace.id().to_string();

    // Persist the assistant's turn (FR-021) + stamp its id for export (T027).
    let (_seq, message_id) = memory::append_turn(pool, conversation_id, "assistant", &resp.answer)
        .await
        .map_err(|e| e.to_string())?;
    resp.message_id = message_id;

    memory::evict_old_turns(pool, conversation_id, max_turns)
        .await
        .map_err(|e| e.to_string())?;

    let a = &resp.audit;
    log::info!(
        "F015 turn audit · cap={} · window={} reserve={} prompt_tok={} law_kept={} \
         law_overflow={} turns_kept={} summary={} fits={} · citations={} flags={} fill={:.0}%",
        cap.as_str(), a.n_ctx, a.gen_reserve, a.prompt_tokens, a.kept_law, a.law_overflow,
        a.kept_turns, a.summary_included, a.fits(),
        resp.citations.len(), resp.flags.len(), resp.context_fill.utilization * 100.0,
    );

    // F027 (FR-016) — label each citation as the officer's own document or as
    // enacted law, before the response is emitted or persisted. Done here, at
    // the one point that knows both the project and the finished citation list,
    // so the label is on the payload the UI renders AND on the `citations_json`
    // written to `query_log` — which is what makes a reopened transcript still
    // show which answers drew on the corpus (FR-019).
    if let Some(project_id) = scope.project_id.as_deref() {
        label_citation_provenance(pool, project_id, &mut resp.citations).await;
    }

    if let Some(a) = app {
        let _ = a.emit("assistant://complete", &resp);
    }
    Ok(resp)
}

/// Intersect the visible scope with an explicit composer selection. An empty /
/// `None` selection means "no restriction" → the full visible set is returned
/// unchanged. A non-empty selection narrows to the ids that are BOTH selected
/// and visible (order follows the visible set, so retrieval ordering is stable).
fn narrow_to_selection(visible: Vec<String>, selected: Option<&[String]>) -> Vec<String> {
    match selected {
        Some(sel) if !sel.is_empty() => {
            let want: std::collections::HashSet<&str> = sel.iter().map(|s| s.as_str()).collect();
            visible.into_iter().filter(|id| want.contains(id.as_str())).collect()
        }
        _ => visible,
    }
}

/// Backend safety net for `@document` scoping: when the client sent NO explicit
/// selection but the question `@`-mentions a visible document by name, resolve it
/// server-side so document scoping never depends solely on the frontend having
/// attached the id. Conservative: a source is included only when `@<file_name>`,
/// `@<file_name-without-extension>`, or `@<cleaned display name>` appears in the
/// text (all case-insensitive). Returns the matched visible source ids (possibly
/// empty).
async fn resolve_at_mentions(pool: &crate::db::Db, visible_ids: &[String], text: &str) -> Vec<String> {
    if visible_ids.is_empty() || !text.contains('@') {
        return Vec::new();
    }
    let rows: Vec<(String, String)> = match sqlx::query_as(
        "SELECT id, file_name FROM sources WHERE id = ANY($1)",
    )
    .bind(visible_ids)
    .fetch_all(pool)
    .await
    {
        Ok(r) => r,
        Err(e) => {
            log::warn!("@-mention resolver: source lookup failed: {e}");
            return Vec::new();
        }
    };
    let hay = text.to_lowercase();
    let mut out = Vec::new();
    for (id, fname) in rows {
        let full = fname.to_lowercase();
        let stem = full.rsplit_once('.').map(|(s, _)| s.to_string()).unwrap_or_else(|| full.clone());
        let cleaned = clean_document_name(&fname).to_lowercase();
        let hit = hay.contains(&format!("@{full}"))
            || hay.contains(&format!("@{stem}"))
            || (!cleaned.is_empty() && hay.contains(&format!("@{cleaned}")));
        if hit {
            out.push(id);
        }
    }
    out
}

// F021: `all_indexed_source_ids` removed — the per-request scope now comes
// from `db::repositories::sources::visible_source_ids(pool, scope)`, the single
// home of the retrieval-privacy invariant. Do NOT reintroduce whole-corpus
// id queries here; that is exactly the leak this feature closed.

#[cfg(test)]
mod tests {
    use super::*;
    use context_budget::EstimateCounter;

    #[test]
    fn cross_document_notice_names_the_capability_and_offers_a_next_step() {
        for (cap, wire, phrase) in [
            (Capability::Conflict, "conflict", "conflicts"),
            (Capability::Compare, "compare", "compare two documents"),
        ] {
            let r = TurnResponse::cross_document_unavailable("c1", cap);
            assert_eq!(r.capability, wire);
            assert!(r.answer.contains(phrase), "{cap:?}: {}", r.answer);
            // Names where the feature is going …
            assert!(r.answer.contains("Deep search"), "{cap:?}");
            // … but never instructs the officer to switch it on, because it does
            // not implement these capabilities yet and is admin-gated besides.
            let a = r.answer.to_lowercase();
            for dead_end in ["turn on deep", "enable deep", "switch on deep", "use deep search"] {
                assert!(!a.contains(dead_end), "{cap:?} sends the user to a dead end: {}", r.answer);
            }
            // A refusal must still leave the officer something to do.
            assert!(a.contains("in the meantime"), "{cap:?}: no next step offered");
            // No retrieval ran, so nothing may be cited.
            assert!(r.citations.is_empty());
        }
    }

    #[test]
    fn kill_switch_never_hides_the_cross_document_notice() {
        use Capability::*;
        // THE 2026-08-12 REGRESSION: map-reduce off (the shipped default) must
        // still let Conflict/Compare reach the notice handler. Shipped broken
        // because this logic was inline and untested.
        for cap in [Conflict, Compare] {
            assert_eq!(resolve_map_reduce_demotion(cap, false, true), cap,
                "{cap:?} must survive the kill-switch to reach the notice");
            assert_eq!(resolve_map_reduce_demotion(cap, true, true), cap);
            // Notice disabled (development) => old behaviour, demoted as before.
            assert_eq!(resolve_map_reduce_demotion(cap, false, false), Guidance);
            assert_eq!(resolve_map_reduce_demotion(cap, true, false), cap);
        }
        // The other two map-reduce capabilities are untouched by the notice.
        for cap in [SummarizeDoc, ActionItems] {
            assert_eq!(resolve_map_reduce_demotion(cap, false, true), Guidance);
            assert_eq!(resolve_map_reduce_demotion(cap, true, true), cap);
        }
        // Non-map-reduce capabilities are never demoted.
        for cap in [Guidance, Draft, Count, Tabular, AmendmentChain] {
            assert_eq!(resolve_map_reduce_demotion(cap, false, true), cap);
        }
    }

    #[test]
    fn cross_document_notice_is_the_default_and_flag_restores_map_reduce() {
        // Serialised via the env, so assert the parse rather than mutate global
        // state in parallel tests (see the pass2 `set_var` flake).
        for (v, want_notice) in [
            ("", true), ("notice", true), ("off", true), ("anything-else", true),
            ("mapreduce", false), ("map_reduce", false), ("run", false), ("ON", false),
        ] {
            let is_notice = !matches!(
                v.trim().to_ascii_lowercase().as_str(),
                "mapreduce" | "map_reduce" | "run" | "on"
            );
            assert_eq!(is_notice, want_notice, "ECHO_CROSSDOC_CAPABILITY={v:?}");
        }
    }

    fn plan_for(window: usize, law: &[String]) -> BudgetPlan {
        let cfg = PersonaConfig::get();
        let mem = MemoryContext::default();
        plan_budget(&EstimateCounter, window, Capability::Guidance, cfg, false, &mem, "what is the leave procedure", law)
    }

    #[test]
    fn narrow_to_selection_intersects_without_widening() {
        let visible = vec!["a".to_string(), "b".to_string(), "c".to_string()];
        // None / empty selection → unrestricted (full visible set, order kept).
        assert_eq!(narrow_to_selection(visible.clone(), None), visible);
        assert_eq!(narrow_to_selection(visible.clone(), Some(&[])), visible);
        // Selection narrows to the intersection, preserving visible order.
        assert_eq!(
            narrow_to_selection(visible.clone(), Some(&["c".to_string(), "a".to_string()])),
            vec!["a".to_string(), "c".to_string()]
        );
        // A selected id that is NOT visible can never be added (privacy invariant).
        assert_eq!(
            narrow_to_selection(visible.clone(), Some(&["z".to_string(), "b".to_string()])),
            vec!["b".to_string()]
        );
        // Selecting only out-of-scope ids yields an empty scope (retrieves nothing).
        assert!(narrow_to_selection(visible, Some(&["z".to_string()])).is_empty());
    }

    #[test]
    fn clean_document_name_strips_prefix_and_extension() {
        assert_eq!(clean_document_name("01_Sindh_Rules_of_Business_1986.pdf"), "Sindh Rules of Business 1986");
        assert_eq!(clean_document_name("02_SPPRA_Rules_2010_Revised_2013.pdf"), "SPPRA Rules 2010 Revised 2013");
        assert_eq!(
            clean_document_name("Govt. of Sindh Manual of Secretariat instruction 2010.pdf"),
            "Govt. of Sindh Manual of Secretariat instruction 2010"
        );
    }

    #[test]
    fn budget_plan_never_exceeds_window() {
        let law = vec!["Section 18 ".repeat(50), "Rule 6(c) ".repeat(40)];
        let plan = plan_for(8192, &law);
        assert!(plan.fits(), "Σ prompt + reserve must be ≤ window: {plan:?}");
        assert!(!plan.must_fit_failed);
    }

    #[test]
    fn guidance_offers_to_tailor_and_grounded_answer_has_no_flags() {
        let context = "Section 18 of the Sindh Civil Servants Act provides for leave.";
        let plan = plan_for(8192, &[context.to_string()]);
        let resp = assemble_response(AssembleInput {
            conversation_id: "c1".into(),
            capability: Capability::Guidance,
            answer: "Under Section 18 you apply in writing.".into(),
            context,
            citations: vec![CitationOut { n: 1, instrument: "Sindh Civil Servants Act".into(), path: "Section 18".into(), verbatim: context.into(), page: None , ..Default::default() }],
            plan: &plan,
            fill_threshold: 0.80,
            chat_fill: None,
            offer_tailor: true,
        });
        assert_eq!(resp.capability, "guidance");
        assert_eq!(resp.offer.as_deref(), Some("tailor"));
        assert!(resp.flags.is_empty(), "grounded answer must not flag");
        assert_eq!(resp.citations.len(), 1);
    }

    #[test]
    fn ungrounded_reference_becomes_a_flag_with_offer_to_index() {
        let context = "Section 18 provides for leave.";
        let plan = plan_for(8192, &[context.to_string()]);
        let resp = assemble_response(AssembleInput {
            conversation_id: "c1".into(),
            capability: Capability::Guidance,
            answer: "This is governed by Section 99.".into(),
            context,
            citations: vec![],
            plan: &plan,
            fill_threshold: 0.80,
            chat_fill: None,
            offer_tailor: true,
        });
        assert_eq!(resp.flags.len(), 1);
        assert_eq!(resp.flags[0].missing.to_lowercase(), "section 99");
        assert!(resp.flags[0].offer_to_index);
    }

    #[test]
    fn context_fill_notice_trips_at_threshold() {
        // Tiny window so utilization is high → notice tripped.
        let law = vec!["Section 18 ".repeat(200)];
        let plan = plan_for(1100, &law);
        let resp = assemble_response(AssembleInput {
            conversation_id: "c1".into(),
            capability: Capability::Guidance,
            answer: "ok".into(),
            context: "",
            citations: vec![],
            plan: &plan,
            fill_threshold: 0.50,
            chat_fill: None,
            offer_tailor: false,
        });
        assert!(resp.context_fill.utilization > 0.0);
        assert_eq!(resp.context_fill.notice, resp.context_fill.utilization >= 0.50);
    }

    #[test]
    fn non_guidance_capability_does_not_offer_tailor() {
        let plan = plan_for(8192, &[]);
        let resp = assemble_response(AssembleInput {
            conversation_id: "c1".into(),
            capability: Capability::SummarizeDoc,
            answer: "summary".into(),
            context: "",
            citations: vec![],
            plan: &plan,
            fill_threshold: 0.80,
            chat_fill: None,
            offer_tailor: true,
        });
        assert_eq!(resp.capability, "summarize_doc");
        assert!(resp.offer.is_none());
    }
}
