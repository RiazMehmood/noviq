//! Feature 009 (Conversational session, Phase A) — turn → action parser.
//!
//! The conversation is a thin interaction layer over the existing deterministic
//! pipeline (FR-028): it asks one intent question at a time and maps each officer
//! turn to a structured [`DraftAction`]. Phase A covers intent collection only
//! (addressee → purpose → scheme; `instrument_type` comes from the template at
//! session creation). Doc-set/gap/refine actions arrive in Phase B.

use super::IntentAnswers;

/// The intent fields gathered conversationally, in question order.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IntentField {
    Addressee,
    Purpose,
    Scheme,
}

/// A parsed officer turn. Phase A variants only.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DraftAction {
    /// Fill one intent field with the officer's verbatim answer.
    SetIntent { field: IntentField, value: String },
    /// The turn could not be mapped to the current question — ask again (FR-030).
    Clarify,
}

/// The next intent field still needing an answer, in question order. `None` once
/// addressee + purpose + scheme are all filled (`instrument_type` comes from the
/// template at session creation, so it is not asked).
pub fn next_field(intent: &IntentAnswers) -> Option<IntentField> {
    if intent.addressee.trim().is_empty() {
        Some(IntentField::Addressee)
    } else if intent.purpose.trim().is_empty() {
        Some(IntentField::Purpose)
    } else if intent.scheme.trim().is_empty() {
        Some(IntentField::Scheme)
    } else {
        None
    }
}

/// True once every conversational intent field is filled.
pub fn intent_complete(intent: &IntentAnswers) -> bool {
    next_field(intent).is_none()
}

/// The system question prompting for a given field.
pub fn question_for(field: IntentField) -> &'static str {
    match field {
        IntentField::Addressee => "Who is the summary addressed to (the routing/addressee)?",
        IntentField::Purpose => "What is the purpose of this summary?",
        IntentField::Scheme => "Which scheme or subject does it concern?",
    }
}

/// Write the officer's answer into the intent.
pub fn apply(intent: &mut IntentAnswers, field: IntentField, value: String) {
    match field {
        IntentField::Addressee => intent.addressee = value,
        IntentField::Purpose => intent.purpose = value,
        IntentField::Scheme => intent.scheme = value,
    }
}

/// Map one officer turn to an action: the trimmed answer fills the next
/// unanswered intent field; an empty turn (or one with nothing left to answer)
/// asks for clarification (FR-028/030). Deterministic — no model involved.
pub fn parse_turn(intent: &IntentAnswers, text: &str) -> DraftAction {
    let value = text.trim();
    if value.is_empty() {
        return DraftAction::Clarify;
    }
    match next_field(intent) {
        Some(field) => DraftAction::SetIntent {
            field,
            value: value.to_string(),
        },
        None => DraftAction::Clarify,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn blank() -> IntentAnswers {
        IntentAnswers {
            instrument_type: "summary".into(),
            addressee: String::new(),
            purpose: String::new(),
            scheme: String::new(),
        }
    }

    #[test]
    fn first_answer_fills_addressee() {
        let i = blank();
        assert_eq!(
            parse_turn(&i, "Chief Secretary, Sindh"),
            DraftAction::SetIntent {
                field: IntentField::Addressee,
                value: "Chief Secretary, Sindh".into()
            }
        );
    }

    #[test]
    fn second_answer_fills_purpose() {
        let mut i = blank();
        i.addressee = "Chief Secretary".into();
        assert_eq!(
            parse_turn(&i, "Approval to use ADP funds"),
            DraftAction::SetIntent {
                field: IntentField::Purpose,
                value: "Approval to use ADP funds".into()
            }
        );
    }

    #[test]
    fn third_answer_fills_scheme() {
        let mut i = blank();
        i.addressee = "Chief Secretary".into();
        i.purpose = "Approval".into();
        assert_eq!(
            parse_turn(&i, "Karachi Water Supply"),
            DraftAction::SetIntent {
                field: IntentField::Scheme,
                value: "Karachi Water Supply".into()
            }
        );
    }

    #[test]
    fn empty_turn_is_clarify() {
        assert_eq!(parse_turn(&blank(), "   "), DraftAction::Clarify);
    }

    #[test]
    fn answer_is_trimmed() {
        assert_eq!(
            parse_turn(&blank(), "  Chief Secretary  "),
            DraftAction::SetIntent {
                field: IntentField::Addressee,
                value: "Chief Secretary".into()
            }
        );
    }
}
