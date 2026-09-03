//! LLM call #1 — decompose + facet-expansion (F025, contract: facet-expansion.md).
//!
//! Expands the officer's question into up to 6 distinct sub-questions and ALWAYS
//! prepends the original query as `SubQuestion { id: 0, is_original: true }`
//! (FR-001). Short structured call (small `max_tokens`, free-text JSON). On any
//! failure the loop degrades to just the original leg (FR-012).

use std::sync::OnceLock;

use regex_lite::Regex;
use tokio_util::sync::CancellationToken;

use crate::services::hardware_detector::HardwareTier;
use crate::services::llm::ModelSlot;
use crate::services::query_router::generate_direct;

use super::SubQuestion;

/// Upper bound only — a safety cap, NOT a target. Most questions split into
/// far fewer (often 0–3); a single simple ask splits into none.
const MAX_FACETS: usize = 6;

/// Cap on issue-spotting facets (2026-07-18). Kept small so the highest-value
/// latent issues are covered without ballooning round-1 retrieval/latency.
const ISSUE_MAX: usize = 3;

/// Produce the sub-questions for a deep turn: `[original, facet1, …]`.
pub(crate) async fn expand_query(
    model_slot: &ModelSlot,
    tier: &HardwareTier,
    query_text: &str,
    cancel: Option<CancellationToken>,
) -> Vec<SubQuestion> {
    let mut subs = vec![SubQuestion { id: 0, text: query_text.to_string(), is_original: true }];

    // FAITHFUL decomposition (2026-07-13): split ONLY the parts the user actually
    // asked — no fixed count, no invented facets. The old prompt DEMANDED "4 to 6
    // sub-questions", so a 2-part question got padded with manufactured topics
    // ("appeal", "exceptions") the user never asked, which then pulled off-topic /
    // cross-domain passages. Now: a simple one-part question returns [] (→ single
    // pass); a genuinely multi-part question splits into exactly its stated parts.
    let prompt = format!(
        "You split a government-legal question into ONLY the distinct parts the user ACTUALLY asked. \
         Do NOT add, infer, or invent any topic the user did not explicitly mention (no 'appeal', \
         'exceptions', 'definitions', etc. unless the user wrote them). RULES: \
         (1) if the question asks a single thing, return an EMPTY list — do not split it; \
         (2) split ONLY when the question clearly names several distinct parts (e.g. 'grounds, \
         penalties, and appeal' → those three), one sub-question per stated part, worded in the \
         user's own terms; \
         (3) if the user named a specific instrument (Act/Ordinance/Rules/Schedule), every \
         sub-question must name that SAME instrument and stay on it. \
         Output ONLY JSON, no other text: {{\"sub_questions\":[]}} or \
         {{\"sub_questions\":[\"...\"]}}.\n\nQUESTION: {query_text}\n/no_think"
    );
    let raw = match generate_direct(model_slot, tier, &prompt, 256, cancel, None).await {
        Ok(s) => s,
        Err(_) => return subs, // degrade to the original leg only
    };
    let mut facets: Vec<String> = parse_facets(&raw).into_iter().take(MAX_FACETS).collect();

    // Issue-driven expansion (2026-07-18, flag-gated). Item decomposition above
    // only splits what the user EXPLICITLY listed (faithful decomposition, 3c9037ad
    // — it forbids inferring topics, to avoid the padding regression). That means
    // legal issues IMPLIED by facts buried in the prompt — quorum not maintained,
    // a relative bidding (conflict of interest), a write-off over a sanction limit —
    // never get a sub-question, so their governing rules are never retrieved (prod
    // case-study audit: both normal and deep missed conflict-of-interest + quorum).
    // `spot_issues` adds a sub-question per latent issue ONLY when its trigger fact
    // is actually present in the prompt — the inverse of the padding bug — so it
    // stays faithful. Deduped against the item facets; the coverage check drops any
    // issue leg that retrieves nothing.
    if super::issue_spotting_enabled() {
        let instrument = detect_instrument(query_text);
        for issue in spot_issues(query_text, &instrument) {
            if facets.len() >= MAX_FACETS {
                break;
            }
            let key = issue.to_lowercase();
            if !facets.iter().any(|f| f.to_lowercase() == key) {
                facets.push(issue);
            }
        }
    }

    for (i, facet) in facets.into_iter().enumerate() {
        subs.push(SubQuestion { id: (i + 1) as u8, text: facet, is_original: false });
    }
    subs
}

/// The named instrument for issue templates: an acronym+year ("SLGA 2013"), a
/// "<Name> Act/Ordinance/Rules/Regulations/Policy[, year]", or the Constitution.
/// Falls back to a generic phrase so a template always reads correctly.
fn detect_instrument(query: &str) -> String {
    static ACRONYM: OnceLock<Regex> = OnceLock::new();
    static NAMED: OnceLock<Regex> = OnceLock::new();
    let acronym = ACRONYM.get_or_init(|| Regex::new(r"\b([A-Z]{2,6})\s*,?\s*((?:19|20)\d{2})\b").unwrap());
    let named = NAMED.get_or_init(|| {
        Regex::new(
            r"\b([A-Z][A-Za-z&]+(?:\s+(?:[A-Z][A-Za-z&]+|of|to|and|the|for|on|in|by)){0,6}\s+(?:Act|Ordinance|Rules|Regulations?|Policy)(?:,?\s*(?:19|20)\d{2})?)\b",
        )
        .unwrap()
    });
    if let Some(c) = acronym.captures(query) {
        return format!("the {} {} rules", &c[1], &c[2]);
    }
    if let Some(c) = named.captures(query) {
        return format!("the {}", c[1].trim());
    }
    if query.contains("Constitution") {
        return "the Constitution".to_string();
    }
    "the applicable local-government rules".to_string()
}

/// One latent-issue rule: a trigger regex over the prompt and the sub-question
/// template to emit when it fires (`{instrument}` is substituted).
struct IssueRule {
    trigger: Regex,
    template: &'static str,
}

fn issue_rules() -> &'static Vec<IssueRule> {
    static RULES: OnceLock<Vec<IssueRule>> = OnceLock::new();
    RULES.get_or_init(|| {
        let r = |p: &str| Regex::new(p).unwrap();
        vec![
            IssueRule {
                trigger: r(r"(?i)\b(quorum|members?\s+(?:present|left)|attendance|before\s+the\s+vote|resolution\s+(?:passed|was\s+passed)|monthly\s+meeting)\b"),
                template: "Under {instrument}, what is the required quorum for a valid meeting and must it be maintained at the time of voting?",
            },
            IssueRule {
                trigger: r(r"(?i)\b(relative|brother|son|daughter|spouse|wife|husband|father|mother|related\s+to|interested|conflict\s+of\s+interest|his\s+own)\b"),
                template: "Under {instrument}, what restrictions apply when a member or officer (or their relative) is interested in a council contract, and what is the effect on the contract's validity?",
            },
            IssueRule {
                trigger: r(r"(?i)\b(write[-\s]?off|book\s+value|unserviceable|condemn|sanction\s+limit|competent\s+authority|Rs\.?\s*[0-9,]+)\b"),
                template: "Under {instrument}, what are the monetary limits and the competent sanctioning authority for write-off/condemnation of stores and for sanction of contracts?",
            },
            IssueRule {
                trigger: r(r"(?i)\b(omnibus|single\s+resolution|in\s+one\s+go|validly\s+passed|onward\s+sanction\s+of\s+government|five\s+decisions)\b"),
                template: "Under {instrument}, may several distinct matters be authorised in one (omnibus) resolution, and which matters require separate Government (not Council) sanction?",
            },
        ]
    })
}

/// Deterministic issue-spotting: for each latent legal issue whose trigger fact is
/// ACTUALLY present in `query`, emit an instrument-anchored sub-question. No trigger
/// ⇒ nothing (never speculative — the inverse of the padding bug). Capped at
/// `ISSUE_MAX`.
fn spot_issues(query: &str, instrument: &str) -> Vec<String> {
    let mut out = Vec::new();
    for rule in issue_rules() {
        if rule.trigger.is_match(query) {
            out.push(rule.template.replace("{instrument}", instrument));
            if out.len() >= ISSUE_MAX {
                break;
            }
        }
    }
    out
}

/// Extract the `sub_questions` array from the model's JSON, tolerant of prose /
/// `<think>` wrappers; dedup near-identical facets (case/whitespace-insensitive).
fn parse_facets(raw: &str) -> Vec<String> {
    let (Some(s), Some(e)) = (raw.find('{'), raw.rfind('}')) else {
        return Vec::new();
    };
    let val: serde_json::Value = match serde_json::from_str(&raw[s..=e]) {
        Ok(v) => v,
        Err(_) => return Vec::new(),
    };
    let mut out: Vec<String> = Vec::new();
    let mut seen: std::collections::HashSet<String> = std::collections::HashSet::new();
    if let Some(arr) = val.get("sub_questions").and_then(|v| v.as_array()) {
        for item in arr {
            if let Some(t) = item.as_str() {
                let t = t.trim();
                if !t.is_empty() && seen.insert(t.to_lowercase()) {
                    out.push(t.to_string());
                }
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_facets_extracts_and_dedups() {
        let raw = "<think>reasoning</think>\n{\"sub_questions\":[\"grounds\",\"Grounds \",\"penalties\",\"\"]}";
        assert_eq!(parse_facets(raw), vec!["grounds".to_string(), "penalties".to_string()]);
    }

    #[test]
    fn parse_facets_empty_on_garbage() {
        assert!(parse_facets("no json here").is_empty());
        assert!(parse_facets("{\"other\":[1,2]}").is_empty());
    }

    #[test]
    fn parse_facets_empty_list_for_simple_question() {
        // Faithful decomposition: a single-part question returns an empty list, so
        // the loop runs a single pass on the original query (no invented facets).
        assert!(parse_facets("{\"sub_questions\":[]}").is_empty());
    }

    // ── issue-driven facet expansion ─────────────────────────────────────────

    const CASE_STUDY: &str = "Examine under the rules framed under the SLGA 2013: lease of a plot with \
        the sanction of the Council; sale of a building to the adjoining owner; award of parking-fee \
        collection rights by negotiation; write-off by the Chairman of unserviceable stores whose book \
        value is Rs. 150,000. The meeting had one-third of members present at the start but several \
        members left before the vote, and one bidder is a relative of the Chairman.";

    #[test]
    fn spot_issues_flags_quorum_and_conflict_of_interest_for_the_case_study() {
        let issues = spot_issues(CASE_STUDY, "the SLGA 2013 rules");
        let joined = issues.join(" | ").to_lowercase();
        assert!(joined.contains("quorum"), "quorum issue spotted: {joined}");
        assert!(joined.contains("interested"), "conflict-of-interest issue spotted: {joined}");
        assert!(issues.len() <= ISSUE_MAX, "respects the cap");
        assert!(issues.iter().all(|s| s.contains("the SLGA 2013 rules")), "instrument anchored: {joined}");
    }

    #[test]
    fn spot_issues_is_empty_without_trigger_facts() {
        // A single-topic legal question with none of the latent-issue trigger facts
        // → no issue facets (anti-regression: never speculative).
        assert!(spot_issues("What are the grounds for penalty under the E&D Rules?", "the E&D Rules").is_empty());
    }

    #[test]
    fn detect_instrument_reads_acronym_named_and_falls_back() {
        assert_eq!(detect_instrument("under the SLGA 2013 rules"), "the SLGA 2013 rules");
        assert_eq!(
            detect_instrument("under the Sindh Local Government Act 2013"),
            "the Sindh Local Government Act 2013"
        );
        assert_eq!(detect_instrument("who can sanction leave?"), "the applicable local-government rules");
    }
}
