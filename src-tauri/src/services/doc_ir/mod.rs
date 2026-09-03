//! Feature 006 — LLM-driven PDF → hierarchical IR importer.
//!
//! Pipeline: PDF text → doc-type classifier → LLM IR builder (Mistral 7B,
//! constrained JSON via llguidance) → validator → cache JSON on disk + index
//! row in SQLite → existing `graph_builder` + `indexer`.
//!
//! See specs/006-llm-pdf-importer/ for the design source-of-truth.

pub mod types;
pub mod validator;
pub mod builder;
pub mod progress;
pub mod converter_from_constitution;
pub mod cache;
pub mod chunker;
pub mod prompts;
pub mod repair;
pub mod retry;
pub mod queue;
pub mod dispatch;
pub mod progress_store;

#[cfg(feature = "llm")]
pub mod grammar;
#[cfg(feature = "llm")]
pub mod mistral_builder;

/// Bump to force corpus-wide re-extraction of `extraction_method='llm_ir'`
/// rows on next launch (mirror of `structured_importer::STRUCTURED_IMPORTER_VERSION`).
pub const LLM_IR_IMPORTER_VERSION: i64 = 1;

/// Minimum extracted-text length below which the LLM is not invoked
/// (FR-028). Below this, the source is marked `extraction_method='unprocessable'`.
pub const MIN_TEXT_CHARS: usize = 200;

/// T025 — adapt a flat `DocIr` into the `structure_parser::DocNode` tree the
/// existing `persist_tree` consumes. Hides the flat-vs-tree mismatch so the
/// downstream pipeline doesn't need to change.
pub fn to_doc_node_tree(
    ir: &types::DocIr,
) -> crate::services::structure_parser::DocNode {
    use crate::services::structure_parser::{DocNode, NodeLevel};

    let title = ir.title.clone();

    // Build a parent_id → children index.
    let mut children_of: std::collections::HashMap<Option<&str>, Vec<&types::StructureNode>> =
        std::collections::HashMap::new();
    for n in &ir.nodes {
        children_of
            .entry(n.parent_id.as_deref())
            .or_default()
            .push(n);
    }

    fn map_kind(k: types::NodeKind) -> NodeLevel {
        use types::NodeKind as K;
        match k {
            K::Part => NodeLevel::Part,
            K::Chapter => NodeLevel::Chapter,
            K::Section | K::Subsection | K::Schedule | K::Preamble | K::Heading => {
                NodeLevel::Article
            }
            K::Clause => NodeLevel::Clause,
            K::SubClause | K::Proviso | K::Explanation | K::Paragraph | K::Footnote => {
                NodeLevel::SubClause
            }
        }
    }

    fn build_node<'a>(
        n: &'a types::StructureNode,
        children_of: &std::collections::HashMap<Option<&'a str>, Vec<&'a types::StructureNode>>,
        parent_path: &str,
    ) -> DocNode {
        let level = map_kind(n.kind);
        let label = n.number.clone().unwrap_or_default();
        let unit_path = if parent_path.is_empty() {
            format!("{}-{}", level_slug(level), label)
        } else {
            format!("{parent_path}/{}-{}", level_slug(level), label)
        };
        let citation = match (n.kind, &n.number) {
            (types::NodeKind::Part, Some(num)) => format!("Part {num}"),
            (types::NodeKind::Chapter, Some(num)) => format!("Chapter {num}"),
            (types::NodeKind::Section, Some(num)) => format!("Section {num}"),
            (types::NodeKind::Clause, Some(num)) => format!("Clause {num}"),
            (types::NodeKind::SubClause, Some(num)) => format!("Sub-clause {num}"),
            (_, Some(num)) => num.clone(),
            (_, None) => n.title.clone().unwrap_or_default(),
        };

        let children = children_of
            .get(&Some(n.id.as_str()))
            .cloned()
            .unwrap_or_default()
            .into_iter()
            .map(|c| build_node(c, children_of, &unit_path))
            .collect();

        DocNode {
            level,
            label,
            title: n.title.clone(),
            text: n.text.clone(),
            citation,
            unit_path,
            attached: Vec::new(),
            footnote_refs: n.footnote_refs.clone(),
            children,
        }
    }

    fn level_slug(l: NodeLevel) -> &'static str {
        match l {
            NodeLevel::Document => "doc",
            NodeLevel::Part => "Part",
            NodeLevel::Chapter => "Ch",
            NodeLevel::Article => "Art",
            NodeLevel::Clause => "cl",
            NodeLevel::SubClause => "sub",
        }
    }

    let roots: Vec<DocNode> = children_of
        .get(&None)
        .cloned()
        .unwrap_or_default()
        .into_iter()
        .map(|n| build_node(n, &children_of, ""))
        .collect();

    DocNode {
        level: NodeLevel::Document,
        label: String::new(),
        title: Some(title.clone()),
        text: String::new(),
        citation: title,
        unit_path: "document".to_string(),
        attached: Vec::new(),
        footnote_refs: Vec::new(),
        children: roots,
    }
}

#[cfg(test)]
mod adapter_tests {
    use super::*;
    use crate::services::structure_parser::NodeLevel;
    use types::*;

    fn ir() -> DocIr {
        DocIr {
            doc_id: "a".repeat(64),
            doc_type: DocType::Act,
            title: "Test Act".into(),
            source_pdf: "/x".into(),
            extracted_at: chrono::Utc::now(),
            llm_model: "stub".into(),
            schema_version: 1,
            nodes: vec![
                StructureNode {
                    id: "n0001".into(),
                    parent_id: None,
                    kind: NodeKind::Part,
                    number: Some("I".into()),
                    title: Some("First Part".into()),
                    text: String::new(),
                    page: 1,
                    annotations: vec![],
                    footnote_refs: vec![],
                    metadata: serde_json::Value::Null,
                    quality_flag: QualityFlag::Ok,
                    quality_error: None,
                },
                StructureNode {
                    id: "n0002".into(),
                    parent_id: Some("n0001".into()),
                    kind: NodeKind::Section,
                    number: Some("1".into()),
                    title: Some("Short title".into()),
                    text: "This Act may be called the Test Act.".into(),
                    page: 1,
                    annotations: vec![],
                    footnote_refs: vec![],
                    metadata: serde_json::Value::Null,
                    quality_flag: QualityFlag::Ok,
                    quality_error: None,
                },
            ],
            doc_metadata: serde_json::Value::Null,
        }
    }

    #[test]
    fn flat_ir_becomes_two_level_tree() {
        let tree = to_doc_node_tree(&ir());
        assert_eq!(tree.level, NodeLevel::Document);
        assert_eq!(tree.children.len(), 1);
        let part = &tree.children[0];
        assert_eq!(part.level, NodeLevel::Part);
        assert_eq!(part.children.len(), 1);
        let section = &part.children[0];
        assert_eq!(section.level, NodeLevel::Article);
        assert!(section.text.contains("Test Act"));
    }

    #[test]
    fn unit_path_is_constructed_from_parent_chain() {
        let tree = to_doc_node_tree(&ir());
        let section = &tree.children[0].children[0];
        assert!(section.unit_path.contains("Part-I"));
        assert!(section.unit_path.contains("Art-1"));
    }
}
