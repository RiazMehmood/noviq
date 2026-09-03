//! End-to-end acceptance: seed a synthetic 3-level chunk hierarchy into the
//! test PG database, then assert that a tsvector keyword search for a precise
//! clause returns the leaf chunk (not the section or document).

use echo_ai_lib::db::repositories::chunks::{bulk_insert, NewChunk};

#[tokio::test]
async fn precise_clause_query_returns_matching_leaf() {
    let pool = echo_ai_lib::db::testing::test_pool().await;

    sqlx::query(
        "INSERT INTO sources(id,file_path,file_name,format,file_size_bytes,file_hash,last_modified,created_at) \
         VALUES('s1','/x','sindh-act.pdf','pdf',1,'h',0,0)",
    )
    .execute(&pool)
    .await
    .unwrap();

    // Build the document → section → leaf hierarchy by hand so the test never
    // pulls in the heading detector / LLM passes.
    let doc = NewChunk {
        source_id: "s1".into(),
        chunk_index: 0,
        content: "Sindh Civil Servants Act 1973".into(),
        char_start: 0,
        char_end: 30,
        page_number: None,
        token_count: 5,
        parent_chunk_id: None,
        unit_type: "document".into(),
        unit_label: Some("Sindh Civil Servants Act 1973".into()),
        unit_path: Some("".into()),
        domain_metadata: None,
    };
    let inserted = bulk_insert(&pool, vec![doc]).await.unwrap();
    let doc_id = inserted[0].id.clone();

    let section = NewChunk {
        source_id: "s1".into(),
        chunk_index: 1,
        content: "Section 8 — Recruitment. Eligibility for various BPS positions is described herein."
            .into(),
        char_start: 0,
        char_end: 100,
        page_number: None,
        token_count: 20,
        parent_chunk_id: Some(doc_id.clone()),
        unit_type: "section".into(),
        unit_label: Some("Section 8".into()),
        unit_path: Some("Section 8".into()),
        domain_metadata: None,
    };
    let s = bulk_insert(&pool, vec![section]).await.unwrap();
    let sec_id = s[0].id.clone();

    let leaf = NewChunk {
        source_id: "s1".into(),
        chunk_index: 2,
        content: "For B-18, the minimum length of service shall be 5 years in B-17.".into(),
        char_start: 0,
        char_end: 80,
        page_number: Some(1),
        token_count: 18,
        parent_chunk_id: Some(sec_id),
        unit_type: "leaf".into(),
        unit_label: None,
        unit_path: Some("Section 8".into()),
        domain_metadata: None,
    };
    bulk_insert(&pool, vec![leaf]).await.unwrap();

    // FTS5 sanity — searching for "minimum length of service" must return the leaf row.
    let row: (String,) = sqlx::query_as(
        "SELECT id FROM chunks \
         WHERE tsv @@ websearch_to_tsquery('english', 'minimum length of service') LIMIT 1",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    let found: (String,) = sqlx::query_as("SELECT unit_type FROM chunks WHERE id = $1")
        .bind(&row.0)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(found.0, "leaf");
}
