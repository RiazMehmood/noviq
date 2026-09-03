//! Feature 009 — document-set proposal & curation (T028, US3).
//!
//! From the officer's intent answers, propose candidate documents over the WHOLE
//! corpus (FR-016), tagging each `CorpusDocRef.role` rules|case (research R2,
//! officer-correctable). The officer-confirmed set is the scoping mechanism —
//! only confirmed docs feed the draft.
//!
//! Retrieval (DB/embedder) lives in the command layer (mirrors `render.rs`'s
//! injected-closure shape); this module owns the pure, testable parts: building
//! the intent query, role-tagging a candidate, and de-duplicating the proposal.

use super::{CorpusDocRef, DocRole, IntentAnswers};

/// A whole-corpus retrieval hit, reduced to the per-document fields the proposal
/// needs. The command layer maps DB rows into these (deduping happens here).
#[derive(Debug, Clone)]
pub struct CorpusHit {
    pub doc_id: String,
    pub title: String,
    pub source_path: String,
}

/// Build the retrieval query that drives whole-corpus candidate proposal from the
/// officer's intent. Generic concatenation — no document-specific keywords.
pub fn intent_query(intent: &IntentAnswers) -> String {
    [
        intent.purpose.trim(),
        intent.scheme.trim(),
        intent.addressee.trim(),
    ]
    .iter()
    .filter(|s| !s.is_empty())
    .copied()
    .collect::<Vec<_>>()
    .join(" ")
}

/// Generic, officer-correctable role guess (research R2). A document whose
/// title/path carries an *instrument-class* token (rules / regulations / act /
/// ordinance / code / manual / policy / notification / constitution) is proposed
/// as `Rules` (supplies citations); everything else is proposed as `Case`
/// (supplies facts). This is a PROPOSAL the officer retags — not a routing key.
pub fn tag_role(title: &str, source_path: &str) -> DocRole {
    const INSTRUMENT_TOKENS: &[&str] = &[
        "rule",
        "regulation",
        "act",
        "ordinance",
        "code",
        "manual",
        "policy",
        "notification",
        "constitution",
        "statute",
        "law",
        "bylaw",
        "schedule",
    ];
    let hay = format!("{} {}", title.to_lowercase(), source_path.to_lowercase());
    if INSTRUMENT_TOKENS.iter().any(|t| hay.contains(t)) {
        DocRole::Rules
    } else {
        DocRole::Case
    }
}

/// Turn whole-corpus hits into a de-duplicated, role-tagged candidate set,
/// preserving retrieval order (most relevant first). One ref per document.
pub fn propose_candidates(hits: &[CorpusHit]) -> Vec<CorpusDocRef> {
    let mut seen = std::collections::HashSet::new();
    let mut out = Vec::new();
    for hit in hits {
        if !seen.insert(hit.doc_id.clone()) {
            continue;
        }
        out.push(CorpusDocRef {
            doc_id: hit.doc_id.clone(),
            title: hit.title.clone(),
            source_path: hit.source_path.clone(),
            role: tag_role(&hit.title, &hit.source_path),
        });
    }
    out
}

/// Documents present in `new` but not in `old` (by `doc_id`), preserving `new`'s
/// order. Used by the watch re-proposal (FR-020) to report just the docs a fresh
/// indexing run added — so gaps can shrink without restarting the draft (SC-005).
pub fn newly_added(old: &[CorpusDocRef], new: &[CorpusDocRef]) -> Vec<CorpusDocRef> {
    let known: std::collections::HashSet<&str> = old.iter().map(|d| d.doc_id.as_str()).collect();
    new.iter()
        .filter(|d| !known.contains(d.doc_id.as_str()))
        .cloned()
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn intent() -> IntentAnswers {
        IntentAnswers {
            instrument_type: "summary".into(),
            addressee: "Chief Secretary".into(),
            purpose: "release ADP funds".into(),
            scheme: "Karachi Water Scheme".into(),
        }
    }

    #[test]
    fn intent_query_joins_nonempty_fields() {
        let q = intent_query(&intent());
        assert!(q.contains("release ADP funds"));
        assert!(q.contains("Karachi Water Scheme"));
        assert!(q.contains("Chief Secretary"));
    }

    #[test]
    fn intent_query_skips_blank_fields() {
        let mut i = intent();
        i.scheme = "   ".into();
        let q = intent_query(&i);
        assert!(!q.contains("  ")); // no doubled separators from the blank
    }

    #[test]
    fn instrument_class_titles_tag_as_rules() {
        assert_eq!(tag_role("Rules of Business 1973", "/x/rob.pdf"), DocRole::Rules);
        assert_eq!(tag_role("ADP Policy", "/x/policy.pdf"), DocRole::Rules);
        assert_eq!(tag_role("anything", "/corpus/Finance_Manual.pdf"), DocRole::Rules);
    }

    #[test]
    fn ordinary_documents_tag_as_case() {
        assert_eq!(tag_role("Summary file noting", "/x/file42.pdf"), DocRole::Case);
        assert_eq!(tag_role("Karachi Water Scheme PC-1", "/x/pc1.pdf"), DocRole::Case);
    }

    #[test]
    fn newly_added_returns_only_unseen_docs_in_new_order() {
        let old = vec![CorpusDocRef {
            doc_id: "rob".into(),
            title: "Rules of Business".into(),
            source_path: "/x/rob.pdf".into(),
            role: DocRole::Rules,
        }];
        let new = vec![
            CorpusDocRef { doc_id: "rob".into(), title: "Rules of Business".into(), source_path: "/x/rob.pdf".into(), role: DocRole::Rules },
            CorpusDocRef { doc_id: "pc1".into(), title: "PC-1".into(), source_path: "/x/pc1.pdf".into(), role: DocRole::Case },
        ];
        let added = newly_added(&old, &new);
        assert_eq!(added.len(), 1);
        assert_eq!(added[0].doc_id, "pc1");
    }

    #[test]
    fn proposal_dedupes_by_doc_id_preserving_order() {
        let hits = vec![
            CorpusHit { doc_id: "rob".into(), title: "Rules of Business".into(), source_path: "/x/rob.pdf".into() },
            CorpusHit { doc_id: "pc1".into(), title: "PC-1".into(), source_path: "/x/pc1.pdf".into() },
            CorpusHit { doc_id: "rob".into(), title: "Rules of Business".into(), source_path: "/x/rob.pdf".into() },
        ];
        let cands = propose_candidates(&hits);
        assert_eq!(cands.len(), 2, "duplicate doc_id collapsed");
        assert_eq!(cands[0].doc_id, "rob");
        assert_eq!(cands[0].role, DocRole::Rules);
        assert_eq!(cands[1].doc_id, "pc1");
        assert_eq!(cands[1].role, DocRole::Case);
    }
}
