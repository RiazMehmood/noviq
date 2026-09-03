//! US9 Move 3 — deterministic stakeholder auto-extraction (FR-016).
//!
//! Tier-1 discipline: regex + a **generic** governance/party role-noun lexicon
//! (no model, no per-document or per-case terms; D2). Stakeholders are drawn
//! from two sources and unioned:
//!   1. the officer's own sentence (the parties they name), and
//!   2. role-holders named inside the retrieved Legal cards (the legally
//!      defined actors — "the competent authority", "the Procurement Committee").
//!
//! This produces *names only*. A stakeholder's powers (Move 4) are asserted
//! elsewhere and only where a cited provision supports them (FR-018) — so an
//! extracted name never implies a fabricated legal status.

use super::state::{OneAskIntent, ProvisionCard};
use regex_lite::Regex;
use std::sync::OnceLock;

/// Generic role / party head-nouns. A candidate phrase qualifies as a
/// stakeholder only if it CONTAINS one of these — so product/object nouns
/// ("Canon", "scanners") are never mistaken for parties. Generic administrative
/// and procurement vocabulary only.
const ROLE_NOUNS: &[&str] = &[
    "authority", "secretary", "director", "minister", "governor", "committee",
    "board", "commission", "council", "department", "officer", "controller",
    "registrar", "agency", "auditor", "accountant", "treasurer", "collector",
    "commissioner", "ombudsman", "chairman", "chairperson", "bidder", "tenderer",
    "contractor", "supplier", "vendor", "distributor", "consultant", "applicant",
    "government", "office", "division", "directorate",
];

/// Optional leading qualifiers kept when they immediately precede a role noun
/// ("Chief" Secretary, "Director" General, "sole" distributor, "competent"
/// authority, "Procurement" Committee). Generic, case-insensitive.
fn role_phrase_re() -> &'static Regex {
    static R: OnceLock<Regex> = OnceLock::new();
    R.get_or_init(|| {
        let nouns = ROLE_NOUNS.join("|");
        // Up to two preceding TitleCase/qualifier words + the role noun, with an
        // optional trailing "general"/"general's" ("Director General").
        let pat = format!(
            r"(?i)\b(?:[A-Z][a-z]+\s+|chief\s+|sole\s+|competent\s+|principal\s+|deputy\s+|additional\s+|provincial\s+|federal\s+){{0,2}}(?:{nouns})(?:\s+general)?\b"
        );
        Regex::new(&pat).expect("role phrase regex compiles")
    })
}

/// A noisy match (e.g. "the Department") is normalised by trimming a leading
/// article and collapsing whitespace; comparison for dedup is case-insensitive.
fn clean(phrase: &str) -> String {
    let p = phrase.trim();
    let p = p.strip_prefix("the ").or_else(|| p.strip_prefix("The ")).unwrap_or(p);
    p.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Extract the stakeholder set: officer-named parties ∪ role-holders named in
/// the supplied Legal cards. Deduped case-insensitively, capped to keep the
/// Background/Legal narrative tight.
pub fn extract_stakeholders(intent: &OneAskIntent, legal_cards: &[&ProvisionCard]) -> Vec<String> {
    const MAX: usize = 6;
    let mut out: Vec<String> = Vec::new();
    let mut seen: std::collections::HashSet<String> = std::collections::HashSet::new();

    let push_from = |text: &str, out: &mut Vec<String>, seen: &mut std::collections::HashSet<String>| {
        for m in role_phrase_re().find_iter(text) {
            let c = clean(m.as_str());
            // A bare head-noun alone ("government", "office") is too generic to
            // be a useful stakeholder unless qualified; keep multi-word phrases
            // and qualified single roles.
            if c.split_whitespace().count() == 1
                && matches!(c.to_lowercase().as_str(), "government" | "office" | "department" | "division")
            {
                continue;
            }
            let key = c.to_lowercase();
            if seen.insert(key) {
                out.push(c);
            }
        }
    };

    // Officer's sentence first (the matter's own parties lead).
    push_from(&intent.raw, &mut out, &mut seen);
    // Then role-holders named in the governing provisions.
    for card in legal_cards {
        push_from(&card.excerpt, &mut out, &mut seen);
        if out.len() >= MAX {
            break;
        }
    }
    out.truncate(MAX);
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::services::drafting::one_ask::skeleton::SectionId;

    fn intent(raw: &str) -> OneAskIntent {
        OneAskIntent { raw: raw.to_string(), figures: vec![], action: None }
    }
    fn card(excerpt: &str) -> ProvisionCard {
        ProvisionCard {
            chunk_id: "c".into(), source_id: "s".into(), source_title: "t".into(),
            excerpt: excerpt.into(), label: None, page: None,
            section_ids: vec![SectionId::LegalValidation],
        }
    }

    #[test]
    fn extracts_officer_parties_and_provision_roleholders_not_products() {
        let i = intent("allow direct contracting with the sole distributor of Canon scanners");
        let c = card("The Director General is the competent authority for direct contracting under this rule.");
        let s = extract_stakeholders(&i, &[&c]);
        let low: Vec<String> = s.iter().map(|x| x.to_lowercase()).collect();
        assert!(low.iter().any(|x| x.contains("distributor")), "officer party missing: {s:?}");
        assert!(low.iter().any(|x| x.contains("director general")), "provision role missing: {s:?}");
        assert!(low.iter().any(|x| x.contains("competent authority")), "competent authority missing: {s:?}");
        // "Canon" / "scanners" are objects, never stakeholders.
        assert!(!low.iter().any(|x| x.contains("canon") || x.contains("scanner")), "product leaked: {s:?}");
    }

    #[test]
    fn dedups_and_returns_empty_when_no_parties() {
        let i = intent("approve the revised policy");
        let s = extract_stakeholders(&i, &[]);
        assert!(s.is_empty(), "no role words → no stakeholders, got {s:?}");
    }

    #[test]
    fn dedups_repeated_roleholder_across_sources() {
        let i = intent("seek approval of the Chief Secretary");
        let c = card("The Chief Secretary shall accord approval.");
        let s = extract_stakeholders(&i, &[&c]);
        let n = s.iter().filter(|x| x.to_lowercase().contains("chief secretary")).count();
        assert_eq!(n, 1, "Chief Secretary should appear once: {s:?}");
    }
}
