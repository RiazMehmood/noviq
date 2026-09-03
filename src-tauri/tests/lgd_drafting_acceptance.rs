//! Feature 009 — citation-grade acceptance gate (T013, US1).
//!
//! Proves the core contract on a synthetic fixture WITHOUT a live LLM or DB:
//! - SC-001: every cited block's quote is verbatim from a corpus chunk AND the
//!   provision number in its citation appears in that same chunk (number never
//!   invented — ADR-0008).
//! - SC-002: a cited slot with no anchorable source renders as Flagged
//!   (refuse-or-flag, FR-006), never a guessed citation.
//! - FR-007: no prose block contains a provision number/citation, even when the
//!   (injected) generator tries to emit one.
//!
//! The render core (`render_draft`) takes injected retrieval + prose closures, so
//! this gate is fully deterministic. Real LGD gold replaces the fixture in T043.

use echo_ai_lib::services::drafting::render::render_draft;
use echo_ai_lib::services::drafting::template::TemplateSchema;
use echo_ai_lib::services::drafting::{Block, DocRole};
use echo_ai_lib::services::drafting::IntentAnswers;
use echo_ai_lib::services::query_router::Citation;

fn fixture_dir() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/lgd")
}

/// Load a fixture text file into one Citation per non-empty line.
fn load_corpus(file: &str, source_id: &str) -> Vec<Citation> {
    let text = std::fs::read_to_string(fixture_dir().join(file)).expect("read fixture");
    text.lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .enumerate()
        .map(|(i, line)| Citation {
            source_id: source_id.to_string(),
            chunk_id: format!("{source_id}-{i}"),
            excerpt: line.to_string(),
            page: Some((i + 1) as i64),
        })
        .collect()
}

fn load_template() -> TemplateSchema {
    let json = std::fs::read_to_string(fixture_dir().join("template.json")).expect("read template");
    serde_json::from_str(&json).expect("parse template")
}

/// Independent FR-007 check (the crate's own checker is pub(crate)).
fn has_provision_number(text: &str) -> bool {
    let re = regex_lite::Regex::new(
        r"(?i)\b(articles?|sections?|rules?|clauses?|regulations?)\s+\d+[A-Za-z]?\b",
    )
    .unwrap();
    re.is_match(text)
}

#[test]
fn citation_grade_draft_holds_the_contract() {
    let template = load_template();
    let rules = load_corpus("rules.txt", "rules");
    let case = load_corpus("case.txt", "case");

    let retrieve = move |_q: &str, role: DocRole| -> Vec<Citation> {
        match role {
            DocRole::Rules => rules.clone(),
            DocRole::Case => case.clone(),
        }
    };
    // A misbehaving generator that tries to slip a provision number into prose.
    let prose = |_g: &str, _c: &str| {
        "The proposal is submitted under Section 99 for the approval of the competent authority."
            .to_string()
    };

    let intent = IntentAnswers {
        instrument_type: "summary".into(),
        addressee: "Chief Secretary".into(),
        purpose: "release of ADP funds".into(),
        scheme: "XYZ Road Improvement".into(),
    };

    let out = render_draft(&template, &intent, retrieve, prose);

    // Block count + order matches the template (FR-002 / FR-014).
    assert_eq!(out.blocks.len(), template.slots.len(), "one block per slot");
    assert!(matches!(out.blocks[0], Block::Boilerplate { .. }));

    // Reload corpus text for verbatim membership checks.
    let rules_text = std::fs::read_to_string(fixture_dir().join("rules.txt")).unwrap();
    let case_text = std::fs::read_to_string(fixture_dir().join("case.txt")).unwrap();

    let mut saw_cited = false;
    let mut saw_fact = false;
    let mut saw_flagged = false;

    for block in &out.blocks {
        match block {
            // SC-001: verbatim quote + the citation's number lives in that quote.
            Block::Cited { quote, citation, .. } => {
                saw_cited = true;
                assert!(
                    rules_text.contains(quote.as_str()),
                    "cited quote not verbatim in corpus: {quote:?}"
                );
                let cite_label = citation.split(',').next().unwrap_or("").trim();
                assert!(!cite_label.is_empty(), "empty citation label");
                assert!(
                    quote.contains(cite_label),
                    "citation number {cite_label:?} not present in its own quote {quote:?} \
                     (number must be lifted from the chunk, never invented)"
                );
            }
            // Facts are extractive from CASE docs (FR-008).
            Block::Fact { value, attribution, .. } => {
                saw_fact = true;
                assert!(
                    case_text.contains(value.as_str()),
                    "fact value not verbatim in case corpus: {value:?}"
                );
                assert!(attribution.contains("case"), "fact must attribute its source");
            }
            // FR-007: prose carries no provision number, even from a bad generator.
            Block::Prose { text, .. } => {
                assert!(
                    !has_provision_number(text),
                    "prose leaked a provision number: {text:?}"
                );
            }
            // SC-002: unanchored cited slot is flagged, never guessed.
            Block::Flagged { .. } => {
                saw_flagged = true;
            }
            Block::Boilerplate { .. } => {}
        }
    }

    assert!(saw_cited, "fixture must exercise at least one resolved cited slot");
    assert!(saw_fact, "fixture must exercise the extractive fact path");
    assert!(
        saw_flagged,
        "SC-002: the unanchorable cited slot must render as Flagged"
    );
}
