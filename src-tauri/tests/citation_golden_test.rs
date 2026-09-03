//! Golden tests for the officer-grade citation formatter (constitution §XI).
//!
//! Each test fixes one (input → expected formatted string) pair. The
//! formatter implementation in `services::citation::formatter` must produce
//! the exact string shown. Do not soften the goldens — patch the
//! implementation instead.

use echo_ai_lib::services::citation::{AmendmentKind, AmendmentNote, LegalCitation, PathSegment};

fn act_rti() -> LegalCitation {
    LegalCitation {
        instrument_title: "The Right of Access to Information Act, 2017".into(),
        instrument_id: Some("Act No. XXXIV of 2017".into()),
        year: Some(2017),
        path: vec![],
        amendment: None,
    }
}

#[test]
fn case_01_constitution_article() {
    let c = LegalCitation {
        instrument_title: "The Constitution of the Islamic Republic of Pakistan, 1973".into(),
        instrument_id: None,
        year: Some(1973),
        path: vec![PathSegment::Article("25".into())],
        amendment: None,
    };
    assert_eq!(
        c.format_officer(),
        "The Constitution of the Islamic Republic of Pakistan, 1973, Article 25",
    );
}

#[test]
fn case_02_act_section_only() {
    let mut c = act_rti();
    c.path = vec![PathSegment::Section("6".into())];
    assert_eq!(
        c.format_officer(),
        "The Right of Access to Information Act, 2017 (Act No. XXXIV of 2017), Section 6",
    );
}

#[test]
fn case_03_act_section_subsection() {
    let mut c = act_rti();
    c.path = vec![
        PathSegment::Section("6".into()),
        PathSegment::SubSection("2".into()),
    ];
    assert_eq!(
        c.format_officer(),
        "The Right of Access to Information Act, 2017 (Act No. XXXIV of 2017), Section 6(2)",
    );
}

#[test]
fn case_04_act_section_subsection_clause() {
    let mut c = act_rti();
    c.path = vec![
        PathSegment::Section("6".into()),
        PathSegment::SubSection("2".into()),
        PathSegment::Clause("a".into()),
    ];
    assert_eq!(
        c.format_officer(),
        "The Right of Access to Information Act, 2017 (Act No. XXXIV of 2017), Section 6(2)(a)",
    );
}

#[test]
fn case_05_act_full_path_to_subclause() {
    let mut c = act_rti();
    c.path = vec![
        PathSegment::Section("6".into()),
        PathSegment::SubSection("2".into()),
        PathSegment::Clause("a".into()),
        PathSegment::SubClause("i".into()),
    ];
    assert_eq!(
        c.format_officer(),
        "The Right of Access to Information Act, 2017 (Act No. XXXIV of 2017), Section 6(2)(a)(i)",
    );
}

#[test]
fn case_06_amended_substituted() {
    let mut c = act_rti();
    c.path = vec![
        PathSegment::Section("6".into()),
        PathSegment::SubSection("2".into()),
        PathSegment::Clause("a".into()),
    ];
    c.amendment = Some(AmendmentNote {
        kind: AmendmentKind::Substituted,
        by_instrument: "Act XII of 2021".into(),
    });
    assert_eq!(
        c.format_officer(),
        "The Right of Access to Information Act, 2017 (Act No. XXXIV of 2017), Section 6(2)(a) — substituted by Act XII of 2021",
    );
}

#[test]
fn case_07_inserted_new_section() {
    let mut c = act_rti();
    c.path = vec![PathSegment::Section("6A".into())];
    c.amendment = Some(AmendmentNote {
        kind: AmendmentKind::Inserted,
        by_instrument: "Act XII of 2021".into(),
    });
    assert_eq!(
        c.format_officer(),
        "The Right of Access to Information Act, 2017 (Act No. XXXIV of 2017), Section 6A — inserted by Act XII of 2021",
    );
}

#[test]
fn case_08_repealed_section() {
    let mut c = act_rti();
    c.path = vec![PathSegment::Section("7".into())];
    c.amendment = Some(AmendmentNote {
        kind: AmendmentKind::Repealed,
        by_instrument: "Ordinance V of 2022".into(),
    });
    assert_eq!(
        c.format_officer(),
        "The Right of Access to Information Act, 2017 (Act No. XXXIV of 2017), Section 7 — repealed by Ordinance V of 2022",
    );
}

#[test]
fn case_09_rules_instrument() {
    let c = LegalCitation {
        instrument_title: "The Sindh Local Government Rules, 2018".into(),
        instrument_id: None,
        year: Some(2018),
        path: vec![
            PathSegment::Section("4".into()),
            PathSegment::SubSection("2".into()),
        ],
        amendment: None,
    };
    assert_eq!(
        c.format_officer(),
        "The Sindh Local Government Rules, 2018, Rule 4(2)",
    );
}

#[test]
fn case_10_chapter_only() {
    let mut c = act_rti();
    c.path = vec![PathSegment::Chapter("III".into())];
    assert_eq!(
        c.format_officer(),
        "The Right of Access to Information Act, 2017 (Act No. XXXIV of 2017), Chapter III",
    );
}

#[test]
fn case_11_schedule_item() {
    let mut c = act_rti();
    c.path = vec![
        PathSegment::Schedule("II".into()),
        PathSegment::ScheduleItem("3".into()),
    ];
    assert_eq!(
        c.format_officer(),
        "The Right of Access to Information Act, 2017 (Act No. XXXIV of 2017), Schedule II, Item 3",
    );
}

#[test]
fn case_12_short_form_inline() {
    let mut c = act_rti();
    c.path = vec![
        PathSegment::Section("6".into()),
        PathSegment::SubSection("2".into()),
        PathSegment::Clause("a".into()),
    ];
    assert_eq!(c.format_short(), "RTI Act 2017 §6(2)(a)");
}
