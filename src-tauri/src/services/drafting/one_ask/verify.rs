//! Deterministic assembly + verification gate (FR-005/006/007, research R3).
//!
//! The guarantee lives HERE, not in the prompt: every citation is spliced from
//! a provision card and verbatim-checked; every figure comes from the officer's
//! intent; any stray provision-number or monetary figure the model emitted in
//! prose is downgraded to a visible Flag run — never shipped silently.

use super::compose::RawRun;
use super::skeleton::{SectionId, SKELETON};
use super::state::{OneAskIntent, Paragraph, ProvisionCard, Run};
use regex_lite::Regex;

/// Whitespace/hyphen-tolerant normalisation (same approach as the Phase D
/// ingest gate): collapse whitespace runs and hyphens so line-wrap and
/// "Rule-6(c)" vs "Rule 6(c)" differences don't defeat verbatim checks.
pub(crate) fn normalize(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut last_space = false;
    for c in s.chars() {
        let c = if c == '-' || c.is_whitespace() { ' ' } else { c };
        if c == ' ' {
            if !last_space {
                out.push(' ');
            }
            last_space = true;
        } else {
            out.push(c.to_ascii_lowercase());
            last_space = false;
        }
    }
    out.trim().to_string()
}

/// Provision-number pattern for the stray-number scan (hyphen-tolerant,
/// mirrors the decompose label family).
fn provision_number_re() -> Regex {
    Regex::new(
        r"(?i)\b(article|rule|section|regulation|clause|schedule|chapter|notification|order)[-\s]+\d+[0-9A-Za-z()\-]*",
    )
    .expect("provision number regex compiles")
}

/// §XI canonical citation render: full label when resolvable (deep tier);
/// otherwise instrument + page ref + explicit disclosure (fast tier).
pub(crate) fn rendered_label(card: &ProvisionCard) -> String {
    match &card.label {
        Some(l) => format!("{}, {}", l, card.source_title),
        None => match card.page {
            Some(p) => format!(
                "{}, p. {} (hierarchical detail unavailable)",
                card.source_title, p
            ),
            None => format!("{} (hierarchical detail unavailable)", card.source_title),
        },
    }
}

/// True when `quoted` is a verbatim (normalised) substring of the card excerpt
/// — data-model invariant 1. A trailing ellipsis is display decoration added
/// by `citation_snippet` (cap cut), not quoted text.
fn quote_verifies(quoted: &str, card: &ProvisionCard) -> bool {
    let q = quoted.trim_end_matches('…');
    !q.trim().is_empty() && normalize(&card.excerpt).contains(&normalize(q))
}

/// Splicing the whole chunk excerpt produced raw multi-sentence dumps (with
/// OCR artifacts) inside paragraphs. Quote only the leading operative span:
/// stop at the first sentence boundary once past `SNIPPET_MIN_WORDS`, hard
/// cap at `SNIPPET_MAX_WORDS`. Returned as an exact char-slice of the excerpt
/// so the verbatim invariant holds by construction.
const SNIPPET_MIN_WORDS: usize = 6;
/// US4/FR-006: compress the quoted span to one operative clause so a citation
/// reads as application, not a block-quote. Lowered 30→20 — still a verbatim
/// char-slice, so the verbatim gate holds by construction.
const SNIPPET_MAX_WORDS: usize = 20;
/// A cap cut must not strand a connective ("…50 million to Rs." — observed
/// live); back off these before marking the cut.
const SNIPPET_TRAIL_STOP: &[&str] = &[
    "to", "of", "the", "and", "or", "a", "an", "in", "for", "by", "with",
    "from", "up", "rs", "under", "on", "at", "per", "be", "shall", "may",
    "is", "are", "between",
];

pub(crate) fn citation_snippet(excerpt: &str) -> String {
    let mut trimmed = excerpt.trim();
    // The structural label is prepended to chunk content at index time, so it
    // leads the excerpt — and it duplicates `rendered_label`. Strip it; keep
    // the full excerpt when nothing else remains (label-only chunk).
    if let Some(m) = provision_number_re().find(trimmed) {
        if m.start() == 0 {
            let rest = trimmed[m.end()..].trim_start();
            if !rest.is_empty() {
                trimmed = rest;
            }
        }
    }
    // Word boundaries up to the cut point.
    let mut words: Vec<(usize, usize)> = Vec::new();
    let mut word_start: Option<usize> = None;
    let mut sentence_end_cut = false;
    let chars = trimmed
        .char_indices()
        .chain(std::iter::once((trimmed.len(), ' ')));
    for (i, c) in chars {
        if !c.is_whitespace() {
            word_start.get_or_insert(i);
            continue;
        }
        let Some(ws) = word_start.take() else { continue };
        words.push((ws, i));
        let word = &trimmed[ws..i];
        // Abbreviations like "Rs." / "No." / "p." are not sentence ends.
        let bare = word.trim_end_matches(['.', ';', ')', '"', '\'']);
        if (word.ends_with('.') || word.ends_with(';'))
            && bare.chars().count() >= 3
            && words.len() >= SNIPPET_MIN_WORDS
        {
            sentence_end_cut = true;
            break;
        }
        if words.len() >= SNIPPET_MAX_WORDS {
            break;
        }
    }
    let Some(&(_, last_end)) = words.last() else {
        return trimmed.to_string();
    };
    let cap_cut = !sentence_end_cut && !trimmed[last_end..].trim().is_empty();
    if !cap_cut {
        return trimmed[..last_end].trim_end().to_string();
    }
    // Cap cut mid-sentence: back off stranded connectives, mark the cut.
    let mut n = words.len();
    while n > SNIPPET_MIN_WORDS {
        let (ws, we) = words[n - 1];
        let bare: String = trimmed[ws..we]
            .trim_matches(|c: char| !c.is_alphanumeric())
            .to_ascii_lowercase();
        if SNIPPET_TRAIL_STOP.contains(&bare.as_str()) {
            n -= 1;
        } else {
            break;
        }
    }
    format!("{}…", trimmed[..words[n - 1].1].trim_end())
}

/// Canonical key for a money figure: numeric value + magnitude word, so
/// formatting variants compare equal — "Rs. 10 million" ≡ "Rs.10.00 million",
/// "Rs . 10,000" ≡ "Rs. 10,000" (observed live: every remaining figure flag
/// was such a variant of a source-backed figure). f64 Display trims trailing
/// zeros ("10.00" → "10").
pub(crate) fn money_key(s: &str) -> Option<String> {
    let lower = s.to_ascii_lowercase();
    let chars: Vec<char> = lower.chars().collect();
    let mut i = 0;
    while i < chars.len() && !chars[i].is_ascii_digit() {
        i += 1;
    }
    if i == chars.len() {
        return None;
    }
    let mut num = String::new();
    while i < chars.len() {
        let c = chars[i];
        if c.is_ascii_digit() {
            num.push(c);
        } else if c == ',' {
            // thousands separator
        } else if c == '.' && i + 1 < chars.len() && chars[i + 1].is_ascii_digit() {
            num.push('.');
        } else {
            break;
        }
        i += 1;
    }
    let v: f64 = num.parse().ok()?;
    let mag = ["billion", "million", "crore", "lakhs", "lakh", "thousand"]
        .iter()
        .find(|m| lower.contains(*m))
        .copied()
        .unwrap_or("");
    Some(format!("{v}|{mag}"))
}

/// Does this prose contain a number the model was forbidden to write?
/// Allowed verbatim: officer figures, and numbers/figures that are
/// source-backed by a card ASSIGNED TO THIS SECTION (label or excerpt match)
/// — the writer narrating about the provision it cites is an echo, not an
/// invention (observed live: 14 of 17 flags were such echoes). Money figures
/// compare by canonical key so formatting variants pass. Everything else
/// must arrive via markers and is flagged.
fn foreign_number_in_prose(
    text: &str,
    intent: &OneAskIntent,
    section_cards: &[&ProvisionCard],
) -> Option<String> {
    let backed_text = |hit: &str| {
        let h = normalize(hit);
        section_cards.iter().any(|c| {
            normalize(&c.excerpt).contains(&h)
                || c.label.as_deref().is_some_and(|l| {
                    let l = normalize(l);
                    l.contains(&h) || h.contains(&l)
                })
        })
    };
    // Money-key backing applies ONLY to figure hits — a provision number like
    // "Article 27" must never pass because some card mentions "27 million".
    let backed_money = |hit: &str| {
        let Some(key) = money_key(hit) else { return false };
        section_cards.iter().any(|c| {
            super::intent::figure_regex()
                .find_iter(&c.excerpt)
                .any(|m| money_key(m.as_str()).as_deref() == Some(key.as_str()))
        })
    };
    for m in provision_number_re().find_iter(text) {
        if !backed_text(m.as_str()) {
            return Some(m.as_str().to_string());
        }
    }
    for m in super::intent::figure_regex().find_iter(text) {
        let hit = m.as_str();
        let known = intent.figures.iter().any(|f| {
            normalize(&f.text) == normalize(hit)
                || (money_key(&f.text).is_some() && money_key(&f.text) == money_key(hit))
        });
        if !known && !backed_text(hit) && !backed_money(hit) {
            return Some(hit.to_string());
        }
    }
    None
}

/// Assemble verified paragraphs from per-section raw runs (data-model
/// invariants 1–4). Paragraph numbering is continuous across sections; each
/// section currently yields one paragraph (the writer is asked for 1–2 short
/// paragraphs which we keep as one numbered unit per section for v1).
pub fn assemble_and_verify(
    sections_raw: &[(SectionId, Vec<RawRun>)],
    cards: &[ProvisionCard],
    intent: &OneAskIntent,
) -> Vec<Paragraph> {
    let mut paragraphs = Vec::new();
    let mut number = 1u32;

    for (section_id, raw_runs) in sections_raw {
        let mut runs: Vec<Run> = Vec::new();
        // Cards offered to this section — the prose echo-allowance (and the
        // out-of-section citation check) are scoped to these.
        let section_cards: Vec<&ProvisionCard> = cards
            .iter()
            .filter(|c| c.section_ids.contains(section_id))
            .collect();
        // Card indices already quoted in this paragraph (repeat-cite drop).
        let mut cited: Vec<usize> = Vec::new();
        // US2/FR-003: per-section citation cap (Background/Proposal/Prayer = 0;
        // Financial = 1). Over-cap or 0-cap citations are dropped as a style cut
        // (NOT flagged — a genuine non-existent marker still flags).
        let section_cap = SKELETON
            .iter()
            .find(|s| s.id == *section_id)
            .map(|s| s.max_citations as usize)
            .unwrap_or(0);
        let mut rendered_cites = 0usize;

        if raw_runs.is_empty() {
            let title = SKELETON
                .iter()
                .find(|s| s.id == *section_id)
                .map(|s| s.title)
                .unwrap_or("this section");
            runs.push(Run::Flag {
                reason: format!(
                    "no source available in the indexed corpus for {title}"
                ),
                original: None,
            });
        }

        for rr in raw_runs {
            match rr {
                RawRun::Prose(text) => match foreign_number_in_prose(text, intent, &section_cards) {
                    None => runs.push(Run::Prose { text: text.clone() }),
                    Some(offender) => runs.push(Run::Flag {
                        reason: format!(
                            "unverified reference \"{offender}\" removed — not present \
                             in any indexed source or officer input"
                        ),
                        original: Some(text.clone()),
                    }),
                },
                RawRun::Cite(n) => {
                    // 1-based marker → card index. Marker ids are global, so a
                    // hallucinated id can resolve to a REAL card from another
                    // section — only cards assigned to THIS section may render.
                    match n.checked_sub(1).and_then(|i| cards.get(i)) {
                        None => runs.push(Run::Flag {
                            reason: format!("writer referenced a non-existent provision [CITE:{n}]"),
                            original: None,
                        }),
                        // Narrative-only section (cap 0): drop the citation so the
                        // prose stands alone (US2). A genuine non-existent marker
                        // is handled above and still flags.
                        Some(_) if section_cap == 0 => continue,
                        Some(card) if !card.section_ids.contains(section_id) => {
                            runs.push(Run::Flag {
                                reason: format!(
                                    "writer cited [CITE:{n}] which belongs to a different \
                                     section of the draft"
                                ),
                                original: None,
                            })
                        }
                        Some(_) if rendered_cites >= section_cap => {
                            // Beyond this section's citation cap — style cut.
                            continue;
                        }
                        Some(card) => {
                            // Quote-stack collapse: a citation directly after
                            // another citation, or a repeat of a card already
                            // quoted in this paragraph, adds no narrative —
                            // drop it (style, not a verification failure).
                            if cited.contains(&(n - 1))
                                || matches!(runs.last(), Some(Run::Citation { .. }))
                            {
                                continue;
                            }
                            let quoted = citation_snippet(&card.excerpt);
                            let verified = quote_verifies(&quoted, card);
                            if verified {
                                cited.push(n - 1);
                                rendered_cites += 1;
                                runs.push(Run::Citation {
                                    card_idx: n - 1,
                                    rendered_label: rendered_label(card),
                                    quoted: Some(quoted),
                                    verified: true,
                                });
                            } else {
                                runs.push(Run::Flag {
                                    reason: "citation failed verbatim verification".into(),
                                    original: Some(quoted),
                                });
                            }
                        }
                    }
                }
                RawRun::Fig(n) => match n.checked_sub(1).and_then(|i| intent.figures.get(i)) {
                    Some(f) => runs.push(Run::Figure {
                        fig_idx: n - 1,
                        text: f.text.clone(),
                    }),
                    None => runs.push(Run::Flag {
                        reason: format!("writer referenced a non-existent figure [FIG:{n}]"),
                        original: None,
                    }),
                },
            }
        }

        // Flags interrupt the sentence they replaced; collect them at the
        // paragraph end (stable, so relative order within each group holds)
        // — still never silent, just not mid-sentence.
        let (mut runs, flags): (Vec<Run>, Vec<Run>) = runs
            .into_iter()
            .partition(|r| !matches!(r, Run::Flag { .. }));
        runs.extend(flags);

        paragraphs.push(Paragraph {
            number,
            section: *section_id,
            runs,
        });
        number += 1;
    }
    paragraphs
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::services::drafting::one_ask::state::Figure;

    fn intent() -> OneAskIntent {
        OneAskIntent {
            raw: "I want to get 4 billion to develop schools".into(),
            figures: vec![Figure { text: "4 billion".into(), normalized: None }],
            action: Some("get".into()),
        }
    }

    fn cards() -> Vec<ProvisionCard> {
        vec![
            ProvisionCard {
                chunk_id: "c1".into(),
                source_id: "s1".into(),
                source_title: "Delegation of Financial Powers 2019".into(),
                excerpt: "Rule 7(2) The competent authority may sanction expenditure for development schemes.".into(),
                label: Some("Rule 7(2)".into()),
                page: Some(3),
                section_ids: vec![SectionId::FinancialAspect],
            },
            ProvisionCard {
                chunk_id: "c2".into(),
                source_id: "s2".into(),
                source_title: "Manual for Development Projects 2019".into(),
                excerpt: "Every development scheme shall be prepared on the prescribed PC-I proforma.".into(),
                label: None, // fast-tier card — §XI disclosure form
                page: Some(11),
                section_ids: vec![SectionId::LegalValidation],
            },
        ]
    }

    #[test]
    fn citation_splices_verbatim_and_verifies() {
        let raw = vec![(
            SectionId::FinancialAspect,
            vec![RawRun::Prose("Under".into()), RawRun::Cite(1), RawRun::Fig(1)],
        )];
        let paras = assemble_and_verify(&raw, &cards(), &intent());
        assert_eq!(paras.len(), 1);
        match &paras[0].runs[1] {
            Run::Citation { card_idx, rendered_label, quoted, verified } => {
                assert_eq!(*card_idx, 0);
                assert!(verified);
                assert!(rendered_label.contains("Rule 7(2)"));
                assert!(rendered_label.contains("Delegation of Financial Powers"));
                assert!(quoted.as_deref().unwrap().contains("sanction expenditure"));
            }
            other => panic!("expected Citation, got {other:?}"),
        }
        match &paras[0].runs[2] {
            Run::Figure { fig_idx, text } => {
                assert_eq!(*fig_idx, 0);
                assert_eq!(text, "4 billion");
            }
            other => panic!("expected Figure, got {other:?}"),
        }
    }

    #[test]
    fn citation_quote_is_short_snippet_not_whole_excerpt() {
        let mut cs = cards();
        // 45-word run-on excerpt with no early sentence boundary.
        cs[0].excerpt = "Rule 7(2) The competent authority may sanction expenditure for development \
            schemes subject to availability of funds and observance of all codal formalities \
            prescribed from time to time by the Finance Department including conditions \
            enumerated in the annexure appended to these rules for that specific purpose only"
            .into();
        let raw = vec![(SectionId::FinancialAspect, vec![RawRun::Cite(1)])];
        let paras = assemble_and_verify(&raw, &cs, &intent());
        match &paras[0].runs[0] {
            Run::Citation { quoted, verified, .. } => {
                let q = quoted.as_deref().unwrap();
                assert!(*verified);
                assert!(
                    q.split_whitespace().count() <= 20,
                    "snippet must be ≤20 words, got {}: {q}",
                    q.split_whitespace().count()
                );
                assert!(quote_verifies(q, &cs[0]), "snippet must stay verbatim");
            }
            other => panic!("expected Citation, got {other:?}"),
        }
    }

    #[test]
    fn citation_quote_stops_at_first_sentence_boundary() {
        let mut cs = cards();
        cs[0].excerpt = "The competent authority may sanction expenditure for development \
            schemes. Provided that nothing in this rule shall apply to works expenditure \
            beyond the delegated ceiling without prior concurrence of the Finance Department."
            .into();
        let raw = vec![(SectionId::FinancialAspect, vec![RawRun::Cite(1)])];
        let paras = assemble_and_verify(&raw, &cs, &intent());
        match &paras[0].runs[0] {
            Run::Citation { quoted, verified, .. } => {
                let q = quoted.as_deref().unwrap();
                assert!(*verified);
                assert!(q.contains("development schemes"));
                assert!(
                    !q.contains("Provided that"),
                    "snippet must stop at the first sentence: {q}"
                );
            }
            other => panic!("expected Citation, got {other:?}"),
        }
    }

    #[test]
    fn fast_tier_citation_carries_xi_disclosure() {
        let raw = vec![(SectionId::LegalValidation, vec![RawRun::Cite(2)])];
        let paras = assemble_and_verify(&raw, &cards(), &intent());
        match &paras[0].runs[0] {
            Run::Citation { rendered_label, verified, .. } => {
                assert!(verified);
                assert!(rendered_label.contains("Manual for Development Projects 2019"));
                assert!(rendered_label.contains("p. 11"));
                assert!(rendered_label.contains("hierarchical detail unavailable"));
            }
            other => panic!("expected Citation, got {other:?}"),
        }
    }

    #[test]
    fn stray_provision_number_in_prose_is_flagged() {
        let raw = vec![(
            SectionId::LegalValidation,
            vec![RawRun::Prose("As provided under Article 99 of the framework, this stands.".into())],
        )];
        let paras = assemble_and_verify(&raw, &cards(), &intent());
        match &paras[0].runs[0] {
            Run::Flag { reason, original } => {
                assert!(reason.contains("Article 99"));
                assert!(original.as_deref().unwrap().contains("Article 99"));
            }
            other => panic!("expected Flag, got {other:?}"),
        }
    }

    #[test]
    fn foreign_money_figure_is_flagged_officer_figure_passes() {
        // Officer figure inline in prose is fine; an invented amount is not.
        let ok = vec![(SectionId::FinancialAspect, vec![RawRun::Prose("An amount of 4 billion is solicited.".into())])];
        let paras = assemble_and_verify(&ok, &cards(), &intent());
        assert!(matches!(&paras[0].runs[0], Run::Prose { .. }));

        let bad = vec![(SectionId::FinancialAspect, vec![RawRun::Prose("A provision of Rs. 9 crore stands earmarked.".into())])];
        let paras = assemble_and_verify(&bad, &cards(), &intent());
        match &paras[0].runs[0] {
            Run::Flag { reason, .. } => assert!(reason.contains("Rs. 9 crore")),
            other => panic!("expected Flag, got {other:?}"),
        }
    }

    #[test]
    fn out_of_range_markers_become_flags() {
        let raw = vec![(
            SectionId::PrayerForApproval,
            vec![RawRun::Cite(7), RawRun::Fig(9)],
        )];
        let paras = assemble_and_verify(&raw, &cards(), &intent());
        assert!(matches!(&paras[0].runs[0], Run::Flag { reason, .. } if reason.contains("[CITE:7]")));
        assert!(matches!(&paras[0].runs[1], Run::Flag { reason, .. } if reason.contains("[FIG:9]")));
    }

    #[test]
    fn empty_section_yields_no_source_flag_and_numbering_is_continuous() {
        let raw = vec![
            (SectionId::Urgency, vec![RawRun::Prose("The matter pertains to schools.".into())]),
            (SectionId::LegalValidation, vec![]),
            (SectionId::PrayerForApproval, vec![RawRun::Prose("Approval is solicited.".into())]),
        ];
        let paras = assemble_and_verify(&raw, &cards(), &intent());
        assert_eq!(paras.iter().map(|p| p.number).collect::<Vec<_>>(), vec![1, 2, 3]);
        assert!(matches!(
            &paras[1].runs[0],
            Run::Flag { reason, .. } if reason.contains("no source available")
                && reason.contains("Legal Validation")
        ));
    }

    #[test]
    fn out_of_section_citation_is_flagged_not_rendered() {
        // Marker ids are global, so a writer can hallucinate a [CITE:n] that
        // resolves to a REAL card belonging to another section (observed live:
        // card 6 cited from financialAspect it was never offered to). Card 1
        // here is assigned to FinancialAspect only.
        let raw = vec![(SectionId::LegalValidation, vec![RawRun::Cite(1)])];
        let paras = assemble_and_verify(&raw, &cards(), &intent());
        match &paras[0].runs[0] {
            Run::Flag { reason, .. } => {
                assert!(reason.contains("CITE:1"), "reason should name the marker: {reason}")
            }
            other => panic!("expected Flag, got {other:?}"),
        }
    }

    #[test]
    fn adjacent_citations_collapse_to_first() {
        // Observed live: 3-5 quotes stacked back-to-back with no connecting
        // prose. Only the first of a directly-adjacent run may render.
        let mut cs = cards();
        cs[1].section_ids = vec![SectionId::FinancialAspect];
        let raw = vec![(
            SectionId::FinancialAspect,
            vec![RawRun::Prose("Under".into()), RawRun::Cite(1), RawRun::Cite(2)],
        )];
        let paras = assemble_and_verify(&raw, &cs, &intent());
        let cites: Vec<usize> = paras[0]
            .runs
            .iter()
            .filter_map(|r| match r {
                Run::Citation { card_idx, .. } => Some(*card_idx),
                _ => None,
            })
            .collect();
        assert_eq!(cites, vec![0], "stack must collapse to first: {:?}", paras[0].runs);
    }

    #[test]
    fn repeat_citation_of_same_card_in_paragraph_is_dropped() {
        let raw = vec![(
            SectionId::FinancialAspect,
            vec![
                RawRun::Cite(1),
                RawRun::Prose("and in furtherance thereof".into()),
                RawRun::Cite(1),
            ],
        )];
        let paras = assemble_and_verify(&raw, &cards(), &intent());
        let n_cites = paras[0]
            .runs
            .iter()
            .filter(|r| matches!(r, Run::Citation { .. }))
            .count();
        assert_eq!(n_cites, 1, "{:?}", paras[0].runs);
    }

    fn legal_card(id: &str, label: &str, excerpt: &str) -> ProvisionCard {
        ProvisionCard {
            chunk_id: id.into(),
            source_id: "s".into(),
            source_title: "SPPRA Rules 2010".into(),
            excerpt: excerpt.into(),
            label: Some(label.into()),
            page: Some(1),
            section_ids: vec![SectionId::LegalValidation],
        }
    }

    #[test]
    fn legal_section_caps_citations_at_two() {
        // FR-003: three distinct cards cited in a cap-2 section with connecting
        // prose so they don't stack-collapse — only the first two render.
        let cs = vec![
            legal_card("c1", "Rule 1, SPPRA 2010", "Rule 1 The first operative provision applies here."),
            legal_card("c2", "Rule 2, SPPRA 2010", "Rule 2 The second operative provision applies here."),
            legal_card("c3", "Rule 3, SPPRA 2010", "Rule 3 The third operative provision applies here."),
        ];
        let raw = vec![(
            SectionId::LegalValidation,
            vec![
                RawRun::Cite(1),
                RawRun::Prose("and further".into()),
                RawRun::Cite(2),
                RawRun::Prose("and also".into()),
                RawRun::Cite(3),
            ],
        )];
        let paras = assemble_and_verify(&raw, &cs, &intent());
        let n = paras[0].runs.iter().filter(|r| matches!(r, Run::Citation { .. })).count();
        assert_eq!(n, 2, "Legal caps at 2: {:?}", paras[0].runs);
        // The dropped 3rd is a silent style cut — no Flag.
        assert!(!paras[0].runs.iter().any(|r| matches!(r, Run::Flag { .. })));
    }


    #[test]
    fn zero_cap_section_drops_valid_citation_without_flag() {
        // Prayer (cap 0): a writer-cited valid Legal-owned card is dropped so the
        // prose stands alone — not flagged (mirrors the live §5 prayer citations).
        let cs = vec![legal_card("c1", "Rule 1, SPPRA 2010", "Rule 1 The provision applies here.")];
        let raw = vec![(
            SectionId::PrayerForApproval,
            vec![RawRun::Prose("Approval is solicited".into()), RawRun::Cite(1)],
        )];
        let paras = assemble_and_verify(&raw, &cs, &intent());
        assert!(!paras[0].runs.iter().any(|r| matches!(r, Run::Citation { .. })));
        assert!(!paras[0].runs.iter().any(|r| matches!(r, Run::Flag { .. })));
        assert!(matches!(&paras[0].runs[0], Run::Prose { .. }));
    }

    #[test]
    fn zero_cap_section_still_flags_nonexistent_marker() {
        // The 0-cap drop must NOT swallow a genuinely non-existent marker.
        let raw = vec![(SectionId::PrayerForApproval, vec![RawRun::Cite(99)])];
        let paras = assemble_and_verify(&raw, &cards(), &intent());
        assert!(matches!(&paras[0].runs[0], Run::Flag { reason, .. } if reason.contains("CITE:99")));
    }

    #[test]
    fn snippet_strips_leading_label_prefix() {
        // The label is prepended to chunk content at index time and already
        // appears in rendered_label — the quote must not repeat it.
        let mut cs = cards();
        cs[0].excerpt = "Section 2010 Development project proposals which exceed \
            in their total cost the prescribed thresholds are regulated accordingly."
            .into();
        let raw = vec![(SectionId::FinancialAspect, vec![RawRun::Cite(1)])];
        let paras = assemble_and_verify(&raw, &cs, &intent());
        match &paras[0].runs[0] {
            Run::Citation { quoted, verified, .. } => {
                assert!(*verified);
                let q = quoted.as_deref().unwrap();
                assert!(
                    q.starts_with("Development project"),
                    "label prefix must be stripped: {q}"
                );
            }
            other => panic!("expected Citation, got {other:?}"),
        }
    }

    #[test]
    fn cap_cut_backs_off_connectives_and_marks_ellipsis() {
        // Observed live: "…all projects costing Rs. 50 million to Rs." — a
        // cap cut stranding a connective, with no cut marker. Filler sized so
        // the 20-word cap lands on "Rs." (word 20), backing off "to Rs." → "million…".
        let filler: String = (1..=17).map(|i| format!("w{i} ")).collect();
        let mut cs = cards();
        cs[0].excerpt = format!("{filler}million to Rs. 500 million more words follow here");
        let raw = vec![(SectionId::FinancialAspect, vec![RawRun::Cite(1)])];
        let paras = assemble_and_verify(&raw, &cs, &intent());
        match &paras[0].runs[0] {
            Run::Citation { quoted, verified, .. } => {
                let q = quoted.as_deref().unwrap();
                assert!(*verified, "ellipsis must not defeat the verbatim gate");
                assert!(q.ends_with("million…"), "back off 'to Rs.' and mark cut: {q}");
            }
            other => panic!("expected Citation, got {other:?}"),
        }
    }

    #[test]
    fn sentence_end_cut_carries_no_ellipsis() {
        let paras = assemble_and_verify(
            &[(SectionId::FinancialAspect, vec![RawRun::Cite(1)])],
            &cards(),
            &intent(),
        );
        match &paras[0].runs[0] {
            Run::Citation { quoted, .. } => {
                assert!(!quoted.as_deref().unwrap().contains('…'));
            }
            other => panic!("expected Citation, got {other:?}"),
        }
    }

    #[test]
    fn label_only_excerpt_falls_back_to_full_excerpt() {
        let mut cs = cards();
        cs[0].excerpt = "Rule 7(2)".into();
        let raw = vec![(SectionId::FinancialAspect, vec![RawRun::Cite(1)])];
        let paras = assemble_and_verify(&raw, &cs, &intent());
        assert!(
            matches!(&paras[0].runs[0], Run::Citation { verified: true, .. }),
            "{:?}",
            paras[0].runs
        );
    }

    #[test]
    fn prose_number_backed_by_section_card_is_allowed() {
        // The writer narrating about the provision it cites is an echo of
        // source-backed text, not an invention — "Rule 7(2)" matches card 1's
        // label and card 1 is assigned to this section.
        let raw = vec![(
            SectionId::FinancialAspect,
            vec![RawRun::Prose("In terms of Rule 7(2) the authority may proceed.".into())],
        )];
        let paras = assemble_and_verify(&raw, &cards(), &intent());
        assert!(
            matches!(&paras[0].runs[0], Run::Prose { .. }),
            "{:?}",
            paras[0].runs
        );
    }

    #[test]
    fn prose_figure_backed_by_section_card_excerpt_is_allowed() {
        let mut cs = cards();
        cs[0].excerpt =
            "Rule 7(2) The limit stands enhanced from Rs. 300 million to Rs. 500 million."
                .into();
        let raw = vec![(
            SectionId::FinancialAspect,
            vec![RawRun::Prose("The enhanced limit of Rs. 500 million applies here.".into())],
        )];
        let paras = assemble_and_verify(&raw, &cs, &intent());
        assert!(
            matches!(&paras[0].runs[0], Run::Prose { .. }),
            "{:?}",
            paras[0].runs
        );
    }

    #[test]
    fn money_key_canonicalizes_formatting_variants() {
        // All observed live as false flags.
        assert_eq!(money_key("Rs. 10 million"), money_key("Rs.10.00 million"));
        assert_eq!(money_key("Rs. 10,000"), money_key("Rs . 10,000"));
        assert_eq!(money_key("Rs. 5 billion"), money_key("Rs.5.00 billion"));
        assert_ne!(money_key("Rs. 10 million"), money_key("Rs. 10 billion"));
        assert_ne!(money_key("Rs. 10 million"), money_key("Rs. 100 million"));
        assert_eq!(money_key("no number here"), None);
    }

    #[test]
    fn prose_figure_variant_of_card_figure_is_allowed() {
        // Card says "Rs.10.00 million"; the writer narrates "Rs. 10 million".
        let mut cs = cards();
        cs[0].excerpt =
            "Rule 7(2) All proposals costing Rs.10.00 million and above need a study.".into();
        let raw = vec![(
            SectionId::FinancialAspect,
            vec![RawRun::Prose("Proposals above Rs. 10 million need a feasibility study.".into())],
        )];
        let paras = assemble_and_verify(&raw, &cs, &intent());
        assert!(
            matches!(&paras[0].runs[0], Run::Prose { .. }),
            "{:?}",
            paras[0].runs
        );
    }

    #[test]
    fn provision_number_never_passes_via_money_backing() {
        // Card mentions "27 million"; prose cites "Article 27" — the money
        // key must NOT back a provision number.
        let mut cs = cards();
        cs[0].excerpt = "Rule 7(2) An amount of Rs. 27 million stands allocated.".into();
        let raw = vec![(
            SectionId::FinancialAspect,
            vec![RawRun::Prose("As per Article 27 of the framework this stands.".into())],
        )];
        let paras = assemble_and_verify(&raw, &cs, &intent());
        assert!(
            matches!(&paras[0].runs[0], Run::Flag { reason, .. } if reason.contains("Article 27")),
            "{:?}",
            paras[0].runs
        );
    }

    #[test]
    fn prose_number_from_another_sections_card_is_still_flagged() {
        // Card 1 ("Rule 7(2)") belongs to FinancialAspect — echoing it in
        // LegalProceduralPosition prose is NOT backed there.
        let raw = vec![(
            SectionId::LegalValidation,
            vec![RawRun::Prose("In terms of Rule 7(2) the authority may proceed.".into())],
        )];
        let paras = assemble_and_verify(&raw, &cards(), &intent());
        assert!(
            matches!(&paras[0].runs[0], Run::Flag { .. }),
            "{:?}",
            paras[0].runs
        );
    }

    #[test]
    fn flags_collect_at_paragraph_end_not_mid_sentence() {
        // The stray-reference prose run sits BETWEEN good runs; its Flag must
        // not interrupt the sentence flow — it belongs after the last good run.
        let raw = vec![(
            SectionId::FinancialAspect,
            vec![
                RawRun::Prose("Under".into()),
                RawRun::Prose("as provided under Article 99 this stands".into()),
                RawRun::Cite(1),
            ],
        )];
        let paras = assemble_and_verify(&raw, &cards(), &intent());
        let runs = &paras[0].runs;
        assert_eq!(runs.len(), 3);
        assert!(matches!(runs[0], Run::Prose { .. }), "got {:?}", runs[0]);
        assert!(matches!(runs[1], Run::Citation { .. }), "got {:?}", runs[1]);
        assert!(
            matches!(&runs[2], Run::Flag { reason, .. } if reason.contains("Article 99")),
            "flag must move to paragraph end: {:?}",
            runs[2]
        );
    }

    #[test]
    fn hyphen_and_whitespace_tolerant_verbatim_check() {
        let mut cs = cards();
        cs[0].excerpt = "Rule-7(2)  The competent\nauthority may sanction expenditure.".into();
        assert!(quote_verifies("Rule 7(2) The competent authority", &cs[0]));
        assert!(!quote_verifies("The authority shall never sanction", &cs[0]));
    }
}
