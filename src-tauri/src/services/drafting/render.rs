//! Feature 009 — slot filling + block assembly (T015–T018, US1 — the MVP).
//!
//! Per ADR-0008 (deterministic citation integrity):
//! - **Cited** slots: select the chunk via
//!   `intent_router::decompose::resolve_provision_label` (title-match, not rank)
//!   → verbatim quote + deterministic citation; `None` → [`Block::Flagged`]
//!   (refuse-or-flag, FR-006).
//! - **Fact** slots: extractive value + attribution from case docs (FR-008).
//! - **Prose** slots: bounded narrative, guaranteed free of numbers/citations
//!   (FR-007) by a defensive sanitizer over the injected generator's output.
//! - **Boilerplate**: verbatim (FR-014).
//!
//! Retrieval and prose generation are INJECTED so this function is deterministic
//! and testable with no live LLM/embeddings (production passes a `query_router`-
//! backed retriever scoped to the confirmed doc set, and an LLM-backed prose fn).

use super::slot::{Slot, SlotKind};
use super::template::TemplateSchema;
use super::{Block, DocRole, DraftOutput, IntentAnswers};
use crate::services::intent_router::decompose::{
    provision_label, render_label, requested_head_noun, resolve_provision_label,
};
use crate::services::query_router::Citation;
use regex_lite::Regex;
use std::sync::OnceLock;

/// Render a full draft. `retrieve(query, role)` returns candidate citations for
/// a slot; `prose(guidance, context)` returns connective narrative.
pub fn render_draft<R, P>(
    template: &TemplateSchema,
    intent: &IntentAnswers,
    retrieve: R,
    prose: P,
) -> DraftOutput
where
    R: Fn(&str, DocRole) -> Vec<Citation>,
    P: Fn(&str, &str) -> String,
{
    let blocks = template
        .slots
        .iter()
        .map(|slot| render_slot(slot, intent, &retrieve, &prose))
        .collect();
    DraftOutput { blocks }
}

/// Render a SINGLE slot to its block (T045, US5 targeted re-render). The full
/// render and every targeted revision share this one code path, so the
/// citation-grade contract (ADR-0008) holds identically on the first render and
/// on every later re-render. Retrieval + prose are injected (deterministic).
pub fn render_slot<R, P>(slot: &Slot, intent: &IntentAnswers, retrieve: &R, prose: &P) -> Block
where
    R: Fn(&str, DocRole) -> Vec<Citation>,
    P: Fn(&str, &str) -> String,
{
    let block = match slot.kind {
        SlotKind::Boilerplate => Block::Boilerplate {
            label: slot.label.clone(),
            text: slot.boilerplate_text.clone().unwrap_or_default(),
        },
        SlotKind::Cited => {
            let topic = slot.required_source_category.clone().unwrap_or_default();
            let candidates = retrieve(&topic, DocRole::Rules);
            fill_cited_slot(slot, &topic, &candidates)
        }
        SlotKind::Prose => {
            let guidance = slot.prose_guidance.clone().unwrap_or_default();
            let context = format!("{} — {}", intent.purpose, intent.scheme);
            Block::Prose {
                label: slot.label.clone(),
                text: sanitize_prose(&prose(&guidance, &context)),
            }
        }
    };
    // A cited slot whose category begins with "fact:" pulls an extractive value
    // from CASE documents instead of a rule citation (FR-008).
    maybe_override_fact(slot, intent, retrieve, block)
}

/// If the slot is a cited slot tagged as a fact (`required_source_category`
/// starting with "fact:"), replace the produced block with an extractive Fact
/// block sourced from CASE documents (FR-008). Otherwise return `block` as-is.
fn maybe_override_fact<R>(
    slot: &Slot,
    _intent: &IntentAnswers,
    retrieve: &R,
    block: Block,
) -> Block
where
    R: Fn(&str, DocRole) -> Vec<Citation>,
{
    if slot.kind != SlotKind::Cited {
        return block;
    }
    let Some(cat) = slot.required_source_category.as_deref() else {
        return block;
    };
    let Some(fact_topic) = cat.strip_prefix("fact:") else {
        return block;
    };
    let topic = fact_topic.trim();
    let candidates = retrieve(topic, DocRole::Case);
    fill_fact_slot(slot, topic, &candidates)
}

/// Fill a cited slot deterministically (ADR-0008). The provision number/label is
/// READ from the matched chunk via `resolve_provision_label`; never generated.
pub(crate) fn fill_cited_slot(slot: &Slot, topic: &str, candidates: &[Citation]) -> Block {
    match resolve_provision_label(topic, candidates) {
        Some(label) => {
            // Find the chunk that produced this label and quote it verbatim.
            let matched = candidates
                .iter()
                .find(|c| provision_label(&c.excerpt).as_deref() == Some(label.as_str()));
            match matched {
                Some(c) => {
                    let head = requested_head_noun(topic);
                    let rendered = render_label(head.as_deref(), &label);
                    Block::Cited {
                        label: slot.label.clone(),
                        quote: c.excerpt.clone(), // verbatim chunk text
                        citation: build_citation(&rendered, c),
                    }
                }
                None => flag(slot),
            }
        }
        None => flag(slot),
    }
}

/// Fill a fact slot extractively from case documents (FR-008). Picks the case
/// citation whose excerpt best overlaps the fact topic; flags if none.
pub(crate) fn fill_fact_slot(slot: &Slot, topic: &str, candidates: &[Citation]) -> Block {
    let topic_tokens: Vec<String> = tokens(topic);
    let best = candidates
        .iter()
        .map(|c| {
            let score = tokens(&c.excerpt)
                .iter()
                .filter(|t| topic_tokens.contains(t))
                .count();
            (score, c)
        })
        .filter(|(score, _)| *score > 0)
        .max_by_key(|(score, _)| *score);
    match best {
        Some((_, c)) => Block::Fact {
            label: slot.label.clone(),
            value: c.excerpt.clone(),
            attribution: attribution(c),
        },
        None => flag(slot),
    }
}

fn flag(slot: &Slot) -> Block {
    Block::Flagged {
        label: slot.label.clone(),
        reason: "unverified — source not available".to_string(),
    }
}

/// The attribution shown on an officer-typed fact value (FR-008a) — explicit and
/// visually distinct from cited content so it can never read as a citation.
pub const OFFICER_SUPPLIED: &str = "officer-supplied";

/// Render an officer-typed fact value (FR-008a, Phase E). The value passes
/// through VERBATIM — the model never generates or alters it — and the block is
/// a `Fact` with the explicit officer-supplied attribution, never a `Cited`.
pub fn officer_supplied_fact(slot: &Slot, value: &str) -> Block {
    Block::Fact {
        label: slot.label.clone(),
        value: value.to_string(),
        attribution: OFFICER_SUPPLIED.to_string(),
    }
}

/// Deterministic citation string from chunk metadata + the resolved label.
fn build_citation(rendered_label: &str, c: &Citation) -> String {
    match c.page {
        Some(p) => format!("{rendered_label}, {} p.{p}", c.source_id),
        None => format!("{rendered_label}, {}", c.source_id),
    }
}

fn attribution(c: &Citation) -> String {
    match c.page {
        Some(p) => format!("{} p.{p}", c.source_id),
        None => c.source_id.clone(),
    }
}

fn provision_ref_re() -> &'static Regex {
    static R: OnceLock<Regex> = OnceLock::new();
    R.get_or_init(|| {
        Regex::new(r"(?i)\b(articles?|sections?|rules?|clauses?|regulations?)\s+\d+[A-Za-z]?\b")
            .unwrap()
    })
}

/// True if the text contains a provision number reference (FR-007 violation).
/// Used by the prose-slot tests and available as a reusable invariant check;
/// the production path enforces FR-007 via [`sanitize_prose`].
#[cfg_attr(not(test), allow(dead_code))]
pub(crate) fn contains_provision_number(text: &str) -> bool {
    provision_ref_re().is_match(text)
}

/// Defensive backstop (FR-007): strip any provision-number reference the prose
/// generator may have emitted, so prose blocks structurally cannot carry a
/// number/citation. The prompt does the real work; this guarantees the invariant.
pub(crate) fn sanitize_prose(text: &str) -> String {
    let cleaned = provision_ref_re().replace_all(text, "the relevant provision");
    // collapse any double spaces introduced by replacement
    cleaned.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn tokens(s: &str) -> Vec<String> {
    s.split(|c: char| !c.is_alphanumeric())
        .map(|t| t.to_ascii_lowercase())
        .filter(|t| t.len() >= 3 && !t.chars().all(|c| c.is_ascii_digit()))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::services::drafting::slot::SlotKind;

    fn cite(source: &str, excerpt: &str) -> Citation {
        Citation {
            source_id: source.to_string(),
            chunk_id: format!("{source}-c"),
            excerpt: excerpt.to_string(),
            page: Some(12),
        }
    }

    fn cited_slot(category: &str) -> Slot {
        Slot {
            ordering: 0,
            kind: SlotKind::Cited,
            label: Some("POLICY / RULES".into()),
            boilerplate_text: None,
            required_source_category: Some(category.into()),
            prose_guidance: None,
            provision_ref: None,
        }
    }

    #[test]
    fn cited_slot_quotes_verbatim_and_reads_number_from_chunk() {
        // The matching chunk is NOT first — title-match-not-rank (ADR-0008).
        let candidates = vec![
            cite("rob", "Preamble In the name of Allah, the most beneficent"),
            cite(
                "fin",
                "Rule 28 financial concurrence of the Finance Department shall be obtained",
            ),
        ];
        let slot = cited_slot("financial concurrence finance");
        let block = fill_cited_slot(&slot, "financial concurrence finance", &candidates);
        match block {
            Block::Cited { quote, citation, .. } => {
                // verbatim quote came from the matched chunk
                assert!(candidates.iter().any(|c| c.excerpt == quote));
                // number read from the chunk, not invented
                assert!(citation.starts_with("Rule 28"), "citation was: {citation}");
                assert!(citation.contains("fin"));
            }
            other => panic!("expected Cited, got {other:?}"),
        }
    }

    #[test]
    fn cited_slot_flags_when_unanchored() {
        let candidates = vec![cite("x", "Preamble fragment with no provision label here")];
        let slot = cited_slot("nonexistent category tokens zzz");
        let block = fill_cited_slot(&slot, "nonexistent category tokens zzz", &candidates);
        assert!(matches!(block, Block::Flagged { .. }), "expected flag, got {block:?}");
    }

    #[test]
    fn prose_is_stripped_of_provision_numbers() {
        assert!(contains_provision_number("as required under Article 28 of the rules"));
        let cleaned = sanitize_prose("The proposal is made under Section 17 for approval.");
        assert!(!contains_provision_number(&cleaned), "still had a number: {cleaned}");
    }

    #[test]
    fn render_slot_reresolves_cited_deterministically_on_revision() {
        // US5: re-resolving a cited slot reads the number from the chunk — an
        // officer can never introduce/alter a citation number (FR-025).
        let slot = cited_slot("financial concurrence finance");
        let intent = IntentAnswers {
            instrument_type: "summary".into(),
            addressee: "CS".into(),
            purpose: "p".into(),
            scheme: "s".into(),
        };
        let retrieve = |_q: &str, _r: DocRole| {
            vec![cite(
                "fin",
                "Rule 28 financial concurrence of the Finance Department shall be obtained",
            )]
        };
        let prose = |_g: &str, _c: &str| String::new();
        match render_slot(&slot, &intent, &retrieve, &prose) {
            Block::Cited { citation, .. } => assert!(citation.starts_with("Rule 28")),
            other => panic!("expected Cited, got {other:?}"),
        }
    }

    #[test]
    fn render_slot_regenerated_prose_stays_number_free() {
        let slot = Slot {
            ordering: 2,
            kind: SlotKind::Prose,
            label: Some("BACKGROUND".into()),
            boilerplate_text: None,
            required_source_category: None,
            prose_guidance: Some("Summarise.".into()),
            provision_ref: None,
        };
        let intent = IntentAnswers {
            instrument_type: "summary".into(),
            addressee: "CS".into(),
            purpose: "p".into(),
            scheme: "s".into(),
        };
        let retrieve = |_q: &str, _r: DocRole| vec![];
        let prose = |_g: &str, _c: &str| "Drawn from Section 9 and Rule 4.".to_string();
        match render_slot(&slot, &intent, &retrieve, &prose) {
            Block::Prose { text, .. } => assert!(!contains_provision_number(&text)),
            other => panic!("expected Prose, got {other:?}"),
        }
    }

    #[test]
    fn officer_supplied_fact_is_verbatim_attributed_and_never_a_citation() {
        // FR-008a (Phase E): the typed value passes through verbatim under the
        // explicit officer-supplied attribution — rendered as Fact, never Cited.
        let slot = Slot {
            ordering: 5,
            kind: SlotKind::Cited,
            label: Some("SCHEME COST".into()),
            boilerplate_text: None,
            required_source_category: Some("fact: scheme cost sanction".into()),
            prose_guidance: None,
            provision_ref: None,
        };
        let value = "Rs. 500 billion for rehabilitation of 100 schools";
        match officer_supplied_fact(&slot, value) {
            Block::Fact { value: v, attribution, label } => {
                assert_eq!(v, value, "value must pass through verbatim");
                assert_eq!(attribution, OFFICER_SUPPLIED);
                assert_eq!(label.as_deref(), Some("SCHEME COST"));
            }
            other => panic!("expected Fact, got {other:?}"),
        }
    }

    #[test]
    fn render_assembles_blocks_in_order_with_injected_fns() {
        let template = TemplateSchema {
            id: None,
            name: "S".into(),
            instrument_type: "summary".into(),
            source_format_ref: None,
            slots: vec![
                Slot {
                    ordering: 0,
                    kind: SlotKind::Boilerplate,
                    label: Some("HEADER".into()),
                    boilerplate_text: Some("SUMMARY FOR THE CHIEF SECRETARY".into()),
                    required_source_category: None,
                    prose_guidance: None,
                    provision_ref: None,
                },
                cited_slot("financial concurrence finance"),
                Slot {
                    ordering: 2,
                    kind: SlotKind::Prose,
                    label: Some("BACKGROUND".into()),
                    boilerplate_text: None,
                    required_source_category: None,
                    prose_guidance: Some("Summarise the background.".into()),
                    provision_ref: None,
                },
            ],
        };
        let intent = IntentAnswers {
            instrument_type: "summary".into(),
            addressee: "Chief Secretary".into(),
            purpose: "release ADP funds".into(),
            scheme: "XYZ road".into(),
        };
        let retrieve = |_q: &str, role: DocRole| -> Vec<Citation> {
            match role {
                DocRole::Rules => vec![cite(
                    "fin",
                    "Rule 28 financial concurrence of the Finance Department shall be obtained",
                )],
                DocRole::Case => vec![],
            }
        };
        // A misbehaving generator that emits a number — must be sanitized out.
        let prose = |_g: &str, _c: &str| "Background under Section 9 is summarised.".to_string();

        let out = render_draft(&template, &intent, retrieve, prose);
        assert_eq!(out.blocks.len(), 3);
        assert!(matches!(out.blocks[0], Block::Boilerplate { .. }));
        assert!(matches!(out.blocks[1], Block::Cited { .. }));
        match &out.blocks[2] {
            Block::Prose { text, .. } => assert!(!contains_provision_number(text)),
            other => panic!("expected Prose, got {other:?}"),
        }
    }
}
