//! T016–T019 — the **guidance** capability (single-pass): retrieve → allocate →
//! grounded answer, general-answer-first with an explicit offer to tailor
//! (FR-003/FR-004). The answer is citation-gated: any provision the retrieved
//! context cannot support verbatim is suppressed and surfaced as an
//! offer-to-index flag (FR-006/FR-007/FR-008).
//!
//! This is also the Phase-3 bridge for capabilities whose dedicated handlers
//! land later — it runs the proven single-pass grounded pipeline for any
//! `Capability`, selecting that capability's persona fragment + generation
//! reserve, while only `Guidance` adds the offer-to-tailor.

use std::sync::Arc;
use crate::compat::{AppHandle, Emitter};

use crate::db::Db;
use crate::services::assistant::{
    self, context_budget::{BudgetPlan, EstimateCounter}, intent::Capability,
    memory::MemoryContext, persona::PersonaConfig, AssembleInput, CitationOut, TurnResponse,
};
use crate::services::{
    embedder::EmbedderState,
    hardware_detector::HardwareTier,
    llm::{ModelSlot, GROUNDING_SYSTEM_PROMPT},
    query_router::{self, Citation, HistoryTurn},
    query_trace::QueryTrace,
    reranker::RerankerSlot,
    scheduler::{Scheduler, WorkloadKind},
};

/// Build the system prompt for a single-pass turn: the bureaucrat persona + the
/// capability's prompt fragment + the grounding/citation rules. The grounding
/// rules are appended verbatim so the `[n]` citation machinery the retrieval
/// pipeline depends on stays intact (FR-006); the persona supplies the voice
/// (FR-002) and the fragment the general-answer-first behavior (FR-003/FR-004).
/// The compact persona is selected on small windows.
pub fn system_prompt(cfg: &PersonaConfig, cap: Capability, compact: bool) -> String {
    system_prompt_with_offer(cfg, cap, compact, true)
}

/// As `system_prompt`, but with explicit control over the follow-up offer
/// (2026-07-29). `show_offer=false` omits the persona's offer fragment
/// ENTIRELY rather than instructing the model to suppress it — a negative
/// instruction ("do not offer…") is unreliable and still costs prompt tokens.
/// Driven by the per-user `refinement_offer` preference; defaults to `true` so
/// every existing call site keeps today's behaviour.
pub fn system_prompt_with_offer(
    cfg: &PersonaConfig,
    cap: Capability,
    compact: bool,
    show_offer: bool,
) -> String {
    // Small-model profile (NOVIQ_PROMPT_PROFILE=compact): the compact grounding
    // prompt already carries the persona voice and, for guidance, the refine
    // offer — layering the full stack on top is what made Gemma-1B refuse
    // answerable queries. Non-guidance capabilities keep their task fragment so
    // draft/compare/… still know what to produce.
    if crate::services::llm::compact_prompt_profile() {
        return if cap == Capability::Guidance {
            crate::services::llm::GROUNDING_SYSTEM_PROMPT_COMPACT.to_string()
        } else {
            format!(
                "{}\n\n{}",
                cfg.capability_prompt(cap).trim(),
                crate::services::llm::GROUNDING_SYSTEM_PROMPT_COMPACT,
            )
        };
    }
    let task = match (show_offer, cfg.capability_offer(cap)) {
        (true, Some(offer)) => format!("{}\n\n{}", cfg.capability_prompt(cap).trim(), offer),
        _ => cfg.capability_prompt(cap).trim().to_string(),
    };
    format!(
        "{}\n\n{}\n\n{}",
        cfg.persona_text(compact).trim(),
        task,
        GROUNDING_SYSTEM_PROMPT,
    )
}

/// Appended to the grounding system prompt when a turn is scoped to a single live
/// chat attachment (scope-routing Phase 3). Cancels the persona's "general answer
/// first / state the governing laws" instruction — which, with only a show-cause
/// notice in scope, made the model expound on procurement law it never retrieved
/// (trace 376bc113) — and requires a clean, subject-named refusal that the UI
/// pairs with the visible "Search all documents" widen button.
pub(crate) const ATTACHMENT_SCOPE_OVERRIDE: &str = "\n\n\
=== SCOPE OVERRIDE — single attached document ===\n\
You are answering STRICTLY from the ONE document the user attached to this chat; it is the only source in scope. This overrides any earlier instruction to give a general answer first or to state \"the governing laws\".\n\
- Use ONLY the attached document's retrieved passages. Do NOT add general legal background, standard procedure, or governing laws from memory or training, and do NOT speculate about how the subject \"may\" relate to other rules or laws.\n\
- If the attached document does not itself contain the specific rule, section, figure, or information asked about, do NOT improvise a general answer. Reply with ONE sentence in exactly this form and nothing else: \"The attached document does not contain <the subject asked about>.\" — naming the subject in a few words (for example, \"the procurement rules\"). The interface separately offers the user a button to search all documents.";

/// A budget plan for the web-only path, which never assembles a legal prompt and
/// so has no real allocation to report. The audit exists to prove the LAW fitted
/// the window; on this path there is no law, so reporting zeros is honest rather
/// than inventing numbers that were never measured.
fn plan_stub(cfg: &PersonaConfig) -> BudgetPlan {
    BudgetPlan {
        gen_reserve: cfg.settings.gen_reserve_qa,
        ..Default::default()
    }
}

/// Assemble the gated wire response from the raw generated answer + retrieved
/// citations. Ungrounded provision references are suppressed from the prose and
/// surfaced as offer-to-index flags (FR-007); guidance turns add the
/// offer-to-tailor (FR-004). Pure — unit-tested without a live model.
pub fn build_response(
    conversation_id: &str,
    cap: Capability,
    raw_answer: &str,
    cites: Vec<CitationOut>,
    plan: &BudgetPlan,
    // Measured chat fullness from the router; `None` falls back to `plan`'s
    // post-hoc estimate (which reads low — see `AssembleInput::chat_fill`).
    chat_fill: Option<crate::services::query_router::ChatFill>,
    cfg: &PersonaConfig,
) -> TurnResponse {
    // Gate haystack: the verbatim passages backing these citations. Built from
    // ALL retrieved passages so the citation gate can still validate grounding.
    let context = cites.iter().map(|c| c.verbatim.clone()).collect::<Vec<_>>().join("\n\n");

    // Drop the citation list on a definite refusal, so "that isn't in the
    // selected document" doesn't ship with stray sources it never used. Precise
    // by construction: only when the answer BOTH cites nothing (`[n]`-free) AND
    // reads as a refusal — a real answer that merely contains a negation ("Rule
    // 16 does not cover X, but Rule 42 does [2]") keeps its citation. Retrieved-
    // but-unused passages still ground the gate above.
    let shown = if is_bare_refusal(raw_answer) { Vec::new() } else { cites };

    assistant::assemble_response(AssembleInput {
        conversation_id: conversation_id.to_string(),
        capability: cap,
        answer: raw_answer.to_string(),
        context: &context,
        citations: shown,
        plan,
        fill_threshold: cfg.settings.context_fill_threshold,
        chat_fill,
        offer_tailor: cap == Capability::Guidance,
    })
}

/// A "definite refusal": the answer cites no source (`[n]`-free) AND uses
/// not-in-the-source language. Both conditions are required so an ordinary answer
/// that happens to negate ("… does not cover …") but still cites a provision is
/// never mistaken for a refusal.
/// The most recent question the officer asked, for carrying a follow-up's
/// SUBJECT into a web search. `history` is oldest-first, so this scans back.
fn last_user_question(history: &[HistoryTurn]) -> Option<&str> {
    history
        .iter()
        .rev()
        .find(|t| t.role == "user")
        .map(|t| t.content.trim())
        .filter(|s| !s.is_empty())
}

fn is_bare_refusal(answer: &str) -> bool {
    use regex_lite::Regex;
    use std::sync::OnceLock;
    static CITE: OnceLock<Regex> = OnceLock::new();
    let has_citation = CITE.get_or_init(|| Regex::new(r"\[\d+\]").unwrap()).is_match(answer);
    if has_citation {
        return false;
    }
    let a = answer.to_lowercase();
    const REFUSAL: &[&str] = &[
        "not found in", "not covered in", "not addressed in", "not mentioned in",
        "not available in", "not present in", "not part of the selected",
        "cannot find", "could not find", "couldn't find", "unable to find",
        "isn't in the", "is not in the", "does not appear in", "doesn't appear in",
        "no information", "outside the scope", "not in the selected",
        "select a document", "attach a", "no relevant",
        // scope-routing Phase 3 attachment-scope refusal (ATTACHMENT_SCOPE_OVERRIDE).
        "does not contain",
    ];
    REFUSAL.iter().any(|p| a.contains(p))
}

/// Tester feedback 2026-07-28: an `unresolved` chunk's unit WORD is a guess the
/// pipeline explicitly declined to stand behind — showing "Schedule 6" for what
/// is really Article 16(1) is worse than showing the bare parent-child ordinal.
/// Unresolved labels drop their leading unit word ("Schedule 6" → "6",
/// "Rule 16(6)" → "16(6)"); resolved labels are shown exactly as stored.
fn display_provision_label(label: Option<&str>, status: Option<&str>) -> String {
    let label = label.unwrap_or_default().trim();
    if status != Some("unresolved") {
        return label.to_string();
    }
    static UNIT: std::sync::OnceLock<regex_lite::Regex> = std::sync::OnceLock::new();
    let re = UNIT.get_or_init(|| {
        regex_lite::Regex::new(
            r"(?i)^(article|rule|section|regulation|schedule|chapter|part|clause|annex(?:ure)?|appendix)\s+",
        )
        .unwrap()
    });
    re.replace(label, "").to_string()
}

/// Resolve retrieval citations into display-ready `CitationOut`: a clean document
/// name (no ordering prefix / extension), the provision label (Rule/Article/…),
/// and the page. Falls back to the raw source id if the DB lookup fails so a
/// trace is never lost to a citation-plumbing error.
pub async fn enrich_citations(pool: &Db, citations: &[Citation]) -> Vec<CitationOut> {
    use std::collections::HashMap;
    let chunk_ids: Vec<String> = citations.iter().map(|c| c.chunk_id.clone()).collect();
    let source_ids: Vec<String> = citations.iter().map(|c| c.source_id.clone()).collect();

    let labels: HashMap<String, (Option<String>, Option<String>)> =
        sqlx::query_as::<_, (String, Option<String>, Option<String>)>(
            "SELECT id::text, unit_label, label_status FROM chunks WHERE id::text = ANY($1)",
        )
        .bind(&chunk_ids)
        .fetch_all(pool)
        .await
        .map(|rows| rows.into_iter().map(|(id, l, s)| (id, (l, s))).collect())
        .unwrap_or_default();

    let names: HashMap<String, String> = sqlx::query_as::<_, (String, String)>(
        "SELECT id::text, file_name FROM sources WHERE id::text = ANY($1)",
    )
    .bind(&source_ids)
    .fetch_all(pool)
    .await
    .map(|rows| rows.into_iter().collect())
    .unwrap_or_default();

    citations
        .iter()
        .enumerate()
        .map(|(i, c)| CitationOut {
            n: (i + 1) as i64,
            instrument: names
                .get(&c.source_id)
                .map(|f| assistant::clean_document_name(f))
                .unwrap_or_else(|| c.source_id.clone()),
            path: labels
                .get(&c.chunk_id)
                .map(|(l, s)| display_provision_label(l.as_deref(), s.as_deref()))
                .unwrap_or_default(),
            verbatim: c.excerpt.clone(),
            page: c.page,
            // The handles the UI needs to open the source at this passage.
            chunk_id: Some(c.chunk_id.clone()),
            source_id: Some(c.source_id.clone()),
            // F027 — stamped after the fact by `label_citation_provenance`,
            // which is the only place that knows which project this turn ran in.
            provenance: None,
        })
        .collect()
}

/// Run a single-pass capability turn end-to-end: retrieve + generate over the
/// proven grounded pipeline (with the bureaucrat system prompt), allocate the
/// window into an audited budget plan (Σ prompt + reserve ≤ window), then gate
/// and assemble. Streams `assistant://progress`.
#[allow(clippy::too_many_arguments)]
pub async fn run(
    pool: &Db,
    embedder: &Arc<EmbedderState>,
    model_slot: &ModelSlot,
    reranker_slot: &RerankerSlot,
    tier: &HardwareTier,
    scheduler: &Arc<Scheduler>,
    // T009b: optional so the headless HTTP server can drive the chat pipeline
    // with `None` (no Tauri runtime). `Some(&app)` from the desktop command keeps
    // live `assistant://progress` events; `None` simply skips them.
    app: Option<&AppHandle>,
    cfg: &PersonaConfig,
    mem: &MemoryContext,
    history: &[HistoryTurn],
    conversation_id: &str,
    text: &str,
    cap: Capability,
    scope_ids: &[String],
    // scope-routing Phase 3: this turn's retrieval was narrowed to a live chat
    // attachment by DEFAULT (no explicit selection, no widen). Tightens the
    // grounding prompt so the model answers strictly from the attached file and
    // refuses cleanly (the UI pairs this with a "Search all documents" widen
    // button) instead of expounding on law it never retrieved.
    scoped_to_attachment: bool,
    // 2026-07-29: per-user `refinement_offer` preference, resolved by `run_turn`
    // (which holds the RequestScope). `false` omits the persona's "Next steps"
    // fragment from the system prompt entirely.
    show_offer: bool,
    // Track 3: the turn-level trace owned by `run_turn`. The retrieval/generation
    // pipeline records its ~30 steps here; the owner writes the file.
    trace: &mut QueryTrace,
) -> Result<TurnResponse, String> {
    let emit_phase = |phase: &str| {
        if let Some(a) = app {
            let _ = a.emit("assistant://progress", serde_json::json!({ "phase": phase }));
        }
    };

    // ── retrieve + generate (proven grounded pipeline, bureaucrat prompt) ─────
    emit_phase("retrieve");
    // F021: ALWAYS pass the scope set. The old `empty → None` fallback failed
    // OPEN (None = notebook-wide retrieval); an empty visible set must retrieve
    // nothing — get_context_chunks/fts5_retrieve return no rows for Some([]).
    let scope_ref: Option<&[String]> = Some(scope_ids);
    let mut sys = system_prompt_with_offer(cfg, cap, false, show_offer);
    // Ordinal turn-recall (graph-memory tier 2). "what was my first question?",
    // "compare the first one with the third one": the officer references a turn by
    // POSITION, not content, so it cannot be embedded and is usually already
    // evicted from the verbatim window — the model otherwise names the oldest turn
    // it can still see (prod audit 2026-07-16: Q6 answered Q3). A deterministic seq
    // lookup over the FULL turn history (incl. summarized turns) injects the exact
    // earlier turn(s). Appended to `sys`, which `route_scoped`'s `law_overhead`
    // MEASURES (query_router.rs:861), so the law budget shrinks to fit and the
    // prompt cannot overflow — the reason the earlier UNMEASURED `memory_block`
    // injection was reverted (80048b37). Best-effort; empty when no ordinal is
    // referenced (zero cost on the common turn). Reaches deep too (it wraps {sys}).
    match assistant::memory::ordinal_recall_block(pool, conversation_id, text).await {
        Ok(recall) if !recall.is_empty() => sys.push_str(&recall),
        Ok(_) => {}
        Err(e) => log::warn!("ordinal_recall_block failed (non-fatal): {e}"),
    }
    // Computed figures from an attached spreadsheet, for the turns that need the
    // sheet AND the corpus. Appended to `sys` by the same measured mechanism as
    // the recall block above, so `law_overhead` shrinks the law budget to fit
    // and the prompt cannot overflow. The model is thereby left with a
    // comparison against a retrieved rule, never with arithmetic — it answered
    // "the FEC share of total cost is 230 million Rs" when asked to divide
    // 230 by 850 (prod, 2026-08-03).
    let sheet_facts = super::tabular::facts_for_turn(pool, conversation_id).await;
    if !sheet_facts.is_empty() {
        sys.push_str(&sheet_facts);
    }
    // scope-routing Phase 3: strictly inside a single attached document → cancel
    // the persona's "give the governing laws / general answer first" instruction
    // so the model never fills the gap with un-retrieved law (the show-cause
    // speculation bug, trace 376bc113). Applied to `sys` before BOTH the deep and
    // single-pass branches below, so both honour the tighter scope.
    if scoped_to_attachment {
        sys.push_str(ATTACHMENT_SCOPE_OVERRIDE);
    }

    // ── Web search: an EXCLUSIVE mode, not a second lane (2026-07-29) ────────
    //
    // When web search is on the indexed corpus is NOT consulted at all. This is
    // a research mode, and the exclusivity is a safety property rather than a
    // simplification.
    //
    // The first design ran both and put web results beside the legal context.
    // The model answered "who is the current President of Pakistan" from the web,
    // tagged it `[5]`, and the citation pipeline renumbered that to `[1]` and
    // bound it to a Constitution chunk whose only link was the phrase "President
    // of Pakistan" (trace 551fb7cd). A web fact shipped wearing a statutory
    // citation.
    //
    // Note that bug was NOT created by web search: with the toggle off, the same
    // question retrieves the same chunks and the model still answers from
    // training and still cites [1]. Web search exposed a pre-existing hole. This
    // mode closes it structurally for web turns — with no legal chunks in
    // context there is no `[n]` to mint and nothing to rebind to — and
    // `strip_legal_markers` enforces it even if the model invents one anyway.
    // An ATTACHED document is not the indexed corpus — it is a file the officer
    // put in this conversation, so web mode reads it. Exclusive web-only applies
    // when there is nothing attached. Without this, "tell me about this document
    // and search the web about it" ignored the attachment entirely and searched
    // for the words "attached document" (Outlook help pages, grammar forums).
    let mut web_results: Vec<crate::services::websearch::WebResult> = Vec::new();
    if query_router::web_search_active() {
        use crate::services::websearch::{self, searxng::SearxngSearcher, WebSearcher};
        emit_phase("web-search");
        let searcher = SearxngSearcher::from_env();
        // When scoped to an attachment, the document is the subject of the
        // search, not the sentence asking about it.
        // For a LEGAL document the useful query is the instruments it CITES, not
        // its title. "Show Casue Notice to Ms Sadaf Mughal lecturer in commerce"
        // searches for a private individual with no web presence, and returned a
        // department homepage and a generic how-to page. The Rules the notice
        // rests on are what the officer wants reference law about.
        let (doc_title, instruments): (Option<String>, Vec<String>) = if scoped_to_attachment {
            let title = crate::db::repositories::sources::display_names_for(pool, scope_ids)
                .await
                .ok()
                .and_then(|v| v.into_iter().next());
            let body = crate::db::repositories::sources::head_text_for(pool, scope_ids, 4000)
                .await
                .unwrap_or_default();
            (title, websearch::instruments_in(&body))
        } else {
            (None, Vec::new())
        };
        // A FOLLOW-UP CARRIES ITS SUBJECT FORWARD. "what amendments were made to
        // SPPR in 2024?" then "and in 2023?" searched the public web for the word
        // "2023" — no subject at all — and no prompt context can rescue results
        // that were never fetched. On a follow-up the previous question is
        // prepended before the filler-stripping runs, so the subject survives.
        //
        // NOT `extract_carryover_keywords`: its stop-list drops "sindh", "rules",
        // "procurement" because for CORPUS retrieval those match everything. On
        // the open web they are precisely the disambiguating words.
        let effective_question = match (
            query_router::is_followup_query(text),
            last_user_question(history),
        ) {
            (true, Some(prev)) => format!("{prev} {text}"),
            _ => text.to_string(),
        };
        // Instruments win when the document names any; otherwise fall back to the
        // title, which is still better than the raw sentence.
        let query = websearch::sanitize_query(&if instruments.is_empty() {
            websearch::build_query(&effective_question, doc_title.as_deref())
        } else {
            format!("{} {}", instruments.join(" "), websearch::build_query(&effective_question, None))
        });
        // Ask for more than we show, then rank: SearXNG orders for general web
        // relevance, which is not the same as usefulness for a statute question.
        let results = match searcher.search(&query, websearch::DEFAULT_RESULT_LIMIT * 2).await {
            Ok(r) => {
                let mut r = websearch::rank_results(&query, r);
                r.truncate(websearch::DEFAULT_RESULT_LIMIT);
                r
            }
            Err(e) => {
                log::warn!("web search failed: {e}");
                trace.step("Web search", &[("Failed", e.to_string())]);
                Vec::new()
            }
        };
        trace.step("Web search (exclusive mode)", &[
            ("Provider", searcher.name().to_string()),
            ("Results", results.len().to_string()),
            ("Search query", query.clone()),
            ("Query built from", if instruments.is_empty() { "document title / question" } else { "instruments cited in the document" }.to_string()),
            ("Mode", if scoped_to_attachment { "attachment + web (corpus skipped)" } else { "web only" }.to_string()),
            ("Sources", results.iter().map(|r| r.url.clone()).collect::<Vec<_>>().join(", ")),
        ]);

        if scoped_to_attachment {
            // Answer about the DOCUMENT through the normal grounded pipeline
            // (already scoped to the attachment, so `[n]` cites that file and
            // nothing else), with the web block alongside. Appended to `sys`
            // before retrieval so `law_overhead` measures it.
            if !results.is_empty() {
                sys.push_str(websearch::WEB_LANE_PROMPT);
                sys.push_str(&websearch::format_web_block(&results));
                web_results = results;
            }
            // fall through to the normal grounded path below
        } else if results.is_empty() {
            // Say so rather than falling back to the corpus: the officer asked
            // for the web, and silently answering from law under a "Web search
            // on" banner would misrepresent where the answer came from.
            let mut resp = build_response(
                conversation_id, cap,
                "The web search returned no results for this question. Try rephrasing it, or turn web search off to search your indexed documents instead.",
                Vec::new(), &plan_stub(cfg), None, cfg,
            );
            resp.web_results = Vec::new();
            return Ok(resp);
        } else {
        let sys_web = format!("{}{}", websearch::WEB_ONLY_SYSTEM_PROMPT, websearch::format_web_block(&results));
        // The no-think prefix EVERY other prompt builder appends. Omitting it let
        // Qwen3 run its default <think> reasoning pass, which leaked into the
        // answer as duplicated, half-finished prose. `NOVIQ_NO_THINK=1` is set on
        // this host precisely because Qwen3 is the active model.
        // THE CONVERSATION. This lane built its prompt from scratch — rules, web
        // results, current question — and so forgot every earlier turn, while the
        // corpus lane has always carried them. "Summarise what you just told me"
        // had nothing to summarise. Reuses the SAME block the corpus lane injects,
        // so both lanes remember identically: already ChatML-formatted, already
        // capped at 600 chars per message over the persona's configured window,
        // and already EMPTY for a self-contained question — zero cost on the
        // common turn, which matters because the web block is large.
        let prior_turns = query_router::prior_turns_block(text, history);
        trace.step("Web conversation memory", &[
            ("Prior turns injected", if prior_turns.is_empty() { "none (self-contained question)" } else { "yes" }.to_string()),
            ("Block size", format!("{} chars", prior_turns.len())),
            ("Search subject carried forward", (effective_question != text).to_string()),
        ]);
        let prompt = format!(
            "<|im_start|>system
{sys_web}<|im_end|>
{prior_turns}<|im_start|>user
{text}<|im_end|>
<|im_start|>assistant
{no_think}",
            no_think = query_router::no_think_assistant_prefix(),
        );
        trace.step("Web prompt assembled", &[
            ("Prompt size", format!("{} chars", prompt.len())),
            ("No-think prefix", if query_router::no_think_assistant_prefix().is_empty() { "off" } else { "on" }.to_string()),
        ]);
        let handle = scheduler.register(WorkloadKind::Chat);
        scheduler.note_chat_turn(conversation_id, handle.id);
        emit_phase("generate");
        let raw = query_router::generate_direct(
            model_slot, tier, &prompt, 1280,
            Some(handle.cancellation_token()),
            app.cloned(),
        )
        .await
        .map_err(|e| format!("web answer generation failed: {e}"))?;

        // Belt and braces: a bare [n] is ALWAYS wrong here — there is no legal
        // context for it to refer to.
        let answer = websearch::strip_legal_markers(&raw);
        trace.step("Web answer (raw model output)", &[
            ("Answer", raw.chars().take(1500).collect::<String>()),
        ]);
        trace.step("Web answer", &[
            ("Legal [n] markers stripped", (raw.len() - answer.len()).to_string()),
            ("Citations", "none — web sources are returned separately, never as CitationOut".to_string()),
        ]);

        // citations stays EMPTY on purpose: CitationOut means an indexed
        // instrument, and nothing here is one.
            let mut resp = build_response(conversation_id, cap, &answer, Vec::new(), &plan_stub(cfg), None, cfg);
            resp.web_results = results;
            return Ok(resp);
        }
    }

    let handle = scheduler.register(WorkloadKind::Chat);
    // Stop-button seam (F026): expose this turn's workload id under its
    // conversation so an out-of-band `cancel_turn` request can stop it (a wrongly
    // typed query, or a slow deep run the officer no longer wants).
    scheduler.note_chat_turn(conversation_id, handle.id);
    let cancel = handle.cancellation_token();
    emit_phase("generate");
    // F025: when Deep is ON *and* the agentic loop is enabled, route this turn
    // through the bounded multi-pass deep search instead of the single-pass
    // retrieval. Both return `(QueryResponse, bool)` so the downstream (budget plan,
    // citation enrichment, response build) is identical. Deep OFF / flag OFF → the
    // existing single-pass path, byte-for-byte unchanged.
    let result = if query_router::deep_search_active()
        && crate::services::deep_search::agentic_enabled()
    {
        // F026: durable-thread id for this deep turn (resume/time-travel key).
        // Derived from conversation + the question, NOT the per-turn workload id —
        // a per-turn id is fresh every attempt, which made resume unreachable.
        let deep_thread = crate::services::deep_search::deep_thread_id(conversation_id, text);
        crate::services::deep_search::run_agentic_deep(
            pool,
            embedder,
            model_slot,
            tier,
            conversation_id,
            &deep_thread,
            text,
            scope_ref,
            &sys,
            Some(cancel),
            app.cloned(),
            Some(trace),
        )
        .await
    } else {
        query_router::route_scoped_with_override(
            pool,
            embedder,
            model_slot,
            reranker_slot,
            tier,
            conversation_id, // notebook id (session/trace key)
            conversation_id, // session id
            text,
            history,
            scope_ref,
            Some(&sys),
            Some(cancel),
            app.cloned(),
            Some(trace),
        )
        .await
    };
    scheduler.unregister(handle.id);
    scheduler.clear_chat_turn(conversation_id, handle.id);
    let (qr, _cancelled) = result.map_err(|e| e.to_string())?;

    // ── allocate (audited budget plan: Σ prompt + reserve ≤ window) ───────────
    let law: Vec<String> = qr.citations.iter().map(|c| c.excerpt.clone()).collect();
    let window = assistant::resolve_active_window(model_slot, tier);
    let plan = assistant::plan_budget(&EstimateCounter, window, cap, cfg, false, mem, text, &law);
    log::info!(
        "F015 turn cap={:?} window={} prompt_tok={} reserve={} kept_law={}/{} law_overflow={} fits={}",
        cap,
        plan.n_ctx,
        plan.prompt_tokens,
        plan.gen_reserve,
        plan.kept_law,
        law.len(),
        plan.law_overflow,
        plan.fits(),
    );

    let cites = enrich_citations(pool, &qr.citations).await;
    // Web results ride along when an ATTACHMENT was in scope (web mode reads the
    // attached file); empty on an ordinary corpus turn.
    let mut resp = build_response(conversation_id, cap, &qr.answer, cites, &plan, qr.chat_fill, cfg);
    resp.web_results = web_results;
    Ok(resp)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cfg() -> &'static PersonaConfig {
        PersonaConfig::get()
    }

    fn plan() -> BudgetPlan {
        let mem = MemoryContext::default();
        assistant::plan_budget(&EstimateCounter, 8192, Capability::Guidance, cfg(), false, &mem, "q", &[])
    }

    fn turn(role: &str, content: &str) -> HistoryTurn {
        HistoryTurn { role: role.into(), content: content.into() }
    }

    /// The web lane forgot the conversation entirely: it built its prompt from
    /// rules + results + the current sentence, so "and in 2023?" searched the
    /// public web for the bare word "2023" and the model never saw what came
    /// before. These two tests cover the two halves — the SEARCH and the PROMPT.
    #[test]
    fn a_followup_carries_its_subject_into_the_web_search() {
        let history = vec![
            turn("user", "what amendments were made to the Sindh Public Procurement Rules in 2024"),
            turn("assistant", "Rules 17(1), 32(3), 37(5) and 46(1) were amended [W1]."),
        ];
        let prev = last_user_question(&history).expect("previous question");
        assert!(query_router::is_followup_query("and in 2023?"), "precondition");

        // What the search is actually built from on a follow-up.
        let effective = format!("{prev} and in 2023?");
        let q = crate::services::websearch::build_query(&effective, None).to_lowercase();
        assert!(q.contains("procurement"), "subject lost from the query: {q}");
        assert!(q.contains("sindh"), "jurisdiction lost from the query: {q}");
        assert!(q.contains("2023"), "the follow-up's own year is missing: {q}");
    }

    /// A self-contained question must NOT drag the previous one in — that would
    /// pollute the search the way carry-over polluted corpus retrieval before.
    #[test]
    fn a_self_contained_question_does_not_carry_the_previous_subject() {
        assert!(
            !query_router::is_followup_query(
                "what is the procedure for blacklisting a contractor under the SPPRA rules"
            ),
            "a full question must not be treated as a follow-up"
        );
        // …and the prior-turns block is empty for it, so the web prompt pays
        // nothing on the common turn.
        let history = vec![turn("user", "earlier question"), turn("assistant", "earlier answer")];
        assert!(query_router::prior_turns_block(
            "what is the procedure for blacklisting a contractor under the SPPRA rules",
            &history,
        )
        .is_empty());
    }

    /// The prompt half: a follow-up must arrive with the earlier turns attached,
    /// in the same ChatML shape the corpus lane uses.
    #[test]
    fn a_followup_web_prompt_carries_the_earlier_turns() {
        let history = vec![
            turn("user", "what amendments were made to the SPPR in 2024"),
            turn("assistant", "Rules 17(1), 32(3), 37(5) and 46(1) were amended."),
        ];
        let block = query_router::prior_turns_block("and in 2023?", &history);
        assert!(!block.is_empty(), "the web lane would forget the conversation");
        assert!(block.contains("SPPR"), "the earlier question is missing");
        assert!(block.contains("17(1)"), "the earlier answer is missing");
        assert!(block.contains("<|im_start|>user"), "must be ChatML, like the corpus lane");
    }

    #[test]
    fn last_user_question_reads_the_most_recent_one() {
        let history = vec![
            turn("user", "first question"),
            turn("assistant", "first answer"),
            turn("user", "second question"),
            turn("assistant", "second answer"),
        ];
        assert_eq!(last_user_question(&history), Some("second question"));
        assert_eq!(last_user_question(&[]), None);
    }

    /// 2026-07-29 — the per-user "Next steps" offer. Turning it off must REMOVE
    /// the instruction, not add a negative one: a "do not offer…" line is
    /// unreliable and still costs prompt tokens.
    #[test]
    fn offer_fragment_is_present_only_when_enabled() {
        let on = system_prompt_with_offer(cfg(), Capability::Guidance, false, true);
        let off = system_prompt_with_offer(cfg(), Capability::Guidance, false, false);
        assert!(on.contains("Next steps"), "offer missing when enabled:\n{on}");
        assert!(!off.contains("Next steps"), "offer leaked when disabled:\n{off}");
        assert!(off.len() < on.len(), "disabling the offer must shorten the prompt");
        // Turning it off must not disturb the answer instruction or the
        // grounding rules the citation machinery depends on.
        assert!(off.contains("governing laws"), "task fragment lost:\n{off}");
        assert!(
            off.contains("[1]") || off.contains("bracketed source index"),
            "grounding rules lost:\n{off}"
        );
    }

    /// The default entry point keeps today's behaviour, so every existing call
    /// site is unaffected by the new parameter.
    #[test]
    fn system_prompt_defaults_to_showing_the_offer() {
        assert_eq!(
            system_prompt(cfg(), Capability::Guidance, false),
            system_prompt_with_offer(cfg(), Capability::Guidance, false, true),
        );
    }

    /// Only guidance defines an offer — enabling the preference must not make a
    /// draft or a comparison sprout a "Next steps" section.
    #[test]
    fn non_guidance_capabilities_have_no_offer() {
        for cap in [Capability::Draft, Capability::Compare, Capability::ActionItems] {
            assert!(
                cfg().capability_offer(cap).is_none(),
                "{cap:?} unexpectedly carries an offer fragment"
            );
        }
    }

    #[test]
    fn system_prompt_carries_persona_fragment_and_grounding_rules() {
        let sp = system_prompt(cfg(), Capability::Guidance, false);
        assert!(sp.to_lowercase().contains("bureaucrat"), "persona voice present");
        // general-answer-first guidance fragment
        assert!(sp.to_lowercase().contains("offer to tailor") || sp.to_lowercase().contains("offer"), "{sp}");
        // grounding/citation machinery retained
        assert!(sp.contains("[1]") || sp.contains("bracketed source index"), "{sp}");
    }

    #[test]
    fn compact_persona_selected_on_small_window() {
        let full = system_prompt(cfg(), Capability::Guidance, false);
        let compact = system_prompt(cfg(), Capability::Guidance, true);
        assert!(compact.len() < full.len());
    }

    /// The "context window full" notice is driven by the MEASURED chat fullness,
    /// and fullness means "how much of the room for conversation history is gone" —
    /// NOT "how full is the prompt". The prompt is ~always ~100% full because law
    /// expands to fill it, so prompt-fullness would warn on turn one, every turn.
    #[test]
    fn fill_notice_follows_measured_chat_fullness_not_the_prompt() {
        use crate::services::query_router::ChatFill;
        let fill_at = |used: usize| {
            build_response(
                "c1",
                Capability::Guidance,
                "Under Section 18 you apply in writing.",
                vec![],
                &plan(),
                Some(ChatFill { used, limit: 4512 }),
                cfg(),
            )
            .context_fill
        };
        // Turn one: overhead ~1557 of 4512 → ~35%. Quiet.
        let early = fill_at(1557);
        assert!(early.utilization < 0.40, "turn one must not look full: {early:?}");
        assert!(!early.notice, "turn one must not nag: {early:?}");

        // ~10 exchanges: overhead ~3457 → ~77%. Still under the 0.80 default.
        assert!(!fill_at(3457).notice, "ten exchanges is close but not yet full");

        // Past the law floor: the budget can no longer fit the law → say so.
        let full = fill_at(4512);
        assert!((full.utilization - 1.0).abs() < f32::EPSILON, "floored = 100%: {full:?}");
        assert!(full.notice, "a floored budget MUST warn — this is where answers break");
    }

    /// Regression: the notice used to be computed from `plan`, whose "law" is the
    /// CITATION EXCERPTS (what the model quoted), not the ~33 chunks it read. That
    /// estimate lands ~a third when the real prompt is at 94%, never crosses the
    /// 0.80 threshold, and the notice never fired — users watched long chats decay
    /// with no warning. Pins that a measured value overrides the estimate.
    #[test]
    fn measured_fullness_overrides_the_post_hoc_estimate() {
        use crate::services::query_router::ChatFill;
        let with_plan_only = build_response(
            "c1", Capability::Guidance, "answer", vec![], &plan(), None, cfg(),
        );
        let measured_full = build_response(
            "c1", Capability::Guidance, "answer", vec![], &plan(),
            Some(ChatFill { used: 4512, limit: 4512 }), cfg(),
        );
        assert!(!with_plan_only.context_fill.notice, "the old estimate never fires: {with_plan_only:?}");
        assert!(measured_full.context_fill.notice, "the measured one does: {measured_full:?}");
    }

    #[test]
    fn grounded_guidance_keeps_answer_offers_tailor_no_flags() {
        let cites = vec![CitationOut {
            n: 1,
            instrument: "Sindh Civil Servants Act".into(),
            path: "Section 18".into(),
            verbatim: "Section 18 of the Act provides for leave on full pay.".into(),
            page: Some(3),
            ..Default::default()
        }];
        let resp = build_response(
            "c1",
            Capability::Guidance,
            "Under Section 18 you apply in writing.",
            cites,
            &plan(),
            None,
            cfg(),
        );
        assert_eq!(resp.capability, "guidance");
        assert_eq!(resp.offer.as_deref(), Some("tailor"));
        assert!(resp.flags.is_empty(), "grounded answer must not flag: {resp:?}");
        assert!(resp.answer.contains("Section 18"));
        assert_eq!(resp.citations.len(), 1);
    }

    #[test]
    fn definite_refusal_ships_no_citations() {
        // Asked about a document that isn't in scope → the model refuses. The
        // (irrelevant) retrieved passages must NOT ride along as citations.
        let cites = vec![CitationOut {
            n: 1,
            instrument: "Some Other Act".into(),
            path: "Section 3".into(),
            verbatim: "unrelated retrieved text".into(),
            page: None,
            ..Default::default()
        }];
        let resp = build_response(
            "c1",
            Capability::Guidance,
            "That subject is not covered in the selected document.",
            cites,
            &plan(),
            None,
            cfg(),
        );
        assert!(resp.citations.is_empty(), "refusal must drop citations: {resp:?}");
    }

    #[test]
    fn negation_with_citation_keeps_citations() {
        // A real answer that negates but still cites is NOT a refusal.
        let cites = vec![CitationOut {
            n: 2,
            instrument: "SPPRA Rules".into(),
            path: "Rule 42".into(),
            verbatim: "Rule 42 permits …".into(),
            page: None,
            ..Default::default()
        }];
        let resp = build_response(
            "c1",
            Capability::Guidance,
            "Rule 16 does not cover direct contracting, but Rule 42 does [2].",
            cites,
            &plan(),
            None,
            cfg(),
        );
        assert_eq!(resp.citations.len(), 1, "grounded answer keeps its citation: {resp:?}");
    }

    #[test]
    fn ungrounded_reference_is_suppressed_and_flagged() {
        let cites = vec![CitationOut {
            n: 1,
            instrument: "Act".into(),
            path: "Section 18".into(),
            verbatim: "Section 18 provides for leave.".into(),
            page: None,
            ..Default::default()
        }];
        let resp = build_response(
            "c1",
            Capability::Guidance,
            "Under Section 18 you apply. It is also governed by Section 404.",
            cites,
            &plan(),
            None,
            cfg(),
        );
        assert!(!resp.answer.contains("Section 404"), "fabricated ref must be suppressed: {resp:?}");
        assert!(resp.answer.contains("Section 18"));
        assert_eq!(resp.flags.len(), 1);
        assert!(resp.flags[0].offer_to_index);
    }

    #[test]
    fn non_guidance_capability_does_not_offer_tailor() {
        let resp = build_response("c1", Capability::SummarizeDoc, "ok", vec![], &plan(), None, cfg());
        assert_eq!(resp.capability, "summarize_doc");
        assert!(resp.offer.is_none());
    }
}
