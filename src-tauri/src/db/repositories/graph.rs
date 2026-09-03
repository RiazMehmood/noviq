//! Repository: `graph_nodes` / `graph_edges` — the legal-document graph layer
//! (feature 003, S4 task T018). Stored inside `echo.db` (ADR-0007, Principle
//! IV); traversed at retrieval time with bounded recursive CTEs.

use anyhow::Result;
use serde::{Deserialize, Serialize};
use crate::db::Db;

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct GraphNode {
    pub id: String,
    /// FK → `chunks.id` — the hierarchy node this graph node mirrors.
    pub chunk_id: String,
    /// `document` … `subclause`.
    pub node_type: String,
    /// Canonical citation, e.g. `"Article 19A(1)"`.
    pub citation: Option<String>,
    /// Reserved — offline-computed analytics (JSON).
    pub metrics: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct GraphEdge {
    pub id: String,
    pub from_node: String,
    pub to_node: String,
    /// `contains` · `cross_reference` · `non_obstante` · `amended_by`.
    pub edge_type: String,
    pub weight: f64,
}

/// One recorded amendment event for a provision (F013 provenance), joined to the
/// amended node's canonical citation. Drives the deterministic amendment chain
/// (Feature 015 US8, FR-005f) — no model produces order/dates/instruments.
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct AmendmentEventRow {
    pub kind: String,
    pub by_instrument: String,
    pub effective_date: Option<String>,
    pub source_page: Option<i64>,
    pub citation: Option<String>,
}

/// Fetch every recorded amendment event for the provision whose citation (or
/// legal path) matches `provision_ref` (case-insensitive substring). Ordered by
/// `effective_date` at the DB; the pure chain builder re-confirms oldest→recent.
pub async fn amendment_events_for_provision(
    pool: &Db,
    provision_ref: &str,
) -> Result<Vec<AmendmentEventRow>> {
    amendment_events_for_provision_scoped(pool, provision_ref, None).await
}

/// F021 — scoped variant: when `scope_source_ids` is Some, only events whose
/// graph node lives in a visible source are returned. The chain narrates
/// instruments/dates, which is answer content, so FR-001 applies to it too.
pub async fn amendment_events_for_provision_scoped(
    pool: &Db,
    provision_ref: &str,
    scope_source_ids: Option<&[String]>,
) -> Result<Vec<AmendmentEventRow>> {
    let pattern = format!("%{}%", provision_ref.to_lowercase());
    if let Some(ids) = scope_source_ids {
        if ids.is_empty() {
            return Ok(vec![]);
        }
        let placeholders = crate::db::pg_in(ids.len(), 2);
        let sql = format!(
            "SELECT ae.kind, ae.by_instrument, ae.effective_date, ae.source_page, gn.citation \
             FROM amendment_events ae \
             JOIN graph_nodes gn ON gn.id = ae.node_id \
             JOIN chunks c ON c.id = gn.chunk_id \
             WHERE (LOWER(gn.citation) LIKE $1 OR LOWER(gn.legal_path) LIKE $1) \
               AND c.source_id IN ({placeholders}) \
             ORDER BY ae.effective_date",
        );
        let mut q = sqlx::query_as::<_, AmendmentEventRow>(&sql).bind(&pattern);
        for id in ids {
            q = q.bind(id);
        }
        return Ok(q.fetch_all(pool).await?);
    }
    Ok(sqlx::query_as::<_, AmendmentEventRow>(
        "SELECT ae.kind, ae.by_instrument, ae.effective_date, ae.source_page, gn.citation \
         FROM amendment_events ae \
         JOIN graph_nodes gn ON gn.id = ae.node_id \
         WHERE LOWER(gn.citation) LIKE $1 OR LOWER(gn.legal_path) LIKE $1 \
         ORDER BY ae.effective_date",
    )
    .bind(&pattern)
    .fetch_all(pool)
    .await?)
}

/// Phase 2 (amendment-awareness) — for the given context chunk ids, the
/// amendment events recorded against each (via its graph node). Returns
/// `(chunk_id, kind, by_instrument, effective_date)` rows so the retrieval path
/// can annotate a provision as amended (the base text may be superseded).
///
/// The amending instruments are almost always OUTSIDE the corpus (constitutional
/// amendment Acts, gazette notifications), so this is an annotation — not a
/// traversable `amended_by` edge (there is no in-corpus node to point at). Scoped
/// to the caller's visible sources: the note names instruments/dates = answer
/// content, so FR-001 applies (mirrors `amendment_events_for_provision_scoped`).
pub async fn amendment_rows_for_chunks(
    pool: &Db,
    chunk_ids: &[String],
    scope_source_ids: Option<&[String]>,
) -> Result<Vec<(String, String, String, Option<String>)>> {
    if chunk_ids.is_empty() {
        return Ok(Vec::new());
    }
    if let Some(ids) = scope_source_ids {
        if ids.is_empty() {
            return Ok(Vec::new());
        }
    }
    let ch_ph = crate::db::pg_in(chunk_ids.len(), 1);
    let mut sql = format!(
        "SELECT c.id, ae.kind, ae.by_instrument, ae.effective_date \
         FROM amendment_events ae \
         JOIN graph_nodes n ON n.id = ae.node_id \
         JOIN chunks c ON c.id = n.chunk_id \
         WHERE c.id IN ({ch_ph})"
    );
    if let Some(ids) = scope_source_ids {
        let src_ph = crate::db::pg_in(ids.len(), chunk_ids.len() + 1);
        sql.push_str(&format!(" AND c.source_id IN ({src_ph})"));
        let mut q = sqlx::query_as::<_, (String, String, String, Option<String>)>(&sql);
        for c in chunk_ids {
            q = q.bind(c);
        }
        for s in ids {
            q = q.bind(s);
        }
        return Ok(q.fetch_all(pool).await?);
    }
    let mut q = sqlx::query_as::<_, (String, String, String, Option<String>)>(&sql);
    for c in chunk_ids {
        q = q.bind(c);
    }
    Ok(q.fetch_all(pool).await?)
}

/// Bulk-insert graph nodes in one transaction.
pub async fn insert_nodes(pool: &Db, nodes: &[GraphNode]) -> Result<()> {
    let mut tx = pool.begin().await?;
    for n in nodes {
        sqlx::query(
            "INSERT INTO graph_nodes (id, chunk_id, node_type, citation, metrics) \
             VALUES ($1, $2, $3, $4, $5)",
        )
        .bind(&n.id)
        .bind(&n.chunk_id)
        .bind(&n.node_type)
        .bind(&n.citation)
        .bind(&n.metrics)
        .execute(&mut *tx)
        .await?;
    }
    tx.commit().await?;
    Ok(())
}

/// Bulk-insert graph edges in one transaction.
pub async fn insert_edges(pool: &Db, edges: &[GraphEdge]) -> Result<()> {
    let mut tx = pool.begin().await?;
    for e in edges {
        sqlx::query(
            "INSERT INTO graph_edges (id, from_node, to_node, edge_type, weight) \
             VALUES ($1, $2, $3, $4, $5)",
        )
        .bind(&e.id)
        .bind(&e.from_node)
        .bind(&e.to_node)
        .bind(&e.edge_type)
        .bind(e.weight)
        .execute(&mut *tx)
        .await?;
    }
    tx.commit().await?;
    Ok(())
}

/// Remove the graph nodes and edges belonging to a source's chunks (re-index).
/// Call this BEFORE deleting the source's `chunks` rows — the edge delete joins
/// through `chunks` to find the source's nodes, so the chunks must still exist.
pub async fn delete_for_source(pool: &Db, source_id: &str) -> Result<()> {
    let mut tx = pool.begin().await?;
    sqlx::query(
        "DELETE FROM graph_edges WHERE from_node IN \
           (SELECT gn.id FROM graph_nodes gn JOIN chunks c ON gn.chunk_id = c.id \
            WHERE c.source_id = $1) \
         OR to_node IN \
           (SELECT gn.id FROM graph_nodes gn JOIN chunks c ON gn.chunk_id = c.id \
            WHERE c.source_id = $1)",
    )
    .bind(source_id)
    .execute(&mut *tx)
    .await?;
    sqlx::query(
        "DELETE FROM graph_nodes WHERE chunk_id IN \
           (SELECT id FROM chunks WHERE source_id = $1)",
    )
    .bind(source_id)
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok(())
}

/// Phase 3 (cross-doc linker) — remove ONLY the cross-document `cross_reference`
/// edges (endpoints in different sources), leaving the within-document overlay
/// built by `graph_builder` intact. Makes `graph_linker::link_corpus` idempotent.
pub async fn delete_cross_doc_cross_reference_edges(pool: &Db) -> Result<u64> {
    let res = sqlx::query(
        "DELETE FROM graph_edges e \
         USING graph_nodes nf, chunks cf, graph_nodes nt, chunks ct \
         WHERE e.from_node = nf.id AND nf.chunk_id = cf.id \
           AND e.to_node   = nt.id AND nt.chunk_id = ct.id \
           AND e.edge_type = 'cross_reference' \
           AND cf.source_id <> ct.source_id",
    )
    .execute(pool)
    .await?;
    Ok(res.rows_affected())
}

/// Rebuild support — remove the WITHIN-document overlay (`cross_reference` /
/// `non_obstante` edges whose endpoints share a source), leaving `contains` tree
/// edges and the cross-document overlay (from `graph_linker`) intact. Lets
/// `graph_linker::rebuild_overlay` regenerate the intra-doc overlay from existing
/// chunk text without a full re-ingest. Disjoint from
/// `delete_cross_doc_cross_reference_edges` (that one requires different sources).
pub async fn delete_intra_doc_overlay_edges(pool: &Db) -> Result<u64> {
    let res = sqlx::query(
        "DELETE FROM graph_edges e \
         USING graph_nodes nf, chunks cf, graph_nodes nt, chunks ct \
         WHERE e.from_node = nf.id AND nf.chunk_id = cf.id \
           AND e.to_node   = nt.id AND nt.chunk_id = ct.id \
           AND e.edge_type IN ('cross_reference','non_obstante') \
           AND cf.source_id = ct.source_id",
    )
    .execute(pool)
    .await?;
    Ok(res.rows_affected())
}

/// The graph-node ids backing the given chunk ids — the seed set for `expand`.
pub async fn node_ids_for_chunks(
    pool: &Db,
    chunk_ids: &[String],
) -> Result<Vec<String>> {
    if chunk_ids.is_empty() {
        return Ok(Vec::new());
    }
    let ph = crate::db::pg_in(chunk_ids.len(), 1);
    let sql = format!("SELECT id FROM graph_nodes WHERE chunk_id IN ({ph})");
    let mut q = sqlx::query_scalar::<_, String>(&sql);
    for c in chunk_ids {
        q = q.bind(c);
    }
    Ok(q.fetch_all(pool).await?)
}

/// Bounded local-neighbourhood expansion. From `seed_ids`, follow out-edges
/// (`from_node -> to_node`) whose `edge_type` is in `edge_types`, up to
/// `max_depth` hops — hard-capped at 3 (ADR-0007 cost bound). `UNION` in the
/// recursive CTE provides visited-set dedup. Returns the distinct nodes
/// reached, seeds included.
pub async fn expand(
    pool: &Db,
    seed_ids: &[String],
    edge_types: &[&str],
    max_depth: u8,
) -> Result<Vec<GraphNode>> {
    if seed_ids.is_empty() || edge_types.is_empty() {
        return Ok(Vec::new());
    }
    let depth = i64::from(max_depth.min(3));
    let seeds_ph = crate::db::pg_in(seed_ids.len(), 1);
    let p_depth = seed_ids.len() + 1;
    let etypes_ph = crate::db::pg_in(edge_types.len(), seed_ids.len() + 2);
    let sql = format!(
        "WITH RECURSIVE reach(id, depth) AS ( \
            SELECT id, 0 FROM graph_nodes WHERE id IN ({seeds_ph}) \
            UNION \
            SELECT e.to_node, r.depth + 1 \
              FROM reach r JOIN graph_edges e ON e.from_node = r.id \
             WHERE r.depth < ${p_depth} AND e.edge_type IN ({etypes_ph}) \
         ) \
         SELECT DISTINCT n.id, n.chunk_id, n.node_type, n.citation, n.metrics \
           FROM graph_nodes n JOIN reach r ON n.id = r.id"
    );
    let mut q = sqlx::query_as::<_, GraphNode>(&sql);
    for s in seed_ids {
        q = q.bind(s);
    }
    q = q.bind(depth);
    for t in edge_types {
        q = q.bind(*t);
    }
    Ok(q.fetch_all(pool).await?)
}

/// Table completion via the `contains` hierarchy (F025). Given seed CHUNK ids,
/// find any `table` / `table_row` nodes among them, resolve each to its parent
/// `table` node (a row seed walks the `contains` edge UPWARD), and return the whole
/// table: the table node plus ALL its `table_row` children. This is what lets a
/// "how many / list all departments" query — which only retrieves a few of the
/// rows by similarity — pull the ENTIRE table so the answer can count/enumerate it.
///
/// `expand` follows out-edges only, so the row→siblings case (the common one) needs
/// this dedicated both-directions walk rather than adding `contains` to `expand`.
/// Returns the reached nodes (empty when no table/row is among the seeds).
pub async fn expand_table_siblings(pool: &Db, seed_chunk_ids: &[String]) -> Result<Vec<GraphNode>> {
    if seed_chunk_ids.is_empty() {
        return Ok(Vec::new());
    }
    let ph = crate::db::pg_in(seed_chunk_ids.len(), 1);
    // seeds → parent tables (a seed that IS a table, OR the table containing a seed
    // row) → all rows those tables contain (plus the table nodes themselves).
    let sql = format!(
        "WITH seeds AS ( \
             SELECT id, node_type FROM graph_nodes \
              WHERE chunk_id IN ({ph}) AND node_type IN ('table','table_row') \
         ), \
         tables AS ( \
             SELECT id FROM seeds WHERE node_type = 'table' \
             UNION \
             SELECT e.from_node AS id FROM graph_edges e \
              WHERE e.edge_type = 'contains' AND e.to_node IN (SELECT id FROM seeds) \
         ) \
         SELECT DISTINCT n.id, n.chunk_id, n.node_type, n.citation, n.metrics \
           FROM graph_nodes n \
          WHERE n.id IN (SELECT id FROM tables) \
             OR n.id IN ( \
                 SELECT e.to_node FROM graph_edges e \
                  WHERE e.edge_type = 'contains' AND e.from_node IN (SELECT id FROM tables) \
             )"
    );
    let mut q = sqlx::query_as::<_, GraphNode>(&sql);
    for c in seed_chunk_ids {
        q = q.bind(c);
    }
    Ok(q.fetch_all(pool).await?)
}

/// Bounded table completion (2026-08-06). Same intent as [`expand_table_siblings`]
/// — pull a whole table so a "how many / list all" question can enumerate it —
/// but with the three properties that one lacks, each of which caused a live
/// production failure on 2026-08-06 (trace: "How many members are on the SPPRA
/// board?", which was refused with 0 citations):
///
/// 1. **ONE table, chosen by rank.** `expand_table_siblings` resolves every table
///    reachable from ANY seed in the pool, so a single query pulled rows from
///    three unrelated tables at once (PC-I Manual, Delegation of Financial
///    Powers, Rules of Business Schedule-I). Here `ranked_chunk_ids` arrives in
///    RANK ORDER and `array_position` picks the best-ranked table seed; only that
///    table is expanded.
/// 2. **Reading order.** `expand_table_siblings` has no `ORDER BY`, so its row
///    order is whatever Postgres yields for the `DISTINCT` — and the caller's
///    `truncate(64)` therefore sliced a large table ARBITRARILY, not at row 64.
///    Rows come back `chunks.chunk_index` ascending, table node first.
/// 3. **A real bound.** The `LIMIT` is applied at the DB, not by truncating a
///    fetched list.
///
/// Returns chunk ids (empty when no id in `ranked_chunk_ids` is a table/row node).
/// Callers still scope-clamp the fetched chunks — this walks the graph, which is
/// not itself scope-aware.
pub async fn table_chunks_for_best_seed(
    pool: &Db,
    ranked_chunk_ids: &[String],
    limit: i64,
) -> Result<Vec<String>> {
    if ranked_chunk_ids.is_empty() || limit <= 0 {
        return Ok(Vec::new());
    }
    // `array_position` returns the 1-based index of the chunk id in the ranked
    // array — i.e. its rank — so the best-ranked table seed wins in SQL.
    let sql = "\
        WITH seeds AS ( \
            SELECT n.id, n.node_type, \
                   array_position($1::text[], n.chunk_id) AS rank \
              FROM graph_nodes n \
             WHERE n.chunk_id = ANY($1::text[]) \
               AND n.node_type IN ('table','table_row') \
        ), \
        best AS (SELECT id, node_type FROM seeds ORDER BY rank ASC LIMIT 1), \
        tbl AS ( \
            SELECT id FROM best WHERE node_type = 'table' \
            UNION \
            SELECT e.from_node AS id FROM graph_edges e \
             WHERE e.edge_type = 'contains' AND e.to_node IN (SELECT id FROM best) \
        ) \
        SELECT n.chunk_id \
          FROM graph_nodes n \
          JOIN chunks c ON c.id = n.chunk_id \
         WHERE n.id IN (SELECT id FROM tbl) \
            OR n.id IN ( \
                SELECT e.to_node FROM graph_edges e \
                 WHERE e.edge_type = 'contains' AND e.from_node IN (SELECT id FROM tbl) \
            ) \
         ORDER BY (n.node_type <> 'table'), c.chunk_index \
         LIMIT $2";
    let rows: Vec<(String,)> = sqlx::query_as(sql)
        .bind(ranked_chunk_ids.to_vec())
        .bind(limit)
        .fetch_all(pool)
        .await?;
    Ok(rows.into_iter().map(|r| r.0).collect())
}
