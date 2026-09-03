//! Built-in single-move-per-paragraph Sindh summary skeleton (US10/FR-019).
//!
//! Structure is product domain knowledge — a compile-time const, never a
//! per-case uploaded template (the 009 failure mode). Each section is ONE
//! analytical move rendered as one concise paragraph; brevity comes from the
//! advocacy strip + this structure, never from truncation (FR-020).

use serde::{Deserialize, Serialize};

/// Skeleton section identity, in canonical (output) order. US10: one move each —
/// Background/Proposal/Legal-bundle of the old skeleton are gone.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum SectionId {
    /// Move 1 — the time-bound reason + what is sought.
    Urgency,
    /// Move 2 — the governing/authorising provision(s).
    LegalValidation,
    /// Move 3 — name the parties involved (narrative, no citation).
    Stakeholders,
    /// Move 4 — each named party's legal status/powers, cited.
    StakeholderStatus,
    /// Move 5 — the prescribed procedure, cited.
    Procedure,
    /// Conditional — rendered only when the officer states a figure.
    FinancialAspect,
    /// One-sentence prayer for approval.
    PrayerForApproval,
}

/// One skeleton section: writer guidance + retrieval-role hints.
#[derive(Debug, Clone, Copy)]
pub struct Section {
    pub id: SectionId,
    pub title: &'static str,
    /// Narrative instruction handed to the writer for this section.
    pub guidance: &'static str,
    /// Query fragments combined with the officer's sentence for retrieval (R5).
    pub role_hints: &'static [&'static str],
    /// Whether officer-stated figures belong in this section's prompt.
    pub wants_figures: bool,
    /// Max citations this section may carry (FR-003). 0 = narrative-only (owns
    /// no provision cards; any citation is dropped in assembly).
    pub max_citations: u8,
}

/// The canonical Sindh summary flow (US10/FR-019). Order is the output order.
pub const SKELETON: [Section; 7] = [
    Section {
        id: SectionId::Urgency,
        title: "Urgency",
        guidance: "In concise secretariat prose, state the time-bound reason this matter is \
                   before the competent authority and what is sought. State the time fact \
                   plainly as fact — NO urgency adjectives, NO advocacy. Do not discuss \
                   legal provisions or finances here.",
        role_hints: &["subject matter purpose request proposal"],
        wants_figures: false,
        max_citations: 0,
    },
    Section {
        id: SectionId::LegalValidation,
        title: "Legal Validation",
        guidance: "State the governing legal provision(s) that authorise or govern this matter, \
                   using ONLY the provisions supplied as citation cards. If no provision is \
                   supplied, state plainly that no clear legal basis was found. Do NOT conclude \
                   that the matter is legal or valid — present the provision and let it speak.",
        role_hints: &[
            "governing provision authorising legal basis empowering rule",
            "rules of business procedure competent authority approval",
        ],
        wants_figures: false,
        max_citations: 2,
    },
    Section {
        id: SectionId::Stakeholders,
        title: "Stakeholders",
        guidance: "Name the parties/stakeholders involved in this matter, in one concise \
                   sentence or two. NAMES ONLY — do not assert any power or status here.",
        role_hints: &["parties involved competent authority"],
        wants_figures: false,
        max_citations: 0,
    },
    Section {
        id: SectionId::StakeholderStatus,
        title: "Stakeholder Legal Status",
        guidance: "State the legal status and powers of each named stakeholder in this matter, \
                   using ONLY the provisions supplied as citation cards. Where no supplied card \
                   supports a stakeholder, name them without asserting any power.",
        role_hints: &[
            "competent authority powers delegated empowered jurisdiction sanction",
        ],
        wants_figures: false,
        max_citations: 2,
    },
    Section {
        id: SectionId::Procedure,
        title: "Procedure",
        guidance: "State the prescribed procedure to be followed in executing this matter, \
                   using ONLY the provisions supplied as citation cards.",
        role_hints: &["procedure process manner routing approval prescribed steps"],
        wants_figures: false,
        max_citations: 2,
    },
    Section {
        id: SectionId::FinancialAspect,
        title: "Financial Aspect",
        guidance: "State the financial dimension: the funds sought and the sanctioning powers \
                   that apply, using ONLY supplied citation cards for any powers or limits.",
        role_hints: &[
            "delegation of financial powers sanction funds expenditure budget allocation",
        ],
        wants_figures: true,
        max_citations: 1,
    },
    Section {
        id: SectionId::PrayerForApproval,
        title: "Prayer for Approval",
        guidance: "Close with the formal prayer in ONE sentence: the approval solicited from \
                   the competent authority for the subject. No citations, no quotations, no \
                   figures, no greetings.",
        role_hints: &["approval solicited orders requested"],
        wants_figures: false,
        max_citations: 0,
    },
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn skeleton_is_one_move_per_paragraph_in_canonical_order() {
        let ids: Vec<SectionId> = SKELETON.iter().map(|s| s.id).collect();
        assert_eq!(
            ids,
            vec![
                SectionId::Urgency,
                SectionId::LegalValidation,
                SectionId::Stakeholders,
                SectionId::StakeholderStatus,
                SectionId::Procedure,
                SectionId::FinancialAspect,
                SectionId::PrayerForApproval,
            ]
        );
    }

    #[test]
    fn financial_aspect_wants_figures() {
        let fin = SKELETON
            .iter()
            .find(|s| s.id == SectionId::FinancialAspect)
            .unwrap();
        assert!(fin.wants_figures);
        for s in SKELETON.iter() {
            assert!(!s.guidance.trim().is_empty());
            assert!(!s.role_hints.is_empty());
        }
    }

    #[test]
    fn citation_caps_match_per_move_policy() {
        let cap = |id: SectionId| SKELETON.iter().find(|s| s.id == id).unwrap().max_citations;
        // Citation-bearing moves: legal basis / status / procedure = 2 each;
        // financial = 1. Narrative moves (urgency / stakeholders / prayer) = 0.
        assert_eq!(cap(SectionId::Urgency), 0);
        assert_eq!(cap(SectionId::LegalValidation), 2);
        assert_eq!(cap(SectionId::Stakeholders), 0);
        assert_eq!(cap(SectionId::StakeholderStatus), 2);
        assert_eq!(cap(SectionId::Procedure), 2);
        assert_eq!(cap(SectionId::FinancialAspect), 1);
        assert_eq!(cap(SectionId::PrayerForApproval), 0);
    }
}
