//! US10 — deterministic advocacy strip (FR-021).
//!
//! The 7B writer ignores "no advocacy", so we enforce it AFTER generation:
//! `strip_advocacy` removes a *generic* pleading/embellishment lexicon (no
//! per-document or per-case terms; D2) — the padding observed live ("it is
//! crucial… promptly… would significantly contribute…").
//!
//! Brevity is achieved by this strip + one-move-per-paragraph structure, NOT by
//! truncation: a blind sentence cap could cut a sentence carrying the governing
//! rule + its citation (a citation-grade violation), so we never truncate
//! (FR-020). This only ever operates on the writer's PROSE, never on a spliced
//! citation quote (those are separate runs and pass through untouched).

use regex_lite::Regex;
use std::sync::OnceLock;

/// Generic pleading / advocacy / puffery phrases to delete. Each entry is a
/// case-insensitive phrase; deletion leaves surrounding facts intact. Generic
/// administrative English only.
const ADVOCACY_PHRASES: &[&str] = &[
    // pleading
    "it is requested that", "it is requested", "we humbly request", "we request that",
    "we request", "we humbly", "we kindly", "kindly", "we await", "we appreciate",
    "it is solicited that",
    // urgency-pleading
    "it is crucial that", "it is crucial to", "it is crucial", "it is imperative that",
    "it is imperative to", "it is imperative", "it is essential that", "it is essential to",
    "it is essential", "as soon as possible", "without delay", "at the earliest",
    "swift action", "prompt action", "swiftly", "promptly", "expeditiously", "expedite",
    // puffery
    "of utmost importance", "utmost importance", "significantly contribute towards",
    "significantly contribute to", "significantly contribute", "significantly contributes",
    "thereby ensuring", "thereby enhancing", "seamless", "streamlined", "smooth transition",
    "in the best interest", "for the smooth", "uninterrupted service delivery",
];

fn advocacy_re() -> &'static Regex {
    static R: OnceLock<Regex> = OnceLock::new();
    R.get_or_init(|| {
        // Longest phrases first so a contained shorter phrase doesn't pre-empt
        // the longer match; word-boundary anchored.
        let mut phrases: Vec<&str> = ADVOCACY_PHRASES.to_vec();
        phrases.sort_by_key(|p| std::cmp::Reverse(p.len()));
        let alt = phrases
            .iter()
            .map(|p| regex_escape(p))
            .collect::<Vec<_>>()
            .join("|");
        Regex::new(&format!(r"(?i)\b(?:{alt})\b")).expect("advocacy regex compiles")
    })
}

fn regex_escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        if "\\.+*?()|[]{}^$".contains(c) {
            out.push('\\');
        }
        out.push(c);
    }
    out
}

/// Tidy the seams a deletion leaves behind: a space before punctuation, an
/// empty clause (" , "), and doubled spaces. We deliberately do NOT strip a
/// leading conjunction/comma — prose runs are FRAGMENTS that legitimately begin
/// with ", " (e.g. the text after an inline citation), and removing it corrupts
/// them.
fn tidy_re() -> &'static [(Regex, &'static str)] {
    static R: OnceLock<Vec<(Regex, &'static str)>> = OnceLock::new();
    R.get_or_init(|| {
        vec![
            (Regex::new(r"\s+([,.;:])").unwrap(), "$1"),
            (Regex::new(r",\s*,").unwrap(), ","),
            (Regex::new(r"\s{2,}").unwrap(), " "),
        ]
    })
}

/// Remove generic advocacy/pleading phrases from a prose string (FR-021).
/// Verbatim citations are not prose, so this never touches a quoted span.
pub fn strip_advocacy(text: &str) -> String {
    let stripped = advocacy_re().replace_all(text, " ");
    let mut out = stripped.into_owned();
    for (re, rep) in tidy_re() {
        out = re.replace_all(&out, *rep).into_owned();
    }
    out.trim().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strips_pleading_and_puffery_keeps_facts() {
        let s = "It is crucial that this approval be granted promptly to ensure efficiency.";
        let out = strip_advocacy(s);
        let low = out.to_lowercase();
        assert!(!low.contains("it is crucial"), "advocacy remains: {out}");
        assert!(!low.contains("promptly"), "advocacy remains: {out}");
        assert!(low.contains("approval"), "fact lost: {out}");
    }

    #[test]
    fn strip_is_noop_on_neutral_factual_text() {
        let s = "The competent authority may sanction the expenditure under the rule.";
        assert_eq!(strip_advocacy(s), s);
    }

    #[test]
    fn strip_keeps_governing_rule_and_numbers_intact() {
        // The strip must never touch a rule reference or a figure — only padding.
        let s = "It is imperative that Rule 16(1)(b) be applied for Rs. 2 million.";
        let out = strip_advocacy(s);
        assert!(out.contains("Rule 16(1)(b)"), "rule lost: {out}");
        assert!(out.contains("Rs. 2 million"), "figure lost: {out}");
        assert!(!out.to_lowercase().contains("imperative"), "advocacy remains: {out}");
    }
}
