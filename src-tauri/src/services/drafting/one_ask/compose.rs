//! Per-section prose composition (research R2/R4, ADR-0013).
//!
//! The writer model is restricted to the prose lane: it writes connective
//! narrative referencing provisions ONLY as `[CITE:n]` markers and officer
//! figures ONLY as `[FIG:n]` markers. It is told to never write a rule/section
//! number, quote, or monetary figure — and `verify.rs` enforces that anyway.

use super::skeleton::Section;
use super::state::{Figure, OneAskIntent, ProvisionCard};
use regex_lite::Regex;

/// The officer's request, made safe to inject as the drafting subject (FR-014):
/// monetary figure spans are replaced with a neutral phrase so the writer can
/// never echo a raw number as prose — amounts re-enter only via `[FIG:n]`,
/// keeping the verbatim-number gate intact. Whitespace is collapsed.
pub fn sanitized_subject(intent: &OneAskIntent) -> String {
    let neutral = super::intent::figure_regex().replace_all(&intent.raw, "the stated amount");
    neutral.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Writer abstraction so tests use canned strings and production wraps the
/// single ModelSlot (no live LLM in any cargo test).
pub trait SectionWriter: Send + Sync {
    fn write(&self, prompt: &str) -> Result<String, String>;
}

/// Mistral `[INST]` prompt for one section.
///
/// `cards` carries `(global_1_based_id, card)` pairs — marker ids are GLOBAL
/// across the whole draft (verify resolves `[CITE:n]` against the full card
/// vec), while only this section's cards are offered in this prompt. Figures
/// are numbered from 1 over the full intent figure list.
pub fn build_section_prompt(
    section: &Section,
    subject: &str,
    cards: &[(usize, &ProvisionCard)],
    figures: &[Figure],
    urgency: Option<&str>,
    stakeholders: &[String],
) -> String {
    use super::skeleton::SectionId;
    let mut p = String::with_capacity(2048);
    p.push_str("[INST] You are drafting one section of a formal Government of Sindh summary.\n");
    p.push_str(&format!(
        "Subject of this summary (the officer's request — every paragraph MUST be about THIS \
         specific matter; the provisions below are its legal basis, NOT the topic): {}\n",
        subject.trim()
    ));
    p.push_str(&format!("Section: {}\n", section.title));
    p.push_str(&format!("Instruction: {}\n", section.guidance));
    // US9/FR-015: the Urgency move opens with the officer's own time-bound fact,
    // stated plainly — never embellished, omitted entirely when absent.
    if section.id == SectionId::Urgency {
        if let Some(u) = urgency {
            p.push_str(&format!(
                "Time-bound fact the officer stated — state this as plain fact \
                 (no urgency adjectives, no advocacy): \"{}\".\n",
                u.trim()
            ));
        }
    }
    // US9/FR-016: the Stakeholders move names the auto-extracted parties; the
    // StakeholderStatus move describes each, asserting a power ONLY where a
    // supplied card supports it.
    if !stakeholders.is_empty()
        && (section.id == SectionId::Stakeholders || section.id == SectionId::StakeholderStatus)
    {
        p.push_str(&format!(
            "Stakeholders identified in this matter (state a power or status ONLY where a \
             provision above supports it, otherwise name without asserting any power): {}.\n",
            stakeholders.join("; ")
        ));
    }
    // US5/FR-008: only the opening (Urgency) move establishes the case; later
    // moves must not restate it.
    if section.id != SectionId::Urgency {
        p.push_str(
            "The matter is already established in the opening move. Do NOT restate the \
             proposal, the urgency, or the departments — write ONLY this move's content.\n",
        );
    }
    p.push('\n');

    if cards.is_empty() {
        p.push_str(
            "No provisions are available for this section. Write only connective \
             narrative from the officer's request, or state that the position is \
             to be confirmed. Do NOT invent any rule, section, number, or quote.\n\n",
        );
    } else {
        p.push_str("Available provisions (reference ONLY as [CITE:n] markers):\n");
        for (id, c) in cards.iter() {
            let label = c.label.as_deref().unwrap_or(&c.source_title);
            let excerpt: String = c.excerpt.chars().take(300).collect();
            p.push_str(&format!("[CITE:{}] {} — \"{}\"\n", id, label, excerpt));
        }
        p.push('\n');
    }

    if section.wants_figures {
        if figures.is_empty() {
            // US5/FR-010: structured fallback instead of "to be determined later".
            p.push_str(
                "The officer stated NO figures. Write NO figures, amounts, or estimates. \
                 State the financial position in standard secretariat form, e.g. that \
                 financial implications shall be met from the approved budget allocation \
                 and that detailed cost estimates shall be placed on record before issuance \
                 of the purchase order.\n\n",
            );
        } else {
            p.push_str("Officer-stated figures (reference ONLY as [FIG:n] markers):\n");
            for (i, f) in figures.iter().enumerate() {
                p.push_str(&format!("[FIG:{}] {}\n", i + 1, f.text));
            }
            p.push('\n');
        }
    }

    p.push_str(
        "Style — Government of Sindh Secretariat summary (mandatory):\n\
         - Write 1-2 concise, formal paragraphs in file-noting register, ABOUT the subject \
           above; apply the provisions to THIS matter. Do NOT describe the provisions in the \
           abstract.\n\
         - Be decision-oriented and factual. NO advocacy or persuasion, NO academic or \
           explanatory commentary, NO greetings, NO \"we request\" / \"kindly\" / \"we humbly\" \
           / \"we await\" / \"we appreciate\".\n\
         - State each fact once; do not repeat anything already written.\n\
         - NEVER write any rule, section, article, regulation or clause number yourself: \
           place a [CITE:n] marker where a provision belongs.\n\
         - NEVER write any monetary figure or amount yourself: place a [FIG:n] marker \
           where an officer-stated figure belongs.\n\
         - NEVER quote source text yourself; the [CITE:n] marker inserts the quote.\n\
         - Do not number the paragraphs; do not add letterhead, addressee, signature, or \
           subject lines.\n\
         [/INST]",
    );
    p
}

// ── US4 (D7): deterministic composition — no LLM-authored prose ──────────────
//
// The writer model is the only fabrication surface left (it authored connective
// prose, which once invented a gloss — the ¶2 violation). US4 removes it: each
// section's runs are emitted from fixed secretariat templates + verbatim slots
// ([CITE:n]/[FIG:n] markers resolved by verify.rs exactly as before). The prose
// the model can no longer write is replaced by short, fixed connectors; every
// number/quote/figure still arrives only via a marker (G1 holds by construction).

/// Uppercase the first character of `s` (template lead-in after a verbatim span).
fn cap_first(s: &str) -> String {
    let mut chars = s.chars();
    match chars.next() {
        Some(c) => c.to_uppercase().collect::<String>() + chars.as_str(),
        None => String::new(),
    }
}

/// "A", "A and B", "A, B and C" — Oxford-free list for stakeholder names.
fn join_list(items: &[String]) -> String {
    match items {
        [] => String::new(),
        [a] => a.clone(),
        [a, b] => format!("{a} and {b}"),
        [rest @ .., last] => format!("{} and {}", rest.join(", "), last),
    }
}

/// Emit up to `cap` `[CITE:n]` markers for this section's cards (global ids).
fn push_cites(runs: &mut Vec<RawRun>, cards: &[(usize, &ProvisionCard)], cap: u8) {
    for (id, _c) in cards.iter().take(cap as usize) {
        runs.push(RawRun::Cite(*id));
    }
}

/// Deterministically compose one section's runs from fixed templates + verbatim
/// slots — the LLM authors NO prose (US4/D7). Marker placement ([CITE:n]/[FIG:n])
/// is resolved verbatim by `verify::assemble_and_verify` exactly as in the model
/// lane, so the verbatim-substring invariant (G1) is preserved by construction.
pub fn compose_deterministic(
    section: &Section,
    subject: &str,
    cards: &[(usize, &ProvisionCard)],
    figures: &[Figure],
    urgency: Option<&str>,
    stakeholders: &[String],
) -> Vec<RawRun> {
    use super::skeleton::SectionId;
    let subj = subject.trim();
    let mut runs: Vec<RawRun> = Vec::new();
    match section.id {
        SectionId::Urgency => {
            let lead = match urgency.map(str::trim).filter(|u| !u.is_empty()) {
                Some(u) => format!(
                    "{}, the matter of {} is placed before the competent authority for orders.",
                    cap_first(u), subj
                ),
                None => format!(
                    "The matter of {subj} is placed before the competent authority for orders."
                ),
            };
            runs.push(RawRun::Prose(lead));
        }
        SectionId::LegalValidation => {
            if cards.is_empty() {
                runs.push(RawRun::Prose(
                    "No specific governing provision for this matter was located in the indexed corpus.".into(),
                ));
            } else {
                runs.push(RawRun::Prose("The matter is governed by the following provision(s):".into()));
                push_cites(&mut runs, cards, section.max_citations);
            }
        }
        SectionId::Stakeholders => {
            if stakeholders.is_empty() {
                runs.push(RawRun::Prose(
                    "The parties to this matter are as identified in the record.".into(),
                ));
            } else {
                runs.push(RawRun::Prose(format!(
                    "The parties to this matter are {}.",
                    join_list(stakeholders)
                )));
            }
        }
        SectionId::StakeholderStatus => {
            if cards.is_empty() {
                let who = if stakeholders.is_empty() {
                    "the parties".to_string()
                } else {
                    join_list(stakeholders)
                };
                runs.push(RawRun::Prose(format!(
                    "No provision in the indexed corpus establishes the powers of {who}."
                )));
            } else {
                runs.push(RawRun::Prose(
                    "The legal status and powers of the parties are established by:".into(),
                ));
                push_cites(&mut runs, cards, section.max_citations);
            }
        }
        SectionId::Procedure => {
            if cards.is_empty() {
                runs.push(RawRun::Prose(
                    "The applicable procedure is to be confirmed from the governing rules.".into(),
                ));
            } else {
                runs.push(RawRun::Prose("The prescribed procedure is as follows:".into()));
                push_cites(&mut runs, cards, section.max_citations);
            }
        }
        SectionId::FinancialAspect => {
            if figures.is_empty() {
                runs.push(RawRun::Prose(
                    "Financial implications shall be met from the approved budget allocation, with \
                     detailed cost estimates placed on record before issuance of the purchase order.".into(),
                ));
            } else {
                runs.push(RawRun::Prose("The financial implication of this matter is".into()));
                for i in 0..figures.len() {
                    runs.push(RawRun::Fig(i + 1));
                }
                if !cards.is_empty() && section.max_citations > 0 {
                    runs.push(RawRun::Prose("sanctioned under".into()));
                    push_cites(&mut runs, cards, section.max_citations);
                }
            }
        }
        SectionId::PrayerForApproval => {
            runs.push(RawRun::Prose(format!(
                "Approval of the competent authority is solicited for {subj}."
            )));
        }
    }
    runs
}

/// A parsed run of raw writer output, pre-verification.
#[derive(Debug, Clone, PartialEq)]
pub enum RawRun {
    Prose(String),
    /// 1-based card reference from a `[CITE:n]` marker.
    Cite(usize),
    /// 1-based figure reference from a `[FIG:n]` marker.
    Fig(usize),
}

fn marker_regex() -> Regex {
    Regex::new(r"\[(CITE|FIG):(\d+)\]").expect("marker regex compiles")
}

/// Malformed marker shapes the writer sometimes emits ("[FIG:x]", "[CITE:]")
/// — observed live shipping as literal prose. Scrubbed, never rendered.
fn stray_marker_regex() -> Regex {
    Regex::new(r"\[(?:CITE|FIG):[^\]]*\]").expect("stray marker regex compiles")
}

/// Literal "Paragraph 1:" / "Paragraph 2:" labels the writer sometimes prefixes
/// despite the no-numbering instruction (observed live). Scrubbed from prose.
fn paragraph_label_regex() -> Regex {
    Regex::new(r"(?i)\bparagraph\s+\d+\s*:").expect("paragraph label regex compiles")
}

fn push_prose(runs: &mut Vec<RawRun>, text: &str) {
    // Scrub model artifacts (literal paragraph labels + malformed marker shapes),
    // strip advocacy/pleading padding (US10/FR-021 — verbatim-safe; never touches
    // a citation quote, which is a separate run), then collapse whitespace.
    let scrubbed = paragraph_label_regex().replace_all(text, " ");
    let scrubbed = stray_marker_regex().replace_all(&scrubbed, " ");
    let stripped = super::brevity::strip_advocacy(&scrubbed);
    let cleaned = stripped.split_whitespace().collect::<Vec<_>>().join(" ");
    if !cleaned.is_empty() {
        runs.push(RawRun::Prose(cleaned));
    }
}

/// Split raw writer output into ordered prose/marker runs. Marker-free output
/// becomes a single Prose run. Whitespace-only prose fragments are dropped;
/// malformed marker shapes are scrubbed from prose.
pub fn parse_markers(raw: &str) -> Vec<RawRun> {
    let re = marker_regex();
    let mut runs = Vec::new();
    let mut last = 0usize;
    for m in re.find_iter(raw) {
        push_prose(&mut runs, &raw[last..m.start()]);
        let caps = re.captures(m.as_str()).expect("match recaptures");
        let n: usize = caps[2].parse().unwrap_or(0);
        match &caps[1] {
            "CITE" => runs.push(RawRun::Cite(n)),
            _ => runs.push(RawRun::Fig(n)),
        }
        last = m.end();
    }
    push_prose(&mut runs, &raw[last..]);
    runs
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::services::drafting::one_ask::skeleton::{SectionId, SKELETON};

    fn card(label: Option<&str>) -> ProvisionCard {
        ProvisionCard {
            chunk_id: "c1".into(),
            source_id: "s1".into(),
            source_title: "Delegation of Financial Powers 2019".into(),
            excerpt: "Rule 7(2) The competent authority may sanction expenditure…".into(),
            label: label.map(|s| s.to_string()),
            page: Some(3),
            section_ids: vec![SectionId::FinancialAspect],
        }
    }

    fn fin_section() -> &'static Section {
        SKELETON.iter().find(|s| s.id == SectionId::FinancialAspect).unwrap()
    }

    const SUBJECT: &str = "direct contracting with the sole distributor of Canon scanners for e-dafter";

    #[test]
    fn prompt_contains_inst_guidance_cards_and_fig_table() {
        let figures = vec![Figure { text: "4 billion".into(), normalized: None }];
        let c = card(Some("Rule 7(2), DFP 2019"));
        // Global card id 3 — marker ids are global, not per-section.
        let p = build_section_prompt(fin_section(), SUBJECT, &[(3, &c)], &figures, None, &[]);
        assert!(p.starts_with("[INST]") && p.ends_with("[/INST]"));
        assert!(p.contains("Financial Aspect"));
        assert!(p.contains("[CITE:3] Rule 7(2), DFP 2019"));
        assert!(p.contains("[FIG:1] 4 billion"));
        assert!(p.contains("NEVER write any rule, section, article"));
        assert!(p.contains("NEVER write any monetary figure"));
    }

    #[test]
    fn empty_figures_financial_prompt_uses_structured_fallback() {
        // US5/FR-010: omit invented figures, but use the structured secretariat
        // fallback (no "to be determined later").
        let c = card(None);
        let p = build_section_prompt(fin_section(), SUBJECT, &[(1, &c)], &[], None, &[]);
        assert!(p.contains("stated NO figures"));
        assert!(p.contains("Write NO figures"));
        assert!(p.contains("approved budget allocation"));
        assert!(p.contains("placed on record before issuance"));
    }

    #[test]
    fn secretariat_style_bans_advocacy_and_greetings() {
        // US5/FR-008: the style profile forbids persuasion/greeting filler.
        let c = card(Some("Rule 4, SPPRA 2010"));
        let p = build_section_prompt(fin_section(), SUBJECT, &[(1, &c)], &[], None, &[]);
        assert!(p.contains("Government of Sindh Secretariat summary"));
        assert!(p.contains("NO advocacy or persuasion"));
        assert!(p.contains("we await"));
        assert!(p.contains("State each fact once"));
    }

    // ── US4 deterministic composition ────────────────────────────────────────

    fn sec(id: SectionId) -> &'static Section {
        SKELETON.iter().find(|s| s.id == id).unwrap()
    }

    /// A prose run must never contain a digit — every number/quote/figure arrives
    /// only via a [CITE:n]/[FIG:n] marker (G1 by construction).
    fn prose_has_no_digit(runs: &[RawRun]) -> bool {
        runs.iter().all(|r| match r {
            RawRun::Prose(t) => !t.chars().any(|c| c.is_ascii_digit()),
            _ => true,
        })
    }

    #[test]
    fn deterministic_urgency_states_fact_and_subject_no_markers() {
        let runs = compose_deterministic(
            sec(SectionId::Urgency), SUBJECT, &[], &[], Some("within a month"), &[],
        );
        assert_eq!(runs.len(), 1);
        match &runs[0] {
            RawRun::Prose(t) => {
                assert!(t.starts_with("Within a month,"), "got {t:?}");
                assert!(t.contains("Canon scanners"));
            }
            other => panic!("expected prose, got {other:?}"),
        }
    }

    #[test]
    fn deterministic_legal_emits_cite_markers_capped() {
        let c1 = card(Some("Rule 7(2)"));
        let c2 = card(Some("Rule 8"));
        let c3 = card(Some("Rule 9"));
        let legal = sec(SectionId::LegalValidation); // cap 2
        let runs = compose_deterministic(
            legal, SUBJECT, &[(5, &c1), (6, &c2), (7, &c3)], &[], None, &[],
        );
        let cites: Vec<usize> = runs.iter().filter_map(|r| match r {
            RawRun::Cite(n) => Some(*n), _ => None }).collect();
        assert_eq!(cites, vec![5, 6], "global ids, capped at 2");
        assert!(prose_has_no_digit(&runs), "no number written as prose");
    }

    #[test]
    fn deterministic_legal_no_cards_states_absence() {
        let runs = compose_deterministic(sec(SectionId::LegalValidation), SUBJECT, &[], &[], None, &[]);
        assert!(matches!(&runs[0], RawRun::Prose(t) if t.contains("No specific governing provision")));
        assert!(!runs.iter().any(|r| matches!(r, RawRun::Cite(_))));
    }

    #[test]
    fn deterministic_financial_uses_fig_markers_not_prose_numbers() {
        let c = card(Some("Rule 7(2)"));
        let figures = vec![
            Figure { text: "Rs. 2 million".into(), normalized: None },
        ];
        let runs = compose_deterministic(fin_section(), SUBJECT, &[(1, &c)], &figures, None, &[]);
        assert!(runs.iter().any(|r| matches!(r, RawRun::Fig(1))));
        assert!(runs.iter().any(|r| matches!(r, RawRun::Cite(1))));
        assert!(prose_has_no_digit(&runs), "figure must be a marker, not prose");
    }

    #[test]
    fn deterministic_prayer_is_one_sentence_about_subject() {
        let runs = compose_deterministic(sec(SectionId::PrayerForApproval), SUBJECT, &[], &[], None, &[]);
        assert_eq!(runs.len(), 1);
        match &runs[0] {
            RawRun::Prose(t) => {
                assert!(t.starts_with("Approval of the competent authority is solicited"));
                assert_eq!(t.matches('.').count(), 1, "exactly one sentence");
                assert!(t.contains("Canon scanners"));
            }
            other => panic!("expected prose, got {other:?}"),
        }
    }

    #[test]
    fn deterministic_stakeholders_joins_names() {
        let runs = compose_deterministic(
            sec(SectionId::Stakeholders), SUBJECT, &[], &[],
            None, &["the Secretary".into(), "the Director General".into()],
        );
        assert!(matches!(&runs[0],
            RawRun::Prose(t) if t.contains("the Secretary and the Director General")));
    }

    #[test]
    fn non_opening_sections_are_told_not_to_restate() {
        // US5/FR-008: only the opening (Urgency) move establishes the case.
        let c = card(None);
        let fin = build_section_prompt(fin_section(), SUBJECT, &[(1, &c)], &[], None, &[]);
        assert!(fin.contains("Do NOT restate the proposal"));
        let urgency = SKELETON.iter().find(|s| s.id == SectionId::Urgency).unwrap();
        let urgency_prompt = build_section_prompt(urgency, SUBJECT, &[], &[], None, &[]);
        assert!(
            !urgency_prompt.contains("Do NOT restate the proposal"),
            "opening move must NOT carry the no-restate instruction"
        );
    }

    #[test]
    fn prompt_injects_officer_subject_as_the_topic() {
        // US6/FR-014: the officer's request is the subject of the section.
        let c = card(Some("Rule 4, SPPRA 2010"));
        let p = build_section_prompt(fin_section(), SUBJECT, &[(1, &c)], &[], None, &[]);
        assert!(p.contains(SUBJECT), "officer subject must be in the prompt");
        assert!(p.contains("Subject of this summary"));
        assert!(
            p.contains("Do NOT describe the provisions in the abstract"),
            "must instruct against generic rule-digest"
        );
    }

    #[test]
    fn sanitized_subject_neutralises_figures_no_raw_number() {
        // FR-014/FR-011: a figure in the ask must not be injected as a raw number.
        let intent = OneAskIntent {
            raw: "sanction Rs 2 million for 4 billion canon scanners".into(),
            figures: vec![],
            action: None,
        };
        let s = sanitized_subject(&intent);
        assert!(!s.contains('2') && !s.contains('4'), "no raw digits: {s}");
        assert!(s.contains("canon scanners"), "subject nouns preserved: {s}");
        assert!(s.contains("the stated amount"), "figures neutralised: {s}");
    }

    #[test]
    fn parse_markers_extracts_ordered_runs() {
        let raw = "Under [CITE:1], the authority may proceed. An amount of [FIG:1] is solicited.";
        let runs = parse_markers(raw);
        assert_eq!(
            runs,
            vec![
                RawRun::Prose("Under".into()),
                RawRun::Cite(1),
                RawRun::Prose(", the authority may proceed. An amount of".into()),
                RawRun::Fig(1),
                RawRun::Prose("is solicited.".into()),
            ]
        );
    }

    #[test]
    fn marker_free_output_is_single_prose_run() {
        let runs = parse_markers("The matter is placed for kind consideration.");
        assert_eq!(runs.len(), 1);
        assert!(matches!(&runs[0], RawRun::Prose(t) if t.contains("kind consideration")));
    }

    #[test]
    fn out_of_range_marker_still_parses_for_verify_to_flag() {
        let runs = parse_markers("Per [CITE:7] this stands.");
        assert!(runs.contains(&RawRun::Cite(7)));
    }

    #[test]
    fn malformed_markers_are_scrubbed_from_prose() {
        // Observed live: "[FIG:x]" (non-numeric) shipped as literal prose.
        let runs = parse_markers("An amount of [FIG:x] is yet to be determined [CITE:1].");
        assert_eq!(
            runs,
            vec![
                RawRun::Prose("An amount of is yet to be determined".into()),
                RawRun::Cite(1),
                RawRun::Prose(".".into()),
            ]
        );
        // A run that is ONLY a malformed marker disappears entirely.
        assert!(parse_markers("[CITE:] [FIG:abc]").is_empty());
    }

    #[test]
    fn paragraph_number_labels_are_scrubbed() {
        // Observed live: "Paragraph 1:" / "Paragraph 2:" emitted despite the
        // no-numbering instruction — scrub them, keep markers + prose.
        let runs = parse_markers(
            "Paragraph 1: Under [CITE:1] this stands. Paragraph 2: It is duly noted.",
        );
        let prose: String = runs
            .iter()
            .filter_map(|r| match r {
                RawRun::Prose(t) => Some(t.to_lowercase()),
                _ => None,
            })
            .collect::<Vec<_>>()
            .join("|");
        assert!(!prose.contains("paragraph 1"), "{prose}");
        assert!(!prose.contains("paragraph 2"), "{prose}");
        assert!(runs.contains(&RawRun::Cite(1)));
    }
}
