//! 013/US5 (T037) — deterministic amendment-event parser.
//!
//! The fast tier isolates amendment-history footnotes in Phase 3
//! (`block_class='footnote'`). This module turns one footnote's verbatim text
//! into an `AmendmentEvent` (kind + the instrument that made the change), with
//! NO model. It captures only — it never invents an instrument or a target and
//! returns `None` for any line that is not amendment apparatus.

use regex_lite::Regex;
use std::sync::OnceLock;

/// The kind of change an amendment instrument made to a provision.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AmendmentKind {
    Inserted,
    Substituted,
    Repealed,
    Omitted,
}

impl AmendmentKind {
    /// DB token for `amendment_events.kind`
    /// (`inserted|substituted|repealed|omitted`).
    pub fn as_str(self) -> &'static str {
        match self {
            AmendmentKind::Inserted => "inserted",
            AmendmentKind::Substituted => "substituted",
            AmendmentKind::Repealed => "repealed",
            AmendmentKind::Omitted => "omitted",
        }
    }
}

/// One amendment event parsed from a footnote. `by_instrument` is a verbatim
/// slice of the footnote (never synthesized); `effective_date` is the `w.e.f`
/// date when stated.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AmendmentEvent {
    pub kind: AmendmentKind,
    pub by_instrument: String,
    pub effective_date: Option<String>,
}

/// Leading amendment verb → kind. Abbreviations carry a trailing dot in the
/// source ("Subs.", "Ins.", "Omitted").
fn classify_verb(text: &str) -> Option<AmendmentKind> {
    static RE: OnceLock<Regex> = OnceLock::new();
    let re = RE.get_or_init(|| {
        // Anchored near the start (after an optional footnote-ref digit), so the
        // verb is the footnote's leading action, not a word buried in prose.
        Regex::new(r"(?i)^\s*\d*\s*\b(subs|substituted|ins|inserted|added|omitted|deleted|repealed)\b")
            .unwrap()
    });
    let m = re.captures(text)?;
    let verb = m.get(1)?.as_str().to_ascii_lowercase();
    Some(match verb.as_str() {
        "subs" | "substituted" => AmendmentKind::Substituted,
        "ins" | "inserted" | "added" => AmendmentKind::Inserted,
        "omitted" | "deleted" => AmendmentKind::Omitted,
        "repealed" => AmendmentKind::Repealed,
        _ => return None,
    })
}

/// Extract the instrument phrase verbatim: the text after the leading
/// `<verb>[.] by`, trimmed, cut before the replaced-text clause (`, for "…"`) or
/// the `w.e.f` parenthetical so it names the INSTRUMENT, not the change target.
fn extract_instrument(text: &str) -> Option<String> {
    static RE: OnceLock<Regex> = OnceLock::new();
    let re = RE.get_or_init(|| {
        Regex::new(r"(?i)\b(?:subs|substituted|ins|inserted|added|omitted|deleted|repealed)\b\.?\s+by\s+(.+)")
            .unwrap()
    });
    let rest = re.captures(text)?.get(1)?.as_str();
    // Cut at the first boundary that ends the instrument designation.
    let mut end = rest.len();
    for marker in [", for ", " for \"", " (w.e.f", ", (w.e.f", " w.e.f", ", which", " which was"] {
        if let Some(i) = find_ci(rest, marker) {
            end = end.min(i);
        }
    }
    let instrument = rest[..end].trim().trim_end_matches([',', '.', ';']).trim();
    if instrument.is_empty() {
        None
    } else {
        Some(instrument.to_string())
    }
}

/// `w.e.f <date>` → the date string (verbatim), when present.
fn extract_effective_date(text: &str) -> Option<String> {
    static RE: OnceLock<Regex> = OnceLock::new();
    let re = RE.get_or_init(|| {
        // Capture up to the closing paren (the `w.e.f` clause is usually
        // parenthesised) or end of line; trim trailing punctuation.
        Regex::new(r"(?i)w\.?e\.?f\.?\s*(?:the\s+)?([^)]+)").unwrap()
    });
    let d = re.captures(text)?.get(1)?.as_str().trim().trim_end_matches(['.', ',', ';']).trim();
    if d.is_empty() {
        None
    } else {
        Some(d.to_string())
    }
}

/// Case-insensitive substring search returning the byte index of the first hit.
fn find_ci(haystack: &str, needle: &str) -> Option<usize> {
    let (h, n) = (haystack.to_ascii_lowercase(), needle.to_ascii_lowercase());
    h.find(&n)
}

/// Parse a footnote into an `AmendmentEvent`, or `None` when the line is not
/// amendment apparatus (no leading amendment verb) or carries no instrument.
pub fn parse_amendment(footnote: &str) -> Option<AmendmentEvent> {
    let kind = classify_verb(footnote)?;
    let by_instrument = extract_instrument(footnote)?;
    let effective_date = extract_effective_date(footnote);
    Some(AmendmentEvent {
        kind,
        by_instrument,
        effective_date,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_substitution_with_instrument_and_date() {
        let f = "Subs. by the Constitution (Fourth Amdt.) Act, 1975 (71 of 1975), s. 2, \
                 for \"paragraph (b)\", (w.e.f the 21st November, 1985), which was previously amended.";
        let ev = parse_amendment(f).expect("should parse");
        assert_eq!(ev.kind, AmendmentKind::Substituted);
        assert_eq!(ev.by_instrument, "the Constitution (Fourth Amdt.) Act, 1975 (71 of 1975), s. 2");
        assert_eq!(ev.effective_date.as_deref(), Some("21st November, 1985"));
    }

    #[test]
    fn parses_insertion_via_ordinance() {
        let ev = parse_amendment("Ins. by Ordinance XX of 2002, w.e.f. 21st August 2002.").unwrap();
        assert_eq!(ev.kind, AmendmentKind::Inserted);
        assert_eq!(ev.by_instrument, "Ordinance XX of 2002");
        assert_eq!(ev.effective_date.as_deref(), Some("21st August 2002"));
    }

    #[test]
    fn parses_eighteenth_amendment_substitution() {
        let ev = parse_amendment("Subs. by the Constitution (Eighteenth Amdt.) Act, 2010, s. 4.").unwrap();
        assert_eq!(ev.kind, AmendmentKind::Substituted);
        assert_eq!(ev.by_instrument, "the Constitution (Eighteenth Amdt.) Act, 2010, s. 4");
        assert_eq!(ev.effective_date, None);
    }

    #[test]
    fn ordinary_prose_and_non_amendment_footnotes_are_none() {
        // A cross-reference footnote is not an amendment event.
        assert!(parse_amendment("See footnote 6 on page 3, supra").is_none());
        // Amendment verb but no instrument ("by …") → not a capturable event.
        assert!(parse_amendment("Substituted text appears in the margin.").is_none());
        // Plain prose.
        assert!(parse_amendment("No person shall be deprived of life or liberty.").is_none());
    }

    #[test]
    fn kind_db_tokens_match_schema_check() {
        for (k, s) in [
            (AmendmentKind::Inserted, "inserted"),
            (AmendmentKind::Substituted, "substituted"),
            (AmendmentKind::Repealed, "repealed"),
            (AmendmentKind::Omitted, "omitted"),
        ] {
            assert_eq!(k.as_str(), s);
        }
    }
}
