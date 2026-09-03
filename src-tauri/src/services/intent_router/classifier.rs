//! Rule-first intent classifier (T024).
//!
//! Curated regex cues per intent. The query is lowercased once, every
//! cue scored, the highest-scoring intent wins. Confidence below 0.6
//! means the dispatcher SHOULD invoke the small-LLM tiebreak (T025) or
//! fall through to `Intent::Fallback`.

use super::types::Intent;
use regex_lite::Regex;
use std::sync::OnceLock;

const TIEBREAK_MARGIN: f32 = 0.15;

struct Cue {
    intent: Intent,
    pattern: &'static str,
    weight: f32,
}

const RAW_CUES: &[Cue] = &[
    // ─── Lookup ──────────────────────────────────────────────────────────
    Cue { intent: Intent::Lookup, weight: 1.0, pattern: r"\b(what (does|is)|define|definition of|meaning of|quote|exact text|verbatim|wording of)\b" },
    Cue { intent: Intent::Lookup, weight: 0.8, pattern: r"\b(article|articles|section|sections|clause|clauses|sub-?section|sub-?clause|rule|rules|regulation|regulations) \d+[a-z]?\b" },
    Cue { intent: Intent::Lookup, weight: 0.9, pattern: r"\b(show me|read out|give me) (article|section|clause|rule)\b" },
    Cue { intent: Intent::Lookup, weight: 0.9, pattern: r"\b(how is|how does) .+? (defined|define)\b" },
    Cue { intent: Intent::Lookup, weight: 0.9, pattern: r"\bunder what( \w+){0,3} (provisions?|articles?|sections?|rules?|clauses?)\b" },
    Cue { intent: Intent::Lookup, weight: 0.9, pattern: r"\bwhat (rights?|protections?|powers?|duties|obligations?) (of|do)\b" },

    // ─── ListChildren ────────────────────────────────────────────────────
    Cue { intent: Intent::ListChildren, weight: 1.0, pattern: r"\b(list|enumerate|index of|outline of|table of contents)\b" },
    Cue { intent: Intent::ListChildren, weight: 0.9, pattern: r"\b(all|every) (articles?|sections?|clauses?|rules?|chapters?|parts?)\b" },
    Cue { intent: Intent::ListChildren, weight: 0.9, pattern: r"\bwhat (articles?|sections?|clauses?|rules?) (are|fall)\b" },
    Cue { intent: Intent::ListChildren, weight: 0.9, pattern: r"\b(listed in|mentioned in)\b" },
    Cue { intent: Intent::ListChildren, weight: 0.9, pattern: r"\bwhat are the .+ (listed|mentioned)\b" },

    // ─── Summarize ───────────────────────────────────────────────────────
    Cue { intent: Intent::Summarize, weight: 1.0, pattern: r"\b(summarize|summarise|summary of|tldr|tl;dr|brief overview|in brief|key points of|gist of|overview of)\b" },
    Cue { intent: Intent::Summarize, weight: 0.7, pattern: r"\b(explain in simple|in plain (english|urdu|language))\b" },
    Cue { intent: Intent::Summarize, weight: 1.2, pattern: r"\bwalk me through\b" },

    // ─── Difference ──────────────────────────────────────────────────────
    Cue { intent: Intent::Difference, weight: 0.9, pattern: r"\b(difference|diff|changed|change log|what changed|before (and|vs\.?|versus) after|amend(ed|ment)?|substituted|inserted|repealed|omitted)\b" },
    Cue { intent: Intent::Difference, weight: 0.8, pattern: r"\b(old|original) (version|wording|text)\b" },

    // ─── CrossCompare ────────────────────────────────────────────────────
    Cue { intent: Intent::CrossCompare, weight: 0.9, pattern: r"\b(compare|comparison|vs\.?|versus|against|between)\b.*\b(act|ordinance|rules?|regulation|constitution|policy)\b" },
    Cue { intent: Intent::CrossCompare, weight: 1.0, pattern: r"\bhow does .+ (compare|differ) (to|from|with)\b" },
    Cue { intent: Intent::CrossCompare, weight: 0.9, pattern: r"\bcompare the .+ with the\b" },
    Cue { intent: Intent::CrossCompare, weight: 0.7, pattern: r"\bwhere do they overlap\b" },

    // ─── ActionItems ─────────────────────────────────────────────────────
    // NOTE: bare `must`/`shall do` were removed (2026-06-29). They matched any
    // ordinary factual question containing "must" (e.g. "must it go higher",
    // "must be advertised") → misrouted to the whole-corpus ActionItems
    // map-reduce → one LLM call per batch over the entire corpus → the request
    // hangs past the client timeout on BOTH engines. The 5 timeouts in the
    // F016/F017 eval set (Q01/Q04/Q16/Q19/Q30) were ALL bare-"must" false
    // positives. The `what must I do` / `what must <det> <role>` phrasal forms
    // below still capture genuine action-extraction requests. The role frame
    // requires a determiner (the/a/each/…) + role noun, so bare "must be
    // advertised" / "must it go higher" still do NOT match; a 0–3 word slot
    // between determiner and role admits real titles ("the Information Officer",
    // "the Chief Secretary", "the Deputy Commissioner").
    Cue { intent: Intent::ActionItems, weight: 1.0, pattern: r"\b(action items?|action points?|to-?dos?|checklist|duties|obligations?|responsibilities|what (must|should|do) i do|what (must|should) (the|a|an|each|every) ([a-z]+ ){0,3}(officer|secretary|director|department|commissioner|authority|official|head|registrar|collector|minister|chairman|board|agency))\b" },
    Cue { intent: Intent::ActionItems, weight: 0.5, pattern: r"\b(steps (to|for))\b" },

    // ─── Timeline ────────────────────────────────────────────────────────
    // ⚠️ Bare "timeline" and "history" are NOT amendment cues, and used to be.
    // Timeline routes to `Capability::AmendmentChain` (assistant/intent.rs), which
    // is a deterministic SQL/graph lookup with NO retrieval and NO model. So a cue
    // hit here does not merely mis-rank an answer — it skips the corpus entirely.
    // Prod audit 2026-07-16: "what about the appeal timeline?" matched bare
    // `timeline`, returned 0 chain links in 7 ms, and never read a line of law. In
    // this domain "timeline" almost always means a DEADLINE ("appeal timeline",
    // "timeline for filing") and "history" a record ("service history", "history of
    // the department") — neither is an amendment chronology.
    // The word must therefore be ANCHORED to amendment/version language.
    Cue { intent: Intent::Timeline, weight: 1.0, pattern: r"\b(chronology|chronological|evolution|over time|when was|since \d{4})\b" },
    Cue { intent: Intent::Timeline, weight: 1.0, pattern: r"\b((amendment|legislative|version|revision)s?\s+(timeline|history)|(timeline|history)\s+of\s+(amendment|change|revision|version)s?)\b" },
    Cue { intent: Intent::Timeline, weight: 0.9, pattern: r"\b(amendment history|amend(ed|ment)?s? (by|in|since))\b" },

    // NOTE: counting ("how many rules/sections/acts") is NOT classified here.
    // "how many" is too overloaded to score as a cue; the deterministic Count
    // capability (ADR-0019) is gated upstream in `assistant::intent` via
    // `counting::is_structural_count_query`, before this classifier runs.

    // ─── Procedure ───────────────────────────────────────────────────────
    Cue { intent: Intent::Procedure, weight: 1.0, pattern: r"\b(procedure|process|workflow|how (do|does|to)|flow chart|step-?by-?step|stepwise|what is the process)\b" },
    Cue { intent: Intent::Procedure, weight: 0.7, pattern: r"\b(apply for|file a|register|submission|grievance|appeal|complaint)\b" },
    Cue { intent: Intent::Procedure, weight: 1.5, pattern: r"\bprocedure (for|of|to)\b" },
    Cue { intent: Intent::Procedure, weight: 1.2, pattern: r"\btrace the (path|flow|process|route)\b" },
];

fn compiled() -> &'static [(Intent, Regex, f32)] {
    static CELL: OnceLock<Vec<(Intent, Regex, f32)>> = OnceLock::new();
    CELL.get_or_init(|| {
        RAW_CUES
            .iter()
            .map(|c| {
                (
                    c.intent,
                    Regex::new(c.pattern).expect("static cue regex must compile"),
                    c.weight,
                )
            })
            .collect()
    })
}

pub fn classify(query: &str) -> (Intent, f32) {
    let lower = query.to_lowercase();
    let mut scores = [0.0_f32; 9];
    for (intent, re, weight) in compiled() {
        if re.is_match(&lower) {
            scores[*intent as usize] += *weight;
        }
    }

    let (best_idx, best_score) = scores
        .iter()
        .enumerate()
        .max_by(|a, b| a.1.partial_cmp(b.1).unwrap())
        .map(|(i, &s)| (i, s))
        .unwrap();

    if best_score < 0.5 {
        return (Intent::Fallback, 0.0);
    }

    let second = scores
        .iter()
        .enumerate()
        .filter(|(i, _)| *i != best_idx)
        .map(|(_, &s)| s)
        .fold(0.0_f32, f32::max);

    let margin = best_score - second;
    let confidence = if margin >= TIEBREAK_MARGIN {
        (0.6 + (best_score / (best_score + 1.0)) * 0.4).min(1.0)
    } else {
        0.55
    };

    (index_to_intent(best_idx), confidence)
}

fn index_to_intent(i: usize) -> Intent {
    match i {
        0 => Intent::Lookup,
        1 => Intent::ListChildren,
        2 => Intent::Summarize,
        3 => Intent::Difference,
        4 => Intent::CrossCompare,
        5 => Intent::ActionItems,
        6 => Intent::Timeline,
        7 => Intent::Procedure,
        _ => Intent::Fallback,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lookup_what_does_article() {
        let (i, c) = classify("What does Article 19 say about freedom of speech?");
        assert_eq!(i, Intent::Lookup);
        assert!(c >= 0.6, "confidence {c}");
    }

    #[test]
    fn listchildren_all_sections() {
        let (i, _) = classify("List all sections in Chapter III of the RTI Act");
        assert_eq!(i, Intent::ListChildren);
    }

    #[test]
    fn summarize_chapter() {
        let (i, _) = classify("Summarize Chapter II of the Constitution");
        assert_eq!(i, Intent::Summarize);
    }

    #[test]
    fn difference_what_changed() {
        let (i, _) = classify("What changed in Section 5 after the 2017 amendment?");
        assert_eq!(i, Intent::Difference);
    }

    #[test]
    fn crosscompare_two_acts() {
        let (i, _) = classify("How does the RTI Act compare to the Punjab Transparency Act?");
        assert_eq!(i, Intent::CrossCompare);
    }

    #[test]
    fn action_items_what_must_officer_do() {
        let (i, _) = classify("What must the Information Officer do under Section 7?");
        assert_eq!(i, Intent::ActionItems);
    }

    #[test]
    fn action_items_titled_roles_route_correctly() {
        // A title/qualifier between the determiner and the role must still route
        // to ActionItems (the 0–3 word slot in the phrasal cue).
        for q in [
            "What must the Chief Secretary do next?",
            "What should the Deputy Commissioner do in this case?",
            "What must each Public Information Officer do?",
        ] {
            assert_eq!(classify(q).0, Intent::ActionItems, "should be ActionItems: {q}");
        }
    }

    #[test]
    fn bare_must_is_not_action_items() {
        // Regression guard for f8828a02: bare "must" (no determiner+role frame)
        // must NOT route to the whole-corpus ActionItems map-reduce.
        for q in [
            "Must the tender be advertised?",
            "Must it go higher for approval?",
            "Does the summary must be signed?",
        ] {
            assert_ne!(classify(q).0, Intent::ActionItems, "must NOT be ActionItems: {q}");
        }
    }

    #[test]
    fn timeline_amendment_history() {
        let (i, _) = classify("Give me the amendment history of Article 25");
        assert_eq!(i, Intent::Timeline);
    }

    /// Timeline routes to AmendmentChain — a SQL/graph lookup with NO retrieval and
    /// NO model. A false positive therefore does not mis-rank the answer, it skips
    /// the corpus entirely.
    ///
    /// Prod 2026-07-16: "what about the appeal timeline?" matched the bare word
    /// `timeline`, returned 0 chain links in 7 ms, and never read a line of law. The
    /// officer just saw a disconnected answer. In this domain "timeline" usually
    /// means a deadline and "history" a record — neither is an amendment chronology.
    #[test]
    fn deadline_and_record_words_are_not_amendment_timelines() {
        for q in [
            "what about the appeal timeline?",          // the real regression
            "what is the timeline for filing an appeal",
            "timeline to submit the reply",
            "what is my service history",
            "history of the local government department",
        ] {
            assert_ne!(classify(q).0, Intent::Timeline, "must NOT skip retrieval: {q}");
        }
        // …while genuinely amendment-shaped questions still route to Timeline.
        for q in [
            "Give me the amendment history of Article 25",
            "show the amendment timeline of the E&D Rules",
            "timeline of amendments to the SPPRA Rules",
            "how has section 18 been amended over time",
        ] {
            assert_eq!(classify(q).0, Intent::Timeline, "must stay Timeline: {q}");
        }
    }

    #[test]
    fn procedure_how_to_file() {
        let (i, _) = classify("What is the procedure to file an RTI request?");
        assert_eq!(i, Intent::Procedure);
    }

    #[test]
    fn nonsense_falls_back() {
        let (i, c) = classify("ajshdkjahsdkjahsd");
        assert_eq!(i, Intent::Fallback);
        assert_eq!(c, 0.0);
    }
}
