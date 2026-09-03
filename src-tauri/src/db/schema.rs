//! Database initialisation (Feature 011 — bundled PostgreSQL + pgvector).
//!
//! Boots the app-managed engine (db/pg/engine.rs), applies the PG-native
//! baseline migrations, detects the embedding dimension, and creates the
//! `chunk_embeddings` vector table + halfvec HNSW index at the configured
//! dim (768/384 — mirrors the old vec0 dynamic-dim pattern).
//!
//! The SQLite wipe-on-mismatch path is gone: a fresh PG cluster starts at the
//! current baseline, and legacy `echo.db` content arrives via the one-time
//! migration tool (US3), never by replaying SQLite migrations here.

use anyhow::{Context, Result};
use std::path::Path;

use super::pg::engine::{self, EngineHandle};
use super::Db;
use crate::services::hardware_detector::HardwareTier;

/// Logical schema version recorded in `db_config` (parity with the SQLite
/// counter; future PG migrations bump it).
pub const TARGET_SCHEMA_VERSION: u32 = 16;

/// Embedding dimension resolved at init (768/384); read by the vector seam to
/// build halfvec-cast expressions that match the HNSW expression index.
static EMBEDDING_DIM: std::sync::OnceLock<u32> = std::sync::OnceLock::new();
pub fn embedding_dim() -> u32 {
    EMBEDDING_DIM.get().copied().unwrap_or(768)
}

/// Boot the bundled engine and return (engine handle, ready pool).
///
/// `pg_root` = `{app_data_dir}/pg`; `pgvector_resources` = vendored extension
/// tree root (platform subdir appended inside engine.rs).
pub async fn init_db(
    pg_root: &Path,
    pgvector_resources: &Path,
    tier: &HardwareTier,
) -> Result<(EngineHandle, Db)> {
    let handle = engine::ensure_running(pg_root, pgvector_resources, tier)
        .await
        .map_err(|e| anyhow::anyhow!("{e}"))?;
    let pool = engine::connect(&handle)
        .await
        .map_err(|e| anyhow::anyhow!("{e}"))?;
    init_schema(&pool, tier).await?;
    Ok((handle, pool))
}

/// Apply baseline migrations + dynamic vector table + startup recovery on an
/// already-connected pool. Shared by production boot and the test helper
/// (FR-015 dev/test parity).
pub async fn init_schema(pool: &Db, tier: &HardwareTier) -> Result<()> {
    sqlx::migrate!("./src/db/pg/migrations")
        .run(pool)
        .await
        .context("pg baseline migration failed")?;

    let dim = resolve_embedding_dim(pool, tier).await?;
    let _ = EMBEDDING_DIM.set(dim);
    ensure_vector_table(pool, dim).await?;
    ensure_summary_vector_table(pool, dim).await?;

    // ── Startup recovery — unstick sources orphaned mid-index ───────────────
    // (constitution.pdf upsert-hang incident, 2026-05-16)
    let recovered = sqlx::query(
        "UPDATE sources SET indexing_status = 'pending' WHERE indexing_status = 'processing'",
    )
    .execute(pool)
    .await
    .map(|r| r.rows_affected())
    .unwrap_or(0);
    if recovered > 0 {
        log::warn!(
            "Startup recovery: reset {recovered} source(s) from 'processing' to 'pending' \
             (previous run did not complete indexing for them)"
        );
    }

    // ── Startup recovery — heal 'failed'/'pending' sources that have chunks ─
    // (constitution_ir.json incident 2026-05-25; ordering-bug recurrence 2026-05-26)
    let healed = sqlx::query(
        "UPDATE sources SET indexing_status = 'indexed', error_reason = NULL, \
                            indexed_at = COALESCE(indexed_at, EXTRACT(EPOCH FROM now())::bigint) \
         WHERE indexing_status IN ('failed', 'pending') \
           AND EXISTS (SELECT 1 FROM chunks WHERE chunks.source_id = sources.id)",
    )
    .execute(pool)
    .await
    .map(|r| r.rows_affected())
    .unwrap_or(0);
    if healed > 0 {
        log::warn!(
            "Startup recovery: marked {healed} source(s) as 'indexed' that had chunks \
             but were stuck on 'failed' (interrupted mid-finalize on a previous run)"
        );
    }

    // ── Startup backfill — display titles for pre-existing sources (2026-07-28)
    // Migration 015 adds the column NULL; new indexes stamp it, this fills the
    // rest once. Idempotent: only NULL rows are touched.
    let rows: Vec<(String, Option<String>)> =
        sqlx::query_as("SELECT id, file_name FROM sources WHERE display_name IS NULL")
            .fetch_all(pool)
            .await
            .unwrap_or_default();
    let mut filled = 0usize;
    for (id, fname) in rows {
        let Some(f) = fname else { continue };
        let d = crate::services::display_title::display_title(&f);
        if d.is_empty() {
            continue;
        }
        if sqlx::query("UPDATE sources SET display_name = $1 WHERE id = $2")
            .bind(&d)
            .bind(&id)
            .execute(pool)
            .await
            .is_ok()
        {
            filled += 1;
        }
    }
    if filled > 0 {
        log::info!("Startup backfill: display_name set for {filled} source(s)");
    }

    log::info!("DB ready — embedding dim: {dim}");
    Ok(())
}

/// Returns the configured embedding dimension, writing it to `db_config` on first run.
/// Once set the dimension is immutable (re-indexing would be required to change it).
async fn resolve_embedding_dim(pool: &Db, tier: &HardwareTier) -> Result<u32> {
    let row: Option<(String,)> =
        sqlx::query_as("SELECT value FROM db_config WHERE key = 'embedding_dim'")
            .fetch_optional(pool)
            .await?;

    if let Some((val,)) = row {
        return val.parse::<u32>().context("invalid embedding_dim in db_config");
    }

    let dim: u32 = if tier.embedding_model == "nomic-embed-text-v1.5" {
        768
    } else {
        384
    };

    sqlx::query("INSERT INTO db_config (key, value) VALUES ('embedding_dim', $1)")
        .bind(dim.to_string())
        .execute(pool)
        .await?;

    Ok(dim)
}

/// Create `chunk_embeddings` (+ halfvec-expression HNSW index, research R3)
/// if it does not already exist. Dim is dynamic, so this lives outside the
/// static baseline — exactly like the old vec0 table.
async fn ensure_vector_table(pool: &Db, dim: u32) -> Result<()> {
    let exists: (i64,) = sqlx::query_as(
        "SELECT COUNT(*) FROM information_schema.tables \
         WHERE table_schema = 'public' AND table_name = 'chunk_embeddings'",
    )
    .fetch_one(pool)
    .await?;

    if exists.0 > 0 {
        return Ok(());
    }

    let ddl = format!(
        "CREATE TABLE chunk_embeddings (\
            chunk_id  TEXT PRIMARY KEY REFERENCES chunks(id) ON DELETE CASCADE, \
            embedding vector({dim}) NOT NULL\
        )"
    );
    sqlx::query(&ddl)
        .execute(pool)
        .await
        .context("failed to create chunk_embeddings table")?;

    let idx = format!(
        "CREATE INDEX chunk_embeddings_hnsw ON chunk_embeddings \
         USING hnsw ((embedding::halfvec({dim})) halfvec_cosine_ops) \
         WITH (m = 16, ef_construction = 64)"
    );
    sqlx::query(&idx)
        .execute(pool)
        .await
        .context("failed to create chunk_embeddings HNSW index")?;

    log::info!("Created chunk_embeddings vector table with dim={dim} + halfvec HNSW index");
    Ok(())
}

/// Track 2 (ADR-0019 §2): the recall-index embeddings for `graph_summary`.
/// Mirrors [`ensure_vector_table`] — dim is runtime (db_config), so it lives
/// outside the static baseline. One embedding per summary row; the summary maps
/// back to its ORIGINAL provision chunk (the answer never reads `summary_text`).
async fn ensure_summary_vector_table(pool: &Db, dim: u32) -> Result<()> {
    let exists: (i64,) = sqlx::query_as(
        "SELECT COUNT(*) FROM information_schema.tables \
         WHERE table_schema = 'public' AND table_name = 'summary_embeddings'",
    )
    .fetch_one(pool)
    .await?;

    if exists.0 > 0 {
        return Ok(());
    }

    let ddl = format!(
        "CREATE TABLE summary_embeddings (\
            summary_id TEXT PRIMARY KEY REFERENCES graph_summary(id) ON DELETE CASCADE, \
            embedding  vector({dim}) NOT NULL\
        )"
    );
    sqlx::query(&ddl)
        .execute(pool)
        .await
        .context("failed to create summary_embeddings table")?;

    let idx = format!(
        "CREATE INDEX summary_embeddings_hnsw ON summary_embeddings \
         USING hnsw ((embedding::halfvec({dim})) halfvec_cosine_ops) \
         WITH (m = 16, ef_construction = 64)"
    );
    sqlx::query(&idx)
        .execute(pool)
        .await
        .context("failed to create summary_embeddings HNSW index")?;

    log::info!("Created summary_embeddings vector table with dim={dim} + halfvec HNSW index");
    Ok(())
}
