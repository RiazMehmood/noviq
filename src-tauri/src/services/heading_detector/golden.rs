//! Golden extraction set — the pages that have actually broken this pipeline.
//!
//! # Why this exists
//!
//! Every extraction defect through 2026-07 was found the same way: index the
//! whole corpus on prod, run `scripts/corpus_gap_check.sql`, notice a number
//! missing, and work backwards. That loop is slow, needs a re-index per attempt,
//! and it is why the false-table guard took FOUR rounds to get right — each dry
//! run was a manual comparison against live extraction.
//!
//! This is the fast loop. Each fixture is one page of one real document, saved
//! verbatim from `pdftotext -layout`, paired with what the heading scan must and
//! must not produce from it. It runs in `cargo test --lib`, needs no database, no
//! PDF, no re-index and no VM.
//!
//! # What it covers, and what it deliberately does not
//!
//! It covers the layer where every defect lived: raw page text →
//! `text_normalizer::normalize` → `regex_pass::scan_with_profile`. That is the
//! seam that turned a footnote into `Article 343`, fused a table's columns, and
//! read schedule serials as rules.
//!
//! It does NOT cover PDF text extraction itself (upstream of the fixture) or
//! chunking and retrieval (downstream). When the extractor changes — the layout
//! work — the fixtures must be REGENERATED, and that is the point: regenerate,
//! run this, and the expectations tell you whether the new extractor is better or
//! merely different.
//!
//! ## Known scope limits (found by the set itself, on its first run)
//!
//! A fixture is ONE PAGE of RAW `pdftotext` output, which excludes three things
//! the real pipeline does. This was not obvious until the set ran and disagreed
//! with reality — the first four "failures" were wrong expectations, not bugs:
//!
//!  1. **Document-global rules cannot fire.** `drop_footnote_tail_outliers` needs
//!     ≥20 distinct provision numbers to judge a tail outlier; one page has a
//!     handful. So `Article 343` IS suppressed on the real document (verified:
//!     280 Articles, zero `Article 343`) and is NOT suppressed here.
//!  2. **Tables are not stripped.** `structured_importer::strip_table_regions`
//!     lifts table regions out BEFORE heading detection; the raw fixture still
//!     contains them, so contents-table rows survive that never reach the scan in
//!     production.
//!  3. **The citation-unit override is not applied.** `scan_with_profile` emits
//!     the profile's default word, so a `manual` document yields `Section 90`
//!     here and `Rule 90` in prod, where `resolve_citation_unit` re-mints it.
//!
//! Cases blocked by (1) and (2) are marked `xfail` with the reason stated as a
//! FIXTURE-SCOPE limit rather than a defect — the distinction matters, because
//! "known broken" and "not exercised at this layer" call for different work.
//! Closing them means whole-document fixtures and sidecar table markers.
//!
//! # Expected failures are recorded, not hidden
//!
//! Two cases are known-failing and marked `xfail` with the reason. A known
//! defect that is written down is a decision; one that is quietly absent from
//! the suite is a surprise waiting for a customer. `xfail` cases do not fail the
//! run, but a case that starts PASSING is reported loudly — that means someone
//! fixed it and the marker should go.
//!
//! # Regenerating
//!
//! `scripts/build_golden_fixtures.py` on the VM (it needs the source PDFs).
//! Fixtures live in `src-tauri/tests/golden/`.

#![cfg(test)]

use std::path::PathBuf;

use crate::services::doc_profiles::{self, DocProfile};
use crate::services::heading_detector::regex_pass::scan_with_profile;
use crate::services::structure_parser::NodeLevel;

/// Which profile a fixture's document uses (mirrors `sources.doc_type`).
#[derive(Clone, Copy, Debug)]
enum Profile {
    Act,
    Rules,
    Manual,
}

impl Profile {
    fn get(self) -> &'static DocProfile {
        match self {
            Profile::Act => &doc_profiles::act::ACT,
            Profile::Rules => &doc_profiles::rules::RULES,
            Profile::Manual => &doc_profiles::manual::MANUAL,
        }
    }
}

/// One page, and what the scan must make of it.
struct Case {
    /// Fixture file stem under `tests/golden/`.
    fixture: &'static str,
    profile: Profile,
    /// Provision labels that MUST be produced. These are the recoveries.
    expect: &'static [&'static str],
    /// Labels that must NOT be produced. These are the fabrications — a footnote
    /// number, a table serial, a sentence that merely starts with a digit.
    /// This half is the more important one: a missing provision is a gap, an
    /// invented one is a citation to law that does not exist.
    forbid: &'static [&'static str],
    /// Known-failing, with the reason. Reported, never silently skipped.
    xfail: Option<&'static str>,
}

const CASES: &[Case] = &[
    // ── fabrications: things that must never become provisions ───────────────
    Case {
        // Footnote 343 on pp.168-170 became `Article 343` and swallowed ~14
        // chunks of real Article 234 text. Fixed by the tail-outlier rule.
        fixture: "constitution_footnote_343",
        profile: Profile::Manual,
        expect: &[],
        forbid: &["Article 343", "Section 343", "Rule 343"],
        xfail: Some(
            "NOT a code defect — a fixture-scope limit. `drop_footnote_tail_outliers`              is DOCUMENT-GLOBAL: it needs >=20 distinct provision numbers to judge a              tail outlier, and one page has a handful. Verified working on the full              Constitution (280 Articles, zero `Article 343`). Needs a whole-document              fixture to exercise here.",
        ),
    },
    Case {
        // `153 This category shall include…` — the dot is MISSING in the source.
        // A typo in the PDF, not a parser defect. Accepting `N ` + capital would
        // mint a provision from any sentence opening with a number.
        fixture: "secretariat_no_dot_153",
        profile: Profile::Manual,
        expect: &["Section 152"],
        forbid: &["Section 153"],
        xfail: None,
    },
    Case {
        // The appointing-authority table's S. No. column (1,2,3…) read as rules,
        // colliding with the document's real Rules 1,2,3.
        fixture: "apt_appointing_table",
        profile: Profile::Rules,
        expect: &["Rule 3", "Rule 4"],
        forbid: &[],
        // Passes at PAGE scope: on this page the real Rules 3 and 4 are found,
        // and that is not the defect. The live defect is a COLLISION across the
        // whole document — the schedule's serials mint Rule 3/4 elsewhere too, so
        // each label names two different things. Page-scope fixtures cannot see
        // that, so this case guards the recovery only; the collision remains open.
        xfail: None,
    },
    Case {
        // A contents listing on pp.80-81 whose rows look like provisions.
        fixture: "dev_manual_toc_table",
        profile: Profile::Manual,
        expect: &[],
        forbid: &["Section 24", "Section 25", "Section 26", "Section 27"],
        xfail: Some(
            "NOT a code defect — a fixture-scope limit. In the real pipeline these              rows never reach heading detection: `structured_importer::             strip_table_regions` lifts the table out first (in prod they are stored              as `Table (p.80) row N`). The fixture is raw pdftotext with no table              stripping, so the rows survive. Exercising this needs the sidecar's              table markers in the fixture.",
        ),
    },
    // ── recoveries: shapes that hid a real provision ─────────────────────────
    Case {
        // `142.Subject to the Constitution` — pdfplumber drops the space, so the
        // provision was absorbed into Article 141.
        fixture: "constitution_glued_142",
        profile: Profile::Manual,
        expect: &["Section 142"],
        forbid: &[],
        xfail: None,
    },
    Case {
        // `90. "Secret" and "Top secret" documents…` — opens on a quoted defined
        // term, not a capital, so the pattern skipped it.
        fixture: "secretariat_quoted_rule_90",
        profile: Profile::Manual,
        expect: &["Section 89", "Section 90", "Section 91"],
        forbid: &[],
        xfail: None,
    },
    Case {
        // `131."PUC" (i.e. fresh receipt)` — glued AND quoted, so both the space
        // repair and the pattern had to change.
        fixture: "secretariat_glued_quote_131",
        profile: Profile::Manual,
        expect: &["Section 130", "Section 131", "Section 132"],
        forbid: &[],
        xfail: None,
    },
    Case {
        // `56. [XXX]` — a repealed rule. Absorbed into Rule 55 until the
        // bracketed-title form was allowed.
        fixture: "sppra_omitted_xxx",
        profile: Profile::Rules,
        expect: &["Rule 55", "Rule 56", "Rule 57"],
        forbid: &[],
        xfail: None,
    },
    Case {
        // `3[10.-  (1) A civil servant aggrieved…` — amendment digit, bracket,
        // and a catchline dash tight against the dot.
        fixture: "ed_dot_hyphen_rule_10",
        profile: Profile::Rules,
        expect: &["Rule 10"],
        forbid: &[],
        xfail: None,
    },
    Case {
        // `13.    13. The Chief Minister may approve…` — the number is printed
        // twice. Deliberately unfixed: matching it needs "number, any other
        // number, text", and regex_lite has no backreference to require the
        // repeat be the SAME number, so a table row would shadow real headings.
        fixture: "rob_doubled_number_13",
        profile: Profile::Rules,
        expect: &["Rule 13"],
        forbid: &[],
        xfail: Some(
            "the provision number is printed twice on the line; matching it would \
             loosen the pattern corpus-wide for one rule in one document",
        ),
    },
    Case {
        // A clean, well-formed page. If this ever breaks, the change was not a
        // targeted fix — it altered the ordinary case.
        //
        // Located by the PROVISION TEXT, not the catchline: "75. Commercial
        // Schemes" also appears in the table of contents, and the first
        // generation of this fixture silently captured the TOC page instead —
        // which would have asserted "Section 75" against a page where the
        // dotted-leader filter is *supposed* to suppress it.
        fixture: "slga_clean_baseline",
        profile: Profile::Act,
        expect: &["Section 75"],
        forbid: &[],
        xfail: None,
    },
    Case {
        // The other half of that discovery: a contents page must produce NO
        // provisions at all. Every line there looks exactly like a catchline,
        // and only the dotted leader distinguishes it — this is what
        // `heading_line_is_dotted` defends, and the poison-guard comment in
        // `drop_out_of_sequence_articles` records what happened when a contents
        // page did leak into the body (max_num past 280, ~100 real labels
        // deleted as "out of sequence").
        fixture: "slga_toc_page",
        profile: Profile::Act,
        expect: &[],
        forbid: &["Section 73", "Section 74", "Section 75", "Section 76"],
        xfail: None,
    },
];

fn fixture_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests").join("golden")
}

/// Labels the scan produces for a fixture, after the SAME normalisation the
/// importer applies. Normalising here is essential, not incidental: several
/// defects were repairs in `normalize` (the space before a glued provision,
/// curly→ASCII quotes) and asserting on raw page text would test the wrong layer.
fn labels_for(case: &Case, raw: &str) -> Vec<String> {
    let text = crate::services::text_normalizer::normalize(raw);
    scan_with_profile(&text, case.profile.get())
        .into_iter()
        .filter(|h| h.level == NodeLevel::Article)
        .map(|h| h.label)
        .collect()
}

/// The whole set in one test, so the report is a single table rather than a
/// dozen separate failures with no overview.
#[test]
fn golden_extraction_set() {
    let dir = fixture_dir();
    let mut missing: Vec<&str> = Vec::new();
    let mut failures: Vec<String> = Vec::new();
    let mut unexpected_passes: Vec<&str> = Vec::new();
    let mut checked = 0usize;

    for case in CASES {
        let path = dir.join(format!("{}.txt", case.fixture));
        let Ok(raw) = std::fs::read_to_string(&path) else {
            missing.push(case.fixture);
            continue;
        };
        checked += 1;
        let labels = labels_for(case, &raw);

        let mut problems: Vec<String> = Vec::new();
        for want in case.expect {
            if !labels.iter().any(|l| l == want) {
                problems.push(format!("missing {want:?}"));
            }
        }
        for deny in case.forbid {
            if labels.iter().any(|l| l == deny) {
                problems.push(format!("FABRICATED {deny:?}"));
            }
        }

        match (case.xfail, problems.is_empty()) {
            // Known failure, still failing — expected, not reported as a fault.
            (Some(_), false) => {}
            // Known failure that now passes: someone fixed it. Say so loudly so
            // the marker is removed rather than quietly masking a regression later.
            (Some(_), true) => unexpected_passes.push(case.fixture),
            (None, false) => failures.push(format!(
                "  {}: {}\n      got: {:?}",
                case.fixture,
                problems.join("; "),
                labels
            )),
            (None, true) => {}
        }
    }

    if !missing.is_empty() {
        eprintln!(
            "golden set: {} fixture(s) not present, skipped: {:?}\n  \
             regenerate with scripts/build_golden_fixtures.py (needs the source PDFs)",
            missing.len(),
            missing
        );
    }
    if !unexpected_passes.is_empty() {
        eprintln!(
            "golden set: {:?} now PASS but are marked xfail — remove the marker",
            unexpected_passes
        );
    }

    assert!(
        failures.is_empty(),
        "golden extraction set: {} of {} checked case(s) failed\n{}",
        failures.len(),
        checked,
        failures.join("\n")
    );
}

/// The set is worthless if it silently checks nothing — a wrong fixture path or
/// a lost directory would make `golden_extraction_set` pass on zero cases.
#[test]
fn golden_fixtures_are_present() {
    let dir = fixture_dir();
    let found = CASES
        .iter()
        .filter(|c| dir.join(format!("{}.txt", c.fixture)).exists())
        .count();
    assert!(
        found * 2 >= CASES.len(),
        "only {found} of {} golden fixtures found under {} — the set is not \
         actually being exercised; regenerate with scripts/build_golden_fixtures.py",
        CASES.len(),
        dir.display(),
    );
}
