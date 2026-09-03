//! Deterministic one-sentence intent parse (FR-003, research R6).
//!
//! Tier-1 discipline (constitution II): regex only, no model. Figures are
//! captured VERBATIM — they re-enter the draft only through [FIG:n] splice.

use super::state::{Figure, OneAskIntent};
use regex_lite::Regex;

/// Money/scale figure pattern: "4 billion", "Rs. 2,500 million", "Rs 500",
/// "3.5 crore", "200 lakh". Verbatim capture, case-insensitive.
pub(crate) fn figure_regex() -> Regex {
    Regex::new(
        r"(?i)\b(?:rs\.?\s*)?\d[\d,]*(?:\.\d+)?\s*(?:billion|million|trillion|crore|lakh|lacs?)\b|\brs\.?\s*\d[\d,]*(?:\.\d+)?\b",
    )
    .expect("figure regex compiles")
}

const ACTION_KEYWORDS: &[&str] = &[
    "get", "obtain", "approve", "approval", "sanction", "allocate", "allocation",
    "develop", "establish", "construct", "create", "launch", "fund", "grant",
    "procure", "rehabilitate", "upgrade",
];

/// Parse the officer's sentence. The raw sentence is the retrieval seed; the
/// figures are lossless verbatim substrings of it.
pub fn parse_intent(text: &str) -> Result<OneAskIntent, String> {
    let raw = text.trim();
    if raw.is_empty() {
        return Err(
            "The ask is empty. Type one sentence stating what you want, e.g. \
             \"I want to get 4 billion to develop schools\"."
                .to_string(),
        );
    }
    let figures = figure_regex()
        .find_iter(raw)
        .map(|m| Figure {
            text: m.as_str().to_string(),
            normalized: None,
        })
        .collect();
    let lower = raw.to_lowercase();
    let action = ACTION_KEYWORDS
        .iter()
        .find(|kw| {
            lower
                .split(|c: char| !c.is_alphanumeric())
                .any(|w| w == **kw)
        })
        .map(|kw| kw.to_string());
    Ok(OneAskIntent {
        raw: raw.to_string(),
        figures,
        action,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn captures_figure_verbatim() {
        let i = parse_intent("I want to get 4 billion to develop schools/IT institutes").unwrap();
        assert_eq!(i.figures.len(), 1);
        assert_eq!(i.figures[0].text, "4 billion");
        assert_eq!(i.action.as_deref(), Some("get"));
        assert!(i.raw.contains("schools/IT institutes"));
    }

    #[test]
    fn captures_multiple_figures_losslessly() {
        let i =
            parse_intent("sanction Rs. 2,500 million over 3 years for 4 billion scheme").unwrap();
        let texts: Vec<&str> = i.figures.iter().map(|f| f.text.as_str()).collect();
        assert!(texts.contains(&"Rs. 2,500 million"));
        assert!(texts.contains(&"4 billion"));
        // every captured figure is a verbatim substring of raw
        for f in &i.figures {
            assert!(i.raw.contains(&f.text));
        }
    }

    #[test]
    fn no_figures_is_fine() {
        let i = parse_intent("approve revised service rules for the department").unwrap();
        assert!(i.figures.is_empty());
        assert_eq!(i.action.as_deref(), Some("approve"));
    }

    #[test]
    fn empty_ask_errors_with_remediation() {
        let e = parse_intent("   ").unwrap_err();
        assert!(e.contains("one sentence"));
    }
}
