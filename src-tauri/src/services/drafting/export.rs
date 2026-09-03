//! Feature 009 — draft export (T020, US1).
//!
//! Serialises a [`DraftOutput`] into copy-paste-ready text/markdown. Self-contained
//! (does not force-fit the AnalysisResult exporters): cited blocks render their
//! verbatim quote + citation, flagged blocks render an explicit unverified marker
//! so verified content is never confused with a gap (FR-003).

use super::{Block, DraftOutput};

/// Plain-text rendering — what an officer copies into a file.
pub fn draft_to_text(out: &DraftOutput) -> String {
    let mut s = String::new();
    for b in &out.blocks {
        match b {
            Block::Boilerplate { text, .. } => {
                s.push_str(text);
            }
            Block::Cited { label, quote, citation } => {
                if let Some(l) = label {
                    s.push_str(l);
                    s.push_str(":\n");
                }
                s.push('"');
                s.push_str(quote);
                s.push_str("\"\n[");
                s.push_str(citation);
                s.push(']');
            }
            Block::Fact { label, value, attribution } => {
                if let Some(l) = label {
                    s.push_str(l);
                    s.push_str(": ");
                }
                s.push_str(value);
                s.push_str(" (");
                s.push_str(attribution);
                s.push(')');
            }
            Block::Prose { label, text } => {
                if let Some(l) = label {
                    s.push_str(l);
                    s.push_str(":\n");
                }
                s.push_str(text);
            }
            Block::Flagged { label, reason } => {
                if let Some(l) = label {
                    s.push_str(l);
                    s.push_str(": ");
                }
                s.push_str("[UNVERIFIED — ");
                s.push_str(reason);
                s.push(']');
            }
        }
        s.push_str("\n\n");
    }
    s.trim_end().to_string()
}

/// Markdown rendering — bold labels, blockquoted citations.
pub fn draft_to_markdown(out: &DraftOutput) -> String {
    let mut s = String::new();
    for b in &out.blocks {
        match b {
            Block::Boilerplate { text, .. } => s.push_str(text),
            Block::Cited { label, quote, citation } => {
                if let Some(l) = label {
                    s.push_str(&format!("**{l}**\n\n"));
                }
                s.push_str(&format!("> {quote}\n>\n> — *{citation}*"));
            }
            Block::Fact { label, value, attribution } => {
                if let Some(l) = label {
                    s.push_str(&format!("**{l}**: "));
                }
                s.push_str(&format!("{value} _({attribution})_"));
            }
            Block::Prose { label, text } => {
                if let Some(l) = label {
                    s.push_str(&format!("**{l}**\n\n"));
                }
                s.push_str(text);
            }
            Block::Flagged { label, reason } => {
                if let Some(l) = label {
                    s.push_str(&format!("**{l}**: "));
                }
                s.push_str(&format!("⚠️ _UNVERIFIED — {reason}_"));
            }
        }
        s.push_str("\n\n");
    }
    s.trim_end().to_string()
}

/// Write a rendered draft to disk in the requested format ("txt" | "md").
pub fn write_draft(out: &DraftOutput, dest_path: &str, format: &str) -> std::io::Result<()> {
    let body = match format {
        "md" | "markdown" => draft_to_markdown(out),
        _ => draft_to_text(out),
    };
    std::fs::write(dest_path, body)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> DraftOutput {
        DraftOutput {
            blocks: vec![
                Block::Boilerplate {
                    label: None,
                    text: "SUMMARY FOR THE CHIEF SECRETARY".into(),
                },
                Block::Cited {
                    label: Some("POLICY / RULES".into()),
                    quote: "Rule 28 financial concurrence shall be obtained".into(),
                    citation: "Rule 28, rob p.12".into(),
                },
                Block::Flagged {
                    label: Some("SCHEME COST".into()),
                    reason: "unverified — source not available".into(),
                },
            ],
        }
    }

    #[test]
    fn text_render_keeps_quote_and_citation_and_flags() {
        let t = draft_to_text(&sample());
        assert!(t.contains("SUMMARY FOR THE CHIEF SECRETARY"));
        assert!(t.contains("\"Rule 28 financial concurrence shall be obtained\""));
        assert!(t.contains("[Rule 28, rob p.12]"));
        assert!(t.contains("[UNVERIFIED — unverified — source not available]"));
    }

    #[test]
    fn markdown_render_marks_flagged_distinctly() {
        let m = draft_to_markdown(&sample());
        assert!(m.contains("**POLICY / RULES**"));
        assert!(m.contains("> Rule 28 financial concurrence shall be obtained"));
        assert!(m.contains("⚠️"));
    }
}
