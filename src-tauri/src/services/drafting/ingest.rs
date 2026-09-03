//! Feature 009 — template ingestion, Phase D rewrite (FR-010/010a/010b, ADR-0012).
//!
//! Option C: a local model (Tier-4 worker, Mistral-7B free-text JSON — NO
//! llguidance, that path hard-aborts) PROPOSES the segmentation of an uploaded
//! format into regions; a deterministic **verbatim-verify gate** is the sole
//! authority over content. Every model-proposed slot must pass:
//!   (a) its `text` is a verbatim substring of the source (whitespace-normalized);
//!   (b) a cited slot's `provision` appears verbatim AND carries a real provision
//!       number (hyphen-tolerant: `Rule-6(c)`, `Regulations-2023`);
//!   (c) the cited slot's `required_source_category` derives from the MATCHED
//!       provision, never from heading tokens;
//!   (d) structural boilerplate (letterhead/dateline/C.C./signature) can never
//!       become a cited slot — it carries no verifiable provision number.
//! A slot that fails the gate is RETAINED in the proposal, marked low-confidence
//! with the rejection reason, for the officer to classify manually (FR-010a) —
//! never silently dropped or silently accepted.
//!
//! The old heading-heuristic segmentation is REMOVED (it produced garbage slots
//! on real numbered-paragraph instruments — see research R5/R5a). When the
//! segmentation model is unavailable the command layer refuses with an actionable
//! error (FR-010b); there is no heuristic fallback here.
//!
//! Everything in this module is pure and deterministically testable; the live
//! model call lives in `commands::drafting::ingest_template`.

use std::collections::HashMap;

use super::slot::{Slot, SlotKind};
use regex_lite::Regex;
use serde::{Deserialize, Serialize};
use std::sync::OnceLock;

/// A proposed (un-confirmed) slot schema returned to the officer for review.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProposedSchema {
    pub name: Option<String>,
    pub instrument_type: String,
    pub slots: Vec<Slot>,
    /// Orderings of slots whose classification was low-confidence (gate-rejected
    /// or otherwise ambiguous) — surfaced so the officer re-tags them (FR-010a).
    pub flagged_for_review: Vec<i64>,
    /// Gate rejection reason per flagged slot ordering (FR-010a).
    #[serde(default)]
    pub rejection_reasons: HashMap<i64, String>,
    /// If this was a filled example, the on-disk ref to stage as gold on confirm.
    pub gold_example_ref: Option<String>,
}

/// One region as proposed by the segmentation model (free-text JSON, tolerant).
/// The 7B emits list-like regions (C.C. blocks, multi-line paragraphs) as JSON
/// arrays of strings — accept string OR array, joining with newlines, so the
/// verbatim-verify gate still sees the exact source lines.
#[derive(Debug, Clone, Deserialize)]
pub struct ModelProposedSlot {
    pub kind: String,
    #[serde(default, deserialize_with = "string_or_lines")]
    pub label: Option<String>,
    #[serde(default, deserialize_with = "string_or_lines")]
    pub provision: Option<String>,
    #[serde(default, deserialize_with = "string_or_lines")]
    pub text: Option<String>,
}

fn string_or_lines<'de, D>(de: D) -> Result<Option<String>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum StrOrList {
        One(String),
        Many(Vec<String>),
    }
    Ok(Option::<StrOrList>::deserialize(de)?.map(|v| match v {
        StrOrList::One(s) => s,
        StrOrList::Many(lines) => lines.join("\n"),
    }))
}

// ── Segmentation prompt (Mistral [INST], proven in the 2026-06-10 spike) ──────

/// The slot-segmentation task. The model labels STRUCTURE only — it must copy
/// any rule/section reference verbatim into `provision` and never invent a
/// number; the gate below verifies every byte against the source regardless.
const SEGMENTATION_TASK: &str = "You segment an official government document into labelled parts for a template. \
Output ONLY a JSON array (no prose, no explanation). Each array item is an object: \
{\"kind\": one of \"boilerplate\"|\"cited\"|\"fact\"|\"prose\", \"label\": a short name, \
\"provision\": the exact law/rule/section/regulation/notification reference if kind is \"cited\", else null, \
\"text\": the verbatim text of that part}. \
Definitions: \"cited\" = the part references a law/rule/regulation/section/notification (copy the reference EXACTLY as written into \"provision\"; never invent or guess a number); \
\"fact\" = case-specific facts (applicant name, dates, amounts, enclosures, cabinet decisions); \
\"boilerplate\" = reference number, letterhead, addressee, signatory, distribution list (C.C.); \
\"prose\" = connective narrative with no citation.";

/// Build the Mistral `[INST]` segmentation prompt for an extracted template text.
pub fn build_segmentation_prompt(text: &str) -> String {
    format!("[INST] {SEGMENTATION_TASK}\n\nDocument:\n\n{text}\n\nReturn ONLY the JSON array. [/INST]")
}

// ── Tolerant JSON parse (free-text model output, F006 lenient-parse pattern) ──

/// Parse the model's raw output into proposed slots. Tolerates fenced code
/// blocks, leading/trailing prose, literal newlines inside strings, and trailing
/// commas. Unparseable output after the lenient chain is an `Err` (the command
/// layer retries, then refuses — FR-010b) — never a partial schema.
pub fn parse_model_slots(raw: &str) -> Result<Vec<ModelProposedSlot>, String> {
    let start = raw
        .find('[')
        .ok_or_else(|| "model output contains no JSON array".to_string())?;
    let end = raw
        .rfind(']')
        .filter(|&e| e > start)
        .ok_or_else(|| "model output contains no closing ']'".to_string())?;
    let candidate = &raw[start..=end];

    // 1) strict
    if let Ok(slots) = serde_json::from_str::<Vec<ModelProposedSlot>>(candidate) {
        return validate_proposals(slots);
    }
    // 2) escape raw control characters inside string literals
    let escaped = escape_control_chars_in_strings(candidate);
    if let Ok(slots) = serde_json::from_str::<Vec<ModelProposedSlot>>(&escaped) {
        return validate_proposals(slots);
    }
    // 3) additionally strip trailing commas before } / ]
    let stripped = strip_trailing_commas(&escaped);
    serde_json::from_str::<Vec<ModelProposedSlot>>(&stripped)
        .map_err(|e| format!("model output is not a parseable slot array: {e}"))
        .and_then(validate_proposals)
}

fn validate_proposals(slots: Vec<ModelProposedSlot>) -> Result<Vec<ModelProposedSlot>, String> {
    if slots.is_empty() {
        return Err("model output parsed to an empty slot array".to_string());
    }
    Ok(slots)
}

/// Escape literal control characters (\n, \r, \t) occurring INSIDE JSON string
/// literals — a frequent free-text model failure mode.
fn escape_control_chars_in_strings(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 16);
    let mut in_string = false;
    let mut escaped = false;
    for c in s.chars() {
        if in_string {
            if escaped {
                out.push(c);
                escaped = false;
                continue;
            }
            match c {
                '\\' => {
                    out.push(c);
                    escaped = true;
                }
                '"' => {
                    out.push(c);
                    in_string = false;
                }
                '\n' => out.push_str("\\n"),
                '\r' => out.push_str("\\r"),
                '\t' => out.push_str("\\t"),
                _ => out.push(c),
            }
        } else {
            if c == '"' {
                in_string = true;
            }
            out.push(c);
        }
    }
    out
}

/// Remove trailing commas before a closing `]`/`}` (outside strings).
fn strip_trailing_commas(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut in_string = false;
    let mut escaped = false;
    let chars: Vec<char> = s.chars().collect();
    for (i, &c) in chars.iter().enumerate() {
        if in_string {
            out.push(c);
            if escaped {
                escaped = false;
            } else if c == '\\' {
                escaped = true;
            } else if c == '"' {
                in_string = false;
            }
            continue;
        }
        if c == '"' {
            in_string = true;
            out.push(c);
            continue;
        }
        if c == ',' {
            // Lookahead: is the next non-whitespace char a closer?
            let next = chars[i + 1..].iter().find(|ch| !ch.is_whitespace());
            if matches!(next, Some(']') | Some('}')) {
                continue; // drop the trailing comma
            }
        }
        out.push(c);
    }
    out
}

// ── The deterministic verbatim-verify gate (FR-010a, ADR-0012) ────────────────

/// Collapse all whitespace runs to single spaces (the only normalization the
/// gate permits — verbatim means verbatim).
fn normalize_ws(s: &str) -> String {
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// True if `needle` (normalized) appears verbatim in the normalized source.
fn verbatim_in(source_norm: &str, needle: &str) -> bool {
    let n = normalize_ws(needle);
    !n.is_empty() && source_norm.contains(&n)
}

/// Hyphen-tolerant provision-number reference: a provision noun joined to a
/// number by whitespace OR hyphens (`Rule 6`, `Rule-6(c)`, `Regulations-2023`,
/// `Section 31-A`), optional sub-clause. Generic legal vocabulary only.
fn provision_number_re() -> &'static Regex {
    static R: OnceLock<Regex> = OnceLock::new();
    R.get_or_init(|| {
        Regex::new(
            r"(?i)\b(articles?|sections?|rules?|clauses?|regulations?|notifications?)[-\s]+(no\.?\s*)?\d+[A-Za-z]?(\([a-z0-9]{1,4}\))?",
        )
        .unwrap()
    })
}

/// True if the text carries a verifiable provision-number reference.
pub(crate) fn contains_provision_number(text: &str) -> bool {
    provision_number_re().is_match(text)
}

/// The category for a cited slot: derived from the MATCHED provision reference
/// (never heading tokens) — lowercased, whitespace-normalized (FR-010/R5a).
fn category_from_provision(provision: &str) -> String {
    normalize_ws(provision).to_ascii_lowercase()
}

/// Bounded connective-narrative instruction for a prose slot.
fn derive_guidance(label: &Option<String>) -> String {
    match label {
        Some(h) if h.chars().any(|c| c.is_alphabetic()) => format!(
            "Write the \"{}\" section as connective narrative for the addressee; \
             do not introduce any rule, article, section, clause, or figure.",
            h.trim_end_matches(':').trim()
        ),
        _ => "Write this section as connective narrative; do not introduce any \
              rule, article, section, clause, or figure."
            .to_string(),
    }
}

const STOPWORDS: &[&str] = &[
    "the", "of", "for", "and", "to", "a", "an", "in", "on", "by", "with", "under",
    "as", "is", "are", "be", "this", "that", "shall", "from", "or", "its", "at",
    "summary", "note", "section",
];

fn salient_tokens(s: &str) -> Vec<String> {
    s.split(|c: char| !c.is_alphanumeric())
        .map(|t| t.to_ascii_lowercase())
        .filter(|t| t.len() >= 3 && !t.chars().all(|c| c.is_ascii_digit()))
        .filter(|t| !STOPWORDS.contains(&t.as_str()))
        .collect()
}

/// Fallback category when the gate rejects a cited proposal (keeps the slot
/// valid so it can be surfaced for officer review instead of dropped).
fn fallback_category(label: &Option<String>, text: &str) -> String {
    let source = label.as_deref().filter(|l| !l.trim().is_empty()).unwrap_or(text);
    let toks: Vec<String> = salient_tokens(source).into_iter().take(5).collect();
    if toks.is_empty() {
        "unspecified source".to_string()
    } else {
        toks.join(" ")
    }
}

/// Verify the model's proposals against the source and assemble the proposed
/// schema (the ONE pure entry point — fed by the command layer's model call,
/// and by tests with canned model output).
pub fn propose_schema_from_model(
    source_text: &str,
    raw_model_output: &str,
    instrument_type: &str,
) -> Result<ProposedSchema, String> {
    let proposals = parse_model_slots(raw_model_output)?;
    let source_norm = normalize_ws(source_text);

    let mut slots = Vec::with_capacity(proposals.len());
    let mut flagged = Vec::new();
    let mut reasons: HashMap<i64, String> = HashMap::new();

    for (i, p) in proposals.iter().enumerate() {
        let ordering = i as i64;
        let text = p.text.clone().unwrap_or_default();
        let label = p.label.clone();

        // Gate (a): the region text must be verbatim in the source (all kinds).
        let mut reason: Option<String> = if text.trim().is_empty() {
            Some("model returned no text for this region".to_string())
        } else if !verbatim_in(&source_norm, &text) {
            Some("region text is not a verbatim substring of the source".to_string())
        } else {
            None
        };

        let slot = match p.kind.as_str() {
            "cited" => {
                // Gates (b)+(c): provision must exist, be verbatim, and carry a
                // real provision number; category derives from the provision.
                // Structural boilerplate (letterhead/dateline/C.C./signature)
                // carries no provision number, so it can never pass (gate d).
                let provision_ok = match p.provision.as_deref().map(str::trim) {
                    None | Some("") => {
                        reason.get_or_insert(
                            "cited slot has no provision reference".to_string(),
                        );
                        None
                    }
                    Some(prov) if !verbatim_in(&source_norm, prov) => {
                        reason.get_or_insert(format!(
                            "provision \"{prov}\" is not a verbatim substring of the source"
                        ));
                        None
                    }
                    Some(prov) if !contains_provision_number(prov) => {
                        reason.get_or_insert(format!(
                            "provision \"{prov}\" carries no verifiable provision number"
                        ));
                        None
                    }
                    Some(prov) => Some(prov.to_string()),
                };
                match (provision_ok, &reason) {
                    (Some(prov), None) => Slot {
                        ordering,
                        kind: SlotKind::Cited,
                        label,
                        boilerplate_text: None,
                        required_source_category: Some(category_from_provision(&prov)),
                        prose_guidance: None,
                        provision_ref: Some(prov),
                    },
                    _ => Slot {
                        ordering,
                        kind: SlotKind::Cited,
                        label: label.clone(),
                        boilerplate_text: None,
                        required_source_category: Some(fallback_category(&label, &text)),
                        prose_guidance: None,
                        provision_ref: None,
                    },
                }
            }
            // Case-specific facts → the existing `fact:` cited convention
            // (extractive from case docs; officer-supplied channel on gaps).
            "fact" => Slot {
                ordering,
                kind: SlotKind::Cited,
                label: label.clone(),
                boilerplate_text: None,
                required_source_category: Some(format!(
                    "fact: {}",
                    fallback_category(&label, &text)
                )),
                prose_guidance: None,
                provision_ref: None,
            },
            "boilerplate" => Slot {
                ordering,
                kind: SlotKind::Boilerplate,
                label: label.clone(),
                boilerplate_text: Some(if text.trim().is_empty() {
                    label.clone().unwrap_or_else(|| "(empty region)".to_string())
                } else {
                    text.clone()
                }),
                required_source_category: None,
                prose_guidance: None,
                provision_ref: None,
            },
            "prose" => Slot {
                ordering,
                kind: SlotKind::Prose,
                label: label.clone(),
                boilerplate_text: None,
                required_source_category: None,
                prose_guidance: Some(derive_guidance(&label)),
                provision_ref: None,
            },
            other => {
                reason.get_or_insert(format!("unknown region kind \"{other}\""));
                Slot {
                    ordering,
                    kind: SlotKind::Prose,
                    label: label.clone(),
                    boilerplate_text: None,
                    required_source_category: None,
                    prose_guidance: Some(derive_guidance(&label)),
                    provision_ref: None,
                }
            }
        };

        // FR-010a: a gate-rejected slot is retained, flagged, with its reason —
        // never silently dropped or silently accepted.
        if let Some(r) = reason {
            flagged.push(ordering);
            reasons.insert(ordering, r);
        }
        slots.push(slot);
    }

    Ok(ProposedSchema {
        name: None,
        instrument_type: instrument_type.to_string(),
        slots,
        flagged_for_review: flagged,
        rejection_reasons: reasons,
        gold_example_ref: None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const SOURCE: &str = "\
GOVERNMENT OF SINDH
(REGULATIONS WING)

NO.RW/1-23/2025: Karachi, dated 14th November, 2025

2. Under Rule-6(c) of the XYZ Regulations-2023, the competent authority may relax the upper age limit.

3. The Law Department has furnished the opinion that the conditions in Rule-23 of the ABC Rules 2020 are satisfied.
";

    fn proposal(kind: &str, label: &str, provision: Option<&str>, text: &str) -> String {
        serde_json::to_string(&serde_json::json!([{
            "kind": kind, "label": label, "provision": provision, "text": text
        }]))
        .unwrap()
    }

    // ── T066: gate behaviour ──────────────────────────────────────────────────

    #[test]
    fn verbatim_text_passes_paraphrase_fails_with_reason() {
        let verbatim = proposal(
            "prose", "background", None,
            "The Law Department has furnished the opinion",
        );
        let p = propose_schema_from_model(SOURCE, &verbatim, "summary").unwrap();
        assert!(p.flagged_for_review.is_empty(), "verbatim text must pass: {:?}", p.rejection_reasons);

        let paraphrased = proposal(
            "prose", "background", None,
            "The Law Department gave its view on the matter",
        );
        let p = propose_schema_from_model(SOURCE, &paraphrased, "summary").unwrap();
        assert_eq!(p.flagged_for_review, vec![0]);
        assert!(p.rejection_reasons[&0].contains("not a verbatim substring"));
        // Slot retained, never dropped (FR-010a).
        assert_eq!(p.slots.len(), 1);
    }

    #[test]
    fn hyphenated_provisions_are_detected_and_matched_verbatim() {
        let raw = proposal(
            "cited", "legal position",
            Some("Rule-6(c) of the XYZ Regulations-2023"),
            "Under Rule-6(c) of the XYZ Regulations-2023, the competent authority may relax the upper age limit.",
        );
        let p = propose_schema_from_model(SOURCE, &raw, "summary").unwrap();
        assert!(p.flagged_for_review.is_empty(), "{:?}", p.rejection_reasons);
        let s = &p.slots[0];
        assert_eq!(s.kind, SlotKind::Cited);
        assert_eq!(s.provision_ref.as_deref(), Some("Rule-6(c) of the XYZ Regulations-2023"));
        // Plain-space form still matches too.
        assert!(contains_provision_number("Rule-23 of the ABC Rules 2020"));
        assert!(contains_provision_number("Rule 23 of the rules"));
    }

    #[test]
    fn structural_boilerplate_never_becomes_a_cited_slot() {
        // The model mislabels the letterhead / dateline / C.C. as cited — the
        // gate rejects each one (no verifiable provision number).
        for (label, text, prov) in [
            ("letterhead", "(REGULATIONS WING)", Some("REGULATIONS WING")),
            ("dateline", "Karachi, dated 14th November, 2025", Some("dated 14th November")),
            ("cc", "GOVERNMENT OF SINDH", None),
        ] {
            let raw = proposal("cited", label, prov, text);
            let p = propose_schema_from_model(SOURCE, &raw, "summary").unwrap();
            assert_eq!(p.flagged_for_review, vec![0], "{label} must be rejected as cited");
            assert!(p.slots[0].provision_ref.is_none(), "{label} must carry no provision_ref");
        }
    }

    #[test]
    fn category_derives_from_matched_provision_not_heading() {
        let raw = proposal(
            "cited", "SOME UNRELATED HEADING TOKENS",
            Some("Rule-23 of the ABC Rules 2020"),
            "The Law Department has furnished the opinion that the conditions in Rule-23 of the ABC Rules 2020 are satisfied.",
        );
        let p = propose_schema_from_model(SOURCE, &raw, "summary").unwrap();
        let cat = p.slots[0].required_source_category.as_deref().unwrap();
        assert_eq!(cat, "rule-23 of the abc rules 2020");
        assert!(!cat.contains("unrelated"), "heading tokens must not leak: {cat}");
    }

    #[test]
    fn rejected_cited_slot_is_flagged_with_reason_and_kept_valid() {
        // Provision invented by the model (not in source) → rejected, retained.
        let raw = proposal(
            "cited", "legal position",
            Some("Article 99 of the Imaginary Act 1900"),
            "Under Rule-6(c) of the XYZ Regulations-2023, the competent authority may relax the upper age limit.",
        );
        let p = propose_schema_from_model(SOURCE, &raw, "summary").unwrap();
        assert_eq!(p.flagged_for_review, vec![0]);
        assert!(p.rejection_reasons[&0].contains("not a verbatim substring"));
        let s = &p.slots[0];
        assert!(s.provision_ref.is_none());
        assert!(s.validate().is_ok(), "flagged slot must stay valid for review");
    }

    // ── T067: tolerant parse ──────────────────────────────────────────────────

    const GOOD_ITEM: &str = r#"{"kind":"prose","label":"x","provision":null,"text":"t"}"#;

    #[test]
    fn parse_tolerates_fences_prose_trailing_commas_and_newlines() {
        // fenced + leading prose
        let fenced = format!("Here is the segmentation:\n```json\n[{GOOD_ITEM}]\n```\nDone.");
        assert_eq!(parse_model_slots(&fenced).unwrap().len(), 1);
        // trailing comma
        let trailing = format!("[{GOOD_ITEM},]");
        assert_eq!(parse_model_slots(&trailing).unwrap().len(), 1);
        // literal newline inside a string
        let newline = "[{\"kind\":\"prose\",\"label\":\"x\",\"provision\":null,\"text\":\"line one\nline two\"}]";
        let slots = parse_model_slots(newline).unwrap();
        assert!(slots[0].text.as_deref().unwrap().contains("line one"));
    }

    #[test]
    fn parse_tolerates_array_valued_string_fields() {
        // Verbatim failure shape from the 2026-06-10 spike (mistral7b_freetext.output
        // line 1273): the 7B emits list-like regions as JSON arrays of strings.
        let raw = r#"[
            {"kind": "boilerplate", "label": "distribution_list", "provision": ["C.C to:-", "1. The Section Officer (opinion) Law Deptt: GoS, Karachi", "2. The Section Officer (S-II) SGA&CD, GoS, Karachi."], "text": null},
            {"kind": "cited", "label": "legal", "provision": "Rule-6(c)", "text": ["Under Rule-6(c) of the XYZ Regulations-2023,", "the competent authority may relax the upper age limit."]}
        ]"#;
        let slots = parse_model_slots(raw).unwrap();
        assert_eq!(slots.len(), 2);
        let prov = slots[0].provision.as_deref().unwrap();
        assert!(prov.starts_with("C.C to:-\n1. The Section Officer"));
        let text = slots[1].text.as_deref().unwrap();
        assert_eq!(
            text,
            "Under Rule-6(c) of the XYZ Regulations-2023,\nthe competent authority may relax the upper age limit."
        );
    }

    #[test]
    fn unparseable_output_is_an_error_not_a_partial_schema() {
        assert!(parse_model_slots("I could not segment this document.").is_err());
        assert!(parse_model_slots("[{borked json").is_err());
        assert!(parse_model_slots("[]").is_err(), "empty array is a failure");
    }

    #[test]
    fn proposed_slots_validate_and_keep_model_order() {
        let raw = format!(
            "[{},{},{}]",
            r#"{"kind":"boilerplate","label":"head","provision":null,"text":"GOVERNMENT OF SINDH"}"#,
            r#"{"kind":"cited","label":"legal","provision":"Rule-6(c) of the XYZ Regulations-2023","text":"Under Rule-6(c) of the XYZ Regulations-2023, the competent authority may relax the upper age limit."}"#,
            r#"{"kind":"fact","label":"affected candidates","provision":null,"text":"GOVERNMENT OF SINDH"}"#,
        );
        let p = propose_schema_from_model(SOURCE, &raw, "summary").unwrap();
        assert_eq!(p.slots.len(), 3);
        for (i, s) in p.slots.iter().enumerate() {
            assert_eq!(s.ordering, i as i64);
            assert!(s.validate().is_ok(), "slot {i} invalid: {s:?}");
        }
        assert_eq!(p.slots[0].kind, SlotKind::Boilerplate);
        assert_eq!(p.slots[1].kind, SlotKind::Cited);
        // fact → cited slot with the `fact:` category convention
        assert!(p.slots[2]
            .required_source_category
            .as_deref()
            .unwrap()
            .starts_with("fact:"));
    }
}
