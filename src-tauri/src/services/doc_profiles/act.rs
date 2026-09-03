//! `act` profile — Acts / Ordinances / Codes.
//! Chapter → Section → Sub-section → Clause hierarchy. Detected by lines
//! starting with `Section N` or `Chapter <roman>`.

use super::{DocProfile, DetectionPattern, HeadingPattern};
use crate::services::structure_parser::NodeLevel;

pub const ACT: DocProfile = DocProfile {
    doc_type: "act",
    level_vocab: [
        "document", "part", "chapter", "section", "subsection", "clause",
    ],
    citation_unit: "Section",
    salient_units: &["Preamble"],
    // Pakistani drafting style: `Ordinance`/`Act` in title + body, `Section N.`
    // mentioned even when section headings are marginal-numbered. Body-word
    // frequencies differentiate Act from Rules (which lacks `Ordinance`).
    detection_patterns: &[
        DetectionPattern { regex: r"\bOrdinance\b", weight: 3, level: NodeLevel::Document },
        DetectionPattern { regex: r"(?m)^Section\s+\d+\.", weight: 3, level: NodeLevel::Article },
        DetectionPattern { regex: r"(?m)^Chapter\s+[IVX]+\b", weight: 2, level: NodeLevel::Chapter },
    ],
    heading_patterns: &[
        HeadingPattern {
            regex: r"(?m)^\s*Chapter\s+(?P<num>[IVX\d]+)(?:\s*[—-]\s*(?P<title>[^\n]+))?",
            level: NodeLevel::Chapter,
            citation_fmt: "Chapter {num}",
        },
        HeadingPattern {
            regex: r"(?m)^\s*Section\s+(?P<num>\d+[A-Z]?)\.\s*(?P<title>[^\n]+)?",
            level: NodeLevel::Article,
            citation_fmt: "Section {num}",
        },
        // Pakistani marginal style: bare-number heading at line start, with
        // a sub-clause opener. RC-A (audit 2026-06-29): accept roman `(i)`/alpha
        // `(a)` openers (not just numeric `(1)`) and an optional leading `[`
        // amendment bracket.
        HeadingPattern {
            // D4 (2026-07-27): the sub-clause opener may sit tight against the
            // dot (`58.(1)`) or behind an amendment bracket (`56. [(1)]`).
            // 2026-07-29: `.-` catchline dash tight against the number
            // (`10.- (1) …`). See rules.rs for the E&D/APT cases that found it.
            regex: r"(?m)^\s*\[*\s*(?P<num>\d+[A-Z]?)\.-?\s*[\[\*]?\s*\((?:\d+|[ivxlcdm]+|[a-z])\)",
            level: NodeLevel::Article,
            citation_fmt: "Section {num}",
        },
        // Bare-number section with title or bracketed marker:
        //   "3.    [Dismissal...]"  "4.  Suspension. --- A person..."
        //   "15. Power to make rules. ---"  "[16. Inserted section ...]"
        // `\s*` covers indent and form-feed page-breaks; the leading `\[?` admits
        // an amendment-INSERTED section whose bracket precedes the number (RC-A).
        HeadingPattern {
            // 2026-07-29: optional opening quote before the capital — a
            // provision may open on a quoted defined term (`90. "Secret" …`).
            // Applied to all three profiles; see manual.rs for the case that
            // found it. The capital stays required.
            regex: r#"(?m)^\s*\[*\s*(?P<num>\d+[A-Z]?)\.-?\s+\*?\s*(?P<title>["']?[\[A-Z][^\n]{0,120})"#,
            level: NodeLevel::Article,
            citation_fmt: "Section {num}",
        },
        // Marginal-label section ("Short title     1. (1) ..."). Requires
        // 2+ spaces between Cap-leading label and the section number to
        // distinguish from inline mentions like "section 5(2)".
        HeadingPattern {
            regex: r"(?m)^[A-Z][a-zA-Z]+(?:\s+[a-z][a-zA-Z]*){0,4}\s{2,}(?P<num>\d+[A-Z]?)\.\s+\((?P<sub>\d+)\)",
            level: NodeLevel::Article,
            citation_fmt: "Section {num}",
        },
        HeadingPattern {
            regex: r"(?m)^\s*\((?P<num>\d+)\)\s+[A-Z]",
            level: NodeLevel::Clause,
            citation_fmt: "({num})",
        },
        HeadingPattern {
            regex: r"(?m)^\s*\((?P<num>[a-z])\)\s+",
            level: NodeLevel::SubClause,
            citation_fmt: "({num})",
        },
    ],
    min_confidence: 3.0,
};

#[cfg(test)]
mod tests {
    use super::*;
    use regex_lite::Regex;

    #[test]
    fn detection_patterns_match_act_corpus() {
        let positives = [
            "Section 3. The competent authority shall...",
            "Section 7. The order shall be in writing.",
            "Chapter II — Powers of the Authority",
        ];
        for sample in positives {
            assert!(
                ACT.detection_patterns.iter().any(|p| Regex::new(p.regex).unwrap().is_match(sample)),
                "no detection pattern matched: {sample:?}",
            );
        }
    }

    #[test]
    fn heading_patterns_present_for_three_levels() {
        let levels: std::collections::HashSet<_> =
            ACT.heading_patterns.iter().map(|h| h.level).collect();
        assert!(levels.contains(&NodeLevel::Chapter));
        assert!(levels.contains(&NodeLevel::Article));
        assert!(levels.contains(&NodeLevel::Clause));
    }
}
