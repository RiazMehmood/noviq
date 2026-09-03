//! S7 / T029 — Constitution v1 acceptance harness.
//!
//! Encodes the A1–A4 gate from `specs/003-constitution-hierarchical-graph/spec.md`
//! as deterministic Rust integration tests. Equivalent to the bash
//! `~/Desktop/db_inspection_constitute_v1/07-10*.sh` probes, but runs in
//! CI without needing the production DB or sqlite3 CLI with fts5.
//!
//! - A1 Structural — Part/Chapter/Article/Clause/SubClause counts in the
//!   right bands, zero orphans, no single parent holding >90 % of the
//!   document's leaves.
//! - A2 Citation accuracy — every article/clause/sub-clause row's
//!   `unit_label`, `unit_path`, parent-chain article, content lead-in and
//!   `domain_metadata.citation` agree.
//! - A3 Graph integrity — `graph_nodes` 1-1 with `chunks`, no dangling
//!   edge endpoints, `contains` edge count = `graph_nodes - 1`, both
//!   `cross_reference` and `non_obstante` edges present.
//! - A4 Retrieval (citation fast-path subset) — F1 / F2 / F4 sample
//!   queries each resolve via `citation_fast_path` to a chunk whose
//!   `unit_label` matches the expected article. F3 / F5 are full-pipeline
//!   queries that depend on hybrid retrieval + LLM and are tracked as
//!   separate concerns per spec.
//!
//! Run with: `cargo test --test constitution_v1_acceptance`.

use echo_ai_lib::services::constitution_ir;
use echo_ai_lib::services::doc_profiles::constitution::CONSTITUTION;
use echo_ai_lib::services::graph_builder::build_graph;
use echo_ai_lib::services::query_router::citation_fast_path;
use echo_ai_lib::services::structure_parser::{parse_tree, persist_tree};
use echo_ai_lib::db::repositories::graph as graph_repo;
use std::collections::HashMap;
use std::path::PathBuf;

/// The S1 importer's committed output, relative to the `src-tauri` crate.
fn ir_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../tools/constitution_import/constitute_IR/constitution_ir.json")
}

/// Spin up an in-memory pool, apply migrations, insert one test source,
/// then load the IR + persist the tree + build the graph. Returns the
/// populated pool ready for assertions.
async fn build_fixture() -> echo_ai_lib::db::Db {
    let pool = echo_ai_lib::db::testing::test_pool().await;
    sqlx::query(
        "INSERT INTO sources(id,file_path,file_name,format,file_size_bytes,file_hash,last_modified,created_at) \
         VALUES('c1','/c','constitution_ir.json','code',1,'h',0,0)",
    )
    .execute(&pool)
    .await
    .expect("seed source");

    let ir = constitution_ir::load(&ir_path()).expect("load IR");
    let tree = parse_tree(&ir, &CONSTITUTION);
    let chunks = persist_tree(&pool, "c1", &tree).await.expect("persist tree");
    let (nodes, edges) = build_graph(&tree, &chunks);
    graph_repo::insert_nodes(&pool, &nodes).await.expect("insert graph nodes");
    graph_repo::insert_edges(&pool, &edges).await.expect("insert graph edges");

    pool
}

// ─── A1 — Structural ─────────────────────────────────────────────────────────

#[tokio::test]
async fn a1_structural_counts_and_linkage() {
    let pool = build_fixture().await;

    // Per-unit counts. Bands come from spec.md ("~12 Parts, ~30 Chapters,
    // ~290 Articles, ~1,500 Clauses, ~3–5k sub-clause leaves"). Idea 1
    // and the Preamble import-gap fix bumped Article/Clause counts —
    // assert against actual post-fix targets, not the spec's ballpark.
    let counts: Vec<(String, i64)> = sqlx::query_as(
        "SELECT unit_type, COUNT(*) FROM chunks GROUP BY unit_type ORDER BY unit_type",
    )
    .fetch_all(&pool)
    .await
    .unwrap();
    let m: HashMap<String, i64> = counts.into_iter().collect();
    assert_eq!(m.get("document").copied(), Some(1), "one document root");
    assert_eq!(m.get("part").copied(), Some(12), "12 Parts");
    assert!(
        (28..=32).contains(&m.get("chapter").copied().unwrap_or(0)),
        "~30 Chapters, got {:?}",
        m.get("chapter")
    );
    assert_eq!(
        m.get("article").copied(),
        Some(361),
        "360 Articles (312 + 48 from the F007 IR data patch f53b4e6a — \
         missing-article + chapter-index injection) + 1 Preamble parent"
    );
    assert_eq!(
        m.get("clause").copied(),
        Some(1024),
        "927 (787 IR + 19 Preamble splits + 121 article-chapeau splits per db8688f4) \
         + 97 from the F007 IR data patch f53b4e6a (clauses of the 48 injected articles)"
    );
    assert!(
        m.get("subclause").copied().unwrap_or(0) >= 300,
        "≥300 SubClauses, got {:?}",
        m.get("subclause")
    );

    // Exactly one root with NULL parent (the document).
    let roots: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM chunks WHERE parent_chunk_id IS NULL")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(roots, 1, "exactly one document root");

    // Zero orphans — every parent_chunk_id resolves.
    let orphans: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM chunks c WHERE c.parent_chunk_id IS NOT NULL \
         AND NOT EXISTS (SELECT 1 FROM chunks p WHERE p.id = c.parent_chunk_id)",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(orphans, 0, "no orphan parent links");

    // Max-children ratio guard (spec A1: "no parent holding >90 % of
    // leaves"). Compute, for each non-leaf parent, the number of direct
    // children; assert no single parent owns >90 % of all `clause` +
    // `subclause` leaves (the actual leaves of the legal hierarchy).
    let leaves: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM chunks WHERE unit_type IN ('clause','subclause')",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    let max_children: i64 = sqlx::query_scalar(
        "SELECT MAX(cnt) FROM (SELECT COUNT(*) AS cnt FROM chunks \
         WHERE parent_chunk_id IS NOT NULL GROUP BY parent_chunk_id)",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    let ratio = (max_children as f64) / (leaves.max(1) as f64);
    assert!(
        ratio < 0.90,
        "max-children ratio {ratio:.2} should be < 0.90 (max children {max_children}, leaves {leaves})"
    );
}

// ─── A2 — Citation accuracy ──────────────────────────────────────────────────

#[tokio::test]
async fn a2_citation_accuracy_every_clause_aligned_with_article() {
    let pool = build_fixture().await;

    // Load every article/clause/subclause row + parent chain.
    #[derive(Debug, Clone)]
    struct Row {
        id: String,
        ut: String,
        label: Option<String>,
        path: Option<String>,
        parent: Option<String>,
        content: String,
        meta: Option<String>,
    }
    let rows: Vec<(String, String, Option<String>, Option<String>, Option<String>, String, Option<String>)> =
        sqlx::query_as(
            "SELECT id, unit_type, unit_label, unit_path, parent_chunk_id, content, domain_metadata \
             FROM chunks",
        )
        .fetch_all(&pool)
        .await
        .unwrap();
    let map: HashMap<String, Row> = rows
        .into_iter()
        .map(|(id, ut, label, path, parent, content, meta)| {
            (id.clone(), Row { id, ut, label, path, parent, content, meta })
        })
        .collect();

    let label_re = regex_lite::Regex::new(r"^Article\s+(\d+[A-Z]*)").unwrap();
    let mut checked = 0usize;
    let mut failed: Vec<String> = Vec::new();

    for row in map.values() {
        if !matches!(row.ut.as_str(), "article" | "clause" | "subclause") {
            continue;
        }
        // Preamble parent + paragraph children carry unit_label='Preamble',
        // not 'Article N…'. Exempt them from the article-label check; they
        // have their own structural invariants (covered in A1 + structure
        // parser tests).
        if row.label.as_deref() == Some("Preamble") {
            continue;
        }
        // Synthetic chapter-index chunks (F007 data patch f53b4e6a) carry
        // labels like "Article IDX-PI" — navigation aids, not citation
        // units. Exempt them like the Preamble.
        if row.label.as_deref().is_some_and(|l| l.starts_with("Article IDX-")) {
            continue;
        }
        checked += 1;
        let label = row.label.clone().unwrap_or_default();
        let m = match label_re.captures(&label) {
            Some(c) => c,
            None => {
                failed.push(format!("{} unit_label {label:?} is not 'Article N…'", row.id));
                continue;
            }
        };
        let num = m.get(1).unwrap().as_str();
        let path = row.path.clone().unwrap_or_default();
        if !path.contains(&format!("/Art-{num}/")) && !path.ends_with(&format!("/Art-{num}")) {
            failed.push(format!("{}: unit_path {path:?} missing /Art-{num}", row.id));
        }
        // domain_metadata.citation must match unit_label exactly.
        if let Some(dm) = &row.meta {
            let needle = format!(r#""citation":"{label}""#);
            if !dm.contains(&needle) {
                failed.push(format!("{}: domain_metadata missing {needle} (was {dm})", row.id));
            }
        } else {
            failed.push(format!("{}: domain_metadata missing", row.id));
        }
        // Content lead-in: a leaf carries its Article heading at the
        // start (T022 — multi-level embedding context propagation).
        if row.ut != "article" && !row.content.starts_with(&format!("Article {num}")) {
            failed.push(format!(
                "{}: content does not start with 'Article {num}' (was {:?}…)",
                row.id,
                &row.content.chars().take(40).collect::<String>()
            ));
        }
        // Parent-chain article: walking parents must reach an article
        // whose `unit_label` agrees.
        let mut cursor = row.parent.clone();
        let mut hops = 0;
        loop {
            let Some(pid) = cursor else { break };
            let Some(parent) = map.get(&pid) else { break };
            if parent.ut == "article" {
                if parent.label.as_deref().unwrap_or("") != label.split('(').next().unwrap_or("") {
                    // Sub-clause label e.g. "Article 8(3)(b)" — parent article should be "Article 8".
                    let bare = label.split('(').next().unwrap_or("");
                    if parent.label.as_deref() != Some(bare) {
                        failed.push(format!(
                            "{}: parent article {:?} disagrees with leaf {label:?}",
                            row.id, parent.label
                        ));
                    }
                }
                break;
            }
            cursor = parent.parent.clone();
            hops += 1;
            if hops > 6 {
                failed.push(format!("{}: parent chain >6 hops", row.id));
                break;
            }
        }
    }

    assert!(
        failed.is_empty(),
        "A2 citation accuracy: {} of {} checked rows failed:\n  {}",
        failed.len(),
        checked,
        failed.iter().take(20).cloned().collect::<Vec<_>>().join("\n  "),
    );
}

// ─── A3 — Graph integrity ────────────────────────────────────────────────────

#[tokio::test]
async fn a3_graph_integrity_nodes_edges_endpoints() {
    let pool = build_fixture().await;

    let gn: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM graph_nodes")
        .fetch_one(&pool)
        .await
        .unwrap();
    let ge: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM graph_edges")
        .fetch_one(&pool)
        .await
        .unwrap();
    let chunks: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM chunks")
        .fetch_one(&pool)
        .await
        .unwrap();

    assert_eq!(gn, chunks, "graph_nodes count must equal chunks count");

    // Every graph_node points at a real chunk.
    let missing: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM graph_nodes gn WHERE NOT EXISTS \
         (SELECT 1 FROM chunks c WHERE c.id = gn.chunk_id)",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(missing, 0, "every graph_node must reference a real chunk");

    // No dangling edge endpoints.
    let dangling: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM graph_edges e \
         WHERE NOT EXISTS (SELECT 1 FROM graph_nodes n WHERE n.id = e.from_node) \
            OR NOT EXISTS (SELECT 1 FROM graph_nodes n WHERE n.id = e.to_node)",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(dangling, 0, "no edge endpoints may dangle");

    // `contains` is the tree skeleton: one edge per non-root node.
    let contains: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM graph_edges WHERE edge_type = 'contains'")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(
        contains,
        gn - 1,
        "contains edges = graph_nodes − 1 (tree skeleton)"
    );

    // The legal overlay: both kinds must be present.
    let cross: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM graph_edges WHERE edge_type = 'cross_reference'")
            .fetch_one(&pool)
            .await
            .unwrap();
    let non_obstante: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM graph_edges WHERE edge_type = 'non_obstante'",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert!(cross > 50, "expected many cross_reference edges, got {cross}");
    assert!(non_obstante > 0, "at least one non_obstante edge expected");

    // Sanity: total edges = contains + cross_reference + non_obstante.
    assert_eq!(
        ge,
        contains + cross + non_obstante,
        "graph_edges total {ge} = contains {contains} + cross_ref {cross} + non_obstante {non_obstante}"
    );
}

// ─── A4 — Retrieval (citation fast-path subset) ──────────────────────────────

/// Fact 1 / Fact 2 / Fact 4 from the F-set are citation-eligible: they
/// name a specific Article and the fast-path must return the cited chunk
/// (plus its sub-tree). Fact 3 ("high treason") and Fact 5 ("Founder of
/// Pakistan") need full hybrid retrieval — tracked separately per spec
/// ("F3/F5 should improve … but may still need the fine-tuned reranker
/// — tracked separately").
#[tokio::test]
async fn a4_citation_fastpath_resolves_f1_f2_f4_seeds() {
    let pool = build_fixture().await;

    let cases: &[(&str, &str)] = &[
        // F1 — Article 10 (Safeguards as to arrest and detention).
        ("Article 10 — Safeguards as to arrest and detention", "Article 10"),
        // F2 — Article 1(2) territory list.
        ("what are the four categories of territory under Article 1(2)?", "Article 1(2)"),
        // F4 — Article 11(3) child-labour minimum age.
        ("what is the minimum age in Article 11(3)?", "Article 11(3)"),
        // Bonus seed — Article 19A (a previously-failed citation).
        ("define Article 19A", "Article 19A"),
    ];

    for (query, expected_label) in cases {
        let ids = citation_fast_path(&pool, query)
            .await
            .unwrap_or_else(|e| panic!("fast-path errored on {query:?}: {e}"))
            .unwrap_or_else(|| panic!("fast-path returned None for {query:?}"));
        assert!(
            !ids.is_empty(),
            "fast-path returned empty for {query:?}"
        );
        // The first id must have the expected unit_label (the cited unit
        // itself is always returned at position 1; descendants follow).
        let head_label: Option<String> =
            sqlx::query_scalar("SELECT unit_label FROM chunks WHERE id = $1")
                .bind(&ids[0])
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(
            head_label.as_deref(),
            Some(*expected_label),
            "fast-path head label mismatch for {query:?}: got {head_label:?}, expected {expected_label}"
        );
    }
}

/// Preamble is the only `unit_label='Preamble'` row matched by name.
/// Idea 1 split it into a parent + paragraph children, so the fast-path
/// must return ≥ 2 chunks (parent + at least one paragraph child) for
/// any query containing the literal token "preamble".
#[tokio::test]
async fn a4_preamble_fastpath_returns_parent_plus_children() {
    let pool = build_fixture().await;
    let ids = citation_fast_path(&pool, "what does the preamble say")
        .await
        .unwrap()
        .expect("Preamble query resolves via fast-path");
    assert!(
        ids.len() >= 2,
        "Preamble fast-path returns parent + paragraph children, got {} chunks",
        ids.len()
    );
}
