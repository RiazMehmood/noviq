//! `generic` profile — last-resort fallback. Produces a chapter → section
//! → leaf tree from visual headings (ALL-CAPS, outline numbering).

use super::{DocProfile, DetectionPattern, HeadingPattern};
use crate::services::structure_parser::NodeLevel;

pub const GENERIC: DocProfile = DocProfile {
    doc_type: "generic",
    level_vocab: [
        "document", "part", "chapter", "section", "subsection", "clause",
    ],
    citation_unit: "Section",
    salient_units: &[],
    // No detection patterns — generic always wins as fallback (min_confidence=0).
    detection_patterns: &[] as &[DetectionPattern],
    heading_patterns: &[
        HeadingPattern {
            regex: r"(?m)^(?P<num>\d+)\.\s+(?P<title>[A-Z][^\n]{2,80})$",
            level: NodeLevel::Chapter,
            citation_fmt: "Chapter {num}",
        },
        HeadingPattern {
            regex: r"(?m)^(?P<num>\d+\.\d+)\s+(?P<title>[A-Z][^\n]{2,80})",
            level: NodeLevel::Article,
            citation_fmt: "Section {num}",
        },
        HeadingPattern {
            regex: r"(?m)^(?P<num>\d+\.\d+\.\d+)\s+(?P<title>[^\n]{2,80})",
            level: NodeLevel::Clause,
            citation_fmt: "Section {num}",
        },
    ],
    min_confidence: 0.0,
};

#[cfg(test)]
mod tests {
    use super::*;
    use regex_lite::Regex;

    #[test]
    fn min_confidence_is_zero() {
        assert_eq!(GENERIC.min_confidence, 0.0);
    }

    #[test]
    fn heading_patterns_capture_numbered_outline() {
        let text = "1. Introduction\n\nbody\n\n1.1 Background\n\nmore\n\n2. Method";
        let any_match = GENERIC.heading_patterns.iter()
            .any(|h| Regex::new(h.regex).unwrap().is_match(text));
        assert!(any_match);
    }
}
