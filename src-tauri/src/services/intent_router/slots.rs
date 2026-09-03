//! Slot extractor (T026). Regex-only, microsecond budget.

use super::types::Slots;
use regex_lite::Regex;
use std::sync::OnceLock;

fn section_path_re() -> &'static Regex {
    static R: OnceLock<Regex> = OnceLock::new();
    R.get_or_init(|| {
        // The head noun is case-insensitive, but the number token is NOT: a
        // global `(?i)` made the Roman-numeral class `[IVXLCDM]` match the
        // lowercase letters i/v/x/l/c/d/m inside ordinary words (e.g.
        // "articles dealing" → "Article de"). Arabic numbers (optionally with a
        // single-letter suffix like 10A) and UPPERCASE Roman numerals only.
        Regex::new(
            r"\b((?i:article|articles|section|sections|clause|clauses|sub-?section|sub-?clause|rule|rules|regulation|regulations|chapter|part|schedule)) ((?:[0-9]+[A-Za-z]?|[IVXLCDM]+)(?:\([A-Za-z0-9]+\))*)",
        )
        .unwrap()
    })
}

/// Match additional citation numbers in a comma-or-and-joined list:
/// "Articles 9, 10, and 10A" → after the head matches "Articles 9", this
/// regex captures the trailing "10" and "10A" tokens. Anchored on the
/// connector to keep the noise floor low.
fn additional_numbers_re() -> &'static Regex {
    static R: OnceLock<Regex> = OnceLock::new();
    R.get_or_init(|| {
        // Same Roman-numeral rule as section_path_re: the number token is
        // case-sensitive (uppercase Roman only) so a trailing "and <word>" is
        // not swept up. Only the "and" connector is case-insensitive.
        Regex::new(
            r"(?:,\s*|\s+(?i:and)\s+)((?:[0-9]+[A-Za-z]?|[IVXLCDM]+)(?:\([A-Za-z0-9]+\))*)\b",
        )
        .unwrap()
    })
}

fn instrument_re() -> &'static Regex {
    static R: OnceLock<Regex> = OnceLock::new();
    R.get_or_init(|| {
        Regex::new(
            r"\b(?:(?P<named>[A-Z][A-Za-z&]+(?:\s+(?:[A-Z][A-Za-z&]+|of|to|and|the|for|on|in|by)){0,6})\s+(?P<kind>Act|Ordinance|Rules|Regulations?|Policy)(?:,?\s*(?P<year>(?:19|20)\d{2}))?|(?P<constitution>Constitution(?: of Pakistan)?))\b",
        )
        .unwrap()
    })
}

fn year_re() -> &'static Regex {
    static R: OnceLock<Regex> = OnceLock::new();
    R.get_or_init(|| Regex::new(r"\b(?:19|20)\d{2}\b").unwrap())
}

pub fn extract(query: &str) -> Slots {
    let mut slots = Slots::default();

    // Capture EVERY "<head> N" provision phrase AND any trailing comma-/and-
    // joined bare-number siblings that share the first match's head noun.
    //   "Article 25"                       → ["Article 25"]
    //   "Articles 9, 10, and 10A"          → ["Article 9", "Article 10", "Article 10A"]
    //   "Section 7(1)(a)"                  → ["Section 7(1)(a)"]
    //   "Article 23 grant … Article 24"    → ["Article 23", "Article 24"]  (Fix A, Q9)
    //
    // `captures_iter` (not `captures`) is what picks up a SECOND separately-
    // phrased provision like "Article 24"; the sibling scanner below only
    // catches bare numbers in a list ("9, 10, 10A"), so both passes are needed.
    let mut first_match = true;
    for cap in section_path_re().captures_iter(query) {
        let head_word = cap.get(1).unwrap().as_str();
        let head_singular = singularise_head(head_word);
        let num = cap.get(2).unwrap().as_str().trim();
        let path = format!("{} {}", titlecase(head_singular), num);
        if !slots.section_paths.contains(&path) {
            slots.section_paths.push(path);
        }

        if !first_match {
            continue;
        }
        first_match = false;
        // Sibling bare-number scan, anchored on the FIRST match only. Stop at
        // the first sentence terminator to avoid sweeping up unrelated numbers
        // later in the query.
        let head_end = cap.get(0).unwrap().end();
        let tail = &query[head_end..];
        let cutoff = tail
            .find(|c: char| matches!(c, '.' | '?' | '!' | ';'))
            .unwrap_or(tail.len());
        let tail = &tail[..cutoff];
        for m in additional_numbers_re().captures_iter(tail) {
            let sib = m.get(1).unwrap().as_str().trim();
            let sib_path = format!("{} {}", titlecase(head_singular), sib);
            if !slots.section_paths.contains(&sib_path) {
                slots.section_paths.push(sib_path);
            }
        }
    }

    let mut instruments: Vec<String> = Vec::new();
    for cap in instrument_re().captures_iter(query) {
        if let Some(c) = cap.name("constitution") {
            instruments.push(c.as_str().to_string());
        } else {
            let named_raw = cap.name("named").map(|m| m.as_str().trim()).unwrap_or("");
            let named = strip_leading_command_verb(named_raw);
            let named = named.as_str();
            let kind = cap.name("kind").unwrap().as_str();
            let year = cap.name("year").map(|m| m.as_str());
            let label = match year {
                Some(y) => format!("{} {} {}", named, titlecase(kind), y),
                None => format!("{} {}", named, titlecase(kind)),
            };
            instruments.push(label.split_whitespace().collect::<Vec<_>>().join(" "));
        }
    }
    if let Some(first) = instruments.first().cloned() {
        slots.instrument = Some(first);
    }
    if instruments.len() >= 2 {
        slots.comparands = instruments;
    }

    if let Some(m) = year_re().find(query) {
        slots.timeline_anchor = Some(m.as_str().to_string());
    }

    if slots.section_paths.is_empty() {
        let t = topic_from(query);
        if !t.is_empty() {
            slots.topic = Some(t);
        }
    }

    slots
}

/// Map the plural head form back to its singular for citation rendering:
/// "articles"/"sections"/... → "article"/"section"/... All other heads
/// (already singular) pass through unchanged.
fn singularise_head(head: &str) -> &str {
    let lower = head.to_ascii_lowercase();
    match lower.as_str() {
        "articles" => "article",
        "sections" => "section",
        "clauses" => "clause",
        "rules" => "rule",
        "regulations" => "regulation",
        _ => head,
    }
}

/// Strip a sentence-leading imperative/command verb (and any articles that
/// immediately follow it) from a captured instrument name. The instrument
/// regex's `named` group starts at `[A-Z]`, so a capitalised sentence-initial
/// verb like "Identify" / "List" / "Explain" gets swept in — "Identify the
/// Principles of Policy" must reduce to "Principles of Policy" (Q15 regression).
fn strip_leading_command_verb(named: &str) -> String {
    const VERBS: &[&str] = &[
        "identify", "list", "explain", "describe", "summarize", "summarise",
        "compare", "define", "give", "show", "tell", "find", "name", "state",
        "outline", "discuss", "provide", "quote", "analyse", "analyze",
    ];
    let mut tokens: Vec<&str> = named.split_whitespace().collect();
    if tokens
        .first()
        .is_some_and(|t| VERBS.contains(&t.to_ascii_lowercase().as_str()))
    {
        tokens.remove(0);
        // Drop articles left dangling at the front ("the"/"a"/"an").
        while tokens
            .first()
            .is_some_and(|t| matches!(t.to_ascii_lowercase().as_str(), "the" | "a" | "an"))
        {
            tokens.remove(0);
        }
    }
    tokens.join(" ")
}

fn titlecase(s: &str) -> String {
    let mut c = s.chars();
    match c.next() {
        Some(f) => f.to_uppercase().collect::<String>() + &c.as_str().to_lowercase(),
        None => String::new(),
    }
}

fn topic_from(query: &str) -> String {
    let lower = query.trim().to_lowercase();
    let mut start = 0;
    for prefix in [
        "what does ", "what is ", "summarize ", "summarise ", "summary of ",
        "tldr ", "tl;dr ", "explain ", "tell me about ", "give me ",
        "show me ", "describe ", "overview of ", "in brief, ", "list ",
    ] {
        if lower.starts_with(prefix) {
            start = prefix.len();
            break;
        }
    }
    query.trim()[start..]
        .trim_end_matches(|c: char| matches!(c, '?' | '.' | '!'))
        .trim()
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn article_path() {
        let s = extract("What does Article 19 say?");
        assert_eq!(s.section_paths, vec!["Article 19".to_string()]);
    }

    #[test]
    fn section_with_subclauses() {
        let s = extract("Quote Section 7(1)(a) verbatim");
        assert_eq!(s.section_paths, vec!["Section 7(1)(a)".to_string()]);
    }

    #[test]
    fn multi_target_articles_comma_and_list() {
        let s = extract("What rights are guaranteed across Articles 9, 10, and 10A taken together?");
        assert_eq!(
            s.section_paths,
            vec![
                "Article 9".to_string(),
                "Article 10".to_string(),
                "Article 10A".to_string()
            ],
            "got {:?}",
            s.section_paths
        );
    }

    #[test]
    fn multi_target_two_articles_with_and() {
        let s = extract("Compare Articles 62 and 63.");
        assert_eq!(
            s.section_paths,
            vec!["Article 62".to_string(), "Article 63".to_string()],
            "got {:?}",
            s.section_paths
        );
    }

    /// Fix A (Q9, 2026-06-07): two SEPARATELY-phrased provisions — each its own
    /// "Article N" head+number, not a comma/and bare-number list — must both be
    /// captured. Previously only the first ("Article 23") was kept because
    /// `captures()` returns a single match and the sibling scanner only catches
    /// bare trailing numbers; "Article 24 provide" was a fresh head phrase →
    /// missed → `section_paths.len() == 1` → no multi-article split.
    #[test]
    fn two_separate_article_phrases_both_captured() {
        let s = extract(
            "What property rights does Article 23 grant, and what additional protection does Article 24 provide?",
        );
        assert_eq!(
            s.section_paths,
            vec!["Article 23".to_string(), "Article 24".to_string()],
            "got {:?}",
            s.section_paths
        );
    }

    #[test]
    fn named_act_with_year() {
        let s = extract("Summarize the Right to Information Act, 2017");
        let inst = s.instrument.unwrap_or_default();
        assert!(inst.contains("Right to Information Act"), "got {inst}");
        assert_eq!(s.timeline_anchor.as_deref(), Some("2017"));
    }

    #[test]
    fn constitution_only() {
        let s = extract("Summarize Chapter II of the Constitution");
        assert_eq!(s.instrument.as_deref(), Some("Constitution"));
    }

    #[test]
    fn two_instruments_populate_comparands() {
        let s = extract("Compare the RTI Act 2017 with the Punjab Transparency Act 2013");
        assert_eq!(s.comparands.len(), 2, "got {:?}", s.comparands);
    }

    #[test]
    fn topic_when_no_section_path() {
        let s = extract("Summarize whistleblower protections");
        assert_eq!(s.topic.as_deref(), Some("whistleblower protections"));
    }

    /// Regression (Q15, 2026-06-06): a head noun followed by an ordinary word
    /// starting with a Roman-numeral letter (i/v/x/l/c/d/m) must NOT be parsed
    /// as a numbered provision. "articles dealing" was matching "Article de"
    /// because the `[IVXLCDM]` class ran under `(?i)`.
    #[test]
    fn head_noun_followed_by_word_is_not_a_section_path() {
        let s = extract(
            "Identify the Principles of Policy articles dealing with the Islamic way of life",
        );
        assert!(
            s.section_paths.is_empty(),
            "expected no section path; got {:?}",
            s.section_paths
        );
    }

    /// Same Roman-numeral root cause in the sibling-list scanner: after a real
    /// primary match, a trailing "and <word>" must not be swept up as another
    /// provision. "Articles 62 and living conditions" must yield ONLY Article 62.
    #[test]
    fn sibling_scanner_does_not_match_trailing_word() {
        let s = extract("Compare Articles 62 and living conditions in the same chapter");
        assert_eq!(
            s.section_paths,
            vec!["Article 62".to_string()],
            "got {:?}",
            s.section_paths
        );
    }

    /// Uppercase Roman numerals are still valid provision numbers.
    #[test]
    fn uppercase_roman_numeral_still_parses() {
        let s = extract("What does Article IV say?");
        assert_eq!(s.section_paths, vec!["Article IV".to_string()]);
    }

    /// Regression (Q15, 2026-06-06): a sentence-leading imperative verb must not
    /// be swept into the named instrument. "Identify the Principles of Policy …"
    /// was yielding instrument "Identify the Principles of Policy", poisoning the
    /// system prompt. Expected: the verb is stripped → "Principles of Policy".
    #[test]
    fn instrument_drops_leading_command_verb() {
        let s = extract(
            "Identify the Principles of Policy articles dealing with the Islamic way of life",
        );
        let inst = s.instrument.unwrap_or_default();
        assert!(
            !inst.to_lowercase().starts_with("identify"),
            "instrument leaked the leading verb: {inst}"
        );
        assert_eq!(inst, "Principles of Policy", "got {inst}");
    }
}
