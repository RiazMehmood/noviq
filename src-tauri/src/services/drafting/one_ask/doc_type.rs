//! Lightweight document-type classification + retrieval profile (US1/FR-001,002).
//!
//! Generic by design (D2/D3): the type is derived from the officer's request
//! (action + subject vocabulary), never from a per-case uploaded template. The
//! type selects per-section retrieval *emphasis* expressed as GENERIC
//! governance/process vocabulary — never document- or case-specific names
//! (no "SPPRA"/"Canon"/"e-daftar"; no boost/suppress lists in retrieval
//! internals). It exists to stop one section's default role-hints from over-
//! fitting the wrong corpus (observed: the Legal hint "development project …
//! PC-I" dragged Development-Manual provisions into a *procurement* note).

use super::skeleton::{Section, SectionId};
use super::state::OneAskIntent;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DocType {
    ProcurementNote,
    AdministrativeApproval,
    FinancialSanction,
    TransferPosting,
    PolicyApproval,
    CabinetSummary,
    DevelopmentScheme,
    General,
}

impl DocType {
    pub fn label(self) -> &'static str {
        match self {
            DocType::ProcurementNote => "Procurement Note",
            DocType::AdministrativeApproval => "Administrative Approval Note",
            DocType::FinancialSanction => "Financial Sanction Note",
            DocType::TransferPosting => "Transfer/Posting Note",
            DocType::PolicyApproval => "Policy Approval Note",
            DocType::CabinetSummary => "Cabinet Summary",
            DocType::DevelopmentScheme => "Development Scheme Note",
            DocType::General => "General Summary",
        }
    }
}

fn contains_any(hay: &str, needles: &[&str]) -> bool {
    needles.iter().any(|n| hay.contains(n))
}

/// Classify the request. Most-specific checks first; falls back to `General`
/// (never errors, never a per-case template). Signals are generic
/// administrative/process vocabulary — the kind present in any such request in
/// any jurisdiction — not case- or document-specific names.
pub fn classify(intent: &OneAskIntent) -> DocType {
    let t = intent.raw.to_lowercase();
    if contains_any(
        &t,
        &[
            "procure", "procurement", "tender", "contracting", "supplier",
            "distributor", "purchase", "bidding", "sole source", "single source",
        ],
    ) {
        DocType::ProcurementNote
    } else if contains_any(&t, &["transfer", "posting", "deputation", "repatriat"]) {
        DocType::TransferPosting
    } else if contains_any(
        &t,
        &["development scheme", "pc-i", "pc i", "feasibility", "project director"],
    ) {
        DocType::DevelopmentScheme
    } else if contains_any(&t, &["re-appropriation", "release of funds"])
        || (contains_any(&t, &["sanction", "expenditure"]) && !t.contains("procur"))
    {
        DocType::FinancialSanction
    } else if contains_any(&t, &["policy", "guideline", "framework"]) {
        DocType::PolicyApproval
    } else if t.contains("cabinet") {
        DocType::CabinetSummary
    } else if contains_any(
        &t,
        &["approval", "approve", "permission", "permit", "regulariz", "regularis"],
    ) {
        DocType::AdministrativeApproval
    } else {
        DocType::General
    }
}

/// Per-(type, section) retrieval role-hint override. `None` ⇒ use the skeleton
/// default. Generic process vocabulary only (no document/case names).
fn role_hints_override(
    doc_type: DocType,
    section_id: SectionId,
) -> Option<&'static [&'static str]> {
    match (doc_type, section_id) {
        // A Procurement Note's Legal position is about procurement *method* and
        // routing — not development-scheme/PC-I process. Replacing the default
        // hint removes the bias that pulled the Development Manual into Legal.
        (DocType::ProcurementNote, SectionId::LegalValidation) => Some(&[
            "procurement method contract award competent authority approval",
            "single source direct contracting tender procedure sanction",
        ]),
        _ => None,
    }
}

/// Effective retrieval role-hints for a section under a document type.
pub fn effective_role_hints(doc_type: DocType, section: &Section) -> &'static [&'static str] {
    role_hints_override(doc_type, section.id).unwrap_or(section.role_hints)
}

#[cfg(test)]
mod tests {
    use super::*;
    use super::super::skeleton::SKELETON;

    fn intent(raw: &str) -> OneAskIntent {
        OneAskIntent { raw: raw.into(), figures: vec![], action: None }
    }

    fn legal() -> &'static Section {
        SKELETON.iter().find(|s| s.id == SectionId::LegalValidation).unwrap()
    }

    #[test]
    fn canon_scanner_ask_is_a_procurement_note() {
        let i = intent(
            "allow procurement through direct contracting with the sole distributor of Canon scanners for 10 departments for e-daftar",
        );
        assert_eq!(classify(&i), DocType::ProcurementNote);
    }

    #[test]
    fn other_types_and_general_fallback() {
        assert_eq!(classify(&intent("transfer and posting of the deputy secretary")), DocType::TransferPosting);
        assert_eq!(classify(&intent("approval of the revised office timings")), DocType::AdministrativeApproval);
        assert_eq!(classify(&intent("note about the annual sports day")), DocType::General);
    }

    #[test]
    fn procurement_legal_hints_drop_development_bias_and_emphasise_procurement() {
        let hints = effective_role_hints(DocType::ProcurementNote, legal()).join(" ");
        assert!(!hints.contains("development project"), "dev-scheme bias removed: {hints}");
        assert!(hints.contains("procurement method"), "{hints}");
        assert!(hints.contains("direct contracting"), "{hints}");
    }

    #[test]
    fn general_type_keeps_skeleton_default_hints() {
        let hints = effective_role_hints(DocType::General, legal());
        assert_eq!(hints, legal().role_hints, "General must not override defaults");
    }
}
