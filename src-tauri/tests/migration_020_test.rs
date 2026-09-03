//! Legal-IR schema guard (originally SQLite migration 020, Feature 007
//! Slice 0; now carried by the PG baseline — Feature 011).
//!
//! Asserts:
//!   1. `sources.index_tier` exists, is TEXT NOT NULL, defaults to 'fast'.
//!   2. `graph_nodes` carries the four legal columns (instrument_id,
//!      legal_path, in_force, source_page).
//!   3. The four legal tables exist: amendment_events, legal_xrefs,
//!      procedures, procedure_steps.
//!   4. CHECK constraints reject bogus enum values.

#[tokio::test]
async fn baseline_carries_legal_ir_schema() {
    let pool = echo_ai_lib::db::testing::test_pool().await;

    // 1a. sources.index_tier exists with correct shape.
    let row: Option<(String, String, Option<String>)> = sqlx::query_as(
        "SELECT data_type, is_nullable, column_default \
         FROM information_schema.columns \
         WHERE table_name = 'sources' AND column_name = 'index_tier'",
    )
    .fetch_optional(&pool)
    .await
    .unwrap();
    let (ty, nullable, default) = row.expect("sources.index_tier column should exist");
    assert_eq!(ty, "text");
    assert_eq!(nullable, "NO", "index_tier should be NOT NULL");
    assert!(
        default.as_deref().unwrap_or("").contains("'fast'"),
        "default should be 'fast', got {default:?}"
    );

    // 1b. CHECK constraint rejects bogus tier values.
    let bad_tier = sqlx::query(
        "INSERT INTO sources \
         (id, file_path, file_name, format, file_size_bytes, file_hash, last_modified, created_at, index_tier) \
         VALUES ('s-bad', '/tmp/x.pdf', 'x.pdf', 'pdf', 0, 'deadbeef', 0, 0, 'bogus')",
    )
    .execute(&pool)
    .await;
    assert!(
        bad_tier.is_err(),
        "CHECK constraint should reject bogus index_tier"
    );

    // 2. graph_nodes carries the legal columns.
    let legal_cols: Vec<(String,)> = sqlx::query_as(
        "SELECT column_name FROM information_schema.columns \
         WHERE table_name = 'graph_nodes' \
           AND column_name IN ('instrument_id','legal_path','in_force','source_page') \
         ORDER BY column_name",
    )
    .fetch_all(&pool)
    .await
    .unwrap();
    let names: Vec<String> = legal_cols.into_iter().map(|(n,)| n).collect();
    assert_eq!(
        names,
        vec![
            "in_force".to_string(),
            "instrument_id".to_string(),
            "legal_path".to_string(),
            "source_page".to_string(),
        ],
        "all four legal columns must be present on graph_nodes"
    );

    // 3. Each legal table exists.
    for table in [
        "amendment_events",
        "legal_xrefs",
        "procedures",
        "procedure_steps",
    ] {
        let row: Option<(String,)> = sqlx::query_as(
            "SELECT table_name FROM information_schema.tables \
             WHERE table_schema = 'public' AND table_name = $1",
        )
        .bind(table)
        .fetch_optional(&pool)
        .await
        .unwrap();
        assert!(row.is_some(), "table {table} missing from baseline");
    }

    // 4a. amendment_events.kind CHECK rejects bogus kind.
    let bad_amendment = sqlx::query(
        "INSERT INTO amendment_events (node_id, kind, by_instrument) \
         VALUES ('n1', 'bogus', 'Act-1-of-2020')",
    )
    .execute(&pool)
    .await;
    assert!(
        bad_amendment.is_err(),
        "CHECK constraint should reject bogus amendment kind"
    );

    // 4b. legal_xrefs.edge_kind CHECK rejects bogus edge.
    let bad_edge = sqlx::query(
        "INSERT INTO legal_xrefs (src_node_id, dst_node_id, edge_kind) \
         VALUES ('n1', 'n2', 'bogus')",
    )
    .execute(&pool)
    .await;
    assert!(
        bad_edge.is_err(),
        "CHECK constraint should reject bogus edge_kind"
    );
}
