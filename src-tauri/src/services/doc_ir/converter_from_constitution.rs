//! T013 — Bridge from the legacy nested `ConstitutionIr` (feature 003) to
//! the flat `DocIr` shape (feature 006).
//!
//! Used by tests and as a one-way reference; the constitution runtime path
//! remains unchanged in v1 per plan.md §Complexity Tracking row 2.

use crate::services::constitution_ir as cir;
use crate::services::doc_ir::types::{
    Annotation, AnnotationKind, DocIr, DocType, NodeKind, QualityFlag, StructureNode,
};

/// Convert a parsed `ConstitutionIr` into a `DocIr`. The conversion is
/// structural-only — no LLM, deterministic.
pub fn constitution_ir_to_doc_ir(c: &cir::ConstitutionIr, doc_id: String, source_pdf: String) -> DocIr {
    let mut ctx = Ctx::default();

    // Preamble as a single node.
    if let Some(preamble) = &c.preamble {
        let id = ctx.next_id();
        ctx.nodes.push(StructureNode {
            id,
            parent_id: None,
            kind: NodeKind::Preamble,
            number: None,
            title: Some("Preamble".into()),
            text: preamble.clone(),
            page: 1,
            annotations: vec![],
            footnote_refs: vec![],
            metadata: serde_json::Value::Null,
            quality_flag: QualityFlag::Ok,
            quality_error: None,
        });
    }

    // Footnotes as first-class nodes — emit early so they can be referenced
    // by id from articles/clauses below.
    let mut footnote_lookup: std::collections::HashMap<String, String> =
        std::collections::HashMap::new();
    for fn_ in &c.footnotes {
        let id = ctx.next_id();
        footnote_lookup.insert(fn_.id.clone(), id.clone());
        ctx.nodes.push(StructureNode {
            id,
            parent_id: None,
            kind: NodeKind::Footnote,
            number: Some(fn_.id.clone()),
            title: None,
            text: fn_.raw.clone(),
            page: 1,
            annotations: vec![Annotation {
                kind: AnnotationKind::Substitution,
                by: fn_.instrument.clone(),
                by_date: fn_.effective_date.clone(),
                footnote_id: None,
                original: fn_.previous_text.clone(),
                note: Some(fn_.change.clone()),
            }],
            footnote_refs: vec![],
            metadata: serde_json::Value::Null,
            quality_flag: QualityFlag::Ok,
            quality_error: None,
        });
    }

    // Parts and their children.
    for part in &c.parts {
        let part_id = emit(
            &mut ctx,
            None,
            NodeKind::Part,
            Some(part.number.clone()),
            Some(part.title.clone()),
            String::new(),
        );

        for chapter in &part.chapters {
            let chapter_id = emit(
                &mut ctx,
                Some(part_id.clone()),
                NodeKind::Chapter,
                Some(chapter.number.clone()),
                Some(chapter.title.clone()),
                String::new(),
            );
            for article in &chapter.articles {
                emit_article(&mut ctx, &footnote_lookup, &chapter_id, article);
            }
        }

        for article in &part.articles {
            emit_article(&mut ctx, &footnote_lookup, &part_id, article);
        }
    }

    DocIr {
        doc_id,
        doc_type: DocType::Constitution,
        title: c.title.clone(),
        source_pdf,
        extracted_at: chrono::Utc::now(),
        llm_model: "converter_from_constitution_ir".into(),
        schema_version: 1,
        nodes: ctx.nodes,
        doc_metadata: serde_json::Value::Object(Default::default()),
    }
}

#[derive(Default)]
struct Ctx {
    next: u32,
    nodes: Vec<StructureNode>,
}

impl Ctx {
    fn next_id(&mut self) -> String {
        self.next += 1;
        format!("n{:04}", self.next)
    }
}

fn emit(
    ctx: &mut Ctx,
    parent: Option<String>,
    kind: NodeKind,
    number: Option<String>,
    title: Option<String>,
    text: String,
) -> String {
    let id = ctx.next_id();
    ctx.nodes.push(StructureNode {
        id: id.clone(),
        parent_id: parent,
        kind,
        number,
        title,
        text,
        page: 1,
        annotations: vec![],
        footnote_refs: vec![],
        metadata: serde_json::Value::Null,
        quality_flag: QualityFlag::Ok,
        quality_error: None,
    });
    id
}

fn resolve_refs(
    lookup: &std::collections::HashMap<String, String>,
    refs: &[String],
) -> Vec<String> {
    refs.iter()
        .filter_map(|r| lookup.get(r).cloned())
        .collect()
}

fn emit_article(
    ctx: &mut Ctx,
    fn_lookup: &std::collections::HashMap<String, String>,
    parent_id: &str,
    a: &cir::Article,
) {
    let id = ctx.next_id();
    ctx.nodes.push(StructureNode {
        id: id.clone(),
        parent_id: Some(parent_id.into()),
        kind: NodeKind::Section,
        number: Some(a.number.clone()),
        title: Some(a.marginal_note.clone()),
        text: a.text.clone().unwrap_or_default(),
        page: 1,
        annotations: vec![],
        footnote_refs: resolve_refs(fn_lookup, &a.footnote_refs),
        metadata: serde_json::Value::Object(Default::default()),
        quality_flag: QualityFlag::Ok,
        quality_error: None,
    });

    for c in &a.clauses {
        emit_clause(ctx, fn_lookup, &id, c);
    }
    for p in &a.provisos {
        emit(
            ctx,
            Some(id.clone()),
            NodeKind::Proviso,
            None,
            None,
            p.text.clone(),
        );
    }
    for e in &a.explanations {
        emit(
            ctx,
            Some(id.clone()),
            NodeKind::Explanation,
            Some(e.label.clone()),
            None,
            e.text.clone(),
        );
    }
}

fn emit_clause(
    ctx: &mut Ctx,
    fn_lookup: &std::collections::HashMap<String, String>,
    parent_id: &str,
    c: &cir::Clause,
) {
    let id = ctx.next_id();
    ctx.nodes.push(StructureNode {
        id: id.clone(),
        parent_id: Some(parent_id.into()),
        kind: NodeKind::Clause,
        number: Some(c.id.clone()),
        title: None,
        text: c.text.clone(),
        page: 1,
        annotations: vec![],
        footnote_refs: resolve_refs(fn_lookup, &c.footnote_refs),
        metadata: serde_json::Value::Null,
        quality_flag: QualityFlag::Ok,
        quality_error: None,
    });
    for sc in &c.subclauses {
        emit_subclause(ctx, fn_lookup, &id, sc);
    }
    for p in &c.provisos {
        emit(
            ctx,
            Some(id.clone()),
            NodeKind::Proviso,
            None,
            None,
            p.text.clone(),
        );
    }
    for e in &c.explanations {
        emit(
            ctx,
            Some(id.clone()),
            NodeKind::Explanation,
            Some(e.label.clone()),
            None,
            e.text.clone(),
        );
    }
}

fn emit_subclause(
    ctx: &mut Ctx,
    fn_lookup: &std::collections::HashMap<String, String>,
    parent_id: &str,
    sc: &cir::SubClause,
) {
    let id = ctx.next_id();
    ctx.nodes.push(StructureNode {
        id: id.clone(),
        parent_id: Some(parent_id.into()),
        kind: NodeKind::SubClause,
        number: Some(sc.id.clone()),
        title: None,
        text: sc.text.clone(),
        page: 1,
        annotations: vec![],
        footnote_refs: resolve_refs(fn_lookup, &sc.footnote_refs),
        metadata: serde_json::Value::Null,
        quality_flag: QualityFlag::Ok,
        quality_error: None,
    });
    for p in &sc.provisos {
        emit(
            ctx,
            Some(id.clone()),
            NodeKind::Proviso,
            None,
            None,
            p.text.clone(),
        );
    }
    for e in &sc.explanations {
        emit(
            ctx,
            Some(id.clone()),
            NodeKind::Explanation,
            Some(e.label.clone()),
            None,
            e.text.clone(),
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cir_minimal() -> cir::ConstitutionIr {
        cir::ConstitutionIr {
            title: "Test Constitution".into(),
            preamble: Some("We the people".into()),
            amendments: vec![],
            footnotes: vec![cir::Footnote {
                id: "f1".into(),
                change: "subs".into(),
                instrument: "Act II of 2020".into(),
                effective_date: Some("2020-01-01".into()),
                previous_text: Some("old text".into()),
                raw: "1. Subs. by Act II of 2020".into(),
            }],
            parts: vec![cir::Part {
                number: "I".into(),
                title: "Part One".into(),
                articles: vec![cir::Article {
                    number: "1".into(),
                    marginal_note: "Short title".into(),
                    text: Some("This Act may be called...".into()),
                    clauses: vec![cir::Clause {
                        id: "(1)".into(),
                        text: "Clause text".into(),
                        subclauses: vec![],
                        provisos: vec![],
                        explanations: vec![],
                        footnote_refs: vec!["f1".into()],
                    }],
                    provisos: vec![],
                    explanations: vec![],
                    footnote_refs: vec![],
                }],
                chapters: vec![],
            }],
        }
    }

    #[test]
    fn converts_constitution_with_preamble_footnote_part_article_clause() {
        let c = cir_minimal();
        let ir = constitution_ir_to_doc_ir(&c, "a".repeat(64), "/x.pdf".into());
        assert_eq!(ir.doc_type, DocType::Constitution);
        // 1 preamble + 1 footnote + 1 part + 1 article + 1 clause = 5 nodes
        assert_eq!(ir.nodes.len(), 5);
        assert!(ir.nodes.iter().any(|n| n.kind == NodeKind::Preamble));
        assert!(ir.nodes.iter().any(|n| n.kind == NodeKind::Footnote));
        assert!(ir.nodes.iter().any(|n| n.kind == NodeKind::Part));
        assert!(ir.nodes.iter().any(|n| n.kind == NodeKind::Section));
        assert!(ir.nodes.iter().any(|n| n.kind == NodeKind::Clause));
    }

    #[test]
    fn footnote_refs_resolve_to_footnote_node_ids() {
        let c = cir_minimal();
        let ir = constitution_ir_to_doc_ir(&c, "b".repeat(64), "/y.pdf".into());
        let clause = ir.nodes.iter().find(|n| n.kind == NodeKind::Clause).unwrap();
        let footnote = ir.nodes.iter().find(|n| n.kind == NodeKind::Footnote).unwrap();
        assert_eq!(clause.footnote_refs, vec![footnote.id.clone()]);
    }

    #[test]
    fn parent_chain_matches_nested_tree() {
        let c = cir_minimal();
        let ir = constitution_ir_to_doc_ir(&c, "c".repeat(64), "/z.pdf".into());
        let part = ir.nodes.iter().find(|n| n.kind == NodeKind::Part).unwrap();
        let article = ir.nodes.iter().find(|n| n.kind == NodeKind::Section).unwrap();
        let clause = ir.nodes.iter().find(|n| n.kind == NodeKind::Clause).unwrap();
        assert_eq!(article.parent_id.as_deref(), Some(part.id.as_str()));
        assert_eq!(clause.parent_id.as_deref(), Some(article.id.as_str()));
    }
}
