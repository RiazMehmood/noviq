use echo_ai_lib::services::chunker::chunk_tree;
use echo_ai_lib::services::heading_detector::{regex_pass, tree};

#[test]
fn legal_pdf_text_yields_three_levels_with_parent_links() {
    // NOTE: each section body needs ≥80 non-whitespace chars to clear
    // the TOC-stripping guard introduced by db8688f4 (chunker.rs:491).
    // Short toy fixtures are otherwise collapsed to "TOC page" and skipped.
    let text = "\
Sindh Civil Servants Act 1973\n\n\
This Act consolidates and amends the law relating to civil servants in the Province of Sindh.\n\n\
Section 8. Recruitment.\n\n\
A person shall be eligible for recruitment to the civil service of the Province only if the prescribed conditions of age, qualification, and physical fitness are met, and only after appearing in the competitive examination conducted by the Sindh Public Service Commission under rules made for that purpose.\n\n\
Sub-clause 8(2)(a) further requires that any candidate seeking permission to apply from outside the cadre must obtain prior written permission from the appointing authority, and such permission shall be granted only in exceptional cases for which reasons must be recorded in writing.\n\n\
Section 9. Appointments.\n\n\
The appointing authority shall determine the eligibility of a candidate for appointment to a post in the civil service of the Province in accordance with the recruitment rules in force on the date the vacancy is advertised, and shall communicate the result of selection in writing to the candidate.";

    let headings = regex_pass::scan(text);
    let t = tree::build_tree(text, &headings, tree::BuildOptions {
        document_title: "Sindh Civil Servants Act 1973.pdf".into(),
    });
    let specs = chunk_tree(text, &t);

    let by_type = |ut: &str| specs.iter().filter(|c| c.unit_type == ut).count();
    assert_eq!(by_type("document"), 1);
    assert!(by_type("section") >= 2, "got {} sections", by_type("section"));
    assert!(by_type("leaf") >= 2);

    let doc = specs.iter().find(|c| c.unit_type == "document").unwrap();
    assert!(doc.parent_index.is_none());

    for s in specs.iter().filter(|c| c.unit_type == "section") {
        assert_eq!(s.parent_index, Some(doc.chunk_index));
    }

    for l in specs.iter().filter(|c| c.unit_type == "leaf") {
        let p = l.parent_index.expect("leaf must have parent");
        let parent = specs.iter().find(|c| c.chunk_index == p).unwrap();
        assert_eq!(parent.unit_type, "section");
    }

    assert!(specs.iter().any(|c| c.unit_label.as_deref() == Some("Section 8")));
    assert!(specs.iter().any(|c| c.unit_label.as_deref() == Some("Section 9")));
}

#[test]
fn flat_text_yields_two_levels_only() {
    // Body needs ≥80 non-whitespace chars to clear the TOC-stripping guard
    // introduced by db8688f4 (chunker.rs:491). See `legal_pdf_text_...`.
    let text = "A flat memo with no detectable section or article headings whatsoever. \
                The body is long enough to clear the TOC-stripping guard built into \
                the chunker but contains no structural cues for the heading detector \
                to latch onto, so this exercises the document → leaf fallback path.";
    let headings = regex_pass::scan(text);
    let t = tree::build_tree(text, &headings, tree::BuildOptions {
        document_title: "memo.txt".into(),
    });
    let specs = chunk_tree(text, &t);
    assert_eq!(specs.iter().filter(|c| c.unit_type == "document").count(), 1);
    assert_eq!(specs.iter().filter(|c| c.unit_type == "section").count(), 0);
    assert_eq!(specs.iter().filter(|c| c.unit_type == "leaf").count(), 1);
}
