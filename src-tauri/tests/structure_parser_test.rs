//! S2 acceptance tests (feature 003, task T012/T013): the structure-aware
//! parser turns the verified `constitution_ir.json` into an L0–L5 `DocNode`
//! tree with provisos attached in-leaf.

use echo_ai_lib::services::constitution_ir;
use echo_ai_lib::services::doc_profiles::constitution::CONSTITUTION;
use echo_ai_lib::services::structure_parser::{parse_tree, AttachedKind, NodeLevel};
use std::path::PathBuf;

/// The S1 importer's committed output, relative to the `src-tauri` crate.
fn ir_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../tools/constitution_import/constitute_IR/constitution_ir.json")
}

#[test]
fn parse_tree_builds_six_level_hierarchy() {
    let ir = constitution_ir::load(&ir_path()).expect("load constitution_ir.json");
    let tree = parse_tree(&ir, &CONSTITUTION);

    assert_eq!(tree.level, NodeLevel::Document);
    assert_eq!(tree.count(NodeLevel::Part), 12, "12 Parts");
    // 2026-05-23: the Preamble import-gap fix added one article-level
    // node (`unit_label='Preamble'`) so the importer's 312 → 313 here.
    // Idea 1 (paragraph splitting at indexing time) then promotes the
    // Preamble's body into ~19 clause-level paragraph children.
    // 2026-05-29 (db8688f4): split_oversized_leaves now also applies to
    // internal-node own-text (threshold 500 chars), so 121 article
    // chapeau bodies got split into paragraph-children too. Clause
    // count is now 927: 787 IR + 19 Preamble + 121 article chapeau.
    // 2026-06-04 (f53b4e6a): F007 IR data patch injected 48 more article
    // nodes (15 missing articles + chapter-index chunks), 312 → 360.
    assert_eq!(tree.count(NodeLevel::Article), 361, "360 Articles + 1 Preamble parent");
    assert_eq!(
        tree.count(NodeLevel::Clause),
        1024,
        "927 (787 IR + 19 Preamble + 121 article-chapeau splits, db8688f4) \
         + 97 from the F007 IR data patch f53b4e6a"
    );
    assert!(
        tree.count(NodeLevel::SubClause) > 300,
        "sub-clause leaves present"
    );
    // Document → Part → Chapter → Article → Clause → Sub-clause → paragraph split.
    // 2026-05-29 (db8688f4): split_oversized_leaves now appends paragraph
    // children to any node with own-text > 500 chars, including SubClauses,
    // adding a 7th depth tier under the deepest leaves.
    assert_eq!(tree.depth(), 7, "six structural levels + paragraph-split tier");
}

#[test]
fn provisos_are_attached_in_leaf_not_as_nodes() {
    let ir = constitution_ir::load(&ir_path()).unwrap();
    let tree = parse_tree(&ir, &CONSTITUTION);

    // Article 18 is a flat article carrying an article-level proviso.
    let art18 = tree.find("Article 18").expect("Article 18");
    assert!(
        art18
            .attached
            .iter()
            .any(|a| a.kind == AttachedKind::Proviso),
        "Article 18's proviso must be attached in-leaf"
    );
    // Every clause/sub-clause is a real level — provisos are never tree nodes.
    let levels_ok = [
        NodeLevel::Document,
        NodeLevel::Part,
        NodeLevel::Chapter,
        NodeLevel::Article,
        NodeLevel::Clause,
        NodeLevel::SubClause,
    ];
    fn all_levels_in(node: &echo_ai_lib::services::structure_parser::DocNode, ok: &[NodeLevel]) {
        assert!(ok.contains(&node.level), "unexpected node level");
        for c in &node.children {
            all_levels_in(c, ok);
        }
    }
    all_levels_in(&tree, &levels_ok);
}

#[test]
fn citations_and_paths_are_canonical() {
    let ir = constitution_ir::load(&ir_path()).unwrap();
    let tree = parse_tree(&ir, &CONSTITUTION);

    // Article 8(3)(b) — the sub-clause recovered by the importer multi-table fix.
    let art8 = tree.find("Article 8").expect("Article 8");
    let cl3 = art8
        .children
        .iter()
        .find(|c| c.label == "(3)")
        .expect("clause 8(3)");
    assert_eq!(cl3.citation, "Article 8(3)");
    let sub_b = cl3
        .children
        .iter()
        .find(|s| s.label == "(b)")
        .expect("sub-clause 8(3)(b)");
    assert_eq!(sub_b.citation, "Article 8(3)(b)");
    assert!(
        sub_b.unit_path.ends_with("/Art-8/cl-(3)/sub-(b)"),
        "unit_path = {}",
        sub_b.unit_path
    );
}
