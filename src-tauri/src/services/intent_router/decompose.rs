//! Query decomposition (Feature 008) — split a multi-fact legal question into
//! single-fact sub-questions, answered separately through the existing RAG seam
//! and re-assembled with a DETERMINISTIC template (no LLM merge). Gated to
//! capacity-limited small models; off for 3B+/GPU deployments.
//!
//! All functions here are pure and unit-tested. The async orchestration that
//! calls the RAG seam lives in `intent_router::dispatch`.

use super::types::Slots;
use crate::services::hardware_detector::HardwareTier;
use crate::services::query_router::Citation;
use regex_lite::Regex;
use std::sync::OnceLock;

/// Upper bound on sub-questions. A question with more parts than this falls back
/// to single-pass rather than answering a truncated subset (spec edge case).
pub(crate) const MAX_SUBQUESTIONS: usize = 6;

/// One single-fact question derived from a multi-fact parent.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct SubQuestion {
    /// Display label for the merged block: "(a) the Islamic way of life" or "Article 23".
    pub label: String,
    /// Self-contained question text sent through the RAG seam.
    pub text: String,
    /// When `Some(topic)`, this sub-question asks "which provision deals with
    /// <topic>" and its answer's NUMBER must be resolved deterministically from
    /// the retrieved chunk's label via [`resolve_provision_label`] (Fix B) —
    /// never trusted from the model. `None` for sub-questions whose provision is
    /// already named in the query (the number is then already trustworthy).
    pub number_topic: Option<String>,
}

/// The result of answering one `SubQuestion` through the seam.
pub(crate) struct SubAnswer {
    pub label: String,
    pub answer: String,
    pub citations: Vec<Citation>,
}

/// Deterministic, template-assembled response combining all sub-answers.
pub(crate) struct MergedAnswer {
    pub answer: String,
    pub citations: Vec<Citation>,
}

// ── Capability gate (FR-011/012/013) ───────────────────────────────────────

/// Parse the `ECHO_DECOMPOSE` override value into a forced decision.
fn parse_override(v: Option<&str>) -> Option<bool> {
    match v.map(|s| s.trim().to_ascii_lowercase()).as_deref() {
        Some("1") | Some("on") | Some("true") => Some(true),
        Some("0") | Some("off") | Some("false") => Some(false),
        _ => None,
    }
}

/// Pure capability decision from the auto-detected signals (no env).
fn gate_from_signals(model_size: Option<f32>, tier: &HardwareTier) -> bool {
    match model_size {
        Some(s) => s <= 1.5,
        None => !tier.has_discrete_gpu && tier.ram_gb < 16.0,
    }
}

/// Whether the decomposition path is allowed for the active deployment.
/// `ECHO_DECOMPOSE` override wins; else gate on model size (≤1.5B) with a
/// hardware-profile fallback when size is unknown.
pub(crate) fn decomposition_enabled(model_size: Option<f32>, tier: &HardwareTier) -> bool {
    parse_override(std::env::var("ECHO_DECOMPOSE").ok().as_deref())
        .unwrap_or_else(|| gate_from_signals(model_size, tier))
}

// ── Detection + split (FR-001/002/006) ──────────────────────────────────────

fn enum_marker_re() -> &'static Regex {
    static R: OnceLock<Regex> = OnceLock::new();
    R.get_or_init(|| Regex::new(r"\(([a-z])\)").unwrap())
}

fn section_ref_re() -> &'static Regex {
    static R: OnceLock<Regex> = OnceLock::new();
    R.get_or_init(|| {
        Regex::new(r"(?i)\b(article|articles|section|sections|clause|clauses|rule|rules|regulation|regulations|chapter|part|schedule)\s+[0-9IVXLCDM]").unwrap()
    })
}

fn trailing_instruction_re() -> &'static Regex {
    static R: OnceLock<Regex> = OnceLock::new();
    // A trailing instruction clause appended to an enumeration, e.g.
    // "…Armed Forces, giving each article number." — strip it from a facet.
    R.get_or_init(|| {
        Regex::new(r"(?i),?\s*(giving|give|list|state|provide|including|with)\b.*$").unwrap()
    })
}

/// Detect a multi-fact question and split it into ordered single-fact
/// sub-questions. Returns `None` (→ single-pass) when neither shape matches,
/// fewer than 2 sub-questions result, or the part count exceeds the cap.
pub(crate) fn detect_and_split(query: &str, slots: &Slots) -> Option<Vec<SubQuestion>> {
    // Enumeration shape takes precedence.
    if let Some(subs) = split_enumeration(query) {
        if subs.len() > MAX_SUBQUESTIONS {
            return None; // over-cap → single-pass (no truncated subset)
        }
        if subs.len() >= 2 {
            return Some(subs);
        }
    }
    if let Some(subs) = split_multi_article(query, slots) {
        if subs.len() > MAX_SUBQUESTIONS {
            return None;
        }
        if subs.len() >= 2 {
            return Some(subs);
        }
    }
    // Lowest precedence: two facets of a SINGLE named provision (Q4/Q6).
    if let Some(subs) = split_single_article_facets(query, slots) {
        if subs.len() >= 2 {
            return Some(subs);
        }
    }
    None
}

/// Split on explicit `(a) … (b) … (c) …` markers. `None` if < 2 markers.
fn split_enumeration(query: &str) -> Option<Vec<SubQuestion>> {
    let marks: Vec<(usize, usize, char)> = enum_marker_re()
        .captures_iter(query)
        .filter_map(|c| {
            let m = c.get(0)?;
            let letter = c.get(1)?.as_str().chars().next()?;
            Some((m.start(), m.end(), letter))
        })
        .collect();
    if marks.len() < 2 {
        return None;
    }
    let stem = query[..marks[0].0].trim().trim_end_matches(':').trim();
    let mut subs = Vec::with_capacity(marks.len());
    for i in 0..marks.len() {
        let facet_start = marks[i].1;
        let facet_end = if i + 1 < marks.len() {
            marks[i + 1].0
        } else {
            query.len()
        };
        let raw = &query[facet_start..facet_end];
        let facet = clean_facet(raw);
        if facet.is_empty() {
            continue;
        }
        let text = format!("{stem} {facet}").trim().to_string();
        subs.push(SubQuestion {
            label: format!("({}) {}", marks[i].2, facet),
            // The facet IS the topic the answer's provision number is matched on.
            number_topic: Some(facet.clone()),
            text,
        });
    }
    if subs.len() < 2 {
        return None;
    }
    Some(subs)
}

/// Trim a facet: surrounding whitespace, commas, leading "and ", and any
/// trailing appended instruction clause.
fn clean_facet(raw: &str) -> String {
    let mut s = raw.trim().trim_matches(|c: char| c == ',' || c == ';').trim();
    s = s.strip_prefix("and ").unwrap_or(s).trim();
    let s = trailing_instruction_re().replace(s, "");
    s.trim().trim_end_matches('.').trim().to_string()
}

/// Split a question that names ≥2 provisions, one sub-question per provision.
fn split_multi_article(query: &str, slots: &Slots) -> Option<Vec<SubQuestion>> {
    if slots.section_paths.len() < 2 {
        return None;
    }
    // Try clause split first; keep clauses that contain a section reference.
    let sep = clause_sep_re();
    let clauses: Vec<&str> = sep
        .split(query)
        .map(|c| c.trim())
        .filter(|c| !c.is_empty() && section_ref_re().is_match(c))
        .collect();

    if clauses.len() == slots.section_paths.len() {
        let subs: Vec<SubQuestion> = clauses
            .iter()
            .zip(slots.section_paths.iter())
            .map(|(clause, path)| SubQuestion {
                label: path.clone(),
                text: ensure_question(clause),
                number_topic: None,
            })
            .collect();
        return Some(subs);
    }

    // Fallback: per-article template so every named provision gets a sub-question.
    let subs: Vec<SubQuestion> = slots
        .section_paths
        .iter()
        .map(|path| SubQuestion {
            label: path.clone(),
            text: format!("What does {path} provide?"),
            number_topic: None,
        })
        .collect();
    Some(subs)
}

fn clause_sep_re() -> &'static Regex {
    static R: OnceLock<Regex> = OnceLock::new();
    R.get_or_init(|| Regex::new(r"(?i),\s+and\s+|;\s+|\s+and\s+").unwrap())
}

// ── Single-article two-facet split (FR-001 shape c; Q4/Q6) ──────────────────

fn count_cue_re() -> &'static Regex {
    static R: OnceLock<Regex> = OnceLock::new();
    R.get_or_init(|| Regex::new(r"(?i)\b(two|three|both)\b").unwrap())
}

/// "… concerning/about/regarding X and Y" — captures the coordinated facet pair.
fn facet_pair_re() -> &'static Regex {
    static R: OnceLock<Regex> = OnceLock::new();
    R.get_or_init(|| {
        Regex::new(r"(?i)(?:concerning|regarding|about|relating to|as to)\s+(.+?)\s+and\s+(.+?)\s*[?.!]*\s*$").unwrap()
    })
}

/// "…, and its {condition|exception|limitation|proviso|restriction}" qualifier.
fn and_its_qualifier_re() -> &'static Regex {
    static R: OnceLock<Regex> = OnceLock::new();
    R.get_or_init(|| {
        Regex::new(r"(?i)^(.*?),?\s+and\s+its\s+(conditions?|exceptions?|limitations?|provisos?|restrictions?)\b").unwrap()
    })
}

/// Split a question that asks for two facets of ONE named provision. Conservative:
/// fires only on an explicit count cue + coordinated pair (Pattern A) or an
/// "and its <qualifier>" tail (Pattern B). Returns exactly 2 sub-questions or None.
fn split_single_article_facets(query: &str, slots: &Slots) -> Option<Vec<SubQuestion>> {
    if slots.section_paths.len() != 1 {
        return None;
    }
    let article = &slots.section_paths[0];

    // Pattern A: count cue + "concerning/about X and Y".
    if count_cue_re().is_match(query) {
        if let Some(c) = facet_pair_re().captures(query) {
            let a = c.get(1).map(|m| m.as_str().trim()).unwrap_or("");
            let b = c.get(2).map(|m| m.as_str().trim()).unwrap_or("");
            let short = |s: &str| !s.is_empty() && s.split_whitespace().count() <= 6;
            if short(a) && short(b) && !section_ref_re().is_match(a) && !section_ref_re().is_match(b) {
                return Some(vec![
                    SubQuestion {
                        label: a.to_string(),
                        text: format!("What does {article} say about {a}?"),
                        number_topic: None,
                    },
                    SubQuestion {
                        label: b.to_string(),
                        text: format!("What does {article} say about {b}?"),
                        number_topic: None,
                    },
                ]);
            }
        }
    }

    // Pattern B: "<main>, and its <qualifier>".
    if let Some(c) = and_its_qualifier_re().captures(query) {
        let main = c.get(1).map(|m| m.as_str().trim()).unwrap_or("");
        let qualifier = c.get(2).map(|m| m.as_str().trim()).unwrap_or("");
        if !main.is_empty() && main.split_whitespace().count() >= 3 {
            // Normalise the qualifier to singular for the question text.
            let q_singular = qualifier.trim_end_matches('s');
            return Some(vec![
                SubQuestion {
                    label: article.clone(),
                    text: ensure_question(main),
                    number_topic: None,
                },
                SubQuestion {
                    label: format!("its {q_singular}"),
                    text: format!("What {q_singular} applies to {article}?"),
                    number_topic: None,
                },
            ]);
        }
    }

    None
}

fn ensure_question(clause: &str) -> String {
    let c = clause.trim().trim_end_matches(['.', ',']).trim();
    if c.ends_with('?') {
        c.to_string()
    } else {
        format!("{c}?")
    }
}

// ── Deterministic merge (FR-004/005/008/009) ────────────────────────────────

fn citation_marker_re() -> &'static Regex {
    static R: OnceLock<Regex> = OnceLock::new();
    R.get_or_init(|| Regex::new(r"\[(\d+)\]").unwrap())
}

/// Merge sub-answers into one labelled, globally-cited answer. Each block's
/// trailing `[n]` is synthesized from its resolved citations (the small model
/// usually emits none); any pre-existing local `[k]` are remapped to global
/// indices. "Not found" / uncited blocks render verbatim with no marker.
pub(crate) fn merge_subanswers(answers: &[SubAnswer]) -> MergedAnswer {
    let mut global: Vec<Citation> = Vec::new();
    let mut blocks: Vec<String> = Vec::with_capacity(answers.len());

    for ans in answers {
        let base = global.len();
        let local_count = ans.citations.len();
        global.extend(ans.citations.iter().cloned());

        // Remap any local [k] → [k+base].
        let remapped = citation_marker_re()
            .replace_all(&ans.answer, |caps: &regex_lite::Captures| {
                let k: usize = caps[1].parse().unwrap_or(0);
                format!("[{}]", k + base)
            })
            .to_string();

        let has_marker = citation_marker_re().is_match(&remapped);
        let body = if !has_marker && local_count > 0 {
            // Synthesize this block's markers from its resolved citations.
            let markers: String = (1..=local_count).map(|i| format!("[{}]", base + i)).collect();
            format!("{remapped} {markers}")
        } else {
            remapped
        };

        let block = if ans.label.trim().is_empty() {
            body
        } else {
            format!("{} — {}", ans.label, body)
        };
        blocks.push(block);
    }

    MergedAnswer {
        answer: blocks.join("\n\n"),
        citations: global,
    }
}

// ── Fix B: deterministic, title-matched provision number (zero-tolerance) ────
//
// V4 proved the 0.5B confabulates the article/section number even with the
// correct labelled chunk in context (Q15(a) → "Article 29" while "Section 31"
// sat in front of it). For the citation-grade contract the NUMBER must never
// come from the model: it is read verbatim from the retrieved chunk's leading
// structural label, and the chunk is chosen by TITLE match (NOT rank — the
// correct chunk is often not rank 1), with a refuse path when nothing matches.

fn leading_label_re() -> &'static Regex {
    static R: OnceLock<Regex> = OnceLock::new();
    R.get_or_init(|| {
        // Anchored at the start of the excerpt: the structural label the indexer
        // prepends. Number token is case-sensitive (uppercase Roman only) so it
        // is never confused with lowercase word letters.
        Regex::new(
            r"^\s*((?i:article|section|clause|rule|regulation|chapter|part|schedule))\s+([0-9]+[A-Za-z]?|[IVXLCDM]+)\b",
        )
        .unwrap()
    })
}

/// Parse the leading structural label ("Section 31", "Article 25A") from a
/// chunk excerpt. `None` when the excerpt does not begin with one (e.g. a
/// Preamble or page-break fragment).
///
/// `pub` (not `pub(crate)`): Feature 010 one-ask research reuses this as the
/// deterministic label source for provision cards (ADR-0013 — numbers are
/// read from indexed text, never generated).
pub fn provision_label(excerpt: &str) -> Option<String> {
    let c = leading_label_re().captures(excerpt)?;
    let head = c.get(1)?.as_str();
    let num = c.get(2)?.as_str();
    // FR-006: a year in the heading ("…Rules, 2017…", "…Act, 2010…") is NOT a
    // provision number. A bare 4-digit number in a plausible calendar range is
    // overwhelmingly a year, never an ordinal (real section/rule numbers reach
    // 4 digits only in pathological cases and never carry a sub-letter), so we
    // refuse to mint "Rule 2017"/"Section 2010" from it.
    if looks_like_year(num) {
        return None;
    }
    Some(format!("{} {}", titlecase_word(head), num))
}

/// A bare 4-digit number in the calendar range 1800–2099 (no trailing letter):
/// treat as a year, not a provision ordinal (FR-006).
fn looks_like_year(num: &str) -> bool {
    num.len() == 4
        && num.bytes().all(|b| b.is_ascii_digit())
        && matches!(num.parse::<u32>(), Ok(y) if (1800..=2099).contains(&y))
}

fn titlecase_word(s: &str) -> String {
    let mut ch = s.chars();
    match ch.next() {
        Some(f) => f.to_uppercase().collect::<String>() + &ch.as_str().to_lowercase(),
        None => String::new(),
    }
}

/// Content tokens of a string: lowercased, ≥3 chars, not a pure number, not a
/// stopword. Used for topic↔title overlap scoring.
fn significant_tokens(s: &str) -> Vec<String> {
    const STOP: &[&str] = &[
        "the", "and", "for", "with", "its", "that", "this", "what", "does",
        "say", "about", "are", "which", "have", "has", "from", "into", "under",
        "shall", "state", "every", "person", "persons", "citizen", "citizens",
    ];
    s.split(|c: char| !c.is_alphanumeric())
        .map(|t| t.to_ascii_lowercase())
        .filter(|t| t.len() >= 3 && !t.chars().all(|c| c.is_ascii_digit()) && !STOP.contains(&t.as_str()))
        .collect()
}

/// The title region of an excerpt: the text after the leading label, cut at the
/// first clause marker `(` and capped at 10 words — enough to carry the heading
/// ("Islamic way of life") without dragging in the body.
fn title_region(excerpt: &str) -> String {
    let after = match leading_label_re().find(excerpt) {
        Some(m) => &excerpt[m.end()..],
        None => excerpt,
    };
    let head = after.split('(').next().unwrap_or(after);
    head.split_whitespace().take(10).collect::<Vec<_>>().join(" ")
}

/// Deterministically resolve the authoritative provision label whose TITLE best
/// matches `topic`, reading the number from the chunk (never the model). Returns
/// `None` (→ caller flags "could not verify") when no retrieved title overlaps
/// the topic by a majority of its content tokens.
pub(crate) fn resolve_provision_label(topic: &str, citations: &[Citation]) -> Option<String> {
    let topic_tokens = significant_tokens(topic);
    if topic_tokens.is_empty() {
        return None;
    }
    let required = ((topic_tokens.len() + 1) / 2).max(1);

    let mut best: Option<(usize, String)> = None;
    for c in citations {
        let Some(label) = provision_label(&c.excerpt) else {
            continue;
        };
        let title = significant_tokens(&title_region(&c.excerpt));
        let score = topic_tokens.iter().filter(|t| title.contains(t)).count();
        if score >= required && best.as_ref().map(|(s, _)| score > *s).unwrap_or(true) {
            best = Some((score, label));
        }
    }
    best.map(|(_, label)| label)
}

/// Refuse sentinel for a number-topic sub-question whose provision could not be
/// confidently matched to a retrieved chunk (zero-tolerance: flag, never guess).
pub(crate) const NUMBER_UNVERIFIED: &str =
    "Could not verify the provision number from the retrieved passages.";

fn head_noun_re() -> &'static Regex {
    static R: OnceLock<Regex> = OnceLock::new();
    // The provision head noun on its own (no trailing number required, unlike
    // section_ref_re): "articles dealing with…" must still yield "Article".
    R.get_or_init(|| {
        Regex::new(r"(?i)\b(articles?|sections?|clauses?|rules?|regulations?|chapters?|parts?|schedules?)\b").unwrap()
    })
}

/// The provision head noun the user asked in — "articles" → "Article". Used to
/// render the number in the user's terms while keeping the corpus label for
/// traceability. `None` when the query names no provision head noun.
pub(crate) fn requested_head_noun(query: &str) -> Option<String> {
    let m = head_noun_re().find(query)?;
    Some(titlecase_word(&singularise_head(m.as_str())))
}

/// Singular form of a provision head noun ("articles" → "article"). All the
/// heads we care about pluralise with a plain trailing -s.
fn singularise_head(head: &str) -> String {
    head.to_ascii_lowercase().trim_end_matches('s').to_string()
}

/// Split a parsed label "Section 31" into ("Section", "31").
fn split_label(label: &str) -> Option<(&str, &str)> {
    label.split_once(' ')
}

/// Render the authoritative label in the user's head noun, keeping the original
/// corpus label in brackets when they differ — "Section 31" + requested
/// "Article" → "Article 31 (Section 31)". Same head (or no request) → unchanged.
pub(crate) fn render_label(requested_head: Option<&str>, original_label: &str) -> String {
    match (requested_head, split_label(original_label)) {
        (Some(req), Some((orig_head, num))) if !req.eq_ignore_ascii_case(orig_head) => {
            format!("{req} {num} ({original_label})")
        }
        _ => original_label.to_string(),
    }
}

/// Build the deterministic body for a resolved number-topic sub-answer: the
/// authoritative label (read from the chunk, not the model — rendered in the
/// user's head noun via [`render_label`]) plus that chunk's verbatim title.
pub(crate) fn authoritative_body(
    requested_head: Option<&str>,
    original_label: &str,
    citations: &[Citation],
) -> String {
    let title = citations
        .iter()
        .find(|c| provision_label(&c.excerpt).as_deref() == Some(original_label))
        .map(|c| title_region(&c.excerpt))
        .unwrap_or_default();
    let rendered = render_label(requested_head, original_label);
    if title.is_empty() {
        rendered
    } else {
        format!("**{rendered}** — {title}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tier(ram: f32, gpu: bool) -> HardwareTier {
        let mut t = HardwareTier::default_for_test();
        t.ram_gb = ram;
        t.has_discrete_gpu = gpu;
        t
    }

    fn cite(id: &str) -> Citation {
        Citation {
            source_id: id.to_string(),
            chunk_id: format!("{id}-chunk"),
            excerpt: format!("excerpt for {id}"),
            page: None,
        }
    }

    fn cite_ex(id: &str, excerpt: &str) -> Citation {
        Citation {
            source_id: id.to_string(),
            chunk_id: format!("{id}-chunk"),
            excerpt: excerpt.to_string(),
            page: None,
        }
    }

    fn slots_with(paths: &[&str]) -> Slots {
        let mut s = Slots::default();
        s.section_paths = paths.iter().map(|p| p.to_string()).collect();
        s
    }

    // T003 — capability gate matrix (pure helpers, no env races).
    #[test]
    fn gate_enabled_only_for_small_models() {
        let no_gpu = tier(8.0, false);
        assert!(gate_from_signals(Some(0.5), &no_gpu));
        assert!(gate_from_signals(Some(1.5), &no_gpu));
        assert!(!gate_from_signals(Some(3.0), &no_gpu));
        assert!(!gate_from_signals(Some(7.0), &no_gpu));
        // Unknown size → hardware fallback.
        assert!(gate_from_signals(None, &tier(8.0, false)));
        assert!(!gate_from_signals(None, &tier(32.0, false)));
        assert!(!gate_from_signals(None, &tier(8.0, true)));
        // Override parsing.
        assert_eq!(parse_override(Some("1")), Some(true));
        assert_eq!(parse_override(Some("on")), Some(true));
        assert_eq!(parse_override(Some("0")), Some(false));
        assert_eq!(parse_override(Some("off")), Some(false));
        assert_eq!(parse_override(None), None);
        assert_eq!(parse_override(Some("garbage")), None);
    }

    // T005 — merge.
    #[test]
    fn merge_renumbers_citations_and_labels_blocks() {
        let answers = vec![
            SubAnswer { label: "(a)".into(), answer: "Foo [1]".into(), citations: vec![cite("a")] },
            SubAnswer { label: "(b)".into(), answer: "Bar [1]".into(), citations: vec![cite("b")] },
        ];
        let m = merge_subanswers(&answers);
        assert!(m.answer.contains("(a) — Foo [1]"), "got: {}", m.answer);
        assert!(m.answer.contains("(b) — Bar [2]"), "got: {}", m.answer);
        assert_eq!(m.citations.len(), 2);
    }

    #[test]
    fn merge_synthesizes_marker_for_markerless_block() {
        let answers = vec![SubAnswer {
            label: "Article 9".into(),
            answer: "No person shall be deprived of life or liberty save in accordance with law".into(),
            citations: vec![cite("x")],
        }];
        let m = merge_subanswers(&answers);
        assert!(m.answer.trim_end().ends_with("[1]"), "got: {}", m.answer);
    }

    #[test]
    fn merge_renders_not_found_block_uncited() {
        let answers = vec![
            SubAnswer { label: "Article 23".into(), answer: "Property rights [1]".into(), citations: vec![cite("p")] },
            SubAnswer { label: "Article 99".into(), answer: "The named provision was not found in the retrieved passages.".into(), citations: vec![] },
        ];
        let m = merge_subanswers(&answers);
        assert!(m.answer.contains("Article 99 — The named provision was not found"), "got: {}", m.answer);
        // The not-found block carries no citation marker.
        let after = m.answer.split("Article 99 —").nth(1).unwrap();
        assert!(!after.contains('['), "not-found block must be uncited: {after}");
        assert_eq!(m.citations.len(), 1);
    }

    // T015 (US3) — single-fact yields no split.
    #[test]
    fn single_fact_query_yields_no_split() {
        let s = slots_with(&["Article 9"]);
        assert!(detect_and_split("What does Article 9 guarantee about security of person?", &s).is_none());
    }

    // T009 (US1) — enumeration split.
    #[test]
    fn enumeration_splits_into_three_subquestions() {
        let q = "Identify the Principles of Policy articles dealing with (a) the Islamic way of life, (b) promotion of local Government institutions, and (c) participation of people in the Armed Forces, giving each article number.";
        let s = slots_with(&[]);
        let subs = detect_and_split(q, &s).expect("should split");
        assert_eq!(subs.len(), 3, "got {:?}", subs);
        assert!(subs[0].label.starts_with("(a)"));
        assert!(subs[1].label.starts_with("(b)"));
        assert!(subs[2].label.starts_with("(c)"));
        // Each sub-question keeps the stem AND its facet; trailing instruction stripped.
        for sub in &subs {
            assert!(sub.text.contains("Principles of Policy"), "stem missing: {}", sub.text);
            assert!(!sub.text.to_lowercase().contains("giving each"), "trailing instr leaked: {}", sub.text);
        }
        assert!(subs[0].text.contains("Islamic way of life"), "{}", subs[0].text);
        assert!(subs[1].text.contains("local Government"), "{}", subs[1].text);
        assert!(subs[2].text.contains("Armed Forces"), "{}", subs[2].text);
    }

    // T011 (US1) — boundary: single marker / sub-clause numbering must NOT split.
    #[test]
    fn enumeration_single_marker_falls_back() {
        let s = slots_with(&[]);
        assert!(detect_and_split("What does paragraph (a) of Article 24 cover?", &s).is_none());
    }

    #[test]
    fn enumeration_ignored_when_only_subclause_numbering() {
        let s = slots_with(&["Section 7(1)(a)"]);
        // One provision, no lettered enumeration prose → no split.
        assert!(detect_and_split("Quote Section 7(1)(a) verbatim.", &s).is_none());
    }

    // T012 (US2) — multi-article split, one sub-question per provision.
    #[test]
    fn multi_article_splits_per_provision() {
        let q = "What property rights does Article 23 grant, and what additional protection does Article 24 provide?";
        let s = slots_with(&["Article 23", "Article 24"]);
        let subs = detect_and_split(q, &s).expect("should split");
        assert_eq!(subs.len(), 2, "got {:?}", subs);
        assert_eq!(subs[0].label, "Article 23");
        assert_eq!(subs[1].label, "Article 24");
        assert!(subs[0].text.to_lowercase().contains("property"), "{}", subs[0].text);
        assert!(subs[1].text.to_lowercase().contains("protection"), "{}", subs[1].text);
    }

    // T021 (US2b) — single-article two-facet split (Q4 / Q6) + conservative guard.
    #[test]
    fn single_article_two_facets_splits() {
        let q = "What are the two protections in Article 14 concerning dignity and torture?";
        let s = slots_with(&["Article 14"]);
        let subs = detect_and_split(q, &s).expect("Q4 should split");
        assert_eq!(subs.len(), 2, "got {:?}", subs);
        assert!(subs.iter().all(|x| x.text.contains("Article 14")), "{:?}", subs);
        assert!(subs[0].text.to_lowercase().contains("dignity"), "{}", subs[0].text);
        assert!(subs[1].text.to_lowercase().contains("torture"), "{}", subs[1].text);
    }

    #[test]
    fn single_article_and_its_qualifier_splits() {
        let q = "What is the freedom of assembly under Article 16, and its condition?";
        let s = slots_with(&["Article 16"]);
        let subs = detect_and_split(q, &s).expect("Q6 should split");
        assert_eq!(subs.len(), 2, "got {:?}", subs);
        assert!(subs[0].text.to_lowercase().contains("freedom of assembly"), "{}", subs[0].text);
        assert!(subs[1].text.to_lowercase().contains("condition"), "{}", subs[1].text);
        assert!(subs[1].text.contains("Article 16"), "{}", subs[1].text);
    }

    #[test]
    fn single_article_no_cue_no_split() {
        // "about X and Y" WITHOUT a count cue must NOT split (conservative).
        let s = slots_with(&["Article 25"]);
        assert!(detect_and_split("What does Article 25 say about law and order?", &s).is_none());
    }

    // ── Fix B (2026-06-07): deterministic, title-matched provision number ──────

    // The structural label is prepended to chunk content at index time, so it
    // leads the excerpt. Parse it back out for an un-hallucinatable number.
    #[test]
    fn provision_label_parses_leading_label() {
        assert_eq!(
            provision_label("Section 31 Islamic way of life (1) Steps shall be taken"),
            Some("Section 31".to_string())
        );
        assert_eq!(
            provision_label("Article 25A. The State shall provide free and compulsory"),
            Some("Article 25A".to_string())
        );
        // No leading provision label (e.g. a Preamble chunk) → None.
        assert_eq!(
            provision_label("Preamble CONSTITUTION OF PAKISTAN 6 PART II Fundamental Rights"),
            None
        );
    }

    // FR-006: a year in the heading must never become a provision number.
    #[test]
    fn provision_label_does_not_take_a_year_as_an_ordinal() {
        assert_eq!(
            provision_label("Rule 2017 these rules may be called the SPPRA Rules"),
            None,
            "1800-2099 4-digit number is a year, not 'Rule 2017'"
        );
        assert_eq!(
            provision_label("Section 2010 Development project proposals which exceed"),
            None
        );
        // Real ordinals (incl. 3-digit and sub-lettered) are still parsed.
        assert_eq!(
            provision_label("Article 260 Definitions In the Constitution"),
            Some("Article 260".to_string())
        );
        assert_eq!(
            provision_label("Section 25A. The State shall provide"),
            Some("Section 25A".to_string())
        );
        // A 4-digit number outside the calendar range is not treated as a year.
        assert_eq!(
            provision_label("Rule 1500 some provision"),
            Some("Rule 1500".to_string())
        );
    }

    // The whole point of Fix B: the correct chunk is NOT rank 1. For the topic
    // "Islamic way of life" the retrieved set is [Preamble, Section 31, Section
    // 29]; the resolver must pick Section 31 by TITLE match, not by rank.
    #[test]
    fn resolve_picks_title_matched_label_not_rank_one() {
        let cites = vec![
            cite_ex("c1", "Preamble CONSTITUTION OF PAKISTAN 6 PART II Fundamental Rights and Principles"),
            cite_ex("c2", "Section 31 Islamic way of life (1) Steps shall be taken to enable the Muslims"),
            cite_ex("c3", "Section 29 Principles of Policy (1) The Principles set out in this Chapter"),
        ];
        assert_eq!(
            resolve_provision_label("the Islamic way of life", &cites),
            Some("Section 31".to_string())
        );
    }

    #[test]
    fn resolve_picks_local_government_and_armed_forces() {
        let b = vec![
            cite_ex("c4", "Section 32 Promotion of local Government institutions 32. The State shall"),
            cite_ex("c5", "Section 28 Preservation of language, script and culture Subject to Article"),
            cite_ex("c6", "Section 29 Principles of Policy (1) The Principles set out in this Chapter"),
        ];
        assert_eq!(
            resolve_provision_label("promotion of local Government institutions", &b),
            Some("Section 32".to_string())
        );
        let c = vec![
            cite_ex("c7", "Preamble CONSTITUTION OF PAKISTAN 6 PART II Fundamental Rights and Principles"),
            cite_ex("c8", "Section 39 Participation of people in Armed Forces The State shall enable"),
            cite_ex("c9", "Section 29 Principles of Policy (1) The Principles set out in this Chapter"),
        ];
        assert_eq!(
            resolve_provision_label("participation of people in the Armed Forces", &c),
            Some("Section 39".to_string())
        );
    }

    // Refuse-or-flag: when no retrieved title matches the topic, return None so
    // the caller flags "could not verify" rather than stating a guessed number.
    // The user asked about "articles" but the corpus labels these as "Section N".
    // Render the user's head noun + the original label in brackets so the number
    // is in their terms while traceability to the source is preserved.
    #[test]
    fn render_label_uses_requested_head_with_original_in_brackets() {
        assert_eq!(
            render_label(Some("Article"), "Section 31"),
            "Article 31 (Section 31)"
        );
    }

    #[test]
    fn render_label_no_brackets_when_heads_match() {
        assert_eq!(render_label(Some("Section"), "Section 31"), "Section 31");
        assert_eq!(render_label(Some("section"), "Section 31"), "Section 31");
        assert_eq!(render_label(None, "Section 31"), "Section 31");
    }

    #[test]
    fn requested_head_noun_reads_user_term() {
        assert_eq!(
            requested_head_noun(
                "Identify the Principles of Policy articles dealing with the Islamic way of life"
            ),
            Some("Article".to_string())
        );
        assert_eq!(requested_head_noun("What do the rules say?"), Some("Rule".to_string()));
        assert_eq!(requested_head_noun("Summarize whistleblower protections"), None);
    }

    #[test]
    fn authoritative_body_uses_requested_head_and_verbatim_title() {
        let cites = vec![
            cite_ex("c1", "Preamble CONSTITUTION OF PAKISTAN 6 PART II Fundamental Rights and Principles"),
            cite_ex("c2", "Section 31 Islamic way of life (1) Steps shall be taken to enable the Muslims"),
        ];
        let label = resolve_provision_label("the Islamic way of life", &cites).unwrap();
        let body = authoritative_body(Some("Article"), &label, &cites);
        assert!(body.contains("Article 31 (Section 31)"), "got: {body}");
        assert!(body.to_lowercase().contains("islamic way of life"), "got: {body}");
    }

    #[test]
    fn resolve_refuses_when_no_title_matches() {
        let cites = vec![
            cite_ex("c1", "Preamble CONSTITUTION OF PAKISTAN 6 PART II Fundamental Rights and Principles"),
            cite_ex("c3", "Section 29 Principles of Policy (1) The Principles set out in this Chapter"),
        ];
        assert_eq!(resolve_provision_label("the Islamic way of life", &cites), None);
    }

    // T013 (US2) — fallback to per-article template when clause count != provision count.
    #[test]
    fn multi_article_per_article_template_fallback() {
        let q = "Explain Articles 23 and 24 together.";
        let s = slots_with(&["Article 23", "Article 24"]);
        let subs = detect_and_split(q, &s).expect("should split");
        assert_eq!(subs.len(), 2);
        assert_eq!(subs[0].text, "What does Article 23 provide?");
        assert_eq!(subs[1].text, "What does Article 24 provide?");
    }
}
