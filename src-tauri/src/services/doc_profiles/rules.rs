//! `rules` profile — Rules / Regulations.
//! Part → Rule → Sub-rule → Clause. Detected by `Rule N` lines.

use super::{DocProfile, DetectionPattern, HeadingPattern};
use crate::services::structure_parser::NodeLevel;

pub const RULES: DocProfile = DocProfile {
    doc_type: "rules",
    level_vocab: [
        "document", "part", "chapter", "rule", "subrule", "clause",
    ],
    citation_unit: "Rule",
    salient_units: &["Preamble"],
    // Detection: "Rules" in title/body is the strongest signal that
    // distinguishes Rules from Acts (which mention Ordinance instead).
    detection_patterns: &[
        DetectionPattern { regex: r"\bRules\b", weight: 3, level: NodeLevel::Document },
        DetectionPattern { regex: r"(?m)^Rule\s+\d+[A-Z]?\.", weight: 4, level: NodeLevel::Article },
        DetectionPattern { regex: r"\bsub-rule\b", weight: 2, level: NodeLevel::Clause },
        DetectionPattern { regex: r"(?m)^Part\s+[IVX\d]+\b", weight: 2, level: NodeLevel::Part },
    ],
    heading_patterns: &[
        HeadingPattern {
            regex: r"(?m)^\s*Part\s+(?P<num>[IVX\d]+)(?:\s*[—-]\s*(?P<title>[^\n]+))?",
            level: NodeLevel::Part,
            citation_fmt: "Part {num}",
        },
        HeadingPattern {
            regex: r"(?m)^\s*Rule\s+(?P<num>\d+[A-Z]?)\.\s*(?P<title>[^\n]+)?",
            level: NodeLevel::Article,
            citation_fmt: "Rule {num}",
        },
        // Pakistani marginal style: bare-number rule heading with leading clause.
        // `5.   (1) Something` — also `       5. (1)` after indent. RC-A (audit
        // 2026-06-29): accept roman `(i)`/alpha `(a)` sub-openers (not just numeric
        // `(1)`) and an optional leading `[` amendment bracket — RoB Rules 3-8 etc.
        // open `3. (i) There shall be ...` and were absorbed into the prior unit.
        HeadingPattern {
            // D4 (2026-07-27): the sub-clause opener may sit tight against the
            // dot (`58.(1)`) or behind an amendment bracket (`56. [(1)]`).
        // 2026-07-29: tolerate a HYPHEN after the provision dot (`10.- (1)`).
        // Sindh drafting writes the catchline dash tight against the number, so
        // E&D 1973 Rule 10 prints as `3[10.-  (1) A civil servant aggrieved…`
        // and APT Rules 1974 uses the shape 20 times. The hyphen is consumed,
        // never captured, so labels stay clean.
            regex: r"(?m)^\s*(?:\d*\[)*\s*(?P<num>\d+[A-Z]?)\.-?\s*[\[\*]?\s*\((?:\d+|[ivxlcdm]+|[a-z])\)",
            level: NodeLevel::Article,
            citation_fmt: "Rule {num}",
        },
        // Bare-number rule with title and trailing dashes (Sindh E&D shape):
        // `3.   Grounds for penalty. --- Where ...`
        // `5.   Inquiry Procedure. ---- The following procedure shall be ...`
        // The dashes (`-{2,}`) and capitalized title differentiate real rule
        // headings from incidental footnote/schedule lines (`3. Subs. Ibid.`)
        // that share the bare-number prefix. (Fix #8, 22-prompt eval,
        // 2026-05-25 — was producing 0 rule detections in E&D, collapsing the
        // entire doc into a single 85-chunk "Preamble".)
        HeadingPattern {
            regex: r"(?m)^\s*(?P<num>\d+[A-Z]?)\.\s+(?P<title>[A-Z][^\n.:]{3,100}[.:])\s*-{2,}",
            level: NodeLevel::Article,
            citation_fmt: "Rule {num}",
        },
        // Sindh RoB 1986 shape (OCR'd): bare-number rule heading whose body
        // begins with a legal opener word — "The", "Notwithstanding", "On",
        // "If", "Where", "Subject", "No", "When", "Every", "Any". The opener
        // safe-list is the cheapest way to separate real rule headings from
        // footnote shapes (`3. Subs. Ibid.`, `4. Inserted ibid.`) without
        // negative lookahead, which regex_lite doesn't support. Required by
        // forensic audit 2026-05-28 (§8 of ANALYSIS_v3.md): RoB only produced
        // 9 `Rule N` chunks of an expected ~47 because Rules 9-15 etc. lack
        // either the `Rule N.` prefix (OCR dropped it) or a leading clause
        // paren `(M)` or trailing `----` dashes.
        HeadingPattern {
            regex: r"(?m)^\s*(?P<num>\d+[A-Z]?)\.\s+(?P<title>(?:The|Notwithstanding|On|If|Where|Subject|No|When|Every|Any|All|It)\s+[A-Za-z][^\n]{8,})",
            level: NodeLevel::Article,
            citation_fmt: "Rule {num}",
        },
        // General bare-number provision heading: `N. Title`, `N. Title - body`
        // (single dash), `N. Title. body` — the dominant Sindh procurement-rules
        // style (SPPRA, Delegation, SPSC, Compendium). Modelled on the proven
        // `act.rs` general pattern (those profiles are at 0% mislabel because
        // they carry it; `rules` previously had only four narrow special-cases
        // and missed ~530 headings — see docs/heading-mislabel-defect-diagnosis.md).
        // High recall by design: it also matches schedule form-fields (`1. Name`)
        // and amendment footnotes (`3. Subs. Ibid.`); those are stripped
        // generically by the ascending-sequence filter in
        // `heading_detector::regex_pass::scan_with_profile` (no keyword list).
        // RC-A (audit 2026-06-29): optional leading `[` so amendment-INSERTED rules
        // (`[18. Response Time – ...]`) are detected — SPPRA Rules 18/32 were missed
        // because the bracket preceded the number and absorbed into the prior rule.
        // 2026-07-12 (item 1): tolerate a leading amendment-footnote DIGIT before the
        // bracket (`(?:\d*\[)?`) — E&D 1973 OCRs inserted provisions as `5[12. …`
        // (superscript note marker glued to the `[`), so `\[?` missed Rule 12 and
        // its text was absorbed into Rule 11.
        HeadingPattern {
        // 2026-07-29: optional opening quote before the capital — a provision
        // may open on a quoted defined term (`90. "Secret" …`). Applied to all
        // three profiles; see manual.rs for the case that found it. The capital
        // stays required.
        // 2026-07-29: optional opening BRACKET too, which `act.rs` has always
        // had and this profile lacked. SPPRA Rules 2010 marks a repealed
        // provision as `56. [XXX]` — five of them (56, 59, 60, 69, 86). Without
        // this the marker is absorbed into the PRECEDING rule's chunk, so a
        // query about Rule 55 retrieves text containing "56. [XXX]", and a
        // query about Rule 56 finds nothing and can be answered from its
        // neighbour. Detecting the label lets the corpus say "omitted" instead.
        // 2026-07-29: `.-` catchline dash, as on the sub-clause pattern above
        // (`14.- Vacancies in the under mentioned posts…`, APT Rules 1974).
            regex: r#"(?m)^\s*(?:\d*\[)*\s*(?P<num>\d+[A-Z]?)\.-?\s+\*?\s*(?P<title>["']?\[?[A-Z][^\n]{2,})"#,
            level: NodeLevel::Article,
            citation_fmt: "Rule {num}",
        },
        HeadingPattern {
            regex: r"(?m)^\s*\((?P<num>\d+)\)\s+[A-Z]",
            level: NodeLevel::Clause,
            citation_fmt: "({num})",
        },
        // Alpha sub-clause, incl. the doubled continuation run `(aa)…(hh)` that
        // long definition rules use after `(z)` (SPPRA Rule 2(1) — previously
        // absorbed into `(z)`'s chunk). Nesting audit 2026-07-02.
        HeadingPattern {
            regex: r"(?m)^\s*\((?P<num>[a-z]{1,2})\)\s+",
            level: NodeLevel::SubClause,
            citation_fmt: "({num})",
        },
        // Roman item nested INSIDE an alpha sub-clause — `Rule 16(1)(a)(ii)`,
        // `Rule 2(1)(q)(iii)`. Ambiguous single letters `(i)/(v)/(x)` (and the
        // doubled shape `(ii)`) also match the alpha pattern above; the
        // sequence-aware resolver in `regex_pass::resolve_ambiguous_sub_markers`
        // decides per occurrence — `(i)` after `(h)` stays alpha, `(i)` opening
        // a list under `(l)` becomes a roman item. Nesting audit 2026-07-02.
        HeadingPattern {
            regex: r"(?m)^\s*\((?P<num>i|ii|iii|iv|v|vi|vii|viii|ix|x|xi|xii|xiii|xiv|xv)\)\s+",
            level: NodeLevel::Item,
            citation_fmt: "({num})",
        },
    ],
    min_confidence: 3.0,
};

#[cfg(test)]
mod tests {
    use super::*;
    use regex_lite::Regex;

    /// Does any Article-level heading pattern match this line?
    fn rule_matches(line: &str) -> bool {
        RULES
            .heading_patterns
            .iter()
            .filter(|p| p.level == NodeLevel::Article)
            .any(|p| Regex::new(p.regex).unwrap().is_match(line))
    }

    /// 2026-07-29 — three shapes copied verbatim from live extractions that the
    /// provision patterns could not see, each swallowing its rule into the
    /// preceding one.
    #[test]
    fn omitted_and_dashed_provisions_are_detected() {
        // SPPRA Rules 2010 marks a repealed rule as `[XXX]` — five of them.
        // `act.rs` has always allowed a bracketed title; this profile did not.
        assert!(rule_matches("56. [XXX]"), "omitted-rule marker must mint Rule 56");
        assert!(rule_matches("86. [XXX]"));
        // Catchline dash tight against the dot — E&D 1973 Rule 10 (with an
        // amendment footnote digit and bracket) and APT Rules 1974 Rule 14.
        assert!(
            rule_matches("3[10.-  (1) A civil servant aggrieved by an order of the authorised"),
            "dot-hyphen must not hide E&D Rule 10"
        );
        assert!(
            rule_matches("14.- Vacancies in the under mentioned posts shall be filled on"),
            "dot-hyphen must not hide APT Rule 14"
        );
        // Neighbours that already worked keep working.
        assert!(rule_matches("55. Entry into Force of the Procurement Contract - A procurement"));
        assert!(rule_matches("9. Procedure of inquiry against officers lent to other government"));
    }

    /// The year guard must survive every tolerance added above: a 4-digit year
    /// must never be minted as a provision, in the dashed or bracketed form
    /// either. Layer 1 is the pattern (glued year); layer 2 is
    /// `regex_pass::looks_like_year`, which is what drops the spaced form.
    #[test]
    fn a_year_survives_the_new_tolerances() {
        assert!(!rule_matches("1996.The Act was passed"));
        let text = "1996. The Act was passed\n\
                    1996.- The Act was passed\n\
                    2009.- (1) The Act was passed\n\
                    2010. [XXX]\n";
        let hits = crate::services::heading_detector::regex_pass::scan_with_profile(text, &RULES);
        assert!(
            hits.iter().all(|h| !h.label.contains("1996")
                && !h.label.contains("2009")
                && !h.label.contains("2010")),
            "a year was minted as a rule: {:?}",
            hits.iter().map(|h| &h.label).collect::<Vec<_>>()
        );
    }

    #[test]
    fn detection_matches_rules_corpus() {
        let positives = [
            "Rule 9. The Secretary shall arrange...",
            "Rule 12. Reconciliation of receipts",
            "Part III — Procedure",
        ];
        for s in positives {
            assert!(
                RULES.detection_patterns.iter().any(|p| Regex::new(p.regex).unwrap().is_match(s)),
                "no rules detection pattern matched: {s:?}",
            );
        }
    }

    #[test]
    fn citation_unit_is_rule() {
        assert_eq!(RULES.citation_unit, "Rule");
        assert_eq!(RULES.level_vocab[NodeLevel::Article as usize], "rule");
        assert_eq!(RULES.level_vocab[NodeLevel::Clause as usize], "subrule");
    }

    /// Forensic audit 2026-05-28 — RoB 1986 OCR rule headings.
    /// Real RoB rule headings (`9. The Chief Minister may...`, `11.
    /// Notwithstanding...`, `15. On Finanee Department's refusal...`) must
    /// match. Footnote shapes (`3. Subs. Ibid.`) must NOT.
    #[test]
    fn bare_number_rule_with_legal_opener_detects_rob_headings() {
        let opener_pat = RULES
            .heading_patterns
            .iter()
            .find(|p| p.regex.contains("Notwithstanding"))
            .expect("RoB opener pattern present");
        let re = Regex::new(opener_pat.regex).unwrap();
        let positives = [
            "9.  The Chief Minister may in his discretion require any case to be submitted to the Cabinet for decision.",
            "10. The Chief Minister may call a meeting of the Cabinet on the date, time and place fixed by him.",
            "11. Notwithstanding anything contained in these rules, the Chief Minister may waive reference to the Cabinet.",
            "15. On Finanee Department's refusal to accord concurrence to any case, the Department may submit ...",
        ];
        for s in positives {
            assert!(re.is_match(s), "expected match for: {s:?}");
        }
        let negatives = [
            "3.    Subs.    Ibid.",
            "4.    Inserted ibid.",
            "1.    Sub rules (5) and (6) of rules 5 added vide ...",
            "5.   Inquiry Procedure. ---- The following procedure ...",
        ];
        for s in negatives {
            assert!(!re.is_match(s), "expected NO match for: {s:?}");
        }
    }

    /// Fix #8 — bare-number rule with title + trailing dashes (Sindh E&D
    /// shape). Real headings must match; footnote/schedule lines that share
    /// the bare-number prefix must NOT.
    #[test]
    fn bare_number_rule_with_title_and_dashes_detects_real_headings() {
        let positives = [
            "3.   Grounds for penalty. --- Where a civil servant ...",
            "4.   Penalties. ---- (1) the following are the minor ...",
            "5.   Inquiry Procedure. ---- The following procedure ...",
            "7.   Powers of Inquiry Officer and Inquiry Committee: --- (1) ...",
        ];
        let title_pat = RULES
            .heading_patterns
            .iter()
            .find(|p| p.regex.contains("-{2,}"))
            .expect("title+dashes pattern present");
        let re = Regex::new(title_pat.regex).unwrap();
        for s in positives {
            assert!(re.is_match(s), "expected match for: {s:?}");
        }
        // Footnote-style false positives — bare-number lines without the
        // title-dashes shape — must be excluded.
        let negatives = [
            "3.    Subs.    Ibid.",
            "4.    Inserted ibid.",
            "1.    Sub rules (5) and (6) of rules 5 added vide ...",
        ];
        for s in negatives {
            assert!(!re.is_match(s), "expected NO match for footnote line: {s:?}");
        }
    }
}
