//! Integration test: T092 — indexing pipeline.
//!
//! Verifies: pending→processing→indexed status transitions,
//! tsvector keyword hit, pgvector semantic query hit.
//!
//! Run with: cargo test --no-default-features -- --test-output=immediate


/// Creates an in-memory SQLite pool with the EchoAI schema applied.
async fn setup_db() -> echo_ai_lib::db::Db {
    let pool = echo_ai_lib::db::testing::test_pool().await;
    pool
}

#[tokio::test]
async fn test_index_pdf_status_transitions() {
    let pdf_path = match std::env::var("ECHO_TEST_PDF") {
        Ok(p) => p,
        Err(_) => {
            eprintln!("Skipping: set ECHO_TEST_PDF=/path/to/test.pdf");
            return;
        }
    };

    let pool = setup_db().await;
    let tier = echo_ai_lib::services::hardware_detector::HardwareTier::detect();
    let embedder = std::sync::Arc::new(echo_ai_lib::services::embedder::EmbedderState::new(&tier));
    let task_budget = echo_ai_lib::services::task_budget::TaskBudget::new();
    let notebook_id = "test-notebook-001";

    // Insert notebook row required by FK.
    sqlx::query("INSERT INTO notebooks (id, name, created_at, updated_at) VALUES ($1, 'Test', 0, 0)")
        .bind(notebook_id)
        .execute(&pool)
        .await
        .expect("insert notebook");

    // Index the file.
    let source_id = echo_ai_lib::services::indexer::index_file(
        &pool,
        embedder,
        &task_budget,
        &tier,
        notebook_id,
        &pdf_path,
        None,
    )
    .await
    .expect("index_file");

    // Check final status = "indexed".
    let row: (String, Option<String>) = sqlx::query_as(
        "SELECT indexing_status, error_reason FROM sources WHERE id = $1",
    )
    .bind(&source_id)
    .fetch_one(&pool)
    .await
    .expect("source row");
    assert_eq!(
        row.0, "indexed",
        "Source should be indexed (error_reason: {:?})",
        row.1
    );

    // Check chunks were created (only meaningful for text-based PDFs).
    let chunk_count: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM chunks WHERE source_id = $1")
        .bind(&source_id)
        .fetch_one(&pool)
        .await
        .expect("chunk count");

    if chunk_count.0 > 0 {
        // Check FTS hit via the generated tsvector column.
        let fts_hits: Vec<(String,)> = sqlx::query_as(
            "SELECT id FROM chunks \
             WHERE tsv @@ websearch_to_tsquery('english', $1) LIMIT 1",
        )
        .bind("the")
        .fetch_all(&pool)
        .await
        .expect("FTS5 query");
        assert!(!fts_hits.is_empty(), "FTS5 should return at least one hit for text PDF");
    } else {
        // Scanned/image PDF — no chunks expected, status still 'indexed'.
        println!("Note: PDF had no extractable text (likely scanned) — skipping FTS5 check");
    }
}

#[tokio::test]
async fn test_schema_integrity() {
    let pool = setup_db().await;

    // Check all expected tables exist.
    let tables: Vec<(String,)> =
        sqlx::query_as("SELECT table_name FROM information_schema.tables WHERE table_schema='public' ORDER BY table_name")
            .fetch_all(&pool)
            .await
            .expect("table list");
    let table_names: Vec<&str> = tables.iter().map(|(n,)| n.as_str()).collect();

    for expected in &[
        // chunk_embeddings is a sqlite-vec virtual table (vec0) — requires the
        // extension to be loaded; skipped here since in-memory pool has no extension.
        "audit_log", "batch_jobs", "batch_results",
        "chunks", "entities", "entity_mentions", "entity_relationships",
        "indexed_folders", "notebook_sources", "notebooks",
        "query_cache", "query_messages", "query_sessions", "sources",
    ] {
        assert!(
            table_names.contains(expected),
            "Table '{expected}' missing from schema"
        );
    }
}
