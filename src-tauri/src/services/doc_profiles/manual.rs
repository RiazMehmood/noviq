//! `manual` profile — Manuals / Handbooks.
//! Chapter → Section → Sub-section hierarchy with outline numbering.

use super::{DocProfile, DetectionPattern, HeadingPattern};
use crate::services::structure_parser::NodeLevel;

pub const MANUAL: DocProfile = DocProfile {
    doc_type: "manual",
    level_vocab: [
        "document", "part", "chapter", "section", "subsection", "clause",
    ],
    citation_unit: "Section",
    salient_units: &["Preamble", "Foreword"],
    // Detection: "Manual" / "Instruction(s)" / "Secretariat" are the
    // discriminating words against an Act/Rules of similar size.
    detection_patterns: &[
        DetectionPattern { regex: r"\bManual\b", weight: 3, level: NodeLevel::Document },
        DetectionPattern { regex: r"\b[Ii]nstruction[s]?\b", weight: 2, level: NodeLevel::Document },
        DetectionPattern { regex: r"\bSecretariat\b", weight: 2, level: NodeLevel::Document },
        // CHAPTER headings may be roman (`CHAPTER II`) or arabic (`CHAPTER 1.`);
        // constitution-style Articles documents use the arabic form. The heading
        // is frequently CENTERED, so allow leading whitespace (`^[ \t]*`).
        DetectionPattern { regex: r"(?m)^[ \t]*CHAPTER\s+(?:[IVX]+|\d+)\b", weight: 3, level: NodeLevel::Chapter },
    ],
    heading_patterns: &[
        HeadingPattern {
            // Accept arabic chapter numbers (`CHAPTER 1. – FUNDAMENTAL RIGHTS`)
            // as well as roman (`CHAPTER II`), and a CENTERED (leading-whitespace)
            // heading line. The trailing `.`/`–`/title is optional, so `num`
            // never swallows the separator.
            regex: r"(?m)^[ \t]*CHAPTER\s+(?P<num>[IVX]+|\d+)(?:\s+(?P<title>[A-Z][^\n]+))?",
            level: NodeLevel::Chapter,
            citation_fmt: "Chapter {num}",
        },
        HeadingPattern {
            // `\d+[A-Z]?` captures alpha-suffixed provision ordinals (10A, 19A,
            // 25A) that a bare `\d+` drops at the `.` boundary.
            regex: r"(?m)^(?P<num>\d+[A-Z]?)\.\s+(?P<title>[A-Z][A-Z\s]{4,})$",
            level: NodeLevel::Article,
            citation_fmt: "Section {num}",
        },
        // Bare-number marginal style (same as Act/Rules) — kept for Manuals
        // that use marginal headings instead of ALL-CAPS section titles.
        // RC-A (audit 2026-06-29): optional leading `[` for amendment-inserted
        // sections.
        // D4 (2026-07-27): an amendment bracket may also follow the dot, wrapping
        // the provision's TEXT rather than its number — `71. [Mediation
        // Committee] Omitted by...`. Requiring whitespace-then-capital missed
        // those. The bracket alternative also permits no whitespace at all
        // (`71.[Mediation`), while a bare `\s*` would not, since that would let
        // `1996.The Act` match.
        HeadingPattern {
        // 2026-07-29: two amendment-notation shapes were still invisible.
        //  * `\[*` (was `\[?`): a twice-substituted provision carries NESTED
        //    brackets — Constitution Article 90 prints as `[[90. (1) Subject to
        //    the Constitution…` while Article 91 beside it has one bracket and
        //    was found. One optional `[` could not match two.
        //  * `[\[\*]?`: a footnote marker may sit between the space and the
        //    text — Article 272 prints as `272. *Notwithstanding anything…`
        //    (the superscript filter strips the marker's digit, leaving `*`).
        // Both consume the marker rather than capture it, so labels are clean.
        // 2026-07-29: allow an opening QUOTE before the capital. Secretariat
        // Instructions Rules 90 and 131 open on a quoted defined term
        // (`90. “Secret” and “Top secret” documents shall not…`) and were the
        // only two of 187 the pattern could not see. Curly quotes are folded to
        // ASCII by `text_normalizer::normalize` before this runs. The capital
        // after it stays REQUIRED — the quote is an extra hop, not a bypass.
            regex: r#"(?m)^\s*\[*\s*(?P<num>\d+[A-Z]?)\.-?(?:\s+[\[\*]?|\[)\s*["']?[A-Z]"#,
            level: NodeLevel::Article,
            citation_fmt: "Section {num}",
        },
        // RC-A (audit 2026-06-29): provision number on the same line as its first
        // sub-clause marker — `3. (i) There shall be ...`, `5. (1) ...`, `7. (a) ...`.
        // The `[A-Z]`-after-`N.` patterns above miss a `(` opener, so Sindh Rules of
        // Business 1986 Rules 3-8/13/14/.. were absorbed into the preceding section.
        // D4: the sub-clause opener may sit tight against the dot (`58.(1)`) or
        // behind an amendment bracket (`56. [(1)]`, `73. [(1)`). Constitution
        // Articles 56, 58, 73 were invisible on all three counts.
        HeadingPattern {
            // `\[*` + `[\[\*]?` for the same two shapes as above — this is the
            // pattern Article 90 (`[[90. (1) …`) needs, since its text opens
            // with a sub-clause marker rather than a capital letter.
            regex: r"(?m)^\s*\[*\s*(?P<num>\d+[A-Z]?)\.\s*[\[\*]?\s*\((?:\d+|[ivxlcdm]+|[a-z])\)",
            level: NodeLevel::Article,
            citation_fmt: "Section {num}",
        },
        HeadingPattern {
            regex: r"(?m)^(?P<num>\d+\.\d+)\s+(?P<title>[A-Z][^\n]+)",
            level: NodeLevel::Clause,
            citation_fmt: "Section {num}",
        },
    ],
    min_confidence: 3.0,
};

#[cfg(test)]
mod tests {
    use super::*;
    use regex_lite::Regex;

    /// Does any Article-level heading pattern match this line?
    fn article_matches(line: &str) -> bool {
        MANUAL
            .heading_patterns
            .iter()
            .filter(|p| p.level == NodeLevel::Article)
            .any(|p| Regex::new(p.regex).unwrap().is_match(line))
    }

    /// 2026-07-29 — amendment notation the patterns used to miss. Both lines
    /// are copied verbatim from the live Constitution extraction, where the
    /// single-bracket neighbours (91, 271, 273) were detected and these were
    /// not, so the text was absorbed into the previous Article.
    #[test]
    fn amendment_markers_do_not_hide_a_provision() {
        // Article 90: substituted twice → NESTED brackets.
        assert!(
            article_matches("[[90. (1) Subject to the Constitution, the executive authority of"),
            "double amendment bracket must not hide Article 90"
        );
        // Article 272: footnote marker between the space and the text.
        assert!(
            article_matches("272. *Notwithstanding anything contained in the Constitution,"),
            "footnote asterisk must not hide Article 272"
        );
        // The single-bracket neighbours must keep working.
        assert!(article_matches(" [91. (1) There shall be a Cabinet of Ministers, with the Prime"));
        assert!(article_matches("[271. (1) Notwithstanding anything contained in the Constitution,"));
    }

    /// 2026-07-29 — a provision may open on a QUOTED defined term. Lines copied
    /// verbatim from the live Secretariat Instructions extraction (curly quotes
    /// already folded to ASCII by `text_normalizer::normalize`, which runs
    /// first). Rules 90 and 131 were the only two of 187 the patterns could not
    /// see; their neighbours 89/91/130/132 were detected, so the text was
    /// absorbed into the preceding rule.
    #[test]
    fn a_quoted_defined_term_does_not_hide_a_provision() {
        assert!(
            article_matches("90. \"Secret\" and \"Top secret\" documents shall not be dealt within the"),
            "opening quote must not hide Rule 90"
        );
        assert!(
            article_matches("131. \"PUC\" (i.e. fresh receipt) will be submitted on relevant numbered"),
            "opening quote must not hide Rule 131"
        );
        // The neighbours that already worked must keep working.
        assert!(article_matches("89. In classifying documents as 'confidential' 'secret' or 'Top"));
        assert!(article_matches("91. Incoming covers which are marked 'confidential', 'secret' or"));
    }

    /// Both layers together, on text copied VERBATIM from the live extraction —
    /// curly quotes, column indent and all. The fix needs `normalize` (space
    /// repair before a quote) and the heading pattern (quote before the capital)
    /// to compose; asserting either alone tests the wrong half. Rule 90 is
    /// already spaced and needs only the pattern; Rule 131 is glued and needs
    /// both.
    #[test]
    fn quoted_provisions_survive_normalize_then_scan() {
        let raw = "                                89. In classifying documents as \u{2018}confidential\u{2019} \u{2018}secret\u{2019} and\n\
                   \x20                                90. \u{201C}Secret\u{201D} and \u{201C}Top secret\u{201D} documents shall not be dealt within the\n\
                                   91. Incoming covers which are marked \u{2018}confidential\u{2019} shall be sent unopened\n\
                                   130.A part file may be opened when the main file is not likely to be\n\
                                   131.\u{201C}PUC\u{201D} (i.e. fresh receipt) will be submitted on relevant numbered\n\
                                   132.The contents of correspondence file should be page numbered in\n";
        let text = crate::services::text_normalizer::normalize(raw);
        let hits = crate::services::heading_detector::regex_pass::scan_with_profile(&text, &MANUAL);
        let nums: Vec<&str> = hits.iter().map(|h| h.label.as_str()).collect();
        for want in ["Section 89", "Section 90", "Section 91", "Section 130", "Section 131", "Section 132"] {
            assert!(nums.contains(&want), "{want} missing from {nums:?}");
        }
    }

    /// Rule 153 prints as `153 This category shall include…` — the dot is
    /// MISSING in the source document. That is a typo in the PDF, not a parser
    /// defect, and it must stay unmatched: accepting `N ` + capital as a
    /// provision would mint a label from any sentence opening with a number.
    /// Asserted so a future "let's be more tolerant" change has to argue with a
    /// failing test rather than discover this in production.
    #[test]
    fn a_provision_number_without_its_dot_is_not_a_heading() {
        assert!(!article_matches("153 This category shall include files which have limited utility"));
        assert!(!article_matches("12 The Secretary shall consider the matter and decide"));
    }

    /// A 4-digit YEAR must never be minted as a provision (`Section 1996`).
    ///
    /// Two layers defend this, and it matters which is which:
    ///   * the PATTERN refuses a year GLUED to its sentence (`1996.The Act`) —
    ///     that is why the dot is followed by `\s+` or a bracket, never `\s*`;
    ///   * the SCAN drops any `num` in 1800–2099 (`regex_pass::looks_like_year`),
    ///     which is what stops the spaced form `1996. The Act`.
    /// The spaced form has always matched the pattern, so asserting on the
    /// pattern alone tests the wrong layer — check the scan's output instead.
    #[test]
    fn a_year_is_still_not_a_provision() {
        // Layer 1 — glued year stays unmatched even with the new tolerances.
        assert!(!article_matches("1996.The Act was passed"));

        // Layer 2 — nothing the scan emits may be labelled with a year, for
        // the plain, asterisk and bracket forms alike.
        let text = "1996. The Act was passed\n\
                    1996. *The Act was passed\n\
                    [[1996. (1) The Act was passed\n";
        let hits = crate::services::heading_detector::regex_pass::scan_with_profile(text, &MANUAL);
        assert!(
            hits.iter().all(|h| !h.label.contains("1996")),
            "a year was minted as a provision: {:?}",
            hits.iter().map(|h| &h.label).collect::<Vec<_>>()
        );
    }

    #[test]
    fn detection_matches_manual_corpus() {
        // detection_patterns identify the DOCUMENT TYPE — they fire on
        // manual-identifying vocabulary (Manual / Instruction / Secretariat /
        // CHAPTER), NOT on bare headings. A bare "1. TITLE" appears in Acts and
        // Rules too, so it must NOT be a manual-detection signal — the previous
        // version of this test asserted exactly that (a category error) and so
        // failed perpetually against correct product behaviour.
        let detect_positives = [
            "CHAPTER II",
            "Secretariat Instructions Manual",
            "Manual of Secretariat Instructions",
        ];
        for s in detect_positives {
            assert!(
                MANUAL.detection_patterns.iter().any(|p| Regex::new(p.regex).unwrap().is_match(s)),
                "no manual detection pattern matched: {s:?}",
            );
        }
    }

    /// D4 (2026-07-27): amendment brackets and tight sub-clause openers.
    ///
    /// The Constitution (amended to the 27th) routes to THIS profile — its title
    /// names no instrument type, so body scoring decides, and `manual` wins
    /// 160/117 over `act`. Articles 56, 58, 71 and 73 print in forms the old
    /// patterns could not anchor, so all four were invisible:
    ///
    ///     56. [(1)] The President may address either House
    ///     [58.(1) The President shall dissolve the National Assembly
    ///     71. [Mediation Committee] Omitted by the Constitution
    ///     73. [(1) Notwithstanding anything contained in Article 70
    #[test]
    fn amendment_brackets_and_tight_openers_anchor() {
        let cases = [
            "56. [(1)] The President may address either House",
            "[58.(1) The President shall dissolve the National Assembly",
            "71. [Mediation Committee] Omitted by the Constitution",
            "73. [(1) Notwithstanding anything contained in Article 70",
            // Forms that already worked must keep working.
            "54. (1) The President may, from time to time, summon either",
            "65. A person elected to a House shall not sit or vote",
        ];
        for s in cases {
            assert!(
                MANUAL.heading_patterns.iter().any(|p| Regex::new(p.regex).unwrap().is_match(s)),
                "no manual heading pattern matched: {s:?}",
            );
        }
    }

    /// The relaxation must not turn ordinary prose into headings. A bare `\s*`
    /// after the dot would match `1996.The Act`, which is why the pattern
    /// requires either whitespace or a bracket.
    #[test]
    fn relaxed_patterns_do_not_match_prose() {
        let article_pats: Vec<_> = MANUAL
            .heading_patterns
            .iter()
            .filter(|p| p.level == NodeLevel::Article)
            .map(|p| Regex::new(p.regex).unwrap())
            .collect();
        for s in [
            "1996.The Act was amended thereafter",
            "See No.A of the list appended",
            "Rs.5000 shall be paid to the claimant",
            "3.1 Indexing of files",
        ] {
            assert!(
                !article_pats.iter().any(|re| re.is_match(s)),
                "prose wrongly matched as a provision heading: {s:?}",
            );
        }
    }

    #[test]
    fn heading_patterns_match_manual_headings() {
        // The bare ALL-CAPS section heading and the dotted sub-section heading
        // are found by heading_patterns (structure), not detection_patterns.
        let heading_positives = ["1. RECEIPT AND DESPATCH", "1.2 Indexing of files"];
        for s in heading_positives {
            assert!(
                MANUAL.heading_patterns.iter().any(|p| Regex::new(p.regex).unwrap().is_match(s)),
                "no manual heading pattern matched: {s:?}",
            );
        }
    }
}
