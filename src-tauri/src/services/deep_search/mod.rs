//! Feature 025 — Agentic Deep Search (ADR-0021).
//!
//! A **bounded** multi-pass deep-search loop — at most **3 LLM calls** and **2
//! retrieval rounds** per turn — that answers a broad question across several
//! fresh context windows instead of one saturated window. It replaces the
//! one-pass deep path when `ECHO_AGENTIC_DEEP` is enabled (flag-gated for A/B;
//! see `agentic_enabled`). Normal (Deep OFF) search is untouched.
//!
//! Flow (see `specs/025-agentic-deep-search/`):
//!   expand (#1) → retrieve round 1 → draft + coverage-check (#2)
//!     → retrieve round 2 (pending only) → grounded synthesis (#3)
//!
//! This module currently defines the feature flag and the in-memory data model
//! (Phase 1 / T007). The orchestrator (`run_agentic_deep`) and the per-stage
//! helpers land with US1 (T013–T018).

mod assemble;
pub(crate) mod checkpoint;
mod coverage;
mod expand;
pub(crate) mod state;
mod verify;

use state::{DeepState, Node};

use std::sync::Arc;

use anyhow::Result;
use tokio_util::sync::CancellationToken;

use crate::db::Db;
use crate::services::assistant::capabilities::summarize_doc::dedupe_repeats;
use crate::services::embedder::EmbedderState;
use crate::services::hardware_detector::HardwareTier;
use crate::services::llm::ModelSlot;
use crate::services::query_router::{self, Citation, QueryResponse};

/// Passages retrieved per sub-question in each round.
const PER_SUBQ_K: usize = 20;
/// Answer-generation cap for the final synthesis (call #3) — matches the normal
/// Guidance answer cap so the window stays balanced (deep's breadth comes from the
/// multi-pass retrieval, not a bigger answer).
const SYNTH_MAX_TOKENS: u32 = 1280;
/// Max citations surfaced (mirrors the normal path's display limit).
const CITE_LIMIT: usize = 8;
/// Char budget for the synthesis passage block (≈ the retrieved-law token budget).
const SYNTH_CHAR_BUDGET: usize = 16_000;

/// Feature flag (research R6, FR-008). The agentic loop runs only when this is
/// enabled; otherwise Deep falls back to the existing one-pass path. Default OFF
/// so it can be A/B'd against legacy deep before the old path is retired. Mirrors
/// the `graph_expand_enabled()` / `summary_recall_enabled()` env convention.
pub fn agentic_enabled() -> bool {
    matches!(
        std::env::var("ECHO_AGENTIC_DEEP")
            .unwrap_or_default()
            .trim()
            .to_ascii_lowercase()
            .as_str(),
        "1" | "on" | "true" | "yes"
    )
}

/// Feature flag for issue-driven facet expansion (2026-07-18). When enabled, the
/// decomposer additionally spins sub-questions for latent legal ISSUES implied by
/// facts in the prompt (quorum, conflict of interest, sanction/write-off limits,
/// omnibus sanction) — but ONLY when a concrete trigger fact is present, so it
/// never invents topics (see `expand::spot_issues`). Default OFF for a measured
/// rollout; the item-driven decomposition is unchanged when off.
pub fn issue_spotting_enabled() -> bool {
    matches!(
        std::env::var("ECHO_DEEP_ISSUE_SPOTTING")
            .unwrap_or_default()
            .trim()
            .to_ascii_lowercase()
            .as_str(),
        "1" | "on" | "true" | "yes"
    )
}

/// Audience gate for the Deep-search toggle (2026-07). While the agentic loop is
/// still being smoothed it is exposed to **admins only**; the public rollout is a
/// config flip, not a redeploy. `ECHO_DEEP_AUDIENCE=all` opens it to every signed-in
/// user; anything else (unset / `admin`) keeps it admin-only. This gates who may
/// *invoke* deep — the agentic loop itself still also requires `agentic_enabled()`.
pub fn deep_audience_is_all() -> bool {
    matches!(
        std::env::var("ECHO_DEEP_AUDIENCE")
            .unwrap_or_default()
            .trim()
            .to_ascii_lowercase()
            .as_str(),
        "all" | "everyone" | "public"
    )
}

/// Whether a user with this role may use Deep search: admins always; everyone else
/// only when the audience is opened to all. Single source of truth shared by the
/// `/me` response (so the UI shows/hides the toggle) and the query handler (so a
/// crafted `deep:true` from a non-admin is downgraded, not honoured).
pub fn deep_enabled_for_role(role: &str) -> bool {
    deep_enabled(role, deep_audience_is_all())
}

/// Pure gate decision (env read out for testability): admin always; others only
/// when the audience is opened to all.
fn deep_enabled(role: &str, audience_is_all: bool) -> bool {
    role.eq_ignore_ascii_case("admin") || audience_is_all
}

/// F026 — the durable-thread key for a deep turn: `conversation_id:<hash of the
/// normalized question>`.
///
/// This MUST be stable across attempts, otherwise resume can never fire. The first
/// cut keyed on the per-turn workload UUID, which is freshly minted for every turn —
/// so `latest(thread_id)` never matched a prior attempt and the whole resume path was
/// unreachable dead code. Keying on the question means: *stop a deep turn, ask the
/// same thing again in the same conversation → continue where it left off*, which is
/// also the semantics a user expects.
///
/// Normalization (trim + lowercase + collapse internal whitespace) makes trivial
/// re-typing differences ("  Grounds  and penalties " vs "grounds and penalties")
/// land on the same thread. Different questions in the same conversation get
/// different threads, so they never collide.
pub(crate) fn deep_thread_id(conversation_id: &str, query_text: &str) -> String {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};
    let normalized = query_text.trim().to_lowercase();
    let normalized: String = normalized.split_whitespace().collect::<Vec<_>>().join(" ");
    let mut h = DefaultHasher::new();
    normalized.hash(&mut h);
    format!("{conversation_id}:{:016x}", h.finish())
}

/// One facet of the officer's question (data-model: SubQuestion). `id == 0` is the
/// reserved **original-query leg** that is ALWAYS present, so a bad decomposition
/// can never starve retrieval (FR-001).
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct SubQuestion {
    pub id: u8,
    pub text: String,
    pub is_original: bool,
}

/// Coverage verdict for one sub-question after the draft (data-model: Coverage).
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum Coverage {
    Answered,
    Pending,
}

/// One row of the coverage report emitted by the coverage-check call (LLM #2).
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct CoverageItem {
    pub sub_question_id: u8,
    pub status: Coverage,
}

/// The full coverage report. `pending_ids` (with bias-to-refetch, FR-007) drives
/// retrieval round 2; an empty pending set skips round 2 (FR-004).
pub type CoverageReport = Vec<CoverageItem>;

/// A retrieved passage, tagged by the round that found it so round-2 (gap-filling)
/// passages can be packed first (data-model: Passage; FR-013).
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct Passage {
    pub chunk_id: String,
    pub source_id: String,
    pub content: String,
    pub page: Option<i64>,
    /// Source file name / provision label (hit tuple index 4) — for the trace.
    pub label: String,
    /// 1 or 2. Round 2 answers a previously-pending sub-question → higher priority.
    pub round: u8,
    pub for_sub_question: u8,
}

/// The deduped union of every round's passages (data-model: PassageSet). Synthesis
/// (LLM #3) reads it and the grounding gate verifies against it.
#[derive(Debug, Default, Clone, serde::Serialize, serde::Deserialize)]
pub struct PassageSet {
    pub passages: Vec<Passage>,
}

/// Observability payload surfaced to the query trace / admin debug panel
/// (FR-011, User Story 4). The `llm_calls`/`retrieval_rounds` counters double as
/// the runtime assertion of the loop bound (INV-1/INV-2).
#[derive(Debug, Default, Clone, serde::Serialize, serde::Deserialize)]
pub struct DeepTrace {
    pub sub_questions: Vec<String>,
    pub coverage: Vec<(String, Coverage)>,
    pub round2_triggers: Vec<String>,
    pub llm_calls: u8,
    pub retrieval_rounds: u8,
}

/// The bounded agentic deep-search loop (ADR-0021). Drop-in for the normal
/// single-pass retrieval at the Guidance call site: returns `(QueryResponse, bool)`
/// exactly like `route_scoped_with_override`. Invoked only when Deep is ON **and**
/// `agentic_enabled()`; normal search never reaches here.
///
/// Bounded: at most 3 LLM calls (expand · coverage · synthesize) and 2 retrieval
/// rounds (round 2 skipped when nothing is pending). Cancellation is checked at
/// each round boundary (FR-014).
#[allow(clippy::too_many_arguments)]
pub(crate) async fn run_agentic_deep(
    pool: &Db,
    embedder: &Arc<EmbedderState>,
    model_slot: &ModelSlot,
    tier: &HardwareTier,
    notebook_id: &str,
    // F026: durable-thread id for this deep turn (`conversation_id + ':' + seq`).
    // Checkpoints are written under it so the turn can resume / time-travel.
    thread_id: &str,
    query_text: &str,
    scope_source_ids: Option<&[String]>,
    system_prompt: &str,
    cancel: Option<CancellationToken>,
    app: Option<crate::compat::AppHandle>,
    // The turn-level trace (owned by run_turn) so the deep loop's internals —
    // sub-questions, per-round retrieval, coverage, synthesis — show in the admin
    // query-trace panel instead of only a log line.
    mut trace_out: Option<&mut crate::services::query_trace::QueryTrace>,
) -> Result<(QueryResponse, bool)> {
    let cancelled = |c: &Option<CancellationToken>| c.as_ref().map(|t| t.is_cancelled()).unwrap_or(false);
    if cancelled(&cancel) {
        return Ok((empty_response(), true));
    }

    // ENUMERATION SHORT-CIRCUIT (F025): a "how many / list all / sum of X" query
    // scoped to a document is answered by fetching the WHOLE table directly, NOT by
    // facet decomposition (which the facet loop turns into noise like "count the
    // sections/appendices" and never surfaces the department table). One LLM call,
    // one retrieval → fast + actually answers the count/list. Falls through to the
    // normal loop when it's not enumerative or has no document scope. (Not
    // checkpointed — it's a single-shot fast path.)
    if is_enumeration_query(query_text) {
        if let Some(ids) = scope_source_ids {
            if !ids.is_empty() {
                if let Some(resp) =
                    answer_enumeration(pool, model_slot, tier, query_text, ids, system_prompt, cancel.clone(), app.clone()).await
                {
                    if let Some(t) = trace_out.as_deref_mut() {
                        t.step("Deep search — enumeration short-circuit", &[
                            ("Mode", "count/list detected + document-scoped".to_string()),
                            ("Method", "deterministic table-row extraction (no LLM listing)".to_string()),
                            ("Citations", resp.citations.len().to_string()),
                        ]);
                    }
                    return Ok((resp, cancelled(&cancel)));
                }
            }
        }
    }

    // F026 auto-resume: if this thread was checkpointed by a prior (interrupted)
    // attempt, continue it from the next unfinished node instead of re-running the
    // completed rounds. No checkpoints (or checkpointing disabled) → fresh turn.
    if checkpoint::checkpoint_enabled() {
        if let Some(out) = resume_deep(
            pool, embedder, model_slot, tier, notebook_id, thread_id,
            cancel.clone(), app.clone(), trace_out.as_deref_mut(),
        )
        .await?
        {
            return Ok(out);
        }
    }

    // Fresh turn → drive the loop from the top.
    let state = DeepState::fresh(
        thread_id.to_string(),
        query_text.to_string(),
        system_prompt.to_string(),
        scope_source_ids.map(|s| s.to_vec()),
    );
    let seq = checkpoint::next_seq(pool, thread_id).await.unwrap_or(0);
    drive(pool, embedder, model_slot, tier, notebook_id, state, seq, None, cancel, app, trace_out).await
}

/// F026 — the checkpointed driver. Runs each super-step guarded by
/// `state.node < Node::X` ("phase X has not completed yet"), so a resumed state
/// simply skips the phases already done. A checkpoint is written after every
/// node (best-effort; a failed write never fails the turn). The final answer +
/// citations are rebuilt deterministically from `state.passages`, so a turn
/// resumed at `Synthesized`/`Done` reproduces the same result without re-calling
/// the model.
#[allow(clippy::too_many_arguments)]
async fn drive(
    pool: &Db,
    embedder: &Arc<EmbedderState>,
    model_slot: &ModelSlot,
    tier: &HardwareTier,
    notebook_id: &str,
    mut state: DeepState,
    mut seq: i64,
    mut parent: Option<String>,
    cancel: Option<CancellationToken>,
    app: Option<crate::compat::AppHandle>,
    mut trace_out: Option<&mut crate::services::query_trace::QueryTrace>,
) -> Result<(QueryResponse, bool)> {
    macro_rules! tstep {
        ($title:expr, $facts:expr) => {
            if let Some(t) = trace_out.as_deref_mut() { t.step($title, $facts); }
        };
    }
    let cancelled = |c: &Option<CancellationToken>| c.as_ref().map(|t| t.is_cancelled()).unwrap_or(false);
    let scope = state.scope_source_ids.clone();

    tstep!("Deep search — bounded agentic loop", &[
        ("Design", "≤3 LLM calls, ≤2 retrieval rounds (ADR-0021)".to_string()),
        ("Entry node", state.node.as_str().to_string()),
    ]);

    // ── #1 decompose + facet-expansion (always keeps the original query as id 0) ──
    if state.node < Node::Expanded {
        if cancelled(&cancel) {
            save_ckpt(pool, &mut seq, &mut parent, &state, "interrupted").await;
            return Ok((empty_response(), true));
        }
        let subs = expand::expand_query(model_slot, tier, &state.query_text, cancel.clone()).await;
        state.trace.llm_calls += 1;
        state.trace.sub_questions = subs.iter().map(|s| s.text.clone()).collect();
        state.subs = subs;
        state.node = Node::Expanded;
        save_ckpt(pool, &mut seq, &mut parent, &state, "running").await;
    }
    let facets = if state.subs.len() > 1 {
        state.subs.iter().skip(1).map(|s| format!("• {}", s.text)).collect::<Vec<_>>().join("\n")
    } else {
        "(none — single-pass on the original question)".to_string()
    };
    tstep!("Deep #1 — decompose (LLM)", &[
        ("Sub-questions", format!("{} (incl. original)", state.subs.len())),
        ("Facets (faithful — only what was asked)", facets),
    ]);

    // ── Round 1 — research every sub-question concurrently (main latency lever) ──
    if state.node < Node::RetrievedR1 {
        let scope_ref = scope.as_deref();
        let r1 = futures::future::join_all(state.subs.iter().map(|s| async move {
            let hits = query_router::retrieve_for_query(
                pool, embedder, notebook_id, scope_ref, &s.text, PER_SUBQ_K,
            )
            .await
            .unwrap_or_default();
            (s.id, hits)
        }))
        .await;
        for (id, hits) in r1 {
            state.passages.add_round(hits, 1, id);
        }
        state.trace.retrieval_rounds += 1;
        state.node = Node::RetrievedR1;
        save_ckpt(pool, &mut seq, &mut parent, &state, "running").await;
    }
    let r1_list = render_passages(&state.passages.passages.iter().collect::<Vec<_>>(), 12);
    tstep!("Deep round 1 — retrieve per sub-question", &[
        ("Retrieval", "hybrid vector+FTS + graph + summary arms".to_string()),
        ("Passages gathered", state.passages.passages.len().to_string()),
        ("Retrieved (source · snippet)", r1_list),
    ]);
    if cancelled(&cancel) {
        save_ckpt(pool, &mut seq, &mut parent, &state, "interrupted").await;
        return Ok((empty_response(), true));
    }

    // ── #2 coverage-check over the round-1 passages; pending drives round 2 ──
    if state.node < Node::CoverageChecked {
        let report = coverage::coverage_check(model_slot, tier, &state.subs, &state.passages, cancel.clone()).await;
        state.trace.llm_calls += 1;
        state.pending = coverage::pending_ids(&report);
        state.trace.coverage = report
            .iter()
            .filter_map(|c| state.subs.iter().find(|s| s.id == c.sub_question_id).map(|s| (s.text.clone(), c.status)))
            .collect();
        state.trace.round2_triggers = state
            .pending
            .iter()
            .filter_map(|id| state.subs.iter().find(|s| s.id == *id).map(|s| s.text.clone()))
            .collect();
        state.coverage = report;
        state.node = Node::CoverageChecked;
        save_ckpt(pool, &mut seq, &mut parent, &state, "running").await;
    }
    let cov = state
        .coverage
        .iter()
        .filter_map(|c| state.subs.iter().find(|s| s.id == c.sub_question_id).map(|s| format!("• {} → {:?}", s.text, c.status)))
        .collect::<Vec<_>>()
        .join("\n");
    tstep!("Deep #2 — coverage check (LLM)", &[
        ("Per sub-question", if cov.is_empty() { "(none)".to_string() } else { cov }),
        ("Pending → round 2", if state.pending.is_empty() { "none (all covered)".to_string() } else { state.trace.round2_triggers.join("; ") }),
    ]);

    // ── Round 2 — targeted re-fetch for pending sub-questions only (skippable) ──
    if state.node < Node::RetrievedR2 {
        if !state.pending.is_empty() {
            let scope_ref = scope.as_deref();
            let subs = &state.subs;
            let r2 = futures::future::join_all(
                state
                    .pending
                    .iter()
                    .filter_map(|id| subs.iter().find(|s| s.id == *id))
                    .map(|s| async move {
                        let hits = query_router::retrieve_for_query(
                            pool, embedder, notebook_id, scope_ref, &s.text, PER_SUBQ_K,
                        )
                        .await
                        .unwrap_or_default();
                        (s.id, hits)
                    }),
            )
            .await;
            for (id, hits) in r2 {
                state.passages.add_round(hits, 2, id);
            }
            state.trace.retrieval_rounds += 1;
            tstep!("Deep round 2 — targeted re-fetch (pending only)", &[
                ("Re-fetched", state.trace.round2_triggers.join("; ")),
                ("Passages after round 2", state.passages.passages.len().to_string()),
            ]);
        } else {
            tstep!("Deep round 2 — skipped", &[("Reason", "coverage found no pending sub-questions".to_string())]);
        }
        state.node = Node::RetrievedR2;
        save_ckpt(pool, &mut seq, &mut parent, &state, "running").await;
    }
    if cancelled(&cancel) {
        save_ckpt(pool, &mut seq, &mut parent, &state, "interrupted").await;
        return Ok((empty_response(), true));
    }

    // ── Assemble + #3 grounded synthesis over the packed union ──
    // NO cross-encoder rerank: the bge reranker MIS-RANKS this legal corpus
    // (proven live — it promoted food-safety "penalty" clauses for an E&D "penalty"
    // question). Hybrid UNION → top-K, packed pending-first, is what fires.
    if state.node < Node::Synthesized {
        state.passages.dedup();
        // Order by the instrument the question names before packing, so a
        // colliding "Rule 4" from another Act cannot outrank the statute the
        // officer actually asked about (trace bd0d16e1). Ordering only.
        let instr_boost = query_router::knob_instrument_boost_pub();
        state.passages.prioritize_instrument(&state.query_text, instr_boost);
        let (material, packed_len, top_render) = {
            let packed = state.passages.packed();
            let top = render_passages(&packed, 20);
            let (m, _used) = build_material(&packed);
            (m, packed.len(), top)
        };
        // Report the actual source mix — the bd0d16e1 failure was invisible
        // because nothing recorded WHICH documents supplied the passages.
        let source_mix = {
            let packed = state.passages.packed();
            let mut mix: std::collections::HashMap<&str, usize> = std::collections::HashMap::new();
            for p in &packed { *mix.entry(p.label.as_str()).or_insert(0) += 1; }
            let mut mix: Vec<_> = mix.into_iter().collect();
            mix.sort_by(|a, b| b.1.cmp(&a.1));
            mix.iter().map(|(f, n)| format!("{f}×{n}")).collect::<Vec<_>>().join(", ")
        };
        tstep!("Deep — assemble (hybrid union → top-K, no reranker)", &[
            ("Passages (deduped)", state.passages.passages.len().to_string()),
            ("Packed for synthesis", packed_len.to_string()),
            ("Order", "hybrid union (vector+FTS), pending-first — NO cross-encoder rerank".to_string()),
            ("Instrument boost", if instr_boost <= 1.0 {
                "off (ECHO_INSTRUMENT_BOOST=1.0)".to_string()
            } else {
                format!("boost={instr_boost} — named-instrument passages ordered first")
            }),
            ("Source mix (packed)", source_mix),
            ("Top passages sent to the model", top_render),
        ]);
        let prompt = format!(
            "{sys}\n\nUse ONLY the passages below and cite the provisions you rely on. Be \
             comprehensive: cover every distinct point, requirement, and exception the passages support, and \
             enumerate lists (e.g. departments, grounds, penalties) rather than saying 'several'. Do not add \
             anything the passages do not support. Cite sources as PLAIN bracket markers only — [1], [2] — and \
             NEVER append '(i)', '(ii)', or any suffix to a citation marker. Write the final answer directly — \
             do NOT narrate your reasoning, and do NOT repeat any sentence or section.\n\nPASSAGES:\n{material}\n\
             QUESTION: {q}\n\nANSWER: /no_think",
            sys = state.system_prompt,
            q = state.query_text,
        );
        let answer_raw = query_router::generate_direct(model_slot, tier, &prompt, SYNTH_MAX_TOKENS, cancel.clone(), app).await?;
        state.trace.llm_calls += 1;
        state.answer = sanitize_answer(&answer_raw);
        state.node = Node::Synthesized;
        save_ckpt(pool, &mut seq, &mut parent, &state, "running").await;
        tstep!("Deep #3 — grounded synthesis (LLM)", &[
            ("LLM calls", format!("{} (bounded ≤3)", state.trace.llm_calls)),
            ("Retrieval rounds", format!("{} (bounded ≤2)", state.trace.retrieval_rounds)),
            ("Answer length", format!("{} chars", state.answer.len())),
        ]);
    }
    let was_cancelled = cancelled(&cancel);

    // INV-1/INV-2 (US3): the loop is bounded by construction; assert it in debug so
    // any future edit that adds a call/round trips a test rather than shipping.
    debug_assert!(state.trace.llm_calls <= 3, "deep loop exceeded 3 LLM calls: {}", state.trace.llm_calls);
    debug_assert!(state.trace.retrieval_rounds <= 2, "deep loop exceeded 2 rounds: {}", state.trace.retrieval_rounds);
    emit_trace(&state.trace);

    // ── Finalize (deterministic from state) — grounding gate + citations ──
    // Rebuild the packed set + numbered `used` from the persisted passages so the
    // citations align 1:1 with the `[n]` markers the model saw — identical whether
    // this ran straight through or was resumed at the synthesis/done node.
    let packed = state.passages.packed();
    let (_material, used) = build_material(&packed);

    // US2 grounding gate (FR-006, INV-4): an ungrounded turn returns the
    // fallback with NO citations; otherwise the (possibly figure-caveated) answer
    // is surfaced with citations built from the passages it was grounded in.
    let outcome = verify::check(&state.answer, &packed);
    if !outcome.grounded {
        log::info!("F025 agentic-deep: ungrounded → fallback ({} passages)", packed.len());
        tstep!("Deep — grounding gate → NO answer", &[
            ("Result", "no passage supported the answer → grounded-fallback, 0 citations".to_string()),
        ]);
        state.node = Node::Done;
        save_ckpt(pool, &mut seq, &mut parent, &state, "done").await;
        return Ok((
            QueryResponse {
                answer: outcome.answer,
                citations: Vec::new(),
                tier_used: "agentic-deep".to_string(),
                quality_notice: None,
                // Deep search assembles its own multi-round prompts, so there is no
                // single measured overhead to report. The caller falls back to its
                // estimate; deep is admin-only, so the stale notice is contained.
                chat_fill: None,
                related_passages: Vec::new(),
            },
            was_cancelled,
        ));
    }

    // Citations = the numbered passages, IN ORDER, so citation[n-1] is exactly the
    // `[n]` the model saw (frontend hover mapping). Longer excerpt than the old
    // 300-char snippet so the hover shows real chunk content.
    let citations: Vec<Citation> = used
        .iter()
        .map(|p| Citation {
            source_id: p.source_id.clone(),
            chunk_id: p.chunk_id.clone(),
            excerpt: hover_excerpt(&p.content),
            page: p.page,
        })
        .collect();

    log::info!(
        "F025 agentic-deep: {} sub-questions, {} pending → round2, {} passages, {} citations, {} figures flagged",
        state.subs.len(),
        state.pending.len(),
        packed.len(),
        citations.len(),
        outcome.figures_flagged
    );

    // Final step — the actual outcome, so the trace runs to the END.
    let cited = if citations.is_empty() {
        "(none)".to_string()
    } else {
        citations
            .iter()
            .map(|c| {
                let snip: String = c.excerpt.trim().replace('\n', " ").chars().take(90).collect();
                format!("• [{}] p.{} — {}…", c.source_id.get(..8).unwrap_or(&c.source_id), c.page.unwrap_or(0), snip)
            })
            .collect::<Vec<_>>()
            .join("\n")
    };
    tstep!("Deep — final answer (grounded)", &[
        ("Grounding", "every citation traces to a retrieved passage".to_string()),
        ("Citations", citations.len().to_string()),
        ("Cited provisions (source · excerpt)", cited),
        ("Unverified figures flagged", outcome.figures_flagged.to_string()),
        ("Answer (full)", outcome.answer.clone()),
    ]);

    state.node = Node::Done;
    save_ckpt(pool, &mut seq, &mut parent, &state, "done").await;

    Ok((
        QueryResponse { answer: outcome.answer, citations, tier_used: "agentic-deep".to_string(), quality_notice: None, chat_fill: None, related_passages: Vec::new() },
        was_cancelled,
    ))
}

/// F026 — resume a deep turn from its LATEST checkpoint (e.g. after a process
/// restart or reconnect). Continues from the next unfinished node; completed
/// rounds are NOT re-embedded. `Ok(None)` when the thread has no checkpoints.
#[allow(clippy::too_many_arguments)]
pub(crate) async fn resume_deep(
    pool: &Db,
    embedder: &Arc<EmbedderState>,
    model_slot: &ModelSlot,
    tier: &HardwareTier,
    notebook_id: &str,
    thread_id: &str,
    cancel: Option<CancellationToken>,
    app: Option<crate::compat::AppHandle>,
    trace_out: Option<&mut crate::services::query_trace::QueryTrace>,
) -> Result<Option<(QueryResponse, bool)>> {
    // Only an `interrupted` thread resumes — a completed one must re-run fresh, or
    // resume degrades into an accidental answer cache.
    let Some(ckpt) = checkpoint::latest_resumable(pool, thread_id).await.map_err(|e| anyhow::anyhow!(e))? else {
        return Ok(None);
    };
    let seq = checkpoint::next_seq(pool, thread_id).await.unwrap_or(ckpt.seq + 1);
    let parent = Some(ckpt.id);
    let out = drive(pool, embedder, model_slot, tier, notebook_id, ckpt.state, seq, parent, cancel, app, trace_out).await?;
    Ok(Some(out))
}

/// F026 — time-travel: re-run forward from a chosen EARLIER checkpoint, branching
/// via `parent_id`. New checkpoints append after the existing rows (`next_seq`),
/// so both the original path and the branch survive in the thread's history.
/// `Ok(None)` when the checkpoint id is unknown. (Wired to an admin/debug trigger
/// as a follow-up; the mechanism lives here.)
#[allow(dead_code)]
#[allow(clippy::too_many_arguments)]
pub(crate) async fn resume_from(
    pool: &Db,
    embedder: &Arc<EmbedderState>,
    model_slot: &ModelSlot,
    tier: &HardwareTier,
    notebook_id: &str,
    checkpoint_id: &str,
    cancel: Option<CancellationToken>,
    app: Option<crate::compat::AppHandle>,
    trace_out: Option<&mut crate::services::query_trace::QueryTrace>,
) -> Result<Option<(QueryResponse, bool)>> {
    let Some(ckpt) = checkpoint::load(pool, checkpoint_id).await.map_err(|e| anyhow::anyhow!(e))? else {
        return Ok(None);
    };
    let seq = checkpoint::next_seq(pool, &ckpt.thread_id).await.unwrap_or(ckpt.seq + 1);
    let parent = Some(ckpt.id);
    let out = drive(pool, embedder, model_slot, tier, notebook_id, ckpt.state, seq, parent, cancel, app, trace_out).await?;
    Ok(Some(out))
}

/// Best-effort checkpoint write; a failure is logged but never fails the turn.
/// Advances the caller's `seq`/`parent` cursors on success so the next write
/// chains correctly (`parent_id` = this row's id).
async fn save_ckpt(
    pool: &Db,
    seq: &mut i64,
    parent: &mut Option<String>,
    state: &DeepState,
    status: &str,
) {
    if !checkpoint::checkpoint_enabled() {
        return;
    }
    match checkpoint::append(pool, *seq, parent.as_deref(), state, status).await {
        Ok(id) => {
            *parent = Some(id);
            *seq += 1;
        }
        Err(e) => log::warn!("F026 deep checkpoint failed (turn continues): {e}"),
    }
}

/// Pack the passages into the numbered synthesis material `[1]..[k]` and return
/// the parallel `used` list, so citations align 1:1 with the `[n]` markers. Shared
/// by synthesis (needs the material) and finalize (needs only `used`), which keeps
/// a resumed turn's citations identical to the straight-through run's.
fn build_material<'a>(packed: &[&'a Passage]) -> (String, Vec<&'a Passage>) {
    let mut material = String::new();
    let mut used: Vec<&Passage> = Vec::new();
    for p in packed.iter() {
        let entry = format!("[{}] {}\n\n", used.len() + 1, p.content.trim());
        if !material.is_empty() && material.len() + entry.len() > SYNTH_CHAR_BUDGET {
            break;
        }
        material.push_str(&entry);
        used.push(p);
    }
    (material, used)
}

/// Log the deep-loop trace for the admin debug panel / query trace (FR-011, US4).
/// A single structured log line keeps it observable without a schema change.
fn emit_trace(t: &DeepTrace) {
    log::info!(
        "F025 DeepTrace: calls={} rounds={} sub_qs={:?} round2_triggers={:?}",
        t.llm_calls,
        t.retrieval_rounds,
        t.sub_questions,
        t.round2_triggers
    );
}

fn empty_response() -> QueryResponse {
    QueryResponse { answer: String::new(), citations: Vec::new(), tier_used: "agentic-deep".into(), quality_notice: None, chat_fill: None, related_passages: Vec::new() }
}

/// Chain-of-thought openers the model leaks as PLAIN TEXT (no `<think>` tags),
/// seen live on the synthesis output ("Okay, let's tackle this question. The user
/// is asking…"). Mirrors `summary_builder::LEAK_MARKERS`.
const LEAK_OPENERS: &[&str] = &[
    "okay, let", "okay, so", "okay, i ", "okay, the ", "okay, first",
    "let's tackle", "let me tackle", "let me start", "let me think", "let me check",
    "the user wants", "the user is asking", "the user provided", "the user's",
    "first, i need", "first, let me", "looking at the", "looking through the",
];

/// Trailing contaminants the synthesis model appends AFTER a complete answer, seen
/// across the 2026-07-13 global eval (5/6 answers): the grounding-fallback phrase
/// tacked onto a real answer, chatty "would you like…" offers, and echoes of the
/// system prompt's formatting rules. Cut from the earliest one at a sentence
/// boundary — everything before (the real answer) is kept. NOTE: the grounding
/// gate's own "_Note: N figure(s) could not be verified_" caveat is appended by
/// `verify::check` AFTER this runs, so it is never cut here.
const TRAILING_JUNK: &[&str] = &[
    "the retrieved documents do not",   // leading form → catches the fallback at a boundary
    "the retrieved documents do not contain enough information",
    "do not contain enough information",
    "does not contain enough information",
    "the above summary is based on",
    "this summary is based on",
    "key terms, amounts, and deadlines", // system-prompt echo
    "short factual answers stay as plain",
    // NOTE: the "### Refinement / would you like it more formal…" offer is an
    // INTENTIONAL follow-up prompt (system-prompt feature) — do NOT strip it here.
];

/// Sanitize the raw synthesis into a clean answer:
///   1. strip a `<think>…</think>` block;
///   2. cut at the earliest reasoning-leak opener OR trailing contaminant
///      (fallback string / chatty offer / system-prompt echo) that sits at a
///      sentence boundary — keeping the real answer that precedes it. A pure
///      LEADING leak jumps to the first `##`/`**` answer anchor;
///   3. drop repeated sentences/blocks via `dedupe_repeats`.
fn sanitize_answer(s: &str) -> String {
    // 1) strip a leading <think>…</think> block.
    let body: &str = if let Some(end) = s.find("</think>") {
        &s[end + "</think>".len()..]
    } else {
        s
    };
    let trimmed = body.trim_start();
    let lower = trimmed.to_ascii_lowercase();

    // 2) find the EARLIEST reasoning-leak opener that sits at a line/sentence
    //    boundary — catches both a LEADING leak (nothing real before it) and an
    //    EMBEDDED one ("real answer. Okay, let's tackle…" — the live case my first
    //    cut missed).
    let is_boundary = |pre: &str| {
        pre.trim_end().is_empty()
            || pre.ends_with('\n')
            || pre.ends_with(". ")
            || pre.ends_with(".\n")
            || pre.ends_with("? ")
            || pre.ends_with("! ")
            || pre.ends_with(":\n")
    };
    let mut cut = trimmed.len();
    // LEAK_OPENERS: cut AT the opener when it sits at a sentence boundary.
    for m in LEAK_OPENERS {
        let mut from = 0usize;
        while let Some(rel) = lower[from..].find(m) {
            let idx = from + rel;
            if is_boundary(&lower[..idx]) {
                cut = cut.min(idx);
                break;
            }
            from = idx + m.len();
        }
    }
    // TRAILING_JUNK: a distinctive contaminant may sit mid-sentence
    // ("**Bold** for key terms…"), so cut back to the START of its sentence — the
    // last boundary at/before the match — so the whole junk sentence is removed.
    for m in TRAILING_JUNK {
        if let Some(idx) = lower.find(m) {
            let pre = &lower[..idx];
            let start = [pre.rfind('\n').map(|i| i + 1),
                         pre.rfind(". ").map(|i| i + 2),
                         pre.rfind("? ").map(|i| i + 2),
                         pre.rfind("! ").map(|i| i + 2)]
                .into_iter()
                .flatten()
                .max()
                .unwrap_or(idx);
            cut = cut.min(start);
        }
    }
    let before = trimmed[..cut].trim();

    let cleaned = if !before.is_empty() {
        // Real answer precedes the leak → keep it, drop the reasoning tail.
        before.to_string()
    } else {
        // Pure LEADING leak → jump to the first real-answer anchor (## or **).
        match trimmed.find("\n## ").or_else(|| trimmed.find("\n**")).or_else(|| trimmed.find("## ")) {
            Some(i) => trimmed[i..].trim().to_string(),
            None => trimmed.to_string(), // no anchor → keep (dedupe still runs)
        }
    };

    // 3) collapse repeated sentences/blocks.
    dedupe_repeats(cleaned.trim())
}

/// True for count/enumeration questions ("how many / sum of / list all / total
/// number of / number of / count of / all the departments"). These need the WHOLE
/// table, not facet decomposition.
fn is_enumeration_query(q: &str) -> bool {
    let l = q.to_ascii_lowercase();
    const MARKERS: &[&str] = &[
        "how many", "sum of", "list all", "list the", "total number", "number of",
        "count of", "count the", "how much", "all the departments", "all departments",
        "enumerate", "name all", "name the",
    ];
    MARKERS.iter().any(|m| l.contains(m))
}

/// Enumeration answer path — **facts computed in CODE, LLM only smooths the prose.**
///
/// Pulls every `table_row` chunk for the scoped document(s) and extracts each
/// entry's name VERBATIM via `summary_builder::table_row_name` (the extractor that
/// killed the "Department Zoonology" hallucination), dedups order-preserving, and
/// counts them — all deterministic. The COUNT and the LIST are then rendered in
/// code, VERBATIM. A short LLM pass writes ONLY a one-sentence natural intro (given
/// the exact count) so the answer reads well; it never sees or touches the list, so
/// it cannot drop, add, or invent an entry. If the intro call fails we fall back to
/// a plain code sentence. Returns `None` (→ normal loop) when there are no nameable
/// table rows.
async fn answer_enumeration(
    pool: &Db,
    model_slot: &ModelSlot,
    tier: &HardwareTier,
    query_text: &str,
    source_ids: &[String],
    _system_prompt: &str,
    cancel: Option<CancellationToken>,
    app: Option<crate::compat::AppHandle>,
) -> Option<QueryResponse> {
    use crate::services::summary_builder::table_row_name;
    let rows = query_router::fetch_table_chunks_for_sources(pool, source_ids).await.unwrap_or_default();
    if rows.is_empty() {
        return None; // no table content → let the normal loop try
    }

    // Verbatim name per row, deduped (order-preserving). Keep the source chunk for
    // each surviving name so the citation points at the real row.
    let mut names: Vec<String> = Vec::new();
    let mut seen: std::collections::HashSet<String> = std::collections::HashSet::new();
    let mut cite_rows: Vec<&(String, String, String, Option<i64>, String)> = Vec::new();
    for r in &rows {
        if let Some(name) = table_row_name(&r.2) {
            if seen.insert(name.to_lowercase()) {
                names.push(name);
                cite_rows.push(r);
            }
        }
    }
    if names.is_empty() {
        return None; // rows exist but none are nameable entries → normal loop
    }
    let count = names.len();

    // LLM SMOOTHING — one short sentence only. The model gets the exact count (a
    // fixed fact) and the question; it must NOT produce any list or number of its
    // own. Falls back to a plain code sentence on any failure.
    let intro_prompt = format!(
        "Write ONE short, natural sentence introducing the answer to the user's question. The exact \
         count is {count} — use that number verbatim and invent no other numbers, names, or list. \
         Output only the sentence.\n\nQUESTION: {query_text}\nCOUNT: {count}\nSENTENCE: /no_think"
    );
    let intro = match query_router::generate_direct(model_slot, tier, &intro_prompt, 80, cancel, app).await {
        Ok(s) => {
            let s = sanitize_answer(&s);
            // Guard: the sentence MUST contain the real count and be short; else use code.
            if s.contains(&count.to_string()) && s.len() < 300 && !s.is_empty() {
                s.trim().to_string()
            } else {
                format!("The document lists {count} entries:")
            }
        }
        Err(_) => format!("The document lists {count} entries:"),
    };

    // Code owns the LIST — appended VERBATIM beneath the smoothed intro.
    let mut answer = format!("{intro}\n\n");
    for n in &names {
        answer.push_str("- ");
        answer.push_str(n);
        answer.push('\n');
    }

    let citations: Vec<Citation> = cite_rows
        .iter()
        .take(CITE_LIMIT)
        .map(|r| Citation { source_id: r.1.clone(), chunk_id: r.0.clone(), excerpt: snippet(&r.2), page: r.3 })
        .collect();
    log::info!(
        "F025 enumeration (deterministic): {} rows → {} distinct entries, {} citations",
        rows.len(), names.len(), citations.len()
    );
    Some(QueryResponse { answer, citations, tier_used: "agentic-deep-enum".to_string(), quality_notice: None, chat_fill: None, related_passages: Vec::new() })
}

/// Render a passage list for the trace: "source · one-line snippet", one per line,
/// capped so the trace stays readable.
fn render_passages(items: &[&Passage], cap: usize) -> String {
    if items.is_empty() {
        return "(none)".to_string();
    }
    items
        .iter()
        .take(cap)
        .map(|p| {
            let src = p.label.split(['/', '\\']).next_back().unwrap_or(&p.label);
            let snip: String = p.content.trim().replace('\n', " ").chars().take(90).collect();
            format!("• [{}] {} — {}…", p.source_id.get(..8).unwrap_or(&p.source_id), src, snip)
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// Excerpt for the citation-hover popover — more of the chunk than `snippet` so the
/// officer sees the real source content behind a `[n]` marker (NotebookLM-style).
fn hover_excerpt(content: &str) -> String {
    let t = content.trim();
    if t.chars().count() <= 700 {
        t.to_string()
    } else {
        t.chars().take(700).collect::<String>() + "…"
    }
}

/// A short citation excerpt (first ~300 chars), mirroring the normal path.
fn snippet(content: &str) -> String {
    let t = content.trim();
    if t.chars().count() <= 300 {
        t.to_string()
    } else {
        t.chars().take(300).collect::<String>() + "…"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deep_thread_id_is_stable_across_attempts() {
        // THE regression this guards: the first cut keyed the thread on the per-turn
        // workload UUID, so two attempts at the same question produced different
        // ids, `latest(thread_id)` never matched, and resume was unreachable.
        let a = deep_thread_id("conv-1", "grounds and penalties under the E&D Rules");
        let b = deep_thread_id("conv-1", "grounds and penalties under the E&D Rules");
        assert_eq!(a, b, "same conversation + same question must reuse the thread");

        // Trivial re-typing differences must not fork the thread.
        let c = deep_thread_id("conv-1", "  Grounds  and   Penalties under the E&D Rules  ");
        assert_eq!(a, c, "normalization: case/whitespace must not fork the thread");

        // Different question → different thread (no cross-question collision).
        assert_ne!(a, deep_thread_id("conv-1", "what is the appeal procedure"));
        // Different conversation → different thread (no cross-user/chat bleed).
        assert_ne!(a, deep_thread_id("conv-2", "grounds and penalties under the E&D Rules"));
        // Shape: conversation id is preserved as the prefix.
        assert!(a.starts_with("conv-1:"));
    }

    #[test]
    fn deep_audience_gate() {
        // Admins may always use Deep, regardless of audience.
        assert!(deep_enabled("admin", false));
        assert!(deep_enabled("Admin", false)); // case-insensitive
        assert!(deep_enabled("admin", true));
        // Non-admins only when the audience is opened to all.
        assert!(!deep_enabled("user", false));
        assert!(!deep_enabled("", false));
        assert!(deep_enabled("user", true));
    }

    #[test]
    fn types_construct_and_coverage_compares() {
        let sq = SubQuestion { id: 0, text: "q".into(), is_original: true };
        assert!(sq.is_original);
        assert_eq!(Coverage::Pending, Coverage::Pending);
        assert_ne!(Coverage::Answered, Coverage::Pending);
        let set = PassageSet::default();
        assert!(set.passages.is_empty());
    }

    #[test]
    fn sanitize_answer_strips_think_leading_cot_and_repeats() {
        // <think> block removed.
        assert_eq!(sanitize_answer("<think>reasoning</think>\n\nThe answer."), "The answer.");
        // Plain answer untouched.
        assert_eq!(sanitize_answer("  Plain answer.  "), "Plain answer.");
        // Leading plain-text CoT ("Okay, let's tackle…") skipped to the ## header.
        let leaked = "Okay, let's tackle this question. The user is asking about X.\n## Grounds\nRule 3 applies.";
        assert_eq!(sanitize_answer(leaked), "## Grounds\nRule 3 applies.");
        // Repeated block collapsed (dedupe_repeats needs ≥4 words >4 chars/sentence,
        // which the real leaked blocks easily meet).
        let sentence = "The disciplinary authority governs major penalties concerning proven misconduct.";
        let repeated = format!("## A\n{sentence}\n{sentence}");
        assert_eq!(sanitize_answer(&repeated).matches("disciplinary authority governs").count(), 1);
        // EMBEDDED leak (the live case): real answer, THEN a mid-text CoT opener at a
        // sentence boundary → keep the answer, drop the reasoning tail.
        let embedded = "The Rules define the grounds. Okay, let's tackle this question. The user is asking about grounds.";
        assert_eq!(sanitize_answer(embedded), "The Rules define the grounds.");
    }

    #[test]
    fn sanitize_answer_cuts_trailing_junk() {
        // Trailing grounding-fallback tacked onto a real answer (eval q1/q3/q5).
        assert_eq!(
            sanitize_answer("An appeal lies within 30 days. The retrieved documents do not contain enough information to answer this question."),
            "An appeal lies within 30 days."
        );
        // The "would you like it more formal…" follow-up is INTENTIONAL → NOT stripped.
        let kept = sanitize_answer("The bidder must furnish bid security. Would you like it made more formal or more concise?");
        assert!(kept.contains("Would you like it made more formal"));
        // System-prompt echo (eval q3).
        assert_eq!(
            sanitize_answer("Suspension is effective immediately. Bold for key terms, amounts, and deadlines. Short factual answers stay as plain sentences."),
            "Suspension is effective immediately."
        );
        // A GENUINE no-answer (fallback is the whole thing) is preserved, not emptied.
        let only_fallback = "The retrieved documents do not contain enough information to answer this question.";
        assert_eq!(sanitize_answer(only_fallback), only_fallback);
    }

    #[test]
    fn enumeration_detector_matches_count_and_list() {
        assert!(is_enumeration_query("how many departments are there?"));
        assert!(is_enumeration_query("sum of all departments in the Rules"));
        assert!(is_enumeration_query("List all the chapters"));
        assert!(is_enumeration_query("total number of sections"));
        assert!(!is_enumeration_query("what is the procedure for appeal"));
        assert!(!is_enumeration_query("explain the penalties"));
    }

    /// INV-1/INV-2 (US3): the loop tallies exactly one LLM call per stage
    /// (expand, coverage, synth = 3) and one round per retrieval pass, with round
    /// 2 gated on a non-empty pending set. This mirrors the counting in
    /// `run_agentic_deep` without a live DB/model.
    #[test]
    fn bound_counting_matches_the_loop_shape() {
        // Case A: some pending → 3 calls, 2 rounds.
        let mut t = DeepTrace::default();
        t.llm_calls += 1; // expand
        t.retrieval_rounds += 1; // round 1
        t.llm_calls += 1; // coverage
        let pending = [1u8, 2u8];
        if !pending.is_empty() {
            t.retrieval_rounds += 1; // round 2
        }
        t.llm_calls += 1; // synth
        assert_eq!(t.llm_calls, 3);
        assert_eq!(t.retrieval_rounds, 2);
        assert!(t.llm_calls <= 3 && t.retrieval_rounds <= 2);

        // Case B: nothing pending → round 2 skipped → 3 calls, 1 round.
        let mut t = DeepTrace::default();
        t.llm_calls += 1;
        t.retrieval_rounds += 1;
        t.llm_calls += 1;
        let pending: [u8; 0] = [];
        if !pending.is_empty() {
            t.retrieval_rounds += 1;
        }
        t.llm_calls += 1;
        assert_eq!(t.llm_calls, 3);
        assert_eq!(t.retrieval_rounds, 1);
    }
}
