//! Feature 009 Phase D — ingestion gate acceptance (T068, FR-010/010a).
//!
//! Runs the verbatim-verify gate pipeline over a synthetic real-instrument
//! fixture (numbered running paragraphs, letterhead, dateline, C.C., inline
//! hyphenated citations — the exact shapes the old heuristic failed on) with a
//! hand-written "model proposal", WITHOUT a live LLM. Asserts the contract
//! checks from contracts/drafting-commands.md:
//!   (a) both inline citations land in cited slots with a verbatim provision_ref;
//!   (b) letterhead/dateline/C.C./signature regions are never cited slots;
//!   (c) every accepted slot's text is a verbatim substring of the fixture.

use echo_ai_lib::services::drafting::ingest::propose_schema_from_model;
use echo_ai_lib::services::drafting::SlotKind;

fn fixture() -> String {
    let p = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/lgd_letter_fixture.md");
    std::fs::read_to_string(p).expect("read lgd_letter_fixture.md")
}

/// A faithful model proposal over the fixture (what the 7B produced on the real
/// gold in the 2026-06-10 spike, transposed to the fixture's content).
fn canned_model_output() -> String {
    serde_json::json!([
        {
            "kind": "boilerplate",
            "label": "letterhead",
            "provision": null,
            "text": "GOVERNMENT OF SINDH\nSERVICES, GENERAL ADMINISTRATION & COORDINATION DEPARTMENT\n(REGULATIONS WING)"
        },
        {
            "kind": "boilerplate",
            "label": "reference and dateline",
            "provision": null,
            "text": "NO.RW/1-23/2025: Karachi, dated 14th November, 2025"
        },
        {
            "kind": "boilerplate",
            "label": "addressee",
            "provision": null,
            "text": "To,\nThe Secretary,\nSindh Public Service Commission,\nHyderabad."
        },
        {
            "kind": "prose",
            "label": "introduction",
            "provision": null,
            "text": "1. I am directed to refer to the subject noted above and to state that the Government of Sindh has been receiving representations from various candidates seeking relaxation in the upper age limit prescribed for the Combined Competitive Examination, on the ground that the examination could not be held during the preceding years and that the delay was not attributable to the candidates concerned."
        },
        {
            "kind": "cited",
            "label": "enabling provision",
            "provision": "Rule-6(c) of the XYZ Regulations-2023",
            "text": "2. The matter has been examined in this Department in consultation with the relevant record. Under Rule-6(c) of the XYZ Regulations-2023, the competent authority may, for reasons to be recorded in writing, relax the upper age limit prescribed for admission to the examination in favour of a class or category of candidates whose case is found to be one of undue hardship."
        },
        {
            "kind": "cited",
            "label": "law department opinion",
            "provision": "Rule-23 of the ABC Rules 2020",
            "text": "3. The Law Department, on a reference made to it, has furnished the opinion that the proposed relaxation is permissible, provided that the conditions laid down in Rule-23 of the ABC Rules 2020 governing relaxation of rules generally are satisfied, and that the relaxation is extended uniformly to all similarly placed candidates."
        },
        {
            "kind": "fact",
            "label": "affected candidates",
            "provision": null,
            "text": "4. The estimated number of affected candidates, as reported by the Commission, is approximately twelve thousand, and the proposed relaxation would cover the examinations to be held during the calendar year 2026."
        },
        {
            "kind": "prose",
            "label": "proposal",
            "provision": null,
            "text": "5. It is therefore proposed that the competent authority may be pleased to accord approval to the grant of relaxation of two years in the upper age limit, as a one-time dispensation, in the light of the legal position stated above."
        },
        {
            "kind": "boilerplate",
            "label": "signature block",
            "provision": null,
            "text": "(MUHAMMAD AHMED)\nSECTION OFFICER (R-I)"
        },
        {
            "kind": "boilerplate",
            "label": "distribution",
            "provision": null,
            "text": "C.C:\n1. The Principal Secretary to the Chief Minister, Sindh.\n2. The Secretary, Law Department, Government of Sindh.\n3. The Master File."
        }
    ])
    .to_string()
}

fn normalize_ws(s: &str) -> String {
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

#[test]
fn gate_pipeline_holds_the_ingestion_contract_on_a_real_shaped_instrument() {
    let source = fixture();
    let schema =
        propose_schema_from_model(&source, &canned_model_output(), "summary").expect("schema");

    // A faithful proposal passes the gate clean.
    assert!(
        schema.flagged_for_review.is_empty(),
        "faithful proposal must not be flagged: {:?}",
        schema.rejection_reasons
    );

    // (a) BOTH inline citations land in cited slots with verbatim provision_ref.
    let provisions: Vec<&str> = schema
        .slots
        .iter()
        .filter(|s| s.kind == SlotKind::Cited)
        .filter_map(|s| s.provision_ref.as_deref())
        .collect();
    assert!(provisions.contains(&"Rule-6(c) of the XYZ Regulations-2023"), "{provisions:?}");
    assert!(provisions.contains(&"Rule-23 of the ABC Rules 2020"), "{provisions:?}");
    let src_norm = normalize_ws(&source);
    for prov in &provisions {
        assert!(src_norm.contains(&normalize_ws(prov)), "provision not verbatim: {prov}");
    }

    // Categories derive from the matched provisions, not headings.
    let cats: Vec<&str> = schema
        .slots
        .iter()
        .filter_map(|s| s.required_source_category.as_deref())
        .collect();
    assert!(cats.iter().any(|c| c.contains("rule-6(c)")), "{cats:?}");
    assert!(cats.iter().any(|c| c.contains("rule-23")), "{cats:?}");

    // (b) letterhead/dateline/C.C./signature regions are NOT cited slots.
    for s in &schema.slots {
        if let Some(text) = s.boilerplate_text.as_deref() {
            assert_eq!(s.kind, SlotKind::Boilerplate);
            assert!(s.provision_ref.is_none(), "boilerplate must carry no provision: {text}");
        }
    }
    let cited_count = schema.slots.iter().filter(|s| s.kind == SlotKind::Cited).count();
    // 2 real citations + 1 fact slot (fact rides the cited kind with `fact:`).
    assert_eq!(cited_count, 3, "exactly the two citations + one fact slot are cited-kind");

    // (c) every accepted slot's text is a verbatim substring of the fixture.
    for s in &schema.slots {
        if let Some(t) = s.boilerplate_text.as_deref() {
            assert!(src_norm.contains(&normalize_ws(t)), "not verbatim: {t}");
        }
    }

    // All slots remain valid for confirm_template.
    for s in &schema.slots {
        assert!(s.validate().is_ok(), "invalid slot: {s:?}");
    }
}

#[test]
fn unfaithful_proposal_is_flagged_never_silently_accepted() {
    let source = fixture();
    // The model paraphrases para 2 and invents a provision — both gate checks fire.
    let bad = serde_json::json!([
        {
            "kind": "cited",
            "label": "enabling provision",
            "provision": "Rule-99 of the Invented Rules 1900",
            "text": "The department reviewed the matter and may relax the age limit."
        }
    ])
    .to_string();
    let schema = propose_schema_from_model(&source, &bad, "summary").expect("schema");
    assert_eq!(schema.flagged_for_review, vec![0]);
    assert!(schema.slots[0].provision_ref.is_none());
    let reason = &schema.rejection_reasons[&0];
    assert!(reason.contains("not a verbatim substring"), "reason: {reason}");
}
