//! T006/T007 — the real-token **priority context allocator** (FR-010..FR-018).
//!
//! The hardest F015 invariant: the assembled prompt MUST fit the model window
//! BEFORE generation, so the `llm.rs` head+tail truncation path never fires
//! (FR-011). Provisions are **atomic** — a law is kept whole or omitted, never
//! sliced (FR-014). **Law outranks history** — law fills before conversation
//! turns (FR-015). When not all relevant law fits, the allocator signals
//! `law_overflow` so the caller runs map-reduce (FR-016) rather than truncating.
//!
//! Pure logic + a pluggable [`TokenCounter`] so the invariant is unit-tested
//! without a live model. The real counter wraps the loaded model's tokenizer
//! (`model.str_to_token`); see `n_ctx.rs` (T005/T006 wiring).
//!
//! Priority order (FR-012): generation reserve (carved first) → persona →
//! pinned case facts → current question → retrieved law → recent turns →
//! running summary.

/// Counts tokens for a piece of text. The production implementation wraps the
/// loaded model's tokenizer; tests use a deterministic stand-in.
pub trait TokenCounter {
    fn count(&self, text: &str) -> usize;
}

/// Tokenizer-free estimate (~4 chars/token) for budget accounting where the
/// live model tokenizer is not in hand (e.g. the command layer / trace). The
/// real [`crate::services::assistant::n_ctx::ModelTokenCounter`] is used inside
/// the generation path; this conservative estimate keeps the plan honest there.
pub struct EstimateCounter;
impl TokenCounter for EstimateCounter {
    fn count(&self, text: &str) -> usize {
        // round up; a non-empty string is never zero tokens.
        if text.is_empty() { 0 } else { (text.chars().count() + 3) / 4 }
    }
}

/// Components offered to the allocator, already in priority order. `retrieved_law`
/// and `recent_turns` are lists of **atomic** items (a whole provision / a whole
/// turn) — the allocator includes each item whole or not at all.
pub struct BudgetInputs<'a> {
    /// Model context window (from the dynamic `n_ctx` resolver, T005).
    pub n_ctx: usize,
    /// Tokens reserved for the answer (carved out first; larger for drafting).
    pub gen_reserve: usize,
    pub persona: &'a str,
    pub pinned_facts: &'a [String],
    pub question: &'a str,
    /// Atomic provisions, highest-priority fill after the must-keeps.
    pub retrieved_law: &'a [String],
    /// Recent conversation turns, most-recent-first.
    pub recent_turns: &'a [String],
    /// Running summary of evicted older turns (one compact block).
    pub summary: Option<&'a str>,
}

/// The allocator's decision. Token counts are real (via [`TokenCounter`]).
/// `Default` = an all-zero plan, used by paths that never assemble a legal
/// prompt (the web-only answer mode) — honest zeros rather than invented
/// numbers for an allocation that never happened.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct BudgetPlan {
    /// Number of leading `retrieved_law` items included WHOLE.
    pub kept_law: usize,
    /// True when some relevant law did not fit ⇒ caller must map-reduce (FR-016).
    pub law_overflow: bool,
    /// Number of leading `recent_turns` included whole.
    pub kept_turns: usize,
    /// Whether the running summary was included.
    pub summary_included: bool,
    /// Total tokens of the kept prompt components (EXCLUDING `gen_reserve`).
    pub prompt_tokens: usize,
    pub n_ctx: usize,
    pub gen_reserve: usize,
    /// True when the must-keeps (persona + pinned + question) alone exceed the
    /// input budget — a pathologically small window. The caller must recover
    /// (compact persona, or surface "window too small"); the `prompt ≤ window`
    /// guarantee can only hold once the must-keeps fit.
    pub must_fit_failed: bool,
}

impl BudgetPlan {
    /// The core invariant (FR-011): the assembled prompt plus the generation
    /// reserve never exceeds the window. Holds whenever `must_fit_failed` is
    /// false. The allocator only ever ADDS an optional component if it still
    /// fits, so this is true by construction in the normal path.
    pub fn fits(&self) -> bool {
        self.prompt_tokens + self.gen_reserve <= self.n_ctx
    }
}

/// Allocate the window by priority, in real tokens, guaranteeing `prompt ≤
/// window` (unless the must-keeps cannot fit — then `must_fit_failed`).
pub fn allocate<C: TokenCounter>(c: &C, inputs: &BudgetInputs<'_>) -> BudgetPlan {
    let budget = inputs.n_ctx.saturating_sub(inputs.gen_reserve);

    // Must-keeps (priorities 2–4): persona + all pinned facts + question.
    let must: usize = c.count(inputs.persona)
        + inputs.pinned_facts.iter().map(|f| c.count(f)).sum::<usize>()
        + c.count(inputs.question);

    if must > budget {
        // Pathological window. Keep nothing optional; report the failure so the
        // caller can recover (compact persona / "window too small").
        return BudgetPlan {
            kept_law: 0,
            law_overflow: !inputs.retrieved_law.is_empty(),
            kept_turns: 0,
            summary_included: false,
            prompt_tokens: must,
            n_ctx: inputs.n_ctx,
            gen_reserve: inputs.gen_reserve,
            must_fit_failed: true,
        };
    }

    let mut used = must;

    // Priority 5 — retrieved law, ATOMIC (whole or omitted). Law fills before
    // any conversation turn (law outranks history). A single oversized law item
    // does not block later items, but it does flag overflow ⇒ map-reduce.
    let mut kept_law = 0usize;
    let mut law_overflow = false;
    for item in inputs.retrieved_law {
        let t = c.count(item);
        if used + t <= budget {
            used += t;
            kept_law += 1;
        } else {
            law_overflow = true; // this (and any remaining) law did not fit
        }
    }

    // Priority 6 — recent turns (most-recent-first), atomic.
    let mut kept_turns = 0usize;
    for turn in inputs.recent_turns {
        let t = c.count(turn);
        if used + t <= budget {
            used += t;
            kept_turns += 1;
        } else {
            break; // older turns won't fit either; eviction→summary is memory.rs's job
        }
    }

    // Priority 7 — running summary (compact), include if it still fits.
    let mut summary_included = false;
    if let Some(s) = inputs.summary {
        let t = c.count(s);
        if used + t <= budget {
            used += t;
            summary_included = true;
        }
    }

    BudgetPlan {
        kept_law,
        law_overflow,
        kept_turns,
        summary_included,
        prompt_tokens: used,
        n_ctx: inputs.n_ctx,
        gen_reserve: inputs.gen_reserve,
        must_fit_failed: false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Deterministic stand-in tokenizer: 1 token per whitespace-separated word.
    struct WordCounter;
    impl TokenCounter for WordCounter {
        fn count(&self, text: &str) -> usize {
            text.split_whitespace().count()
        }
    }

    fn s(n: usize, w: &str) -> String {
        std::iter::repeat(w).take(n).collect::<Vec<_>>().join(" ")
    }

    #[test]
    fn everything_fits_keeps_all_and_respects_window() {
        let law = vec![s(10, "law"), s(10, "law")];
        let turns = vec![s(5, "t"), s(5, "t")];
        let summary = s(8, "sum");
        let inputs = BudgetInputs {
            n_ctx: 200,
            gen_reserve: 50,
            persona: &s(20, "p"),
            pinned_facts: &[s(4, "f")],
            question: &s(6, "q"),
            retrieved_law: &law,
            recent_turns: &turns,
            summary: Some(&summary),
        };
        let plan = allocate(&WordCounter, &inputs);
        assert_eq!(plan.kept_law, 2);
        assert!(!plan.law_overflow);
        assert_eq!(plan.kept_turns, 2);
        assert!(plan.summary_included);
        assert!(plan.fits(), "prompt+reserve must be ≤ n_ctx: {plan:?}");
        assert!(!plan.must_fit_failed);
    }

    #[test]
    fn law_is_atomic_and_overflow_triggers_mapreduce_signal() {
        // budget = 100-20 = 80; must = 10(persona). Three 40-token laws: first
        // fits (50), second would be 90 > 80 → omitted whole, overflow flagged.
        let law = vec![s(40, "a"), s(40, "b"), s(40, "c")];
        let inputs = BudgetInputs {
            n_ctx: 100,
            gen_reserve: 20,
            persona: &s(10, "p"),
            pinned_facts: &[],
            question: &s(0, "q"), // empty → 0 tokens
            retrieved_law: &law,
            recent_turns: &[],
            summary: None,
        };
        let plan = allocate(&WordCounter, &inputs);
        assert_eq!(plan.kept_law, 1, "only the first whole provision fits");
        assert!(plan.law_overflow, "remaining law must signal map-reduce");
        assert!(plan.fits(), "never exceed the window: {plan:?}");
        // prompt = 10 persona + 40 law = 50 ≤ 80
        assert_eq!(plan.prompt_tokens, 50);
    }

    #[test]
    fn law_outranks_history_under_pressure() {
        // budget = 60-10 = 50; persona 10 → 40 left. One 30-law + many turns.
        // Law takes priority; only what's left goes to turns.
        let law = vec![s(30, "law")];
        let turns = vec![s(20, "t"), s(20, "t")]; // 20 fits (10+30+20=60>50? no: used=40, +20=60>50) -> 0 turns
        let inputs = BudgetInputs {
            n_ctx: 60,
            gen_reserve: 10,
            persona: &s(10, "p"),
            pinned_facts: &[],
            question: "",
            retrieved_law: &law,
            recent_turns: &turns,
            summary: None,
        };
        let plan = allocate(&WordCounter, &inputs);
        assert_eq!(plan.kept_law, 1, "law kept whole before any turn");
        assert_eq!(plan.kept_turns, 0, "history yields to law");
        assert!(plan.fits());
    }

    #[test]
    fn pinned_and_question_always_kept_when_must_fits() {
        let inputs = BudgetInputs {
            n_ctx: 100,
            gen_reserve: 10,
            persona: &s(5, "p"),
            pinned_facts: &[s(3, "grade"), s(3, "service")],
            question: &s(4, "q"),
            retrieved_law: &[s(200, "huge")], // cannot fit
            recent_turns: &[],
            summary: None,
        };
        let plan = allocate(&WordCounter, &inputs);
        assert!(!plan.must_fit_failed);
        assert_eq!(plan.kept_law, 0);
        assert!(plan.law_overflow);
        // must-keeps counted: 5 + 3 + 3 + 4 = 15
        assert_eq!(plan.prompt_tokens, 15);
        assert!(plan.fits());
    }

    #[test]
    fn must_keeps_exceeding_budget_reports_failure_not_silent_overflow() {
        let inputs = BudgetInputs {
            n_ctx: 30,
            gen_reserve: 10, // budget 20
            persona: &s(25, "p"), // already over budget
            pinned_facts: &[],
            question: "",
            retrieved_law: &[],
            recent_turns: &[],
            summary: None,
        };
        let plan = allocate(&WordCounter, &inputs);
        assert!(plan.must_fit_failed, "tiny window must be reported, not hidden");
    }

    #[test]
    fn summary_included_only_if_room_after_law_and_turns() {
        // budget 50-5=45; persona 5, law 30 → used 35; turn 8 → 43; summary 5 → 48>45 dropped
        let law = vec![s(30, "law")];
        let turns = vec![s(8, "t")];
        let summary = s(5, "sum");
        let inputs = BudgetInputs {
            n_ctx: 50,
            gen_reserve: 5,
            persona: &s(5, "p"),
            pinned_facts: &[],
            question: "",
            retrieved_law: &law,
            recent_turns: &turns,
            summary: Some(&summary),
        };
        let plan = allocate(&WordCounter, &inputs);
        assert_eq!(plan.kept_law, 1);
        assert_eq!(plan.kept_turns, 1);
        assert!(!plan.summary_included, "summary dropped when it would overflow");
        assert!(plan.fits());
    }
}
