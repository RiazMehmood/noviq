//! Feature 009 — gap detection, Phase E rewrite (FR-019 two-tier · FR-008a · R11).
//!
//! Coverage is judged against document CONTENT — the command layer embeds each
//! cited slot's required-source category and compares it with the documents'
//! indexed chunk embeddings — never against document titles (the shipped
//! title-token overlap over-reported ~10–11 false gaps on an 8-document corpus:
//! "consultations" can never lexically match the title "Rules of Business 1986"
//! even though Rule 17 covers it).
//!
//! Two tiers (FR-019): a category covered by an indexed document NOT in the
//! confirmed set is an add-this-document **suggestion** (one-turn officer
//! accept); only a category covered by NO indexed document is a true **missing**
//! gap (index into the watched folder, or proceed-without → refuse-or-flag).
//! `fact:` slots with no covering case document take the **officer-input**
//! channel (FR-008a) — case-specific facts live in no corpus.
//!
//! `detect_gaps` stays pure/sync: the semantic coverage provider is INJECTED as
//! a closure (the command layer supplies the embedding-backed one; tests supply
//! a fake), so this module needs no embedder or DB.

use super::slot::SlotKind;
use super::{CorpusDocRef, DocRole, Gap, GapTier, Slot};

/// Detect uncovered required-source categories for a template's cited slots.
///
/// * `confirmed` — the officer-confirmed working set.
/// * `corpus` — the whole indexed corpus (role-tagged candidates; may overlap
///   `confirmed`, overlap is filtered here).
/// * `covers(topic, doc)` — content-semantic coverage: does this document's
///   indexed content cover the category topic? (Injected; never title-based.)
pub fn detect_gaps<F>(
    slots: &[Slot],
    confirmed: &[CorpusDocRef],
    corpus: &[CorpusDocRef],
    covers: F,
    destination_folder: &str,
) -> Vec<Gap>
where
    F: Fn(&str, &CorpusDocRef) -> bool,
{
    let mut gaps = Vec::new();
    for slot in slots {
        if slot.kind != SlotKind::Cited {
            continue;
        }
        let Some(cat) = slot.required_source_category.as_deref() else {
            continue;
        };
        // `fact:` cited slots draw from CASE docs; plain cited slots from RULES.
        let (role_needed, topic) = match cat.strip_prefix("fact:") {
            Some(t) => (DocRole::Case, t.trim()),
            None => (DocRole::Rules, cat),
        };
        if topic.is_empty() {
            continue; // nothing concrete to require → not a gap
        }

        // Tier 0: covered by the confirmed set → no gap.
        let covered_confirmed = confirmed
            .iter()
            .filter(|d| d.role == role_needed)
            .any(|d| covers(topic, d));
        if covered_confirmed {
            continue;
        }

        // FR-008a: an uncovered fact slot is an officer-input prompt, not a
        // missing-document gap — no corpus holds the case-specific value.
        if role_needed == DocRole::Case {
            gaps.push(Gap {
                slot_ordering: slot.ordering,
                required_source_category: cat.to_string(),
                tier: GapTier::OfficerInput,
            });
            continue;
        }

        // Tier a: covered by indexed-but-unconfirmed document(s) → suggestion.
        let suggested: Vec<CorpusDocRef> = corpus
            .iter()
            .filter(|d| d.role == role_needed)
            .filter(|d| !confirmed.iter().any(|c| c.doc_id == d.doc_id))
            .filter(|d| covers(topic, d))
            .cloned()
            .collect();
        if !suggested.is_empty() {
            gaps.push(Gap {
                slot_ordering: slot.ordering,
                required_source_category: cat.to_string(),
                tier: GapTier::Suggestion { suggested_docs: suggested },
            });
            continue;
        }

        // Tier b: covered by nothing in the corpus → true missing-document gap.
        gaps.push(Gap {
            slot_ordering: slot.ordering,
            required_source_category: cat.to_string(),
            tier: GapTier::Missing {
                destination_folder: destination_folder.to_string(),
            },
        });
    }
    gaps
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cited(ordering: i64, category: &str) -> Slot {
        Slot {
            ordering,
            kind: SlotKind::Cited,
            label: Some("X".into()),
            boilerplate_text: None,
            required_source_category: Some(category.into()),
            prose_guidance: None,
            provision_ref: None,
        }
    }

    fn doc(doc_id: &str, title: &str, role: DocRole) -> CorpusDocRef {
        CorpusDocRef {
            doc_id: doc_id.into(),
            title: title.into(),
            source_path: format!("/corpus/{doc_id}.pdf"),
            role,
        }
    }

    /// Fake content-semantic coverage: (topic, doc_id) pairs that "cover".
    fn covers_from<'a>(
        pairs: &'a [(&'a str, &'a str)],
    ) -> impl Fn(&str, &CorpusDocRef) -> bool + 'a {
        move |topic: &str, d: &CorpusDocRef| {
            pairs.iter().any(|(t, id)| *t == topic && *id == d.doc_id)
        }
    }

    #[test]
    fn category_covered_by_confirmed_doc_yields_no_gap() {
        // Content covers even though the TITLE shares no token with the category
        // — the live-test defect ("consultations" vs "Rules of Business 1986").
        let slots = vec![cited(0, "consultations mandatory where applicable")];
        let confirmed = vec![doc("rob", "Rules of Business 1986", DocRole::Rules)];
        let covers = covers_from(&[("consultations mandatory where applicable", "rob")]);
        assert!(detect_gaps(&slots, &confirmed, &confirmed, covers, "/c").is_empty());
    }

    #[test]
    fn category_covered_only_by_unconfirmed_corpus_doc_is_a_suggestion() {
        let slots = vec![cited(1, "consultations mandatory where applicable")];
        let confirmed = vec![doc("sppra", "SPPRA Rules 2010", DocRole::Rules)];
        let corpus = vec![
            doc("sppra", "SPPRA Rules 2010", DocRole::Rules),
            doc("rob", "Rules of Business 1986", DocRole::Rules),
        ];
        let covers = covers_from(&[("consultations mandatory where applicable", "rob")]);
        let gaps = detect_gaps(&slots, &confirmed, &corpus, covers, "/c");
        assert_eq!(gaps.len(), 1);
        match &gaps[0].tier {
            GapTier::Suggestion { suggested_docs } => {
                assert_eq!(suggested_docs.len(), 1);
                assert_eq!(suggested_docs[0].doc_id, "rob");
            }
            other => panic!("expected Suggestion, got {other:?}"),
        }
    }

    #[test]
    fn category_covered_by_nothing_is_a_missing_gap_with_destination() {
        let slots = vec![cited(2, "environmental clearance certificate")];
        let confirmed = vec![doc("rob", "Rules of Business 1986", DocRole::Rules)];
        let covers = covers_from(&[]);
        let gaps = detect_gaps(&slots, &confirmed, &confirmed, covers, "/corpus");
        assert_eq!(gaps.len(), 1);
        assert_eq!(gaps[0].slot_ordering, 2);
        match &gaps[0].tier {
            GapTier::Missing { destination_folder } => assert_eq!(destination_folder, "/corpus"),
            other => panic!("expected Missing, got {other:?}"),
        }
    }

    #[test]
    fn uncovered_fact_slot_is_an_officer_input_prompt_not_a_missing_gap() {
        let slots = vec![cited(3, "fact: scheme cost sanction")];
        // A RULES doc covering the topic must not satisfy a fact slot (role split),
        // and the result is OfficerInput — never Missing (FR-008a).
        let confirmed = vec![doc("rob", "Rules of Business 1986", DocRole::Rules)];
        let covers = covers_from(&[("scheme cost sanction", "rob")]);
        let gaps = detect_gaps(&slots, &confirmed, &confirmed, covers, "/c");
        assert_eq!(gaps.len(), 1);
        assert!(matches!(gaps[0].tier, GapTier::OfficerInput), "{:?}", gaps[0].tier);

        // A covering CASE doc in the confirmed set clears it.
        let with_case = vec![doc("pc1", "Karachi Water Scheme PC-1", DocRole::Case)];
        let covers = covers_from(&[("scheme cost sanction", "pc1")]);
        assert!(detect_gaps(&slots, &with_case, &with_case, covers, "/c").is_empty());
    }

    #[test]
    fn boilerplate_and_prose_slots_are_ignored() {
        let slots = vec![
            Slot { ordering: 0, kind: SlotKind::Boilerplate, label: None, boilerplate_text: Some("hi".into()), required_source_category: None, prose_guidance: None, provision_ref: None },
            Slot { ordering: 1, kind: SlotKind::Prose, label: None, boilerplate_text: None, required_source_category: None, prose_guidance: Some("g".into()), provision_ref: None },
        ];
        let covers = covers_from(&[]);
        assert!(detect_gaps(&slots, &[], &[], covers, "/c").is_empty());
    }
}
