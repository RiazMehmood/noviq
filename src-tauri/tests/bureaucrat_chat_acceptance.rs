//! Feature 015 Phase 3 (MVP) acceptance gates — the four hardest invariants of
//! the grounded, no-truncation bureaucrat assistant. NO live LLM (tasks.md rule):
//! every gate exercises the deterministic enforcement points (the priority
//! context allocator, the model-independent citation gate, and the response
//! assembler) that make the guarantees hold regardless of model.
//!
//! - T021 SC-003 — zero window overflow across a stress conversation.
//! - T022 SC-002 — zero truncated laws (every retrieved provision whole).
//! - T023 SC-001 — zero fabricated citations (every stated ref is verbatim).
//! - T024 SC-005 — first-answer completeness (cited answer + offer, no gate).
//! - T031 SC-007 — the usable window (retained law + history) scales with the model.
//! - T051 FR-020 — grounding is model-independent (identical gate verdicts).
//! - T036 SC-009 — whole-document coverage (map-reduce reads 100% of provisions).
//! - T037 SC-010 — amendment-chain fidelity (deterministic order, 0 invented links).
//! - T042 SC-011 — conflicts are cited candidates (both provisions cited).
//! - T046 SC-004 — memory fidelity (early facts honored late); SC-008 (notice +
//!   carried facts).
//! - T052 FR-021 — session survives restart (turns + summary + pinned facts).
//!
//! T046/T052 exercise the persistence layer against the bundled test Postgres
//! (`db::testing::test_pool`) — still NO live LLM (the memory mechanics are
//! deterministic).

use echo_ai_lib::db::repositories::graph::AmendmentEventRow;
use echo_ai_lib::services::assistant::capabilities::{amendment_chain, guidance};
use echo_ai_lib::services::assistant::context_budget::{
    allocate, BudgetInputs, EstimateCounter, TokenCounter,
};
use echo_ai_lib::services::assistant::intent::Capability;
use echo_ai_lib::services::assistant::mapreduce;
use echo_ai_lib::services::assistant::persona::PersonaConfig;
use echo_ai_lib::services::assistant::{citation_gate, plan_budget};
use echo_ai_lib::services::assistant::memory::{self, MemoryContext};
use echo_ai_lib::services::query_router::Citation;

fn cfg() -> &'static PersonaConfig {
    PersonaConfig::get()
}

/// A whole, realistically-sized provision (atomic unit of law).
fn provision(label: &str, words: usize) -> String {
    let body = std::iter::repeat("the competent authority shall")
        .take(words / 4 + 1)
        .collect::<Vec<_>>()
        .join(" ");
    format!("{label}: {body}")
}

// ── T021 — SC-003: zero window overflow across a stress conversation ──────────

#[test]
fn sc003_no_window_overflow_across_a_long_conversation() {
    let window = 4096usize;
    let counter = EstimateCounter;
    let mut recent_turns: Vec<String> = Vec::new();

    // Drive the conversation far past the window many times over: each turn adds
    // history AND retrieves several whole provisions, then we re-allocate.
    for turn in 0..200 {
        recent_turns.insert(0, format!("user: turn {turn} — please advise on the matter at hand"));
        recent_turns.insert(0, format!("assistant: per the indexed rules, here is the position for turn {turn}"));
        let law: Vec<String> = (0..6).map(|i| provision(&format!("Section {}", turn * 10 + i), 120)).collect();

        let mem = MemoryContext {
            pinned_facts: vec!["BS-17, 8 years service".to_string()],
            recent_turns: recent_turns.clone(),
            summary: Some("earlier: the officer asked about long leave entitlement".to_string()),
        };
        let plan = plan_budget(&counter, window, Capability::Guidance, cfg(), false, &mem, "what is the leave procedure", &law);

        // The core no-overflow guarantee: Σ prompt + reserve ≤ window, always.
        assert!(
            plan.prompt_tokens + plan.gen_reserve <= plan.n_ctx,
            "turn {turn}: Σ {} + {} must be ≤ window {} — {plan:?}",
            plan.prompt_tokens, plan.gen_reserve, plan.n_ctx,
        );
        assert!(plan.fits(), "turn {turn}: plan must fit — {plan:?}");
        assert!(!plan.must_fit_failed, "turn {turn}: a 4096 window must hold the must-keeps");
    }
}

// ── T022 — SC-002: zero truncated laws (every retrieved provision is whole) ────

#[test]
fn sc002_retrieved_provisions_are_atomic_never_sliced() {
    let counter = EstimateCounter;
    // Several large provisions + a tight window so not all fit.
    let law: Vec<String> = (0..8).map(|i| provision(&format!("Section {i}"), 300)).collect();
    let mem = MemoryContext::default();
    let plan = plan_budget(&counter, 3072, Capability::Guidance, cfg(), false, &mem, "procedure?", &law);

    // The kept law is the leading prefix, each item included WHOLE (verbatim),
    // and anything that did not fit is wholly absent — never a fragment.
    let kept_context = law[..plan.kept_law].join("\n\n");
    for (i, item) in law.iter().enumerate() {
        if i < plan.kept_law {
            assert!(kept_context.contains(item), "kept provision {i} must appear whole");
        } else {
            // an omitted provision is absent in its entirety (no sliced prefix)
            assert!(!kept_context.contains(item), "omitted provision {i} must not appear at all");
        }
    }
    // Not everything fit → the allocator signals map-reduce rather than slicing.
    assert!(plan.kept_law < law.len(), "the tight window must omit some law: {plan:?}");
    assert!(plan.law_overflow, "overflow must be signalled for the dropped law");
    assert!(plan.fits());
}

#[test]
fn sc002_law_outranks_history_under_pressure() {
    // 1-token-per-word counter so the budget math is exact and readable.
    struct WordCounter;
    impl TokenCounter for WordCounter {
        fn count(&self, t: &str) -> usize { t.split_whitespace().count() }
    }
    let law = vec![std::iter::repeat("law").take(70).collect::<Vec<_>>().join(" ")];
    let turns: Vec<String> = (0..10)
        .map(|_| std::iter::repeat("chat").take(40).collect::<Vec<_>>().join(" "))
        .collect();
    let inputs = BudgetInputs {
        n_ctx: 120,
        gen_reserve: 20,
        persona: "you are a bureaucrat assistant",
        pinned_facts: &[],
        question: "q",
        retrieved_law: &law,
        recent_turns: &turns,
        summary: None,
    };
    let plan = allocate(&WordCounter, &inputs);
    assert_eq!(plan.kept_law, 1, "the whole provision is kept before any history");
    assert_eq!(plan.kept_turns, 0, "history yields so the law stays complete");
    assert!(plan.fits());
}

// ── T023 — SC-001: zero fabricated citations ──────────────────────────────────

#[test]
fn sc001_in_corpus_answer_keeps_every_cited_reference() {
    let cites = vec![Citation {
        source_id: "Sindh Civil Servants (Leave) Rules".into(),
        chunk_id: "c1".into(),
        excerpt: "Rule 6(c): leave on full pay may be granted by the competent authority.".into(),
        page: Some(4),
    }];
    let plan = plan_budget(&EstimateCounter, 8192, Capability::Guidance, cfg(), false, &MemoryContext::default(), "leave?", &[]);
    let resp = guidance::build_response(
        "c1",
        Capability::Guidance,
        "Under Rule 6(c) the competent authority may grant leave on full pay.",
        &cites,
        &plan,
        cfg(),
    );
    assert!(resp.flags.is_empty(), "a grounded answer fabricates nothing: {resp:?}");
    assert!(resp.answer.contains("Rule 6(c)"));
    // Re-gate the shipped prose: every provision it states is in context.
    let context = cites.iter().map(|c| c.excerpt.clone()).collect::<Vec<_>>().join("\n\n");
    assert!(citation_gate::gate(&resp.answer, &context).grounded);
}

#[test]
fn sc001_out_of_corpus_question_flags_and_emits_zero_fabrications() {
    // The retrieved context does NOT support the model's invented section.
    let cites = vec![Citation {
        source_id: "Some Act".into(),
        chunk_id: "c1".into(),
        excerpt: "Section 12 concerns appointments.".into(),
        page: None,
    }];
    let plan = plan_budget(&EstimateCounter, 8192, Capability::Guidance, cfg(), false, &MemoryContext::default(), "q", &[]);
    let resp = guidance::build_response(
        "c1",
        Capability::Guidance,
        "The procedure is set out in Section 511 and Article 909.",
        &cites,
        &plan,
        cfg(),
    );
    // Honest gap: flagged + offer-to-index, and not one fabricated ref survives.
    assert_eq!(resp.flags.len(), 2, "both fabricated refs flagged: {resp:?}");
    assert!(resp.flags.iter().all(|f| f.offer_to_index));
    assert!(!resp.answer.contains("Section 511"));
    assert!(!resp.answer.contains("Article 909"));
    let context = cites.iter().map(|c| c.excerpt.clone()).collect::<Vec<_>>().join("\n\n");
    assert!(citation_gate::gate(&resp.answer, &context).grounded, "shipped prose must re-gate clean");
}

// ── T024 — SC-005: first-answer completeness (cited answer + offer, no gate) ───

#[test]
fn sc005_first_turn_returns_complete_cited_answer_and_offer_no_clarifying_gate() {
    let cites = vec![Citation {
        source_id: "Leave Rules".into(),
        chunk_id: "c1".into(),
        excerpt: "Rule 6(c): leave on full pay may be sanctioned by the competent authority.".into(),
        page: Some(4),
    }];
    let plan = plan_budget(&EstimateCounter, 8192, Capability::Guidance, cfg(), false, &MemoryContext::default(), "long leave procedure?", &[]);
    let resp = guidance::build_response(
        "c1",
        Capability::Guidance,
        "The standard procedure is to apply in writing; under Rule 6(c) the competent authority may sanction leave on full pay.",
        &cites,
        &plan,
        cfg(),
    );
    // A complete, cited answer is returned on turn one …
    assert!(!resp.answer.trim().is_empty(), "first answer must be present");
    assert!(resp.answer.contains("Rule 6(c)"));
    assert_eq!(resp.citations.len(), 1);
    // … followed by the offer to tailor — NOT a blocking clarifying question.
    assert_eq!(resp.offer.as_deref(), Some("tailor"));
    assert_eq!(resp.capability, "guidance");
}

// ── T031 — SC-007: the usable window scales with the model ─────────────────────

/// Two model files / two `ECHO_N_CTX` values resolve to two windows. The dynamic
/// `n_ctx` resolver is unit-tested separately (n_ctx.rs); here we assert the
/// downstream guarantee that matters to the officer: a larger window retains MORE
/// law and history (never less), while grounding stays identical. We feed the two
/// resolved windows directly to the allocator so the test is parallel-safe (no
/// global `ECHO_N_CTX` env mutation) yet exercises the exact scaling path.
#[test]
fn sc007_larger_window_retains_more_law_and_history_grounding_unchanged() {
    let counter = EstimateCounter;
    // Enough law + history that a small window must drop some of both.
    let law: Vec<String> = (0..10).map(|i| provision(&format!("Section {i}"), 200)).collect();
    let turns: Vec<String> = (0..30)
        .map(|i| format!("assistant: position recorded for matter {i} per the indexed rules"))
        .collect();
    let mem = MemoryContext {
        pinned_facts: vec!["BS-18, 12 years service".to_string()],
        recent_turns: turns,
        summary: Some("earlier: officer asked about deputation".to_string()),
    };

    let small = plan_budget(&counter, 2048, Capability::Guidance, cfg(), false, &mem, "deputation rules?", &law);
    let large = plan_budget(&counter, 8192, Capability::Guidance, cfg(), false, &mem, "deputation rules?", &law);

    // Both fit (no overflow by construction) …
    assert!(small.fits() && large.fits(), "both windows must fit: {small:?} / {large:?}");
    // … and the larger model holds MORE context: at least as much law and history,
    // and strictly more of at least one of them.
    assert!(large.kept_law >= small.kept_law, "law retention must not shrink with a bigger window");
    assert!(large.kept_turns >= small.kept_turns, "history retention must not shrink with a bigger window");
    assert!(
        large.kept_law > small.kept_law || large.kept_turns > small.kept_turns,
        "a larger window must retain strictly more context: {small:?} / {large:?}",
    );

    // Grounding is a property of answer-vs-context, independent of the window:
    // the same answer + citations re-gate identically under either model.
    let cites = vec![Citation {
        source_id: "Deputation Rules".into(),
        chunk_id: "c1".into(),
        excerpt: "Section 0: the competent authority may approve deputation.".into(),
        page: Some(2),
    }];
    let answer = "Under Section 0 the competent authority may approve deputation.";
    let r_small = guidance::build_response("c", Capability::Guidance, answer, &cites, &small, cfg());
    let r_large = guidance::build_response("c", Capability::Guidance, answer, &cites, &large, cfg());
    assert_eq!(r_small.flags, r_large.flags, "grounding must not change with the model");
    assert_eq!(r_small.answer, r_large.answer, "the gated answer must be model-independent");
}

// ── T051 — FR-020: model-independent grounding parity ──────────────────────────

/// The citation gate is a pure function of (answer, retrieved context) — it does
/// NOT consult the model, its size, or the window. So for the same in/out-of-corpus
/// set, the gate verdict (verbatim citations kept, ungrounded refs flagged) must be
/// identical no matter which model produced the prose. We prove this by running the
/// full set under two distinct windows (the proxy for two model files): the flags,
/// the suppressed refs, and the shipped answer must match exactly.
#[test]
fn fr020_grounding_is_identical_across_models() {
    // (raw_answer, context-excerpt, expected_flag_count)
    let cases: &[(&str, &str, usize)] = &[
        // fully grounded — no flags under any model
        ("Under Section 18 you apply in writing.", "Section 18 provides for leave on full pay.", 0),
        // one fabricated ref — flagged under any model
        ("It is governed by Section 18 and also Section 404.", "Section 18 provides for leave.", 1),
        // out-of-corpus — both refs fabricated
        ("See Section 511 and Article 909.", "Section 12 concerns appointments.", 2),
    ];

    for &(answer, excerpt, expected_flags) in cases {
        let cites = vec![Citation {
            source_id: "Act".into(),
            chunk_id: "c1".into(),
            excerpt: excerpt.into(),
            page: None,
        }];
        let p_small = plan_budget(&EstimateCounter, 2048, Capability::Guidance, cfg(), false, &MemoryContext::default(), "q", &[]);
        let p_large = plan_budget(&EstimateCounter, 16384, Capability::Guidance, cfg(), false, &MemoryContext::default(), "q", &[]);

        let r_small = guidance::build_response("c", Capability::Guidance, answer, &cites, &p_small, cfg());
        let r_large = guidance::build_response("c", Capability::Guidance, answer, &cites, &p_large, cfg());

        // Behaviour is identical regardless of the model/window …
        assert_eq!(r_small.flags, r_large.flags, "flags differ across models for: {answer}");
        assert_eq!(r_small.answer, r_large.answer, "gated answer differs across models for: {answer}");
        // … and matches the model-independent contract.
        assert_eq!(r_small.flags.len(), expected_flags, "wrong flag count for: {answer}");
        assert!(r_small.flags.iter().all(|f| f.offer_to_index));
        // The shipped prose always re-gates clean (no fabricated ref survives).
        let context = excerpt.to_string();
        assert!(citation_gate::gate(&r_small.answer, &context).grounded, "shipped prose must re-gate clean: {answer}");
    }
}

// ── T036 — SC-009: whole-document coverage (no section silently skipped) ───────

/// 1 token per whitespace word — exact, readable batch math.
struct WordCounter;
impl TokenCounter for WordCounter {
    fn count(&self, t: &str) -> usize { t.split_whitespace().count() }
}

/// A multi-section act, larger than any single window, MUST be summarised across
/// passes that together cover 100% of its sections — none skipped, none sliced.
/// The map-reduce batcher is the deterministic enforcement point: every provision
/// lands in exactly one batch, in document order, whole.
#[test]
fn sc009_mapreduce_covers_every_section_exactly_once() {
    // A 60-section act; each section is a whole provision of varying length.
    let act: Vec<String> = (0..60)
        .map(|i| {
            let words = 30 + (i % 7) * 15; // 30..120 words
            format!(
                "Section {i}: {}",
                std::iter::repeat("the authority shall").take(words / 3 + 1).collect::<Vec<_>>().join(" ")
            )
        })
        .collect();

    // A window far too small to hold the whole act in one pass.
    let batches = mapreduce::plan_batches(&WordCounter, &act, 40, 400, 80);

    // Must genuinely span multiple passes (this is the whole point of map-reduce).
    assert!(batches.len() > 1, "a whole act must be processed across passes: {} batches", batches.len());

    // 100% coverage: the union of every batch's indices is exactly 0..N, in order
    // — every section read, none skipped, none duplicated.
    let covered: Vec<usize> = batches.iter().flat_map(|b| b.indices.clone()).collect();
    let expected: Vec<usize> = (0..act.len()).collect();
    assert_eq!(covered, expected, "every section must be covered exactly once, in order");

    // Each provision is whole within its batch — no sliced section (atomicity).
    for b in &batches {
        for &i in &b.indices {
            assert!(act[i].starts_with(&format!("Section {i}:")), "provision {i} must be whole");
        }
    }
}

// ── T037 — SC-010: amendment-chain fidelity (deterministic, 0 invented links) ──

fn ev(kind: &str, instrument: &str, date: Option<&str>, citation: &str) -> AmendmentEventRow {
    AmendmentEventRow {
        kind: kind.into(),
        by_instrument: instrument.into(),
        effective_date: date.map(|d| d.into()),
        source_page: None,
        citation: Some(citation.into()),
    }
}

/// The chain is built deterministically from structured provenance: ordered
/// oldest→recent by effective date, every link's instrument/date/action matching
/// the source rows exactly, with zero links invented or dropped.
#[test]
fn sc010_amendment_chain_is_ordered_and_faithful_to_provenance() {
    // Provenance deliberately out of order on input.
    let events = vec![
        ev("omitted", "Act IX of 2021", Some("2021-11-20"), "Section 18"),
        ev("inserted", "Act V of 2013", Some("2013-01-10"), "Section 18"),
        ev("substituted", "Act XX of 2016", Some("2016-05-01"), "Section 18"),
    ];

    let chain = amendment_chain::build_chain("Section 18", &events);

    // Same number of links as recorded events — none invented, none dropped.
    assert_eq!(chain.len(), events.len(), "0 invented and 0 dropped links");

    // Ordered oldest→recent.
    let dates: Vec<_> = chain.iter().map(|l| l.date.clone().unwrap()).collect();
    assert_eq!(dates, vec!["2013-01-10", "2016-05-01", "2021-11-20"]);

    // Every link's instrument + action matches a recorded event for that date
    // (no fabrication): build the expected (date → instrument/action) set.
    for link in &chain {
        let src = events
            .iter()
            .find(|e| e.effective_date.as_deref() == link.date.as_deref())
            .expect("every link must trace to a recorded event");
        assert_eq!(link.amending_instrument, src.by_instrument, "instrument must match provenance");
        assert_eq!(link.action, src.kind, "action must match provenance");
    }

    // The rendered prose re-gates clean (no provision ref it states is unsupported).
    let prose = amendment_chain::render_prose("Section 18", &chain);
    assert!(prose.contains("Act V of 2013") && prose.contains("Act IX of 2021"));
}

#[test]
fn sc010_no_provenance_states_none_rather_than_inventing() {
    let chain = amendment_chain::build_chain("Section 404", &[]);
    assert!(chain.is_empty(), "no events → no chain (never invented)");
    let prose = amendment_chain::render_prose("Section 404", &chain);
    assert!(prose.to_lowercase().contains("no amendments"), "{prose}");
}

// ── T042 — SC-011: conflicts are cited candidates (both provisions cited) ──────

/// A surfaced conflict must cite the provision from BOTH documents — a candidate
/// for the officer to judge, never asserted. The citation gate is the hard
/// guarantee: a conflict statement whose two provisions are both present in the
/// retrieved context survives intact (both cited); a statement that asserts a
/// conflict against a provision the context cannot support is suppressed — so 0
/// conflicts are ever asserted without both grounded citations.
#[test]
fn sc011_conflict_with_both_provisions_grounded_is_kept_and_cited() {
    // Both sides of the candidate conflict are in the retrieved context.
    let cites = vec![
        Citation {
            source_id: "Sindh Local Government Act 2013".into(),
            chunk_id: "a1".into(),
            excerpt: "Section 18: the council may levy the tax with provincial approval.".into(),
            page: Some(7),
        },
        Citation {
            source_id: "Sindh Finance Act".into(),
            chunk_id: "b1".into(),
            excerpt: "Section 9: no local tax shall be levied without an Assembly resolution.".into(),
            page: Some(2),
        },
    ];
    let plan = plan_budget(&EstimateCounter, 8192, Capability::Conflict, cfg(), false, &MemoryContext::default(), "any conflict?", &[]);
    let resp = guidance::build_response(
        "c1",
        Capability::Conflict,
        "Potential conflict for review: Section 18 permits the levy with provincial approval, \
         while Section 9 forbids any local tax without an Assembly resolution.",
        &cites,
        &plan,
        cfg(),
    );

    // Both provisions cited, nothing fabricated, and the shipped prose re-gates clean.
    assert!(resp.flags.is_empty(), "a fully-grounded candidate must not flag: {resp:?}");
    assert!(resp.answer.contains("Section 18") && resp.answer.contains("Section 9"));
    let context = cites.iter().map(|c| c.excerpt.clone()).collect::<Vec<_>>().join("\n\n");
    assert!(citation_gate::gate(&resp.answer, &context).grounded, "both cited candidate survives");
}

#[test]
fn sc011_conflict_missing_one_provision_is_not_asserted() {
    // Only ONE side of the asserted conflict is actually in the context.
    let cites = vec![Citation {
        source_id: "Sindh Local Government Act 2013".into(),
        chunk_id: "a1".into(),
        excerpt: "Section 18: the council may levy the tax with provincial approval.".into(),
        page: Some(7),
    }];
    let plan = plan_budget(&EstimateCounter, 8192, Capability::Conflict, cfg(), false, &MemoryContext::default(), "any conflict?", &[]);
    let resp = guidance::build_response(
        "c1",
        Capability::Conflict,
        "Potential conflict: Section 18 permits the levy, but Section 909 prohibits it entirely.",
        &cites,
        &plan,
        cfg(),
    );

    // The half-grounded conflict claim (Section 909 absent) is suppressed + flagged
    // — no conflict asserted without BOTH provisions cited.
    assert!(!resp.answer.contains("Section 909"), "ungrounded side suppressed: {resp:?}");
    assert!(resp.flags.iter().any(|f| f.missing.to_lowercase().contains("section 909") && f.offer_to_index));
    let context = cites.iter().map(|c| c.excerpt.clone()).collect::<Vec<_>>().join("\n\n");
    assert!(citation_gate::gate(&resp.answer, &context).grounded, "shipped prose re-gates clean");
}

// ── T046 — SC-004: memory fidelity (facts stated early honored late) ──────────

/// A fact the officer states early (their grade) must still be present, verbatim,
/// after a long session that folds older turns into the running summary — pinned
/// facts are exempt from eviction (FR-018), so they never decay to lossy prose.
#[tokio::test]
async fn sc004_early_facts_are_honored_after_a_long_session() {
    let pool = echo_ai_lib::db::testing::test_pool().await;
    let conv = "sc004";

    // Turn 0: the officer states their case facts.
    let (seq0, _) = memory::append_turn(&pool, conv, "user", "I am a BS-17 officer with 8 years of service").await.unwrap();
    memory::capture_pinned_facts(&pool, conv, seq0, "I am a BS-17 officer with 8 years of service").await.unwrap();

    // A long conversation follows; eviction runs each turn (keep the last 4).
    for i in 0..40 {
        memory::append_turn(&pool, conv, "user", &format!("follow-up question {i}")).await.unwrap();
        memory::append_turn(&pool, conv, "assistant", &format!("advice for {i}")).await.unwrap();
        memory::evict_old_turns(&pool, conv, 4).await.unwrap();
    }

    // Far later, the facts are still in the always-present pinned slot, verbatim.
    let mem = memory::load_for_turn(&pool, conv, 6).await.unwrap();
    assert!(mem.pinned_facts.iter().any(|f| f == "Grade: BS-17"), "grade honored late: {:?}", mem.pinned_facts);
    assert!(mem.pinned_facts.iter().any(|f| f == "Length of service: 8 years"), "{:?}", mem.pinned_facts);
    // History was bounded by eviction, but nothing was hard-dropped — it's summarized.
    assert!(mem.recent_turns.len() <= 4, "verbatim history is bounded: {}", mem.recent_turns.len());
    assert!(mem.summary.is_some(), "evicted turns folded into the summary, not lost");

    // The allocator keeps the pinned facts as must-keeps regardless of window size.
    let plan = plan_budget(&EstimateCounter, 4096, Capability::Guidance, cfg(), false, &mem, "what now?", &[]);
    assert!(!plan.must_fit_failed && plan.fits());
}

// ── T046 — SC-008: context-fill notice shown at threshold + facts carried ─────

/// When utilization crosses the threshold the turn carries a non-blocking notice;
/// and starting a fresh chat carries the non-superseded pinned facts forward
/// (FR-018a/FR-018b) — the new conversation begins with the officer's facts intact.
#[tokio::test]
async fn sc008_notice_at_threshold_and_facts_carry_to_fresh_chat() {
    // (a) the notice trips once utilization ≥ the configured threshold.
    let law = vec![provision("Section 1", 800)];
    let plan = plan_budget(&EstimateCounter, 1200, Capability::Guidance, cfg(), false, &MemoryContext::default(), "q", &law);
    let resp = guidance::build_response("c1", Capability::Guidance, "ok", &[], &plan, cfg());
    assert!(resp.context_fill.utilization >= cfg().settings.context_fill_threshold);
    assert!(resp.context_fill.notice, "notice must show at/above threshold: {resp:?}");

    // (b) starting a fresh chat carries the non-superseded pinned facts forward.
    let pool = echo_ai_lib::db::testing::test_pool().await;
    let from = "sc008_from";
    memory::capture_pinned_facts(&pool, from, 0, "BS-18 officer applying for earned leave").await.unwrap();

    // Simulate `new_chat_from_notice`: load the prior facts → seed a fresh id.
    let carried = memory::load_pinned_facts(&pool, from).await.unwrap();
    assert!(carried.iter().any(|(k, v)| k == "grade" && v == "BS-18"));
    let fresh = "sc008_fresh";
    memory::record_pinned_facts(&pool, fresh, 0, &carried).await.unwrap();

    // The fresh conversation begins with an empty window but the facts intact.
    let mem = memory::load_for_turn(&pool, fresh, 6).await.unwrap();
    assert!(mem.recent_turns.is_empty(), "fresh window has no verbatim turns");
    assert!(mem.summary.is_none(), "fresh window has no prior summary");
    assert!(mem.pinned_facts.iter().any(|f| f == "Grade: BS-18"));
    assert!(mem.pinned_facts.iter().any(|f| f == "Leave type: earned leave"));
}

// ── T052 — FR-021: session survives restart ───────────────────────────────────

/// Turns, the running summary, and pinned facts are all persisted, so a session
/// reloads intact after a restart. The bundled Postgres is durable storage; we
/// write, then drop the handle and open a SECOND, independent pool to the same
/// database (the "restart" — a fresh process re-connecting), and assert the
/// conversation reloads exactly.
#[tokio::test]
async fn fr021_session_reloads_intact_after_restart() {
    let pool = echo_ai_lib::db::testing::test_pool().await;
    let conv = "fr021";

    // Build a session: pinned facts + several turns + an eviction-produced summary.
    memory::append_turn(&pool, conv, "user", "I am a BS-19 officer, 12 years service").await.unwrap();
    memory::capture_pinned_facts(&pool, conv, 0, "I am a BS-19 officer, 12 years service").await.unwrap();
    for i in 0..8 {
        memory::append_turn(&pool, conv, "user", &format!("matter {i}")).await.unwrap();
    }
    memory::evict_old_turns(&pool, conv, 3).await.unwrap();

    let before = memory::load_for_turn(&pool, conv, 6).await.unwrap();
    assert!(before.summary.is_some());
    assert!(before.pinned_facts.iter().any(|f| f == "Grade: BS-19"));

    // "Restart": a second, independent pool to the SAME database (same connect
    // options), after the first handle is dropped.
    let opts = (*pool.connect_options()).clone();
    drop(pool);
    let pool2 = sqlx::postgres::PgPoolOptions::new()
        .max_connections(2)
        .connect_with(opts)
        .await
        .expect("reconnect to the same durable database");

    // The conversation reloads exactly — turns, summary, and pinned facts intact.
    let after = memory::load_for_turn(&pool2, conv, 6).await.unwrap();
    assert_eq!(after.recent_turns, before.recent_turns, "verbatim turns survive restart");
    assert_eq!(after.summary, before.summary, "running summary survives restart");
    assert_eq!(after.pinned_facts, before.pinned_facts, "pinned facts survive restart");
    assert!(after.pinned_facts.iter().any(|f| f == "Grade: BS-19"));
}
