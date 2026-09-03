//! Targeted repair of oversize chunks — `POST /api/admin/resplit_oversize`.
//!
//! ─── Why this exists rather than a re-index ────────────────────────────────
//! The chunker fix (2026-08-25) corrected how an oversized leaf is SPLIT. It
//! does nothing to rows already written. The obvious remedy is to re-index the
//! affected documents, but that re-parses and re-embeds every chunk in them:
//! 32,775 chunks rewritten to repair 367. Since only the SPLITTER changed — the
//! structure importer is untouched at version 4 — a re-index would re-detect
//! structure exactly as before and reach the same end state. So it does 89x the
//! work for an identical result, and every rewritten row is a chance to disturb
//! something that was already right.
//!
//! This takes the text already stored on the bad chunk, re-splits it with the
//! same function the indexer now uses, and replaces the one row.
//!
//! ⚠️ DRY RUN BY DEFAULT. `dry_run` is true unless explicitly set false, so an
//! accidental call reports and writes nothing.

use axum::extract::{Query, State};
use axum::Json;
use serde::{Deserialize, Serialize};

use super::session::AdminUser;
use super::{ApiError, ApiResult, AppState};
use crate::db::repositories::chunks::{self as chunk_repo, NewChunk};
use crate::services::chunker::{split_oversized_text, LEAF_SOFT_CAP};

#[derive(Deserialize)]
pub struct ResplitParams {
    /// ⚠️ Defaults to TRUE. Only an explicit `dry_run=false` writes.
    pub dry_run: Option<bool>,
    /// Cap the number of chunks handled in one call, so a first real run can be
    /// kept small and inspected before doing the rest.
    pub limit: Option<i64>,
}

#[derive(Serialize)]
pub struct ResplitReport {
    pub dry_run: bool,
    /// Chunks over the cap that this call considered.
    pub considered: usize,
    /// Chunks that would be replaced (dry run) or were QUEUED for replacement.
    /// ⚠️ On a real run the work is detached, so this is not a completion count —
    /// watch the oversize total fall, or the server log's "resplit done" line.
    pub replaced: usize,
    /// Pieces that would be (or were) created in their place.
    pub pieces_created: usize,
    /// Distinct documents touched.
    pub documents: usize,
    /// ⚠️ Skipped because another chunk is a CHILD of them. `parent_chunk_id`
    /// is NO ACTION, so deleting such a row would fail the constraint. Left
    /// alone and reported rather than forced.
    pub skipped_has_children: usize,
    /// Skipped because re-splitting produced no improvement (one piece back).
    pub skipped_no_gain: usize,
    /// Entity relationships anchored on the chunks being removed. These are
    /// `SET NULL` on delete, so they survive but lose their anchor.
    pub relationships_unanchored: i64,
    pub largest_token_count: i64,
    pub errors: Vec<String>,
}

/// POST /api/admin/resplit_oversize?dry_run=false&limit=50
pub async fn resplit_oversize(
    State(st): State<AppState>,
    _admin: AdminUser,
    Query(p): Query<ResplitParams>,
) -> ApiResult<Json<ResplitReport>> {
    let dry_run = p.dry_run.unwrap_or(true);
    let limit = p.limit.unwrap_or(1000).clamp(1, 5000);

    // The offenders, worst first — if a run is capped, it should fix the most
    // damaging chunks rather than an arbitrary slice.
    let rows: Vec<(String, String, i64, i64, Option<i64>, String, Option<String>, String)> =
        sqlx::query_as(
            "SELECT id, source_id, chunk_index, token_count, page_number, unit_type, unit_path, content \
             FROM chunks WHERE token_count > $1 ORDER BY token_count DESC LIMIT $2",
        )
        .bind(LEAF_SOFT_CAP as i64)
        .bind(limit)
        .fetch_all(&st.pool)
        .await
        .map_err(|e| ApiError::internal(e.to_string()))?;

    let mut report = ResplitReport {
        dry_run,
        considered: rows.len(),
        replaced: 0,
        pieces_created: 0,
        documents: 0,
        skipped_has_children: 0,
        skipped_no_gain: 0,
        relationships_unanchored: 0,
        largest_token_count: rows.first().map(|r| r.3).unwrap_or(0),
        errors: Vec::new(),
    };
    let mut docs = std::collections::HashSet::new();
    #[allow(clippy::type_complexity)]
    let mut work: Vec<(String, String, i64, Option<i64>, String, Option<String>, Vec<String>)> =
        Vec::new();

    for (id, source_id, chunk_index, _tokens, page_number, unit_type, unit_path, content) in rows {
        // ⚠️ `chunks.parent_chunk_id` is NO ACTION: deleting a row that another
        // chunk points at would fail the constraint mid-run. Check first and
        // skip — a loud failure halfway through a repair is worse than a
        // reported skip.
        let children: i64 = sqlx::query_scalar("SELECT count(*) FROM chunks WHERE parent_chunk_id = $1")
            .bind(&id)
            .fetch_one(&st.pool)
            .await
            .map_err(|e| ApiError::internal(e.to_string()))?;
        if children > 0 {
            report.skipped_has_children += 1;
            continue;
        }

        let pieces = split_oversized_text(&content);
        if pieces.len() <= 1 {
            // Nothing gained — re-splitting returned the same single blob.
            report.skipped_no_gain += 1;
            continue;
        }

        let rel: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM entity_relationships WHERE source_chunk_id = $1",
        )
        .bind(&id)
        .fetch_one(&st.pool)
        .await
        .unwrap_or(0);
        report.relationships_unanchored += rel;

        docs.insert(source_id.clone());
        report.replaced += 1;
        report.pieces_created += pieces.len();

        if dry_run {
            continue;
        }
        work.push((id, source_id, chunk_index, page_number, unit_type, unit_path, pieces));
    }

    report.documents = docs.len();

    // ⚠️ The real run is DETACHED. An inline handler is cancelled the moment the
    // client disconnects, and this one takes minutes — a closed tab, a
    // navigation, or a tunnel timeout killed it mid-repair, leaving a document
    // holding both its old oversize chunk and unembedded replacements. That is
    // the safe half-state by design, but it should not be reachable by
    // something as ordinary as changing tabs. `query.rs` learned this for chat
    // turns; the same reasoning applies here.
    //
    // Progress is in the server log (`METRIC embed_done_ms`, `upserted N
    // embeddings`) and in the shrinking count of oversize chunks.
    if !dry_run && !work.is_empty() {
        let st2 = st.clone();
        let planned = work.len();
        tokio::spawn(async move {
            let mut ok = 0usize;
            let mut failed = 0usize;
            for (id, source_id, chunk_index, page_number, unit_type, unit_path, pieces) in work {
                match replace_chunk(
                    &st2, &id, &source_id, chunk_index, page_number, &unit_type,
                    unit_path.as_deref(), pieces,
                )
                .await
                {
                    Ok(()) => ok += 1,
                    Err(e) => {
                        failed += 1;
                        log::error!("resplit: chunk {id} failed: {e}");
                    }
                }
            }
            log::info!("resplit done: {planned} planned, {ok} repaired, {failed} failed");
        });
    }

    Ok(Json(report))
}

/// Replace one oversize chunk with its pieces, in order, and embed them.
///
/// Ordering matters: the pieces take the original's position, and everything
/// after it in the same document shifts up. Appending at the end would have been
/// simpler and would have scrambled the document's chunk order — `chunk_index`
/// is the reading order a citation is rendered from.
async fn replace_chunk(
    st: &AppState,
    chunk_id: &str,
    source_id: &str,
    chunk_index: i64,
    page_number: Option<i64>,
    unit_type: &str,
    unit_path: Option<&str>,
    pieces: Vec<String>,
) -> anyhow::Result<()> {
    let extra = (pieces.len() - 1) as i64;

    // Make room: shift later chunks up by the number of NEW rows.
    sqlx::query("UPDATE chunks SET chunk_index = chunk_index + $1 WHERE source_id = $2 AND chunk_index > $3")
        .bind(extra)
        .bind(source_id)
        .bind(chunk_index)
        .execute(&st.pool)
        .await?;

    let rows: Vec<NewChunk> = pieces
        .iter()
        .enumerate()
        .map(|(i, piece)| NewChunk {
            source_id: source_id.to_string(),
            chunk_index: chunk_index + i as i64,
            char_start: 0,
            char_end: piece.chars().count() as i64,
            page_number,
            token_count: piece.split_whitespace().count() as i64,
            parent_chunk_id: None,
            unit_type: unit_type.to_string(),
            unit_label: None,
            unit_path: unit_path.map(|s| s.to_string()),
            domain_metadata: None,
            content: piece.clone(),
        })
        .collect();

    let inserted = chunk_repo::bulk_insert(&st.pool, rows).await?;

    // ⚠️ Embed BEFORE deleting the original. If embedding fails, the document is
    // left with both the old chunk and unembedded new ones — visibly wrong and
    // repairable. Deleting first would leave a gap in the corpus with nothing to
    // show for it.
    crate::services::indexer::embed_and_upsert(&st.pool, &st.embedder, source_id, &inserted, &None)
        .await?;

    // Cascades to chunk_embeddings / graph_nodes / graph_summary /
    // entity_mentions; entity_relationships.source_chunk_id goes NULL.
    sqlx::query("DELETE FROM chunks WHERE id = $1")
        .bind(chunk_id)
        .execute(&st.pool)
        .await?;

    Ok(())
}
