//! US8/R2 (FR-013b) — generic boilerplate detection.
//!
//! Some indexed chunks are page chrome, not provisions: contact pages, phone/
//! email/URL footers, "Page N of M" markers. They are correctly *extracted*
//! (the text is really on the page) but should not rank as retrievable
//! provisions. This classifier is purely **structural / content-type** — no
//! per-document keyword lists (durable project rule) — and is deliberately
//! conservative (keep-bias): the cost of dropping a real provision is silent
//! and unrecoverable, so we only flag chunks we are confident are non-content.
//!
//! Safety invariant (FR-013b ii): a chunk carrying a provision identity (a real
//! provision label or a leading provision marker) is NEVER flagged. The caller
//! passes `has_provision_identity`; we hard-return `false` for it.
//!
//! The flag is advisory: callers FLAG (never delete) and retrieval excludes
//! flagged chunks from ranking, so a false positive is reversible + auditable.

use regex_lite::Regex;
use std::sync::OnceLock;

/// True when `content` reads as boilerplate page-chrome rather than a provision.
/// `has_provision_identity` short-circuits to `false` (the keep invariant).
pub fn is_boilerplate(content: &str, has_provision_identity: bool) -> bool {
    if has_provision_identity {
        return false;
    }
    let trimmed = content.trim();
    if trimmed.is_empty() {
        return true;
    }
    if is_page_marker(trimmed) {
        return true;
    }
    let tokens: Vec<&str> = trimmed.split_whitespace().collect();
    let n = tokens.len();
    if n == 0 {
        return true;
    }
    // Short fragments only — a long passage is treated as prose even if it
    // happens to contain a phone number, so we never flag a real provision.
    let is_short = n < 25;
    if !is_short {
        return false;
    }
    let has_contact = has_email(trimmed) || has_url(trimmed) || has_phone(trimmed);
    let wordy = tokens.iter().filter(|t| is_wordish(t)).count();
    let low_prose = (wordy as f32 / n as f32) < 0.5;
    has_contact || low_prose
}

/// A token is "wordish" when it is mostly alphabetic with ≥3 letters
/// ("Authority" yes; "021-99211234", "::", "of" no).
fn is_wordish(tok: &str) -> bool {
    let alpha = tok.chars().filter(|c| c.is_alphabetic()).count();
    let len = tok.chars().count().max(1);
    alpha >= 3 && (alpha as f32 / len as f32) >= 0.6
}

fn is_page_marker(s: &str) -> bool {
    static RE: OnceLock<Regex> = OnceLock::new();
    let re = RE.get_or_init(|| {
        // "P a g e", "Page 3 of 12", a bare "- 5 -" / "5" page number line.
        Regex::new(r"(?i)^(?:p\s*a\s*g\s*e\b|page\s+\d+\s+of\s+\d+|-?\s*\d{1,4}\s*-?)$")
            .expect("page marker regex compiles")
    });
    re.is_match(s.trim())
}

fn has_email(s: &str) -> bool {
    static RE: OnceLock<Regex> = OnceLock::new();
    let re = RE.get_or_init(|| {
        Regex::new(r"[\w.+-]+@[\w-]+\.[\w.-]+").expect("email regex compiles")
    });
    re.is_match(s)
}

fn has_url(s: &str) -> bool {
    static RE: OnceLock<Regex> = OnceLock::new();
    let re = RE.get_or_init(|| {
        Regex::new(r"(?i)(?:https?://|www\.)\S+").expect("url regex compiles")
    });
    re.is_match(s)
}

fn has_phone(s: &str) -> bool {
    static RE: OnceLock<Regex> = OnceLock::new();
    let re = RE.get_or_init(|| {
        // A separated digit run of length ≥7 (phone/fax), e.g. 021-99211234.
        Regex::new(r"\+?\d[\d\s().-]{5,}\d").expect("phone regex compiles")
    });
    re.is_match(s)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn contact_page_is_boilerplate() {
        let c = "Contact Us Phone: 021-99211234 Email: info@spsc.gov.pk www.spsc.gov.pk";
        assert!(is_boilerplate(c, false));
    }

    #[test]
    fn page_markers_are_boilerplate() {
        assert!(is_boilerplate("Page 3 of 12", false));
        assert!(is_boilerplate("P a g e", false));
        assert!(is_boilerplate("- 5 -", false));
    }

    #[test]
    fn provision_identity_is_never_flagged() {
        // The keep invariant: even a terse real rule is kept when it carries a
        // provision identity.
        assert!(!is_boilerplate("English.", true));
        assert!(!is_boilerplate("Contact Us info@x.gov.pk", true));
    }

    #[test]
    fn real_provision_prose_is_kept() {
        let c = "The competent authority may sanction expenditure within the limits \
                 prescribed under the applicable financial rules and shall record reasons \
                 in writing before according such sanction to any proposal placed before it.";
        assert!(!is_boilerplate(c, false), "long prose passage must be kept");
    }

    #[test]
    fn short_real_clause_without_identity_is_kept() {
        // No contact, high prose ratio → not flagged even when short.
        assert!(!is_boilerplate("No fee shall be charged for inspection.", false));
    }

    #[test]
    fn symbol_fragment_is_boilerplate() {
        assert!(is_boilerplate("----- :: //// 12 34 ----", false));
    }
}
