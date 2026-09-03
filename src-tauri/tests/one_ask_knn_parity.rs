//! Feature 010 T004 — KNN parity gate (research R1 caveat, PHR-0002).
//!
//! `chunk_embeddings` is declared WITHOUT `distance_metric=cosine`, so the
//! vec0 `MATCH ?/k=?` KNN form ranks by L2 while the legacy brute-force path
//! ranks by cosine. For L2-normalised embeddings the orders are identical
//! (L2² = 2 − 2·cos). This test gates `retrieval::knn` on exactly that
//! equivalence, plus the indexed-source/watched-folder filter, BEFORE any
//! dependent is built (tasks.md ordering rule).

use echo_ai_lib::db::Db;
use echo_ai_lib::services::drafting::retrieval;

/// Deterministic pseudo-random unit vector (no rand dependency).
fn unit_vec(dim: usize, seed: u64) -> Vec<f32> {
    let mut x = seed.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
    let mut v: Vec<f32> = (0..dim)
        .map(|_| {
            x ^= x << 13;
            x ^= x >> 7;
            x ^= x << 17;
            ((x % 2000) as f32 / 1000.0) - 1.0
        })
        .collect();
    let norm = v.iter().map(|a| a * a).sum::<f32>().sqrt().max(1e-9);
    v.iter_mut().for_each(|a| *a /= norm);
    v
}

async fn embedding_dim(pool: &Db) -> usize {
    let (dim,): (String,) =
        sqlx::query_as("SELECT value FROM db_config WHERE key = 'embedding_dim'")
            .fetch_one(pool)
            .await
            .expect("embedding_dim configured by init_db");
    dim.parse().expect("numeric dim")
}

struct Fixture {
    pool: Db,
    dim: usize,
    /// chunk ids of the eligible (indexed + watched) corpus, by seed.
    eligible: Vec<String>,
}

async fn build_fixture() -> Fixture {
    let pool = echo_ai_lib::db::testing::test_pool().await;
    let dim = embedding_dim(&pool).await;

    sqlx::query(
        "INSERT INTO indexed_folders (id, folder_path, indexing_status, file_count, created_at) \
         VALUES ('f1', '/corpus/', 'indexed', 2, 0)",
    )
    .execute(&pool)
    .await
    .unwrap();

    // Three sources: two eligible, one outside the watched folder, plus one
    // eligible-path source still 'processing' (both must be filtered out).
    for (id, path, status) in [
        ("s1", "/corpus/rules_of_business.md", "indexed"),
        ("s2", "/corpus/financial_powers.md", "indexed"),
        ("s3", "/elsewhere/stray.md", "indexed"),
        ("s4", "/corpus/pending.md", "processing"),
    ] {
        sqlx::query(
            "INSERT INTO sources (id, file_path, file_name, format, file_size_bytes, \
             file_hash, indexing_status, last_modified, created_at) \
             VALUES ($1, $2, $3, 'md', 1, $1, $4, 0, 0)",
        )
        .bind(id)
        .bind(path)
        .bind(path.rsplit('/').next().unwrap())
        .bind(status)
        .execute(&pool)
        .await
        .unwrap();
    }

    let mut eligible = Vec::new();
    for seed in 0..12u64 {
        let chunk_id = format!("c{seed:02}");
        // Spread chunks across sources; seeds 9..12 land on filtered sources.
        let source = match seed {
            0..=4 => "s1",
            5..=8 => "s2",
            9 | 10 => "s3",
            _ => "s4",
        };
        sqlx::query(
            "INSERT INTO chunks (id, source_id, chunk_index, content, char_start, \
             char_end, token_count, created_at) \
             VALUES ($1, $2, $3, $4, 0, 10, 5, 0)",
        )
        .bind(&chunk_id)
        .bind(source)
        .bind(seed as i64)
        .bind(format!("Rule {seed} content of chunk {seed}"))
        .execute(&pool)
        .await
        .unwrap();
        let emb = serde_json::to_string(&unit_vec(dim, seed + 1)).unwrap();
        sqlx::query("INSERT INTO chunk_embeddings (chunk_id, embedding) VALUES ($1, $2::vector)")
            .bind(&chunk_id)
            .bind(emb)
            .execute(&pool)
            .await
            .unwrap();
        if matches!(source, "s1" | "s2") {
            eligible.push(chunk_id);
        }
    }
    Fixture { pool, dim, eligible }
}

/// Legacy brute-force cosine ranking with the same eligibility filter — the
/// reference ordering the KNN seam must reproduce.
async fn brute_force_top_k(pool: &Db, query: &[f32], k: usize) -> Vec<String> {
    let emb_json = serde_json::to_string(query).unwrap();
    let rows: Vec<(String,)> = sqlx::query_as(
        "SELECT ce.chunk_id \
         FROM chunk_embeddings ce \
         JOIN chunks c ON c.id = ce.chunk_id \
         JOIN sources s ON s.id = c.source_id \
         WHERE s.indexing_status = 'indexed' \
           AND EXISTS (SELECT 1 FROM indexed_folders f \
                       WHERE s.file_path LIKE (f.folder_path || '%')) \
         ORDER BY ce.embedding <=> $1::vector ASC \
         LIMIT $2",
    )
    .bind(&emb_json)
    .bind(k as i64)
    .fetch_all(pool)
    .await
    .unwrap();
    rows.into_iter().map(|(id,)| id).collect()
}

#[tokio::test]
async fn knn_top_k_matches_brute_force_cosine_with_filters() {
    let fx = build_fixture().await;
    // Three different query directions, full result-set parity each time.
    for qseed in [101u64, 202, 303] {
        let q = unit_vec(fx.dim, qseed);
        let expected = brute_force_top_k(&fx.pool, &q, 5).await;
        let got: Vec<String> = retrieval::knn(&fx.pool, &q, 5, 4)
            .await
            .unwrap()
            .into_iter()
            .map(|h| h.chunk_id)
            .collect();
        assert_eq!(got, expected, "KNN order diverged for query seed {qseed}");
        // Filter correctness: nothing from s3 (unwatched) or s4 (processing).
        for id in &got {
            assert!(fx.eligible.contains(id), "{id} should have been filtered");
        }
    }
}

#[tokio::test]
async fn knn_hydrates_content_and_respects_k() {
    let fx = build_fixture().await;
    let q = unit_vec(fx.dim, 7);
    let hits = retrieval::knn(&fx.pool, &q, 3, 4).await.unwrap();
    assert_eq!(hits.len(), 3);
    for h in &hits {
        assert!(h.content.contains("content of chunk"));
        assert!(h.file_path.starts_with("/corpus/"));
        assert!(!h.file_name.is_empty());
    }
    // Degenerate inputs.
    assert!(retrieval::knn(&fx.pool, &[], 5, 4).await.unwrap().is_empty());
    assert!(retrieval::knn(&fx.pool, &q, 0, 4).await.unwrap().is_empty());
}
