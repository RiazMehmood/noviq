//! Feature 010 — the single vector-search seam (ADR-0014, research R1).
//!
//! ALL vector retrieval in 010+ code goes through [`knn`]. The sqlite-vec
//! `MATCH ?/k=?` KNN form is an implementation detail inside this module; a
//! pgvector backend replaces the internals without touching callers. No call
//! site may inline vec0 SQL.

use crate::db::Db;

/// Lexical leg of hybrid retrieval (H, 2026-06-13): `websearch_to_tsquery`
/// over the generated `tsv` column, same indexed/watched-folder filters as
/// [`knn`]. `distance` carries `ts_rank_cd` (HIGHER is better) — callers
/// consume only the ORDER (RRF in one_ask research merges by rank).
pub async fn lexical(pool: &Db, query: &str, k: usize) -> Result<Vec<Hit>, String> {
    if query.trim().is_empty() || k == 0 {
        return Ok(vec![]);
    }
    let rows: Vec<(String, String, String, String, String, f64)> = sqlx::query_as(
        "SELECT c.id, c.source_id, s.file_name, s.file_path, c.content, \
                ts_rank_cd(c.tsv, websearch_to_tsquery('english', $1))::float8 AS score \
         FROM chunks c JOIN sources s ON s.id = c.source_id \
         WHERE c.tsv @@ websearch_to_tsquery('english', $1) \
           AND s.indexing_status = 'indexed' \
           AND c.is_boilerplate = FALSE \
           AND EXISTS (SELECT 1 FROM indexed_folders f \
                       WHERE s.file_path LIKE (f.folder_path || '%')) \
         ORDER BY score DESC LIMIT $2",
    )
    .bind(query)
    .bind(k as i64)
    .fetch_all(pool)
    .await
    .map_err(|e| format!("lexical query failed: {e}"))?;
    Ok(rows
        .into_iter()
        .map(|(chunk_id, source_id, file_name, file_path, content, score)| Hit {
            chunk_id,
            source_id,
            file_name,
            file_path,
            content,
            distance: score,
        })
        .collect())
}

/// One KNN hit, hydrated to its document.
#[derive(Debug, Clone)]
pub struct Hit {
    pub chunk_id: String,
    pub source_id: String,
    pub file_name: String,
    pub file_path: String,
    pub content: String,
    pub distance: f64,
}

/// Indexed KNN over `chunk_embeddings`, filtered to indexed sources under a
/// watched folder, KNN order preserved, truncated to `k`.
///
/// Two-step (vec0 KNN queries cannot take joins): (1) `embedding MATCH ?` with
/// `k = k×overfetch` against the virtual table only; (2) hydrate + filter via
/// `chunks`/`sources`/`indexed_folders`.
pub async fn knn(
    pool: &Db,
    embedding: &[f32],
    k: usize,
    overfetch: usize,
) -> Result<Vec<Hit>, String> {
    if embedding.is_empty() || k == 0 {
        return Ok(vec![]);
    }
    let emb_json = serde_json::to_string(embedding).map_err(|e| e.to_string())?;
    let fetch_k = (k * overfetch.max(1)) as i64;

    // Step 1: pure KNN on the vec0 table (uses the index; brute-force scan is
    // the 281 s failure mode this module exists to kill — research R1).
    let d = crate::db::schema::embedding_dim();
    let knn_sql = format!(
        "SELECT chunk_id, (embedding::halfvec({d}) <=> $1::vector::halfvec({d})) AS distance \
         FROM chunk_embeddings \
         ORDER BY embedding::halfvec({d}) <=> $1::vector::halfvec({d}) ASC LIMIT $2",
    );
    let knn_rows: Vec<(String, f64)> = sqlx::query_as(&knn_sql)
    .bind(&emb_json)
    .bind(fetch_k)
    .fetch_all(pool)
    .await
    .map_err(|e| format!("pgvector KNN query failed: {e}"))?;

    if knn_rows.is_empty() {
        return Ok(vec![]);
    }

    // Step 2: hydrate + filter, preserving KNN order.
    let mut hits = Vec::with_capacity(k);
    for (chunk_id, distance) in knn_rows {
        let row: Option<(String, String, String, String)> = sqlx::query_as(
            "SELECT c.id, c.source_id, s.file_name, s.file_path \
             FROM chunks c JOIN sources s ON s.id = c.source_id \
             WHERE c.id = $1 AND s.indexing_status = 'indexed' \
               AND c.is_boilerplate = FALSE \
               AND EXISTS (SELECT 1 FROM indexed_folders f \
                           WHERE s.file_path LIKE (f.folder_path || '%'))",
        )
        .bind(&chunk_id)
        .fetch_optional(pool)
        .await
        .map_err(|e| e.to_string())?;
        if let Some((cid, source_id, file_name, file_path)) = row {
            let (content,): (String,) =
                sqlx::query_as("SELECT content FROM chunks WHERE id = $1")
                    .bind(&cid)
                    .fetch_one(pool)
                    .await
                    .map_err(|e| e.to_string())?;
            hits.push(Hit {
                chunk_id: cid,
                source_id,
                file_name,
                file_path,
                content,
                distance,
            });
            if hits.len() == k {
                break;
            }
        }
    }
    Ok(hits)
}
