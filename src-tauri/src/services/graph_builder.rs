//! Graph builder (feature 003, S4 task T019): derive `graph_nodes` and
//! `graph_edges` from a persisted `DocNode` tree.
//!
//! Edge types built in v1:
//!  - `contains`        — parent → child, from the hierarchy tree.
//!  - `cross_reference` — a unit → the provision it names ("Article N", "Rule N",
//!                        "Section N", …) in its text.
//!  - `non_obstante`    — a unit → a provision it overrides; emitted when the
//!                        unit's text carries "notwithstanding".
//!
//! 2026-07-12 — provision-kind generalisation. The reference extractor used to
//! match "Article N" ONLY, so the relational overlay existed for the Constitution
//! alone; every Rule/Act ("Rule 16", "Section 9") produced zero cross-reference
//! edges (live audit: 116 overlay edges, all `constitution.pdf`). It now matches
//! the full legal-citation vocabulary and resolves each reference against the
//! document's own citation set via a NORMALISED key (case/hyphen/plural-tolerant),
//! so the target format need not be hard-coded per doc-type (stored citations
//! vary: `Rule 5(6)`, `Section 142`, `Schedule-II`, `Article 19A`).
//!
//! `amended_by` is intentionally NOT built here (Phase 2). Migration 014 models
//! `graph_edges.to_node` as a NOT NULL FK to `graph_nodes`, and `node_type`
//! admits only the six hierarchy levels — there is no amendment node to point
//! at. Amendment provenance lives in `amendment_events`; an `amended_by` overlay
//! (to the amending instrument's document node) is wired in a later pass.
//! See CHANGELOG_DEV 2026-05-22 (Constitution S4) + 2026-07-12 (GraphRAG).

use std::collections::HashMap;
use std::sync::OnceLock;

use regex_lite::Regex;
use uuid::Uuid;

use super::structure_parser::{DocNode, NodeLevel};
use crate::db::repositories::chunks::Chunk;
use crate::db::repositories::graph::{GraphEdge, GraphNode};

/// Matches a provision reference — a legal-citation kind word followed by an
/// arabic number (optionally with a letter suffix, e.g. "19A"). Case-insensitive;
/// the plural form ("Rules 3 and 4") is tolerated via the optional trailing `s`.
///
/// v1 is **arabic-only on purpose** — roman-numbered Parts/Schedules ("Part IV")
/// are few and matching bare romans (`i`, `v`, `x`) against ordinary words would
/// mint false legal edges, which are worse than none (correctness-first). Roman
/// support is a scoped follow-up.
pub(crate) fn provision_ref_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(
            r"(?i)\b(article|section|rule|regulation|clause|chapter|part|schedule)s?\s+(\d+[A-Za-z]?)\b",
        )
        .unwrap()
    })
}

/// Distinct provision references in `text`, each normalised to a lookup key
/// (`ruleN`, `sectionN`, …), in first-seen order.
pub(crate) fn provision_refs(text: &str) -> Vec<String> {
    let mut seen: Vec<String> = Vec::new();
    for caps in provision_ref_re().captures_iter(text) {
        let kind = caps.get(1).unwrap().as_str();
        let number = caps.get(2).unwrap().as_str();
        let key = provision_key(kind, number);
        if !seen.contains(&key) {
            seen.push(key);
        }
    }
    seen
}

/// Canonical lookup key for a (kind, number) pair: lowercased kind (singular,
/// trailing plural `s` stripped) + upper-cased number, no separators. So the
/// reference "Rules 3" and the stored citation "Rule 3" collapse to `rule3`;
/// "SCHEDULE-II"/"Schedule II" → `scheduleii`; "Article 19a" → `article19A`.
pub(crate) fn provision_key(kind: &str, number: &str) -> String {
    let mut k = kind.trim().to_lowercase();
    if let Some(stripped) = k.strip_suffix('s') {
        k = stripped.to_string();
    }
    format!("{k}{}", number.trim().to_uppercase())
}

/// Split a stored citation head ("Rule 5(6)", "Schedule-II", "Part IV") into its
/// (kind, number) and return the normalised `provision_key`. The head is
/// everything before the first sub-clause paren; kind is the leading alphabetic
/// run, number the alphanumeric token after the space/hyphen separator.
pub(crate) fn citation_key(citation: &str) -> Option<String> {
    let head = citation.split('(').next().unwrap_or(citation).trim();
    let kind_end = head.find(|c: char| !c.is_ascii_alphabetic())?;
    let kind = &head[..kind_end];
    if kind.is_empty() {
        return None;
    }
    let rest = head[kind_end..].trim_start_matches([' ', '-', '.', '\u{2013}']);
    let num_end = rest
        .find(|c: char| !c.is_ascii_alphanumeric())
        .unwrap_or(rest.len());
    let number = &rest[..num_end];
    if number.is_empty() || !number.chars().any(|c| c.is_ascii_digit()) {
        return None;
    }
    Some(provision_key(kind, number))
}

/// Build the graph node + edge sets from a persisted tree.
///
/// `persisted` is the output of `structure_parser::persist_tree`; each chunk's
/// `unit_label` is its canonical citation, which supplies the NOT NULL
/// `graph_nodes.chunk_id` FK. (This extends the data-model.md §5
/// `build_graph(tree)` sketch — the FK requires the persisted chunk ids.)
pub fn build_graph(tree: &DocNode, persisted: &[Chunk]) -> (Vec<GraphNode>, Vec<GraphEdge>) {
    let chunk_id: HashMap<&str, &str> = persisted
        .iter()
        .filter_map(|c| c.unit_label.as_deref().map(|l| (l, c.id.as_str())))
        .collect();

    let mut nodes: Vec<GraphNode> = Vec::new();
    let mut gid: HashMap<String, String> = HashMap::new();
    collect_nodes(tree, &chunk_id, &mut nodes, &mut gid);

    // Normalised citation → graph-node id, over PROVISION HEADS (article-level)
    // only. References resolve to the provision head (e.g. "Rule 5(6)" in text
    // → the "Rule 5" node), never a sub-clause, so a single reference yields one
    // stable edge target. Article-level nodes win on key collision.
    let mut head_index: HashMap<String, String> = HashMap::new();
    for n in &nodes {
        if n.node_type != NodeLevel::Article.unit_type() {
            continue;
        }
        if let Some(cit) = n.citation.as_deref().and_then(citation_key) {
            head_index.insert(cit, n.id.clone());
        }
    }

    let mut edges: Vec<GraphEdge> = Vec::new();
    collect_edges(tree, None, None, &gid, &head_index, &mut edges);

    (nodes, edges)
}

/// Pass 1 — one `GraphNode` per tree node; record citation → graph-node id.
fn collect_nodes(
    node: &DocNode,
    chunk_id: &HashMap<&str, &str>,
    nodes: &mut Vec<GraphNode>,
    gid: &mut HashMap<String, String>,
) {
    let id = Uuid::new_v4().to_string();
    nodes.push(GraphNode {
        id: id.clone(),
        chunk_id: chunk_id
            .get(node.citation.as_str())
            .map(|s| (*s).to_string())
            .unwrap_or_default(),
        node_type: node.level.unit_type().to_string(),
        citation: Some(node.citation.clone()),
        metrics: None,
    });
    gid.insert(node.citation.clone(), id);
    for child in &node.children {
        collect_nodes(child, chunk_id, nodes, gid);
    }
}

/// Pass 2 — `contains` tree edges plus the `cross_reference` / `non_obstante`
/// overlay parsed from each node's text.
fn collect_edges(
    node: &DocNode,
    parent_gid: Option<&str>,
    article_citation: Option<&str>,
    gid: &HashMap<String, String>,
    head_index: &HashMap<String, String>,
    edges: &mut Vec<GraphEdge>,
) {
    let self_gid = gid.get(&node.citation).cloned().unwrap_or_default();

    if let Some(parent) = parent_gid {
        edges.push(edge(parent, &self_gid, "contains"));
    }

    // The Article enclosing this node (an Article encloses itself).
    let enclosing = if node.level == NodeLevel::Article {
        Some(node.citation.as_str())
    } else {
        article_citation
    };
    // Normalised key of the enclosing provision — a unit referencing its own
    // provision (in any spelling) is not an edge.
    let enclosing_key = enclosing.and_then(citation_key);

    if !node.text.is_empty() {
        let non_obstante = node.text.to_lowercase().contains("notwithstanding");
        for key in provision_refs(&node.text) {
            if Some(&key) == enclosing_key.as_ref() {
                continue; // self-reference
            }
            if let Some(target_gid) = head_index.get(&key) {
                if *target_gid == self_gid {
                    continue; // resolves back to this very node
                }
                edges.push(edge(&self_gid, target_gid, "cross_reference"));
                if non_obstante {
                    edges.push(edge(&self_gid, target_gid, "non_obstante"));
                }
            }
        }
    }

    for child in &node.children {
        collect_edges(child, Some(&self_gid), enclosing, gid, head_index, edges);
    }
}

fn edge(from: &str, to: &str, edge_type: &str) -> GraphEdge {
    GraphEdge {
        id: Uuid::new_v4().to_string(),
        from_node: from.to_string(),
        to_node: to.to_string(),
        edge_type: edge_type.to_string(),
        weight: 1.0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::repositories::graph;
    use crate::services::constitution_ir;
    use crate::services::doc_profiles::constitution::CONSTITUTION;
    use crate::services::structure_parser::{parse_tree, persist_tree};
    use crate::db::Db;
    use std::path::PathBuf;

    fn ir_path() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../tools/constitution_import/constitute_IR/constitution_ir.json")
    }

    async fn pool_with_source() -> Db {
        let pool = crate::db::test_pool().await;
        sqlx::query(
            "INSERT INTO sources(id,file_path,file_name,format,file_size_bytes,file_hash,last_modified,created_at) \
             VALUES('c1','/c','constitution','pdf',1,'h',0,0)",
        )
        .execute(&pool)
        .await
        .unwrap();
        pool
    }

    #[test]
    fn provision_refs_extracts_distinct_keys_across_kinds() {
        // Article vocabulary (Constitution) — unchanged behaviour, now keyed.
        let refs = provision_refs(
            "Subject to Article 199 and Articles 184, notwithstanding Article 199.",
        );
        assert_eq!(refs, vec!["article199".to_string(), "article184".to_string()]);
        // a bare number without a kind lead-in is not a reference
        assert!(provision_refs("19A protections apply").is_empty());
        // Rules/Acts vocabulary now produces references too.
        let refs = provision_refs("as provided in Rule 16 and section 9, see also rule 16.");
        assert_eq!(refs, vec!["rule16".to_string(), "section9".to_string()]);
    }

    #[test]
    fn provision_key_and_citation_key_normalise_consistently() {
        // reference spellings and stored citations collapse to the same key
        assert_eq!(provision_key("Rules", "3"), "rule3");
        assert_eq!(provision_key("article", "19a"), "article19A");
        assert_eq!(citation_key("Rule 5(6)").as_deref(), Some("rule5"));
        assert_eq!(citation_key("Section 142").as_deref(), Some("section142"));
        assert_eq!(citation_key("Schedule-II"), None); // roman: no arabic digit → skipped in v1
        assert_eq!(citation_key("Article 19A").as_deref(), Some("article19A"));
        // a bare sub-clause citation ("(2)") has no kind → no key
        assert_eq!(citation_key("(2)"), None);
    }

    #[tokio::test]
    async fn build_graph_covers_edge_types_with_no_dangling_endpoints() {
        let ir = constitution_ir::load(&ir_path()).unwrap();
        let tree = parse_tree(&ir, &CONSTITUTION);
        let pool = pool_with_source().await;
        let chunks = persist_tree(&pool, "c1", &tree).await.unwrap();

        let (nodes, edges) = build_graph(&tree, &chunks);
        assert_eq!(nodes.len(), chunks.len(), "one graph node per chunk");
        assert!(
            nodes.iter().all(|n| !n.chunk_id.is_empty()),
            "every node's chunk_id FK resolves"
        );

        graph::insert_nodes(&pool, &nodes).await.unwrap();
        graph::insert_edges(&pool, &edges).await.unwrap();

        // `contains` spans every node except the document root.
        let contains = edges.iter().filter(|e| e.edge_type == "contains").count();
        assert_eq!(contains, nodes.len() - 1);
        assert!(edges.iter().any(|e| e.edge_type == "cross_reference"));
        assert!(edges.iter().any(|e| e.edge_type == "non_obstante"));

        // A3: no dangling endpoints — every edge endpoint is a real node.
        let dangling: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM graph_edges e \
             WHERE NOT EXISTS (SELECT 1 FROM graph_nodes n WHERE n.id = e.from_node) \
                OR NOT EXISTS (SELECT 1 FROM graph_nodes n WHERE n.id = e.to_node)",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(dangling, 0);
    }

    fn rule_node(num: &str, text: &str) -> DocNode {
        DocNode {
            level: NodeLevel::Article,
            label: num.to_string(),
            title: None,
            text: text.to_string(),
            citation: format!("Rule {num}"),
            unit_path: format!("Rule-{num}"),
            attached: Vec::new(),
            footnote_refs: Vec::new(),
            children: Vec::new(),
        }
    }

    #[test]
    fn build_graph_links_rule_to_rule_cross_reference() {
        // A Rules document (non-Constitution vocabulary) that would have produced
        // ZERO overlay edges before the 2026-07-12 generalisation.
        let tree = DocNode {
            level: NodeLevel::Document,
            label: String::new(),
            title: Some("Test Rules".into()),
            text: String::new(),
            citation: "Test Rules".into(),
            unit_path: String::new(),
            attached: Vec::new(),
            footnote_refs: Vec::new(),
            children: vec![
                rule_node("4", "The council constituted under Rule 12 shall inquire."),
                rule_node("12", "There shall be an anti-corruption council."),
                // self-reference must NOT create an edge
                rule_node("7", "Nothing in Rule 7 limits this."),
            ],
        };
        // Edge resolution runs off citations (gid + head_index), not chunk_id,
        // so an empty persisted set is sufficient to exercise the overlay.
        let (_nodes, edges) = build_graph(&tree, &[]);
        let xrefs: Vec<_> = edges
            .iter()
            .filter(|e| e.edge_type == "cross_reference")
            .collect();
        assert_eq!(xrefs.len(), 1, "exactly one Rule 4 → Rule 12 edge: {edges:?}");
        // no self-reference edge from Rule 7 → Rule 7
        assert!(
            !edges.iter().any(|e| e.from_node == e.to_node),
            "no self-loop edges"
        );
    }

    #[tokio::test]
    async fn expand_is_bounded_and_follows_contains() {
        let ir = constitution_ir::load(&ir_path()).unwrap();
        let tree = parse_tree(&ir, &CONSTITUTION);
        let pool = pool_with_source().await;
        let chunks = persist_tree(&pool, "c1", &tree).await.unwrap();
        let (nodes, edges) = build_graph(&tree, &chunks);
        graph::insert_nodes(&pool, &nodes).await.unwrap();
        graph::insert_edges(&pool, &edges).await.unwrap();

        let root = nodes.iter().find(|n| n.node_type == "document").unwrap();

        // depth 0 returns just the seed.
        let d0 = graph::expand(&pool, &[root.id.clone()], &["contains"], 0)
            .await
            .unwrap();
        assert_eq!(d0.len(), 1);

        // depth 1 over `contains` reaches the document root + its
        // top-level children: 12 Parts + the Preamble node (added by the
        // 2026-05-23 Preamble import-gap fix; sibling of Parts) = 14 total.
        let d1 = graph::expand(&pool, &[root.id.clone()], &["contains"], 1)
            .await
            .unwrap();
        assert_eq!(d1.len(), 14);
    }
}
