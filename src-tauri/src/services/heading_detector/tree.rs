//! Document tree builder: combines pass A + pass B candidates into a nested
//! (document -> section -> leaf) tree. Each node records a byte range into the
//! source text so the chunker can emit content faithfully.

use super::regex_pass::HeadingCandidate;

#[derive(Debug, Clone, PartialEq)]
pub enum NodeKind { Document, Section, Leaf }

#[derive(Debug, Clone)]
pub struct DocNode {
    pub kind: NodeKind,
    /// Heading label or document title. None on Leaf.
    pub label: Option<String>,
    /// Breadcrumb from document root, e.g., "Chapter III > Section 8".
    pub path: String,
    pub byte_start: usize,
    pub byte_end: usize,
    pub children: Vec<DocNode>,
}

pub struct BuildOptions {
    pub document_title: String,
}

/// Build a doc -> section -> leaf tree.
/// `headings` MUST be sorted by `offset` ascending (the standard output of regex_pass::scan).
/// If `headings` is empty, returns document with the entire text as a single leaf.
pub fn build_tree(text: &str, headings: &[HeadingCandidate], opts: BuildOptions) -> DocNode {
    if text.is_empty() {
        return DocNode {
            kind: NodeKind::Document, label: Some(opts.document_title), path: String::new(),
            byte_start: 0, byte_end: 0, children: vec![],
        };
    }

    if headings.is_empty() {
        return DocNode {
            kind: NodeKind::Document, label: Some(opts.document_title.clone()),
            path: String::new(), byte_start: 0, byte_end: text.len(),
            children: vec![DocNode {
                kind: NodeKind::Leaf, label: None, path: String::new(),
                byte_start: 0, byte_end: text.len(), children: vec![],
            }],
        };
    }

    let mut sections: Vec<DocNode> = Vec::with_capacity(headings.len());

    if headings[0].offset > 0 {
        sections.push(DocNode {
            kind: NodeKind::Section, label: Some("Preamble".into()),
            path: "Preamble".into(),
            byte_start: 0, byte_end: headings[0].offset,
            children: vec![DocNode {
                kind: NodeKind::Leaf, label: None, path: "Preamble".into(),
                byte_start: 0, byte_end: headings[0].offset, children: vec![],
            }],
        });
    }

    for (i, h) in headings.iter().enumerate() {
        let start = h.offset;
        let end = headings.get(i + 1).map(|n| n.offset).unwrap_or(text.len());
        let path = h.label.clone();
        sections.push(DocNode {
            kind: NodeKind::Section,
            label: Some(h.label.clone()),
            path: path.clone(),
            byte_start: start, byte_end: end,
            children: vec![DocNode {
                kind: NodeKind::Leaf, label: None, path,
                byte_start: start, byte_end: end, children: vec![],
            }],
        });
    }

    DocNode {
        kind: NodeKind::Document, label: Some(opts.document_title),
        path: String::new(), byte_start: 0, byte_end: text.len(),
        children: sections,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use super::super::regex_pass::{HeadingCandidate, HeadingType};

    fn h(offset: usize, label: &str) -> HeadingCandidate {
        HeadingCandidate { offset, label: label.into(), unit_type: HeadingType::Section, confidence: 0.95 }
    }

    #[test]
    fn empty_text_returns_empty_document() {
        let t = build_tree("", &[], BuildOptions { document_title: "x.pdf".into() });
        assert!(matches!(t.kind, NodeKind::Document));
        assert!(t.children.is_empty());
    }

    #[test]
    fn no_headings_yields_two_level_fallback() {
        let text = "Just some flat text.";
        let t = build_tree(text, &[], BuildOptions { document_title: "memo.txt".into() });
        assert_eq!(t.children.len(), 1);
        assert!(matches!(t.children[0].kind, NodeKind::Leaf));
    }

    #[test]
    fn sections_partition_text_byte_ranges() {
        let text = "Section 8. Body of 8. Section 9. Body of 9.";
        let headings = vec![h(0, "Section 8"), h(20, "Section 9")];
        let t = build_tree(text, &headings, BuildOptions { document_title: "x".into() });
        assert_eq!(t.children.len(), 2);
        assert_eq!(t.children[0].byte_start, 0);
        assert_eq!(t.children[0].byte_end, 20);
        assert_eq!(t.children[1].byte_start, 20);
        assert_eq!(t.children[1].byte_end, text.len());
    }

    #[test]
    fn preamble_before_first_heading_becomes_section() {
        let text = "Some preamble.\n\nSection 1. Body";
        let headings = vec![h(16, "Section 1")];
        let t = build_tree(text, &headings, BuildOptions { document_title: "x".into() });
        assert_eq!(t.children.len(), 2);
        assert_eq!(t.children[0].label.as_deref(), Some("Preamble"));
        assert_eq!(t.children[1].label.as_deref(), Some("Section 1"));
    }
}
