//! T008/T009 — validator for `DocIr` produced by the LLM builder.
//!
//! Two passes:
//!   1. Schema pass — `jsonschema` against the canonical schema (T006).
//!   2. Semantic pass — Rust function over parsed `DocIr`.
//!
//! Errors flag the offending node `LowConfidence`; warnings surface in the
//! repair UI but do not flip the flag.

use std::collections::{HashMap, HashSet};

use crate::services::doc_ir::types::{AnnotationKind, DocIr, NodeKind, StructureNode};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Severity {
    Error,
    Warning,
}

/// Closed enum — adding new codes requires a schema-version bump (per
/// data-model.md §6).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ValidationCode {
    MissingRequiredField,
    InvalidEnum,
    OrphanParent,
    CycleDetected,
    DuplicateId,
    EmptyLeafText,
    LowCoverage,
    NonMonotonicNumber,
    UnresolvedFootnoteRef,
    AnnotationMissingBy,
    OmissionMissingMarker,
    PageOutOfRange,
}

impl ValidationCode {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::MissingRequiredField => "MissingRequiredField",
            Self::InvalidEnum => "InvalidEnum",
            Self::OrphanParent => "OrphanParent",
            Self::CycleDetected => "CycleDetected",
            Self::DuplicateId => "DuplicateId",
            Self::EmptyLeafText => "EmptyLeafText",
            Self::LowCoverage => "LowCoverage",
            Self::NonMonotonicNumber => "NonMonotonicNumber",
            Self::UnresolvedFootnoteRef => "UnresolvedFootnoteRef",
            Self::AnnotationMissingBy => "AnnotationMissingBy",
            Self::OmissionMissingMarker => "OmissionMissingMarker",
            Self::PageOutOfRange => "PageOutOfRange",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ValidationIssue {
    pub node_id: String,
    pub severity: Severity,
    pub code: ValidationCode,
    pub message: String,
}

/// Pass 1 — JSON-schema validation of a raw `serde_json::Value`. Used before
/// `serde_json::from_value` to surface schema errors with structural context.
///
/// Returns issues with empty `node_id` since the schema layer doesn't know
/// which node owns the error.
pub fn validate_schema(payload: &serde_json::Value) -> Vec<ValidationIssue> {
    use crate::services::doc_ir::types::SCHEMA_JSON;

    let schema: serde_json::Value = match serde_json::from_str(SCHEMA_JSON) {
        Ok(v) => v,
        Err(_) => {
            return vec![ValidationIssue {
                node_id: String::new(),
                severity: Severity::Error,
                code: ValidationCode::MissingRequiredField,
                message: "embedded schema.json failed to parse (build error)".into(),
            }];
        }
    };

    let compiled = match jsonschema::JSONSchema::options()
        .with_draft(jsonschema::Draft::Draft202012)
        .compile(&schema)
    {
        Ok(s) => s,
        Err(e) => {
            return vec![ValidationIssue {
                node_id: String::new(),
                severity: Severity::Error,
                code: ValidationCode::MissingRequiredField,
                message: format!("schema compile failed: {e}"),
            }];
        }
    };

    // Materialize errors immediately — `validate()` returns an iterator that
    // borrows `compiled`, so we must collect the formatted strings before the
    // borrow goes out of scope.
    let raw: Vec<(String, String)> = match compiled.validate(payload) {
        Ok(_) => Vec::new(),
        Err(errors) => errors
            .map(|e| (e.to_string(), e.instance_path.to_string()))
            .collect(),
    };
    raw.into_iter()
        .map(|(msg, path)| {
            let code = classify_schema_error(&msg);
            ValidationIssue {
                node_id: extract_node_id_from_path(&path),
                severity: Severity::Error,
                code,
                message: format!("{msg} (at {path})"),
            }
        })
        .collect()
}

fn classify_schema_error(msg: &str) -> ValidationCode {
    let lower = msg.to_lowercase();
    // Enum / pattern / const violations — `jsonschema` phrases enum failures
    // as "X is not one of [...]" and pattern failures as "X does not match …".
    if lower.contains("not one of")
        || lower.contains("does not match")
        || lower.contains(" enum")
        || lower.contains("const")
    {
        ValidationCode::InvalidEnum
    } else {
        ValidationCode::MissingRequiredField
    }
}

fn extract_node_id_from_path(path: &str) -> String {
    // path like "/nodes/3/text"; we can't get the actual id without the
    // payload, so surface the index as a fallback identifier.
    path.to_string()
}

/// Pass 2 — semantic validation over a parsed `DocIr`. Caller has already
/// passed `validate_schema` (or accepted its issues).
///
/// `source_char_count` is the count of non-whitespace characters in the
/// extracted PDF text — used for the ≥90% coverage check (FR-007, SC-002).
pub fn validate_semantic(ir: &DocIr, source_char_count: usize) -> Vec<ValidationIssue> {
    let mut issues = Vec::new();

    // Build id index.
    let mut seen: HashSet<&str> = HashSet::new();
    let mut id_to_node: HashMap<&str, &StructureNode> = HashMap::new();
    for n in &ir.nodes {
        if !seen.insert(&n.id) {
            issues.push(ValidationIssue {
                node_id: n.id.clone(),
                severity: Severity::Error,
                code: ValidationCode::DuplicateId,
                message: format!("node id `{}` appears more than once", n.id),
            });
        }
        id_to_node.insert(&n.id, n);
    }

    // Parent integrity + cycle check.
    for n in &ir.nodes {
        if let Some(pid) = &n.parent_id {
            if !id_to_node.contains_key(pid.as_str()) {
                issues.push(ValidationIssue {
                    node_id: n.id.clone(),
                    severity: Severity::Error,
                    code: ValidationCode::OrphanParent,
                    message: format!("parent `{pid}` does not exist"),
                });
            }
        }
    }
    for n in &ir.nodes {
        if has_cycle(n, &id_to_node) {
            issues.push(ValidationIssue {
                node_id: n.id.clone(),
                severity: Severity::Error,
                code: ValidationCode::CycleDetected,
                message: format!("ancestor chain of `{}` cycles", n.id),
            });
        }
    }

    // Per-node checks.
    for n in &ir.nodes {
        check_node(n, &id_to_node, &mut issues);
    }

    // Monotonic numbering within siblings (warning only).
    check_monotonic_siblings(ir, &mut issues);

    // Footnote references resolve to kind=footnote.
    let footnote_ids: HashSet<&str> = ir
        .nodes
        .iter()
        .filter(|n| n.kind == NodeKind::Footnote)
        .map(|n| n.id.as_str())
        .collect();
    for n in &ir.nodes {
        for fref in &n.footnote_refs {
            if !footnote_ids.contains(fref.as_str()) {
                issues.push(ValidationIssue {
                    node_id: n.id.clone(),
                    severity: Severity::Error,
                    code: ValidationCode::UnresolvedFootnoteRef,
                    message: format!("footnote_ref `{fref}` does not resolve to a footnote node"),
                });
            }
        }
    }

    // Coverage: sum of non-whitespace chars across all node.text vs
    // source_char_count (also non-whitespace).
    if source_char_count > 0 {
        let covered: usize = ir
            .nodes
            .iter()
            .map(|n| n.text.chars().filter(|c| !c.is_whitespace()).count())
            .sum();
        let ratio = covered as f64 / source_char_count as f64;
        if ratio < 0.90 {
            issues.push(ValidationIssue {
                node_id: String::new(),
                severity: Severity::Error,
                code: ValidationCode::LowCoverage,
                message: format!(
                    "text coverage {:.1}% < 90% (covered {covered} / source {source_char_count} non-whitespace chars)",
                    ratio * 100.0
                ),
            });
        }
    }

    issues
}

fn has_cycle<'a>(start: &'a StructureNode, idx: &HashMap<&'a str, &'a StructureNode>) -> bool {
    let mut seen: HashSet<&str> = HashSet::new();
    let mut cur = start;
    while let Some(pid) = &cur.parent_id {
        if !seen.insert(pid.as_str()) {
            return true;
        }
        match idx.get(pid.as_str()) {
            Some(p) => cur = p,
            None => return false, // orphan already reported
        }
        if seen.len() > 10_000 {
            return true; // safety
        }
    }
    false
}

fn check_node<'a>(
    n: &'a StructureNode,
    idx: &HashMap<&'a str, &'a StructureNode>,
    issues: &mut Vec<ValidationIssue>,
) {
    let is_leaf = !idx.values().any(|m| m.parent_id.as_deref() == Some(n.id.as_str()));
    let has_omission = n
        .annotations
        .iter()
        .any(|a| a.kind == AnnotationKind::Omission);

    // EmptyLeafText: a leaf with empty text must be marked as an omission.
    if is_leaf
        && n.text.trim().is_empty()
        && !has_omission
        && n.kind != NodeKind::Part
        && n.kind != NodeKind::Chapter
    {
        issues.push(ValidationIssue {
            node_id: n.id.clone(),
            severity: Severity::Error,
            code: ValidationCode::EmptyLeafText,
            message: "leaf node has empty text without an omission annotation".into(),
        });
    }

    // Empty-text non-leaf must still satisfy OmissionMissingMarker check only
    // when text is explicitly empty AND the node looks like a deleted slot.
    if n.text.trim().is_empty() && n.number.is_some() && !has_omission && is_leaf {
        // already covered by EmptyLeafText above; specific marker check below.
    }
    if n.text.trim().is_empty() && has_marker_intent(n) && !has_omission {
        issues.push(ValidationIssue {
            node_id: n.id.clone(),
            severity: Severity::Error,
            code: ValidationCode::OmissionMissingMarker,
            message: "empty-text node lacks an `omission` annotation".into(),
        });
    }

    // Annotation MUST have non-empty `by`.
    for a in &n.annotations {
        if a.by.trim().is_empty() {
            issues.push(ValidationIssue {
                node_id: n.id.clone(),
                severity: Severity::Error,
                code: ValidationCode::AnnotationMissingBy,
                message: format!("annotation `{:?}` missing `by`", a.kind),
            });
        }
    }
}

fn has_marker_intent(n: &StructureNode) -> bool {
    // Heuristic — a node *looks like* it should be a marker when it has a
    // number and no children and no text. The check_node leaf branch covers
    // this, but we keep this hook for future profile-specific extensions.
    n.number.is_some()
}

fn check_monotonic_siblings(ir: &DocIr, issues: &mut Vec<ValidationIssue>) {
    let mut by_parent: HashMap<Option<&str>, Vec<&StructureNode>> = HashMap::new();
    for n in &ir.nodes {
        by_parent
            .entry(n.parent_id.as_deref())
            .or_default()
            .push(n);
    }
    for (_parent, sibs) in by_parent.iter() {
        let nums: Vec<(usize, &StructureNode, Option<u64>)> = sibs
            .iter()
            .enumerate()
            .map(|(i, n)| (i, *n, n.number.as_deref().and_then(extract_leading_int)))
            .filter(|(_, _, k)| k.is_some())
            .collect();

        // Walk by document order; warn when integer numbers go backwards.
        let mut prev: Option<u64> = None;
        for (_, node, num) in &nums {
            if let (Some(p), Some(c)) = (prev, num) {
                if *c < p {
                    issues.push(ValidationIssue {
                        node_id: node.id.clone(),
                        severity: Severity::Warning,
                        code: ValidationCode::NonMonotonicNumber,
                        message: format!(
                            "sibling number `{}` decreases from previous `{p}`",
                            node.number.as_deref().unwrap_or("")
                        ),
                    });
                }
            }
            prev = *num;
        }
    }
}

fn extract_leading_int(s: &str) -> Option<u64> {
    let mut digits = String::new();
    for c in s.chars() {
        if c.is_ascii_digit() {
            digits.push(c);
        } else if !digits.is_empty() {
            break;
        }
    }
    digits.parse().ok()
}

/// Convenience: page-out-of-range check given a known page count.
pub fn validate_page_bounds(ir: &DocIr, page_count: u32) -> Vec<ValidationIssue> {
    ir.nodes
        .iter()
        .filter(|n| n.page > page_count)
        .map(|n| ValidationIssue {
            node_id: n.id.clone(),
            severity: Severity::Warning,
            code: ValidationCode::PageOutOfRange,
            message: format!("page {} exceeds source PDF page count {page_count}", n.page),
        })
        .collect()
}

/// Top-level convenience — runs schema + semantic + (optional) page-bounds.
pub fn validate_doc_ir(
    ir: &DocIr,
    source_char_count: usize,
    page_count: Option<u32>,
) -> Vec<ValidationIssue> {
    let mut out = match serde_json::to_value(ir) {
        Ok(v) => validate_schema(&v),
        Err(_) => Vec::new(),
    };
    out.extend(validate_semantic(ir, source_char_count));
    if let Some(pc) = page_count {
        out.extend(validate_page_bounds(ir, pc));
    }
    out
}

/// Utility for callers: count non-whitespace chars in extracted text — the
/// canonical denominator for coverage checks (SC-002, FR-007).
pub fn non_whitespace_char_count(s: &str) -> usize {
    s.chars().filter(|c| !c.is_whitespace()).count()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::services::doc_ir::types::*;

    fn node(id: &str, parent: Option<&str>, kind: NodeKind, text: &str) -> StructureNode {
        StructureNode {
            id: id.into(),
            parent_id: parent.map(|s| s.into()),
            kind,
            number: None,
            title: None,
            text: text.into(),
            page: 1,
            annotations: vec![],
            footnote_refs: vec![],
            metadata: serde_json::Value::Null,
            quality_flag: QualityFlag::Ok,
            quality_error: None,
        }
    }

    fn ir_with(nodes: Vec<StructureNode>) -> DocIr {
        DocIr {
            doc_id: "a".repeat(64),
            doc_type: DocType::Act,
            title: "T".into(),
            source_pdf: "/x".into(),
            extracted_at: chrono::Utc::now(),
            llm_model: "test".into(),
            schema_version: 1,
            nodes,
            doc_metadata: serde_json::Value::Object(Default::default()),
        }
    }

    #[test]
    fn detects_duplicate_id() {
        let ir = ir_with(vec![
            node("n0001", None, NodeKind::Section, "hello world"),
            node("n0001", None, NodeKind::Section, "duplicate"),
        ]);
        let issues = validate_semantic(&ir, 0);
        assert!(issues
            .iter()
            .any(|i| i.code == ValidationCode::DuplicateId));
    }

    #[test]
    fn detects_orphan_parent() {
        let ir = ir_with(vec![node("n0002", Some("n9999"), NodeKind::Clause, "x")]);
        let issues = validate_semantic(&ir, 0);
        assert!(issues
            .iter()
            .any(|i| i.code == ValidationCode::OrphanParent));
    }

    #[test]
    fn detects_low_coverage() {
        // Source has 100 non-ws chars, node text only has 50 → < 90%.
        let ir = ir_with(vec![node("n0001", None, NodeKind::Section, &"x".repeat(50))]);
        let issues = validate_semantic(&ir, 100);
        assert!(issues
            .iter()
            .any(|i| i.code == ValidationCode::LowCoverage));
    }

    #[test]
    fn passes_when_coverage_meets_threshold() {
        let ir = ir_with(vec![node("n0001", None, NodeKind::Section, &"x".repeat(95))]);
        let issues = validate_semantic(&ir, 100);
        assert!(!issues
            .iter()
            .any(|i| i.code == ValidationCode::LowCoverage));
    }

    #[test]
    fn unresolved_footnote_ref_flagged() {
        let mut n = node("n0001", None, NodeKind::Section, "see footnote");
        n.footnote_refs = vec!["n9999".into()];
        let issues = validate_semantic(&ir_with(vec![n]), 0);
        assert!(issues
            .iter()
            .any(|i| i.code == ValidationCode::UnresolvedFootnoteRef));
    }

    #[test]
    fn empty_leaf_without_omission_flagged() {
        let n = node("n0001", None, NodeKind::Clause, "");
        let issues = validate_semantic(&ir_with(vec![n]), 0);
        assert!(issues
            .iter()
            .any(|i| i.code == ValidationCode::EmptyLeafText));
    }

    #[test]
    fn empty_leaf_with_omission_passes() {
        let mut n = node("n0001", None, NodeKind::Clause, "");
        n.annotations = vec![Annotation {
            kind: AnnotationKind::Omission,
            by: "Act II of 2020".into(),
            by_date: None,
            footnote_id: None,
            original: None,
            note: None,
        }];
        let issues = validate_semantic(&ir_with(vec![n]), 0);
        assert!(!issues
            .iter()
            .any(|i| i.code == ValidationCode::EmptyLeafText));
    }

    #[test]
    fn annotation_missing_by_flagged() {
        let mut n = node("n0001", None, NodeKind::Section, "x");
        n.annotations = vec![Annotation {
            kind: AnnotationKind::Insertion,
            by: "   ".into(),
            by_date: None,
            footnote_id: None,
            original: None,
            note: None,
        }];
        let issues = validate_semantic(&ir_with(vec![n]), 0);
        assert!(issues
            .iter()
            .any(|i| i.code == ValidationCode::AnnotationMissingBy));
    }

    #[test]
    fn cycle_detected() {
        let a = node("n0001", Some("n0002"), NodeKind::Section, "a");
        let b = node("n0002", Some("n0001"), NodeKind::Section, "b");
        let issues = validate_semantic(&ir_with(vec![a, b]), 0);
        assert!(issues
            .iter()
            .any(|i| i.code == ValidationCode::CycleDetected));
    }

    #[test]
    fn non_monotonic_warning() {
        let mut a = node("n0001", None, NodeKind::Section, "x");
        a.number = Some("5".into());
        let mut b = node("n0002", None, NodeKind::Section, "y");
        b.number = Some("3".into());
        let issues = validate_semantic(&ir_with(vec![a, b]), 0);
        assert!(issues
            .iter()
            .any(|i| i.code == ValidationCode::NonMonotonicNumber && i.severity == Severity::Warning));
    }

    #[test]
    fn page_out_of_range_warning() {
        let mut n = node("n0001", None, NodeKind::Section, "x");
        n.page = 999;
        let issues = validate_page_bounds(&ir_with(vec![n]), 10);
        assert!(issues
            .iter()
            .any(|i| i.code == ValidationCode::PageOutOfRange));
    }
}
