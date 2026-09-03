//! Migration 023 schema guard — Feature 027 "My Projects".
//!
//! Asserts:
//!   1. The four tables exist: project, project_source, project_chat, chat_share.
//!   2. All five indexes exist, including the PARTIAL unique on live shares.
//!   3. `project.copied_from` has NO foreign key — an FK with cascade would
//!      destroy a recipient's project when the sharer deleted theirs (FR-024).
//!   4. `project_source` has NO `owner_id` column — its absence is the design
//!      (ADR-0028), not an oversight, and a well-meaning future migration
//!      adding one would break copy-on-share.
//!   5. The availability gate seeds to OFF.
//!   6. Cascades behave: deleting a project removes its memberships and chat
//!      mappings, and does NOT remove the source rows.

#[tokio::test]
async fn migration_023_creates_project_schema() {
    let pool = echo_ai_lib::db::testing::test_pool().await;

    // 1. Tables.
    for table in ["project", "project_source", "project_chat", "chat_share"] {
        let exists: bool = sqlx::query_scalar(
            "SELECT EXISTS (SELECT 1 FROM information_schema.tables \
             WHERE table_schema = 'public' AND table_name = $1)",
        )
        .bind(table)
        .fetch_one(&pool)
        .await
        .unwrap();
        assert!(exists, "migration 023 should create table `{table}`");
    }

    // 2. Indexes.
    for index in [
        "idx_project_owner",
        "idx_project_source_project",
        "idx_project_source_source",
        "idx_project_chat_project",
        "idx_chat_share_live",
    ] {
        let exists: bool = sqlx::query_scalar(
            "SELECT EXISTS (SELECT 1 FROM pg_indexes \
             WHERE schemaname = 'public' AND indexname = $1)",
        )
        .bind(index)
        .fetch_one(&pool)
        .await
        .unwrap();
        assert!(exists, "migration 023 should create index `{index}`");
    }

    // 2b. The live-share index must be PARTIAL, or revoking and re-sharing a
    // chat would collide with the revoked row instead of minting a new link.
    let predicate: Option<String> = sqlx::query_scalar(
        "SELECT indexdef FROM pg_indexes WHERE indexname = 'idx_chat_share_live'",
    )
    .fetch_optional(&pool)
    .await
    .unwrap();
    let def = predicate.expect("idx_chat_share_live should exist");
    assert!(
        def.contains("WHERE") && def.contains("revoked_at IS NULL"),
        "idx_chat_share_live must be partial on revoked_at IS NULL, got: {def}"
    );

    // 3. `copied_from` must carry NO foreign key.
    let fk_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM information_schema.table_constraints tc \
         JOIN information_schema.key_column_usage kcu \
           ON tc.constraint_name = kcu.constraint_name \
         WHERE tc.table_name = 'project' AND tc.constraint_type = 'FOREIGN KEY' \
           AND kcu.column_name = 'copied_from'",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(
        fk_count, 0,
        "project.copied_from must NOT be a foreign key — a cascade from it would \
         destroy a recipient's project when the sharer deleted theirs (FR-024)"
    );

    // 4. `project_source` must have NO owner_id (ADR-0028).
    let has_owner: bool = sqlx::query_scalar(
        "SELECT EXISTS (SELECT 1 FROM information_schema.columns \
         WHERE table_name = 'project_source' AND column_name = 'owner_id')",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert!(
        !has_owner,
        "project_source must NOT have owner_id: the only owner it could record is the \
         SOURCE's, which for a copied project is a different user than the caller, so any \
         predicate on it would break copy-on-share (ADR-0028)"
    );

    // 5. The gate seeds OFF.
    let gate: Option<String> =
        sqlx::query_scalar("SELECT value FROM db_config WHERE key = 'my_projects_enabled'")
            .fetch_optional(&pool)
            .await
            .unwrap();
    assert_eq!(
        gate.as_deref(),
        Some("0"),
        "applying migration 023 must expose nothing until an admin enables it"
    );
}

#[tokio::test]
async fn deleting_a_project_cascades_membership_but_not_content() {
    let pool = echo_ai_lib::db::testing::test_pool().await;

    sqlx::query(
        "INSERT INTO sources(id,file_path,file_name,format,file_size_bytes,file_hash,\
         last_modified,created_at,indexing_status) \
         VALUES('s_keep','/s_keep','s_keep.pdf','pdf',10,'h',0,0,'indexed')",
    )
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO project(id,owner_id,name,created_at,updated_at) \
         VALUES('p_del','userA','p_del',0,0)",
    )
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query("INSERT INTO project_source(project_id,source_id,added_at) \
                 VALUES('p_del','s_keep',0)")
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query(
        "INSERT INTO project_chat(conversation_id,project_id,owner_id,created_at) \
         VALUES('c_del','p_del','userA',0)",
    )
    .execute(&pool)
    .await
    .unwrap();

    sqlx::query("DELETE FROM project WHERE id = 'p_del'")
        .execute(&pool)
        .await
        .unwrap();

    let memberships: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM project_source WHERE project_id = 'p_del'")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(memberships, 0, "membership should cascade with the project");

    let chats: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM project_chat WHERE project_id = 'p_del'")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(chats, 0, "chat mappings should cascade with the project (FR-036)");

    // The document itself must survive — deleting a project is not deleting
    // content, and another project (possibly another user's copy) may hold it.
    let source_alive: bool =
        sqlx::query_scalar("SELECT EXISTS (SELECT 1 FROM sources WHERE id = 's_keep')")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert!(source_alive, "deleting a project must not delete document content");
}
