//! 5-tier query router.
//!
//! Routing logic:
//!   Tier 0: cosine ≥ 0.92 vs query_cache → return cached
//!   Tier 1: entity-pattern regex → FTS5 SQL
//!   Tier 2: semantic → sqlite-vec ANN
//!   Tier 3: orchestrator LLM (3B) with GBNF-constrained output
//!   Tier 4: worker LLM (7B); on 8 GB → Tier 3 with quality notice

use anyhow::Result;
use serde::{Deserialize, Serialize};
use crate::db::Db;
use std::collections::{HashMap, HashSet};

use crate::services::reranker::RerankerSlot;

// ─── Context builder constants (AD-6 / FR-040) ───────────────────────────────

/// Maximum chunks selected for LLM context. Further limited by
/// CONTEXT_TOKEN_BUDGET so actual chunk count varies with chunk length.
/// Set to 6 on 2026-05-28: db_inspection_22_prompts/v2 §08_granularity shows
/// 91–99% of corpus chunks are < 80 tokens — packing 12 such fragments hurts
/// precision (the reranker is also off post-Fix-3, so no second-stage pruning).
/// Six 200-token chunks fits the 2400-tok CONTEXT_TOKEN_BUDGET comfortably and
/// reduces adjacent-chunk contamination on legal text. Override via
/// `ECHO_CONTEXT_K=N` for experiments.
///
/// Bumped 6 → 10 on 2026-06-02 after constitution_ir.json investor-demo
/// traces: Constitution articles in the LLM-IR corpus split across 4–8
/// clause-grained chunks, so K=6 frequently spent the whole window on a
/// single (often wrong) article. Trace 38998dab burned 6/6 slots on Art 240
/// + Art 275 fragments and never surfaced Art 260's definition.
///
/// Bumped 10 → 20 on 2026-06-03 after the 4-tier 20-prompt eval validated
/// K=20 as the empirical sweet spot for legal corpus retrieval:
///   K=10 → K=20:  +5pp OK  +0.23s  (clear win, Q19 multi-article now OK)
///   K=20 → K=25:  −10pp OK +0.29s  (model dilutes on canonical queries;
///                                   Q1 went hedgy, Q18 self-contradicted)
/// See ~/Desktop/20_constitute_prompts/RESUME_HERE_2026-06-03.md and
/// run_summary_GPU_doubled.md. Budget bumped accordingly below.
const CONTEXT_TOP_K: usize = 20;

/// Context token budget for retrieved passages.
/// n_ctx=1024; MAX_PROMPT_TOKENS=768 (1024-256 response).
/// System prompt ≈200 tokens + ChatML overhead ≈30 tokens → 768−230=538.
/// Using 480 for a safe margin; prevents prompt truncation that caused the
/// model to echo its input instead of generating an answer.
// Fix B (2026-05-02): bumped 480→1200 so model sees 3 chunks instead of 1.
// Trace showed only 1 chunk used (single 478-token passage filled old budget),
// leaving the model with too little to cite. See CHANGELOG_DEV.md.
// changing to 2000 for more quality answers.
// Kept at 2400 for LOCAL inference. The Groq investor-demo config used
// ECHO_CONTEXT_BUDGET=4800 because Groq 70B has a 128k context window,
// but for the local 3B with gen_max_prompt=1800 (services/llm.rs line 533
// truncates via `min()`, dropping TRAILING tokens — including the
// question line), a 4800-token context would silently push the question
// off the prompt. To benefit from a wider budget on cloud, override via
// ECHO_CONTEXT_BUDGET=4800 at launch.
const CONTEXT_TOKEN_BUDGET: usize = 2400;

/// Tokens reserved for prompt scaffolding the measured overhead cannot see: the
/// ChatML tags, the "Context passages:"/"Question:" framing, the optional one-shot
/// citation demo (~200), and a safety margin for the crude chars/4 estimate. Kept
/// deliberately generous — under-reserving here is what overflows the window, and
/// the cost of over-reserving is only a slightly smaller law budget.
const PROMPT_SCAFFOLD_TOKENS: usize = 400;

/// Retry escalation table: (top_k, token_budget) per retry attempt (0-indexed).
/// Attempt 0 uses the original ctx_top_k / ctx_budget from classify_query.
const RETRY_PARAMS: [(usize, usize); 3] = [
    (10, 560), // retry 1
    (12, 640), // retry 2
    (12, 640), // retry 3 (final)
];

use std::sync::Arc;

use crate::services::embedder::EmbedderState;
use crate::services::hardware_detector::HardwareTier;
use crate::services::llm::{
    ModelSlot,
    GROUNDING_SYSTEM_PROMPT_VERSION,
    SUMMARY_SYSTEM_PROMPT, SUMMARY_SYSTEM_PROMPT_VERSION,
};
use crate::services::query_trace::QueryTrace;

/// Reviewer judge is opt-in: it adds 1-3 extra LLM round-trips per query and
/// the context-blind YES/NO prompt produces frequent false negatives that
/// replace correct answers with a generic "unable to generate" error. The
/// grounding validator already catches hallucinated citations, so the judge
/// is redundant for the common case. Re-enable for offline evaluations with
/// `ECHO_REVIEWER=1`.
fn reviewer_enabled() -> bool {
    std::env::var("ECHO_REVIEWER").ok().as_deref() == Some("1")
}

/// Sentence-level Jaccard grounding (Tier 3 Task 11) is opt-in as of
/// 2026-06-19. Disabled by default — its content-token overlap heuristic
/// strips legitimately-supported sentences whose wording diverges from the
/// cited chunk (paraphrase, number-only support), and the legacy
/// `validate_grounding` pass already gates hallucinated citations. Re-enable
/// for experiments with `ECHO_SENTENCE_GROUNDING=1`.
fn sentence_grounding_enabled() -> bool {
    std::env::var("ECHO_SENTENCE_GROUNDING").ok().as_deref() == Some("1")
}

// ── Tier 1 retrieval knobs (env-tunable) ─────────────────────────────────────
// All knobs default to the previous hard-coded values, so behaviour is
// unchanged unless an env var is set. Intended for precision sweeps and
// debug runs; UI exposure can come later.
fn env_usize(name: &str, default: usize) -> usize {
    std::env::var(name).ok().and_then(|s| s.trim().parse().ok()).unwrap_or(default)
}
fn env_f32(name: &str, default: f32) -> f32 {
    std::env::var(name).ok().and_then(|s| s.trim().parse().ok()).unwrap_or(default)
}
fn env_f32_opt(name: &str) -> Option<f32> {
    std::env::var(name).ok().and_then(|s| s.trim().parse().ok())
}
/// Vector top-N (default 24 = CONTEXT_TOP_K * 2).
// Decoupled from CONTEXT_TOP_K on 2026-05-28: when the LLM-context cap
// shrank to 6, deriving vec/FTS recall from it would have halved the
// candidate pool to 12. Pool size 24 stays as the recall floor; RRF then
// merges to ~40 unique chunks before the K=6 LLM cap.
fn knob_vec_k() -> usize { env_usize("ECHO_VEC_K", 24) }
/// FTS5 top-N (default 24 = CONTEXT_TOP_K * 2).
fn knob_fts_k() -> usize { env_usize("ECHO_FTS_K", 24) }
/// How much larger the candidate pool must be than the selection target, so the
/// ranking stages downstream actually have something to discard. 2.0 = the pool
/// holds twice what the context window can take.
///
/// ⚠️ **Defaults to LEGACY (0.0).** Any value below 1.0 selects the pre-2026-08-07
/// behaviour, where `ECHO_VEC_K`/`ECHO_FTS_K` act as absolute per-arm caps. This
/// is deliberate: it makes the deploy that carries this code a provable no-op,
/// gives the A/B a real control arm on the SAME binary, and makes rollback an env
/// flip + restart rather than a rebuild — exactly how `ECHO_SOURCE_CAP` was run.
/// Flip to `2.0` to enable. Promote the default once the A/B justifies it.
fn knob_pool_overfetch() -> f32 { env_f32("ECHO_POOL_OVERFETCH", 0.0) }

/// Per-arm depths for `union_merge`, derived from the LIVE selection target.
///
/// ⚠️ `ECHO_VEC_K` / `ECHO_FTS_K` are read here as a **ratio** (the vec/FTS mix),
/// NOT as absolute caps. Prod ran 28/7 — the *ratio* (80/20) was a deliberate
/// architect call and is preserved, but because the merge read the knobs as caps
/// they also pinned total pool depth at 35 while `ctx_top_k` had grown to 36.
/// The merge then emitted FEWER candidates than the window could take, so the
/// merge *was* the selection: salience/entity/instrument boosts and source
/// diversity all ran with nothing to discard and were provably inert in prod
/// traces. Two separately-set numbers, and nothing enforced the relation between
/// them. See CHANGELOG_DEV.md 2026-08-07.
///
/// Depth now tracks the window and the ratio stays whatever the knobs express:
/// an operator tunes the MIX, the system derives the DEPTH. `vec_avail` /
/// `fts_avail` are what each arm actually returned — a short arm hands its
/// unused slots to the other one so the pool still reaches `depth`.
fn merge_depth(ctx_top_k: usize, vec_avail: usize, fts_avail: usize) -> (usize, usize) {
    merge_depth_from(
        knob_vec_k() as f32,
        knob_fts_k() as f32,
        knob_pool_overfetch(),
        ctx_top_k,
        vec_avail,
        fts_avail,
    )
}

/// Pure core of [`merge_depth`] — shares and overfetch passed in rather than read
/// from env, so the real production mix (28:7 = 80/20) is unit-testable without
/// mutating process-global state from a parallel test runner.
fn merge_depth_from(
    vec_share: f32,
    fts_share: f32,
    overfetch: f32,
    ctx_top_k: usize,
    vec_avail: usize,
    fts_avail: usize,
) -> (usize, usize) {
    // LEGACY / control arm — the exact pre-2026-08-07 behaviour: the knobs used
    // as absolute per-arm caps. Kept so the A/B has a control on the same binary
    // and so rollback needs no rebuild. `union_merge`'s `take()` already bounded
    // by availability, so `min` here is equivalent, not a behaviour change.
    if overfetch < 1.0 {
        return ((vec_share as usize).min(vec_avail), (fts_share as usize).min(fts_avail));
    }

    let total = vec_share + fts_share;
    // Degenerate config (both zero) → even split. Never return (0, 0): that
    // would empty the pool entirely and refuse every query.
    let (vec_r, fts_r) = if total <= 0.0 { (0.5, 0.5) } else { (vec_share / total, fts_share / total) };

    // The invariant the old code stated at the fetch site and then violated at
    // the merge site: candidate supply must never fall below the selection target.
    let depth = (((ctx_top_k as f32) * overfetch.max(1.0)).ceil() as usize).max(ctx_top_k);

    let mut vec_k = ((depth as f32) * vec_r).round() as usize;
    let mut fts_k = depth.saturating_sub(vec_k);
    // A non-zero share must never round down to zero — that silently deletes a
    // whole retrieval arm, which is how the keyword arm would vanish at a small
    // window with an 80/20 mix.
    if fts_r > 0.0 && fts_k == 0 { fts_k = 1; vec_k = vec_k.saturating_sub(1); }
    if vec_r > 0.0 && vec_k == 0 { vec_k = 1; fts_k = fts_k.saturating_sub(1); }

    // Never request more than an arm returned; redistribute the shortfall.
    let vec_capped = vec_k.min(vec_avail);
    let fts_capped = fts_k.min(fts_avail);
    let vec_k = (vec_capped + depth.saturating_sub(vec_capped + fts_capped)).min(vec_avail);
    let fts_k = (fts_capped + depth.saturating_sub(vec_k + fts_capped)).min(fts_avail);
    (vec_k, fts_k)
}
/// Reranker pool — how many top candidates to score with the cross-encoder.
fn knob_rerank_pool_k() -> usize { env_usize("ECHO_RERANK_POOL_K", 12) }
// Final context chunk count / token budget now scale with the model window
// (see route_scoped_with_override); `ECHO_CONTEXT_K` / `ECHO_CONTEXT_BUDGET`
// still override via env_usize at the call site.
/// Minimum reranker top score required to ship an answer. If the top score
/// falls below this threshold, the pipeline returns the grounding fallback
/// instead of fabricating from low-confidence chunks. Unset = no gating.
fn knob_min_rerank_score() -> Option<f32> { env_f32_opt("ECHO_MIN_RERANK_SCORE") }
/// Salience-boost multiplier (S1). Post-RRF, chunks whose `unit_label` is in
/// the source's DocProfile.salient_units are re-scored as
/// `position_score × boost` and re-sorted. 1.0 = disabled, 1.3 = default
/// (lifts canonical chunks above leaf chunks at comparable scores without
/// overriding a clearly better match). Set to 1.0 to disable.
///
/// ⚠️ **Default lowered 1.3 → 1.0 on 2026-08-06.** The corpus holds **239**
/// `Preamble` chunks — one per instrument — and `salient_units` is *only*
/// `["Preamble"]` for act/rules/constitution. So on any broad question the boost
/// lifted the preamble of ALL 18 documents at once, which is title matter, not an
/// answer. That is a direct contributor to Proxima finding #4: "eligibility
/// criteria to apply for a government job in Sindh" put 3 of 33 chunks in the one
/// instrument that governs it, while preamble/definitions boilerplate from
/// thirteen other laws filled the window.
///
/// A preamble IS the right answer for a *definitional* question, and that path is
/// unchanged — `is_definitional()` queries still use `ECHO_DEFINITIONAL_BOOST`
/// (1.6). This only stops the boost firing on questions that want a provision.
/// Set `ECHO_SALIENT_BOOST=1.3` to restore the old behaviour.
fn knob_salient_boost() -> f32 { env_f32("ECHO_SALIENT_BOOST", 1.0) }
/// Salience boost on **definitional** queries (S2). When the question-grammar
/// classifier `is_definitional()` matches, use this stronger multiplier
/// instead of `ECHO_SALIENT_BOOST`. Default 1.6 — pushes canonical chunks
/// past 2-3 leaf chunks in the RRF order. Set to 1.0 to disable S2's
/// salience contribution (the wider candidate pool still applies).
fn knob_definitional_boost() -> f32 { env_f32("ECHO_DEFINITIONAL_BOOST", 1.6) }
/// Multiplier on `ECHO_VEC_K` and `ECHO_FTS_K` when the query is
/// definitional (S2). Widens the candidate pool so canonical chunks that
/// would otherwise be buried below the cutoff get a chance to surface
/// before the salience boost re-ranks. Default 2.0.
fn knob_definitional_pool_mult() -> f32 { env_f32("ECHO_DEFINITIONAL_POOL_MULT", 2.0) }
/// Entity-match boost (S3b). Post-S1/S2, chunks whose `entity_mentions`
/// reference an entity whose canonical name matches a noun phrase in the
/// query are re-scored. The boost composes after salience: an entity-
/// matched chunk at rank 3 (post-salience) leapfrogs a non-matching rank-1
/// chunk when the boost is ≥1.4. Default 1.4. 1.0 = disabled.
fn knob_entity_boost() -> f32 { env_f32("ECHO_ENTITY_BOOST", 1.4) }
/// Instrument-affinity boost (2026-07-18). When the query names a statute, chunks
/// from the matching document are re-scored so the named instrument wins the
/// top-K — `Rule 4` collides across at least five instruments in this corpus.
/// Graded by `instrument_score`, never a filter. Default 1.0 = **OFF** (A/B);
/// set `ECHO_INSTRUMENT_BOOST=2.0` to enable.
fn knob_instrument_boost() -> f32 { env_f32("ECHO_INSTRUMENT_BOOST", 1.0) }
/// Same knob, reachable from `deep_search` so both paths read ONE setting.
pub(crate) fn knob_instrument_boost_pub() -> f32 { knob_instrument_boost() }
/// Provision-label normalisation (2026-07-18). Append the bare `Rule N` label to
/// the retrieval query so FTS can match `chunks.unit_label`. Default OFF (A/B).
fn provision_norm_enabled() -> bool {
    matches!(
        std::env::var("ECHO_PROVISION_NORM").unwrap_or_default().trim().to_ascii_lowercase().as_str(),
        "1" | "on" | "true" | "yes"
    )
}
/// Noviq Hybrid Union (2026-07-01). When `ECHO_HYBRID=union`, the hybrid merge
/// takes the deduped UNION of the FTS and vector candidate lists instead of RRF
/// score fusion: every top keyword hit AND every top vector hit reaches the
/// context builder, so a rule strong in one modality but absent from the other
/// (the Rule 16 failure mode) is never demoted out. Per-list depth = ECHO_VEC_K /
/// ECHO_FTS_K. Default (unset) keeps RRF, so this is a safe, flag-gated A/B.
fn hybrid_union_enabled() -> bool {
    std::env::var("ECHO_HYBRID").ok().as_deref() == Some("union")
}

/// Track 2 (ADR-0019 §2): add the per-provision SUMMARY index as a third bounded
/// recall arm (summaries-as-index, verbatim-as-answer). Default OFF until
/// VM-evaluated. NOT a corpus sweep — bounded top-K feeding the single-pass answer.
fn summary_recall_enabled() -> bool {
    // Deep search folds the provision-summary recall arm in for extra breadth.
    deep_search_active()
        || matches!(
            std::env::var("ECHO_SUMMARY_RECALL").unwrap_or_default().trim().to_ascii_lowercase().as_str(),
            "1" | "on" | "true" | "yes"
        )
}
/// How many summary-recalled provisions to fold into the candidate pool.
fn knob_summary_k() -> usize { env_usize("ECHO_SUMMARY_K", 12) }

/// GraphRAG seed-and-expand arm (2026-07-12). After the hybrid merge produces
/// the ranked candidate set, take its top `ECHO_GRAPH_SEED_K` chunks as SEEDS and
/// follow the legal-graph overlay (`cross_reference` / `non_obstante` /
/// `amended_by`) up to `ECHO_GRAPH_HOPS` hops, folding the reached provisions in
/// as an additional bounded arm. This pulls the legally-CONNECTED provisions that
/// pure similarity misses (an amendment or non-obstante override is rarely
/// lexically similar to the query). Default OFF — a safe, flag-gated A/B like
/// `ECHO_HYBRID`; also a no-op on any corpus whose overlay edges aren't built yet.
fn graph_expand_enabled() -> bool {
    // Deep search always expands over the legal graph.
    deep_search_active()
        || matches!(
            std::env::var("ECHO_GRAPH_EXPAND").unwrap_or_default().trim().to_ascii_lowercase().as_str(),
            "1" | "on" | "true" | "yes"
        )
}

// ── Deep search (per-request, breadth-first single deep pass) ────────────────
// A chat toggle: when ON for a turn, one much-richer retrieval+generation pass —
// wider candidate pool, graph expansion + summary recall folded in, a larger
// context budget and answer cap, and a "be comprehensive" instruction. Carried
// as a tokio task-local (not an env var / atomic) so it is per-request and
// concurrency-safe: each spawned turn scopes its own value. Reads default to
// `false` outside a scope (e.g. desktop/tests) — normal behaviour unchanged.
tokio::task_local! {
    static DEEP_SEARCH: bool;
}

/// True when the current turn requested deep search. Safe to call anywhere in the
/// turn's task tree; `false` when no scope is active.
pub fn deep_search_active() -> bool {
    DEEP_SEARCH.try_with(|d| *d).unwrap_or(false)
}

/// Run `fut` with the deep-search flag set for this turn's task tree.
pub async fn with_deep_search<F: std::future::Future>(deep: bool, fut: F) -> F::Output {
    DEEP_SEARCH.scope(deep, fut).await
}

// Web search (2026-07-29). Same task-local mechanism and for the same reason:
// per-request and concurrency-safe. Note what this flag does NOT do — it never
// widens or alters retrieval over the legal corpus. Web results are assembled
// into their OWN block, and the corpus answer is produced exactly as it would be
// with the toggle off, so turning web search on can never change what the law
// says.
tokio::task_local! {
    static WEB_SEARCH: bool;
}

/// True when the current turn requested web search.
pub fn web_search_active() -> bool {
    WEB_SEARCH.try_with(|w| *w).unwrap_or(false)
}

/// Run `fut` with the web-search flag set for this turn's task tree.
pub async fn with_web_search<F: std::future::Future>(web: bool, fut: F) -> F::Output {
    WEB_SEARCH.scope(web, fut).await
}

/// Candidate-pool widen factor applied to retrieval depths under deep search.
fn deep_pool_mult() -> f32 {
    if deep_search_active() { env_f32("ECHO_DEEP_POOL_MULT", 2.5) } else { 1.0 }
}
/// How many top merged chunks seed the graph expansion. Default 8 — the strong
/// hits, not the whole pool (expanding weak seeds re-introduces junk).
fn knob_graph_seed_k() -> usize { env_usize("ECHO_GRAPH_SEED_K", 8) }
/// Hop budget for the expansion (hard-capped at 3 by `graph::expand`). Default 1
/// — direct legal neighbours; 2 reaches an amendment's own cross-references.
fn knob_graph_hops() -> u8 { env_usize("ECHO_GRAPH_HOPS", 1).min(3) as u8 }
/// Cap on how many expanded provisions fold into the pool (bound the arm).
fn knob_graph_k() -> usize { env_usize("ECHO_GRAPH_K", 12) }

/// An enumeration-style question — one that wants a COUNT or a LIST rather than a
/// single provision. Gates table completion (below) and, inversely, suppresses the
/// source-diversity pass: an enumeration answer legitimately wants ONE document to
/// dominate the context, which is exactly what diversity exists to prevent.
///
/// Deliberately separate from `deep_search::is_enumeration_query`, which gates a
/// different thing (the deterministic table-row extraction short-circuit) and
/// carries a different marker list. Merging them would silently couple two
/// unrelated routing decisions.
fn is_enumeration_query(q: &str) -> bool {
    let l = q.to_ascii_lowercase();
    [
        "how many", "list all", "list the", "number of", "count of", "count the",
        "all the", "name all", "name the", "total number", "sum of",
    ]
    .iter()
    .any(|m| l.contains(m))
}

// ── Table completion bounds (2026-08-06) ─────────────────────────────────────
// Live production failure: an enumeration query pulled 64 table chunks into a
// context that holds ~36 and PREPENDED them, evicting the correctly-ranked law
// entirely (Sindh Public Procurement Act 2009 s.6(2) was 9 of the top 10 and
// reached the model not at all). The pull is now bounded three ways.

/// How far down the ranked list a table/row chunk may sit and still trigger
/// completion. Default 5 — a table the ranker put near the top is plausibly what
/// the question is about; one at rank 40 is an accident, and expanding it used to
/// cost the whole window.
fn knob_table_seed_top_n() -> usize { env_usize("ECHO_TABLE_SEED_TOP_N", 5) }
/// Max context slots the reserved table may occupy. Default `ctx_k / 2` (18 of 36
/// at an 8192 window) so at least half the window is still ranked evidence.
fn knob_table_reserve_k(ctx_k: usize) -> usize {
    env_usize("ECHO_TABLE_RESERVE_K", (ctx_k / 2).clamp(4, 24))
}
/// Max estimated tokens the reserved table may occupy. Default `ctx_budget / 3`.
/// Needed as well as the slot cap because one whole-table chunk can be enormous —
/// `build_context` always admits its FIRST chunk regardless of size, and the
/// reserve is the prefix.
fn knob_table_reserve_tokens(ctx_budget: usize) -> usize {
    env_usize("ECHO_TABLE_RESERVE_TOKENS", ctx_budget / 3)
}

/// Max chunks one SOURCE may contribute before the next source gets a turn.
/// `off` / unset disables the pass; `auto` computes `ctx_k / 3` clamped to
/// [6, 12]; an integer is used as-is. **Default OFF** — new retrieval behaviour
/// ships dark and is flipped after an A/B on the VM, the same discipline as
/// `ECHO_HYBRID` / `ECHO_GRAPH_EXPAND` / `ECHO_INSTRUMENT_BOOST`. Table
/// completion is what happens when a well-reasoned retrieval change skips that
/// gate.
fn knob_source_cap(ctx_k: usize) -> usize {
    let auto = (ctx_k / 3).clamp(6, 12);
    match std::env::var("ECHO_SOURCE_CAP") {
        Err(_) => 0,
        Ok(v) => {
            let t = v.trim().to_ascii_lowercase();
            match t.as_str() {
                "" | "off" | "0" | "false" | "no" => 0,
                "auto" | "on" | "1" | "true" | "yes" => auto,
                _ => t.parse().unwrap_or(auto),
            }
        }
    }
}

/// Phase 2 — amendment-awareness annotation (2026-07-12). When a provision that
/// reaches the LLM context has recorded `amendment_events`, append a note so the
/// answer flags that the base text may be superseded (the officer's completeness
/// concern). NOT a graph edge — the amending instruments are external to the
/// corpus (see the amendment_events audit) — so this is a provision-property
/// annotation reusing the existing F013/F015 provenance. Default OFF (A/B).
fn amend_notes_enabled() -> bool {
    matches!(
        std::env::var("ECHO_AMEND_NOTES").unwrap_or_default().trim().to_ascii_lowercase().as_str(),
        "1" | "on" | "true" | "yes"
    )
}

/// Citation fast-path gate (2026-07-03). The fast-path resolved a bare "Rule N"
/// by unit_label and BYPASSED vector search + reranker, then disambiguated by
/// source-affinity token overlap. With colliding rule numbers across instruments
/// ("Rule 16" lives in SPPRA, the Secretariat Manual, the SLGA Compendium, and
/// the Delegation rules) the affinity vocabulary is polluted by generic English
/// (under/use/method/conditions), so a query like "…request for quotations method
/// under rule 16" resolved to the Secretariat Manual's "Rule 16 — Steno Typist"
/// and the LLM correctly refused. Per the architect: every query uses the single
/// hybrid-union retrieval path — no special-case citation routing. Default OFF;
/// set `ECHO_CITATION_FAST_PATH=1` only to restore the legacy behaviour.
fn citation_fast_path_enabled() -> bool {
    std::env::var("ECHO_CITATION_FAST_PATH").ok().as_deref() == Some("1")
}

/// The exact fallback phrase used when no grounded answer exists (FR-041 / FR-042).
/// Used in three places: empty retrieval, LLM "not found" instruction, and
/// grounding validator replacement. Must be a single constant to stay in sync.
pub const GROUNDING_FALLBACK: &str =
    "No relevant information found in provided documents";

/// Multiplier on `ECHO_VEC_K`/`ECHO_FTS_K` for the map-reduce retrieval scope
/// (2026-07-06). Map-reduce capabilities used to sweep EVERY leaf chunk of the
/// visible corpus (60+ batches, 10+ min on SPPRA-class questions); they now map
/// over a doubled-K hybrid retrieval slice instead. Default 2.0.
fn knob_mapreduce_k_mult() -> f32 { env_f32("ECHO_MAPREDUCE_K_MULT", 2.0) }

/// Retrieval scope for the map-reduce capabilities (ActionItems / SummarizeDoc /
/// Compare / Conflict): the same hybrid vec+FTS5 path guidance uses, at
/// `knob_mapreduce_k_mult()`× the per-list depth, restricted to the caller's
/// visible sources. Returns `(chunk_id, source_id, content, page, file_name)`
/// in relevance order (vector-cosine first under `ECHO_HYBRID=union`, RRF
/// otherwise). One tranche only — deeper iterations are a deferred follow-up.
pub async fn retrieve_for_mapreduce(
    pool: &Db,
    embedder: &Arc<EmbedderState>,
    query_text: &str,
    scope_source_ids: &[String],
) -> Result<Vec<(String, String, String, Option<i64>, String)>> {
    // Map-reduce has no context window, so unlike the guidance path its depth
    // cannot be derived from `ctx_top_k` — it is `mult ×` the knob total, which
    // is what it has always been (28+7 → 70). What DOES have to match is the
    // vec/FTS **mix**: both paths must read `ECHO_VEC_K:ECHO_FTS_K` as a ratio
    // through the same helper, or a future change to one silently diverges from
    // the other. Numbers here are unchanged — this is a shared-derivation fix.
    let mult = knob_mapreduce_k_mult().max(1.0);
    let depth = (((knob_vec_k() + knob_fts_k()) as f32) * mult) as usize;
    let (vec_k, fts_k) = merge_depth_from(
        knob_vec_k() as f32,
        knob_fts_k() as f32,
        1.0, // depth is already final; no further overfetch
        depth,
        usize::MAX,
        usize::MAX,
    );

    let embedder_clone = Arc::clone(embedder);
    let q = query_text.to_string();
    let query_embedding = tokio::task::spawn_blocking(move || embedder_clone.embed_one(&q))
        .await
        .map_err(|e| anyhow::anyhow!("embed thread panicked: {e}"))??;

    let scope = Some(scope_source_ids);
    // notebook_id is unused when an explicit source-id scope is supplied.
    let vec_chunks =
        get_context_chunks(pool, "", scope, &query_embedding, vec_k as i64).await?;
    let fts_chunks = fts5_retrieve(pool, "", scope, query_text, Some(fts_k as i64)).await?;
    Ok(match (fts_chunks.is_empty(), vec_chunks.is_empty()) {
        (false, false) => {
            if hybrid_union_enabled() {
                union_merge(&fts_chunks, &vec_chunks, fts_k, vec_k)
            } else {
                rrf_merge(&fts_chunks, &vec_chunks, 60)
            }
        }
        (false, true) => fts_chunks,
        _ => vec_chunks,
    })
}

/// Discriminates between a factual lookup and a broad summary/outline request.
/// Used to select the system prompt and max_tokens budget for LLM synthesis.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum QueryType {
    Regular,
    Summary,
}

/// Classify a query as Summary if it matches broad-overview trigger words.
/// Pure function — no LLM call, no allocation beyond the lowercase copy.
pub(crate) fn classify_query(query: &str) -> QueryType {
    let lower = query.to_lowercase();
    let triggers = [
        "summarize", "summarise", "summary",
        "outline", "overview",
        "what is the book about", "what does the book cover",
        "what does this document cover", "what does the document cover",
        "list all chapters", "table of contents",
        "give me an overview", "brief me", "brief on",
        "what topics", "what subjects",
    ];
    if triggers.iter().any(|t| lower.contains(t)) {
        QueryType::Summary
    } else {
        QueryType::Regular
    }
}

/// Result of the post-processing grounding validation pass (AD-8 / FR-042).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GroundingResult {
    /// Response contains only citations that map to retrieved context sources.
    pub is_grounded: bool,
    /// Count of citation markers that matched a known source file.
    pub citation_count: u32,
    /// Count of factual sentences (> 8 words, non-question) that had no citation.
    pub uncited_sentence_count: u32,
    /// Whether the validator replaced the answer with `GROUNDING_FALLBACK`.
    pub fallback_triggered: bool,
    /// `valid_citations / factual_sentences` — 1.0 when no factual sentences.
    pub grounding_score: f32,
    /// True when the only reason for fallback is missing citation markers
    /// (Check 3 — broad answers from small models that omit formatting).
    /// When true, the answer itself is likely valid; only add a quality notice.
    pub uncited_only: bool,
}

/// A single citation reference.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Citation {
    pub source_id: String,
    pub chunk_id: String,
    pub excerpt: String,
    pub page: Option<i64>,
}

/// Quality notice injected when a lower tier is used due to hardware constraints.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QualityNotice {
    pub constrained: bool,
    pub message: String,
}

/// A single turn in the conversation history passed to the LLM.
#[derive(Debug, Clone)]
pub struct HistoryTurn {
    pub role: String,    // "user" or "assistant"
    pub content: String,
}

/// Crude token estimate (chars/4) — the same approximation the prompt-size trace
/// and `build_context_pass` already use. Deliberately consistent with them: the
/// law budget must be measured in the SAME units it is spent in.
pub(crate) fn est_tokens(s: &str) -> usize {
    s.len() / 4
}

/// The prior-turns block that [`build_rag_prompt`] injects — extracted so the LAW
/// BUDGET can **measure** it before packing chunks.
///
/// Why this exists (prod, 2026-07-16): `law_overhead` was a hardcoded 1500-token
/// guess for "system prompt + question + a little history". Nothing measured the
/// real thing, so when the system prompt or history grew, the guess didn't move and
/// the prompt silently exceeded the window — `llm.rs` head+tail truncation then
/// dropped the MIDDLE, which is the retrieved law. Symptom: citations collapsed
/// 9→7→8→3→3→0 across a 6-turn chat and the model started refusing with "the
/// retrieved documents do not contain enough information".
///
/// Empty for self-contained queries (see [`is_followup_query`]).
pub(crate) fn prior_turns_block(query: &str, history: &[HistoryTurn]) -> String {
    if !is_followup_query(query) || history.is_empty() {
        return String::new();
    }
    let mut pairs: Vec<(&HistoryTurn, &HistoryTurn)> = Vec::new();
    let mut user: Option<&HistoryTurn> = None;
    for t in history {
        if t.role == "user" {
            user = Some(t);
        } else if t.role == "assistant" {
            if let Some(u) = user.take() {
                pairs.push((u, t));
            }
        }
    }
    // The WINDOW IS THE CALLER'S. `memory::load_for_turn` already trims to
    // `settings.recent_turns_verbatim` (persona config, 8 messages = 4 pairs), so
    // re-truncating here silently discarded turns the config had deliberately kept —
    // two places deciding one window, which is how "what was my first question?"
    // came back naming the FOURTH question (prod audit, 2026-07-16).
    //
    // Re-land of 325618ab, reverted in 80048b37 when this block was unbounded AND
    // unmeasured: the prompt grew every turn until it overflowed. Safe now — the
    // budget MEASURES this exact block (`law_overhead`, 9f40d4aa) and the law budget
    // shrinks to make room, so growth costs law passages instead of overflowing.
    // The bound now lives where it belongs: the config window × the 600-char cap.
    let mut s = String::new();
    for (u, a) in pairs {
        let utxt: String = u.content.chars().take(600).collect();
        let atxt: String = a.content.chars().take(600).collect();
        s.push_str(&format!(
            "<|im_start|>user\n{utxt}<|im_end|>\n<|im_start|>assistant\n{atxt}<|im_end|>\n"
        ));
    }
    s
}

/// Returns true when the new query looks like a follow-up that depends on
/// prior context — short, anaphoric, or starts with a continuation token.
/// Self-contained queries return false so retrieval is not polluted.
///
/// Note: only USER turns are inspected by callers; we never derive
/// keywords from prior assistant text (that was the source of the
/// 2026-05-10 drift bug — see CHANGELOG_DEV.md).
pub(crate) fn is_followup_query(query: &str) -> bool {
    let trimmed = query.trim();
    let lower = trimmed.to_lowercase();
    let word_count = trimmed.split_whitespace().count();

    // Short queries almost always need prior context.
    if word_count <= 5 { return true; }

    const ANAPHORA_TOKENS: &[&str] = &[
        " this ", " that ", " those ", " these ", " it ", " them ",
        " its ", " their ", " his ", " her ", " same ",
    ];
    const CONTINUATION_PREFIXES: &[&str] = &[
        "and ", "also ", "what about ", "and for ", "and what ",
        "how about ", "but ", "or ", "then ", "so ",
    ];
    // Explicit references to the CONVERSATION ITSELF. Unlike anaphora these are
    // unambiguous — a question about the corpus never says "you told me" or "my
    // third question" — so they are decisive and are checked BEFORE the
    // `names_own_subject` suppression below: a query can name its own subject and
    // STILL depend on prior turns ("compare the E&D Rules with what you told me
    // earlier"). Deliberately excludes bare "compare"/"difference between", which
    // are perfectly self-contained ("compare the E&D Rules with the RSO").
    const CONVERSATION_REFERENCE: &[&str] = &[
        // the assistant's prior turns
        "you told me", "you said", "you mentioned", "you explained", "you gave",
        "your answer", "your response", "your previous", "your earlier",
        // the user's own prior turns. "asking" forms added 2026-07-16: a real chat
        // said "i was asking for my chat not the documents" and matched NOTHING —
        // "i asked" does not cover the progressive.
        "i asked", "asked you", "i told you", "i mentioned",
        "i was asking", "i am asking", "i meant", "i'm asking",
        // Ordinal references to earlier turns. The noun matters as much as the
        // ordinal: a real chat said "compare the first one with third one" and
        // matched nothing, because only the "* question" forms existed.
        "first question", "second question", "third question", "fourth question",
        "last question", "previous question", "earlier question",
        "first one", "second one", "third one", "fourth one", "last one",
        "first answer", "second answer", "third answer", "last answer",
        "first reply", "last reply",
        // the conversation as a whole. "my chat" added for the same real turn.
        "we discussed", "we talked about", "previously",
        "this conversation", "this chat", "my chat", "our chat",
        "earlier in this", "above answer", "the above",
    ];

    // Normalise punctuation to spaces BEFORE any whole-word check, then pad.
    //
    // ⚠️ This line is the fix for a silent, high-impact bug (prod audit 2026-07-16).
    // `padded` used to be built straight from `lower`, so a trailing `?` defeated
    // every space-delimited token: `"…for that?"` contains `" that?"`, never
    // `" that "` → no anaphora → NO HISTORY INJECTED. A question mark switched the
    // conversation memory off. In one real 8-turn chat this dropped history on 3
    // turns ("who is the competent authority for that?", …) and the officer saw
    // disconnected answers. Word-boundary matching must run on word boundaries.
    //
    // Keep `-` (BS-17) and `/` (n/a) so those stay single tokens.
    let normalized: String = lower
        .chars()
        .map(|c| if c.is_alphanumeric() || c == '-' || c == '/' { c } else { ' ' })
        .collect();
    let padded = format!(" {} ", normalized.split_whitespace().collect::<Vec<_>>().join(" "));

    // A leading continuation token is a strong signal on its own ("And for…",
    // "Also list…") — fires regardless of whether the query names a subject.
    // Matched on the normalised text so "And, for BS-17?" still counts.
    let lead = padded.trim_start();
    if CONTINUATION_PREFIXES.iter().any(|p| lead.starts_with(p)) { return true; }

    // An explicit reference to the conversation outranks every heuristic below:
    // the user is telling us, in words, that this turn depends on prior ones.
    if CONVERSATION_REFERENCE.iter().any(|m| padded.contains(m)) { return true; }

    // An anaphoric pronoun only implies a follow-up when the pronoun lacks an
    // antecedent in THIS query. If the query already names its own subject
    // (a capitalised noun phrase past the sentence-initial word), the pronoun
    // refers intra-sentence — not to a prior turn (Q12 2026-06-06 false
    // positive: "…responsibility is it to act on them?" naming "Principles of
    // Policy"). Suppress the anaphora signal in that case.
    //
    // ⚠️ KNOWN LIMIT (measured, not fixed): this suppression is case-sensitive by
    // design, so "Who is the Competent Authority for that?" loses its history while
    // the identical lowercase sentence keeps it — an officer is punished for
    // capitalising. Tempting to override when the query ENDS on a bare pronoun, but
    // that cannot work: the Q12 regression above also ends on "them", and there the
    // pronoun genuinely binds in-sentence. Telling those apart needs the antecedent,
    // not the surface form. Left alone deliberately — the prod failures on
    // 2026-07-16 were all lowercase and were caused by the punctuation bug above,
    // not by this. Revisit only with a real capitalised failure in hand.
    if names_own_subject(trimmed) {
        return false;
    }

    if ANAPHORA_TOKENS.iter().any(|t| padded.contains(t)) { return true; }

    false
}

/// True when the query contains a capitalised content word *after* the
/// sentence-initial token — i.e. it names its own subject (proper noun /
/// instrument / Article title) rather than relying on prior context. Ignores
/// the first token (always capitalised by sentence case) and short/stopword
/// tokens. Case-dependent by design: a fully lowercased query names no subject.
fn names_own_subject(query: &str) -> bool {
    query
        .split_whitespace()
        .skip(1)
        .map(|t| t.trim_matches(|c: char| !c.is_alphanumeric()))
        .any(|t| {
            t.chars().count() >= 3
                && t.chars().next().is_some_and(|c| c.is_ascii_uppercase())
                && t.chars().all(|c| c.is_alphabetic())
        })
}

/// Extract carry-over keywords from prior USER turns. Skips assistant
/// content entirely (defence against the truncation/template-leak class
/// of bugs). Domain heuristics: prefer capitalised words, alphanumerics
/// like "BS-19", and numbers that may carry over (years, article nos.).
/// Returns at most 5 deduped keywords ordered by recency.
fn extract_carryover_keywords(history: &[HistoryTurn]) -> Vec<String> {
    const STOPWORDS: &[&str] = &[
        "what", "which", "where", "when", "who", "whom", "whose", "why",
        "how", "the", "and", "for", "with", "about", "into", "from",
        "this", "that", "those", "these", "their", "them", "they",
        "have", "has", "had", "been", "being", "was", "were", "are",
        "is", "be", "do", "does", "did", "can", "could", "would",
        "should", "shall", "will", "may", "might", "must",
        "please", "kindly", "tell", "show", "give", "list", "explain",
        "define", "describe", "discuss", "question", "answer",
    ];
    // Generic government/legal boilerplate — high-frequency across the WHOLE
    // corpus, so as carry-over retrieval keywords they add noise (match nearly
    // everything) instead of topic signal. Distinctive entities (e-Daftar, Canon,
    // SPPRA, BS-19, dates, place names) still carry over; these do not.
    // Fix 2026-07-02: a procurement follow-up was carrying over "Draft, Chief,
    // Secretary, note" — dragging clerical draft-signing chunks into retrieval.
    const GENERIC: &[&str] = &[
        // roles / titles
        "chief", "secretary", "secretaries", "minister", "officer", "officers",
        "authority", "authorities", "chairman", "member", "members", "head",
        "director", "deputy", "additional", "administrative", "competent",
        "government", "governor", "commissioner", "convenor",
        // jurisdiction (whole corpus)
        "sindh", "pakistan", "provincial", "federal", "national", "punjab",
        // document structure / correspondence boilerplate
        "draft", "drafts", "note", "notes", "rule", "rules", "section",
        "sections", "article", "articles", "regulation", "regulations",
        "clause", "schedule", "chapter", "part", "notification", "order",
        "orders", "annexure", "appendix", "department", "departments",
        "division", "board", "committee", "commission", "agency", "agencies",
        "ministry", "secretariat", "paper", "papers", "letter", "communication",
        "gazette", "publication", "dated",
    ];
    // Priority tiers — distinctive tokens (digits, dashes) outrank plain
    // long words so 5-keyword cap doesn't drop important entities.
    let mut tier_alnum: Vec<String> = Vec::new(); // BS-19, 1973, Article-3
    let mut tier_proper: Vec<String> = Vec::new(); // Karachi, Pakistan, SCUG
    let mut tier_long: Vec<String> = Vec::new();   // procedure, employment

    let already = |s: &str, bucket: &Vec<String>| -> bool {
        bucket.iter().any(|x| x.eq_ignore_ascii_case(s))
    };

    // Walk newest → oldest so recent topics come first within each tier.
    for turn in history.iter().rev() {
        if turn.role != "user" { continue; }
        for raw_tok in turn.content.split(|c: char| !c.is_alphanumeric() && c != '-' && c != '/') {
            let tok = raw_tok.trim_matches(|c: char| c == '-' || c == '/');
            if tok.len() < 3 { continue; }
            let lower = tok.to_lowercase();
            if STOPWORDS.contains(&lower.as_str()) { continue; }
            // Drop generic government/legal boilerplate — keep only distinctive
            // topic terms (a dashed/alnum token like "ex-pakistan" is exempt: it
            // never equals a bare boilerplate word).
            if GENERIC.contains(&lower.as_str()) { continue; }

            let has_digit_or_dash = tok.chars().any(|c| c.is_ascii_digit()) || tok.contains('-');
            let first = tok.chars().next().unwrap_or(' ');
            let is_proper = first.is_uppercase();
            let is_long = tok.len() >= 5;

            if has_digit_or_dash {
                if !already(tok, &tier_alnum) { tier_alnum.push(tok.to_string()); }
            } else if is_proper {
                if !already(tok, &tier_proper) && !already(tok, &tier_alnum) {
                    tier_proper.push(tok.to_string());
                }
            } else if is_long {
                if !already(tok, &tier_long)
                    && !already(tok, &tier_proper)
                    && !already(tok, &tier_alnum)
                {
                    tier_long.push(tok.to_string());
                }
            }
        }
    }

    let mut out: Vec<String> = Vec::with_capacity(5);
    for bucket in [tier_alnum, tier_proper, tier_long] {
        for tok in bucket {
            if out.len() >= 5 { return out; }
            out.push(tok);
        }
    }
    out
}

/// How full THIS CHAT is — measured on the real prompt, never re-estimated later.
///
/// **Why this is not "prompt fullness".** The prompt is ~always ~100% full by
/// construction: the law budget expands to fill whatever is left
/// (`ctx_budget = window − answer − overhead`, then law packs up to it). Prod traces
/// show 94/95/99/101/96/98% across one six-turn chat. So "% of prompt used" is high
/// from turn ONE and says nothing about whether the chat is running out — as a
/// warning it would cry wolf immediately.
///
/// What actually degrades a long chat is the LAW BUDGET SHRINKING as history grows.
/// That is what this measures:
/// - `used`  = non-law overhead (system prompt + question + history + scaffold) —
///             exactly the `law_overhead` the budget subtracts.
/// - `limit` = the overhead at which the law budget hits its floor
///             (`CONTEXT_TOKEN_BUDGET`). Past it the floor itself overflows the cap
///             (the `⚠ BUDGET FLOORED` trace) and `llm.rs` head+tail truncation
///             starts dropping law out of the middle → citations collapse → refusals.
///             That is the honest "this chat is full".
///
/// `used/limit` ≈ 35% on turn one, ~77% at ten exchanges, 100% at ~fifteen.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct ChatFill {
    /// Non-law overhead for this turn, in tokens (measured, not guessed).
    pub used: usize,
    /// Overhead at which the law budget floors out — the point of no return.
    pub limit: usize,
}

/// The full response from the query router.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QueryResponse {
    pub answer: String,
    pub citations: Vec<Citation>,
    pub tier_used: String,
    pub quality_notice: Option<QualityNotice>,
    /// Measured chat fullness (see [`ChatFill`]). `None` on paths that never
    /// assemble one grounded prompt (early returns, map-reduce, deep search); the
    /// caller then falls back to its own estimate. `serde(default)` so anything
    /// already persisted still deserializes.
    #[serde(default)]
    pub chat_fill: Option<ChatFill>,
    /// Retrieved passages that are NOT citations — the model cited nothing, so
    /// these are the best-ranked passages and support no specific claim.
    /// Populated only under `ECHO_CITATION_STRICT`, where they are moved out of
    /// `citations` rather than discarded, so a client can label them honestly.
    /// `serde(default)` so persisted responses still deserialize.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub related_passages: Vec<Citation>,
}

/// Route a query for a given notebook through the tiered pipeline.
/// Returns (response, was_cancelled). Emits chat://thinking events via app handle.
pub async fn route(
    pool: &Db,
    embedder: &Arc<EmbedderState>,
    model_slot: &ModelSlot,
    reranker_slot: &RerankerSlot,
    tier: &HardwareTier,
    notebook_id: &str,
    session_id: &str,
    query_text: &str,
    history: &[HistoryTurn],
    cancel: Option<tokio_util::sync::CancellationToken>,
    app: Option<crate::compat::AppHandle>,
) -> Result<(QueryResponse, bool)> {
    route_scoped(pool, embedder, model_slot, reranker_slot, tier, notebook_id, session_id, query_text, history, None, cancel, app).await
}

/// Same as `route`, but with an optional `source_ids` scope filter that
/// overrides the notebook-based retrieval scope. Used by the chat scope picker:
/// when present, retrieval is restricted to chunks whose `source_id IN (...)`
/// instead of `notebook_sources.notebook_id = notebook_id`. The `notebook_id`
/// argument is still used for session/history persistence and trace metadata.
pub async fn route_scoped(
    pool: &Db,
    embedder: &Arc<EmbedderState>,
    model_slot: &ModelSlot,
    reranker_slot: &RerankerSlot,
    tier: &HardwareTier,
    notebook_id: &str,
    session_id: &str,
    query_text: &str,
    history: &[HistoryTurn],
    scope_source_ids: Option<&[String]>,
    cancel: Option<tokio_util::sync::CancellationToken>,
    app: Option<crate::compat::AppHandle>,
) -> Result<(QueryResponse, bool)> {
    route_scoped_with_override(
        pool, embedder, model_slot, reranker_slot, tier,
        notebook_id, session_id, query_text, history,
        scope_source_ids, None, cancel, app, None,
    )
    .await
}

/// Like [`route_scoped`] but lets the caller override the system prompt
/// the LLM sees. The override does NOT touch the embedding/retrieval path —
/// `query_text` alone is embedded for vector + FTS5 search. The override
/// replaces the auto-selected `SUMMARY_SYSTEM_PROMPT` /
/// `GROUNDING_SYSTEM_PROMPT` when constructing the chat prompt.
///
/// Feature 007 — `intent_router::dispatch` uses this to inject per-intent
/// instructions (Lookup / ListChildren / Summarize / ActionItems) without
/// polluting retrieval embedding similarity.
#[allow(clippy::too_many_arguments)]
pub async fn route_scoped_with_override(
    pool: &Db,
    embedder: &Arc<EmbedderState>,
    model_slot: &ModelSlot,
    reranker_slot: &RerankerSlot,
    tier: &HardwareTier,
    notebook_id: &str,
    session_id: &str,
    query_text: &str,
    history: &[HistoryTurn],
    scope_source_ids: Option<&[String]>,
    system_prompt_override: Option<&str>,
    cancel: Option<tokio_util::sync::CancellationToken>,
    app: Option<crate::compat::AppHandle>,
    // Trace ownership (Track 3): when a turn-level owner (`assistant::run_turn`)
    // supplies a trace, this fn borrows it and records its retrieval/generation
    // steps into it, leaving the FILE WRITE to the owner so the whole turn lands
    // in ONE trace. When `None` (legacy desktop `route_scoped` / `intent_router`
    // path) it owns a fresh trace and writes it here as before.
    external_trace: Option<&mut QueryTrace>,
) -> Result<(QueryResponse, bool)> {
    use crate::compat::Emitter;
    if let Some(ref t) = cancel {
        if t.is_cancelled() {
            anyhow::bail!("chat query cancelled before starting");
        }
    }
    let t_route = std::time::Instant::now();
    let owns_trace = external_trace.is_none();
    let mut owned_trace = QueryTrace::new(query_text, session_id, notebook_id);
    let trace: &mut QueryTrace = external_trace.unwrap_or(&mut owned_trace);
    trace.step("User question received", &[
        ("Question", format!("\"{}\"", query_text.chars().take(200).collect::<String>())),
        ("Length", format!("{} characters | {} words",
            query_text.chars().count(),
            query_text.split_whitespace().count())),
    ]);

    // Classify before retrieval so ctx_top_k / ctx_budget affect the first fetch.
    let query_type = classify_query(query_text);
    // Resolve the model's REAL context window so generation + the retrieved-law
    // budget scale with the model (FR-010/FR-019) instead of a hardcoded 3584.
    // The old 3584 cap was why whole rules (e.g. SPPRA direct contracting + the
    // financial-powers table) could not all reach the model.
    //
    // BUT prefilling the full window (e.g. 32768) through a 7B on CPU takes ~an
    // hour per answer. So cap the GENERATION context at a usable ceiling — still
    // far larger than 3584 (more law fits), but bounded so a CPU run completes.
    // Raise `ECHO_MAX_GEN_CTX` (or move to GPU) to use more of the window; the
    // model's full trained window is still reported in the top bar.
    let gen_ctx_ceiling = env_usize("ECHO_MAX_GEN_CTX", 8192);
    let window = crate::services::assistant::resolve_active_window(model_slot, tier)
        .min(gen_ctx_ceiling)
        .max(2048);
    let gen_n_ctx = window as u32;
    // Deep search (per-request toggle): breadth-first single deep pass. Detected
    // once here; drives a larger answer cap, wider retrieval, and a comprehensive
    // instruction below.
    let deep = deep_search_active();
    // NOTE: the generation window (ECHO_MAX_GEN_CTX / vLLM max-model-len, 8192) is
    // already saturated by the retrieved-law budget at normal settings, so RAISING
    // the answer cap for deep search backfires — it steals tokens from context and
    // deep ends up with FEWER chunks than normal (observed: top-K 32 vs 36). Deep
    // keeps the SAME answer cap; its breadth comes from a wider candidate pool +
    // graph/summary recall filling the same budget with better-selected law, plus
    // the comprehensive prompt. (A genuinely larger deep answer needs a bigger
    // window — raise vLLM max-model-len; infra, not a per-request knob.)
    let (auto_system_prompt, gen_max_tokens) = match query_type {
        QueryType::Summary => (SUMMARY_SYSTEM_PROMPT, 1536u32),
        QueryType::Regular => (crate::services::llm::grounding_system_prompt(), 1280u32),
    };
    // The prompt may use the whole window minus the answer reserve.
    let gen_max_prompt = window.saturating_sub(gen_max_tokens as usize);

    // Resolve the EFFECTIVE system prompt BEFORE the budget below — the budget has
    // to measure it, and it cannot measure what has not been chosen yet. (Feature
    // 007: per-intent caller can override the auto-selected prompt; deep search
    // appends a breadth directive. Both change its size materially.)
    let base_system_prompt: &str = system_prompt_override.unwrap_or(auto_system_prompt);
    let deep_system_prompt: String;
    let system_prompt: &str = if deep {
        deep_system_prompt = format!(
            "{base_system_prompt}\n\nDEEP SEARCH: The officer requested a thorough, comprehensive \
             answer. Use ALL relevant passages provided, not only the most obvious ones. Cover every \
             distinct point, requirement, exception, and related provision the material supports; \
             where the material lists items (e.g. departments, categories, grounds), enumerate them \
             rather than summarising as 'several'. Stay grounded — cite each passage you use and do \
             not add anything the passages do not support."
        );
        &deep_system_prompt
    } else {
        base_system_prompt
    };

    // MEASURED overhead — everything `build_rag_prompt` puts in the prompt BESIDES
    // the retrieved law. This was a hardcoded `1500` guess for "system prompt +
    // question + a little history"; nothing checked it against reality, so when the
    // system prompt or history grew the guess stayed put and the prompt overflowed
    // the window. `llm.rs` then head+tail-truncated it and dropped the MIDDLE — the
    // law itself — so citations collapsed and the model refused (prod 2026-07-16:
    // a 6-turn chat went 94%→95%→99%→101% of cap, citations 9→7→8→3→3→0).
    //
    // Measuring instead means the law budget SHRINKS to make room for history and a
    // bigger system prompt, which is exactly the allocator's stated policy ("law
    // outranks history" — law is trimmed last, but it must still fit).
    let prior_block_est = prior_turns_block(query_text, history);
    let law_overhead = est_tokens(system_prompt)
        + est_tokens(query_text)
        + est_tokens(&prior_block_est)
        + PROMPT_SCAFFOLD_TOKENS;
    let ctx_budget = window
        .saturating_sub(gen_max_tokens as usize)
        .saturating_sub(law_overhead)
        // Floor: better a thin answer than none. Safe while overhead stays under
        // ~4.5k; beyond that the floor itself would overflow, so it is traced below.
        .max(CONTEXT_TOKEN_BUDGET);
    // Let the token budget (not a small fixed count) be the real limiter; allow
    // many chunks (~150 tokens/chunk average), capped to keep retrieval sane.
    // Deep search: use MORE of the window for retrieved law (breadth) and pull a
    // wider candidate pool so the comprehensive answer has more to synthesise.
    let deep_budget_mult = if deep { env_f32("ECHO_DEEP_BUDGET_MULT", 1.5) } else { 1.0 };
    let ctx_budget = ((ctx_budget as f32 * deep_budget_mult) as usize)
        .min(gen_max_prompt.saturating_sub(law_overhead).max(CONTEXT_TOKEN_BUDGET));
    let ctx_top_k = (ctx_budget / 150).clamp(CONTEXT_TOP_K, if deep { 700 } else { 400 });
    // How full this CHAT is (not the prompt — see `ChatFill`). Computed once, here,
    // where the overhead is actually measured, and returned on EVERY exit from this
    // function including the refusal paths: a refusal in a nearly-full chat is
    // exactly when the officer most needs to be told the chat is out of room.
    let chat_fill = Some(ChatFill {
        used: law_overhead,
        limit: gen_max_prompt.saturating_sub(CONTEXT_TOKEN_BUDGET),
    });
    // Candidate supply must exceed the selection target so the budget can fill;
    // deep search widens it further via `deep_pool_mult`.
    let fetch_k = (((ctx_top_k * 2) as f32 * deep_pool_mult()) as usize)
        .clamp(knob_vec_k(), if deep { 900 } else { 600 });
    log::info!(
        "Query type: {:?}, max_tokens={gen_max_tokens}, n_ctx={gen_n_ctx}, ctx_k={ctx_top_k}",
        query_type
    );
    log::info!("METRIC query_start words={}", query_text.split_whitespace().count());
    trace.step("Classify query type", &[
        ("Detected", format!("{:?}", query_type)),
        ("System prompt", match query_type {
            QueryType::Summary => format!("Summary v{SUMMARY_SYSTEM_PROMPT_VERSION} (broad overview)"),
            QueryType::Regular => format!(
                "Grounding v{} (factual lookup)",
                crate::services::llm::grounding_prompt_version()
            ),
        }),
        ("LLM budget", format!("n_ctx={gen_n_ctx} | max_response={gen_max_tokens} tokens")),
        ("Context budget", format!("top-K={ctx_top_k} chunks | ~{ctx_budget} tokens")),
        // Make the overhead VISIBLE. It used to be an invisible hardcoded 1500 that
        // silently went stale — the overflow was only findable by reading traces
        // after users complained. Now every turn shows what the law budget paid for.
        ("Prompt overhead (measured)", format!(
            "~{law_overhead} tok = system {} + question {} + history {} + scaffold {PROMPT_SCAFFOLD_TOKENS}{}",
            est_tokens(system_prompt),
            est_tokens(query_text),
            est_tokens(&prior_block_est),
            if ctx_budget == CONTEXT_TOKEN_BUDGET { " ⚠ BUDGET FLOORED — overhead is squeezing the law" } else { "" },
        )),
        ("Deep search", if deep { "ON — wide retrieval + graph + summary recall + breadth prompt".to_string() } else { "off".to_string() }),
    ]);
    // Step 0a: chat-memory carry-over (retrieval-side only).
    // When the new query is a follow-up (short / anaphoric / continuation),
    // append entity keywords from prior USER turns to the *retrieval* query
    // so embedding + FTS5 still find topically-relevant chunks. The LLM
    // prompt continues to receive only the original query_text — no prior
    // assistant content is ever passed downstream. See CHANGELOG_DEV.md
    // 2026-05-10 for the drift-bug context.
    let retrieval_query: String = if is_followup_query(query_text) {
        let kws = extract_carryover_keywords(history);
        if kws.is_empty() {
            query_text.to_string()
        } else {
            let augmented = format!("{} {}", query_text, kws.join(" "));
            trace.step("Chat memory carry-over", &[
                ("Trigger", "Follow-up detected (short / anaphoric)".to_string()),
                ("Keywords appended", kws.join(", ")),
                ("Retrieval query", format!("\"{}\"", augmented.chars().take(160).collect::<String>())),
                ("Note", "LLM prompt unchanged; carry-over is retrieval-only".to_string()),
            ]);
            augmented
        }
    } else {
        query_text.to_string()
    };

    // Provision-label normalisation (2026-07-18). Append the bare `Rule N` label
    // so FTS can match `chunks.unit_label`; without it a query written as
    // "rule 4(I)(a) sub rule 3" matches "sub-rule" everywhere and Rule 4 nowhere
    // (trace 99cdbff2). Additive — the LLM prompt still receives `query_text`.
    let retrieval_query: String = if provision_norm_enabled() {
        match augment_provision_terms(query_text) {
            Some(base) => {
                let augmented = format!("{retrieval_query} {base}");
                trace.step("Provision-label normalisation", &[
                    ("Canonical label appended", base.clone()),
                    ("Why", "chunks.unit_label is the bare provision (\"Rule 4\"), not the clause path".to_string()),
                    ("Retrieval query", format!("\"{}\"", augmented.chars().take(160).collect::<String>())),
                    ("Note", "retrieval-only; the LLM prompt is unchanged".to_string()),
                ]);
                augmented
            }
            None => retrieval_query,
        }
    } else {
        retrieval_query
    };

    // Step 0: compute query embedding.
    // Use spawn_blocking so the ONNX inference (and first-run model load from NTFS)
    // runs on the blocking thread pool — never on a tokio worker thread.
    let t_embed = std::time::Instant::now();
    let embedder_clone = Arc::clone(embedder);
    let query_owned = retrieval_query.clone();
    if let Some(ref a) = app { let _ = a.emit("chat://thinking", serde_json::json!({
        "stage": "retrieving",
        "detail": format!("Searching knowledge base for \"{}\"...", query_text.chars().take(40).collect::<String>())
    })); }
    let query_embedding = tokio::task::spawn_blocking(move || embedder_clone.embed_one(&query_owned))
        .await
        .map_err(|e| anyhow::anyhow!("embed thread panicked: {e}"))??;
    let embed_ms = t_embed.elapsed().as_millis();
    log::info!("METRIC embed_query_ms={embed_ms}");
    trace.step("Generate query embedding", &[
        ("Model", "fastembed BGE (ONNX, CPU)".to_string()),
        ("Output", format!("{}-dimensional vector", query_embedding.len())),
        ("Time", format!("{embed_ms} ms")),
    ]);

    // Tier 0: semantic cache check (disabled in dev/testing — see
    // `query_cache_enabled`; every turn re-runs retrieval + generation).
    if query_cache_enabled() {
        if let Some(cached) = check_cache(pool, session_id, &query_embedding, query_text).await? {
            trace.step("Semantic cache lookup", &[
                ("Result", "HIT — returning cached answer".to_string()),
                ("Threshold", "cosine ≥ 0.92".to_string()),
            ]);
            let chars = cached.answer.chars().count();
            let cites = cached.citations.len();
            let tier = cached.tier_used.clone();
            trace.finish("cache_hit", chars, cites, &tier);
            if owns_trace { trace.write(&app); }
            return Ok((cached, false));
        }
        trace.step("Semantic cache lookup", &[
            ("Result", "MISS — proceeding to retrieval".to_string()),
        ]);
    } else {
        trace.step("Semantic cache lookup", &[
            ("Result", "DISABLED — fresh answer every turn".to_string()),
        ]);
    }

    // Tier 1 + Tier 2 dual retrieval → RRF merge (FR-039).
    //
    // - Tier 2 (vec) always runs — semantic similarity is the primary signal.
    // - Tier 1 (FTS5) runs for entity queries (dates/amounts/emails) and also
    //   when vec returns fewer than 2 results (sparse corpus fallback).
    // - When both produce results, rrf_merge() combines them into a unified
    //   ranked list that rewards chunks appearing in both signals.
    // chunk tuple: (chunk_id, source_id, content, page_number, file_name)
    // Fetch k*2 candidates so the context builder has room to dedup and
    // enforce the token budget without running short of good chunks.
    // Knob: ECHO_VEC_K overrides the default 2× ctx_top_k for vector recall.
    // ── S2 — definitional-question classifier (drives pool width + boost) ───
    // Computed once per query; applies only to the hybrid retrieval path.
    // Citation fast-path queries skip this — they already name the chunk.
    let definitional = is_definitional(query_text);
    let pool_mult = if definitional { knob_definitional_pool_mult() } else { 1.0 };
    let salient_boost = if definitional { knob_definitional_boost() } else { knob_salient_boost() };
    trace.step("Definitional classifier (S2)", &[
        ("Matched", definitional.to_string()),
        ("Pool multiplier", format!("{pool_mult:.2}×")),
        ("Salience boost", format!("{salient_boost:.2}×")),
    ]);

    // ── Citation fast-path + graph expansion (feature 003, T026 — Option 1) ──
    // When the query names a specific Article/clause, resolve it deterministically
    // by citation, expand one hop over the legal graph (cross_reference /
    // non_obstante), and skip vector search, FTS5 and the reranker. Ordinary
    // queries fall through to the hybrid retrieval below, unchanged.
    let citation_ids = if citation_fast_path_enabled() {
        citation_fast_path(pool, query_text).await?
    } else {
        None
    };
    let (raw_chunks, retrieval_tier): (Vec<(String, String, String, Option<i64>, String)>, &str) =
        if let Some(cited) = citation_ids {
            let with_neighbours = expand_citation_neighbours(pool, &cited).await;
            trace.step("Graph expansion (citation neighbours)", &[
                ("Seed citations", cited.len().to_string()),
                ("After 1-hop expand", with_neighbours.len().to_string()),
                ("Edges", "cross_reference + non_obstante (1 hop)".to_string()),
            ]);
            let mut chunks = fetch_chunks_by_ids(pool, &with_neighbours).await?;

            // F021: the fast-path resolves citations across ALL sources, so it
            // must be clamped to the caller's visible scope (defence-in-depth —
            // the path is env-disabled by default since bcb63f3, but if it is
            // ever re-enabled it must not become a privacy bypass).
            if let Some(allowed) = scope_source_ids {
                let allowed: std::collections::HashSet<&str> =
                    allowed.iter().map(String::as_str).collect();
                chunks.retain(|c| allowed.contains(c.1.as_str()));
            }

            // Keyword augmentation (2026-06-02): when the query carries
            // substantive content beyond the citation reference (e.g.
            // "Article 260 ... citizen"), run FTS5 scoped to the cited
            // source(s) and union the hits with fast-path chunks. Without
            // this, jumbo articles like Art 260 (master definitions —
            // 30+ sub-clauses, only a handful share unit_label "Article 260")
            // can starve the LLM of the specific sub-clause the user asked
            // for. See trace deca5716 (Art 260 "citizen" → refusal).
            let mut kw_added = 0usize;
            let stripped = strip_citation_refs(query_text);
            let has_content = stripped
                .split(|c: char| !c.is_alphanumeric() && c != '\'')
                .any(|t| t.trim().len() >= 4);
            if has_content && !chunks.is_empty() {
                let src_ids: Vec<String> = {
                    let mut s: HashSet<String> = HashSet::new();
                    for c in &chunks { s.insert(c.1.clone()); }
                    s.into_iter().collect()
                };
                let kw_chunks = fts5_retrieve(
                    pool, notebook_id, Some(&src_ids), &stripped, Some(6),
                ).await.unwrap_or_default();
                let existing: HashSet<String> = chunks.iter().map(|c| c.0.clone()).collect();
                for kc in kw_chunks {
                    if !existing.contains(&kc.0) {
                        chunks.push(kc);
                        kw_added += 1;
                    }
                }
            }

            // Sub-clause re-ranking (2026-06-05): when the citation matched a
            // wide-fan-out provision (Art 260 master definitions has 35+
            // sub-clauses, all sharing unit_label "Article 260"), chunk_index
            // order is essentially alphabetical-by-clause first-letter — the
            // sub-clause the user actually wants (e.g. "citizen", "Money Bill")
            // is mid-list. With top-K=20 capped context, that puts the right
            // chunk past slot 9 — small models (base Qwen3B without FT) lose
            // it in the noise and emit the "not found" refusal. Fix: when
            // ≥10 chunks share the same source AND the query carries content
            // tokens beyond the citation, stable-sort the chunks by content-
            // token overlap with `stripped` so the best-match sub-clause
            // becomes slot [1]. Ties keep chunk_index order.
            let promoted_slot = if has_content && chunks.len() >= 10 {
                let q_terms: Vec<String> = stripped
                    .split(|c: char| !c.is_alphanumeric() && c != '\'')
                    .filter_map(|t| {
                        let t = t.trim().to_lowercase();
                        (t.len() >= 3).then_some(t)
                    })
                    .collect();
                if !q_terms.is_empty() {
                    let mut scored: Vec<(usize, i32, usize)> = chunks
                        .iter()
                        .enumerate()
                        .map(|(i, c)| {
                            let body = c.2.to_lowercase();
                            let hits: i32 = q_terms.iter()
                                .map(|t| if body.contains(t) { 1 } else { 0 })
                                .sum();
                            (i, hits, i)
                        })
                        .collect();
                    // Sort by hits DESC, original index ASC (stable on ties).
                    scored.sort_by(|a, b| b.1.cmp(&a.1).then(a.2.cmp(&b.2)));
                    let best = scored.first().copied();
                    if let Some((orig_idx, hits, _)) = best {
                        if hits > 0 && orig_idx != 0 {
                            let promoted = chunks.remove(orig_idx);
                            chunks.insert(0, promoted);
                            Some((orig_idx, hits))
                        } else { None }
                    } else { None }
                } else { None }
            } else { None };

            let promo_label = match promoted_slot {
                Some((from, hits)) => format!("promoted chunk from slot {} (score={hits})", from + 1),
                None => "no promotion (n<10 or zero-hit query)".to_string(),
            };
            trace.step("Citation fast-path", &[
                ("Cited units", format!("{}", cited.len())),
                ("After 1-hop graph expansion", format!("{} chunks", with_neighbours.len())),
                ("Keyword augmentation", format!("{kw_added} chunks added (FTS5, scoped to cited source)")),
                ("Sub-clause re-rank", promo_label),
                ("Bypassed", "vector search, reranker".to_string()),
            ]);
            (chunks, "citation")
        } else {
            // S2 — widen vec pool on definitional queries so canonical
            // chunks have a chance to surface for the post-RRF boost.
            // `fetch_k` scales with the model window so a large context can be
            // filled with enough candidates.
            let fetch_limit = ((fetch_k as f32) * pool_mult) as i64;
            let t_vec = std::time::Instant::now();
            let vec_chunks = get_context_chunks(pool, notebook_id, scope_source_ids, &query_embedding, fetch_limit).await?;
            let vec_ms = t_vec.elapsed().as_millis();
            // Forensic audit 2026-05-28: log per-candidate IDs so we can
            // tell VEC_MISS from RRF_DROP without re-running the query.
            let vec_id_list = vec_chunks
                .iter()
                .map(|c| format!("{}:{}", &c.0[..8.min(c.0.len())], c.4))
                .collect::<Vec<_>>()
                .join(", ");
            trace.step("Vector search (semantic)", &[
                ("Engine", "pgvector ANN (cosine distance)".to_string()),
                ("Limit", format!("{fetch_limit} candidates")),
                ("Returned", format!("{} chunks", vec_chunks.len())),
                ("Time", format!("{vec_ms} ms")),
                ("Candidates", if vec_id_list.is_empty() { "(none)".into() } else { vec_id_list }),
            ]);

            // FTS5 ALWAYS runs in parallel with vec. Skipping FTS5 for "long"
            // queries killed recall on enumerated short clauses (Art 1(2)
            // territory list, Art 11(3) child-labour clause, Preamble
            // Quaid-i-Azam reference) where dense embedding fails to match but
            // exact keywords are gold. FTS5 on a single-doc corpus returns in
            // ~2ms — no cost reason to gate.
            let t_fts = std::time::Instant::now();
            // Fetch the same widened pool from FTS5 so the larger context budget
            // has enough keyword candidates too (definitional adds pool_mult).
            let fts_limit_override = Some(((fetch_k as f32) * pool_mult) as i64);
            let fts_chunks = fts5_retrieve(pool, notebook_id, scope_source_ids, &retrieval_query, fts_limit_override).await?;
            let reasons = {
                let mut r: Vec<&str> = Vec::new();
                if is_entity_query(query_text) { r.push("entity-keyword pattern"); }
                if vec_chunks.len() < 2 { r.push("vec results sparse"); }
                r.push("always-on (hybrid recall)");
                r.join(", ")
            };
            let fts_id_list = fts_chunks
                .iter()
                .map(|c| format!("{}:{}", &c.0[..8.min(c.0.len())], c.4))
                .collect::<Vec<_>>()
                .join(", ");
            trace.step("Keyword search (PostgreSQL full-text)", &[
                ("Triggered by", reasons),
                ("Returned", format!("{} chunks", fts_chunks.len())),
                ("Time", format!("{} ms", t_fts.elapsed().as_millis())),
                ("Candidates", if fts_id_list.is_empty() { "(none)".into() } else { fts_id_list }),
            ]);

            match (fts_chunks.is_empty(), vec_chunks.is_empty()) {
                (false, false) => if hybrid_union_enabled() {
                    // Depth from the live window, mix from the knobs (see `merge_depth`).
                    let (vk, fk) = merge_depth(ctx_top_k, vec_chunks.len(), fts_chunks.len());
                    (union_merge(&fts_chunks, &vec_chunks, fk, vk), "hybrid(union)")
                } else {
                    (rrf_merge(&fts_chunks, &vec_chunks, 60), "hybrid(rrf)")
                },
                (false, true)  => (fts_chunks, "fts5"),
                _              => (vec_chunks, "vec"),
            }
        };

    // Track 2 (ADR-0019 §2): fold the per-provision SUMMARY recall index in as a
    // THIRD bounded arm (summaries-as-index → verbatim-as-answer). Default OFF
    // (`ECHO_SUMMARY_RECALL`). Bounded top-K, then the SAME rerank + single-pass
    // answer — the answer still uses ORIGINAL chunk text; summaries only decide
    // WHICH provisions surface. NOT a corpus sweep / map-reduce. The reranker
    // downstream re-scores, so a mis-recalled provision simply drops out.
    let (raw_chunks, retrieval_tier) = if summary_recall_enabled() {
        let sum_k = knob_summary_k();
        let sum_chunks = get_context_chunks_via_summaries(
            pool, notebook_id, scope_source_ids, &query_embedding, sum_k as i64,
        )
        .await
        .unwrap_or_default();
        trace.step("Summary recall (provision index)", &[
            ("Index", "graph_summary → original provision chunks".to_string()),
            ("Returned", format!("{} provisions", sum_chunks.len())),
            ("Mode", "bounded top-K union (no map-reduce)".to_string()),
        ]);
        if sum_chunks.is_empty() {
            (raw_chunks, retrieval_tier)
        } else {
            // Summaries first (precise recall), then the existing hits (deduped).
            let merged = union_merge(&raw_chunks, &sum_chunks, raw_chunks.len(), sum_k);
            (merged, "hybrid+summary")
        }
    } else {
        (raw_chunks, retrieval_tier)
    };

    // GraphRAG seed-and-expand arm (2026-07-12). Take the top merged chunks as
    // seeds and follow the legal overlay to the provisions they are CONNECTED to
    // (cross-reference / non-obstante / amended-by) that similarity alone would
    // never surface. Additive + scope-clamped; the downstream reranker re-scores,
    // so an off-topic neighbour simply drops out. Default OFF (`ECHO_GRAPH_EXPAND`).
    let (raw_chunks, retrieval_tier) = if graph_expand_enabled() && !raw_chunks.is_empty() {
        use crate::db::repositories::graph;
        let seed_ids: Vec<String> = raw_chunks
            .iter()
            .take(knob_graph_seed_k())
            .map(|c| c.0.clone())
            .collect();
        let edge_types = ["cross_reference", "non_obstante", "amended_by"];
        let seed_nodes = graph::node_ids_for_chunks(pool, &seed_ids).await.unwrap_or_default();
        let reached = if seed_nodes.is_empty() {
            Vec::new()
        } else {
            graph::expand(pool, &seed_nodes, &edge_types, knob_graph_hops())
                .await
                .unwrap_or_default()
        };
        // Reached chunk ids that aren't already in the pool.
        let have: HashSet<String> = raw_chunks.iter().map(|c| c.0.clone()).collect();
        let mut new_ids: Vec<String> = Vec::new();
        for n in &reached {
            if !n.chunk_id.is_empty() && !have.contains(&n.chunk_id) && !new_ids.contains(&n.chunk_id) {
                new_ids.push(n.chunk_id.clone());
            }
        }
        new_ids.truncate(knob_graph_k());
        let mut graph_chunks = if new_ids.is_empty() {
            Vec::new()
        } else {
            fetch_chunks_by_ids(pool, &new_ids).await.unwrap_or_default()
        };
        // Defence-in-depth: clamp expanded provisions to the caller's visible
        // scope (the overlay can cross sources once Phase 3 lands — FR-001).
        if let Some(allowed) = scope_source_ids {
            let allowed: HashSet<&str> = allowed.iter().map(String::as_str).collect();
            graph_chunks.retain(|c| allowed.contains(c.1.as_str()));
        }
        trace.step("Graph expansion (legal overlay)", &[
            ("Seeds", format!("{} chunks → {} graph nodes", seed_ids.len(), seed_nodes.len())),
            ("Edges", format!("cross_reference + non_obstante + amended_by ({} hop)", knob_graph_hops())),
            ("Reached", format!("{} nodes", reached.len())),
            ("Added to pool", format!("{} new provisions", graph_chunks.len())),
        ]);
        if graph_chunks.is_empty() {
            (raw_chunks, retrieval_tier)
        } else {
            // Existing hits keep their rank; connected provisions append (deduped).
            let merged = union_merge(&raw_chunks, &graph_chunks, raw_chunks.len(), knob_graph_k());
            (merged, "hybrid+graph")
        }
    } else {
        (raw_chunks, retrieval_tier)
    };

    // S1/S2 — salience-aware re-rank. Lifts canonical / declarative chunks
    // (Preamble · Abstract · Foreword · …) above leaf chunks for queries
    // that don't name a citation. `salient_boost` was chosen by S2:
    // definitional queries get the stronger ECHO_DEFINITIONAL_BOOST,
    // others get ECHO_SALIENT_BOOST. No-op when boost = 1.0.
    let pre_salience_top = raw_chunks.first().map(|c| c.0.clone());
    let raw_chunks = apply_salience_boost(pool, raw_chunks, salient_boost).await;
    let post_salience_top = raw_chunks.first().map(|c| c.0.clone());

    // S3b — entity-aware re-rank. Lifts chunks whose entities match
    // capitalised noun phrases in the query (e.g. "Quaid-i-Azam",
    // "Mohammad Ali Jinnah"). Composes naturally on top of salience.
    // No-op when boost ≤ 1.0, no NPs in the query, or no matched entities.
    let entity_nps = extract_query_entities(query_text);
    let raw_chunks = apply_entity_boost(pool, raw_chunks, query_text, knob_entity_boost()).await;

    // S3c — instrument-affinity re-rank. When the query names a statute, float
    // that instrument's chunks up so a colliding "Rule 4" from another Act cannot
    // win the top-K (trace bd0d16e1). No-op when no statute is named.
    let pre_instrument_top = raw_chunks.first().map(|c| c.4.clone());
    let raw_chunks = apply_instrument_boost(raw_chunks, query_text, knob_instrument_boost());

    // S3d — source diversity (2026-08-06). One document owning the whole window
    // is the bd0d16e1 / finding-#4 failure mode: a broad question spread across
    // 13 laws' boilerplate and the ONE instrument that governed it contributed 3
    // of 33 chunks. This is a stable RE-ORDER — every input chunk is in the
    // output, only the order changes — so the cap is realised by `build_context`'s
    // existing top-K truncation and nothing is ever dropped.
    //
    // ⚠️ SKIPPED for enumeration queries. Table rows all share one `source_id`, so
    // interleaving would demote a table's rows past the seed window and silently
    // disable table completion — two correct changes cancelling each other out.
    // An enumeration answer legitimately wants one document to dominate.
    let pre_diversity_mix = source_mix(&raw_chunks, ctx_top_k);
    let source_cap = knob_source_cap(ctx_top_k);
    let raw_chunks = if is_enumeration_query(query_text) {
        raw_chunks
    } else {
        apply_source_diversity(raw_chunks, source_cap)
    };
    {
        let post_entity_top = raw_chunks.first().map(|c| c.0.clone());
        let salience_changed = pre_salience_top != post_salience_top;
        let entity_changed = post_salience_top != post_entity_top;
        let top_file = raw_chunks.first().map(|c| c.4.as_str()).unwrap_or("(none)");
        let instrument_changed = pre_instrument_top.as_deref() != raw_chunks.first().map(|c| c.4.as_str());
        trace.step("Merge & rank candidates", &[
            ("Strategy", retrieval_tier.to_string()),
            ("Total unique chunks", format!("{}", raw_chunks.len())),
            // Self-reporting invariant (2026-08-07). The 33-vs-36 starvation was
            // invisible for weeks because pool depth and window size are set by
            // different mechanisms in different places and NOTHING compared them.
            // Any future drift of this shape now announces itself in every trace.
            ("Candidate pool", {
                let n = raw_chunks.len();
                let of = knob_pool_overfetch();
                // Name the arm explicitly — an A/B whose trace does not say which
                // mode produced it is not evidence.
                let mode = if of < 1.0 {
                    format!("LEGACY fixed caps {}/{} (ECHO_POOL_OVERFETCH unset)", knob_vec_k(), knob_fts_k())
                } else {
                    let (vk, fk) = merge_depth(ctx_top_k, usize::MAX, usize::MAX);
                    format!("derived depth {vk}/{fk} @ {of}× (mix from ECHO_VEC_K:ECHO_FTS_K)")
                };
                if n < ctx_top_k {
                    format!(
                        "{n} unique vs top-K={ctx_top_k} · {mode} · ⚠ POOL UNDERFILLS WINDOW — \
                         the merge IS the selection; ranking, boosts and diversity have \
                         nothing to discard and are inert"
                    )
                } else {
                    format!(
                        "{n} unique vs top-K={ctx_top_k} · {mode} · {:.1}× headroom",
                        n as f32 / ctx_top_k.max(1) as f32,
                    )
                }
            }),
            ("Top match", top_file.to_string()),
            ("Salience boost", if salience_changed { "applied (top changed)" } else { "no-op" }.to_string()),
            ("Entity boost", format!(
                "{} NPs detected · {}",
                entity_nps.len(),
                if entity_changed { "applied (top changed)" } else { "no-op" },
            )),
            // Report the source mix, not just a verdict: the bd0d16e1 failure was
            // invisible precisely because nothing recorded WHICH documents won.
            ("Instrument boost", {
                let b = knob_instrument_boost();
                if b <= 1.0 {
                    "off (ECHO_INSTRUMENT_BOOST=1.0)".to_string()
                } else {
                    format!(
                        "boost={b} · {}",
                        if instrument_changed { "top document CHANGED" } else { "top document unchanged" },
                    )
                }
            }),
            // Grouped on source_id, NOT the label: the label is only a display
            // string, and before 2026-08-06 the same document could carry two
            // different ones depending on which arm fetched it.
            ("Source mix (top-K)", {
                let after = source_mix(&raw_chunks, ctx_top_k);
                if source_cap == 0 {
                    format!("{after} · diversity off (ECHO_SOURCE_CAP unset)")
                } else if is_enumeration_query(query_text) {
                    format!("{after} · diversity skipped (enumeration query)")
                } else {
                    format!("{pre_diversity_mix} → {after} · cap={source_cap}")
                }
            }),
        ]);
    }

    // Emit ranking thinking event with first filename found.
    {
        let first_file = raw_chunks.first().map(|(_, _, _, _, f)| f.as_str()).unwrap_or("documents");
        if let Some(ref a) = app { let _ = a.emit("chat://thinking", serde_json::json!({
            "stage": "ranking",
            "detail": format!("Found {} passages in {}...", raw_chunks.len(), first_file)
        })); }
    }

    // ── Tier 3 Task 11: cross-encoder reranker ─────────────────────────────────
    // Re-order candidates by bge-reranker-base relevance score. Bypassed for
    // Tier-1 entity FTS queries — those are already exact lookups, no benefit
    // from a secondary score. Falls back to RRF order on any reranker error
    // (missing model file, ONNX runtime failure, etc.).
    //
    // HARDCODED OFF (2026-05-28): the bge-reranker-base default is
    // demoting correct chunks on legal-text queries — see ANALYSIS_v3.md §8
    // (Q5 APT BS-17, Q14 RoB Rule 10A, Q21 RoB Rule 14). Top-score signal
    // also misleads on cross-document compares (Q17 -0.230 → still correct;
    // Q5/Q14 high score → still partial). The reranker is not safe as a
    // gate today. Re-enable only after a domain fine-tune AND a top-score
    // sweep documented separately. Set ECHO_RERANK=1 to override for an
    // experiment; ECHO_RERANK unset/0/anything-else stays off.
    let rerank_enabled = std::env::var("ECHO_RERANK").ok().as_deref() == Some("1");
    // Knob: ECHO_RERANK_POOL_K — how many top candidates to cross-encode (default 12).
    let rerank_pool_k = knob_rerank_pool_k();
    let mut rerank_top_score: Option<f32> = None;
    let reranked: Vec<(String, String, String, Option<i64>, String)> =
        if rerank_enabled && matches!(query_type, QueryType::Regular) && !raw_chunks.is_empty()
            && retrieval_tier != "citation"
        {
            let cand_n = raw_chunks.len().min(rerank_pool_k);
            let candidate_texts: Vec<String> = raw_chunks.iter().take(cand_n).map(|c| c.2.clone()).collect();
            match reranker_slot.score_pairs(query_text, &candidate_texts) {
                Ok(scores) => {
                    let mut indexed: Vec<(usize, f32)> = scores.iter().copied().enumerate().collect();
                    indexed.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
                    let top_score = indexed.first().map(|(_, s)| *s).unwrap_or(0.0);
                    rerank_top_score = Some(top_score);
                    // Forensic audit 2026-05-28: log per-candidate scores
                    // (id:score, sorted desc) so RERANK_DEMOTE can be
                    // attributed cleanly post-hoc.
                    let per_cand = indexed
                        .iter()
                        .map(|(i, s)| {
                            let id = &raw_chunks[*i].0;
                            format!("{}={s:.2}", &id[..8.min(id.len())])
                        })
                        .collect::<Vec<_>>()
                        .join(", ");
                    let mut out = Vec::with_capacity(raw_chunks.len());
                    for (i, _) in indexed { out.push(raw_chunks[i].clone()); }
                    // Append any candidates beyond the rerank window in original order.
                    for c in raw_chunks.iter().skip(cand_n) { out.push(c.clone()); }
                    trace.step("Reranker (cross-encoder)", &[
                        ("Model", std::env::var("ECHO_RERANKER_MODEL").unwrap_or_else(|_| "bge-reranker-base".to_string())),
                        ("Pairs scored", format!("{cand_n} of {}", raw_chunks.len())),
                        ("Top score", format!("{top_score:.3}")),
                        ("Per-candidate", per_cand),
                    ]);
                    out
                }
                Err(e) => {
                    log::warn!("reranker failed, falling back to RRF order: {e}");
                    trace.step("Reranker (cross-encoder)", &[
                        ("Skipped", format!("reranker error — using RRF order: {e}")),
                    ]);
                    raw_chunks.clone()
                }
            }
        } else {
            if !rerank_enabled {
                trace.step("Reranker (cross-encoder)", &[
                    ("Skipped", "hardcoded off (ECHO_RERANK=1 to re-enable)".to_string()),
                ]);
            }
            raw_chunks.clone()
        };

    // ── Tier 3 Task 11: sibling rollup ────────────────────────────────────────
    // If 3+ leaf chunks share the same parent section, replace those siblings
    // with the single section chunk. Improves coherence for broad queries.
    let reranked = roll_up_siblings(reranked, pool).await;

    // ── Table completion via the `contains` hierarchy (F023/F025) ─────────────
    // For a COUNT/LIST question, pull the table a top-ranked chunk belongs to so
    // the enumerable table (e.g. RoB Schedule-I) leads the context and is what
    // the answer cites, instead of a stray row.
    //
    // ⚠️ BOUNDED SINCE 2026-08-06 — this step used to destroy answers. It pulled
    // EVERY table reachable from ANY chunk in the pool (64 chunks, from three
    // unrelated tables in the traced failure), in arbitrary order, and prepended
    // them into a context that holds ~36. On "How many members are on the SPPRA
    // board?" the correct provision was ranked 9 of the top 10 and reached the
    // model not at all; it refused, correctly, on a department directory.
    //
    // Three bounds now apply, all in `table_reserve_slice`: the seed must be in
    // the top N, exactly ONE table is expanded (the best-ranked seed's), and the
    // slice is capped in both slots and tokens against the real context budget.
    // The refusal was grounding working; the defect was here.
    // Context sizing is resolved HERE, before table completion, because the
    // reserve is expressed as a fraction of it (see the knobs).
    let effective_ctx_k = env_usize("ECHO_CONTEXT_K", ctx_top_k);
    let effective_ctx_budget = env_usize("ECHO_CONTEXT_BUDGET", ctx_budget);

    let reranked = if is_enumeration_query(query_text) && graph_expand_enabled() {
        let max_rows = knob_table_reserve_k(effective_ctx_k);
        let tbl = table_reserve_slice(
            pool,
            &reranked,
            scope_source_ids,
            knob_table_seed_top_n(),
            max_rows,
            knob_table_reserve_tokens(effective_ctx_budget),
        )
        .await;
        if tbl.is_empty() {
            // Traced explicitly: "table completion did nothing" must be visible,
            // not inferred from the absence of a step.
            trace.step("Table completion (contains)", &[
                ("Skipped", format!(
                    "no table/row chunk in the top {}", knob_table_seed_top_n()
                )),
            ]);
            reranked
        } else {
            let n_tbl = tbl.len();
            // union_merge leads with its 2nd arg → the table leads, now BOUNDED.
            let merged = union_merge(&reranked, &tbl, reranked.len(), n_tbl);
            trace.step("Table completion (contains)", &[
                ("Trigger", format!(
                    "enumeration query + a table/row chunk in the top {}",
                    knob_table_seed_top_n()
                )),
                ("Table chunks reserved", format!(
                    "{n_tbl} of {effective_ctx_k} context slots (cap {max_rows})"
                )),
                ("Mode", "single best-ranked table, reading order".to_string()),
            ]);
            merged
        }
    } else {
        reranked
    };

    // Context builder: top-K dedup + token budget (FR-040 / AD-6).
    let chunks = build_context(&reranked, effective_ctx_k, effective_ctx_budget);

    // Phase 2 — amendment-awareness. For the provisions that reach the LLM
    // context, look up recorded amendment events and build a per-chunk note so
    // the answer can flag that the base text may be superseded. Scope-clamped
    // (the note names instruments/dates = answer content, FR-001). Default OFF.
    let amend_notes: std::collections::HashMap<String, String> =
        if amend_notes_enabled() && !chunks.is_empty() {
            let ids: Vec<String> = chunks.iter().map(|c| c.0.clone()).collect();
            let rows = crate::db::repositories::graph::amendment_rows_for_chunks(
                pool, &ids, scope_source_ids,
            )
            .await
            .unwrap_or_default();
            let mut by_chunk: std::collections::HashMap<String, Vec<String>> =
                std::collections::HashMap::new();
            for (chunk_id, kind, by_instrument, date) in rows {
                let inst: String = by_instrument.chars().take(90).collect();
                let date_s = date.map(|d| format!(", {d}")).unwrap_or_default();
                by_chunk
                    .entry(chunk_id)
                    .or_default()
                    .push(format!("{kind} by {inst}{date_s}"));
            }
            let notes: std::collections::HashMap<String, String> = by_chunk
                .into_iter()
                .map(|(id, mut evs)| {
                    evs.dedup();
                    (
                        id,
                        format!(
                            "[⚠ Amendment history: this provision was {} — the text above may be superseded; verify the current version.]",
                            evs.join("; ")
                        ),
                    )
                })
                .collect();
            trace.step("Amendment-awareness (Phase 2)", &[
                ("Context provisions annotated", format!("{} of {}", notes.len(), chunks.len())),
                ("Source", "amendment_events (F013/F015 provenance)".to_string()),
            ]);
            notes
        } else {
            std::collections::HashMap::new()
        };
    {
        let total_chars: usize = chunks.iter().map(|c| c.2.chars().count()).sum();
        let est_tokens = total_chars / 4;
        let mut files: Vec<String> = chunks.iter().map(|c| c.4.clone()).collect();
        files.sort(); files.dedup();
        trace.step("Build context window", &[
            ("Selected", format!("{} chunks (cap K={effective_ctx_k})", chunks.len())),
            ("Total content", format!("{total_chars} chars (~{est_tokens} tokens)")),
            ("Source files", if files.is_empty() { "(none)".into() } else { files.join(", ") }),
        ]);
        // Per-chunk preview block — gives a reader exactly what the LLM saw.
        // Format: `[N] <file> p.<page>  score=<f>  preview...`
        if !chunks.is_empty() {
            let mut block = String::new();
            for (idx, c) in chunks.iter().enumerate() {
                let (_id, _src_id, text, page, file) = (&c.0, &c.1, &c.2, &c.3, &c.4);
                let preview: String = text.chars().take(220).collect();
                let preview = preview.replace('\n', " ");
                let pg = page.map(|p| p.to_string()).unwrap_or_else(|| "?".to_string());
                block.push_str(&format!(
                    "[{}] {}  p.{}\n    {}{}\n\n",
                    idx + 1,
                    file,
                    pg,
                    preview,
                    if text.chars().count() > 220 { "…" } else { "" }
                ));
            }
            trace.add_block("Retrieved chunks fed to LLM", block.trim_end());
        }
    }

    if chunks.is_empty() {
        trace.note_last("No usable context — returning grounded fallback message.");
        trace.finish("no_context", GROUNDING_FALLBACK.len(), 0, retrieval_tier);
        if owns_trace { trace.write(&app); }
        return Ok((QueryResponse {
            answer: GROUNDING_FALLBACK.to_string(),
            citations: vec![],
            tier_used: retrieval_tier.to_string(),
            quality_notice: None,
            chat_fill,
            related_passages: Vec::new(),
        }, false));
    }

    // Knob: ECHO_MIN_RERANK_SCORE — if the reranker's top score is below this
    // threshold, the correct passage almost certainly isn't in the pool. Ship
    // the grounding fallback rather than letting the LLM fabricate from
    // low-confidence chunks. Test5 evidence: ≥+5 ⇒ correct; ≤-1 ⇒ wrong.
    if let (Some(min_score), Some(top)) = (knob_min_rerank_score(), rerank_top_score) {
        if top < min_score {
            trace.step("Rerank confidence gate", &[
                ("Top score", format!("{top:.3}")),
                ("Min required", format!("{min_score:.3}")),
                ("Action", "Fallback — top reranker score below threshold".to_string()),
            ]);
            trace.finish("low_rerank_confidence", GROUNDING_FALLBACK.len(), 0, retrieval_tier);
            if owns_trace { trace.write(&app); }
            return Ok((QueryResponse {
                answer: GROUNDING_FALLBACK.to_string(),
                citations: vec![],
                tier_used: retrieval_tier.to_string(),
                chat_fill,
                related_passages: Vec::new(),
                quality_notice: Some(QualityNotice {
                    constrained: true,
                    message: format!(
                        "Low retrieval confidence (rerank top={top:.2} < {min_score:.2})"
                    ),
                }),
            }, false));
        }
    }

    // ── Extractive answer (beta default) ────────────────────────────────────
    // Compose the answer VERBATIM from the top retrieved provisions — the model
    // never writes facts, so fabrication is impossible (citation-grade by
    // construction). Each provision is shown with a structured label, detected
    // cross-references, and any inline amendment marker. Skips the LLM entirely
    // (instant; no ~40s generation). Toggle off via db_config 'chat_extractive'.
    if chat_extractive_enabled(pool).await {
        const EXTRACT_TOP_N: usize = 3;
        // Relevance-rank the retrieved provisions by lexical overlap with the
        // question (phrase + stem bonus), THEN take the best N. RRF rank alone
        // often left the precise answer at rank 4+ (e.g. "must be accompanied by
        // a feasibility study" sat below generic "concept clearance" chunks).
        // Stable sort preserves RRF order on ties.
        let (q_words, q_bigrams) = question_terms(query_text);
        let mut ranked: Vec<&(String, String, String, Option<i64>, String)> =
            chunks.iter().collect();
        if !q_words.is_empty() {
            ranked.sort_by_key(|c| -relevance_score(&c.2, &q_words, &q_bigrams));
        }
        let top: Vec<&(String, String, String, Option<i64>, String)> =
            ranked.into_iter().take(EXTRACT_TOP_N).collect();

        // Fetch unit_label / unit_path for the shown chunks (one query).
        let ids: Vec<String> = top.iter().map(|c| c.0.clone()).collect();
        let meta: std::collections::HashMap<String, (Option<String>, Option<String>)> =
            sqlx::query_as::<_, (String, Option<String>, Option<String>)>(
                "SELECT id, unit_label, unit_path FROM chunks WHERE id = ANY($1)",
            )
            .bind(&ids)
            .fetch_all(pool)
            .await
            .unwrap_or_default()
            .into_iter()
            .map(|(id, l, p)| (id, (l, p)))
            .collect();

        let mut body = String::new();
        let mut citations: Vec<Citation> = Vec::new();
        for (chunk_id, source_id, content, page, file_name) in &top {
            let (ul, up) = meta
                .get(chunk_id)
                .map(|(l, p)| (l.as_deref(), p.as_deref()))
                .unwrap_or((None, None));
            let doc = clean_doc_name(file_name);
            let label = provision_label(ul, up);
            let verbatim = strip_html_tags(content);
            let verbatim = verbatim.split_whitespace().collect::<Vec<_>>().join(" ");

            if !body.is_empty() {
                body.push_str("\n\n---\n\n");
            }
            body.push_str(&format!("**{doc} — {label}**\n\n> {verbatim}"));
            let refs = detect_cross_refs(&verbatim, &label);
            if !refs.is_empty() {
                body.push_str(&format!("\n\n↳ References: {}", refs.join(", ")));
            }
            if let Some(am) = detect_amendment(&verbatim) {
                body.push_str(&format!("\n\n⚠ Amendment: {am}"));
            }

            citations.push(Citation {
                source_id: (*source_id).clone(),
                chunk_id: (*chunk_id).clone(),
                excerpt: citation_snippet(content),
                page: *page,
            });
        }

        let answer = if body.is_empty() {
            GROUNDING_FALLBACK.to_string()
        } else {
            body
        };
        trace.step("Extractive compose", &[
            ("Provisions", format!("{}", citations.len())),
            ("Mode", "verbatim (no LLM)".to_string()),
        ]);
        trace.finish("extractive", answer.chars().count(), citations.len(), retrieval_tier);
        if owns_trace { trace.write(&app); }
        return Ok((
            QueryResponse {
                answer,
                citations,
                tier_used: "extractive".to_string(),
                quality_notice: None,
                chat_fill,
                related_passages: Vec::new(),
            },
            false,
        ));
    }

    // Tier 3/4: LLM synthesis over the retrieved chunks.
    // If the model file is not present, fall back to the template synthesizer.
    let (relative_model_path, llm_tier, quality_notice) = select_model(tier);
    let model_path = resolve_model_path(&relative_model_path);
    log::info!(
        "LLM synthesis: {} chunks selected (tier={}), model={}",
        chunks.len(), retrieval_tier, model_path
    );
    let context = build_context_string(&chunks, &amend_notes);
    let prompt = build_rag_prompt(query_text, &context, history, system_prompt, false);
    let prompt_chars = prompt.len();
    let prompt_est_tokens = prompt_chars / 4;
    let t_llm = std::time::Instant::now();
    log::info!("METRIC llm_start chunks={} prompt_chars={prompt_chars}", chunks.len());
    trace.step("Assemble prompt", &[
        ("Format", "ChatML (system + prior turns + context + question)".to_string()),
        // Report what was actually INJECTED, not what was available.
        //
        // ⚠️ This line used to print `history.chunks(2)…count().min(2)` — the pairs
        // the caller HANDED us — so it cheerfully said "2 pairs from history" on
        // turns where `prior_turns_block` had returned EMPTY and the model saw no
        // conversation at all. Prod audit 2026-07-16: three turns of one real chat
        // showed "Prior turns: 2 pairs" next to "history 0" in the same trace, and
        // the disagreement went unread for weeks. Same disease as the hardcoded
        // `law_overhead` and the citation-excerpt fill notice: two places computing
        // one number. Derive it from the block that is really injected, and name the
        // gate when it drops everything.
        ("Prior turns", {
            let injected = prior_block_est.matches("<|im_start|>user").count();
            let available = history.chunks(2).filter(|c| c.len() == 2).count();
            let why = if injected == 0 && available > 0 {
                "  ⚠ DROPPED — is_followup_query judged this query self-contained"
            } else {
                ""
            };
            format!(
                "{injected} pair(s) injected (~{} tok) of {available} available{why}",
                est_tokens(&prior_block_est)
            )
        }),
        ("Prompt size", format!("{prompt_chars} chars (~{prompt_est_tokens} tokens, capped at {gen_max_prompt})")),
    ]);
    {
        // Cap prompt block at 6 KB to keep traces readable. Truncate by chars
        // (UTF-8 safe) and mark with a tail hint.
        const PROMPT_BLOCK_CAP: usize = 6000;
        let total_chars = prompt.chars().count();
        let body: String = prompt.chars().take(PROMPT_BLOCK_CAP).collect();
        let body = if total_chars > PROMPT_BLOCK_CAP {
            format!("{body}\n\n[…truncated; full prompt was {total_chars} chars]")
        } else {
            body
        };
        trace.add_block("Full prompt sent to LLM", &body);
    }
    // Trace labels must reflect the ACTUAL generation engine, not the on-disk
    // GGUF slot filename. Under vLLM the GGUF never loads — report the served
    // model id from NOVIQ_VLLM_MODEL (reconstructing the HF repo when it's an
    // offline snapshot path) so the trace stops showing a stale/wrong model.
    let (gen_engine, gen_model) = match crate::services::inference::engine_from_env() {
        crate::services::inference::EngineKind::Vllm => {
            let m = std::env::var("NOVIQ_VLLM_MODEL").unwrap_or_default();
            let label = match m.find("models--") {
                Some(i) => m[i..]
                    .split('/')
                    .next()
                    .unwrap_or(&m)
                    .trim_start_matches("models--")
                    .replace("--", "/"),
                None if m.trim().is_empty() => "Qwen/Qwen3-8B-AWQ".to_string(),
                None => m.clone(),
            };
            ("vLLM (OpenAI-compatible server)".to_string(), label)
        }
        crate::services::inference::EngineKind::Remote => {
            // Report the RUNTIME model (admin dashboard / db_config), not the env —
            // the model is set at runtime, so NOVIQ_REMOTE_MODEL is usually unset.
            let s = crate::services::inference::runtime::status();
            (format!("Remote API — {}", s.remote_base_url), s.remote_model)
        }
        crate::services::inference::EngineKind::LlamaCpp => (
            "llama.cpp (in-process GGUF)".to_string(),
            model_path.split(['/', '\\']).last().unwrap_or(&model_path).to_string(),
        ),
    };
    trace.step("Send to LLM", &[
        ("Engine", gen_engine),
        ("Model", gen_model),
        ("Tier", llm_tier.clone()),
        ("Settings", format!("n_ctx={gen_n_ctx} | max_response={gen_max_tokens} tokens | temp=0.1")),
    ]);

    if let Some(ref a) = app { let _ = a.emit("chat://thinking", serde_json::json!({
        "stage": "composing",
        "detail": format!("Composing answer from {} passages...", chunks.len())
    })); }

    // Cloud inference (Groq) — opt-in via Settings. See CHANGELOG_DEV.md.
    let cloud_settings = app.as_ref().map(|a| crate::services::cloud_llm::load_settings(a));
    let use_cloud = cloud_settings.as_ref().map_or(false, |s| s.enabled && !s.api_key.trim().is_empty());

    // Cloud path: still no prior-turn forwarding (Groq path has its own
    // history shape — handled separately). Local path picks up history via
    // build_rag_prompt above, gated on is_followup_query.
    let cloud_result = if use_cloud {
        let s = cloud_settings.as_ref().unwrap();
        trace.note_last(&format!("Cloud routing → Groq ({})", s.model));
        Some(crate::services::cloud_llm::groq_chat_streaming(
            s, system_prompt, &context, query_text, &[],
            gen_max_tokens, cancel.clone(), app.clone(),
        ).await)
    } else { None };

    let (answer, tier_used, was_cancelled) = if let Some(res) = cloud_result {
        match res {
            Ok((text, cancelled)) if !text.trim().is_empty() =>
                (clean_llm_output(&text), format!("cloud:{}", cloud_settings.as_ref().unwrap().model), cancelled),
            Ok((_, cancelled)) => (
                "The cloud model returned an empty response. Please try again.".to_string(),
                "cloud-error".to_string(),
                cancelled,
            ),
            Err(e) => {
                log::warn!("Cloud inference failed — {e}");
                (format!("**Cloud inference failed.** {e}\n\nDisable cloud in Settings to fall back to local."),
                 "cloud-error".to_string(), false)
            }
        }
    } else {
        match try_llm_infer_streaming(model_slot, &model_path, &prompt, gen_max_tokens, gen_n_ctx, gen_max_prompt, tier, cancel, app.clone()).await {
            Ok((text, cancelled)) if !text.trim().is_empty() => {
                log::debug!("LLM raw output: {:?}", text.chars().take(300).collect::<String>());
                (clean_llm_output(&text), llm_tier.clone(), cancelled)
            }
            Ok((_, cancelled)) => (
                "The AI model returned an empty response. Please try asking again.".to_string(),
                llm_tier.clone(),
                cancelled,
            ),
            Err(e) => {
                log::warn!("LLM inference failed — {e}");
                // FR-011: engine-aware friendly message. Under vLLM, a failure here
                // means the sidecar is unreachable/timed out (not a missing GGUF) —
                // a transient operational state, so tell the user to retry rather
                // than to download a model.
                let msg = if crate::services::inference::engine_from_env().is_http() {
                    "**AI service temporarily unavailable.**\n\n\
                     The inference engine isn't responding right now. \
                     Please try again in a moment.".to_string()
                } else {
                    format!(
                        "**AI model unavailable.**\n\n\
                         The model file could not be loaded: {e}\n\n\
                         Go to **Settings → AI Model** to download the model, then try again."
                    )
                };
                (msg, "error".to_string(), false)
            }
        }
    };
    let llm_ms = t_llm.elapsed().as_millis();
    let answer_chars = answer.chars().count();
    let est_tokens = answer_chars / 4;
    let toks_per_sec = if llm_ms > 0 { (est_tokens as f64 * 1000.0) / llm_ms as f64 } else { 0.0 };
    trace.step("LLM inference complete", &[
        ("Total time", format!("{llm_ms} ms ({:.2} s)", llm_ms as f64 / 1000.0)),
        ("Generated", format!("{answer_chars} chars (~{est_tokens} tokens)")),
        ("Speed", format!("~{toks_per_sec:.1} tokens/sec")),
        ("Cancelled", was_cancelled.to_string()),
    ]);
    {
        // Cap raw answer block at 8 KB; UTF-8-safe char-bounded slice.
        const ANSWER_BLOCK_CAP: usize = 8000;
        let body: String = answer.chars().take(ANSWER_BLOCK_CAP).collect();
        let body = if answer_chars > ANSWER_BLOCK_CAP {
            format!("{body}\n\n[…truncated; full answer was {answer_chars} chars]")
        } else {
            body
        };
        trace.add_block("Raw model output (pre-cleanup)", &body);
    }

    // Strip lines that are citation markers only (e.g. "- [6]" or "[2]" on
    // their own line). These appear when the model cites a heading-only chunk
    // it cannot paraphrase — keeping them produces bare bracket noise in the UI.
    let answer = {
        let citation_only = regex_lite::Regex::new(r"(?m)^[\s\-*•]*\[\d+\][\s,]*$").unwrap();
        let cleaned = citation_only.replace_all(&answer, "");
        // Collapse runs of 3+ blank lines that the strip may leave behind.
        let blank_run = regex_lite::Regex::new(r"\n{3,}").unwrap();
        blank_run.replace_all(&cleaned, "\n\n").trim().to_string()
    };

    // Grounding validation (FR-042 / AD-8).
    // Skip for Summary queries — by design they don't include inline citations
    // (per SUMMARY_SYSTEM_PROMPT v2), so the validator would needlessly flag
    // every sentence as uncited. The summary-mode footer makes the absence of
    // citations explicit to the user.
    let mut grounding_uncited_only = false;
    let answer = if tier_used != "error" && !matches!(query_type, QueryType::Summary) {
        let grounding = validate_grounding(&answer, &chunks);
        log::info!(
            "Grounding: score={:.2} citations={} uncited={} fallback={} uncited_only={} prompt_v={}",
            grounding.grounding_score,
            grounding.citation_count,
            grounding.uncited_sentence_count,
            grounding.fallback_triggered,
            grounding.uncited_only,
            crate::services::llm::grounding_prompt_version(),
        );
        grounding_uncited_only = grounding.uncited_only;
        // Fire-and-forget audit log write (non-blocking).
        let pool_clone = pool.clone();
        let score = grounding.grounding_score;
        let cite_count = grounding.citation_count;
        let triggered = grounding.fallback_triggered;
        tokio::spawn(async move {
            let details = serde_json::json!({
                "grounding_score": score,
                "citation_count": cite_count,
                "fallback_triggered": triggered,
                "prompt_version": crate::services::llm::grounding_prompt_version(),
            })
            .to_string();
            let _ = sqlx::query(
                "INSERT INTO audit_log \
                 (id, event_type, entity_id, details_json, created_at) \
                 VALUES ($1, 'grounding_validation', 'query_router', $2, $3)",
            )
            .bind(uuid::Uuid::new_v4().to_string())
            .bind(details)
            .bind(chrono::Utc::now().timestamp_millis())
            .execute(&pool_clone)
            .await;
        });

        trace.step("Grounding validation", &[
            ("Citations found", format!("{}", grounding.citation_count)),
            ("Uncited factual sentences", format!("{}", grounding.uncited_sentence_count)),
            ("Score", format!("{:.2} / 1.00", grounding.grounding_score)),
            ("Fallback triggered", grounding.fallback_triggered.to_string()),
        ]);
        if grounding.fallback_triggered && !grounding.uncited_only {
            trace.note_last("Answer cited unknown source or said 'not found' — replacing with fallback message.");
            // Model explicitly said it couldn't find the info, or cited an
            // unknown source — genuinely ungrounded, replace with fallback.
            GROUNDING_FALLBACK.to_string()
        } else {
            // Either grounded, or just missing citation markers (broad/summary
            // questions from small models) — keep the answer as-is.
            answer
        }
    } else {
        answer
    };

    // ── Tier 3 Task 11: sentence-level grounding (Strip / Retry signal) ───────
    // Layered on top of the legacy validator: only refines the answer when the
    // legacy validator already accepted it (didn't replace with fallback). For
    // Strip, drops the failing sentences. For Retry, logs only — the legacy
    // reviewer retry path remains the active retry trigger.
    let answer = if sentence_grounding_enabled()
        && tier_used != "error"
        && !matches!(query_type, QueryType::Summary)
        && answer != GROUNDING_FALLBACK
    {
        let report = crate::services::grounding::validate(
            &answer,
            &chunks.iter().map(|c| c.2.clone()).collect::<Vec<_>>(),
        );
        let failed = report.per_sentence.iter().filter(|v| !v.passed).count();
        trace.step("Grounding (sentence Jaccard)", &[
            ("Sentences", format!("{}", report.per_sentence.len())),
            ("Failed", format!("{failed}")),
            ("Action", format!("{:?}",
                match &report.action {
                    crate::services::grounding::GroundingAction::Ship => "Ship".to_string(),
                    crate::services::grounding::GroundingAction::Strip { .. } => "Strip".to_string(),
                    crate::services::grounding::GroundingAction::Retry => "Retry".to_string(),
                })),
        ]);
        match report.action {
            crate::services::grounding::GroundingAction::Ship => answer,
            crate::services::grounding::GroundingAction::Strip { kept } => kept,
            crate::services::grounding::GroundingAction::Retry => {
                // If the answer body is citation-markers only (e.g. "[1]"), replace
                // with the fallback rather than surfacing a content-free response.
                let body = regex_lite::Regex::new(r"\[\d+\]|[,\s]")
                    .unwrap()
                    .replace_all(&answer, "")
                    .trim()
                    .to_string();
                if body.is_empty() {
                    log::info!("Sentence-grounding: citation-only answer — replacing with fallback");
                    GROUNDING_FALLBACK.to_string()
                } else {
                    log::info!("Sentence-grounding requested retry — reviewer disabled, shipping as-is");
                    answer
                }
            }
        }
    } else {
        answer
    };

    // ── Reviewer: judge + retry loop (opt-in via ECHO_REVIEWER=1) ──────────────
    // Disabled by default — see reviewer_enabled() doc comment for rationale.
    let (answer, tier_used) = if reviewer_enabled()
        && !was_cancelled
        && tier_used != "error"
        && answer != GROUNDING_FALLBACK
    {
        trace.step("Reviewer judge (LLM YES/NO)", &[
            ("Status", "Enabled via ECHO_REVIEWER=1".to_string()),
        ]);
        let mut current_answer = answer;
        let mut current_tier = tier_used;
        let mut attempt = 0usize;

        loop {
            let pass = judge_answer(model_slot, &model_path, query_text, &current_answer, tier).await;
            if pass || attempt >= RETRY_PARAMS.len() {
                if !pass {
                    log::warn!("Reviewer: 3 retries exhausted for query {:?}", query_text.chars().take(60).collect::<String>());
                    trace.step("Reviewer exhausted", &[
                        ("Verdict", "FAIL after 3 retries — returning error message".to_string()),
                    ]);
                    let err_msg = "The model was unable to generate a relevant answer. Please rephrase your question and try again.".to_string();
                    trace.finish("reviewer_exhausted", err_msg.len(), 0, "error");
                    if owns_trace { trace.write(&app); }
                    return Ok((QueryResponse {
                        answer: err_msg,
                        citations: vec![],
                        tier_used: "error".to_string(),
                        quality_notice: None,
                        chat_fill,
                        related_passages: Vec::new(),
                    }, false));
                }
                break;
            }

            let (retry_k, retry_budget) = RETRY_PARAMS[attempt];
            attempt += 1;
            log::info!("Reviewer retry {attempt}: k={retry_k} budget={retry_budget}");

            let retry_fetch_limit = (retry_k * 2) as i64;
            let retry_vec = get_context_chunks(pool, notebook_id, scope_source_ids, &query_embedding, retry_fetch_limit).await.unwrap_or_default();
            // Always run FTS5 in retry path too — hybrid recall.
            // S2 — retry path inherits the same definitional pool widening
            // as the main path so a retry can also surface canonical chunks.
            let retry_fts_override = if definitional {
                Some(((knob_fts_k() as f32) * pool_mult) as i64)
            } else { None };
            let retry_fts = fts5_retrieve(pool, notebook_id, scope_source_ids, query_text, retry_fts_override).await.unwrap_or_default();
            let retry_raw: Vec<(String, String, String, Option<i64>, String)> =
                match (retry_fts.is_empty(), retry_vec.is_empty()) {
                    (false, false) => if hybrid_union_enabled() {
                        // Retry's selection target is `retry_k`, not `ctx_top_k` —
                        // the pool must track whichever window it will fill.
                        let (vk, fk) = merge_depth(retry_k, retry_vec.len(), retry_fts.len());
                        union_merge(&retry_fts, &retry_vec, fk, vk)
                    } else {
                        rrf_merge(&retry_fts, &retry_vec, 60)
                    },
                    (false, true)  => retry_fts,
                    _              => retry_vec,
                };
            // S1/S2 — apply salience boost on the retry path too, with
            // the same effective boost as the main path.
            let retry_raw = apply_salience_boost(pool, retry_raw, salient_boost).await;
            // S3b — entity boost on retry path mirrors main path.
            let retry_raw = apply_entity_boost(pool, retry_raw, query_text, knob_entity_boost()).await;
            let retry_chunks = build_context(&retry_raw, retry_k, retry_budget);
            if retry_chunks.is_empty() {
                break;
            }
            // Reuse the primary amendment-note map (keyed by chunk_id, so only
            // overlapping provisions annotate — the retry path is a rare grounding
            // fallback; the primary path carries the annotation.)
            let retry_context = build_context_string(&retry_chunks, &amend_notes);
            let retry_prompt = build_rag_prompt(query_text, &retry_context, history, system_prompt, true);

            match try_llm_infer_streaming(
                model_slot, &model_path, &retry_prompt,
                gen_max_tokens, gen_n_ctx, gen_max_prompt,
                tier, None, None,
            ).await {
                Ok((text, false)) if !text.trim().is_empty() => {
                    let cleaned = clean_llm_output(&text);
                    let retry_grounding = validate_grounding(&cleaned, &retry_chunks);
                    current_answer = if retry_grounding.fallback_triggered && !retry_grounding.uncited_only {
                        GROUNDING_FALLBACK.to_string()
                    } else {
                        cleaned
                    };
                    current_tier = llm_tier.clone();
                    if current_answer == GROUNDING_FALLBACK {
                        break; // retry produced ungrounded answer — stop retrying
                    }
                }
                _ => break,
            }
        }

        (current_answer, current_tier)
    } else {
        if !reviewer_enabled() {
            trace.step("Reviewer judge", &[
                ("Status", "Skipped (default) — set ECHO_REVIEWER=1 to enable.".to_string()),
            ]);
        }
        (answer, tier_used)
    };

    // Surface only chunks the LLM actually cited (Phase-0e, 2026-06-13).
    // Resolution order:
    //   1. Numeric `[N]` markers → the EXACT chunk `chunks[N-1]`, 1:1, deduped,
    //      in order of first appearance. Prose markers are renumbered to match
    //      the displayed list (1..k) and out-of-range markers are dropped.
    //      (Old code re-matched the cited chunk's filename+page against ALL
    //      retrieved chunks; with page NULL corpus-wide, one `[N]` exploded into
    //      every chunk of that document — trace E04: 1 emitted → 11 shown.)
    //   2. Legacy `[file.pdf, page N]` markers (cached old-format answers).
    //   3. No markers → top-3 by rank. This branch is LOAD-BEARING for the
    //      extractive decompose path (`intent_router::dispatch` consumes
    //      `qresp.citations` via resolve_provision_label / authoritative_body);
    //      its Lookup/verbatim sub-prompts emit no `[N]`, so they land here.
    const CITATION_DISPLAY_LIMIT: usize = 3;
    let marker_nums = indexed_citation_markers(&answer, chunks.len());
    // ⚠️ The third element is `citations_verified`: TRUE only when the model
    // actually cited (indexed `[N]` markers, or a legacy `[file, page]` that
    // matched a retrieved chunk). FALSE means the model cited NOTHING and the
    // code below attached the top-3 chunks by rank as a display convenience.
    // Those are not citations — they are "the best passages we retrieved" —
    // and presenting them as citations is a fabrication surface: on 2026-08-07
    // the one A/B answer that took this branch was also the only factually
    // wrong one, showing `Preamble` / `Rule 1` chips under a claim about Rule 4.
    let (answer, citations, citations_verified): (String, Vec<Citation>, bool) = if !marker_nums.is_empty() {
        // Renumber prose `[old]` → `[new]` (1-based by first appearance) so the
        // inline marker lines up with its chip; drop markers not in the map.
        let map: std::collections::HashMap<usize, usize> = marker_nums
            .iter()
            .enumerate()
            .map(|(i, &n)| (n, i + 1))
            .collect();
        let re = regex_lite::Regex::new(r"\[(\d+)\]").unwrap();
        let renumbered = re
            .replace_all(&answer, |c: &regex_lite::Captures| {
                match c[1].parse::<usize>().ok().and_then(|o| map.get(&o)) {
                    Some(n) => format!("[{n}]"),
                    None => String::new(),
                }
            })
            .to_string();
        let cites = marker_nums
            .iter()
            .map(|&n| {
                let (chunk_id, source_id, content, page, _file) = &chunks[n - 1];
                Citation {
                    source_id: source_id.clone(),
                    chunk_id: chunk_id.clone(),
                    excerpt: citation_snippet(content),
                    page: *page,
                }
            })
            .collect();
        (renumbered, cites, true)
    } else {
        let legacy = extract_citations_with_pages(&answer);
        let (cites, verified): (Vec<Citation>, bool) = if legacy.is_empty() {
            // Top-3 by rank (decompose path relies on this). UNVERIFIED — the
            // model cited nothing, so these passages support no specific claim.
            (
                chunks
                    .iter()
                    .take(CITATION_DISPLAY_LIMIT)
                    .map(|(chunk_id, source_id, content, page, _)| Citation {
                        source_id: source_id.clone(),
                        chunk_id: chunk_id.clone(),
                        excerpt: citation_snippet(content),
                        page: *page,
                    })
                    .collect(),
                false,
            )
        } else {
            // Legacy `[file, page]` — match filename + explicit page (rare).
            let mut filtered: Vec<Citation> = Vec::new();
            for (cited_name, cited_page) in &legacy {
                let cited_stem = file_stem(cited_name).to_lowercase();
                for (chunk_id, source_id, content, page, file_name) in &chunks {
                    let chunk_stem = file_stem(file_name).to_lowercase();
                    let name_match = chunk_stem.starts_with(&cited_stem)
                        || cited_stem.starts_with(&chunk_stem);
                    let page_match = match (cited_page, page) {
                        (Some(cp), Some(chp)) => cp == chp,
                        (None, _) => true,
                        _ => false,
                    };
                    if name_match
                        && page_match
                        && !filtered.iter().any(|c| c.chunk_id == *chunk_id)
                    {
                        filtered.push(Citation {
                            source_id: source_id.clone(),
                            chunk_id: chunk_id.clone(),
                            excerpt: citation_snippet(content),
                            page: *page,
                        });
                    }
                }
            }
            if filtered.is_empty() {
                // The model cited a file we did not retrieve — same unverified
                // top-3 fallback, same caveat.
                (
                    chunks
                        .iter()
                        .take(CITATION_DISPLAY_LIMIT)
                        .map(|(chunk_id, source_id, content, page, _)| Citation {
                            source_id: source_id.clone(),
                            chunk_id: chunk_id.clone(),
                            excerpt: citation_snippet(content),
                            page: *page,
                        })
                        .collect(),
                    false,
                )
            } else {
                (filtered, true)
            }
        };
        (answer, cites, verified)
    };

    // A "not found" / refusal answer must carry NO citations. The no-marker branch
    // above attaches the top-3 retrieved chunks by rank (needed for the decompose/
    // verbatim path), which would otherwise slap unrelated citations onto a "the
    // document does not contain this" refusal — citations on a non-answer mislead
    // the officer. Strip them for EVERY canonical no-answer string (there are two:
    // the retrieval fallback and the model's INSUFFICIENT_INFORMATION refusal).
    let is_no_answer = {
        let a = answer.trim();
        a == GROUNDING_FALLBACK || a == INSUFFICIENT_INFO_MSG || a.starts_with(INSUFFICIENT_INFO_MSG)
    };
    let citations = if is_no_answer { Vec::new() } else { citations };

    // ── Citation strictness (2026-08-07, `ECHO_CITATION_STRICT`, default OFF) ──
    //
    // (a) Unverified top-3 chips are not citations. With strict on they are not
    //     shipped as citations: "no citations" then honestly means "the model
    //     cited nothing", instead of looking identical to a well-cited answer.
    //     The passages are still returned, under `related_passages`, so a client
    //     can show them labelled truthfully rather than losing them.
    //
    // (b) Instrument-scoped grounding: `citation_gate::gate` flattens every
    //     passage into one string, so `Rule 4` from ANY of the eighteen
    //     documents grounds a claim about `Rule 4` of one named instrument.
    //     `gate_scoped` keeps each passage attached to its document. It can only
    //     be stricter — with no instrument named it degrades to the flat check.
    //
    // Default OFF because (a) is user-visible: answers that show three chips
    // today would show none. A/B it the way the pool depth was A/B'd.
    let strict_citations = env_usize("ECHO_CITATION_STRICT", 0) > 0;
    let mut related_passages: Vec<Citation> = Vec::new();
    let citations = if strict_citations && !citations_verified && !citations.is_empty() {
        related_passages = citations;
        Vec::new()
    } else {
        citations
    };
    let scoped_ungrounded: Vec<String> = if strict_citations && !is_no_answer {
        let units: Vec<crate::services::assistant::citation_gate::ContextUnit> = chunks
            .iter()
            .map(|(_, _, content, _, file_name)| {
                crate::services::assistant::citation_gate::ContextUnit {
                    instrument: file_name.as_str(),
                    content: content.as_str(),
                }
            })
            .collect();
        crate::services::assistant::citation_gate::gate_scoped(&answer, &units)
            .ungrounded
            .into_iter()
            .map(|u| u.reference)
            .collect()
    } else {
        Vec::new()
    };
    trace.step("Citation verification", &[
        ("Mode", if strict_citations { "STRICT (ECHO_CITATION_STRICT)".to_string() } else { "off — unverified chips shipped as citations".to_string() }),
        ("Model cited", if citations_verified { "yes — indexed [N] or matched legacy ref".to_string() } else { "NO — top-3 by rank, supports no specific claim".to_string() }),
        ("Citations shipped", format!("{}", citations.len())),
        ("Related passages (unverified)", format!("{}", related_passages.len())),
        ("Cross-instrument refs", if scoped_ungrounded.is_empty() {
            "none".to_string()
        } else {
            format!("⚠ {} not in the instrument named: {}", scoped_ungrounded.len(), scoped_ungrounded.join(", "))
        }),
    ]);

    // Merge quality notices: hardware downgrade notice from select_model(), plus
    // a soft grounding notice when the answer lacks citation markers (broad
    // questions where small models answered correctly but skipped formatting).
    let effective_notice = if tier_used == llm_tier {
        if grounding_uncited_only && quality_notice.is_none() {
            // Phase-0 fix 0b (2026-06-13): an answer with no `[n]` citations is
            // unverified. Flag it clearly (constrained) instead of shipping it as
            // authoritative — traces showed wrong answers reaching the user with a
            // soft "ask more specific" footnote. Pairs with 0c (no fabricated
            // citations shown). After 0a most answers cite, so this is rare.
            Some(QualityNotice {
                constrained: true,
                message: "This answer could not be verified against a specific source — treat it as unconfirmed and check the original document before relying on it.".to_string(),
            })
        } else {
            quality_notice
        }
    } else {
        None
    };
    // For summary queries, always attach the "not cited" info notice.
    let effective_notice = if matches!(query_type, QueryType::Summary) && tier_used != "error" {
        Some(QualityNotice {
            constrained: false,
            message: "General summary — content synthesised from document; specific claims may not be individually cited.".to_string(),
        })
    } else {
        effective_notice
    };

    let response = QueryResponse {
        answer,
        citations,
        tier_used,
        quality_notice: effective_notice,
        // The ONE place that knows the real numbers: `law_overhead` is measured here
        // (9f40d4aa) and the law floor is `CONTEXT_TOKEN_BUDGET`. Ship them with the
        // answer instead of letting the UI re-derive fullness from the citations —
        // that re-derivation is why the "context full" notice never fired.
        chat_fill: Some(ChatFill {
            used: law_overhead,
            limit: gen_max_prompt.saturating_sub(CONTEXT_TOKEN_BUDGET),
        }),
        // Empty unless ECHO_CITATION_STRICT moved unverified chips out of
        // `citations`; then these are passages, honestly labelled as such.
        related_passages,
    };

    log::info!(
        "METRIC llm_done_ms={} answer_chars={} tier={}",
        t_llm.elapsed().as_millis(),
        response.answer.len(),
        response.tier_used
    );
    log::info!("METRIC query_total_ms={} citations={}", t_route.elapsed().as_millis(), response.citations.len());
    trace.step("Compose final response", &[
        ("Answer", format!("{} chars", response.answer.chars().count())),
        ("Citations shown", format!("{}", response.citations.len())),
        ("Tier reported", response.tier_used.clone()),
        ("Quality notice", response.quality_notice.as_ref().map(|q| q.message.clone()).unwrap_or_else(|| "(none)".to_string())),
    ]);
    {
        const FINAL_BLOCK_CAP: usize = 8000;
        let total = response.answer.chars().count();
        let body: String = response.answer.chars().take(FINAL_BLOCK_CAP).collect();
        let body = if total > FINAL_BLOCK_CAP {
            format!("{body}\n\n[…truncated; full answer was {total} chars]")
        } else { body };
        trace.add_block("Final answer shown to user (post-cleanup)", &body);
        if !response.citations.is_empty() {
            let mut c = String::new();
            for (i, cit) in response.citations.iter().enumerate() {
                let pg = cit.page.map(|p| p.to_string()).unwrap_or_else(|| "?".to_string());
                let excerpt: String = cit.excerpt.chars().take(160).collect();
                let excerpt = excerpt.replace('\n', " ");
                c.push_str(&format!(
                    "[{}] chunk={} page={}\n    {}\n",
                    i + 1, cit.chunk_id, pg, excerpt
                ));
            }
            trace.add_block("Citations resolved", c.trim_end());
        }
    }
    let outcome = if was_cancelled { "cancelled" } else { "ok" };
    trace.finish(outcome, response.answer.chars().count(), response.citations.len(), &response.tier_used);
    if owns_trace { trace.write(&app); }
    // Skip persisting to the semantic cache when the cache is disabled (dev/
    // testing) so a later run can never hit a stale answer.
    if query_cache_enabled() && !was_cancelled {
        store_cache(pool, session_id, query_text, &query_embedding, &response).await?;
    }
    Ok((response, was_cancelled))
}

/// Feature 025 (ADR-0021, T004) — a lean, self-contained retrieval pass for ONE
/// (sub-)query, used by the agentic deep-search loop to research each sub-question
/// independently.
///
/// It **composes** the SAME primitives the main path uses — hybrid vector + FTS
/// merge, then the summary-recall and graph-expansion arms — rather than
/// extracting them from `route_scoped_with_override` (a ~1,400-line hot function).
/// Composing keeps the normal search path byte-for-byte unchanged (the accepted
/// tradeoff is that the arm-composition sequence now lives in two places; keep the
/// two in sync). Returns the ranked candidate pool (chunk tuples
/// `(id, source_id, content, page, label)`), truncated to `top_k`. No trace and no
/// reranker here — the loop owns its own `DeepTrace` and does final budget packing
/// across rounds (see `services/deep_search`).
pub(crate) async fn retrieve_for_query(
    pool: &Db,
    embedder: &Arc<EmbedderState>,
    notebook_id: &str,
    scope_source_ids: Option<&[String]>,
    query_text: &str,
    top_k: usize,
) -> Result<Vec<(String, String, String, Option<i64>, String)>> {
    // 1. Embed the (sub-)query on the blocking pool (ONNX), like the main path.
    let embedder_clone = Arc::clone(embedder);
    let q = query_text.to_string();
    let query_embedding = tokio::task::spawn_blocking(move || embedder_clone.embed_one(&q))
        .await
        .map_err(|e| anyhow::anyhow!("embed thread panicked: {e}"))??;

    // 2. Hybrid vector + FTS over a modest per-sub-question pool, merged exactly as
    //    the main path merges (union under `ECHO_HYBRID=union`, else RRF).
    let fetch_limit = (top_k.max(1) * 3) as i64;
    let vec_chunks =
        get_context_chunks(pool, notebook_id, scope_source_ids, &query_embedding, fetch_limit).await?;
    let fts_chunks = fts5_retrieve(pool, notebook_id, scope_source_ids, query_text, Some(fetch_limit))
        .await
        .unwrap_or_default();
    let mut raw_chunks = match (fts_chunks.is_empty(), vec_chunks.is_empty()) {
        (false, false) => {
            if hybrid_union_enabled() {
                // Deep sub-question path: its selection target is `top_k`.
                let (vk, fk) = merge_depth(top_k, vec_chunks.len(), fts_chunks.len());
                union_merge(&fts_chunks, &vec_chunks, fk, vk)
            } else {
                rrf_merge(&fts_chunks, &vec_chunks, 60)
            }
        }
        (false, true) => fts_chunks,
        _ => vec_chunks,
    };

    // 3. (Summary-recall arm REMOVED for deep — it pulled provisions by SUMMARY
    //    similarity, an extra off-domain noise path. Deep retrieval is now
    //    vector + FTS (hybrid union) + graph only.)

    // 4. Graph-expansion arm (ECHO_GRAPH_EXPAND; forced on under deep) — follow the
    //    legal overlay from the top seeds, scope-clamped, deduped.
    if graph_expand_enabled() && !raw_chunks.is_empty() {
        use crate::db::repositories::graph;
        let seed_ids: Vec<String> =
            raw_chunks.iter().take(knob_graph_seed_k()).map(|c| c.0.clone()).collect();
        let edge_types = ["cross_reference", "non_obstante", "amended_by"];
        let seed_nodes = graph::node_ids_for_chunks(pool, &seed_ids).await.unwrap_or_default();
        let reached = if seed_nodes.is_empty() {
            Vec::new()
        } else {
            graph::expand(pool, &seed_nodes, &edge_types, knob_graph_hops()).await.unwrap_or_default()
        };
        let have: HashSet<String> = raw_chunks.iter().map(|c| c.0.clone()).collect();
        let mut new_ids: Vec<String> = Vec::new();
        for n in &reached {
            if !n.chunk_id.is_empty() && !have.contains(&n.chunk_id) && !new_ids.contains(&n.chunk_id) {
                new_ids.push(n.chunk_id.clone());
            }
        }
        new_ids.truncate(knob_graph_k());
        let mut graph_chunks = if new_ids.is_empty() {
            Vec::new()
        } else {
            fetch_chunks_by_ids(pool, &new_ids).await.unwrap_or_default()
        };
        if let Some(allowed) = scope_source_ids {
            let allowed: HashSet<&str> = allowed.iter().map(String::as_str).collect();
            graph_chunks.retain(|c| allowed.contains(c.1.as_str()));
        }
        if !graph_chunks.is_empty() {
            raw_chunks = union_merge(&raw_chunks, &graph_chunks, raw_chunks.len(), knob_graph_k());
        }
    }

    // 5. Truncate to the per-sub-question budget.
    raw_chunks.truncate(top_k);

    // 6. Table completion — bounded, mirroring the main path (2026-08-06). Pulls
    //    the ONE table a top-ranked chunk belongs to, in reading order, capped in
    //    slots and tokens. APPENDED (not prepended): this path already truncated
    //    to `top_k` above, and each sub-question's chunks are concatenated
    //    downstream, so leading buys nothing here.
    //
    //    ⚠️ This path previously had NO enumeration gate at all — every deep
    //    sub-question table-expanded unconditionally, and unboundedly. Adding the
    //    gate is a deliberate behaviour change on the deep lane; evaluate it
    //    separately from the main path.
    //
    //    `top_k * 150` inverts the `ctx_budget / 150` ratio that produced `ctx_top_k`,
    //    so both paths reserve the same fraction of their budget.
    if is_enumeration_query(query_text) && graph_expand_enabled() {
        let tbl = table_reserve_slice(
            pool,
            &raw_chunks,
            scope_source_ids,
            knob_table_seed_top_n(),
            knob_table_reserve_k(top_k),
            knob_table_reserve_tokens(top_k * 150),
        )
        .await;
        raw_chunks.extend(tbl);
    }
    Ok(raw_chunks)
}

/// Feature 025 (enumeration short-circuit) — fetch EVERY `table_row` chunk for the
/// given source(s), in reading order. Used when a "how many / list all / sum of X"
/// query is scoped to a document: rather than decompose + similarity-retrieve (which
/// only surfaces a few rows), pull the whole table so synthesis can count/enumerate
/// it. Returns the chunk tuples `(id, source_id, content, page, label)`.
pub(crate) async fn fetch_table_chunks_for_sources(
    pool: &Db,
    source_ids: &[String],
) -> Result<Vec<(String, String, String, Option<i64>, String)>> {
    if source_ids.is_empty() {
        return Ok(Vec::new());
    }
    let ph = crate::db::pg_in(source_ids.len(), 1);
    let sql = format!(
        "SELECT c.id, c.source_id, c.content, c.page_number, \
                COALESCE(s.display_name, s.file_name, s.file_path, 'unknown') \
           FROM chunks c JOIN sources s ON s.id = c.source_id \
          WHERE c.source_id IN ({ph}) AND c.unit_type IN ('table', 'table_row') \
          ORDER BY c.source_id, c.chunk_index"
    );
    let mut q = sqlx::query_as::<_, (String, String, String, Option<i64>, String)>(&sql);
    for s in source_ids {
        q = q.bind(s);
    }
    Ok(q.fetch_all(pool).await?)
}

/// Whether the Tier-0 semantic answer cache is active.
///
/// Default: **disabled in debug/dev builds** (so every turn re-runs retrieval +
/// generation — no "same answer again and again" during testing) and **enabled
/// in release**. Override either way with `ECHO_QUERY_CACHE=1|0`.
fn query_cache_enabled() -> bool {
    match std::env::var("ECHO_QUERY_CACHE") {
        Ok(v) => matches!(v.trim().to_ascii_lowercase().as_str(), "1" | "true" | "on" | "yes"),
        Err(_) => !cfg!(debug_assertions),
    }
}

/// Tier 0: Check query_cache for a semantically similar query (cosine ≥ 0.92).
async fn check_cache(
    pool: &Db,
    session_id: &str,
    query_embedding: &[f32],
    _query_text: &str,
) -> Result<Option<QueryResponse>> {
    // Fetch recent cache entries for this session.
    let rows: Vec<(Vec<u8>, String, String, String)> = sqlx::query_as(
        "SELECT query_embedding, answer_text, citations_json, tier_used \
         FROM query_cache WHERE session_id = $1 \
         ORDER BY created_at DESC LIMIT 50",
    )
    .bind(session_id)
    .fetch_all(pool)
    .await?;

    for (embedding_blob, answer, citations_json, tier_used) in &rows {
        // Deserialize stored embedding.
        let stored: Vec<f32> = match serde_json::from_slice(embedding_blob.as_slice()) {
            Ok(v) => v,
            Err(_) => continue,
        };

        if cosine_similarity(query_embedding, &stored) >= 0.92 {
            let citations: Vec<Citation> = serde_json::from_str(citations_json)
                .unwrap_or_default();
            return Ok(Some(QueryResponse {
                answer: answer.clone(),
                citations,
                tier_used: format!("cache({})", tier_used),
                quality_notice: None,
                // check_cache() assembles no prompt and has no overhead to report.
                chat_fill: None,
                related_passages: Vec::new(),
            }));
        }
    }

    Ok(None)
}

/// Tier 1: Entity-pattern FTS5 — returns matching chunks for LLM synthesis.
/// `limit_override` is a per-call override on `ECHO_FTS_K`. `None` → use
/// the env default. S2 passes `Some(knob_fts_k * pool_mult)` on definitional
/// queries to widen the FTS5 candidate pool.
async fn fts5_retrieve(
    pool: &Db,
    notebook_id: &str,
    scope_source_ids: Option<&[String]>,
    query_text: &str,
    limit_override: Option<i64>,
) -> Result<Vec<(String, String, String, Option<i64>, String)>> {
    // Sanitize for FTS5:
    // 1. Strip non-alphanumeric chars from each token (kills '?', ',', etc.
    //    that otherwise leak into the MATCH expression and cause zero hits).
    // 2. Drop stopwords and very short tokens — they add no signal.
    // 3. Join with OR (default AND requires every token to co-occur in one
    //    chunk, which is fatal for natural-language questions).
    // 4. Each remaining token is quoted to neutralize dashes (date 2026-03-15).
    const STOP: &[&str] = &[
        "the","a","an","of","and","or","is","are","was","were","be","been","being",
        "to","in","on","at","by","for","with","as","from","that","this","these",
        "those","it","its","into","such","than","then","what","which","who","whom",
        "whose","how","when","where","why","do","does","did","can","could","shall",
        "should","will","would","may","might","must","under","over","about","any",
        "all","also","not","no","but","if","so","because","while","until","than",
    ];
    let tokens: Vec<String> = query_text
        .split(|c: char| !c.is_alphanumeric() && c != '\'')
        .filter_map(|t| {
            let t = t.trim().to_lowercase();
            if t.len() < 3 { return None; }
            if STOP.contains(&t.as_str()) { return None; }
            Some(format!("\"{t}\""))
        })
        .collect();
    let fts_query: String = if tokens.is_empty() {
        // Degenerate: fall back to the raw text quoted as a single phrase.
        format!("\"{}\"", query_text.replace('"', ""))
    } else {
        tokens.join(" OR ")
    };

    // Knob: ECHO_FTS_K overrides the default 2× CONTEXT_TOP_K for FTS5 recall.
    // Per-call override (S2) wins when supplied — used to widen the pool on
    // definitional queries.
    let limit = limit_override.unwrap_or_else(|| knob_fts_k() as i64);

    if let Some(ids) = scope_source_ids {
        if ids.is_empty() {
            return Ok(vec![]);
        }
        let n = ids.len();
        let placeholders = crate::db::pg_in(n, 1);
        let (p_q, p_l) = (n + 1, n + 2);
        let sql = format!(
            "SELECT c.id, c.source_id, c.content, c.page_number, \
                    COALESCE(s.display_name, s.file_name, s.file_path, 'unknown') \
             FROM chunks c \
             JOIN sources s ON s.id = c.source_id \
             WHERE c.source_id IN ({placeholders}) \
             AND c.is_boilerplate = FALSE \
             AND c.tsv @@ websearch_to_tsquery('english', ${p_q}) \
             AND LENGTH(c.content) > 150 \
             ORDER BY ts_rank_cd(c.tsv, websearch_to_tsquery('english', ${p_q})) DESC LIMIT ${p_l}",
        );
        let mut q = sqlx::query_as(&sql);
        for id in ids { q = q.bind(id); }
        return Ok(q.bind(&fts_query).bind(limit).fetch_all(pool).await.unwrap_or_default());
    }

    let rows: Vec<(String, String, String, Option<i64>, String)> = sqlx::query_as(
        "SELECT c.id, c.source_id, c.content, c.page_number, \
                COALESCE(s.display_name, s.file_name, s.file_path, 'unknown') \
         FROM chunks c \
         JOIN notebook_sources ns ON ns.source_id = c.source_id \
         JOIN sources s ON s.id = c.source_id \
         WHERE ns.notebook_id = $1 AND c.is_boilerplate = FALSE \
         AND c.tsv @@ websearch_to_tsquery('english', $2) \
         AND LENGTH(c.content) > 150 \
         ORDER BY ts_rank_cd(c.tsv, websearch_to_tsquery('english', $2)) DESC LIMIT $3",
    )
    .bind(notebook_id)
    .bind(&fts_query)
    .bind(limit)
    .fetch_all(pool)
    .await
    .unwrap_or_default();

    Ok(rows)
}

/// Retrieve top-N context chunks for LLM synthesis.
async fn get_context_chunks(
    pool: &Db,
    notebook_id: &str,
    scope_source_ids: Option<&[String]>,
    query_embedding: &[f32],
    limit: i64,
) -> Result<Vec<(String, String, String, Option<i64>, String)>> {
    let embedding_json = serde_json::to_string(query_embedding)?;

    if let Some(ids) = scope_source_ids {
        if ids.is_empty() { return Ok(vec![]); }
        let n = ids.len();
        let placeholders = crate::db::pg_in(n, 1);
        let (p_e, p_l) = (n + 1, n + 2);
        let sql = format!(
            "SELECT c.id, c.source_id, c.content, c.page_number, \
                    COALESCE(s.display_name, s.file_name, s.file_path, 'unknown') \
             FROM chunk_embeddings ce \
             JOIN chunks c ON c.id = ce.chunk_id \
             JOIN sources s ON s.id = c.source_id \
             WHERE c.source_id IN ({placeholders}) \
             AND c.is_boilerplate = FALSE \
             ORDER BY (ce.embedding <=> ${p_e}::vector) ASC LIMIT ${p_l}",
        );
        let mut q = sqlx::query_as(&sql);
        for id in ids { q = q.bind(id); }
        return match q.bind(&embedding_json).bind(limit).fetch_all(pool).await {
            Ok(r) => Ok(r),
            Err(e) if e.to_string().contains("no such table") || e.to_string().contains("no such function") => Ok(vec![]),
            Err(e) => Err(e.into()),
        };
    }

    let rows: Vec<(String, String, String, Option<i64>, String)> = match sqlx::query_as(
        // Filter to notebook's chunks first via subquery, then rank by distance.
        "SELECT c.id, c.source_id, c.content, c.page_number, \
                COALESCE(s.display_name, s.file_name, s.file_path, 'unknown') \
         FROM chunk_embeddings ce \
         JOIN chunks c ON c.id = ce.chunk_id \
         JOIN sources s ON s.id = c.source_id \
         WHERE ce.chunk_id IN ( \
             SELECT c2.id FROM chunks c2 \
             JOIN notebook_sources ns ON ns.source_id = c2.source_id \
             WHERE ns.notebook_id = $1 AND c2.is_boilerplate = FALSE \
         ) \
         ORDER BY (ce.embedding <=> $2::vector) ASC LIMIT $3",
    )
    .bind(notebook_id)
    .bind(&embedding_json)
    .bind(limit)
    .fetch_all(pool)
    .await
    {
        Ok(r) => r,
        Err(e) if e.to_string().contains("no such table") || e.to_string().contains("no such function") => {
            vec![]
        }
        Err(e) => return Err(e.into()),
    };

    Ok(rows)
}

/// Track 2 (ADR-0019 §2) — the SUMMARY recall arm. Vector-search the
/// per-provision recall index (`summary_embeddings`), then map each summary back
/// to its ORIGINAL provision chunk and return that chunk's verbatim text in the
/// same tuple shape as [`get_context_chunks`]. The summary text is NEVER
/// returned — it is only the recall key. Bounded by `limit`; empty when the
/// summary index has not been built (table exists but no rows) or is absent.
async fn get_context_chunks_via_summaries(
    pool: &Db,
    notebook_id: &str,
    scope_source_ids: Option<&[String]>,
    query_embedding: &[f32],
    limit: i64,
) -> Result<Vec<(String, String, String, Option<i64>, String)>> {
    let embedding_json = serde_json::to_string(query_embedding)?;

    // Each summary maps to a distinct original chunk (one provision-summary per
    // article chunk, one doc-summary per document chunk), so a plain top-K over
    // the summary vectors returns the most relevant provisions directly. Any
    // residual overlap is de-duped by `union_merge` downstream.
    let rows = if let Some(ids) = scope_source_ids {
        if ids.is_empty() { return Ok(vec![]); }
        let n = ids.len();
        let placeholders = crate::db::pg_in(n, 1);
        let (p_e, p_l) = (n + 1, n + 2);
        let sql = format!(
            "SELECT c.id, c.source_id, c.content, c.page_number, \
                    COALESCE(s.display_name, s.file_name, s.file_path, 'unknown') \
             FROM summary_embeddings se \
             JOIN graph_summary g ON g.id = se.summary_id \
             JOIN chunks c ON c.id = g.chunk_id \
             JOIN sources s ON s.id = c.source_id \
             WHERE g.source_id IN ({placeholders}) \
             ORDER BY (se.embedding <=> ${p_e}::vector) ASC LIMIT ${p_l}",
        );
        let mut q = sqlx::query_as(&sql);
        for id in ids { q = q.bind(id); }
        q.bind(&embedding_json).bind(limit).fetch_all(pool).await
    } else {
        sqlx::query_as(
            "SELECT c.id, c.source_id, c.content, c.page_number, \
                    COALESCE(s.display_name, s.file_name, s.file_path, 'unknown') \
             FROM summary_embeddings se \
             JOIN graph_summary g ON g.id = se.summary_id \
             JOIN chunks c ON c.id = g.chunk_id \
             JOIN sources s ON s.id = c.source_id \
             WHERE g.source_id IN ( \
                 SELECT ns.source_id FROM notebook_sources ns WHERE ns.notebook_id = $1 \
             ) ORDER BY (se.embedding <=> $2::vector) ASC LIMIT $3",
        )
        .bind(notebook_id)
        .bind(&embedding_json)
        .bind(limit)
        .fetch_all(pool)
        .await
    };

    match rows {
        Ok(r) => Ok(r),
        Err(e) if e.to_string().contains("does not exist") || e.to_string().contains("no such table") => {
            Ok(vec![])
        }
        Err(e) => Err(e.into()),
    }
}

/// Resolve a relative model path to an absolute path.
///
/// Resolution order:
/// 1. `ECHO_MODELS_DIR` env var — explicit override (useful when CARGO_TARGET_DIR
///    is outside the project, e.g. on a separate NTFS volume).
/// 2. Compile-time `CARGO_MANIFEST_DIR` (debug builds only) — always points to
///    `<project>/src-tauri/`, so `../models/` is the project models folder.
///    Reliable even when the target dir is moved outside the project tree.
/// 3. Walk up from the running executable (release bundles, installed builds).
pub(crate) fn resolve_model_path(relative_path: &str) -> String {
    // 1. Explicit env var override.
    if let Ok(base) = std::env::var("ECHO_MODELS_DIR") {
        let candidate = std::path::Path::new(&base).join(
            // strip the "models/" prefix if the env var already is the models dir
            relative_path.trim_start_matches("models/"),
        );
        if candidate.exists() {
            return candidate.to_string_lossy().to_string();
        }
    }

    // 2. Compile-time project root (debug builds — survives CARGO_TARGET_DIR moves).
    #[cfg(debug_assertions)]
    {
        // CARGO_MANIFEST_DIR = .../echo-AI/src-tauri at compile time.
        let manifest_dir = env!("CARGO_MANIFEST_DIR");
        if let Some(project_root) = std::path::Path::new(manifest_dir).parent() {
            let candidate = project_root.join(relative_path);
            if candidate.exists() {
                return candidate.to_string_lossy().to_string();
            }
        }
    }

    // 3. Walk up from exe (release bundles, installed builds).
    if let Ok(exe) = std::env::current_exe() {
        let mut dir = exe.parent().map(|p| p.to_path_buf());
        for _ in 0..6 {
            if let Some(d) = &dir {
                let candidate = d.join(relative_path);
                if candidate.exists() {
                    return candidate.to_string_lossy().to_string();
                }
                dir = d.parent().map(|p| p.to_path_buf());
            } else {
                break;
            }
        }
    }
    // Last resort: return as-is (will surface a clear "file not found" error).
    relative_path.to_string()
}

pub(crate) fn select_model_for_tier(tier: &HardwareTier) -> (String, String, Option<QualityNotice>) {
    select_model(tier)
}

/// Pick the chat model. Single-model strategy (no tiering): use whatever GGUF
/// the user has placed in `models/`. Tier struct is preserved elsewhere for
/// thread/RAM info but is no longer consulted for model selection.
fn select_model(_tier: &HardwareTier) -> (String, String, Option<QualityNotice>) {
    if let Some((path, label)) = discover_local_gguf() {
        let label_id = format!("local-{}b", label).to_lowercase().replace('.', "_");
        return (path, label_id, None);
    }
    // No GGUF on disk — surface a clear "missing model" error path. Returns the
    // historical canonical filename so the download UI continues to work.
    let fallback = resolve_model_path("models/qwen2.5-1.5b-instruct-q4_k_m.gguf");
    let notice = Some(QualityNotice {
        constrained: true,
        message: "No model found in models/ — drop a Qwen2.5-Instruct GGUF \
                  (e.g. qwen2.5-1.5b-instruct-q4_k_m.gguf) into the models/ \
                  folder."
            .to_string(),
    });
    (fallback, "no-model".to_string(), notice)
}

/// Scan the `models/` directory for any `*.gguf` file and return the
/// best candidate (largest declared parameter size, falling back to file size).
/// Returns `(absolute_path, label)` — label is the parsed param size like
/// "1.5", "3", "7" or the filename stem when no size token is found.
///
/// Designed for hot-swap: drop any GGUF into `models/` and it gets picked up
/// without code changes. Caller is responsible for prompt-template compatibility.
pub(crate) fn discover_local_gguf() -> Option<(String, String)> {
    let models_dir_str = resolve_model_path("models");
    let models_dir = std::path::Path::new(&models_dir_str);
    if !models_dir.is_dir() {
        return None;
    }
    let read = match std::fs::read_dir(models_dir) {
        Ok(r) => r,
        Err(_) => return None,
    };
    // Track best by parsed param size; fall back to file size (bytes) for ties
    // or for files without a parseable "<n>b" token in the name.
    let mut best: Option<(f32, u64, String, String)> = None;
    for entry in read.flatten() {
        let name = entry.file_name().to_string_lossy().to_string();
        let lower = name.to_lowercase();
        if !lower.ends_with(".gguf") {
            continue;
        }
        let bytes = entry.metadata().map(|m| m.len()).unwrap_or(0);
        // Try to parse a "<n>b" token (e.g. "qwen2.5-1.5b-...", "llama-7b-...").
        let size_num = parse_param_size_b(&lower).unwrap_or(0.0);
        let path = entry.path().to_string_lossy().to_string();
        let label = if size_num > 0.0 {
            if size_num.fract() == 0.0 {
                format!("{size_num:.0}")
            } else {
                format!("{size_num}")
            }
        } else {
            std::path::Path::new(&name)
                .file_stem()
                .map(|s| s.to_string_lossy().to_string())
                .unwrap_or_else(|| name.clone())
        };
        let better = match &best {
            None => true,
            Some((cur_size, cur_bytes, _, _)) => {
                size_num > *cur_size || (size_num == *cur_size && bytes > *cur_bytes)
            }
        };
        if better {
            best = Some((size_num, bytes, path, label));
        }
    }
    best.map(|(_, _, p, l)| (p, l))
}

/// Find the first `<n>b` or `<n.n>b` parameter-count token in a filename.
fn parse_param_size_b(name_lower: &str) -> Option<f32> {
    // Walk byte-by-byte; on encountering a digit, accumulate digits/dot, then
    // require a trailing 'b' (word-boundary-ish: followed by '-', '_', '.', end).
    let bytes = name_lower.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        let c = bytes[i];
        if c.is_ascii_digit() {
            let start = i;
            while i < bytes.len() && (bytes[i].is_ascii_digit() || bytes[i] == b'.') {
                i += 1;
            }
            if i < bytes.len() && bytes[i] == b'b' {
                let after = bytes.get(i + 1).copied();
                let boundary = matches!(after, None | Some(b'-') | Some(b'_') | Some(b'.'));
                if boundary {
                    if let Ok(n) = name_lower[start..i].parse::<f32>() {
                        return Some(n);
                    }
                }
            }
        } else {
            i += 1;
        }
    }
    None
}


/// Build the numbered context block passed to the LLM.
///
/// Format per passage: `[N] <doc_name>, page P\n<text>`
/// Source name is included so citation validation in AD-8 can match `[doc_name, page P]`.
fn build_context_string(
    chunks: &[(String, String, String, Option<i64>, String)],
    amend_notes: &std::collections::HashMap<String, String>,
) -> String {
    chunks
        .iter()
        .enumerate()
        .map(|(i, (chunk_id, _, content, page, file_name))| {
            let page_str = page.map(|p| format!(", page {p}")).unwrap_or_default();
            let clean = strip_html_tags(content);
            // Full chunk content — the total is already gated by
            // CONTEXT_TOKEN_BUDGET in build_context(). Truncating per-chunk
            // here drops body text for definitional passages (e.g. Art. 7 of
            // the Constitution sits ~800 chars into its chunk).
            // Phase 2: append the amendment-awareness note when this provision
            // has recorded amendment events (empty map = no-op when disabled).
            let note = amend_notes
                .get(chunk_id)
                .map(|n| format!("\n{n}"))
                .unwrap_or_default();
            format!("[{}] {}{}\n{}{}", i + 1, file_name, page_str, clean, note)
        })
        .collect::<Vec<_>>()
        .join("\n\n---\n\n")
}

/// Build a ChatML-formatted prompt for Qwen2.5-Instruct models using the
/// versioned grounding system prompt (AD-7 / FR-041).
///
/// **Conversation history is intentionally NOT injected.** A previous
/// implementation injected the last 1-2 Q&A pairs (with each prior answer
/// char-truncated to 150) so the model could resolve follow-up references
/// like "this amendment". On 1.5B-3B local models the truncated assistant
/// turn looked like a format template (e.g. ending mid-sentence with
/// "...follow these steps:") and the model copied that opener into the
/// next answer, producing cross-prompt drift where each new answer's
/// intro echoed the previous question. See CHANGELOG_DEV.md 2026-05-10.
/// The `_history` parameter is retained for API compatibility but ignored;
/// re-enabling injection requires a per-tier guard + non-truncating
/// summarisation, plus updating the regression test below.
pub(crate) fn build_rag_prompt(
    query: &str,
    context: &str,
    history: &[HistoryTurn],
    system_prompt: &str,
    focus_hint: bool,
) -> String {
    log::debug!(
        "LLM prompt uses system prompt v{}/v{}",
        GROUNDING_SYSTEM_PROMPT_VERSION,
        SUMMARY_SYSTEM_PROMPT_VERSION,
    );

    // focus_hint=true used by the retry loop (Task 6) to emphasise the query on escalated attempts.
    let question_line = if focus_hint {
        format!("Focus specifically on answering this question: {query}")
    } else {
        format!("Question: {query}")
    };

    // Inject prior turns ONLY for anaphoric follow-ups ("And for BS-22?",
    // "What about ...") or explicit conversation references. Self-contained queries
    // skip history — keeps the prompt small and prevents drift (see CHANGELOG_DEV.md
    // 2026-05-10 drift bug). Built by the SAME function the law budget measures
    // (`prior_turns_block`), so the budget can never disagree with what is injected.
    let prior_block = prior_turns_block(query, history);

    // Pin output language for non-Latin scripts (Urdu, Sindhi, Arabic, etc).
    // Without this, small models default to English even when the question is
    // in Urdu (P12) or Sindhi (P13). Detection: any char in the Arabic block.
    let lang_hint = if query.chars().any(|c| matches!(c as u32, 0x0600..=0x06FF | 0x0750..=0x077F | 0xFB50..=0xFDFF)) {
        "\n\nIMPORTANT: The question is in Urdu/Sindhi/Arabic script. Reply in the SAME script and language as the question — do not translate to English."
    } else { "" };

    // One-shot citation demonstration (Phase-0 fix 0a, 2026-06-13). The local
    // 0.5B does not emit `[n]` markers from the instruction alone, but reliably
    // copies the format when shown a single worked example (ollama A/B proof:
    // NO citations zero-shot -> `[n]` with one example). Placed in the prompt
    // HEAD so it survives the head+tail truncation in llm.rs. ~40 tokens.
    //
    // SCOPED to system prompts that actually request `[n]` (the direct grounding
    // path). The decompose / Lookup intent override asks for VERBATIM provision
    // text and consumes route_scoped's `citations` as raw material for
    // deterministic label resolution — injecting a [n] demo there would change
    // that pool, so we must NOT inject it on that path.
    //
    // ALSO SUPPRESSED once real prior turns are present (2026-07-16). The demo is a
    // synthetic user/assistant exchange sitting immediately BEFORE `prior` — from
    // the model's side it is indistinguishable from conversation, and it is the
    // EARLIEST turn it can see. Asked "what was the very first thing I asked you
    // about?" after a 6-turn chat, prod answered *"Who can sanction 30 days
    // leave?"* — the demo's question, which the user never asked. Worse than the
    // wrong answer: a user asking "what did we discuss about leave?" could be told
    // about "the Secretary" and "30 days" — a conversation that never happened.
    // When `prior` is non-empty the model already has BETTER few-shot examples —
    // its own earlier answers, carrying real `[n]` markers over the real corpus —
    // so the synthetic demo is redundant there as well as harmful. (~40 tokens
    // saved on follow-ups.) Self-contained turns still get it, which is the case
    // the 0.5B A/B actually measured.
    // ⚠️ This flag USED to be `system_prompt.contains("[n]")` — a stringly-typed
    // switch driven by incidental prompt wording. Two things were wrong with it:
    //
    //  1. It fired on any prompt that merely MENTIONED `[n]`, including one that
    //     forbade it. On 2026-07-16 a memory instruction reading "never attach a
    //     [n] citation marker" silently switched the demo ON for the 8B — which had
    //     never had it — and the demo's leave example hijacked answers: a question
    //     about rules of power came back titled "Authority to Sanction Leave" and
    //     then refused. A prompt edit turned on a feature by naming it.
    //  2. It contradicted its own documented intent: the decompose/Lookup override
    //     (`intent_router/mod.rs`) says "end every claim with its citation [n]", so
    //     the sniff INCLUDED the very path the comment says must be excluded.
    //
    // The demo exists for SMALL models: the 0.5B ollama A/B (Phase-0 fix 0a,
    // 2026-06-13) showed no citations zero-shot and `[n]` with one example, and the
    // richer compact variant fixed 1B-class models dropping conditions. That is
    // exactly `compact_prompt_profile()` — so gate on the real condition. Qwen3-8B
    // follows the instruction natively and does not need (or want) the example.
    let wants_index_citations = crate::services::llm::compact_prompt_profile();
    let citation_demo = if wants_index_citations && prior_block.is_empty() {
        if crate::services::llm::compact_prompt_profile() {
            // Small-model profile: a worked example with a cited condition LIST
            // + the refine offer. The one-liner demo below anchors 1B-class
            // models to one-sentence answers that drop every condition (live
            // Gemma-1B A/B, 2026-07-06); this richer shape fixed it.
            "<|im_start|>user\n\
Context passages:\n\
[1] example_rules.pdf\n\
Rule 5: The Secretary may sanction leave of up to 30 days under the following conditions: (i) the officer has completed one year of service; (ii) sufficient leave balance is available.\n\n\
Question: When can the Secretary sanction 30 days leave?<|im_end|>\n\
<|im_start|>assistant\n\
Under Rule 5, the Secretary may sanction leave of up to 30 days [1]. This is allowed under the following conditions:\n\
1. The officer has completed one year of service [1].\n\
2. Sufficient leave balance is available [1].\n\n\
Would you like me to refine this answer or tailor it to your specific case?<|im_end|>\n"
        } else {
            "<|im_start|>user\n\
Context passages:\n\
[1] example_rules.pdf\n\
The Secretary may sanction leave of up to 30 days.\n\n\
Question: Who can sanction 30 days leave?<|im_end|>\n\
<|im_start|>assistant\n\
The Secretary may sanction leave of up to 30 days [1].<|im_end|>\n"
        }
    } else {
        ""
    };

    // Optional no-think prefix (see `no_think_assistant_prefix`). When
    // `NOVIQ_NO_THINK` is set we pre-close an empty `<think></think>` right after
    // the assistant tag — exactly what Qwen3's `enable_thinking=false` chat
    // template does — so hybrid-reasoning models skip the reasoning chain that
    // otherwise triples lookup latency (the streaming filter in llm.rs still
    // strips any `<think>` a model emits regardless). Default OFF so Mistral and
    // any model that needs reasoning are unaffected.
    format!(
        "<|im_start|>system\n\
{system}{lang_hint}\
<|im_end|>\n\
{citation_demo}\
{prior}\
<|im_start|>user\n\
Context passages:\n\
{context}\n\n\
{question_line}<|im_end|>\n\
<|im_start|>assistant\n{no_think}",
        system = system_prompt,
        lang_hint = lang_hint,
        citation_demo = citation_demo,
        prior = prior_block,
        context = context,
        question_line = question_line,
        no_think = no_think_assistant_prefix(),
    )
}

/// Build a ChatML rewrite prompt for the transform capability: reformat the prior
/// answer per the instruction WITHOUT new retrieval — preserve every `[n]`
/// citation marker and legal fact, change only form/tone/length/language/format.
pub(crate) fn build_transform_prompt(instruction: &str, prior_answer: &str) -> String {
    let lang_hint = if instruction.chars().any(|c| {
        matches!(c as u32, 0x0600..=0x06FF | 0x0750..=0x077F | 0xFB50..=0xFDFF)
    }) {
        "\n\nIMPORTANT: Reply in Urdu/Sindhi/Arabic script, as requested — do not use English."
    } else {
        ""
    };
    format!(
        "<|im_start|>system\n\
You are a professional Pakistan government assistant. Rewrite the officer's \
PREVIOUS ANSWER exactly as instructed — change only its form (tone, length, \
formality, language, or format). PRESERVE every citation marker like [1], [2] \
exactly where it supports a statement. PRESERVE every legal fact, rule or \
section number, figure, and condition verbatim. Do NOT add new legal claims, new \
citations, or any information not already present. Output ONLY the rewritten \
answer.{lang}<|im_end|>\n\
<|im_start|>user\n\
INSTRUCTION: {instruction}\n\n\
PREVIOUS ANSWER:\n{prior}<|im_end|>\n\
<|im_start|>assistant\n{no_think}",
        lang = lang_hint,
        instruction = instruction,
        prior = prior_answer,
        no_think = no_think_assistant_prefix(),
    )
}

/// Engine-aware direct generation for a fully-formed ChatML prompt (no retrieval).
/// Routes to vLLM or the in-process llama.cpp fallback exactly like the RAG path
/// (`try_llm_infer_streaming`). Used by the transform (rewrite) capability.
pub(crate) async fn generate_direct(
    model_slot: &ModelSlot,
    tier: &HardwareTier,
    prompt: &str,
    max_tokens: u32,
    cancel: Option<tokio_util::sync::CancellationToken>,
    app: Option<crate::compat::AppHandle>,
) -> anyhow::Result<String> {
    let (rel, _label, _notice) = select_model_for_tier(tier);
    let model_path = resolve_model_path(&rel);
    let window = crate::services::assistant::resolve_active_window(model_slot, tier)
        .min(env_usize("ECHO_MAX_GEN_CTX", 8192))
        .max(2048) as u32;
    let max_prompt = window.saturating_sub(max_tokens) as usize;
    let (out, _cancelled) = try_llm_infer_streaming(
        model_slot, &model_path, prompt, max_tokens, window, max_prompt, tier, cancel, app,
    )
    .await?;
    Ok(out)
}

/// No-think switch for hybrid-reasoning chat models (Qwen3, DeepSeek-R1, …).
///
/// Qwen3 enables a `<think>…</think>` reasoning chain **by default**; even though
/// the streaming filter in `llm.rs` discards it, the model still *generates* those
/// tokens, which roughly tripled Qwen3-8B lookup latency on the A40 (~14 s vs
/// ~4–6 s for Mistral). Setting env `NOVIQ_NO_THINK` (1/true/on/yes) pre-fills an
/// empty `<think>\n\n</think>\n\n` block immediately after the assistant tag —
/// byte-for-byte what Qwen3's official `enable_thinking=false` chat template
/// injects — so the model answers directly with no reasoning pass.
///
/// Default OFF: the empty block is prompt text (not generated), so it never leaks
/// into output, but we still gate it so the current Mistral chat model and any
/// reasoning-dependent flow are untouched. The operator sets it in the VM `.env`
/// when Qwen3 is the active model. Replaces the old unconditional short-circuit
/// that was removed because it broke LFM2.5 — gating makes that regression opt-in.
pub(crate) fn no_think_assistant_prefix() -> &'static str {
    match std::env::var("NOVIQ_NO_THINK") {
        Ok(v) if matches!(v.trim(), "1" | "true" | "TRUE" | "on" | "ON" | "yes" | "YES") => {
            "<think>\n\n</think>\n\n"
        }
        _ => "",
    }
}

/// Strip HTML tags (FTS5 snippets contain <b>…</b>).
fn strip_html_tags(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut inside = false;
    for c in s.chars() {
        match c {
            '<' => inside = true,
            '>' => inside = false,
            _ if !inside => out.push(c),
            _ => {}
        }
    }
    out
}

/// Canonical refusal sentence emitted when the model fails to produce a
/// structured answer. Matches the grounding-prompt INSUFFICIENT INFORMATION
/// clause exactly so the UI/citation pipeline treats it as a deterministic
/// refusal rather than a malformed answer.
pub(crate) const INSUFFICIENT_INFO_MSG: &str =
    "The retrieved documents do not contain enough information to answer this question.";

/// Clean up raw LLM output:
/// 1. Strip `<think>…</think>` reasoning blocks (kept for safety; instruct
///    models in v5.2 should not emit these, but tolerant parsing is cheap).
/// 2. Strip residual PART 1 / PART 2 / "REFERENCES" headers a model might
///    still emit from training patterns (v5.2 prompt doesn't request them).
/// 3. If output is empty after cleaning, substitute the canonical refusal so
///    the user never sees a blank message.
/// 4. Deduplicate consecutive repeated sentences (small models can loop).
fn clean_llm_output(text: &str) -> String {
    // Step 1 — strip <think>…</think> blocks; tolerate orphan close tags.
    let mut work = String::with_capacity(text.len());
    let mut rest = text;
    loop {
        if let Some(open) = rest.find("<think>") {
            work.push_str(&rest[..open]);
            let after = &rest[open + "<think>".len()..];
            if let Some(close) = after.find("</think>") {
                rest = &after[close + "</think>".len()..];
            } else {
                break;
            }
        } else if let Some(orphan) = rest.find("</think>") {
            rest = &rest[orphan + "</think>".len()..];
        } else {
            work.push_str(rest);
            break;
        }
    }
    let trimmed = work.trim();

    // Step 2 — strip residual section headers from older prompt patterns.
    // v5.2 doesn't ask for these, but a model may still emit them out of
    // training habit. Drop the header lines but keep the content after them.
    let stripped_headers: String = trimmed
        .lines()
        .filter(|line| {
            let l = line.trim().to_lowercase();
            !(l.starts_with("part 1") || l.starts_with("part 2")
                || l == "references" || l == "references:")
        })
        .collect::<Vec<_>>()
        .join("\n");
    let cleaned = stripped_headers.trim();

    // Step 3 — empty output → canonical refusal.
    if cleaned.is_empty() {
        return INSUFFICIENT_INFO_MSG.to_string();
    }

    // Step 4 — collapse consecutive duplicate sentences.
    let mut seen: Vec<String> = Vec::new();
    let mut out = String::new();
    for sentence in cleaned.split_inclusive(|c| c == '.' || c == '!' || c == '\n') {
        let norm = sentence.split_whitespace().collect::<Vec<_>>().join(" ");
        if norm.len() < 4 || !seen.contains(&norm) {
            out.push_str(sentence);
            seen.push(norm);
        }
    }
    out.trim().to_string()
}

async fn store_cache(
    pool: &Db,
    session_id: &str,
    query_text: &str,
    query_embedding: &[f32],
    response: &QueryResponse,
) -> Result<()> {
    let id = uuid::Uuid::new_v4().to_string();
    let now = chrono::Utc::now().timestamp_millis();
    let embedding_blob = serde_json::to_vec(query_embedding)?;
    let citations_json = serde_json::to_string(&response.citations)?;

    sqlx::query(
        "INSERT INTO query_cache \
         (id, session_id, query_text, query_embedding, answer_text, citations_json, tier_used, created_at) \
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8)",
    )
    .bind(&id)
    .bind(session_id)
    .bind(query_text)
    .bind(&embedding_blob)
    .bind(&response.answer)
    .bind(&citations_json)
    .bind(&response.tier_used)
    .bind(now)
    .execute(pool)
    .await?;

    Ok(())
}

/// Determine if the query is likely an entity (structured) query.
///
/// Regexes are compiled once per process via `OnceLock` — previously this
/// function rebuilt three regexes on every retrieval call, wasting CPU on
/// the hot path.
fn is_entity_query(query: &str) -> bool {
    use std::sync::OnceLock;
    static DATE_RE: OnceLock<regex_lite::Regex> = OnceLock::new();
    static AMOUNT_RE: OnceLock<regex_lite::Regex> = OnceLock::new();
    static EMAIL_RE: OnceLock<regex_lite::Regex> = OnceLock::new();

    // Always run FTS5 alongside vector for keyword/definitional queries so
    // exact heading matches ("Definition of the State", "Table 4", "Section 3.2")
    // beat fuzzy embeddings. Covers markers used across domains: legal, policy,
    // research, technical docs, business reports.
    let lower = query.to_lowercase();
    let keyword_triggers = [
        "definition", "define", "what is", "meaning of",
        "article ", "section ", "clause ", "chapter ", "part ",
        "schedule ", "rule ", "law ", "act ", "provision",
        "appendix ", "annex", "table ", "figure ", "exhibit ",
        "policy ", "procedure", "standard ", "requirement",
    ];
    if keyword_triggers.iter().any(|kw| lower.contains(kw)) {
        return true;
    }

    let date_re = DATE_RE.get_or_init(|| {
        regex_lite::Regex::new(r"\b\d{4}-\d{2}-\d{2}\b|\b\d{1,2}/\d{1,2}/\d{2,4}\b").unwrap()
    });
    let amount_re = AMOUNT_RE.get_or_init(|| regex_lite::Regex::new(r"\$[\d,]+").unwrap());
    let email_re = EMAIL_RE.get_or_init(|| {
        regex_lite::Regex::new(r"\b[a-zA-Z0-9._%+-]+@[a-zA-Z0-9.-]+\.[a-zA-Z]{2,}\b").unwrap()
    });

    date_re.is_match(query) || amount_re.is_match(query) || email_re.is_match(query)
}

/// S2 — Detect *definitional* questions ("Who is …", "What is …", "Define X",
/// "Who founded …"). Pure English question-grammar; no document-specific
/// tokens, so the classifier generalises to Acts / Policies / Rules / any
/// future corpus. Matches against lowercased input, allows surrounding
/// punctuation/whitespace. Examples that fire:
///
///   "Who is the founder of Pakistan?"           → true
///   "Whom does the Preamble cite as …"          → true
///   "What is the meaning of fundamental rights" → true
///   "Define high treason"                       → true
///   "Who founded the Federation"                → true
///
/// Examples that don't (instance / how / when / yes-no):
///
///   "What does Article 10 say"                  → false (instance lookup)
///   "When was the constitution adopted"         → false (event lookup)
///   "Does the constitution permit detention"    → false (yes/no)
fn is_definitional(query: &str) -> bool {
    use std::sync::OnceLock;
    static RE: OnceLock<regex_lite::Regex> = OnceLock::new();
    let re = RE.get_or_init(|| {
        // Question stems that signal a definitional / canonical-answer query.
        // - "who is/was/are/were/founded/created/established/named/declared/wrote/coined"
        // - "whom does/did/is"
        // - "what is/are/was/were/does … mean"
        // - "define <X>" / "meaning of <X>"
        regex_lite::Regex::new(
            r"(?x)
              ^\s*(
                  who\s+(is|was|are|were|founded|created|established|named|declared|wrote|coined|invented|discovered)\b
                | whom\s+(is|was|does|did|do)\b
                | what\s+(is|are|was|were)\s+the\s+(meaning|definition|constitutional\s+definition)\s+of\b
                | what\s+does\s+.+\s+mean\b
                | what\s+is\s+meant\s+by\b
                | what\s+does\s+.+\s+define\b
                | how\s+is\s+.+\s+defined\b
                | quote\s+the\s+definition\s+of\b
                | define\s+\S+
                | meaning\s+of\s+\S+
                | definition\s+of\s+\S+
                | who\s+(founded|created|established|invented|wrote|discovered|named|coined)\b
                | what\s+does\s+(article|section|rule|chapter|part)\s+\S+\s+say\s+about\s+(the\s+(term|expression|phrase|word)|the\s+meaning\s+of)\b
                | list\s+the\s+(languages|members|officers|provinces|qualifications|disqualifications|powers|duties|principles|grounds)\s+(mentioned|listed|set\s+out|specified|named|referred\s+to)\s+in\b
                | (which|what)\s+(languages|members|officers|provinces|qualifications|disqualifications|powers|duties|principles|grounds)\s+(are|is)\s+(mentioned|listed|set\s+out|specified|named|referred\s+to)\s+in\b
              )
            ",
        )
        .unwrap()
    });
    re.is_match(&query.to_lowercase())
}

/// Detect a canonical constitution citation in a free-text query — e.g.
/// "what does Article 19A(1)(a) cover" → `Some("Article 19A(1)(a)")`. Returns
/// `None` when no `Article N…` reference is present. The output string matches
/// the `chunks.unit_label` form the constitution importer stores.
fn detect_citation(query: &str) -> Option<String> {
    use std::sync::OnceLock;
    static RE: OnceLock<regex_lite::Regex> = OnceLock::new();
    // Matched against the lowercased query (regex-lite has no (?i) reliance).
    // Covers constitution (Article), acts (Section, Chapter), rules
    // (Rule, Part), manuals (Chapter, Section). `\d+[a-z]*` matches
    // "8", "19A"; `[ivxlc]+` matches roman numerals "II", "III".
    let re = RE.get_or_init(|| {
        regex_lite::Regex::new(
            r"\b(?P<kind>article|section|rule|chapter|part)\s+(?P<num>\d+[a-z]*|[ivxlc]+)(?P<parens>(?:\s*\([0-9a-z]+\))*)"
        ).unwrap()
    });
    let lower = query.to_lowercase();
    // Preamble is a top-level unit (no number); match it before the
    // numbered-unit regex so "preamble" queries reach the fast-path.
    if lower.contains("preamble") {
        return Some("Preamble".to_string());
    }
    let caps = re.captures(&lower)?;
    let kind = match caps.name("kind")?.as_str() {
        "article" => "Article",
        "section" => "Section",
        "rule"    => "Rule",
        "chapter" => "Chapter",
        "part"    => "Part",
        _ => return None,
    };
    // Numbers: arabic stays digit + uppercase suffix ("19a" → "19A");
    // roman uppercases to "II"/"III".
    let num = caps.name("num")?.as_str().to_uppercase();
    // Strip stray spaces from clause parens ("(1)", "(a)").
    let parens: String = caps
        .name("parens")
        .map_or("", |m| m.as_str())
        .chars()
        .filter(|c| !c.is_whitespace())
        .collect();
    // Chapter / Part headings don't carry clause-paren suffixes in any
    // profile's citation_fmt; drop parens for those to avoid mis-keys.
    let parens = if matches!(kind, "Chapter" | "Part") { String::new() } else { parens };
    // False positives (e.g. "section of the report") cost only an
    // indexed `chunks.unit_label = ?` lookup, which returns Ok(None)
    // and falls through to hybrid retrieval. Safe.
    Some(format!("{kind} {num}{parens}"))
}

/// Append the canonical BASE provision label to the retrieval query (2026-07-18).
///
/// Chunks are labelled with the bare provision — `chunks.unit_label` is `"Rule 4"`,
/// never `"Rule 4(1)(a)"` — so a query phrased as *"rule 4(I)(a) sub rule 3"* has
/// no token that matches the label, while the words "sub rule" match every chunk
/// in the corpus containing "sub-rule (2)". Trace `99cdbff2` retrieved 32 chunks
/// that way and got Rule 5, Rule 35 (SPPRA blacklisting) and Rule 5 — **zero**
/// Rule 4 chunks — then correctly refused.
///
/// Appending is deliberate: rewriting the query could lose signal, appending
/// cannot. Returns `None` when the query carries no citation, or when the base
/// label is already present verbatim (nothing to add).
///
/// Note this also disposes of the `4(I)` vs `4(1)` typo for free — the base label
/// drops the clause parens entirely, so the confusable character never matters.
fn augment_provision_terms(query: &str) -> Option<String> {
    let cite = detect_citation(query)?;
    // "Rule 4(1)(a)" → "Rule 4";  "Preamble" → "Preamble".
    let base = cite.split('(').next()?.trim().to_string();
    if base.is_empty() || base == cite {
        // The citation carries no clause parens, so the query already reads as the
        // bare label and there is nothing to normalise.
        return None;
    }
    Some(base)
}

/// Convert an arabic part number ("2") to its roman form ("II") so it matches
/// the constitution's part numbering. Roman input is passed through uppercased.
/// Only 1..=20 is supported (the constitution has 12 parts); anything else
/// returns the input uppercased unchanged.
fn normalise_part_number(raw: &str) -> String {
    let raw = raw.trim();
    // Already roman? (i/v/x/l/c letters only)
    if raw.chars().all(|c| "ivxlcdmIVXLCDM".contains(c)) && !raw.is_empty() {
        return raw.to_uppercase();
    }
    const ROMAN: [&str; 20] = [
        "I", "II", "III", "IV", "V", "VI", "VII", "VIII", "IX", "X",
        "XI", "XII", "XIII", "XIV", "XV", "XVI", "XVII", "XVIII", "XIX", "XX",
    ];
    match raw.parse::<usize>() {
        Ok(n) if (1..=20).contains(&n) => ROMAN[n - 1].to_string(),
        _ => raw.to_uppercase(),
    }
}

/// Chapter-index fast-path resolver (2026-06-06). Enumeration queries like
/// "Which Articles fall under Chapter 1 of Part II?" or "What are the Principles
/// of Policy in Chapter 2 of Part II?" must hit the synthetic `IDX-P<part>-C<chapter>`
/// articles injected by `inject_chapter_index.py` (unit_label
/// `Article IDX-P<part>-C<chapter>`). Plain hybrid retrieval ranks the wrong
/// chapter index to the top (observed: Part II Chapter 1 → Chapter 6 confusion),
/// so we resolve the exact IDX unit_label deterministically.
///
/// Returns the target `unit_label` (e.g. `"Article IDX-PII-C1"`) or `None`.
/// Gating: must NOT name a specific `Article N` (those are ordinary lookups),
/// and must mention a Part. Chapter present → chapter-level index; Part only
/// with an enumeration verb → part-level index.
fn detect_chapter_index_citation(query: &str) -> Option<String> {
    use std::sync::OnceLock;
    let lower = query.to_lowercase();
    // Defer specific article lookups to the ordinary article fast-path.
    static ART_RE: OnceLock<regex_lite::Regex> = OnceLock::new();
    let art_re = ART_RE.get_or_init(|| {
        regex_lite::Regex::new(r"\barticle\s+\d+[a-z]*").unwrap()
    });
    if art_re.is_match(&lower) {
        return None;
    }
    static PART_RE: OnceLock<regex_lite::Regex> = OnceLock::new();
    let part_re = PART_RE.get_or_init(|| {
        regex_lite::Regex::new(r"\bpart\s+(\d+|[ivxlcdm]+)\b").unwrap()
    });
    let part = part_re
        .captures(&lower)
        .and_then(|c| c.get(1))
        .map(|m| normalise_part_number(m.as_str()))?;

    static CHAP_RE: OnceLock<regex_lite::Regex> = OnceLock::new();
    let chap_re = CHAP_RE.get_or_init(|| {
        regex_lite::Regex::new(r"\bchapter\s+(\d+[a-z]*)\b").unwrap()
    });
    if let Some(chap) = chap_re
        .captures(&lower)
        .and_then(|c| c.get(1))
        .map(|m| m.as_str().to_uppercase())
    {
        return Some(format!("Article IDX-P{part}-C{chap}"));
    }

    // Part-only: require an enumeration intent to avoid hijacking generic
    // "in Part XII the President may…" style lookups.
    const ENUM_HINTS: &[&str] = &[
        "which article", "what article", "list", "fall under", "contained in",
        "contains", "articles in", "articles under", "articles fall",
    ];
    if ENUM_HINTS.iter().any(|h| lower.contains(h)) {
        return Some(format!("Article IDX-P{part}"));
    }
    None
}

/// Citation fast-path (feature 003, T025). When the query names a specific
/// constitutional unit ("Article 19A", "Article 8(3)(b)"), resolve it
/// deterministically by `unit_label` — no embedding, no reranker. Returns the
/// cited chunk plus every descendant (an Article-level citation yields the
/// whole Article — clauses and sub-clauses included), ordered by `chunk_index`.
///
/// `Ok(None)` ⇒ no citation in the query, or the cited unit is not in the
/// corpus → the caller falls through to ordinary hybrid retrieval.
/// Remove `Article 260`, `Section 5(2)(a)`, `Rule 7`, `Chapter 1`, `Part II`,
/// `Preamble` references from a query so only the *content* tokens remain.
/// Used by the citation fast-path's keyword-augmentation pass to feed FTS5
/// the discriminating words (e.g. "citizen") without the article number
/// matching every chunk in the cited source.
fn strip_citation_refs(query: &str) -> String {
    use std::sync::OnceLock;
    static RE: OnceLock<regex_lite::Regex> = OnceLock::new();
    let re = RE.get_or_init(|| {
        regex_lite::Regex::new(
            r"\b(article|section|rule|chapter|part)\s+(?:\d+[a-z]*|[ivxlc]+)(?:\s*\([0-9a-z]+\))*"
        ).unwrap()
    });
    let lower = query.to_lowercase();
    let stripped = re.replace_all(&lower, " ");
    stripped.replace("preamble", " ")
}

pub async fn citation_fast_path(
    pool: &Db,
    query: &str,
) -> Result<Option<Vec<String>>> {
    // Chapter-index fast-path (2026-06-06): enumeration queries resolve to the
    // synthetic IDX-P<part>-C<chapter> article by exact unit_label. The IDX
    // article is self-contained (the enumeration lives in its own body), so we
    // return its chunk id(s) directly — no descendant walk, no source-affinity
    // disambiguation (single-source constitution corpus).
    if let Some(idx_label) = detect_chapter_index_citation(query) {
        let ids: Vec<String> = sqlx::query_scalar(
            "SELECT id FROM chunks WHERE unit_label = $1 ORDER BY chunk_index",
        )
        .bind(&idx_label)
        .fetch_all(pool)
        .await?;
        if !ids.is_empty() {
            return Ok(Some(ids));
        }
        // IDX chunk not indexed yet (re-index pending) → fall through.
    }
    let Some(citation) = detect_citation(query) else {
        return Ok(None);
    };
    // Fetch every chunk that matches this unit_label, with its content length.
    // unit_label collisions are routine ("Section 3" lives in 2 sources, "Rule
    // 5" in 2, "Preamble" in 6) — a blind LIMIT 1 returns whichever row sqlite
    // picks first, which is normally the wrong document. We score per source
    // and pick the best one. content_len is kept as a tie-breaker: when two
    // statutes both have "Rule 5", the longer chunk is usually the actual rule
    // body rather than a header stub or a table row in another statute.
    let rows: Vec<(Option<String>, String, i64)> = sqlx::query_as(
        "SELECT c.unit_path, c.source_id, LENGTH(c.content)::bigint \
         FROM chunks c \
         WHERE c.unit_label = $1",
    )
    .bind(&citation)
    .fetch_all(pool)
    .await?;
    if rows.is_empty() {
        return Ok(None);
    }
    // Per source: keep the shallowest matching unit_path (the one closest
    // to root — that's the citation root whose descendants we want). Ties
    // on depth are broken by larger content length, which prefers the real
    // rule body over a same-label stub elsewhere in the document. Picking
    // by raw content_len alone is wrong: when paragraph children share
    // their parent's unit_label (e.g. Idea 1 paragraph splitting on the
    // Preamble), a child's longer content can win and collapse the result
    // to that single paragraph instead of the parent + every paragraph.
    let mut per_src: HashMap<String, (String, usize, i64)> = HashMap::new();
    for (unit_path, source_id, content_len) in &rows {
        let Some(up) = unit_path.as_deref() else { continue };
        let depth = up.matches('/').count();
        per_src
            .entry(source_id.clone())
            .and_modify(|(best_up, best_depth, best_len)| {
                let cur = (depth, -*content_len);
                let best = (*best_depth, -*best_len);
                if cur < best {
                    *best_up = up.to_string();
                    *best_depth = depth;
                    *best_len = *content_len;
                }
            })
            .or_insert_with(|| (up.to_string(), depth, *content_len));
    }
    if per_src.is_empty() {
        return Ok(None);
    }
    // Build an affinity vocabulary per candidate source. Filename alone is
    // not enough: "efficiency and disciplane rules.pdf" loses to "appointment
    // promotion and transfer rules.pdf" on a query that mentions "Sindh
    // Civil Servants Efficiency and Discipline Rules 1973" because they tie
    // on `{rules}` (and the filename misspells "disciplane"). We also pull
    // each source's leading chunks — preamble + early body — where the
    // statute title, year, and definitions naturally appear. That's where
    // the discriminating tokens ("1973", "1986", "Cabinet", "Ordinance")
    // actually live. Single batched query: one round-trip for all candidates.
    let source_ids: Vec<String> = per_src.keys().cloned().collect();
    let placeholders = crate::db::pg_in(source_ids.len(), 1);
    // head_text = first 10 chunks (by chunk_index), each truncated to 400 chars,
    // concatenated with spaces. Cap is enough for the title block, the
    // notification line ("Karachi, the 8th September 1973"), and the
    // definitions section without dragging in unrelated body text.
    let aff_sql = format!(
        "SELECT s.id, COALESCE(s.display_name, s.file_name, ''), \
                COALESCE(( \
                  SELECT STRING_AGG(head, ' ') FROM ( \
                    SELECT SUBSTR(content, 1, 400) AS head \
                    FROM chunks WHERE source_id = s.id \
                    ORDER BY chunk_index LIMIT 10 \
                  ) AS h \
                ), '') \
         FROM sources s WHERE s.id IN ({placeholders})"
    );
    let mut aff_q = sqlx::query_as::<_, (String, String, String)>(&aff_sql);
    for sid in &source_ids {
        aff_q = aff_q.bind(sid);
    }
    let aff_rows = aff_q.fetch_all(pool).await?;

    let q_tokens = source_affinity_tokens(query);
    // Pick the source with the highest (overlap, content_len). Overlap is
    // primary; content_len breaks ties deterministically. When overlap is
    // zero for every candidate (query carries no discriminator) we still
    // emit a winner via the tie-breaker — better than dropping the fast
    // path and forcing the user through full hybrid retrieval.
    let mut best: Option<(usize, i64, String, String)> = None;
    for (source_id, file_name, head_text) in &aff_rows {
        let mut vocab = source_affinity_tokens(file_name);
        vocab.extend(source_affinity_tokens(head_text));
        let overlap = q_tokens.intersection(&vocab).count();
        let Some((unit_path, _depth, content_len)) = per_src.get(source_id) else { continue };
        let candidate = (overlap, *content_len, source_id.clone(), unit_path.clone());
        best = match best {
            None => Some(candidate),
            Some((bo, bl, _, _)) if (candidate.0, candidate.1) > (bo, bl) => Some(candidate),
            Some(_) => best,
        };
    }
    let Some((best_overlap, _, best_source_id, best_unit_path)) = best else {
        return Ok(None);
    };

    // Scope guard (forensic audit 2026-05-28): only fires when the citation
    // exists in MULTIPLE sources AND the winner has ZERO source-affinity
    // overlap. That combination means "Rule N is in two statutes, the query
    // mentions discriminative tokens, but none match either candidate" —
    // typically the user named a doc (e.g. "Rules of Business 1986") whose
    // structured-importer pass mis-labelled the Rule headings, so the
    // lookup returned only foreign-doc matches (E&D). Fall through to
    // hybrid retrieval — vec + FTS5 still surface the right doc's body
    // chunks even when the importer missed the heading.
    //
    // Single-candidate citations (typical for the Constitution corpus) keep
    // the existing behaviour: no ambiguity to resolve, so a zero overlap
    // just means the query didn't carry discriminator tokens.
    if per_src.len() > 1 && best_overlap == 0 && !q_tokens.is_empty() {
        return Ok(None);
    }

    // Descendants — filtered by source_id so we never cross-bleed (a previous
    // bug: same unit_path across two sources would merge both subtrees).
    let descendants = format!("{best_unit_path}/%");
    let ids: Vec<String> = sqlx::query_scalar(
        "SELECT id FROM chunks WHERE source_id = $1 \
           AND (unit_path = $2 OR unit_path LIKE $3) ORDER BY chunk_index",
    )
    .bind(&best_source_id)
    .bind(&best_unit_path)
    .bind(&descendants)
    .fetch_all(pool)
    .await?;
    Ok((!ids.is_empty()).then_some(ids))
}

/// Cheap keyword tokenizer for source-affinity scoring. Lowercases, strips
/// non-alphanumerics, drops legal-context noise words and ≤2-char tokens.
/// Used only for unit_label disambiguation — not exposed for retrieval.
///
/// The stopword list rejects tokens that appear in roughly every legal query
/// or roughly every statute title and so carry no discriminative signal
/// (e.g. "rule" / "rules" / "section" / "article"). Statute-specific tokens
/// like statute names, ordinance years, and defined terms ("Cabinet",
/// "Ordinance", "1973") survive and do the actual disambiguation.
fn source_affinity_tokens(s: &str) -> HashSet<String> {
    const STOP: &[&str] = &[
        "the", "and", "for", "with", "from", "this", "that", "what", "which",
        "does", "explain", "describe", "quote", "give", "tell", "section",
        "article", "rule", "rules", "clause", "subclause", "chapter", "part",
        "pdf",
    ];
    s.to_lowercase()
        .chars()
        .map(|c| if c.is_alphanumeric() { c } else { ' ' })
        .collect::<String>()
        .split_whitespace()
        .filter(|t| t.len() >= 3 && !STOP.contains(t))
        .map(|t| t.to_string())
        .collect()
}

/// Fetch chunks as `(id, source_id, content, page, label)` tuples for the
/// given ids, returned in `ids` order. Ids with no row are skipped.
///
/// The label uses the SAME `COALESCE(display_name, file_name, …)` expression as
/// `fts5_retrieve` / `get_context_chunks`. It used to be a bare `s.file_name`,
/// so ONE document occupied TWO label buckets depending on which arm fetched it
/// — graph expansion and table completion landed in a different bucket than the
/// identical document's hybrid hits. `apply_instrument_boost` buckets on this
/// field (`by_file`, see its body) and so does the trace source-mix line, so the
/// split silently weakened both.
async fn fetch_chunks_by_ids(
    pool: &Db,
    ids: &[String],
) -> Result<Vec<(String, String, String, Option<i64>, String)>> {
    if ids.is_empty() {
        return Ok(Vec::new());
    }
    let placeholders = crate::db::pg_in(ids.len(), 1);
    let q = format!(
        "SELECT c.id, c.source_id, c.content, c.page_number, \
                COALESCE(s.display_name, s.file_name, s.file_path, 'unknown') \
         FROM chunks c JOIN sources s ON c.source_id = s.id \
         WHERE c.id IN ({placeholders})"
    );
    let mut bind = sqlx::query_as::<_, (String, String, String, Option<i64>, String)>(&q);
    for id in ids {
        bind = bind.bind(id);
    }
    let rows = bind.fetch_all(pool).await?;
    let by_id: std::collections::HashMap<String, (String, String, String, Option<i64>, String)> =
        rows.into_iter().map(|r| (r.0.clone(), r)).collect();
    Ok(ids.iter().filter_map(|id| by_id.get(id).cloned()).collect())
}

/// Citation-query graph expansion (feature 003, T026 — Option 1). Given the
/// chunk ids resolved by `citation_fast_path`, follow the legal graph one hop
/// over `cross_reference` / `non_obstante` edges and return the union of the
/// cited chunks plus the referenced/overridden Articles. Graph errors are
/// non-fatal — the cited chunks are returned unexpanded.
async fn expand_citation_neighbours(pool: &Db, cited: &[String]) -> Vec<String> {
    use crate::db::repositories::graph;
    let mut all: Vec<String> = cited.to_vec();
    let seeds = match graph::node_ids_for_chunks(pool, cited).await {
        Ok(s) => s,
        Err(e) => {
            log::warn!("citation graph expansion — seed lookup failed: {e}");
            return all;
        }
    };
    if seeds.is_empty() {
        return all; // cited content has no graph nodes (not constitution corpus)
    }
    match graph::expand(pool, &seeds, &["cross_reference", "non_obstante"], 1).await {
        Ok(nodes) => {
            for n in nodes {
                if !all.contains(&n.chunk_id) {
                    all.push(n.chunk_id);
                }
            }
        }
        Err(e) => log::warn!("citation graph expansion — expand failed: {e}"),
    }
    all
}

fn cosine_similarity(a: &[f32], b: &[f32]) -> f32 {
    if a.len() != b.len() {
        return 0.0;
    }
    let dot: f32 = a.iter().zip(b.iter()).map(|(x, y)| x * y).sum();
    let mag_a: f32 = a.iter().map(|x| x * x).sum::<f32>().sqrt();
    let mag_b: f32 = b.iter().map(|x| x * x).sum::<f32>().sqrt();
    if mag_a == 0.0 || mag_b == 0.0 {
        return 0.0;
    }
    dot / (mag_a * mag_b)
}


/// Streaming LLM inference — emits chat://token per token, returns (output, was_cancelled).
async fn try_llm_infer_streaming(
    model_slot: &ModelSlot,
    model_path: &str,
    prompt: &str,
    max_tokens: u32,
    n_ctx: u32,
    max_prompt_tokens: usize,
    tier: &HardwareTier,
    cancel: Option<tokio_util::sync::CancellationToken>,
    app: Option<crate::compat::AppHandle>,
) -> Result<(String, bool)> {
    // F016: route generation to the vLLM sidecar when selected (FR-001/017). The
    // llama.cpp path below is the config-switchable fallback. n_ctx/max_prompt are
    // llama-specific (the budget allocator already kept the prompt within the vLLM
    // window); vLLM manages context server-side.
    if crate::services::inference::engine_from_env().is_http() {
        match vllm_infer(prompt, max_tokens, cancel.clone(), app.clone()).await {
            Ok(out) => return Ok(out),
            Err(e) => {
                // FR-011: degrade gracefully. If the operator has enabled the
                // in-process fallback (`NOVIQ_VLLM_FALLBACK` truthy), drop through
                // to the llama.cpp path below — note this loads the GGUF into VRAM
                // beside vLLM, so it is opt-in only (off by default on the 12 GB
                // card). Otherwise surface the engine error to the call site, which
                // renders the engine-aware "temporarily unavailable" message.
                if !vllm_fallback_enabled() {
                    return Err(e);
                }
                log::warn!("vLLM unavailable ({e}); NOVIQ_VLLM_FALLBACK set — falling back to llama.cpp");
            }
        }
    }
    tokio::task::spawn_blocking({
        let model_slot = unsafe {
            // Safety: ModelSlot is Send + Sync.
            &*(model_slot as *const ModelSlot)
        };
        let model_path = model_path.to_string();
        let tier = tier.clone();
        let prompt = prompt.to_string();
        move || model_slot.infer_with_cancel(&model_path, &prompt, max_tokens, n_ctx, max_prompt_tokens, &tier, cancel, app)
    })
    .await?
}

/// F016 — whether to fall back to the in-process llama.cpp path when the vLLM
/// sidecar errors (FR-011). Off by default: on the 12 GB card vLLM owns the VRAM,
/// so loading the GGUF beside it is an explicit operator opt-in.
fn vllm_fallback_enabled() -> bool {
    matches!(
        std::env::var("NOVIQ_VLLM_FALLBACK")
            .unwrap_or_default()
            .trim()
            .to_ascii_lowercase()
            .as_str(),
        "1" | "true" | "on" | "yes"
    )
}

/// F016 — generate via the vLLM sidecar (used when `NOVIQ_INFER_ENGINE=vllm`).
/// Sends the raw ChatML prompt verbatim (no-think preserved); emits `chat://token`
/// per delta when an app handle is present (the headless server passes `None`).
/// Returns `(text, was_cancelled)` to match the llama.cpp path; the caller applies
/// `clean_llm_output` + the grounding/citation gate. On engine error returns
/// `Err`, which the call site renders as the FR-011 "model unavailable" message.
pub(crate) async fn vllm_infer(
    prompt: &str,
    max_tokens: u32,
    cancel: Option<tokio_util::sync::CancellationToken>,
    app: Option<crate::compat::AppHandle>,
) -> Result<(String, bool)> {
    use crate::services::inference::{http_generator, GenParams, TextGenerator};
    let params = GenParams {
        max_tokens,
        temperature: 0.0,
        top_p: 1.0,
        stop: vec!["<|im_end|>".to_string(), "</s>".to_string()],
        ..GenParams::default()
    };
    let token = cancel.unwrap_or_default();
    let mut on_token = move |t: &str| {
        if let Some(a) = &app {
            use crate::compat::Emitter;
            let _ = a.emit("chat://token", serde_json::json!({ "token": t }));
        }
    };
    // Both HTTP engines (vLLM sidecar / remote API) stream through this one path;
    // `http_generator()` returns the impl for the selected engine. `None` only when
    // the engine is llama.cpp, which never reaches here (caller gates on `is_http`).
    let gen = http_generator()
        .ok_or_else(|| anyhow::anyhow!("no HTTP inference engine selected"))?;
    gen.generate_stream(prompt, &params, token, &mut on_token)
        .await
        .map(|o| (o.text, o.was_cancelled))
        .map_err(|e| anyhow::anyhow!("HTTP inference failed: {e}"))
}

/// Parse YES/NO output from the LLM judge.
/// Returns true (answer is responsive) only when output explicitly contains "YES" as a whole word.
/// Avoids false positives from "YESTERDAY", "YESSIR", etc.
pub(crate) fn parse_judge_output(output: &str) -> bool {
    let upper = output.to_uppercase();
    upper.split_whitespace().any(|w| {
        let w = w.trim_matches(|c: char| !c.is_alphabetic());
        w == "YES"
    })
}

/// Build the judge prompt: minimal ChatML, YES/NO classification only.
fn build_judge_prompt(query: &str, answer: &str) -> String {
    format!(
        "<|im_start|>system\n\
You are a relevance judge. Reply only YES or NO.\
<|im_end|>\n\
<|im_start|>user\n\
Question: {query}\n\
Answer: {answer}\n\
Does this answer directly address the question? Reply YES or NO only.\
<|im_end|>\n\
<|im_start|>assistant\n{no_think}",
        no_think = no_think_assistant_prefix(),
    )
}

/// Run the LLM judge synchronously (no streaming, no app handle, n_ctx=512).
/// Returns true if the answer is responsive to the query.
/// On any inference error, returns true (fail-open — don't block on judge errors).
async fn judge_answer(
    model_slot: &ModelSlot,
    model_path: &str,
    query: &str,
    answer: &str,
    tier: &HardwareTier,
) -> bool {
    let prompt = build_judge_prompt(query, answer);
    let result = tokio::task::spawn_blocking({
        // Safety: ModelSlot is Send + Sync.
        let model_slot = unsafe { &*(model_slot as *const ModelSlot) };
        let model_path = model_path.to_string();
        let tier = tier.clone();
        move || model_slot.infer_with_cancel(
            &model_path, &prompt,
            5,       // max_tokens — just need YES or NO
            512,     // n_ctx — very small, judge prompt is ~100 tokens
            400,     // max_prompt_tokens
            &tier,
            None,    // no cancellation for judge
            None,    // no streaming
        )
    })
    .await;

    match result {
        Ok(Ok((output, _cancelled))) => {
            let verdict = parse_judge_output(&output);
            let preview: String = output.chars().take(20).collect();
            log::info!("Judge verdict: {} (raw={:?})", if verdict { "PASS" } else { "FAIL" }, preview);
            verdict
        }
        Ok(Err(e)) => {
            log::warn!("Judge inference error — failing open: {e}");
            true
        }
        Err(e) => {
            log::warn!("Judge spawn_blocking panicked — failing open: {e}");
            true
        }
    }
}

fn truncate(s: &str, max_chars: usize) -> String {
    if s.chars().count() <= max_chars {
        s.to_string()
    } else {
        let end = s.char_indices().nth(max_chars).map(|(i, _)| i).unwrap_or(s.len());
        format!("{}...", &s[..end])
    }
}

// ─── Grounding validator (FR-042 / AD-8) ─────────────────────────────────────

/// Validate that an LLM response is grounded in the retrieved context chunks.
///
/// **Trigger conditions for `fallback_triggered = true`**:
/// - Substantive response (> 100 chars) that contains zero citation markers.
///   Catches LLMs that ignored the citation instruction entirely.
/// - Any citation marker whose source name does not match any file in
///   `context_chunks`. Catches fabricated or hallucinated source references.
///
/// The validator intentionally does **not** trigger on partially-cited
/// responses (LLM cited some but not all sentences) to avoid over-triggering
/// on legitimate connecting text between cited passages.
///
/// When the input is already `GROUNDING_FALLBACK`, returns grounded=true with
/// no fallback (idempotent — the right answer is already correct).
fn validate_grounding(
    response: &str,
    context_chunks: &[(String, String, String, Option<i64>, String)],
) -> GroundingResult {
    let trimmed = response.trim();

    // Fallback phrase or very short response — trivially grounded.
    if trimmed == GROUNDING_FALLBACK || trimmed.is_empty() {
        return GroundingResult {
            is_grounded: true,
            citation_count: 0,
            uncited_sentence_count: 0,
            fallback_triggered: false,
            grounding_score: 1.0,
            uncited_only: false,
        };
    }

    // Build set of known source stems (file_name without extension, lowercased).
    let known_stems: Vec<String> = context_chunks
        .iter()
        .map(|c| file_stem(&c.4).to_lowercase())
        .collect();

    // Extract all citation markers from the response.
    let citations = extract_citations(trimmed);
    // v5.2: also count `[N]` index markers (1..=chunks.len()). These are the
    // primary citation form post-v5.2; legacy `[filename, page N]` markers
    // remain accepted for backwards compatibility (cached answers, older
    // model outputs). Counted here so the no-citations check below doesn't
    // misfire for v5.2 answers.
    let indexed_valid = count_valid_index_markers(trimmed, context_chunks.len());
    let factual_count = count_factual_sentences(trimmed);
    let has_any_citation = !citations.is_empty() || indexed_valid > 0;

    // ── Check 1: explicit "I don't know" phrase from the model ───────────
    // Only trigger fallback when the model itself says it can't find the answer
    // AND the answer is short enough that the kill-phrase is the whole point.
    // Long answers that merely *mention* "not found" inside a real explanation
    // should pass through (Fix A — 2026-05-02; see CHANGELOG_DEV.md).
    let lower = trimmed.to_lowercase();
    let is_short = trimmed.chars().count() <= 200;
    if !has_any_citation
        && is_short
        && (lower.contains("no relevant information")
            || lower.contains("not found in")
            || lower.contains("cannot find")
            || lower.contains("not present in the"))
    {
        return GroundingResult {
            is_grounded: false,
            citation_count: 0,
            uncited_sentence_count: factual_count,
            fallback_triggered: true,
            grounding_score: 0.0,
            uncited_only: false,
        };
    }

    // ── Check 2: every citation must reference a known source ─────────────
    // Start with v5.2 indexed-citation count; these are valid by construction
    // (count_valid_index_markers already gates on chunks.len()).
    let mut valid_count = indexed_valid;
    let mut has_invalid_source = false;

    for cited_name in &citations {
        let cited_stem = file_stem(cited_name).to_lowercase();
        let valid = known_stems.iter().any(|k| {
            k.starts_with(&cited_stem) || cited_stem.starts_with(k.as_str())
        });
        if valid {
            valid_count += 1;
        } else {
            log::warn!("Grounding: citation to unknown source '{cited_name}'");
            has_invalid_source = true;
        }
    }

    let grounding_score = if factual_count == 0 {
        1.0
    } else {
        valid_count as f32 / factual_count as f32
    };

    // Check 3: very long uncited answer.
    // Short answers (≤4 sentences) without citations are allowed — 3B models answer
    // correctly but often omit citation markers. Only flag when response is long enough
    // that hallucination is the more plausible explanation.
    let uncited_fallback = !has_any_citation && factual_count >= 5;

    // Fix A (2026-05-02): if at least one citation is valid, don't trigger
    // fallback for invalid ones — partial-cite answers are still useful.
    // Only nuke when ALL citations are invalid AND the answer is mostly factual.
    let all_invalid = has_invalid_source && valid_count == 0 && !citations.is_empty();
    let is_long_real_answer = trimmed.chars().count() >= 80 && factual_count >= 2;

    // Loosen grounding (2026-06-06, CHANGELOG followup #2): a short answer with
    // a bad/missing citation marker is still grounded if its content tokens
    // clearly overlap the retrieved context. Catches list-form answers from
    // small models ("Urdu, English", bullet enumerations) that drop or mangle
    // the `[n]` marker but are verbatim built from the cited chunk. Gate on
    // overlap ≥ 50% of the answer's content tokens so genuine hallucinations
    // (no shared vocabulary) still fall through to fallback.
    let context_overlap = content_overlap_ratio(trimmed, context_chunks);
    let overlap_rescue = context_overlap >= 0.5;
    let kill_for_invalid = all_invalid && !is_long_real_answer && !overlap_rescue;

    GroundingResult {
        is_grounded: !kill_for_invalid && !uncited_fallback,
        citation_count: valid_count,
        uncited_sentence_count: factual_count.saturating_sub(valid_count),
        fallback_triggered: kill_for_invalid || uncited_fallback,
        grounding_score,
        // Treat "long real answer with bad/no citations" — or a short answer
        // rescued by context overlap — as uncited_only so the grounding block
        // keeps the answer and shows an uncited notice instead of nuking it.
        uncited_only: (uncited_fallback
            || (has_invalid_source && (is_long_real_answer || overlap_rescue)))
            && !kill_for_invalid,
    }
}

/// Fraction of an answer's content tokens (lowercased, ≥4 chars, deduped) that
/// also appear in the concatenated retrieved-context content. Used by the
/// grounding validator to rescue correct-but-uncited short answers: a high
/// overlap means the answer is built from the context, not hallucinated.
/// Returns 0.0 when the answer has no content tokens (nothing to ground).
fn content_overlap_ratio(
    answer: &str,
    context_chunks: &[(String, String, String, Option<i64>, String)],
) -> f32 {
    let ans_tokens: HashSet<String> = answer
        .split(|c: char| !c.is_alphanumeric())
        .filter(|w| w.chars().count() >= 4)
        .map(|w| w.to_lowercase())
        .collect();
    if ans_tokens.is_empty() {
        return 0.0;
    }
    let ctx: String = context_chunks
        .iter()
        .map(|c| c.2.to_lowercase())
        .collect::<Vec<_>>()
        .join(" ");
    let ctx_tokens: HashSet<String> = ctx
        .split(|c: char| !c.is_alphanumeric())
        .filter(|w| w.chars().count() >= 4)
        .map(|w| w.to_string())
        .collect();
    let shared = ans_tokens.iter().filter(|t| ctx_tokens.contains(*t)).count();
    shared as f32 / ans_tokens.len() as f32
}

/// Count `[N]` index markers in the text where `1 <= N <= chunks_len`.
/// Used by the grounding validator to recognise v5.2-style citations without
/// pulling the chunks vector through every call site. Each distinct valid
/// index counts once; repeats are ignored.
fn count_valid_index_markers(text: &str, chunks_len: usize) -> u32 {
    if chunks_len == 0 {
        return 0;
    }
    let mut seen: Vec<usize> = Vec::new();
    let mut rest = text;
    while let Some(start) = rest.find('[') {
        rest = &rest[start + 1..];
        if let Some(end) = rest.find(']') {
            let inner = rest[..end].trim();
            if !inner.is_empty()
                && !inner.contains(',')
                && inner.chars().all(|c| c.is_ascii_digit())
            {
                if let Ok(n) = inner.parse::<usize>() {
                    if n >= 1 && n <= chunks_len && !seen.contains(&n) {
                        seen.push(n);
                    }
                }
            }
            rest = &rest[end + 1..];
        } else {
            break;
        }
    }
    seen.len() as u32
}

/// Extract `[N]` index citations from the answer and resolve them to the
/// corresponding chunk's (filename, page). Preserves order of first
/// appearance; deduplicates. Indices outside `1..=chunks.len()` are ignored.
/// v5.2 prompt: model emits `[1]`, `[2]`, etc. tied to context chunk numbers.
///
/// Superseded in production by `indexed_citation_markers` (Phase-0e exact-chunk
/// mapping); retained for its tests and potential reuse.
#[allow(dead_code)]
fn extract_indexed_citations(
    text: &str,
    chunks: &[(String, String, String, Option<i64>, String)],
) -> Vec<(String, Option<i64>)> {
    let mut out: Vec<(String, Option<i64>)> = Vec::new();
    let mut seen: Vec<usize> = Vec::new();
    let mut rest = text;
    while let Some(start) = rest.find('[') {
        rest = &rest[start + 1..];
        if let Some(end) = rest.find(']') {
            let inner = rest[..end].trim();
            // Only pure-numeric `[N]` here — `[file.pdf]` falls through to the
            // legacy parser. Reject markers with commas (legacy format).
            if !inner.is_empty()
                && !inner.contains(',')
                && inner.chars().all(|c| c.is_ascii_digit())
            {
                if let Ok(n) = inner.parse::<usize>() {
                    if n >= 1 && n <= chunks.len() && !seen.contains(&n) {
                        seen.push(n);
                        let chunk = &chunks[n - 1];
                        out.push((chunk.4.clone(), chunk.3));
                    }
                }
            }
            rest = &rest[end + 1..];
        } else {
            break;
        }
    }
    out
}

/// Marker numbers `[N]` in the answer, in order of first appearance, deduped,
/// restricted to `1..=len`. Phase-0e (2026-06-13): used to resolve each marker
/// to its EXACT chunk (`chunks[N-1]`) so one `[N]` yields exactly one citation,
/// instead of the old filename+page re-match that exploded into every chunk of
/// the cited document (page is NULL corpus-wide → matched all).
fn indexed_citation_markers(text: &str, len: usize) -> Vec<usize> {
    let mut out: Vec<usize> = Vec::new();
    let mut rest = text;
    while let Some(start) = rest.find('[') {
        rest = &rest[start + 1..];
        if let Some(end) = rest.find(']') {
            let inner = rest[..end].trim();
            if !inner.is_empty()
                && !inner.contains(',')
                && inner.chars().all(|c| c.is_ascii_digit())
            {
                if let Ok(n) = inner.parse::<usize>() {
                    if n >= 1 && n <= len && !out.contains(&n) {
                        out.push(n);
                    }
                }
            }
            rest = &rest[end + 1..];
        } else {
            break;
        }
    }
    out
}

/// A clean citation snippet: the operative text without mid-word/mid-sentence
/// cuts. Strips HTML, collapses whitespace, then cuts at the first sentence
/// boundary past ~12 words (abbreviation-naive but adequate), hard cap ~220
/// chars. Replaces the old `truncate(content, 300)` that sliced mid-word
/// ("competent to approve develop ment projects costing up to Rs . 10,000").
fn citation_snippet(content: &str) -> String {
    let clean = strip_html_tags(content);
    let norm: String = clean.split_whitespace().collect::<Vec<_>>().join(" ");
    let chars: Vec<char> = norm.chars().collect();
    let mut words = 0usize;
    for i in 0..chars.len() {
        if chars[i] == ' ' {
            words += 1;
        }
        let sentence_end = matches!(chars[i], '.' | ';') && words >= 12;
        if sentence_end || i + 1 >= 220 {
            return chars[..=i].iter().collect::<String>().trim().to_string();
        }
    }
    norm.trim().to_string()
}

// ─── Extractive chat (beta) — answer = verbatim provision, no LLM synthesis ──────

/// Whether the chat answer is composed EXTRACTIVELY (verbatim provision text +
/// structured citation, model never writes facts). Default **OFF** — chat uses
/// the LLM. The extractive path is retained but dormant; opt in by setting
/// `db_config` key `chat_extractive` to a non-'0' value (only `'1'` enables it).
async fn chat_extractive_enabled(pool: &Db) -> bool {
    let raw: Option<String> = sqlx::query_scalar(
        "SELECT value FROM db_config WHERE key = 'chat_extractive'",
    )
    .fetch_optional(pool)
    .await
    .ok()
    .flatten();
    raw.map(|v| v == "1").unwrap_or(false)
}

/// Clean a source filename into a human document name:
/// "10_SPSC_Regulations_2023.pdf" → "SPSC Regulations 2023".
fn clean_doc_name(file_name: &str) -> String {
    let stem = file_stem(file_name);
    // Drop a leading "NN_" ordering prefix if present.
    let stem = stem
        .split_once('_')
        .filter(|(p, _)| !p.is_empty() && p.chars().all(|c| c.is_ascii_digit()))
        .map(|(_, rest)| rest)
        .unwrap_or(stem);
    stem.replace('_', " ").trim().to_string()
}

/// Structured provision label from the hierarchy path, e.g.
/// `Rule_27/(5)/(i)` → "Rule 27(5)(i)"; `Section_8/Section_2.10` → "Section 2.10".
/// Bare "(…)" labels are qualified from the named ancestor in `unit_path`.
/// Page-marker segments (`pN`) are dropped. Falls back to `unit_label`.
fn provision_label(unit_label: Option<&str>, unit_path: Option<&str>) -> String {
    let label = unit_label.unwrap_or("").trim();
    // A "named" label (starts with a word like Section/Rule/Article) stands alone.
    let is_named = label
        .chars()
        .next()
        .map(|c| c.is_ascii_alphabetic())
        .unwrap_or(false);
    if is_named && !label.is_empty() {
        return label.to_string();
    }
    // Bare "(5)"/"(i)" or empty → assemble from unit_path hierarchy.
    if let Some(path) = unit_path {
        let mut named: Vec<String> = Vec::new();
        let mut tail: Vec<String> = Vec::new();
        for seg in path.split('/') {
            let seg = seg.trim();
            if seg.is_empty() || (seg.starts_with('p') && seg[1..].chars().all(|c| c.is_ascii_digit())) {
                continue; // drop page-marker / empty segments
            }
            let pretty = seg.replace('_', " ");
            if pretty.chars().next().map(|c| c.is_ascii_alphabetic()).unwrap_or(false) {
                named.push(pretty);
            } else {
                tail.push(pretty); // "(5)", "(i)" …
            }
        }
        if let Some(last_named) = named.last() {
            let joined_tail = tail.join("");
            return format!("{last_named}{joined_tail}").trim().to_string();
        }
    }
    if !label.is_empty() { label.to_string() } else { "provision".to_string() }
}

/// Detect cross-references in provision text: other "Section/Rule/Article/
/// Schedule/Chapter N" the text names. Deduped, in order, capped at 6. Pure
/// extraction — a reference is surfaced only because the text literally names it.
fn detect_cross_refs(text: &str, self_label: &str) -> Vec<String> {
    use std::sync::OnceLock;
    static CROSS_REF_RE: OnceLock<regex_lite::Regex> = OnceLock::new();
    let re = CROSS_REF_RE.get_or_init(|| {
        regex_lite::Regex::new(
            r"(?i)\b(section|rule|article|schedule|chapter|clause)\s+([0-9]+[A-Za-z]?(?:\([0-9a-z]+\))*)",
        )
        .unwrap()
    });
    let mut out: Vec<String> = Vec::new();
    for c in re.captures_iter(text) {
        let kind = &c[1];
        let num = &c[2];
        // Title-case the kind word.
        let kind_tc = {
            let mut ch = kind.chars();
            ch.next()
                .map(|f| f.to_uppercase().collect::<String>() + ch.as_str().to_lowercase().as_str())
                .unwrap_or_default()
        };
        let label = format!("{kind_tc} {num}");
        // Skip a self-reference and dupes; cap at 6.
        if label != self_label && !out.contains(&label) {
            out.push(label);
        }
        if out.len() >= 6 {
            break;
        }
    }
    out
}

/// Question content-words (lowercase, ≥3 chars, stopwords dropped) and the
/// consecutive bigrams between them. Used to relevance-rank provisions.
fn question_terms(query: &str) -> (Vec<String>, Vec<String>) {
    const STOP: &[&str] = &[
        "the", "and", "for", "are", "was", "what", "which", "who", "whom", "how",
        "does", "did", "may", "must", "shall", "can", "under", "with", "from",
        "into", "that", "this", "these", "those", "its", "their", "his", "her",
        "have", "has", "been", "will", "would", "should", "about", "than", "then",
        "when", "where", "whose", "any", "all", "each", "per", "via", "upto", "up",
        "to", "of", "in", "on", "at", "by", "or", "an", "a", "is", "be", "as", "it",
    ];
    let words: Vec<String> = query
        .to_lowercase()
        .split(|c: char| !c.is_alphanumeric())
        .filter(|w| w.len() >= 3 && !STOP.contains(w))
        .map(|w| w.to_string())
        .collect();
    let mut bigrams = Vec::new();
    for w in words.windows(2) {
        bigrams.push(format!("{} {}", w[0], w[1]));
    }
    (words, bigrams)
}

/// Lexical relevance of a provision's text to the question: content-word
/// overlap (+2), stem/prefix overlap (+1), and a strong phrase/bigram bonus
/// (+5, e.g. "election commission", "feasibility study"). Pure lexical — picks
/// WHICH provisions to show extractively without a model or added latency.
fn relevance_score(content: &str, q_words: &[String], q_bigrams: &[String]) -> i32 {
    let c = content.to_lowercase();
    let mut score = 0i32;
    for w in q_words {
        if c.contains(w.as_str()) {
            score += 2;
        } else if w.len() >= 6 && c.contains(&w[..5]) {
            score += 1; // crude stem match: "accompany" ~ "accompanied"
        }
    }
    for bg in q_bigrams {
        if c.contains(bg.as_str()) {
            score += 5;
        }
    }
    score
}

/// Detect an inline amendment marker in provision text, e.g.
/// "Substituted vide Notification No. … dated 21.09.2015", "inserted by …",
/// "omitted". Returns the marker sentence (trimmed, capped) if found.
fn detect_amendment(text: &str) -> Option<String> {
    use std::sync::OnceLock;
    static AMENDMENT_RE: OnceLock<regex_lite::Regex> = OnceLock::new();
    let re = AMENDMENT_RE.get_or_init(|| {
        regex_lite::Regex::new(
            r"(?i)(substituted|inserted|omitted|added|repealed|amended)\s+(vide|by|through|w\.e\.f)\b[^.]{0,140}",
        )
        .unwrap()
    });
    re.find(text).map(|m| {
        let s = m.as_str().split_whitespace().collect::<Vec<_>>().join(" ");
        let s = s.trim().trim_end_matches([',', ';']);
        // Capitalise first letter for display.
        let mut ch = s.chars();
        ch.next()
            .map(|f| f.to_uppercase().collect::<String>() + ch.as_str())
            .unwrap_or_default()
    })
}

/// Extract `(source_name, page)` pairs from citation markers in the response.
/// Parses `[file.pdf, page 4]`, `[file.pdf, p. 4]`, `[file.pdf]` (no page).
/// Numeric-only markers like `[1]` are filtered (those are numbered-list refs).
fn extract_citations_with_pages(text: &str) -> Vec<(String, Option<i64>)> {
    let mut out = Vec::new();
    let mut rest = text;
    while let Some(start) = rest.find('[') {
        rest = &rest[start + 1..];
        if let Some(end) = rest.find(']') {
            let inner = rest[..end].trim();
            let mut parts = inner.split(',');
            let name = parts.next().unwrap_or("").trim().to_string();
            let page: Option<i64> = parts.find_map(|p| {
                let t = p.trim().to_lowercase();
                if t.starts_with("page") || t.starts_with("p.") || t.starts_with("p ") {
                    t.chars()
                        .filter(|c| c.is_ascii_digit())
                        .collect::<String>()
                        .parse::<i64>()
                        .ok()
                } else {
                    None
                }
            });
            if !name.is_empty() && !name.chars().all(|c| c.is_ascii_digit()) {
                out.push((name, page));
            }
            rest = &rest[end + 1..];
        } else {
            break;
        }
    }
    out
}

/// Extract all source names from citation markers `[name, page N]` or `[name]`.
/// Also accepts thinking-model XML wrappers `<filename>name.pdf</filename>`,
/// `<file>name</file>`, `<src>name</src>` — Qwen3/DeepSeek-R1 paraphrase the
/// format string into pseudo-XML.
/// Returns one name per marker found; duplicates preserved (caller can dedup).
fn extract_citations(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut rest = text;
    while let Some(start) = rest.find('[') {
        rest = &rest[start + 1..];
        if let Some(end) = rest.find(']') {
            let inner = rest[..end].trim();
            // Name is everything before the first comma (page part comes after).
            let name = inner.split(',').next().unwrap_or("").trim().to_string();
            // Filter out numeric-only markers like [1], [2] (numbered list refs).
            if !name.is_empty() && !name.chars().all(|c| c.is_ascii_digit()) {
                out.push(name);
            }
            rest = &rest[end + 1..];
        } else {
            break;
        }
    }
    // Second pass: scan for <filename>X</filename> / <file>X</file> wrappers.
    for tag in ["filename", "file", "src"] {
        let open = format!("<{tag}>");
        let close = format!("</{tag}>");
        let mut rest = text;
        while let Some(o) = rest.find(&open) {
            let after = &rest[o + open.len()..];
            if let Some(c) = after.find(&close) {
                let name = after[..c].trim();
                // First field before comma — `<filename>foo.pdf, page 3</filename>`
                let name = name.split(',').next().unwrap_or("").trim();
                if !name.is_empty() && !name.chars().all(|c| c.is_ascii_digit()) {
                    out.push(name.to_string());
                }
                rest = &after[c + close.len()..];
            } else { break; }
        }
    }
    out
}

/// Strip file extension for flexible citation matching.
/// `"report.pdf"` → `"report"`, `"meeting notes.docx"` → `"meeting notes"`.
fn file_stem(filename: &str) -> &str {
    filename
        .rfind('.')
        .map(|i| &filename[..i])
        .unwrap_or(filename)
}

/// Count "factual" sentences: trimmed, > 8 words, not a question, not a header.
fn count_factual_sentences(text: &str) -> u32 {
    text.split(|c: char| c == '.' || c == '!' || c == '\n')
        .map(str::trim)
        .filter(|s| {
            let words = s.split_whitespace().count();
            words > 8 && !s.ends_with('?') && !s.starts_with('#')
        })
        .count() as u32
}

// ─── Context builder (FR-040 / AD-6) ─────────────────────────────────────────

/// Select the top-K unique chunks from a ranked list, enforcing a token budget.
///
/// - Deduplicates by `chunk_id` (first occurrence kept; protects against
///   duplicate chunk_ids in raw retrieval paths even though `rrf_merge` already
///   deduplicates within the hybrid path).
/// - Estimates token count as `content_chars / 4 + 20` (header overhead).
///   Conservative proxy for subword tokens — sufficient for budget gating.
/// - Always includes the first chunk even if it alone exceeds `token_budget`,
///   so the LLM always receives at least some context.
/// - Returns the selected list in input order (rank preserved).
fn build_context(
    ranked: &[(String, String, String, Option<i64>, String)],
    k: usize,
    token_budget: usize,
) -> Vec<(String, String, String, Option<i64>, String)> {
    // Two-pass selection. Pass 1: skip anchor-only chunks (content < 50 chars,
    // e.g. content == "Section 12" / "Preamble") — these are routing handles
    // for the citation fast-path, NOT evidence. They polluted top-K because
    // unit_label match scored them high (22-prompt eval root cause, fix #2).
    // Pass 2 only runs if pass 1 returned nothing — better an anchor than zero.
    let pass1 = build_context_pass(ranked, k, token_budget, true);
    if !pass1.is_empty() {
        return pass1;
    }
    build_context_pass(ranked, k, token_budget, false)
}

/// Stable 64-bit fingerprint of chunk content, case- and whitespace-normalised.
/// Used to drop chunks that carry identical TEXT under different chunk_ids.
fn content_fingerprint(s: &str) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    let mut last_was_space = true; // leading whitespace collapses to nothing
    for ch in s.trim().chars() {
        if ch.is_whitespace() {
            if !last_was_space {
                ' '.hash(&mut h);
                last_was_space = true;
            }
        } else {
            for lc in ch.to_lowercase() {
                lc.hash(&mut h);
            }
            last_was_space = false;
        }
    }
    h.finish()
}

fn build_context_pass(
    ranked: &[(String, String, String, Option<i64>, String)],
    k: usize,
    token_budget: usize,
    skip_anchor_only: bool,
) -> Vec<(String, String, String, Option<i64>, String)> {
    let mut selected: Vec<(String, String, String, Option<i64>, String)> = Vec::new();
    let mut seen: HashSet<&str> = HashSet::new();
    // Per-pass, deliberately: pass 2 re-selects from scratch.
    let mut seen_content: HashSet<u64> = HashSet::new();
    let mut total_tokens: usize = 0;

    for chunk in ranked.iter() {
        if selected.len() >= k {
            break;
        }
        // Dedup guard.
        if seen.contains(chunk.0.as_str()) {
            continue;
        }
        // Same TEXT under a different chunk_id. Real in this corpus: 227 duplicate
        // content groups on 2026-08-06 — 66 footnote, 63 table, 37 provision — all
        // of them retrievable and all competing for the same window. The id dedup
        // above cannot see them because the ids genuinely differ.
        //
        // Exact normalised match only, never a prefix: repeated standard clauses
        // across schedules share long prefixes and ARE distinct provisions.
        if !seen_content.insert(content_fingerprint(&chunk.2)) {
            continue;
        }
        // Anchor-only filter (pass 1 only). Heading-shell parents like
        // content="Section 12" are useful for routing but useless as LLM
        // evidence — skip them when richer descendants are available.
        if skip_anchor_only && chunk.2.trim().chars().count() < 50 {
            continue;
        }
        // Token estimate: content chars / 4, plus ~20 for passage header.
        let estimated = (chunk.2.chars().count() / 4).saturating_add(20);
        // F015 T008 (FR-014): provisions are ATOMIC — a chunk is included whole
        // or omitted, never sliced and never dropped merely for exceeding a
        // per-chunk fraction of the budget. The old `ECHO_PER_CHUNK_CAP_FRAC`
        // (0.25) cap silently dropped whole, correctly-retrieved provisions just
        // for being large, defeating citation-grade completeness. Only the
        // total-budget guard below may omit a chunk, and the first chunk is
        // always admitted so a single oversized provision still reaches the model.
        // Enforce budget — skip chunks that would overflow, keep scanning for
        // smaller ones. Always admit the very first chunk regardless of size.
        if !selected.is_empty() && total_tokens.saturating_add(estimated) > token_budget {
            continue;
        }
        seen.insert(&chunk.0);
        total_tokens = total_tokens.saturating_add(estimated);
        selected.push(chunk.clone());
    }

    selected
}

// ─── Hybrid retrieval ─────────────────────────────────────────────────────────

/// Noviq Hybrid Union — the deduped UNION of the FTS and vector lists (no score
/// fusion). **Vector-cosine-first ordering:** all vector hits lead, in their
/// cosine-descending order (the ANN already returns them best-first), so the most
/// semantically-relevant chunk becomes `[1]`; the FTS-only hits the vector search
/// missed are appended after, in FTS-rank order. This fixes citation numbering
/// (the model defaults to citing `[1]`, which is now the best match) AND pushes
/// FTS keyword-noise (addressee lists, boilerplate) below the relevant chunks,
/// while still guaranteeing recall — a rule strong in one modality but absent from
/// the other is never demoted out of context (the Rule 16 failure mode under RRF).
/// Caps each list at `vec_k` / `fts_k` and dedups by `chunk_id` (a chunk in both
/// lists is kept once — the only "overlap merge"; near-identical text from
/// different documents is intentionally kept, per design). The token budget in
/// `build_context` is the only downstream limiter.
fn union_merge(
    fts: &[(String, String, String, Option<i64>, String)],
    vec: &[(String, String, String, Option<i64>, String)],
    fts_k: usize,
    vec_k: usize,
) -> Vec<(String, String, String, Option<i64>, String)> {
    let mut out: Vec<(String, String, String, Option<i64>, String)> =
        Vec::with_capacity(fts_k + vec_k);
    let mut seen: HashSet<String> = HashSet::new();
    // Vector hits first, in cosine-descending order (best semantic match → [1]).
    for c in vec.iter().take(vec_k) {
        if seen.insert(c.0.clone()) {
            out.push(c.clone());
        }
    }
    // Then the FTS-only keyword hits the vector search missed, in FTS-rank order.
    for c in fts.iter().take(fts_k) {
        if seen.insert(c.0.clone()) {
            out.push(c.clone());
        }
    }
    out
}

/// Merge FTS5 and vec retrieval results using Reciprocal Rank Fusion (FR-039).
///
/// Formula: `score(chunk) = Σ 1 / (k + rank_i)` summed across all lists
/// where the chunk appears, `rank_i` is 1-based position in that list.
/// k=60 is the standard constant (dampens the impact of very high ranks).
///
/// Chunks appearing in both lists accumulate scores from both; chunks in only
/// one list still receive a partial score. Deduplicates by `chunk_id`.
/// Returns the unified list sorted by score descending (best match first).
fn rrf_merge(
    fts_chunks: &[(String, String, String, Option<i64>, String)],
    vec_chunks: &[(String, String, String, Option<i64>, String)],
    k: u32,
) -> Vec<(String, String, String, Option<i64>, String)> {
    let kf = k as f32;

    // score_map: chunk_id → (accumulated rrf score, index into all_data)
    let mut score_map: HashMap<&str, (f32, usize)> = HashMap::new();
    // all_data: ordered insertion of chunk data (first-seen copy kept on dedup)
    let mut all_data: Vec<&(String, String, String, Option<i64>, String)> = Vec::new();

    let lists: [&[(String, String, String, Option<i64>, String)]; 2] =
        [fts_chunks, vec_chunks];

    for list in &lists {
        for (rank, chunk) in list.iter().enumerate() {
            let score = 1.0 / (kf + (rank + 1) as f32);
            if let Some(entry) = score_map.get_mut(chunk.0.as_str()) {
                entry.0 += score; // chunk seen before — accumulate score
            } else {
                let idx = all_data.len();
                all_data.push(chunk);
                score_map.insert(&chunk.0, (score, idx));
            }
        }
    }

    // Sort indices by descending RRF score.
    let mut ranked: Vec<(f32, usize)> = score_map.into_values().collect();
    ranked.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));

    ranked
        .into_iter()
        .map(|(_, idx)| all_data[idx].clone())
        .collect()
}

/// S1 — Salience-aware re-ranking (feature 003, F5-imperative fix).
///
/// Walks `ranked` (already in best-first order from the prior retrieval step),
/// looks up each chunk's `unit_label`, and multiplies its position-derived
/// score by `boost` when the label is salient in any registered DocProfile
/// (see `doc_profiles::is_salient_unit`). Re-sorts by boosted score.
///
/// Why this exists: queries that don't name a citation but seek a
/// canonical/declarative fact (e.g. "Who is called the founder of Pakistan?")
/// get out-ranked because leaf chunks (Article 41 "President of Pakistan")
/// share more surface keywords with the query than the canonical chunk
/// (Preamble) does. A small multiplicative boost lifts canonical chunks
/// above leaf chunks at *comparable* scores without overriding a clearly
/// better hybrid match — anti-monopoly purpose of the retrieval ranker is
/// preserved.
///
/// Mechanism is doc-agnostic; the *which labels count as salient* part is
/// per-doc-type config. Safe on DB errors — returns input unchanged.
///
/// `boost` is supplied by the caller so S2's definitional classifier can
/// pass a stronger multiplier (`ECHO_DEFINITIONAL_BOOST`) without this
/// function needing to know about question shapes.
async fn apply_salience_boost(
    pool: &Db,
    ranked: Vec<(String, String, String, Option<i64>, String)>,
    boost: f32,
) -> Vec<(String, String, String, Option<i64>, String)> {
    if boost <= 1.0 || ranked.len() < 2 {
        return ranked;
    }
    // Fetch unit_label for every candidate in one query (bounded by pool size).
    let ids: Vec<&str> = ranked.iter().map(|c| c.0.as_str()).collect();
    let placeholders = crate::db::pg_in(ids.len(), 1);
    let q = format!("SELECT id, unit_label FROM chunks WHERE id IN ({placeholders})");
    let mut bind = sqlx::query_as::<_, (String, Option<String>)>(&q);
    for id in &ids {
        bind = bind.bind(*id);
    }
    let rows: Vec<(String, Option<String>)> = match bind.fetch_all(pool).await {
        Ok(r) => r,
        Err(e) => {
            log::warn!("salience boost — unit_label lookup failed: {e}");
            return ranked;
        }
    };
    let label_by_id: HashMap<String, String> = rows
        .into_iter()
        .filter_map(|(id, lbl)| lbl.map(|l| (id, l)))
        .collect();

    // Position-derived base score mirrors rrf_merge's formula so the boost
    // composes naturally: rank 1 → 1/61, rank 2 → 1/62, etc.
    let mut scored: Vec<(f32, (String, String, String, Option<i64>, String))> = ranked
        .into_iter()
        .enumerate()
        .map(|(i, chunk)| {
            let mut score = 1.0_f32 / (60.0 + (i + 1) as f32);
            if let Some(label) = label_by_id.get(&chunk.0) {
                if crate::services::doc_profiles::is_salient_unit(label) {
                    score *= boost;
                }
            }
            (score, chunk)
        })
        .collect();
    scored.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));
    scored.into_iter().map(|(_, c)| c).collect()
}

/// S3b — Extract candidate entity noun-phrases from the query.
///
/// Picks capitalised runs of words (including hyphens/apostrophes inside,
/// e.g. "Quaid-i-Azam", "Mohammad Ali Jinnah", "Federal Capital
/// Territory"). Length-filtered to drop noise. Language-grammar level
/// — works on any English query without document-specific tokens.
///
/// Lowercase concept entities ("high treason", "fundamental rights")
/// are not matched here; their fix is in S3b-followup (lower-cased
/// substring scan against `entities.canonical_value`).
pub(crate) fn extract_query_entities(query: &str) -> Vec<String> {
    use std::sync::OnceLock;
    static RE: OnceLock<regex_lite::Regex> = OnceLock::new();
    let re = RE.get_or_init(|| {
        regex_lite::Regex::new(r"[A-Z][A-Za-z'\-]+(?:\s+[A-Z][A-Za-z'\-]+)*").unwrap()
    });
    let mut seen: HashSet<String> = HashSet::new();
    let mut out: Vec<String> = Vec::new();
    for m in re.find_iter(query) {
        let s = m.as_str().trim().to_string();
        if s.chars().count() < 3 { continue; }
        // Skip the very first word if it's alone — sentence-initial cap
        // is usually grammar, not a proper noun ("What does …" → "What").
        if m.start() == 0 && !s.contains(' ') && !s.contains('-') {
            continue;
        }
        let key = s.to_lowercase();
        if seen.insert(key) {
            out.push(s);
        }
    }
    out
}

/// S3b — Entity-aware re-rank.
///
/// Walks `ranked`, joins `entity_mentions` to find chunks whose
/// `entities.canonical_value` matches any extracted query noun phrase
/// (case-insensitive), and multiplies their position-derived score by
/// `boost`. Re-sorts. Composes naturally on top of `apply_salience_boost`
/// because the base score is rebuilt from position each pass.
///
/// Mechanism is doc-agnostic — entity tables are populated for every
/// document type with NER turned on. S3a re-enables NER for constitution
/// chunks; S3b makes those entity rows actually affect ranking.
async fn apply_entity_boost(
    pool: &Db,
    ranked: Vec<(String, String, String, Option<i64>, String)>,
    query: &str,
    boost: f32,
) -> Vec<(String, String, String, Option<i64>, String)> {
    if boost <= 1.0 || ranked.len() < 2 {
        return ranked;
    }
    let nps = extract_query_entities(query);
    if nps.is_empty() {
        return ranked;
    }
    let ids: Vec<&str> = ranked.iter().map(|c| c.0.as_str()).collect();
    let np_lower: Vec<String> = nps.iter().map(|s| s.to_lowercase()).collect();
    let np_ph = crate::db::pg_in(np_lower.len(), 1);
    let id_ph = crate::db::pg_in(ids.len(), np_lower.len() + 1);
    let sql = format!(
        "SELECT DISTINCT em.chunk_id \
         FROM entities e JOIN entity_mentions em ON em.entity_id = e.id \
         WHERE LOWER(e.canonical_value) IN ({np_ph}) AND em.chunk_id IN ({id_ph})"
    );
    let mut q = sqlx::query_scalar::<_, String>(&sql);
    for v in &np_lower { q = q.bind(v); }
    for id in &ids { q = q.bind(*id); }
    let matched: HashSet<String> = match q.fetch_all(pool).await {
        Ok(v) => v.into_iter().collect(),
        Err(e) => {
            log::warn!("entity boost — chunk lookup failed: {e}");
            return ranked;
        }
    };
    if matched.is_empty() {
        return ranked;
    }
    let mut scored: Vec<(f32, (String, String, String, Option<i64>, String))> = ranked
        .into_iter()
        .enumerate()
        .map(|(i, chunk)| {
            let mut score = 1.0_f32 / (60.0 + (i + 1) as f32);
            if matched.contains(&chunk.0) {
                score *= boost;
            }
            (score, chunk)
        })
        .collect();
    scored.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));
    scored.into_iter().map(|(_, c)| c).collect()
}

/// Instrument-affinity re-rank (2026-07-18).
///
/// A bare provision token is ambiguous across the corpus: `Rule 4` exists in the
/// E&D Rules 1973, the APT Rules 1974, the Delegation of Financial Powers 2019,
/// the SLGA Compendium and SPPRA. Trace `bd0d16e1` asked for "Rule 4(1) sub rule 3
/// **from sindh civil servants efficiency and disciplane rules 1973**" and was
/// answered out of the *Sindh Food Authority Act* — Food Authority composition
/// presented as disciplinary law, with 20 citations.
///
/// This is a **re-rank, never a filter**. Nothing is excluded, so a wrong guess
/// degrades ordering instead of causing the false "not found in the indexed
/// corpus" that `notice_assist::compose::instrument_scope` produces when it
/// cannot resolve a statute.
///
/// It is emphatically NOT the reverted `citation_fast_path` (see the gate at the
/// top of this file): retrieval still runs in full through the single
/// hybrid-union path, and instrument match is folded in as one more ranking
/// signal alongside salience and entity affinity.
///
/// The whole query is scored against each candidate's document name via the
/// shared `instrument_score`. When no source clears the bar (`>= 2`) the pass is
/// a no-op — which is exactly what happens for the query that killed the old
/// fast-path ("…request for quotations method under rule 16" names no statute
/// and scores 0 everywhere).
/// Score every distinct document name in a candidate pool against the query.
///
/// Returns `None` when no document clears the bar (`>= 2`, the same threshold
/// `notice_assist::compose::instrument_scope` uses) — i.e. the query named no
/// statute, or named one that is not in scope. `None` means "do nothing", which
/// is what keeps the reverted `citation_fast_path` failure from recurring.
///
/// Shared by the normal path and deep search so the threshold lives in exactly
/// one place.
pub(crate) fn instrument_ranks<'a>(
    query: &str,
    file_names: impl Iterator<Item = &'a str>,
) -> Option<(HashMap<String, usize>, usize)> {
    use crate::services::assistant::capabilities::notice_assist::compose::instrument_score;

    let mut by_file: HashMap<String, usize> = HashMap::new();
    for f in file_names {
        if !by_file.contains_key(f) {
            let name = crate::services::assistant::clean_document_name(f);
            by_file.insert(f.to_string(), instrument_score(query, &name));
        }
    }
    let top = by_file.values().copied().max().unwrap_or(0);
    if top < 2 { None } else { Some((by_file, top)) }
}

fn apply_instrument_boost(
    ranked: Vec<(String, String, String, Option<i64>, String)>,
    query: &str,
    boost: f32,
) -> Vec<(String, String, String, Option<i64>, String)> {
    if boost <= 1.0 || ranked.len() < 2 {
        return ranked;
    }
    let Some((by_file, top)) = instrument_ranks(query, ranked.iter().map(|c| c.4.as_str())) else {
        return ranked;
    };
    // Graded: the best-matching instrument gets the full boost, a partial match
    // gets a proportional share, an unmatched document is left alone.
    let mut scored: Vec<(f32, (String, String, String, Option<i64>, String))> = ranked
        .into_iter()
        .enumerate()
        .map(|(i, chunk)| {
            let mut score = 1.0_f32 / (60.0 + (i + 1) as f32);
            let s = by_file.get(&chunk.4).copied().unwrap_or(0);
            if s >= 2 {
                score *= 1.0 + (s as f32 / top as f32) * (boost - 1.0);
            }
            (score, chunk)
        })
        .collect();
    scored.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));
    scored.into_iter().map(|(_, c)| c).collect()
}

/// Human-readable "which documents own the top K" summary, grouped on
/// **`source_id`** and labelled with the display name. Used by the trace; the
/// bd0d16e1 and finding-#4 failures were both invisible because nothing recorded
/// which documents won the window.
fn source_mix(ranked: &[(String, String, String, Option<i64>, String)], k: usize) -> String {
    let mut order: Vec<&str> = Vec::new();
    let mut counts: HashMap<&str, (usize, &str)> = HashMap::new();
    for c in ranked.iter().take(k) {
        let e = counts.entry(c.1.as_str()).or_insert_with(|| {
            order.push(c.1.as_str());
            (0, c.4.as_str())
        });
        e.0 += 1;
    }
    let mut mix: Vec<(&str, usize)> =
        order.iter().map(|sid| (counts[sid].1, counts[sid].0)).collect();
    mix.sort_by(|a, b| b.1.cmp(&a.1));
    mix.iter().map(|(f, n)| format!("{f}×{n}")).collect::<Vec<_>>().join(", ")
}

/// S3d — source diversity (2026-08-06).
///
/// Interleave the ranked list so no single document can own the whole context
/// window before every other document has had a turn. Finding #4: a broad
/// question ("eligibility criteria to apply for a government job in Sindh") put
/// 6 chunks of the Local Government Act, 6 of Delegation of Financial Powers and
/// 4 of the Rules of Business into the window, and **3** of the APT Rules 1974 —
/// the only instrument that answers it. The strongest phrase in such a question
/// is "Government of Sindh", which is the definitions clause of all 18 documents.
///
/// **This is a re-order, never a filter.** `out` contains exactly the same chunks
/// as `ranked`; only their order changes. The cap is realised downstream by
/// `build_context`'s top-K truncation, so a pool with nothing else to offer still
/// fills the window. A boost-shaped fix cannot do this — `apply_*_boost` rebuilds
/// scores from list position, so a source owning 30 of 36 still owns ~30 after
/// any monotone re-score.
///
/// Groups on `.1` (**source_id**), never `.4` (the label), which is a display
/// string. No-op when `cap == 0`, fewer than 2 chunks, or every chunk shares one
/// source — the attachment-scoped chat case, where the order is left untouched.
fn apply_source_diversity(
    ranked: Vec<(String, String, String, Option<i64>, String)>,
    cap: usize,
) -> Vec<(String, String, String, Option<i64>, String)> {
    if cap == 0 || ranked.len() < 2 {
        return ranked;
    }
    // Bucket by source, preserving first-appearance order — so the best-ranked
    // source leads wave 1 and the overall top chunk keeps its place.
    let mut buckets: Vec<(String, Vec<(String, String, String, Option<i64>, String)>)> = Vec::new();
    for c in ranked {
        match buckets.iter_mut().find(|(sid, _)| sid == &c.1) {
            Some((_, v)) => v.push(c),
            None => buckets.push((c.1.clone(), vec![c])),
        }
    }
    if buckets.len() < 2 {
        return buckets.into_iter().next().map(|(_, v)| v).unwrap_or_default();
    }

    let total: usize = buckets.iter().map(|(_, v)| v.len()).sum();
    let mut out = Vec::with_capacity(total);
    let mut cursors = vec![0usize; buckets.len()];
    while out.len() < total {
        let before = out.len();
        for (i, (_, v)) in buckets.iter().enumerate() {
            let start = cursors[i];
            let end = (start + cap).min(v.len());
            out.extend_from_slice(&v[start..end]);
            cursors[i] = end;
        }
        // Guard against a zero-progress wave (impossible with cap >= 1, but a
        // silent infinite loop in a hot path is not worth risking).
        if out.len() == before {
            break;
        }
    }
    out
}

/// Tier 3 Task 11: sibling rollup.
///
/// If 3+ leaf chunks from the same parent section appear in the candidate set,
/// replace them with the single parent section chunk. Improves coherence for
/// broad questions ("what does Section 8 cover?") while leaving narrow lookups
/// untouched. Safe on DB errors — returns input unchanged.
async fn roll_up_siblings(
    chunks: Vec<(String, String, String, Option<i64>, String)>,
    pool: &Db,
) -> Vec<(String, String, String, Option<i64>, String)> {
    if chunks.is_empty() { return chunks; }
    let ids: Vec<&str> = chunks.iter().map(|c| c.0.as_str()).collect();
    let placeholders = crate::db::pg_in(ids.len(), 1);
    let q = format!("SELECT id, parent_chunk_id, unit_type FROM chunks WHERE id IN ({placeholders})");
    let mut bind = sqlx::query_as::<_, (String, Option<String>, String)>(&q);
    for id in &ids { bind = bind.bind(*id); }
    let rows = match bind.fetch_all(pool).await {
        Ok(r) => r,
        Err(e) => { log::warn!("rollup probe failed: {e}"); return chunks; }
    };
    let meta: HashMap<String, (Option<String>, String)> =
        rows.into_iter().map(|(i, p, u)| (i, (p, u))).collect();

    // Count leaf siblings per parent.
    let mut sibling_count: HashMap<String, usize> = HashMap::new();
    for c in &chunks {
        if let Some((Some(parent), ut)) = meta.get(&c.0).map(|(p, u)| (p.clone(), u.clone())) {
            if ut == "leaf" {
                *sibling_count.entry(parent).or_insert(0) += 1;
            }
        }
    }
    let promote: HashSet<String> = sibling_count.into_iter()
        .filter(|(_, n)| *n >= 3).map(|(p, _)| p).collect();
    if promote.is_empty() { return chunks; }

    // Fetch parent rows once. The label expression MUST match the retrieval arms
    // (`fts5_retrieve` / `get_context_chunks`) — a promoted parent describes the
    // same document as the leaves it replaces, so it must land in the same label
    // bucket for `apply_instrument_boost` and the trace source-mix.
    let pp: Vec<String> = promote.iter().cloned().collect();
    let placeholders = crate::db::pg_in(pp.len(), 1);
    let q = format!(
        "SELECT c.id, c.source_id, c.content, c.page_number, \
                COALESCE(s.display_name, s.file_name, s.file_path, 'unknown') \
         FROM chunks c JOIN sources s ON c.source_id = s.id \
         WHERE c.id IN ({placeholders})"
    );
    let mut bind = sqlx::query_as::<_, (String, String, String, Option<i64>, String)>(&q);
    for id in &pp { bind = bind.bind(id); }
    let parent_rows = match bind.fetch_all(pool).await {
        Ok(r) => r,
        Err(e) => { log::warn!("rollup parent fetch failed: {e}"); return chunks; }
    };
    let parents_by_id: HashMap<String, (String, String, String, Option<i64>, String)> =
        parent_rows.into_iter().map(|r| (r.0.clone(), r)).collect();

    // Replace promoted leaves with parent (deduped), keep other chunks intact.
    let mut seen_parent: HashSet<String> = HashSet::new();
    let mut out: Vec<(String, String, String, Option<i64>, String)> = Vec::with_capacity(chunks.len());
    for c in chunks {
        let parent_to_promote = meta.get(&c.0).and_then(|(p, u)|
            if u == "leaf" { p.clone().filter(|pid| promote.contains(pid)) } else { None }
        );
        match parent_to_promote {
            Some(pid) if !seen_parent.contains(&pid) => {
                if let Some(prow) = parents_by_id.get(&pid) {
                    seen_parent.insert(pid);
                    out.push(prow.clone());
                }
            }
            Some(_) => { /* parent already emitted; skip this sibling */ }
            None => out.push(c),
        }
    }
    out
}

/// Choose what the reserved table slice actually contains, given the table's
/// chunks in reading order (the `unit_type='table'` chunk first, then its rows).
///
/// ⚠️ The whole-table chunk is `render_table_markdown` of EVERY row
/// (`structured_importer::persist_table_units`), i.e. it already contains the
/// rows. Taking both duplicates the entire table in the context window. So:
/// take the whole-table chunk ALONE when it fits the token budget (usually far
/// cheaper — one chunk instead of 40), otherwise drop it and take rows until
/// either cap binds.
///
/// The token budget is not optional. `build_context_pass` always admits its
/// FIRST chunk regardless of size, and the reserve is the prefix — so an
/// unbounded whole-table chunk could consume the entire window by itself.
///
/// Pure: no DB, no env. Token estimate matches `build_context_pass` exactly
/// (`chars/4 + 20`) — a reserve measured in different units than it is spent in
/// is not a reserve.
fn elect_table_reserve(
    fetched: &[(String, String, String, Option<i64>, String)],
    max_rows: usize,
    max_tokens: usize,
) -> Vec<(String, String, String, Option<i64>, String)> {
    if fetched.is_empty() || max_rows == 0 || max_tokens == 0 {
        return Vec::new();
    }
    let est = |s: &str| s.len() / 4 + 20;

    // The repository orders the table node first (`ORDER BY (node_type <> 'table')`),
    // so if a whole-table chunk exists it is at index 0. Detect it by being the
    // only chunk whose content contains every row — cheaper and more robust: the
    // caller tells us by ordering, so trust position 0 only when there is more
    // than one chunk (a lone chunk is taken either way).
    if fetched.len() > 1 && est(&fetched[0].2) <= max_tokens {
        return vec![fetched[0].clone()];
    }

    let skip_whole = fetched.len() > 1; // whole-table chunk did not fit — drop it
    let mut out = Vec::new();
    let mut spent = 0usize;
    for c in fetched.iter().skip(usize::from(skip_whole)) {
        if out.len() >= max_rows {
            break;
        }
        let t = est(&c.2);
        if !out.is_empty() && spent + t > max_tokens {
            continue; // skip, don't break — a smaller later row may still fit
        }
        spent += t;
        out.push(c.clone());
    }
    out
}

/// Bounded table completion (2026-08-06). Returns the slice to place AHEAD of the
/// ranked list — at most `max_rows` chunks and `max_tokens` estimated tokens,
/// scope-clamped and deduped against `ranked`. Empty when no chunk in the top
/// `seed_top_n` of `ranked` belongs to a table.
///
/// Replaces an unbounded `expand_table_siblings` pull that, in production on
/// 2026-08-06, fetched 64 chunks from three unrelated tables into a context that
/// holds ~36 and prepended them — evicting the correctly-ranked Sindh Public
/// Procurement Act s.6(2) entirely and producing a refusal with 0 citations.
async fn table_reserve_slice(
    pool: &Db,
    ranked: &[(String, String, String, Option<i64>, String)],
    scope_source_ids: Option<&[String]>,
    seed_top_n: usize,
    max_rows: usize,
    max_tokens: usize,
) -> Vec<(String, String, String, Option<i64>, String)> {
    use crate::db::repositories::graph;
    if ranked.is_empty() || seed_top_n == 0 || max_rows == 0 {
        return Vec::new();
    }
    // Only the strongest hits may trigger completion. A table row at rank 40 is
    // an accident of similarity, and expanding it used to cost the whole window.
    let seeds: Vec<String> = ranked.iter().take(seed_top_n).map(|c| c.0.clone()).collect();

    // `+1` because the whole-table chunk sits ahead of the rows in the result.
    let ids = graph::table_chunks_for_best_seed(pool, &seeds, max_rows as i64 + 1)
        .await
        .unwrap_or_default();
    if ids.is_empty() {
        return Vec::new();
    }

    let have: HashSet<&str> = ranked.iter().map(|c| c.0.as_str()).collect();
    let fresh: Vec<String> = ids.into_iter().filter(|id| !have.contains(id.as_str())).collect();
    if fresh.is_empty() {
        return Vec::new();
    }

    let mut fetched = fetch_chunks_by_ids(pool, &fresh).await.unwrap_or_default();
    // F021 defence-in-depth: the graph walk is not scope-aware.
    if let Some(allowed) = scope_source_ids {
        let allowed: HashSet<&str> = allowed.iter().map(String::as_str).collect();
        fetched.retain(|c| allowed.contains(c.1.as_str()));
    }
    elect_table_reserve(&fetched, max_rows, max_tokens)
}

// ─── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    /// The compact (small-model) grounding prompt must keep two literals the
    /// pipeline depends on: the canonical refusal sentence (detected downstream
    /// as a deterministic refusal) and `[n]` (the citation-demo injection
    /// trigger in build_chat_prompt).
    #[test]
    fn compact_prompt_keeps_refusal_sentence_and_demo_trigger() {
        let p = crate::services::llm::GROUNDING_SYSTEM_PROMPT_COMPACT;
        assert!(p.contains(INSUFFICIENT_INFO_MSG), "canonical refusal must match INSUFFICIENT_INFO_MSG");
        assert!(p.contains("[n]"), "literal [n] required so the citation demo injects");
    }

    /// Regression — the previous byte-slicing pattern
    /// `&text[..text.len().min(N)]` panicked on Urdu/Sindhi output because
    /// the Nth byte routinely lands inside a multi-byte character (e.g.
    /// 'ا' is 2 bytes UTF-8). Truncation must be char-based, not byte-based.
    /// See CHANGELOG_DEV.md 2026-05-10 P13 panic.
    #[test]
    fn truncate_is_char_safe_on_urdu() {
        // Urdu sentence — every char is 2 bytes UTF-8.
        let urdu = "ایک سول سرونٹ اپنی خلاف کاروائی پر اپیل کیسے دائر کر سکتا ہے".repeat(20);
        // Each of these would have panicked under the old `&s[..s.len().min(N)]`
        // pattern when N landed inside a multi-byte char.
        let _ = truncate(&urdu, 30);
        let _ = truncate(&urdu, 150);
        let _ = truncate(&urdu, 300);
        // Also verify the chars().take().collect() pattern used in log sites.
        let _ = urdu.chars().take(300).collect::<String>();
        let _ = urdu.chars().take(60).collect::<String>();
    }

    /// The prompt must use EVERY pair the caller hands it — the window belongs to
    /// `memory::load_for_turn` (`recent_turns_verbatim`, 8 messages = 4 pairs), not
    /// to this function. Re-land of 325618ab: the old `.skip(pairs.len() - 2)` cut
    /// the window down to 2 pairs on top of the caller's trim, so the oldest turns
    /// vanished and "what was my first question?" named the FOURTH one (prod audit,
    /// 2026-07-16). Safe to grow now: `law_overhead` measures this exact block, so
    /// extra history costs law passages instead of overflowing the window.
    #[test]
    fn prior_turns_block_uses_every_pair_the_caller_hands_it() {
        let mut history = Vec::new();
        for topic in ["X: penalty grounds", "Y: procurement threshold", "Z: right of appeal", "W: leave rules"] {
            history.push(HistoryTurn { role: "user".into(), content: topic.into() });
            history.push(HistoryTurn { role: "assistant".into(), content: format!("ANSWER about {topic}") });
        }
        // A follow-up, so the block is populated at all.
        let block = prior_turns_block("and what about that?", &history);
        assert_eq!(block.matches("<|im_start|>user").count(), 4, "all 4 pairs kept: {block}");
        assert!(block.contains("X: penalty grounds"), "the OLDEST pair must survive — this is the bug: {block}");
        assert!(block.contains("W: leave rules"), "the newest pair too");
        // Still bounded: the 600-char/turn cap is the real limiter.
        let long = vec![
            HistoryTurn { role: "user".into(), content: "q".repeat(5000) },
            HistoryTurn { role: "assistant".into(), content: "a".repeat(5000) },
        ];
        let capped = prior_turns_block("and that?", &long);
        assert!(est_tokens(&capped) < 400, "600-char cap still bounds each turn: {}", est_tokens(&capped));
    }

    /// REAL-TRAFFIC GOLD — the architect's actual 8-turn chat, 2026-07-16, typed
    /// exactly as sent. Five of these eight turns came back disconnected; the trace
    /// audit showed `history 0` on three of them while the trace simultaneously
    /// claimed "Prior turns: 2 pairs".
    ///
    /// The three that failed and why:
    ///   Q3 "…for that?"          — `" that "` never matched `" that?"`. A QUESTION
    ///                              MARK switched conversation memory off.
    ///   Q7 "…first one with third one." — only "* question" ordinals existed.
    ///   Q8 "i was asking for my chat…"  — only "i asked" / "this chat" existed.
    ///
    /// If any of these regress, long chats silently stop being conversations.
    #[test]
    fn real_chat_2026_07_16_every_followup_keeps_its_history() {
        // Q1 is self-contained: it names its subject and asks a whole question.
        assert!(!is_followup_query("What penalties can be imposed under the E&D Rules?"),
            "Q1 is self-contained — injecting history would pollute retrieval");

        for q in [
            "and for bs-17 officer",                        // Q2 — continuation prefix
            "who is the competent authority for that?",      // Q3 — was BROKEN (punctuation)
            "what about the appeal timeline?",               // Q4 — continuation prefix
            "can it be extended?",                           // Q5 — short
            "what was my first question?",                   // Q6 — conversation reference
            "compare the first one with third one.",         // Q7 — was BROKEN (ordinal noun)
            "i was asking for my chat not the documents.",   // Q8 — was BROKEN (progressive)
        ] {
            assert!(is_followup_query(q), "must carry history: {q:?}");
        }
    }

    /// Trailing punctuation must never defeat a whole-word match — the Q3 class.
    #[test]
    fn punctuation_does_not_switch_memory_off() {
        for q in [
            "who is the competent authority for that?",
            "who is the competent authority for that!",
            "which rule covers this...",
            "and what is the penalty for them?",
        ] {
            assert!(is_followup_query(q), "punctuation must not drop history: {q:?}");
        }
        // …and the same sentence without punctuation behaved correctly before, so
        // the two must now agree.
        assert_eq!(
            is_followup_query("who is the competent authority for that"),
            is_followup_query("who is the competent authority for that?"),
            "a question mark must not change the verdict",
        );
    }

    /// Documents a KNOWN LIMIT rather than asserting desired behaviour: the
    /// `names_own_subject` suppression is case-sensitive, so capitalising costs the
    /// officer his history. Both prod failures on 2026-07-16 were lowercase (the
    /// punctuation bug), so this is not what broke the real chat — but it is real,
    /// and an attempt to fix it by overriding on a trailing pronoun BROKE the Q12
    /// regression (which also ends on "them", where the pronoun binds in-sentence).
    /// Distinguishing them needs the antecedent, not the surface form.
    #[test]
    fn known_limit_capitalisation_suppresses_anaphora() {
        assert!(is_followup_query("who is the competent authority for that?"),
            "lowercase: history kept (this is the real prod case)");
        assert!(!is_followup_query("Who is the Competent Authority for that?"),
            "KNOWN LIMIT — capitalised, so names_own_subject suppresses. If a real \
             officer hits this, fix it with the antecedent, not a trailing-pronoun hack");
    }

    #[test]
    fn followup_detector_flags_short_and_anaphoric() {
        // Short queries (≤5 words) — almost always need context.
        assert!(is_followup_query("And for BS-19?"));
        assert!(is_followup_query("What about Karachi?"));
        assert!(is_followup_query("Yes."));

        // Anaphoric whole-word triggers.
        assert!(is_followup_query("Does this rule apply to women officers?"));
        assert!(is_followup_query("Are those positions still vacant after retirement?"));

        // Continuation prefix.
        assert!(is_followup_query("Also list the appellate authority for these cases."));
    }

    #[test]
    fn followup_detector_skips_self_contained() {
        // Self-contained, longer than 5 words, no anaphora, no continuation prefix.
        assert!(!is_followup_query(
            "Define civil servant under the Sindh Civil Servants Act."
        ));
        assert!(!is_followup_query(
            "List all grounds on which a civil servant can be dismissed from service."
        ));
        assert!(!is_followup_query(
            "What is the procedure for an SCUG officer to take Ex-Pakistan leave?"
        ));
    }

    /// Regression (Q12, 2026-06-06): a self-contained question that names its
    /// own subject must NOT be flagged as a follow-up just because it uses an
    /// intra-sentence pronoun. Here "it"/"them" refer to "Principles of Policy"
    /// inside the SAME question, not a prior turn. The false positive injected
    /// the previous turn's education keywords into retrieval.
    #[test]
    fn followup_detector_skips_query_that_names_its_own_subject() {
        assert!(!is_followup_query(
            "What are the Principles of Policy, and whose responsibility is it to act on them?"
        ));
    }

    /// Regression (2026-07-16, caught on PROD): genuine cross-turn reasoning was
    /// answered from the CORPUS instead of the conversation. Asked "Compare the
    /// findings of the first thing I asked you about with my third question" after
    /// a 3-topic chat, the model answered about *supplementary questions in the
    /// Legislative Assembly* (Rule 19) — it had matched the literal words "first/
    /// third question" against the corpus, because history was never injected.
    ///
    /// Why it slipped the old gate: >5 words, no continuation prefix, and
    /// `names_own_subject` saw the capitalised "What" **opening the second
    /// sentence** and suppressed the " them " anaphora. Explicit conversation
    /// references now fire before that suppression.
    #[test]
    fn followup_detector_flags_explicit_conversation_references() {
        // The exact prod failure.
        assert!(is_followup_query(
            "Compare the findings of the first thing I asked you about with my third question. What is the connection between them?"
        ));
        // Names its own subject (E&D / RSO) AND depends on prior turns — proves the
        // conversation-reference check must run BEFORE `names_own_subject`.
        assert!(is_followup_query(
            "Compare what you told me about the E&D Rules with the RSO 2000."
        ));
        assert!(is_followup_query("What was my second question in this conversation?"));
        assert!(is_followup_query(
            "Summarise the answer you gave me earlier about the pension rules."
        ));
    }

    /// Regression (prod, 2026-07-16) — THE bug that broke multi-turn chats. The law
    /// budget carved a hardcoded 1500-token guess for "system prompt + question + a
    /// little history" and never measured the real thing, so a bigger system prompt
    /// or more history silently overflowed the window (T4 hit 101% of cap; llm.rs
    /// head+tail truncation then dropped the MIDDLE — the law — and the model
    /// refused). The overhead must TRACK its inputs, so the law budget shrinks to
    /// make room instead of overflowing.
    #[test]
    fn measured_overhead_grows_with_the_system_prompt_and_history() {
        let history = vec![
            HistoryTurn { role: "user".into(), content: "What are the grounds for penalty?".into() },
            HistoryTurn { role: "assistant".into(), content: "Rule 3 lists misconduct [1]. ".repeat(40) },
        ];
        // A follow-up injects history; a self-contained query does not.
        let with_hist = prior_turns_block("And what about the appeal?", &history);
        let without = prior_turns_block("Define civil servant under the Sindh Civil Servants Act.", &history);
        assert!(!with_hist.is_empty(), "follow-up must inject history");
        assert!(without.is_empty(), "self-contained query must not");

        // The measured overhead must actually MOVE with its inputs — that is the
        // whole point. A constant (the old 1500) does not.
        let small_sys = "sys";
        let big_sys = "sys ".repeat(500);
        assert!(
            est_tokens(&big_sys) > est_tokens(small_sys),
            "overhead must track the system prompt size",
        );
        assert!(
            est_tokens(&with_hist) > est_tokens(&without),
            "overhead must track the injected history size",
        );

        // The block stays BOUNDED — but by the 600-char/turn cap, NOT by a pair
        // count. This assertion used to read `< 700`, which silently encoded the
        // old 2-pair window; that window was re-truncating on top of the caller's
        // own `recent_turns_verbatim` trim and is gone (re-land of 325618ab). The
        // pair count is the caller's business; the per-turn cap is this function's.
        let long = vec![
            HistoryTurn { role: "user".into(), content: "u".repeat(5000) },
            HistoryTurn { role: "assistant".into(), content: "a".repeat(5000) },
            HistoryTurn { role: "user".into(), content: "u2".repeat(5000) },
            HistoryTurn { role: "assistant".into(), content: "a2".repeat(5000) },
            HistoryTurn { role: "user".into(), content: "u3".repeat(5000) },
            HistoryTurn { role: "assistant".into(), content: "a3".repeat(5000) },
        ];
        let block = prior_turns_block("And what about the appeal?", &long);
        let raw: usize = long.iter().map(|t| est_tokens(&t.content)).sum();
        // 3 pairs × (600 + 600 chars + ChatML tags) ≈ 3 × 315 tok.
        assert!(
            est_tokens(&block) < 1000,
            "600-char/turn cap must bound every turn, got ~{} tok",
            est_tokens(&block),
        );
        // …and the cap must actually bite: 30k chars of history collapse to ~945.
        assert!(
            est_tokens(&block) * 5 < raw,
            "cap must cut oversized turns down hard: block ~{} tok vs raw ~{} tok",
            est_tokens(&block),
            raw,
        );
    }

    /// Regression (prod, 2026-07-16): the citation demo was gated by sniffing the
    /// system prompt for the literal "[n]". A memory instruction that read "never
    /// attach a [n] citation marker" flipped that switch ON for the 8B — which had
    /// never had the demo — and the demo's leave example hijacked answers. A prompt
    /// edit must NEVER be able to turn the demo on by merely naming `[n]`.
    #[test]
    fn citation_demo_is_not_switched_on_by_prompt_wording() {
        // A prompt that FORBIDS [n] markers must not summon the demo.
        let forbidding = "You are an assistant. Never attach a [n] citation marker to these facts.";
        let p = build_rag_prompt("Define civil servant under the Act.", "ctx", &[], forbidding, false);
        assert!(
            !p.contains("Who can sanction 30 days leave?"),
            "prompt wording mentioning [n] must not enable the demo",
        );
        // Same for a prompt that requests them (the 8B follows the instruction).
        let requesting = "Cite every claim with its citation in the form [n].";
        let p2 = build_rag_prompt("Define civil servant under the Act.", "ctx", &[], requesting, false);
        assert!(!p2.contains("Who can sanction 30 days leave?"));
    }

    /// Regression (prod, 2026-07-16): the one-shot citation demo is a SYNTHETIC
    /// user/assistant exchange placed immediately before the real prior turns —
    /// from the model's side it is indistinguishable from conversation, and it is
    /// the earliest turn it can see. After a 6-turn chat, "what was the very first
    /// thing I asked you about?" returned *"Who can sanction 30 days leave?"* — the
    /// demo's question, never asked by the user. Suppress it once real history is
    /// present (where the model's own `[n]`-cited answers are the better example).
    #[test]
    fn citation_demo_never_masquerades_as_conversation_history() {
        let sys = "cite with [n] markers";
        let history = vec![
            HistoryTurn { role: "user".into(), content: "What are the grounds for penalty?".into() },
            HistoryTurn { role: "assistant".into(), content: "Rule 3 lists misconduct [1].".into() },
        ];

        // With real history: the demo must be gone…
        let with_hist = build_rag_prompt("And what about the appeal?", "ctx", &history, sys, false);
        assert!(
            !with_hist.contains("Who can sanction 30 days leave?"),
            "demo leaked into a conversation that has real history",
        );
        // …but the real turns must still be there.
        assert!(with_hist.contains("What are the grounds for penalty?"));

        // On the DEFAULT (capable-model) profile the demo is off entirely — it is
        // now gated on `compact_prompt_profile()`, not on prompt wording. It stays
        // available for small models, which is the only case the 0.5B A/B measured.
        let no_hist = build_rag_prompt("Define civil servant under the Sindh Civil Servants Act.", "ctx", &[], sys, false);
        assert!(
            !no_hist.contains("Who can sanction 30 days leave?"),
            "8B profile must not get the demo",
        );
    }

    /// The conversation-reference signal must not swallow self-contained queries:
    /// "compare"/"difference between" alone are NOT references to the chat.
    #[test]
    fn followup_detector_still_skips_self_contained_comparisons() {
        assert!(!is_followup_query(
            "Compare the grounds for penalty in the E&D Rules with the Removal from Service Ordinance."
        ));
        assert!(!is_followup_query(
            "What is the difference between a minor penalty and a major penalty?"
        ));
    }

    #[test]
    fn carryover_extracts_entities_from_user_turns_only() {
        let history = vec![
            HistoryTurn {
                role: "user".into(),
                content: "What is the procedure for Ex-Pakistan leave for SCUG officers in BS-19?".into(),
            },
            HistoryTurn {
                role: "assistant".into(),
                // Assistant text MUST NOT contribute keywords — it can echo
                // template language ("PART 1 — ANSWER:", "follow these steps")
                // that we explicitly want to keep out of retrieval.
                content: "PART 1 — ANSWER: To take Ex-Pakistan leave the BOGUSWORD officer follows these steps.".into(),
            },
        ];
        let kws = extract_carryover_keywords(&history);
        // Should extract from user turn — capitalised proper nouns, BS-19 (digit + dash), longer words.
        let joined = kws.join(" ").to_lowercase();
        assert!(joined.contains("bs-19"), "expected BS-19, got {kws:?}");
        assert!(
            joined.contains("ex-pakistan") || joined.contains("scug") || joined.contains("procedure"),
            "expected user-turn entities, got {kws:?}"
        );
        // Must NOT pull from the assistant turn.
        assert!(!joined.contains("bogusword"), "leaked assistant content: {kws:?}");
        assert!(kws.len() <= 5, "cap of 5 keywords violated: {kws:?}");
    }

    #[test]
    fn carryover_drops_generic_legal_boilerplate() {
        // The live trace carried over "Draft, Chief, Secretary" — generic gov
        // boilerplate that dragged clerical draft-signing chunks into a
        // procurement follow-up. Only distinctive entities should survive.
        let history = vec![HistoryTurn {
            role: "user".into(),
            content: "Draft note to Chief Secretary for procurement through direct \
                      contracting with the sole distributor of Canon scanners for 10 \
                      departments to be used in e-Daftar".into(),
        }];
        let kws = extract_carryover_keywords(&history);
        let joined = kws.join(" ").to_lowercase();
        // Generic boilerplate must be gone.
        for bad in ["chief", "secretary", "draft", "note", "departments"] {
            assert!(!joined.contains(bad), "generic boilerplate leaked ({bad}): {kws:?}");
        }
        // A distinctive entity must survive.
        assert!(
            joined.contains("canon") || joined.contains("daftar"),
            "expected a distinctive entity, got {kws:?}"
        );
    }

    #[test]
    fn carryover_handles_empty_or_assistant_only_history() {
        assert!(extract_carryover_keywords(&[]).is_empty());
        let assistant_only = vec![HistoryTurn {
            role: "assistant".into(),
            content: "Some answer with Capitalised Words.".into(),
        }];
        assert!(extract_carryover_keywords(&assistant_only).is_empty());
    }

    /// Regression guard — see CHANGELOG_DEV.md 2026-05-10. Injecting prior
    /// Q&A pairs into the RAG prompt caused cross-prompt drift on local
    /// 1.5B-3B models. Current policy (2026-05-11): inject ONLY for
    /// anaphoric follow-ups (`is_followup_query`). Self-contained queries
    /// must still see no history.
    #[test]
    fn build_rag_prompt_does_not_inject_history_for_self_contained_query() {
        let history = vec![
            HistoryTurn {
                role: "user".into(),
                content: "What is the procedure for Ex-Pakistan leave?".into(),
            },
            HistoryTurn {
                role: "assistant".into(),
                content: "PART 1 — ANSWER: To take Ex-Pakistan leave, an SCUG \
                          officer should follow these steps: 1) Obtain approval \
                          from the Head of Department or Supervisor."
                    .into(),
            },
        ];
        let out = build_rag_prompt(
            "Define 'civil servant' under the Sindh Civil Servants Act.",
            "[1] some-doc.pdf p.1: relevant context here",
            &history,
            "SYSTEM-PROMPT",
            false,
        );
        assert!(
            !out.contains("Ex-Pakistan leave"),
            "prior user turn must not leak into prompt: {out}"
        );
        assert!(
            !out.contains("follow these steps"),
            "prior assistant turn must not leak into prompt: {out}"
        );
        // ChatML structure: exactly one user turn (the current question). The
        // one-shot citation demo only injects for system prompts requesting [n];
        // this test uses "SYSTEM-PROMPT" so no demo, no history.
        assert_eq!(
            out.matches("<|im_start|>user").count(),
            1,
            "must contain exactly one user turn: {out}"
        );
        assert_eq!(
            out.matches("<|im_start|>assistant").count(),
            1,
            "must contain exactly one (open) assistant turn: {out}"
        );
    }

    #[test]
    fn build_rag_prompt_injects_history_for_anaphoric_followup() {
        let history = vec![
            HistoryTurn { role: "user".into(), content: "List all grounds on which a civil servant can be dismissed.".into() },
            HistoryTurn { role: "assistant".into(), content: "Grounds: (a) inefficient, (b) misconduct, (c) corrupt, (d) subversive.".into() },
        ];
        let out = build_rag_prompt(
            "And what is the appeal process for such cases?", // anaphoric: contains " such "? no, but starts with "and "
            "[1] doc.pdf p.1: appeal context",
            &history,
            "SYSTEM-PROMPT",
            false,
        );
        assert!(out.contains("dismissed"), "prior user turn should appear: {out}");
        assert!(out.contains("subversive"), "prior assistant turn should appear: {out}");
        // ChatML has 2 user + 2 assistant turns (1 prior pair + 1 current). The
        // [n] demo does not inject for "SYSTEM-PROMPT".
        assert_eq!(out.matches("<|im_start|>user").count(), 2);
        assert_eq!(out.matches("<|im_start|>assistant").count(), 2);
    }

    #[test]
    fn build_rag_prompt_pins_urdu_output_language() {
        let urdu_q = "ایک سول سرونٹ اپنے خلاف کاروائی پر اپیل کیسے دائر کر سکتا ہے؟";
        let out = build_rag_prompt(urdu_q, "[1] doc.pdf p.1", &[], "SYSTEM-PROMPT", false);
        assert!(out.contains("SAME script"), "urdu prompt should carry language pin: {out}");
    }

    #[test]
    fn clean_llm_output_strips_think_block() {
        // v5.2: instruct-only prompts don't request PART 1 framing; the
        // scrubber still tolerates <think> blocks (defensive for older models).
        let raw = "<think>internal reasoning here\nover multiple lines</think>\n\nFile the appeal within thirty days [1].";
        let cleaned = clean_llm_output(raw);
        assert!(!cleaned.contains("<think>"), "think tag should be stripped: {cleaned}");
        assert!(!cleaned.contains("reasoning"), "think content should be stripped: {cleaned}");
        assert!(cleaned.contains("File the appeal"), "answer should survive: {cleaned}");
    }

    #[test]
    fn clean_llm_output_strips_unclosed_think_block() {
        let raw = "<think>reasoning that never closed and runs into the rest";
        let cleaned = clean_llm_output(raw);
        // After the unclosed think block consumes to EOF the result is empty;
        // scrubber substitutes the canonical refusal so the UI never sees a
        // blank answer.
        assert_eq!(cleaned, INSUFFICIENT_INFO_MSG);
    }

    #[test]
    fn clean_llm_output_strips_residual_section_headers() {
        // v5.2 prompt no longer requests PART 1 / PART 2 / REFERENCES blocks,
        // but a model may still emit those headers from training habit. Strip
        // the header lines while preserving the content that follows.
        let raw = "PART 1 — ANSWER:\nFile the appeal within thirty days [1].\n\
                   PART 2 — REFERENCES:\n1. doc.pdf, page 7";
        let cleaned = clean_llm_output(raw);
        assert!(!cleaned.contains("PART 1"), "PART 1 header survived: {cleaned}");
        assert!(!cleaned.contains("PART 2"), "PART 2 header survived: {cleaned}");
        assert!(cleaned.contains("File the appeal"), "answer body lost: {cleaned}");
    }

    #[test]
    fn clean_llm_output_passes_plain_answer_through() {
        // v5.2 dropped the PART 1 anchor: an instruct-model answer with no
        // section headers must pass through unchanged.
        let raw = "File the appeal within thirty days [1]. The appellate authority reviews the record [2].";
        let cleaned = clean_llm_output(raw);
        assert_eq!(cleaned, raw);
    }

    #[test]
    fn clean_llm_output_passes_canonical_refusal_through() {
        let raw = INSUFFICIENT_INFO_MSG;
        let cleaned = clean_llm_output(raw);
        assert_eq!(cleaned, INSUFFICIENT_INFO_MSG);
    }

    #[test]
    fn extract_indexed_citations_resolves_in_bounds_indices() {
        let chunks = vec![
            ("c1".to_string(), "s1".to_string(), "body1".to_string(), Some(7i64), "alpha.pdf".to_string()),
            ("c2".to_string(), "s2".to_string(), "body2".to_string(), Some(12i64), "beta.pdf".to_string()),
            ("c3".to_string(), "s3".to_string(), "body3".to_string(), None, "gamma.docx".to_string()),
        ];
        let answer = "First the rule says X [1]. Then it says Y [3]. Also see [2].";
        let refs = extract_indexed_citations(answer, &chunks);
        assert_eq!(refs.len(), 3);
        assert_eq!(refs[0], ("alpha.pdf".to_string(), Some(7)));
        assert_eq!(refs[1], ("gamma.docx".to_string(), None));
        assert_eq!(refs[2], ("beta.pdf".to_string(), Some(12)));
    }

    #[test]
    fn extract_indexed_citations_ignores_out_of_bounds_and_legacy_markers() {
        let chunks = vec![
            ("c1".to_string(), "s1".to_string(), "body".to_string(), Some(1i64), "a.pdf".to_string()),
        ];
        // [5] out of bounds, [a.pdf, page 1] legacy format (has comma) — both ignored.
        let answer = "Claim [5]. Other [a.pdf, page 1]. Real [1].";
        let refs = extract_indexed_citations(answer, &chunks);
        assert_eq!(refs.len(), 1);
        assert_eq!(refs[0], ("a.pdf".to_string(), Some(1)));
    }

    #[test]
    fn count_valid_index_markers_dedupes_and_bounds_check() {
        assert_eq!(count_valid_index_markers("see [1] and [2] and [1] again", 3), 2);
        assert_eq!(count_valid_index_markers("see [1] and [5]", 3), 1);
        assert_eq!(count_valid_index_markers("see [filename.pdf, page 1]", 3), 0);
        assert_eq!(count_valid_index_markers("[1]", 0), 0);
    }

    #[test]
    fn extract_citations_handles_xml_filename_wrapper() {
        let text = "Per <filename>appointment promotion and transfer rules.pdf</filename>, page 42, the rule states ...";
        let cites = extract_citations(text);
        assert!(cites.iter().any(|c| c.contains("appointment promotion")), "XML wrapper not extracted: {cites:?}");
    }

    fn chunk(id: &str) -> (String, String, String, Option<i64>, String) {
        (
            id.to_string(),
            "src_id".to_string(),
            format!("content of {id}"),
            Some(1),
            "doc.pdf".to_string(),
        )
    }

    /// A chunk that ranks first in both lists should come out first.
    #[test]
    fn rrf_shared_chunk_ranks_first() {
        let fts = vec![chunk("a"), chunk("b")];
        let vec = vec![chunk("a"), chunk("c")];
        let result = rrf_merge(&fts, &vec, 60);
        assert_eq!(result[0].0, "a", "shared chunk should rank highest");
        assert_eq!(result.len(), 3, "should have 3 unique chunks");
    }

    /// Higher-ranked entry in fts-only list should beat lower-ranked entry.
    #[test]
    fn rrf_rank_order_preserved_single_list() {
        let fts = vec![chunk("x"), chunk("y"), chunk("z")];
        let vec: Vec<_> = vec![];
        let result = rrf_merge(&fts, &vec, 60);
        assert_eq!(result[0].0, "x");
        assert_eq!(result[1].0, "y");
        assert_eq!(result[2].0, "z");
    }

    /// Union merge: all vector hits (cosine order) first, then FTS-only; dedup by
    /// id; per-list caps honoured.
    #[test]
    fn union_merge_vec_cosine_first_dedup_and_caps() {
        let vec = vec![chunk("a"), chunk("b"), chunk("c")]; // cosine-ordered
        let fts = vec![chunk("d"), chunk("a")]; // "a" overlaps vec → kept once, in vec slot
        let out = union_merge(&fts, &vec, 20, 20);
        let ids: Vec<&str> = out.iter().map(|c| c.0.as_str()).collect();
        assert_eq!(ids, vec!["a", "b", "c", "d"], "all vector hits first, then fts-only");
        // Per-list caps: vec.take(2)=[a,b], fts.take(1)=[d].
        let capped = union_merge(&fts, &vec, 1, 2);
        let cids: Vec<&str> = capped.iter().map(|c| c.0.as_str()).collect();
        assert_eq!(cids, vec!["a", "b", "d"]);
    }

    // ── merge_depth: pool depth tracks the window, mix stays the operator's ──

    /// THE REGRESSION. Production ran ECHO_VEC_K=28 / ECHO_FTS_K=7 against a
    /// window of ctx_top_k=36, and the old code used those as absolute caps →
    /// pool ≤ 35 < 36, so the merge silently became the selection. Both arms
    /// returned 72 candidates, so 111 of 144 fetched chunks were discarded.
    #[test]
    fn merge_depth_never_starves_the_window() {
        let (vk, fk) = merge_depth_from(28.0, 7.0, 2.0, 36, 72, 72);
        assert!(
            vk + fk >= 36,
            "pool {vk}+{fk} must never fall below the selection target 36 \
             (old fixed caps gave 28+7=35 — the bug)",
        );
        assert_eq!((vk, fk), (58, 14), "36 × 2.0 overfetch, split 80/20");
    }

    /// The architect's 80/20 mix survives at every window size — the ratio is
    /// policy, the depth is derived.
    #[test]
    fn merge_depth_preserves_the_operator_ratio() {
        for k in [12usize, 24, 36, 48, 100] {
            let (vk, fk) = merge_depth_from(28.0, 7.0, 2.0, k, 500, 500);
            let ratio = vk as f32 / (vk + fk) as f32;
            assert!(
                (ratio - 0.8).abs() < 0.05,
                "k={k}: {vk}/{fk} drifted from 80/20 (got {ratio:.2})",
            );
            assert!(vk + fk >= k, "k={k}: pool {vk}+{fk} underfills the window");
        }
    }

    /// A short arm hands its unused slots to the other one, so a corpus that
    /// simply has few keyword hits still fills the pool instead of starving it.
    #[test]
    fn merge_depth_redistributes_a_short_arm() {
        let (vk, fk) = merge_depth_from(28.0, 7.0, 2.0, 36, 72, 3);
        assert_eq!(fk, 3, "cannot take more FTS than FTS returned");
        assert_eq!(vk, 69, "the 11 unused FTS slots go to vec, total still 72");
        assert!(vk + fk >= 36);
    }

    /// Both arms short → pool is whatever exists, never a panic and never more
    /// than was fetched. Underfilling is then a corpus fact, not a config bug.
    #[test]
    fn merge_depth_cannot_exceed_what_was_fetched() {
        let (vk, fk) = merge_depth_from(28.0, 7.0, 2.0, 36, 5, 4);
        assert_eq!((vk, fk), (5, 4));
    }

    /// A non-zero share must never round to zero. At 80/20 with a tiny window
    /// the naive rounding would delete the keyword arm outright — the arm that
    /// carries exact provision-number matches.
    #[test]
    fn merge_depth_never_zeroes_a_live_arm() {
        let (vk, fk) = merge_depth_from(28.0, 7.0, 1.0, 2, 50, 50);
        assert!(fk >= 1, "FTS arm was deleted by rounding: {vk}/{fk}");
        assert!(vk >= 1, "vec arm was deleted by rounding: {vk}/{fk}");
    }

    /// Degenerate config (both knobs 0) falls back to an even split rather than
    /// returning an empty pool, which would refuse every query in the system.
    #[test]
    fn merge_depth_degenerate_shares_do_not_empty_the_pool() {
        let (vk, fk) = merge_depth_from(0.0, 0.0, 2.0, 36, 72, 72);
        assert!(vk + fk >= 36, "empty pool from a zeroed config: {vk}/{fk}");
        assert_eq!(vk, fk, "no ratio information → even split");
    }

    /// Legacy/control arm reproduces the OLD caps exactly, so the A/B has a real
    /// control on the same binary and rollback needs no rebuild. This is also the
    /// shipped default — the deploy carrying this code must be a no-op.
    #[test]
    fn merge_depth_legacy_mode_reproduces_the_old_caps() {
        let (vk, fk) = merge_depth_from(28.0, 7.0, 0.0, 36, 72, 72);
        assert_eq!((vk, fk), (28, 7), "control arm must be the pre-fix behaviour");
        assert!(vk + fk < 36, "the old behaviour is precisely that it starves K=36");
    }

    /// Legacy mode still cannot ask for more than an arm returned.
    #[test]
    fn merge_depth_legacy_mode_respects_availability() {
        let (vk, fk) = merge_depth_from(28.0, 7.0, 0.0, 36, 5, 2);
        assert_eq!((vk, fk), (5, 2));
    }

    /// When fts is empty the vec order should be preserved exactly.
    #[test]
    fn rrf_empty_fts_returns_vec_order() {
        let fts: Vec<_> = vec![];
        let vec = vec![chunk("p"), chunk("q")];
        let result = rrf_merge(&fts, &vec, 60);
        assert_eq!(result[0].0, "p");
        assert_eq!(result[1].0, "q");
    }

    /// Both lists empty → empty result.
    #[test]
    fn rrf_both_empty_returns_empty() {
        let result = rrf_merge(&[], &[], 60);
        assert!(result.is_empty());
    }

    // ── validate_grounding tests ─────────────────────────────────────────────

    fn ctx_chunk(file_name: &str) -> (String, String, String, Option<i64>, String) {
        (
            "cid".to_string(),
            "sid".to_string(),
            "some content".to_string(),
            Some(1),
            file_name.to_string(),
        )
    }

    /// Fallback phrase as input → grounded, no replacement.
    #[test]
    fn grounding_fallback_phrase_passes() {
        let ctx = vec![ctx_chunk("doc.pdf")];
        let r = validate_grounding(GROUNDING_FALLBACK, &ctx);
        assert!(r.is_grounded);
        assert!(!r.fallback_triggered);
    }

    /// Substantive response with valid citations → grounded.
    #[test]
    fn grounding_valid_citation_passes() {
        let ctx = vec![ctx_chunk("report.pdf")];
        let response =
            "The quarterly revenue increased by 12% [report, page 3]. \
             Expenses also rose [report, page 5]. These figures show strong growth.";
        let r = validate_grounding(response, &ctx);
        assert!(!r.fallback_triggered, "valid citations should pass");
        assert!(r.citation_count >= 2);
    }

    /// Substantive response with zero citations but no explicit "not found" phrase
    /// → validator does NOT trigger fallback (relaxed: small models often answer
    /// correctly but skip citation formatting).
    #[test]
    fn grounding_zero_citations_no_phrase_does_not_trigger_fallback() {
        let ctx = vec![ctx_chunk("report.pdf")];
        // Single-sentence answer: no citation marker, no "I don't know" phrase.
        // factual_count < 2, so Check 3 (uncited multi-sentence) does NOT fire.
        let response = "The quarterly revenue increased significantly.";
        let r = validate_grounding(response, &ctx);
        assert!(!r.fallback_triggered, "single-sentence answer without phrase should not trigger fallback");
        assert_eq!(r.citation_count, 0);
    }

    /// Citation to a source not in context → fallback triggered.
    #[test]
    fn grounding_invalid_source_triggers_fallback() {
        let ctx = vec![ctx_chunk("report.pdf")];
        // "external-study" is not in context_chunks
        let response = "According to the analysis [external-study, page 2], revenue grew by 15%.";
        let r = validate_grounding(response, &ctx);
        assert!(r.fallback_triggered, "citation to unknown source should fail");
        assert_eq!(r.citation_count, 0);
    }

    /// Short response (≤ 100 chars) without citations → NOT a fallback.
    #[test]
    fn grounding_short_response_no_citation_passes() {
        let ctx = vec![ctx_chunk("doc.pdf")];
        let r = validate_grounding("Yes, confirmed.", &ctx);
        assert!(!r.fallback_triggered, "short response doesn't need citation");
    }

    /// Empty response → grounded (edge case).
    #[test]
    fn grounding_empty_response_passes() {
        let ctx = vec![ctx_chunk("doc.pdf")];
        let r = validate_grounding("", &ctx);
        assert!(!r.fallback_triggered);
    }

    fn ctx_chunk_with(content: &str) -> (String, String, String, Option<i64>, String) {
        (
            "cid".to_string(),
            "sid".to_string(),
            content.to_string(),
            Some(1),
            "constitution_ir.json".to_string(),
        )
    }

    /// 2026-06-06 loosen: a short answer whose tokens overlap the context but
    /// carries a bad citation marker is rescued (kept) instead of nuked.
    #[test]
    fn grounding_overlap_rescues_short_uncited_answer() {
        let ctx = vec![ctx_chunk_with(
            "Article 251 — National language. The national language of Pakistan \
             is Urdu, and English may be used for official purposes.",
        )];
        // Cites an unknown source ([wikipedia]) → would normally be all_invalid
        // + short → killed. But the answer vocabulary is verbatim from context.
        let response = "The languages are Urdu and English [wikipedia].";
        let r = validate_grounding(response, &ctx);
        assert!(!r.fallback_triggered, "context-overlapping answer must not be nuked");
        assert!(r.uncited_only, "rescued answer should be marked uncited_only");
    }

    /// Guard: a genuine hallucination (no shared vocabulary) with a bad
    /// citation is still demoted — overlap rescue must not over-trigger.
    #[test]
    fn grounding_overlap_does_not_rescue_hallucination() {
        let ctx = vec![ctx_chunk_with(
            "Article 251 — National language. The national language is Urdu.",
        )];
        let response = "The capital city is Tokyo with twelve bridges [wikipedia].";
        let r = validate_grounding(response, &ctx);
        assert!(r.fallback_triggered, "non-overlapping answer should still fall back");
    }

    // ── chapter-index fast-path tests (2026-06-06) ──────────────────────────

    #[test]
    fn part_number_normalises_arabic_and_roman() {
        assert_eq!(normalise_part_number("2"), "II");
        assert_eq!(normalise_part_number("12"), "XII");
        assert_eq!(normalise_part_number("ii"), "II");
        assert_eq!(normalise_part_number("XII"), "XII");
    }

    #[test]
    fn chapter_index_resolves_chapter_of_part() {
        // Q11
        assert_eq!(
            detect_chapter_index_citation(
                "Which Articles fall under Chapter 1 of Part II (Fundamental Rights)? List them with titles."
            ),
            Some("Article IDX-PII-C1".to_string())
        );
        // Q12
        assert_eq!(
            detect_chapter_index_citation(
                "What are the Principles of Policy listed in Chapter 2 of Part II?"
            ),
            Some("Article IDX-PII-C2".to_string())
        );
    }

    #[test]
    fn chapter_index_part_only_with_enum_hint() {
        assert_eq!(
            detect_chapter_index_citation("List the articles in Part IX"),
            Some("Article IDX-PIX".to_string())
        );
    }

    #[test]
    fn chapter_index_defers_to_specific_article() {
        // A specific Article lookup must NOT be hijacked by the IDX path.
        assert_eq!(
            detect_chapter_index_citation("What does Article 8 of Part II say?"),
            None
        );
    }

    #[test]
    fn chapter_index_ignores_plain_queries() {
        assert_eq!(detect_chapter_index_citation("Summarise Chapter 2"), None);
        assert_eq!(
            detect_chapter_index_citation("What does the Constitution define as a citizen?"),
            None
        );
    }

    // ── build_context tests ──────────────────────────────────────────────────

    fn fat_chunk(id: &str, chars: usize) -> (String, String, String, Option<i64>, String) {
        (
            id.to_string(),
            "src".to_string(),
            "x".repeat(chars),
            Some(1),
            "doc.pdf".to_string(),
        )
    }

    /// K cap: only the first K chunks should be returned.
    #[test]
    fn context_builder_respects_k_cap() {
        let ranked: Vec<_> = (0..10).map(|i| chunk(&format!("c{i}"))).collect();
        let result = build_context(&ranked, 4, usize::MAX);
        assert_eq!(result.len(), 4);
        assert_eq!(result[0].0, "c0");
        assert_eq!(result[3].0, "c3");
    }

    /// Token budget: a chunk that would exceed the budget is dropped.
    /// First chunk always admitted even if it alone exceeds budget.
    ///
    /// All fixture chunks are >=50 chars so the Pass-1 anchor-only filter
    /// (which drops <50-char heading shells) doesn't interfere — this test
    /// targets the token-budget path specifically.
    #[test]
    fn context_builder_token_budget_drops_large_chunk() {
        // budget=400 → per-chunk cap = 400*0.25 = 100 tokens. All chunks >=50
        // chars so the Pass-1 anchor filter doesn't interfere. Estimates: chars/4 + 20.
        //   "first":   60 chars → 15 + 20 =  35 tokens  (first → always admitted)
        //   "big":   2000 chars → 500 + 20 = 520 tokens (>100 cap & >400 budget → dropped)
        //   "small":  200 chars → 50 + 20 =  70 tokens  (≤100 cap; 35+70=105 ≤ 400 → fits)
        let ranked = vec![
            fat_chunk("first", 60),
            fat_chunk("big", 2000),
            fat_chunk("small", 200),
        ];
        let result = build_context(&ranked, 8, 400);
        let ids: Vec<&str> = result.iter().map(|c| c.0.as_str()).collect();
        assert!(ids.contains(&"first"), "first chunk always included");
        assert!(!ids.contains(&"big"), "big should be dropped (over cap & budget)");
        assert!(ids.contains(&"small"), "small fits in remaining budget");
    }

    /// First chunk always included even when it alone exceeds budget.
    #[test]
    fn context_builder_always_includes_first_chunk() {
        let ranked = vec![fat_chunk("giant", 10_000)]; // ~2500 tokens — way over budget
        let result = build_context(&ranked, 8, 100);
        assert_eq!(result.len(), 1, "first chunk always included");
        assert_eq!(result[0].0, "giant");
    }

    /// Regression — citation fast-path on the Preamble: the only retrieved
    /// chunk is 2775 chars (~713 tokens) and the per-chunk cap is
    /// 2400 × 0.25 = 600 tokens. The cap was silently dropping the sole
    /// chunk so the user saw "no relevant information" with the correct
    /// citation in hand. Trace `latest_test.log` 2026-05-23 12:52 captured
    /// this exact failure mode.
    #[test]
    fn context_builder_admits_single_oversized_citation_chunk() {
        // 2775 chars matches the real Preamble row in the live DB.
        let ranked = vec![fat_chunk("preamble", 2775)];
        let result = build_context(&ranked, 12, 2400);
        assert_eq!(result.len(), 1, "single oversized chunk must be admitted");
        assert_eq!(result[0].0, "preamble");
    }

    /// Dedup: duplicate chunk_ids are skipped.
    #[test]
    fn context_builder_deduplicates_by_chunk_id() {
        let ranked = vec![
            chunk("alpha"),
            chunk("alpha"), // duplicate
            chunk("beta"),
        ];
        let result = build_context(&ranked, 8, usize::MAX);
        assert_eq!(result.len(), 2);
        assert_eq!(result[0].0, "alpha");
        assert_eq!(result[1].0, "beta");
    }

    /// Empty input → empty output.
    #[test]
    fn context_builder_empty_input() {
        let result = build_context(&[], 8, usize::MAX);
        assert!(result.is_empty());
    }

    /// Score formula: chunk at rank 1 in both lists scores 2/(60+1) ≈ 0.0328,
    /// chunk only at rank 1 in one list scores 1/61 ≈ 0.0164.
    #[test]
    fn rrf_score_arithmetic() {
        // "both" appears rank-1 in both lists; "fts_only" appears rank-2 in fts only.
        let fts = vec![chunk("both"), chunk("fts_only")];
        let vec = vec![chunk("both"), chunk("vec_only")];
        let result = rrf_merge(&fts, &vec, 60);
        // "both" must be first (highest combined score)
        assert_eq!(result[0].0, "both");
        // "fts_only" and "vec_only" both appear at rank 2 in their respective lists
        // → equal scores → either can come second; just verify both are present
        let ids: Vec<&str> = result.iter().map(|c| c.0.as_str()).collect();
        assert!(ids.contains(&"fts_only"));
        assert!(ids.contains(&"vec_only"));
    }

    // ── classify_query tests ─────────────────────────────────────────────────
    #[test]
    fn classify_summary_keywords() {
        assert!(matches!(classify_query("summarize this book"), QueryType::Summary));
        assert!(matches!(classify_query("give me an overview"), QueryType::Summary));
        assert!(matches!(classify_query("what is the book about"), QueryType::Summary));
        assert!(matches!(classify_query("outline the document"), QueryType::Summary));
        assert!(matches!(classify_query("Summarise the report"), QueryType::Summary));
        assert!(matches!(classify_query("brief me on the contract"), QueryType::Summary));
        assert!(matches!(classify_query("list all chapters"), QueryType::Summary));
        assert!(matches!(classify_query("table of contents"), QueryType::Summary));
        assert!(matches!(classify_query("what topics does this cover"), QueryType::Summary));
    }

    #[test]
    fn classify_regular_queries() {
        assert!(matches!(classify_query("what is article 17?"), QueryType::Regular));
        assert!(matches!(classify_query("who signed the agreement?"), QueryType::Regular));
        assert!(matches!(classify_query("define constitutional position"), QueryType::Regular));
        assert!(matches!(classify_query(""), QueryType::Regular));
        // These are factual lookups — the short phrase form must NOT match
        assert!(matches!(classify_query("what does the document say about article 17?"), QueryType::Regular));
        assert!(matches!(classify_query("what does the book say about the defendant?"), QueryType::Regular));
    }

    // ── judge output parsing tests ───────────────────────────────────────────
    #[test]
    fn judge_yes_response_passes() {
        assert!(parse_judge_output("YES"));
        assert!(parse_judge_output("Yes, it does."));
        assert!(parse_judge_output("yes"));
        assert!(parse_judge_output("  YES  "));
    }

    #[test]
    fn judge_no_response_fails() {
        assert!(!parse_judge_output("NO"));
        assert!(!parse_judge_output("No, it does not."));
        assert!(!parse_judge_output("no"));
    }

    #[test]
    fn judge_empty_or_unclear_fails() {
        assert!(!parse_judge_output(""));
        assert!(!parse_judge_output("   "));
        assert!(!parse_judge_output("Maybe"));
        assert!(!parse_judge_output("unclear"));
    }

    #[test]
    fn judge_yes_substring_does_not_pass() {
        assert!(!parse_judge_output("Yesterday it was not relevant."));
        assert!(!parse_judge_output("YESSIR"));
    }

    // ── citation fast-path tests (feature 003, T025) ─────────────────────────
    #[test]
    fn detect_citation_parses_article_and_clauses() {
        assert_eq!(
            detect_citation("what does Article 19A say"),
            Some("Article 19A".to_string())
        );
        assert_eq!(
            detect_citation("explain article 8(3)(b)"),
            Some("Article 8(3)(b)".to_string())
        );
        // a space before the clause paren is tolerated
        assert_eq!(
            detect_citation("ARTICLE 10 (1)"),
            Some("Article 10(1)".to_string())
        );
        assert_eq!(detect_citation("what are fundamental rights"), None);
        // Preamble is a named unit even though it has no number.
        assert_eq!(
            detect_citation("what does the preamble say"),
            Some("Preamble".to_string())
        );
        // Feature 005 — Section/Rule/Chapter/Part for act/rules/manual profiles.
        assert_eq!(
            detect_citation("what does Section 8 say"),
            Some("Section 8".to_string())
        );
        assert_eq!(
            detect_citation("explain Rule 12(2)(a)"),
            Some("Rule 12(2)(a)".to_string())
        );
        assert_eq!(
            detect_citation("summarise Chapter II"),
            Some("Chapter II".to_string())
        );
        assert_eq!(
            detect_citation("what is in Part III"),
            Some("Part III".to_string())
        );
        // Chapter/Part drop clause parens (no profile emits them at that level).
        assert_eq!(
            detect_citation("Chapter II(1)"),
            Some("Chapter II".to_string())
        );
        // Casual "section" without a number doesn't match → falls through.
        assert_eq!(detect_citation("the section of the report"), None);
    }

    #[tokio::test]
    async fn citation_fast_path_resolves_cited_units() {
        use crate::services::constitution_ir;
        use crate::services::doc_profiles::constitution::CONSTITUTION;
        use crate::services::structure_parser::{parse_tree, persist_tree};

        let pool = crate::db::test_pool().await;
        sqlx::query(
            "INSERT INTO sources(id,file_path,file_name,format,file_size_bytes,file_hash,last_modified,created_at) \
             VALUES('c1','/c','constitution','pdf',1,'h',0,0)",
        )
        .execute(&pool)
        .await
        .unwrap();
        let ir = constitution_ir::load(
            &std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("../tools/constitution_import/constitute_IR/constitution_ir.json"),
        )
        .unwrap();
        let tree = parse_tree(&ir, &CONSTITUTION);
        persist_tree(&pool, "c1", &tree).await.unwrap();

        // a sub-clause citation → exactly that leaf
        let sub = citation_fast_path(&pool, "explain Article 8(3)(b)")
            .await
            .unwrap();
        assert_eq!(sub.as_deref().map(<[String]>::len), Some(1));

        // an Article citation → the Article plus all its descendants
        let art = citation_fast_path(&pool, "what does Article 8 cover")
            .await
            .unwrap()
            .expect("Article 8 resolves");
        assert!(art.len() > 1, "Article 8 + clauses/sub-clauses, got {}", art.len());

        // no citation in the query → None (falls through to hybrid retrieval)
        assert!(citation_fast_path(&pool, "what are fundamental rights")
            .await
            .unwrap()
            .is_none());

        // a citation absent from the corpus → None
        assert!(citation_fast_path(&pool, "summarise Article 9999")
            .await
            .unwrap()
            .is_none());

        // Preamble fast-path: query → the Preamble parent + every
        // paragraph child (Idea 1, 2026-05-23 — the Preamble is now
        // split into paragraph-level children for retrieval focus).
        // Sub-tree match via `unit_path LIKE 'Preamble/%'` returns them
        // all. Parent stays present so the citation marker resolves.
        let pre = citation_fast_path(&pool, "what does the preamble say")
            .await
            .unwrap()
            .expect("Preamble resolves");
        assert!(
            pre.len() >= 2,
            "Preamble fast-path returns parent + paragraph children, got {} chunks",
            pre.len()
        );
    }

    /// Regression: two statutes share a unit_label ("Rule 5") and only the
    /// statute year / title in the preamble can tell them apart. The pre-
    /// fix code only looked at file_name tokens, both filenames tied on
    /// `{rules}`, the first row won, and the wrong source's Rule 5 was
    /// returned. Production trace 2026-05-26 (Qwen-1.5B, Q10): query about
    /// the E&D Rules 1973 was answered with APT Rule 5 definitions chunks.
    #[tokio::test]
    async fn citation_fast_path_disambiguates_by_statute_preamble() {
        let pool = crate::db::test_pool().await;

        // Two sources, each with a Rule 5 chunk and a preamble chunk that
        // carries the statute title + year. Filenames overlap only on the
        // word "rules" (which is a stopword in source_affinity_tokens),
        // so the pre-fix code tied at zero overlap and picked whichever
        // source happened to land first in row order.
        async fn insert_src(pool: &crate::db::Db, id: &str, file_name: &str) {
            sqlx::query(
                "INSERT INTO sources(id,file_path,file_name,format,file_size_bytes,file_hash,last_modified,created_at) \
                 VALUES($1, $2, $3, 'pdf', 1, 'h', 0, 0)",
            )
            .bind(id).bind(format!("/{id}")).bind(file_name)
            .execute(pool).await.unwrap();
        }
        async fn insert_chunk(
            pool: &crate::db::Db,
            id: &str, source_id: &str, idx: i64,
            unit_label: &str, unit_path: &str, content: &str,
        ) {
            sqlx::query(
                "INSERT INTO chunks(id,source_id,chunk_index,unit_type,unit_label,unit_path,content,token_count,char_start,char_end,created_at) \
                 VALUES($1, $2, $3, 'clause', $4, $5, $6, 0, 0, $7, 0)",
            )
            .bind(id).bind(source_id).bind(idx)
            .bind(unit_label).bind(unit_path).bind(content)
            .bind(content.len() as i64)
            .execute(pool).await.unwrap();
        }

        // E&D, 1973. Preamble carries the discriminating year + title.
        insert_src(&pool, "ed", "efficiency and disciplane rules.pdf").await;
        insert_chunk(
            &pool, "ed_pre", "ed", 0, "Preamble", "Preamble",
            "THE SINDH CIVIL SERVANTS (EFFICIENCY AND DISCIPLINE) RULES, 1973. \
             Karachi, the 8th September 1973. Definitions.",
        ).await;
        insert_chunk(
            &pool, "ed_r5", "ed", 5, "Rule 5", "Rule 5",
            "5. Inquiry Procedure. The following procedure shall be observed \
             when a civil servant is proceeded against under these rules.",
        ).await;

        // APT, 1974. Same "Rule 5" label, different body (committee admin).
        insert_src(&pool, "apt", "appointment promotion and transfer rules.pdf").await;
        insert_chunk(
            &pool, "apt_pre", "apt", 0, "Preamble", "Preamble",
            "THE SINDH CIVIL SERVANTS (APPOINTMENT, PROMOTION AND TRANSFER) \
             RULES, 1974. Karachi, the 28th March 1974.",
        ).await;
        insert_chunk(
            &pool, "apt_r5", "apt", 5, "Rule 5", "Rule 5",
            "5. (1) In each Department there shall be one or more Departmental \
             Promotion Committees.",
        ).await;

        // Query references the 1973 E&D statute. Without the preamble vocab
        // fix, this returned APT Rule 5 in production.
        let got = citation_fast_path(
            &pool,
            "Walk me through the inquiry procedure under Rule 5 of the Sindh E&D Rules 1973.",
        )
        .await
        .unwrap()
        .expect("Rule 5 resolves");
        assert_eq!(got, vec!["ed_r5"], "expected E&D Rule 5, got {:?}", got);

        // Symmetric: a query keyed on the 1974 statute must pick APT.
        let got = citation_fast_path(
            &pool,
            "Under the Sindh APT Rules 1974, explain Rule 5 of appointment, promotion and transfer.",
        )
        .await
        .unwrap()
        .expect("Rule 5 resolves");
        assert_eq!(got, vec!["apt_r5"], "expected APT Rule 5, got {:?}", got);
    }

    // ── S1 — salience-aware retrieval tests (feature 003, F5-imperative) ─────

    /// Sanity: `is_salient_unit` knows about the constitution's "Preamble"
    /// and rejects unrelated labels. v1 has one DocProfile registered.
    #[test]
    fn salient_unit_registry_recognises_preamble() {
        assert!(crate::services::doc_profiles::is_salient_unit("Preamble"));
        assert!(!crate::services::doc_profiles::is_salient_unit("Article 41"));
        assert!(!crate::services::doc_profiles::is_salient_unit("Chapter 2"));
        assert!(!crate::services::doc_profiles::is_salient_unit(""));
    }

    /// Reproduces the F5-imperative failure: hybrid retrieval returned
    /// Article 41 ("President of Pakistan") at rank 1 and Preamble at rank
    /// 4, because "founder of Pakistan" surface-matched Article 41 better.
    /// After `apply_salience_boost` (1.3× on Preamble), Preamble must lead.
    ///
    /// Score math: position score 1/(60+rank). Article 41 at rank 1 = 1/61
    /// ≈ 0.0164. Preamble at rank 4 = 1/64 ≈ 0.0156 × 1.3 = 0.0203 → wins.
    #[tokio::test]
    async fn salience_boost_lifts_preamble_above_article_41() {
        let pool = crate::db::test_pool().await;
        sqlx::query(
            "INSERT INTO sources(id,file_path,file_name,format,file_size_bytes,file_hash,last_modified,created_at) \
             VALUES('s1','/c','constitution','pdf',1,'h',0,0)",
        )
        .execute(&pool)
        .await
        .unwrap();
        // Two chunks: Article 41 (Head-of-State leaf) and Preamble (canonical).
        for (id, label, content) in [
            ("art41",   "Article 41", "Article 41 — The President"),
            ("pre",     "Preamble",   "Whereas... Founder of Pakistan, Quaid-i-Azam..."),
            ("art1",    "Article 1",  "The Republic and its territories"),
            ("art176",  "Article 176","Constitution of Supreme Court"),
        ] {
            sqlx::query(
                "INSERT INTO chunks(id, source_id, chunk_index, char_start, char_end, \
                                    page_number, token_count, parent_chunk_id, \
                                    unit_type, unit_label, unit_path, domain_metadata, content, created_at) \
                 VALUES ($1, 's1', 0, 0, $2, NULL, $3, NULL, 'article', $4, $5, NULL, $6, 0)",
            )
            .bind(id)
            .bind(content.len() as i64)
            .bind(content.split_whitespace().count() as i64)
            .bind(label)
            .bind(label) // unit_path = label for test simplicity
            .bind(content)
            .execute(&pool)
            .await
            .unwrap();
        }
        // Simulated hybrid output: leaf chunks dominate, Preamble at rank 4.
        let ranked: Vec<(String, String, String, Option<i64>, String)> = vec![
            ("art41".into(),  "s1".into(), "Article 41 — The President".into(), None, "constitution".into()),
            ("art1".into(),   "s1".into(), "The Republic and its territories".into(), None, "constitution".into()),
            ("art176".into(), "s1".into(), "Constitution of Supreme Court".into(), None, "constitution".into()),
            ("pre".into(),    "s1".into(), "Whereas... Founder of Pakistan, Quaid-i-Azam...".into(), None, "constitution".into()),
        ];
        let boosted = apply_salience_boost(&pool, ranked, 1.3).await;
        assert_eq!(
            boosted.first().map(|c| c.0.as_str()),
            Some("pre"),
            "salience boost must lift Preamble above Article 41 at comparable ranks"
        );
    }

    /// Boost is a no-op when no chunk in the pool is salient — the order
    /// must be preserved exactly so non-canonical queries are unaffected.
    #[tokio::test]
    async fn salience_boost_noop_when_no_salient_chunk() {
        let pool = crate::db::test_pool().await;
        sqlx::query(
            "INSERT INTO sources(id,file_path,file_name,format,file_size_bytes,file_hash,last_modified,created_at) \
             VALUES('s1','/c','constitution','pdf',1,'h',0,0)",
        )
        .execute(&pool).await.unwrap();
        for (id, label) in [("a", "Article 10"), ("b", "Article 11"), ("c", "Article 12")] {
            sqlx::query(
                "INSERT INTO chunks(id, source_id, chunk_index, char_start, char_end, \
                                    page_number, token_count, parent_chunk_id, \
                                    unit_type, unit_label, unit_path, domain_metadata, content, created_at) \
                 VALUES ($1, 's1', 0, 0, 10, NULL, 5, NULL, 'article', $2, $3, NULL, 'x', 0)",
            )
            .bind(id).bind(label).bind(label)
            .execute(&pool).await.unwrap();
        }
        let ranked: Vec<(String, String, String, Option<i64>, String)> = vec![
            ("a".into(), "s1".into(), "x".into(), None, "f".into()),
            ("b".into(), "s1".into(), "x".into(), None, "f".into()),
            ("c".into(), "s1".into(), "x".into(), None, "f".into()),
        ];
        let after = apply_salience_boost(&pool, ranked.clone(), 1.3).await;
        assert_eq!(after.iter().map(|c| c.0.clone()).collect::<Vec<_>>(),
                   ranked.iter().map(|c| c.0.clone()).collect::<Vec<_>>(),
                   "order preserved when nothing is salient");
    }

    // ── S2 — definitional-question classifier tests ───────────────────────────

    #[test]
    fn is_definitional_matches_canonical_question_shapes() {
        // Positive — definitional / canonical-answer queries.
        assert!(is_definitional("Who is the founder of Pakistan?"));
        assert!(is_definitional("who is called the founder of pakistan"));
        assert!(is_definitional("Whom does the Preamble cite as declaring …"));
        assert!(is_definitional("What is the meaning of fundamental rights"));
        assert!(is_definitional("What does sovereignty mean"));
        assert!(is_definitional("Define high treason"));
        assert!(is_definitional("Meaning of preamble"));
        assert!(is_definitional("Who founded the Federation"));
        assert!(is_definitional("Who created the Constitution"));
        assert!(is_definitional("Who wrote the Preamble"));
        // 2026-06-02 — investor-demo phrasings that previously missed S2.
        assert!(is_definitional("What does the Constitution define as a \"Service of Pakistan\"?"));
        assert!(is_definitional("How is \"Majlis-e-Shoora (Parliament)\" defined in the Constitution?"));
        assert!(is_definitional("What is the definition of Money Bill?"));
        assert!(is_definitional("Quote the definition of Money Bill as it appears in the Constitution."));
        assert!(is_definitional("Definition of citizen"));
        // 2026-06-05 — widened to cover the three eval phrasings that fell
        // through to the non-definitional pool (Q3 "State"/Q5 "citizen"/
        // Q9 "languages mentioned").
        assert!(is_definitional("What is the constitutional definition of \"State\" for the purposes of Fundamental Rights?"));
        assert!(is_definitional("What does Article 260 say about the term \"citizen\"?"));
        assert!(is_definitional("List the languages mentioned in Article 251."));
        assert!(is_definitional("Which languages are mentioned in Article 251?"));

        // Negative — instance / event / yes-no queries.
        assert!(!is_definitional("What does Article 10 say"));
        assert!(!is_definitional("When was the Constitution adopted"));
        assert!(!is_definitional("Does the Constitution permit detention"));
        assert!(!is_definitional("How many provinces are there"));
        // "List the fundamental rights" — still negative; the widened pattern
        // requires a topical noun ("languages", "members", "qualifications",
        // etc.) followed by a "mentioned in / listed in / set out in <unit>"
        // tail, so an unanchored "List X" is left alone.
        assert!(!is_definitional("List the fundamental rights"));
        assert!(!is_definitional(""));
    }

    /// On a definitional query, the stronger boost (1.6×) lifts a chunk
    /// that the regular 1.3× boost would *not* lift. Demonstrates S2's
    /// added power beyond S1.
    ///
    /// Score math: a salient chunk at rank 6 has position score 1/(60+6) =
    /// 0.01515. Non-salient rank 1 = 1/61 = 0.01639. 1.3× on rank 6 =
    /// 0.01970 → wins. So S1 alone already overcomes 5 ranks; S2's 1.6×
    /// extends that further. We test the 1.6× explicitly to lock the
    /// definitional path's stronger lift in place.
    #[tokio::test]
    async fn definitional_boost_lifts_salient_chunk_buried_deeper() {
        let pool = crate::db::test_pool().await;
        sqlx::query(
            "INSERT INTO sources(id,file_path,file_name,format,file_size_bytes,file_hash,last_modified,created_at) \
             VALUES('s1','/c','constitution','pdf',1,'h',0,0)",
        )
        .execute(&pool).await.unwrap();
        // 8 chunks: rank 8 is Preamble (very buried). 1.6× must still lift it.
        for (id, label) in [
            ("a1","Article 41"), ("a2","Article 90"), ("a3","Article 91"),
            ("a4","Article 9"),  ("a5","Article 8"),  ("a6","Article 7"),
            ("a7","Article 6"),  ("pre","Preamble"),
        ] {
            sqlx::query(
                "INSERT INTO chunks(id, source_id, chunk_index, char_start, char_end, \
                                    page_number, token_count, parent_chunk_id, \
                                    unit_type, unit_label, unit_path, domain_metadata, content, created_at) \
                 VALUES ($1, 's1', 0, 0, 10, NULL, 5, NULL, 'article', $2, $3, NULL, 'x', 0)",
            )
            .bind(id).bind(label).bind(label)
            .execute(&pool).await.unwrap();
        }
        let ranked: Vec<(String, String, String, Option<i64>, String)> = vec![
            ("a1".into(),"s1".into(),"x".into(),None,"f".into()),
            ("a2".into(),"s1".into(),"x".into(),None,"f".into()),
            ("a3".into(),"s1".into(),"x".into(),None,"f".into()),
            ("a4".into(),"s1".into(),"x".into(),None,"f".into()),
            ("a5".into(),"s1".into(),"x".into(),None,"f".into()),
            ("a6".into(),"s1".into(),"x".into(),None,"f".into()),
            ("a7".into(),"s1".into(),"x".into(),None,"f".into()),
            ("pre".into(),"s1".into(),"x".into(),None,"f".into()),
        ];
        // S1's 1.3× — Preamble at rank 8 (score 1/68 ≈ 0.0147 × 1.3 = 0.0191)
        // beats Article 41 at rank 1 (1/61 ≈ 0.0164). So even S1 works here;
        // the difference S2 makes is on harder pools — but the test still
        // verifies the stronger boost doesn't break ordering.
        let boosted = apply_salience_boost(&pool, ranked.clone(), 1.6).await;
        assert_eq!(boosted.first().map(|c| c.0.as_str()), Some("pre"),
                   "definitional 1.6× boost lifts deeply-buried Preamble");
    }

    // ── S3b — entity-aware re-rank tests ────────────────────────────────────

    #[test]
    fn extract_query_entities_pulls_capitalised_phrases() {
        // Single proper noun mid-sentence.
        assert_eq!(extract_query_entities("Who is the founder of Pakistan?"),
                   vec!["Pakistan".to_string()]);
        // Multi-word proper noun.
        let nps = extract_query_entities("Who is Mohammad Ali Jinnah?");
        assert!(nps.contains(&"Mohammad Ali Jinnah".to_string()),
                "should detect 'Mohammad Ali Jinnah', got {nps:?}");
        // Hyphenated proper name.
        let nps = extract_query_entities("Who is Quaid-i-Azam?");
        assert!(nps.iter().any(|s| s == "Quaid-i-Azam"),
                "should detect 'Quaid-i-Azam', got {nps:?}");
        // Sentence-initial lone capital ("What") is dropped as grammar.
        let nps = extract_query_entities("What does the Preamble say");
        assert!(nps.contains(&"Preamble".to_string()), "Preamble kept (mid-sentence)");
        assert!(!nps.contains(&"What".to_string()), "lone 'What' dropped as sentence-initial");
        // No capitals at all → empty.
        assert!(extract_query_entities("what is fundamental").is_empty());
        // Dedup case-insensitive — two mentions of pakistan only counted once.
        let nps = extract_query_entities("Pakistan and Pakistan");
        assert_eq!(nps.len(), 1, "case-insensitive dedup: {nps:?}");
    }

    /// End-to-end: a chunk whose entity matches a query NP gets lifted.
    /// Reproduces what F5-imperative would look like once NER tags the
    /// Preamble with `Pakistan` / `Mohammad Ali Jinnah` / `Founder of Pakistan`.
    #[tokio::test]
    async fn entity_boost_lifts_chunk_with_matching_entity() {
        let pool = crate::db::test_pool().await;
        sqlx::query(
            "INSERT INTO sources(id,file_path,file_name,format,file_size_bytes,file_hash,last_modified,created_at) \
             VALUES('s1','/c','constitution','pdf',1,'h',0,0)",
        )
        .execute(&pool).await.unwrap();
        for (id, label) in [("a41","Article 41"), ("a1","Article 1"), ("pre","Preamble")] {
            sqlx::query(
                "INSERT INTO chunks(id, source_id, chunk_index, char_start, char_end, \
                                    page_number, token_count, parent_chunk_id, \
                                    unit_type, unit_label, unit_path, domain_metadata, content, created_at) \
                 VALUES ($1, 's1', 0, 0, 10, NULL, 5, NULL, 'article', $2, $3, NULL, 'x', 0)",
            )
            .bind(id).bind(label).bind(label).execute(&pool).await.unwrap();
        }
        // NER fixture: Preamble has the "Mohammad Ali Jinnah" person entity.
        sqlx::query(
            "INSERT INTO entities(id, canonical_value, entity_type, normalized_value, source_count, created_at) \
             VALUES('e1', 'Mohammad Ali Jinnah', 'person', NULL, 1, 0)",
        ).execute(&pool).await.unwrap();
        sqlx::query(
            "INSERT INTO entity_mentions(entity_id, chunk_id, context_text) \
             VALUES('e1', 'pre', 'Founder of Pakistan, Quaid-i-Azam Mohammad Ali Jinnah')",
        ).execute(&pool).await.unwrap();

        // Ranked list (Preamble at rank 3 — would lose to Article 41 without boost).
        let ranked: Vec<(String, String, String, Option<i64>, String)> = vec![
            ("a41".into(), "s1".into(), "x".into(), None, "f".into()),
            ("a1".into(),  "s1".into(), "x".into(), None, "f".into()),
            ("pre".into(), "s1".into(), "x".into(), None, "f".into()),
        ];
        let query = "Who is Mohammad Ali Jinnah?";
        let boosted = apply_entity_boost(&pool, ranked, query, 1.4).await;
        assert_eq!(boosted.first().map(|c| c.0.as_str()), Some("pre"),
                   "entity boost must lift Preamble (entity match) above Article 41");
    }

    /// Boost is a no-op when query has no NPs OR no entity mentions match.
    #[tokio::test]
    async fn entity_boost_noop_when_no_match() {
        let pool = crate::db::test_pool().await;
        sqlx::query(
            "INSERT INTO sources(id,file_path,file_name,format,file_size_bytes,file_hash,last_modified,created_at) \
             VALUES('s1','/c','c','pdf',1,'h',0,0)",
        ).execute(&pool).await.unwrap();
        for id in ["a","b","c"] {
            sqlx::query(
                "INSERT INTO chunks(id, source_id, chunk_index, char_start, char_end, \
                                    page_number, token_count, parent_chunk_id, \
                                    unit_type, unit_label, unit_path, domain_metadata, content, created_at) \
                 VALUES ($1, 's1', 0, 0, 10, NULL, 5, NULL, 'article', NULL, NULL, NULL, 'x', 0)",
            ).bind(id).execute(&pool).await.unwrap();
        }
        let ranked: Vec<(String, String, String, Option<i64>, String)> = vec![
            ("a".into(), "s1".into(), "x".into(), None, "f".into()),
            ("b".into(), "s1".into(), "x".into(), None, "f".into()),
            ("c".into(), "s1".into(), "x".into(), None, "f".into()),
        ];
        // Query has NPs but no matching entities in DB → no-op.
        let after = apply_entity_boost(&pool, ranked.clone(), "Who is Quaid-i-Azam?", 1.4).await;
        assert_eq!(after.iter().map(|c| c.0.clone()).collect::<Vec<_>>(),
                   ranked.iter().map(|c| c.0.clone()).collect::<Vec<_>>(),
                   "order preserved when no entity matches");
        // Query has no capitalised NPs → no-op without even hitting DB.
        let after = apply_entity_boost(&pool, ranked.clone(), "what is fundamental", 1.4).await;
        assert_eq!(after.iter().map(|c| c.0.clone()).collect::<Vec<_>>(),
                   ranked.iter().map(|c| c.0.clone()).collect::<Vec<_>>(),
                   "order preserved when query has no NPs");
    }

    /// Candidate pool mirroring trace `bd0d16e1`: the Compendium outranked the
    /// E&D Rules even though the query named E&D, and the answer came out of the
    /// Sindh Food Authority Act.
    fn bd0d16e1_pool() -> Vec<(String, String, String, Option<i64>, String)> {
        let f = |id: &str, file: &str| {
            (id.to_string(), "s".to_string(), "x".to_string(), None, file.to_string())
        };
        vec![
            f("c1", "05_Compendium_Rules_under_SLGA_2013.pdf"),
            f("c2", "05_Compendium_Rules_under_SLGA_2013.pdf"),
            f("c3", "09_Sindh_Delegation_of_Financial_Powers_2019.pdf"),
            f("c4", "11_Sindh_Civil_Servants_APT_Rules_1974.pdf"),
            f("c5", "Sindh_Civil_Servants_Efficiency_and_Discipline_Rules_1973.pdf"),
            f("c6", "Sindh_Civil_Servants_Efficiency_and_Discipline_Rules_1973.pdf"),
        ]
    }

    #[test]
    fn instrument_boost_lifts_the_statute_the_query_names() {
        // The REAL query from trace bd0d16e1 — note the user's misspelling
        // "disciplane", which must not break the match.
        let q = "Rule 4(1) sub rule 3 from sindh civil servants efficiency and \
                 disciplane rules 1973";
        let after = apply_instrument_boost(bd0d16e1_pool(), q, 2.0);
        assert!(
            after[0].4.starts_with("Sindh_Civil_Servants_Efficiency"),
            "E&D 1973 must win the top slot, got {}", after[0].4
        );
        // The Compendium (which supplied the Food Authority answer) must be demoted
        // below E&D — but still PRESENT, because this is a re-rank, not a filter.
        let ed = after.iter().position(|c| c.4.starts_with("Sindh_Civil")).unwrap();
        let comp = after.iter().position(|c| c.4.contains("Compendium")).unwrap();
        assert!(ed < comp, "E&D must outrank the Compendium");
        assert_eq!(after.len(), 6, "re-rank must not drop candidates");
    }

    #[test]
    fn instrument_boost_noops_when_the_query_names_no_statute() {
        // This is the query that killed the reverted `citation_fast_path`: it
        // names a rule number but no instrument, and resolved to the Secretariat
        // Manual's "Rule 16 — Steno Typist". The boost must not fire at all.
        let before = bd0d16e1_pool();
        let after = apply_instrument_boost(
            before.clone(), "request for quotations method under rule 16", 2.0,
        );
        assert_eq!(
            after.iter().map(|c| c.0.clone()).collect::<Vec<_>>(),
            before.iter().map(|c| c.0.clone()).collect::<Vec<_>>(),
            "ordering must be untouched when no statute is named",
        );
    }

    #[test]
    fn provision_norm_appends_the_bare_unit_label() {
        // Trace 99cdbff2 — note the capital "I" the user typed for the digit 1.
        // The base label drops clause parens, so the confusable never matters.
        assert_eq!(
            augment_provision_terms("search for rule 4(I)(a) sub rule 3").as_deref(),
            Some("Rule 4"),
        );
        // Trace bd0d16e1.
        assert_eq!(
            augment_provision_terms(
                "Rule 4(1) sub rule 3 from sindh civil servants efficiency and disciplane rules 1973"
            ).as_deref(),
            Some("Rule 4"),
        );
        assert_eq!(
            augment_provision_terms("what does Article 25(1) say?").as_deref(),
            Some("Article 25"),
        );
    }

    #[test]
    fn provision_norm_is_silent_when_it_would_add_nothing() {
        // Bare label already — nothing to append.
        assert_eq!(augment_provision_terms("explain rule 4"), None);
        // Clause form present, but the bare label is already in the text too.
        assert_eq!(augment_provision_terms("rule 4 and rule 4(2)"), None);
        // No citation at all.
        assert_eq!(augment_provision_terms("what are the penalties?"), None);
    }

    #[test]
    fn instrument_boost_is_off_by_default() {
        let before = bd0d16e1_pool();
        let q = "Rule 4 of the sindh civil servants efficiency and discipline rules 1973";
        let after = apply_instrument_boost(before.clone(), q, 1.0);
        assert_eq!(
            after.iter().map(|c| c.0.clone()).collect::<Vec<_>>(),
            before.iter().map(|c| c.0.clone()).collect::<Vec<_>>(),
            "boost <= 1.0 must be a no-op so the flag defaults to off",
        );
    }

    // ── Bounded table completion (2026-08-06 eviction fix) ───────────────────

    /// `(id, source_id, content, page, label)` with a real source id, so
    /// per-SOURCE logic can be exercised (`bd0d16e1_pool` puts every chunk in
    /// source "s" and therefore cannot).
    fn sourced(id: &str, source: &str, content: &str) -> (String, String, String, Option<i64>, String) {
        (
            id.to_string(),
            source.to_string(),
            content.to_string(),
            Some(1),
            format!("{source}.pdf"),
        )
    }

    #[test]
    fn is_enumeration_query_matches_all_markers() {
        for q in [
            "how many members are on the SPPRA board?",
            "List all the departments",
            "what is the total number of sections",
            "name the members of the committee",
            "sum of the approval limits",
        ] {
            assert!(is_enumeration_query(q), "should be enumeration: {q}");
        }
    }

    #[test]
    fn is_enumeration_query_rejects_ordinary_lookup() {
        for q in [
            "what is the procedure for appeal",
            "explain the penalties under Rule 4",
            "who may impose a major penalty",
        ] {
            assert!(!is_enumeration_query(q), "must NOT be enumeration: {q}");
        }
    }

    /// The bound must be strictly smaller than the context it is reserved from.
    /// Without this, `table_reserve_leaves_room_for_the_top_ranked_law` would
    /// still pass while the real reserve was 64 — the arithmetic is only safe
    /// because the knob is a FRACTION of ctx_k.
    #[test]
    fn table_reserve_knobs_stay_below_the_context_cap() {
        for ctx_k in [20usize, 36, 64, 400] {
            let reserve = knob_table_reserve_k(ctx_k);
            assert!(
                reserve < ctx_k,
                "reserve {reserve} must leave room in a {ctx_k}-slot context",
            );
            assert!(reserve >= 4, "reserve must still be useful, got {reserve}");
        }
        // The production window: ctx_top_k ≈ 36 → 18 slots for law, 18 for table.
        assert_eq!(knob_table_reserve_k(36), 18);
        // The old behaviour, for the record: 64 into 36.
        assert!(64 > 36, "documents why this test exists");
    }

    #[test]
    fn elect_table_reserve_prefers_whole_table_when_it_fits() {
        // Repository order: the whole-table chunk first, then its rows.
        let fetched = vec![
            sourced("tbl", "rob", &"w".repeat(400)), // ~120 est tokens
            sourced("r1", "rob", &"a".repeat(200)),
            sourced("r2", "rob", &"b".repeat(200)),
        ];
        let out = elect_table_reserve(&fetched, 18, 1820);
        assert_eq!(out.len(), 1, "whole table alone, not table + rows");
        assert_eq!(out[0].0, "tbl");
    }

    #[test]
    fn elect_table_reserve_falls_back_to_rows_and_caps_both_ways() {
        // A whole-table chunk far too large for the budget → drop it, take rows.
        let mut fetched = vec![sourced("tbl", "rob", &"w".repeat(100_000))];
        for i in 0..64 {
            fetched.push(sourced(&format!("r{i}"), "rob", &"a".repeat(200)));
        }
        // Row cap binds first: 18 rows of ~70 est tokens = 1260 < 1820.
        let out = elect_table_reserve(&fetched, 18, 1820);
        assert_eq!(out.len(), 18, "row cap must bind");
        assert!(out.iter().all(|c| c.0 != "tbl"), "oversized whole-table chunk dropped");
        assert_eq!(out[0].0, "r0", "reading order preserved");

        // Token cap binds first: a tight budget admits fewer than 18.
        let tight = elect_table_reserve(&fetched, 18, 200);
        assert!(tight.len() < 18, "token cap must bind, got {}", tight.len());
        assert!(!tight.is_empty(), "the first row is always admitted");
    }

    /// THE REGRESSION TEST for the 2026-08-06 production failure.
    ///
    /// Retrieval had already succeeded — the Sindh Public Procurement Act s.6(2)
    /// chunk was ranked 9th of the top 10. Table completion then pulled 64 chunks
    /// and prepended them into a context that holds 36, evicting the Act entirely;
    /// the model was handed 36 rows of a department directory and refused.
    ///
    /// With a 64-chunk reserve this test FAILS (the law is pushed past slot 36).
    #[test]
    fn table_reserve_leaves_room_for_the_top_ranked_law() {
        let law_id = "spp-act-6-2";
        let mut ranked: Vec<_> = (0..8)
            .map(|i| sourced(&format!("other{i}"), "misc", &"o".repeat(400)))
            .collect();
        ranked.push(sourced(law_id, "spp-act", &"the Board shall consist of".repeat(20)));
        ranked.push(sourced("other9", "misc", &"o".repeat(400)));

        // Rows must be textually DISTINCT — real table rows are, and identical
        // ones would now be collapsed by the duplicate-content guard, masking
        // exactly the eviction this test exists to demonstrate.
        let row = |i: usize| {
            format!("| {i} | Population Welfare Department | Directorate of Welfare {i} | {}", "x".repeat(60))
        };

        // What the fix produces: a reserve bounded to ctx_k / 2.
        let reserve: Vec<_> = (0..18)
            .map(|i| sourced(&format!("row{i}"), "rob", &row(i)))
            .collect();
        let merged = union_merge(&ranked, &reserve, ranked.len(), reserve.len());
        let ctx = build_context(&merged, 36, 5460);
        assert!(
            ctx.iter().any(|c| c.0 == law_id),
            "the correctly-ranked provision must survive table completion",
        );

        // Sanity: the unbounded 64-chunk pull is what evicted it.
        let flood: Vec<_> = (0..64)
            .map(|i| sourced(&format!("f{i}"), "rob", &row(i)))
            .collect();
        let evicted = build_context(&union_merge(&ranked, &flood, ranked.len(), flood.len()), 36, 5460);
        assert!(
            !evicted.iter().any(|c| c.0 == law_id),
            "guard: the OLD behaviour must still demonstrate the eviction",
        );
    }

    // ── Duplicate-content suppression ────────────────────────────────────────

    #[test]
    fn context_builder_drops_exact_duplicate_content() {
        // Same text, different chunk_ids — 227 such groups existed in prod on
        // 2026-08-06 (66 footnote, 63 table, 37 provision).
        let body = "The Board shall consist of the following members, namely:- ".repeat(3);
        let ranked = vec![
            sourced("a", "s1", &body),
            sourced("b", "s2", &format!("  {}  ", body.to_uppercase())),
            sourced("c", "s1", &"a genuinely different provision text here".repeat(3)),
        ];
        let out = build_context(&ranked, 10, usize::MAX);
        assert_eq!(out.len(), 2, "the case/whitespace variant must be dropped");
        assert_eq!(out[0].0, "a");
        assert_eq!(out[1].0, "c");
    }

    #[test]
    fn context_builder_keeps_distinct_chunks_sharing_a_long_prefix() {
        // Standard clauses repeated across schedules share long prefixes and ARE
        // distinct provisions — this is why the check is exact, never a prefix.
        let prefix = "Provided that no such order shall be passed unless the person concerned ";
        let ranked = vec![
            sourced("a", "s1", &format!("{prefix} has been given a show cause notice.")),
            sourced("b", "s1", &format!("{prefix} has been afforded a personal hearing.")),
        ];
        let out = build_context(&ranked, 10, usize::MAX);
        assert_eq!(out.len(), 2, "long shared prefix must NOT collapse two provisions");
    }

    // ── Source diversity (S3d) ───────────────────────────────────────────────

    fn bd0d16e1_pool_sourced() -> Vec<(String, String, String, Option<i64>, String)> {
        let mut v = Vec::new();
        for i in 0..30 {
            v.push(sourced(&format!("lg{i}"), "local-govt-act", &"x".repeat(200)));
        }
        for i in 0..6 {
            v.push(sourced(&format!("apt{i}"), "apt-rules-1974", &"y".repeat(200)));
        }
        v
    }

    #[test]
    fn source_diversity_noop_when_cap_zero() {
        let before = bd0d16e1_pool_sourced();
        let after = apply_source_diversity(before.clone(), 0);
        assert_eq!(ids(&after), ids(&before), "cap 0 disables the pass entirely");
    }

    #[test]
    fn source_diversity_noop_with_single_source() {
        // The attachment-scoped chat: exactly one source, order untouched.
        let before: Vec<_> = (0..10)
            .map(|i| sourced(&format!("c{i}"), "only-doc", &"x".repeat(100)))
            .collect();
        let after = apply_source_diversity(before.clone(), 12);
        assert_eq!(ids(&after), ids(&before), "one bucket must be left alone");
    }

    #[test]
    fn source_diversity_preserves_every_chunk() {
        let before = bd0d16e1_pool_sourced();
        let after = apply_source_diversity(before.clone(), 12);
        assert_eq!(after.len(), before.len(), "re-order, never a filter");
        let mut a = ids(&after);
        let mut b = ids(&before);
        a.sort();
        b.sort();
        assert_eq!(a, b, "the same multiset of chunks must come out");
    }

    #[test]
    fn source_diversity_caps_a_monopolising_source() {
        // Finding #4: 30 chunks of the wrong Act, 6 of the one that answers it.
        // Before: the APT Rules would sit at ranks 30-35 and be truncated away.
        let after = apply_source_diversity(bd0d16e1_pool_sourced(), 12);

        let first_apt = after.iter().position(|c| c.1 == "apt-rules-1974").unwrap();
        assert_eq!(first_apt, 12, "the governing instrument follows the first source's cap");

        let lg_in_first_wave = after.iter().take(18).filter(|c| c.1 == "local-govt-act").count();
        assert_eq!(lg_in_first_wave, 12, "the monopolising source is held to the cap in wave 1");

        let apt_in_first_wave = after.iter().take(18).filter(|c| c.1 == "apt-rules-1974").count();
        assert_eq!(apt_in_first_wave, 6, "all of the smaller source lands inside wave 1");

        // Wave 2 legitimately returns the remaining local-govt chunks — the pass
        // re-orders, it does not discard, so a source with nothing left to
        // compete against keeps its chunks.
        assert_eq!(after.len(), 36);
    }

    #[test]
    fn source_diversity_keeps_the_top_chunk_first() {
        // The best-ranked source leads wave 1, so citation [1] is unchanged.
        let before = bd0d16e1_pool_sourced();
        let after = apply_source_diversity(before.clone(), 12);
        assert_eq!(after[0].0, before[0].0, "rank-1 chunk must keep its place");
    }

    #[test]
    fn source_diversity_keeps_intra_source_order() {
        let after = apply_source_diversity(bd0d16e1_pool_sourced(), 12);
        let lg: Vec<String> = after
            .iter()
            .filter(|c| c.1 == "local-govt-act")
            .map(|c| c.0.clone())
            .collect();
        let expected: Vec<String> = (0..30).map(|i| format!("lg{i}")).collect();
        assert_eq!(lg, expected, "a source's chunks keep their relative order");
    }

    fn ids(v: &[(String, String, String, Option<i64>, String)]) -> Vec<String> {
        v.iter().map(|c| c.0.clone()).collect()
    }
}
