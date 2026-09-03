//! Grounding gate on the final synthesis (US2; FR-006, INV-4, Constitution XI).
//!
//! Enforces that a Deep answer stays tied to the retrieved passages:
//!   1. **No grounding at all** (no passages, or an empty answer) → the caller
//!      returns the grounded-fallback message instead of ungrounded prose.
//!   2. **Foreign figures** — numbers stated in the answer that appear in NO
//!      retrieved passage are flagged with a verification caveat (conservative:
//!      flagged, not silently deleted, since removing a figure from prose would
//!      mangle the sentence). This is the deterministic defence against the
//!      "made-up number" class.
//!
//! Reuses the same normalize-then-substring basis as `one_ask::verify`
//! (`quote_verifies`), replicated here as a small self-contained helper rather
//! than coupling to the drafting `ProvisionCard`/section model.
//!
//! HONEST LIMITATION: a fabricated *bare entity name* in free prose (the live
//! "Department Zoonology" class) cannot be caught deterministically without NLI.
//! The defences against it are (a) the synthesis prompt's "use ONLY these
//! passages", (b) this figure/quote grounding, and (c) citations that are built
//! from the passages themselves — not free-form entity verification.

use super::Passage;

/// Message returned when an answer cannot be grounded in any retrieved passage.
pub(crate) const GROUNDING_FALLBACK: &str =
    "I could not find enough grounded material in the indexed documents to answer this reliably. \
     Try rephrasing, or narrow the question to a specific instrument or provision.";

/// Outcome of the grounding gate.
pub(crate) struct GroundingOutcome {
    /// The answer to surface (possibly with an appended verification caveat).
    pub answer: String,
    /// False → the caller must use the fallback and drop citations (INV-4).
    pub grounded: bool,
    /// Count of answer figures not found in any passage.
    pub figures_flagged: usize,
}

/// Apply the grounding gate to a synthesized answer against the packed passages.
pub(crate) fn check(answer: &str, passages: &[&Passage]) -> GroundingOutcome {
    let a = answer.trim();
    if a.is_empty() || passages.is_empty() {
        return GroundingOutcome { answer: GROUNDING_FALLBACK.to_string(), grounded: false, figures_flagged: 0 };
    }

    let haystack: Vec<String> = passages.iter().map(|p| p.content.clone()).collect();
    let foreign = foreign_numbers(a, &haystack.join(" "));

    if foreign.is_empty() {
        GroundingOutcome { answer: a.to_string(), grounded: true, figures_flagged: 0 }
    } else {
        let caveat = format!(
            "\n\n_Note: {} figure(s) in this answer could not be verified against the retrieved \
             provisions and should be checked against the source text._",
            foreign.len()
        );
        GroundingOutcome { answer: format!("{a}{caveat}"), grounded: true, figures_flagged: foreign.len() }
    }
}

/// Normalize for substring comparison: hyphens/whitespace → single space,
/// lowercase. (Replicated from `one_ask::verify::normalize`.)
fn normalize(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut last_space = false;
    for c in s.chars() {
        let c = if c == '-' || c.is_whitespace() { ' ' } else { c };
        if c == ' ' {
            if !last_space {
                out.push(' ');
            }
            last_space = true;
        } else {
            out.push(c.to_ascii_lowercase());
            last_space = false;
        }
    }
    out.trim().to_string()
}

/// The digit-only form of a numeric token ("10,000" / "10.000" → "10000"), so
/// formatting variants compare equal.
fn digits_only(tok: &str) -> String {
    tok.chars().filter(|c| c.is_ascii_digit()).collect()
}

/// Maximal runs of digit/separator chars in `s`, as raw tokens.
fn extract_numbers(s: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    for c in s.chars() {
        if c.is_ascii_digit() || (!cur.is_empty() && (c == ',' || c == '.')) {
            cur.push(c);
        } else if !cur.is_empty() {
            out.push(std::mem::take(&mut cur));
        }
    }
    if !cur.is_empty() {
        out.push(cur);
    }
    out
}

/// Remove `[...]` citation-marker spans so their numbers aren't mistaken for
/// figures. Everything between `[` and the next `]` (inclusive) is dropped.
fn strip_citation_markers(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut depth = 0u32;
    for c in s.chars() {
        match c {
            '[' => depth += 1,
            ']' if depth > 0 => depth -= 1,
            _ if depth == 0 => out.push(c),
            _ => {}
        }
    }
    out
}

/// Numeric tokens in `prose` (≥ 2 digits, to skip list markers like "1." / "2)")
/// whose digit form appears in NO passage. Deduped.
fn foreign_numbers(prose: &str, passages_text: &str) -> Vec<String> {
    let hay: std::collections::HashSet<String> =
        extract_numbers(&normalize(passages_text)).iter().map(|t| digits_only(t)).collect();
    // Strip `[n]` citation markers first — they are references, NOT figures. Without
    // this the gate flagged its own citation numbers ("[14]", "[16]") as
    // "unverified figures" (live: 5 flagged on an answer with no real numbers).
    let prose = strip_citation_markers(prose);
    let mut out: Vec<String> = Vec::new();
    for tok in extract_numbers(&prose) {
        let d = digits_only(&tok);
        if d.len() >= 2 && !hay.contains(&d) && !out.contains(&tok) {
            out.push(tok);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn passage(content: &str) -> Passage {
        Passage {
            chunk_id: "c".into(), source_id: "s".into(), content: content.into(),
            page: None, label: "l".into(), round: 1, for_sub_question: 1,
        }
    }

    #[test]
    fn no_passages_or_empty_answer_is_ungrounded() {
        let o = check("some answer", &[]);
        assert!(!o.grounded);
        assert_eq!(o.answer, GROUNDING_FALLBACK);

        let p = passage("text");
        let o2 = check("   ", &[&p]);
        assert!(!o2.grounded);
    }

    #[test]
    fn figure_in_passage_is_clean_foreign_figure_is_flagged() {
        let p = passage("The appeal must be filed within 30 days to the Service Tribunal.");
        // "30" present → clean.
        let clean = check("An appeal lies within 30 days.", &[&p]);
        assert!(clean.grounded);
        assert_eq!(clean.figures_flagged, 0);
        assert!(!clean.answer.contains("could not be verified"));

        // "90" absent from passages → flagged (a fabricated deadline).
        let flagged = check("An appeal lies within 90 days.", &[&p]);
        assert!(flagged.grounded);
        assert_eq!(flagged.figures_flagged, 1);
        assert!(flagged.answer.contains("could not be verified"));
    }

    #[test]
    fn citation_markers_are_not_flagged_as_figures() {
        // Citation markers [12]/[14]/[16] must NOT be treated as unverified figures
        // (they are references, not numbers in the prose).
        let p = passage("Minor penalties include censure and withholding of increment.");
        let o = check("Penalties are minor or major [12]. Minor include censure [14]; major include dismissal [16].", &[&p]);
        assert_eq!(o.figures_flagged, 0);
        assert!(!o.answer.contains("could not be verified"));
    }

    #[test]
    fn separator_variants_match() {
        let p = passage("Bid security is Rs. 1,000,000 for large contracts.");
        let o = check("The security is Rs. 1000000.", &[&p]);
        assert_eq!(o.figures_flagged, 0); // "1,000,000" ≡ "1000000"
    }

    #[test]
    fn single_digit_list_markers_are_ignored() {
        let p = passage("Grounds include misconduct and inefficiency.");
        let o = check("1. Misconduct. 2. Inefficiency.", &[&p]);
        assert_eq!(o.figures_flagged, 0); // single-digit markers skipped
    }
}
