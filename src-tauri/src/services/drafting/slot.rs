//! Feature 009 — slot-schema types for the drafting templates.
//!
//! A template is an ordered list of [`Slot`]s. Each slot is one of three kinds:
//! - `Boilerplate` — copied verbatim into every draft (FR-014).
//! - `Cited` — filled with a verbatim quote + deterministic citation lifted from
//!   an indexed chunk (ADR-0008); carries a `required_source_category` that names
//!   the KIND of source it needs (clarification: category, not a filename).
//! - `Prose` — bounded connective narrative written by the model, structurally
//!   barred from emitting any number/citation (FR-007).

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SlotKind {
    Boilerplate,
    Cited,
    Prose,
}

impl SlotKind {
    pub fn as_str(self) -> &'static str {
        match self {
            SlotKind::Boilerplate => "boilerplate",
            SlotKind::Cited => "cited",
            SlotKind::Prose => "prose",
        }
    }

    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "boilerplate" => Some(SlotKind::Boilerplate),
            "cited" => Some(SlotKind::Cited),
            "prose" => Some(SlotKind::Prose),
            _ => None,
        }
    }
}

/// One ordered region of a template.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Slot {
    pub ordering: i64,
    pub kind: SlotKind,
    pub label: Option<String>,
    /// Present iff `kind == Boilerplate`.
    pub boilerplate_text: Option<String>,
    /// Present iff `kind == Cited` — the source KIND/category needed to fill it.
    pub required_source_category: Option<String>,
    /// Present iff `kind == Prose` — bounded instruction for connective narrative.
    pub prose_guidance: Option<String>,
    /// Cited slots only (Phase D, FR-010a): the verbatim provision reference as
    /// written in the uploaded instrument (e.g. "Rule-6(c) of SPSC
    /// Regulations-2023"), gate-verified as a verbatim substring of the source.
    /// `None` for non-cited slots and legacy templates (migration 024).
    #[serde(default)]
    pub provision_ref: Option<String>,
}

impl Slot {
    /// Validate per data-model.md rules. Returns an error message on violation.
    pub fn validate(&self) -> Result<(), String> {
        match self.kind {
            SlotKind::Boilerplate => {
                if self.boilerplate_text.as_deref().unwrap_or("").is_empty() {
                    return Err(format!(
                        "slot {}: boilerplate slot requires boilerplate_text",
                        self.ordering
                    ));
                }
            }
            SlotKind::Cited => {
                if self
                    .required_source_category
                    .as_deref()
                    .unwrap_or("")
                    .is_empty()
                {
                    return Err(format!(
                        "slot {}: cited slot requires a required_source_category",
                        self.ordering
                    ));
                }
            }
            SlotKind::Prose => {
                if self.prose_guidance.as_deref().unwrap_or("").is_empty() {
                    return Err(format!(
                        "slot {}: prose slot requires prose_guidance",
                        self.ordering
                    ));
                }
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn slot(kind: SlotKind) -> Slot {
        Slot {
            ordering: 0,
            kind,
            label: None,
            boilerplate_text: None,
            required_source_category: None,
            prose_guidance: None,
            provision_ref: None,
        }
    }

    #[test]
    fn slotkind_roundtrips() {
        for k in [SlotKind::Boilerplate, SlotKind::Cited, SlotKind::Prose] {
            assert_eq!(SlotKind::from_str(k.as_str()), Some(k));
        }
        assert_eq!(SlotKind::from_str("nonsense"), None);
    }

    #[test]
    fn cited_slot_requires_category() {
        let mut s = slot(SlotKind::Cited);
        assert!(s.validate().is_err());
        s.required_source_category = Some("financial-powers rules".into());
        assert!(s.validate().is_ok());
    }

    #[test]
    fn boilerplate_requires_text_prose_requires_guidance() {
        assert!(slot(SlotKind::Boilerplate).validate().is_err());
        assert!(slot(SlotKind::Prose).validate().is_err());
        let mut b = slot(SlotKind::Boilerplate);
        b.boilerplate_text = Some("SUMMARY FOR THE CHIEF SECRETARY".into());
        assert!(b.validate().is_ok());
        let mut p = slot(SlotKind::Prose);
        p.prose_guidance = Some("Summarise the background in two sentences.".into());
        assert!(p.validate().is_ok());
    }
}
