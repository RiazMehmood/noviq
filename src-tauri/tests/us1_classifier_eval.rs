//! T022 — US1 intent classifier evaluation against the real
//! constitution-IR demo prompt set.
//!
//! Source set (verbatim text + hand-labelled expected intent):
//! `~/Desktop/20_constitute_prompts/constitution_ir_test_prompts.md`
//!
//! The classifier under test is rule-only (T024). Misrouting a legal query
//! is a correctness bug, not a quality knob — the gate is therefore **100%
//! exact-match across the full 20-prompt set**, in-scope AND out-of-scope.
//! If a new prompt regresses the bar, add the cue or restructure the
//! classifier; do not lower the threshold.

use echo_ai_lib::services::intent_router::{classifier, types::Intent};

struct Row {
    id: u8,
    text: &'static str,
    expected: Intent,
}

const PROMPTS: &[Row] = &[
    // ─── Tier 1 — Definitional recall ─────────────────────────────────
    Row { id: 1,  expected: Intent::Lookup,       text: "What does the Constitution define as a \"Service of Pakistan\"?" },
    Row { id: 2,  expected: Intent::Lookup,       text: "How is \"Majlis-e-Shoora (Parliament)\" defined in the Constitution?" },
    Row { id: 3,  expected: Intent::Lookup,       text: "What is the constitutional definition of \"State\" for the purposes of Fundamental Rights?" },
    Row { id: 4,  expected: Intent::Lookup,       text: "Quote the definition of \"Money Bill\" as it appears in the Constitution." },
    Row { id: 5,  expected: Intent::Lookup,       text: "What does Article 260 say about the term \"citizen\"?" },

    // ─── Tier 2 — Article-level lookup ────────────────────────────────
    Row { id: 6,  expected: Intent::Lookup,       text: "What does Article 25 guarantee regarding equality of citizens?" },
    Row { id: 7,  expected: Intent::Summarize,    text: "Summarise Article 19 on freedom of speech, including its stated restrictions." },
    Row { id: 8,  expected: Intent::Lookup,       text: "What protections does Article 10A provide?" },
    Row { id: 9,  expected: Intent::ListChildren, text: "List the languages mentioned in Article 251." },
    Row { id: 10, expected: Intent::Lookup,       text: "What does Article 6 declare about high treason?" },

    // ─── Tier 3 — Structural / navigational ───────────────────────────
    Row { id: 11, expected: Intent::ListChildren, text: "Which Articles fall under Chapter 1 of Part II (Fundamental Rights)? List them with titles." },
    Row { id: 12, expected: Intent::ListChildren, text: "What are the Principles of Policy listed in Chapter 2 of Part II?" },
    Row { id: 13, expected: Intent::Lookup,       text: "How many schedules does the Constitution have, and what does the Fourth Schedule cover?" },
    Row { id: 14, expected: Intent::Summarize,    text: "Walk me through the composition of the National Assembly as set out in Article 51." },
    Row { id: 15, expected: Intent::Procedure,    text: "What is the procedure for the election of the President under Article 41?" },

    // ─── Tier 4 — Multi-hop reasoning ─────────────────────────────────
    Row { id: 16, expected: Intent::CrossCompare, text: "Compare the qualifications for membership of the National Assembly (Article 62) with the disqualifications (Article 63). Where do they overlap?" },
    Row { id: 17, expected: Intent::Lookup,       text: "Under what combined provisions can a Provincial Assembly be dissolved, and who has that power?" },
    Row { id: 18, expected: Intent::Procedure,    text: "Trace the path of a Money Bill from introduction to assent — cite every Article involved." },
    Row { id: 19, expected: Intent::Lookup,       text: "What rights of an arrested person are guaranteed across Articles 9, 10, and 10A taken together?" },
    Row { id: 20, expected: Intent::Lookup,       text: "If Fundamental Rights conflict with a Principle of Policy, what does the Constitution say about precedence? Cite the controlling Articles." },
];

fn is_us1(intent: Intent) -> bool {
    matches!(
        intent,
        Intent::Lookup | Intent::ListChildren | Intent::Summarize | Intent::ActionItems
    )
}

#[test]
fn us1_intent_classifier_100pct_on_constitution_prompts() {
    let mut in_scope_total = 0;
    let mut in_scope_correct = 0;
    let mut oos_total = 0;
    let mut oos_correct = 0;
    let mut failures: Vec<(u8, &'static str, Intent, Intent, f32)> = Vec::new();

    for row in PROMPTS {
        let (got, conf) = classifier::classify(row.text);
        let correct = got == row.expected;
        if is_us1(row.expected) {
            in_scope_total += 1;
            if correct { in_scope_correct += 1; }
        } else {
            oos_total += 1;
            if correct { oos_correct += 1; }
        }
        if !correct {
            failures.push((row.id, row.text, row.expected, got, conf));
        }
    }

    let in_scope_acc = in_scope_correct as f32 / in_scope_total as f32;
    let oos_acc = if oos_total > 0 { oos_correct as f32 / oos_total as f32 } else { 0.0 };

    eprintln!("─── US1 intent classifier eval (constitution_ir 20-prompt set) ───");
    eprintln!("In-scope (P1): {}/{} = {:.1}%", in_scope_correct, in_scope_total, in_scope_acc * 100.0);
    eprintln!("Out-of-scope (P2/P3, tracked only): {}/{} = {:.1}%", oos_correct, oos_total, oos_acc * 100.0);
    if !failures.is_empty() {
        eprintln!("─── Failures ─────────────────────────────────────────────────────");
        for (id, text, expected, got, conf) in &failures {
            eprintln!(
                "  #{:>2} expected={:?} got={:?} conf={:.2}  \"{}\"",
                id,
                expected,
                got,
                conf,
                (*text).chars().take(80).collect::<String>()
            );
        }
    }
    eprintln!("──────────────────────────────────────────────────────────────────");

    assert_eq!(
        in_scope_correct, in_scope_total,
        "in-scope classifier accuracy {:.1}% < 100% gate ({}/{} correct) — legal routing is zero-tolerance",
        in_scope_acc * 100.0,
        in_scope_correct,
        in_scope_total
    );
    assert_eq!(
        oos_correct, oos_total,
        "out-of-scope classifier accuracy {:.1}% < 100% gate ({}/{} correct) — legal routing is zero-tolerance",
        oos_acc * 100.0,
        oos_correct,
        oos_total
    );
}
