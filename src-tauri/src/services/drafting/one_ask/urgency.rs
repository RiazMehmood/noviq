//! US9 Move 1 — factual urgency detection (FR-015).
//!
//! Tier-1 discipline (constitution II): regex only, no model. Urgency is
//! **detected-and-surfaced** from the officer's own words — a verbatim
//! time/consequence span — never generated. Returns `None` when the officer
//! states no time-bound fact, so the Background section never manufactures
//! urgency or slips into advocacy (FR-008/FR-015). Generic temporal vocabulary
//! only — no per-document or per-case terms (D2).

use super::state::OneAskIntent;
use regex_lite::Regex;
use std::sync::OnceLock;

/// Patterns that capture a *factual* time/consequence span in the officer's own
/// sentence. Each is anchored on a concrete temporal unit or a lapse/expiry cue
/// so generic phrases ("within the rules", "within the powers") do NOT match.
fn urgency_patterns() -> &'static Vec<Regex> {
    static P: OnceLock<Vec<Regex>> = OnceLock::new();
    P.get_or_init(|| {
        [
            // "within one month", "in 30 days", "by next quarter", "within the
            // coming two weeks" — a period anchored on a real time unit.
            r"(?i)\b(?:with)?in\s+(?:the\s+)?(?:next\s+|coming\s+|end\s+of\s+)?(?:\d+|one|two|three|four|five|six|seven|eight|nine|ten|a|few|couple\s+of)\s+(?:day|days|week|weeks|fortnight|month|months|quarter|year|years)\b",
            // "by/before 30 June", "by end of June", "before the 15th of March"
            r"(?i)\b(?:by|before)\s+(?:the\s+)?(?:end\s+of\s+)?(?:\d{1,2}(?:st|nd|rd|th)?\s+)?(?:of\s+)?(?:january|february|march|april|may|june|july|august|september|october|november|december)\b",
            // "launching within …", "expires/lapses on …", financial-year close
            r"(?i)\b(?:current\s+)?financial\s+year(?:\s+clos\w+|\s+end\w*)?\b",
            r"(?i)\blaps\w+\b|\bexpir\w+\b|\bdeadline\b|\btime[- ]?bound\b",
        ]
        .iter()
        .map(|p| Regex::new(p).expect("urgency regex compiles"))
        .collect()
    })
}

/// Detect a factual, time-bound urgency span in the officer's sentence. Returns
/// the **verbatim** matched span (so the Background prompt can state it plainly,
/// not embellish it), or `None` when no such fact is present.
pub fn detect_urgency(intent: &OneAskIntent) -> Option<String> {
    let raw = &intent.raw;
    let mut best: Option<(usize, usize)> = None;
    for re in urgency_patterns() {
        if let Some(m) = re.find(raw) {
            // Prefer the earliest, longest concrete span.
            let span = (m.start(), m.end());
            best = match best {
                Some((s, e)) if (e - s) >= (span.1 - span.0) && s <= span.0 => Some((s, e)),
                _ => Some(span),
            };
        }
    }
    best.map(|(s, e)| raw[s..e].trim().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn intent(raw: &str) -> OneAskIntent {
        OneAskIntent { raw: raw.to_string(), figures: vec![], action: None }
    }

    #[test]
    fn surfaces_within_a_period_verbatim() {
        let u = detect_urgency(&intent(
            "allow direct contracting for scanners to be used in e-Daftar, as e-Daftar is launching within one month",
        ));
        assert_eq!(u.as_deref(), Some("within one month"));
    }

    #[test]
    fn surfaces_by_a_month_deadline() {
        let u = detect_urgency(&intent("sanction funds before 30 June for the scheme"));
        assert!(u.is_some(), "expected a deadline span, got {u:?}");
        assert!(u.unwrap().to_lowercase().contains("june"));
    }

    #[test]
    fn surfaces_financial_year_close() {
        let u = detect_urgency(&intent("allocate the balance before the current financial year closes"));
        assert!(u.is_some());
    }

    #[test]
    fn omits_when_no_time_fact_present() {
        assert_eq!(detect_urgency(&intent("approve revised service rules for the department")), None);
    }

    #[test]
    fn does_not_false_match_within_the_rules() {
        // "within" not anchored on a time unit → not urgency (no fabrication).
        assert_eq!(detect_urgency(&intent("procure within the rules of business")), None);
        assert_eq!(detect_urgency(&intent("act within the powers delegated to the authority")), None);
    }
}
