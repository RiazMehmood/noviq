-- ============================================================
-- EchoAI — Constitution hierarchical + graph (migration 014)
-- Feature 003-constitution-hierarchical-graph. See ADR-0007.
--
-- Adds the legal-document graph layer as two tables INSIDE echo.db
-- (Constitution Principle IV — single DB file, no dedicated graph engine).
-- Traversed at retrieval time with bounded recursive CTEs.
--
-- The chunks.unit_type vocabulary was widened in migration 013 (edited in
-- place — see that file's header for why this is safe).
-- ============================================================

-- ── graph_nodes ─────────────────────────────────────────────────────────────
-- One row per hierarchy node (mirrors a row in `chunks`).
CREATE TABLE IF NOT EXISTS graph_nodes (
    id         TEXT PRIMARY KEY,
    chunk_id   TEXT NOT NULL REFERENCES chunks(id),
    node_type  TEXT NOT NULL
               CHECK (node_type IN ('document','part','chapter','article','clause','subclause')),
    citation   TEXT,                       -- canonical citation, e.g. "Article 19A(1)"
    metrics    TEXT                        -- reserved: offline-computed analytics (JSON)
);

-- ── graph_edges ─────────────────────────────────────────────────────────────
-- Typed edges between graph_nodes. `contains` = tree edges; the rest are the
-- legal graph overlay.
CREATE TABLE IF NOT EXISTS graph_edges (
    id         TEXT PRIMARY KEY,
    from_node  TEXT NOT NULL REFERENCES graph_nodes(id),
    to_node    TEXT NOT NULL REFERENCES graph_nodes(id),
    edge_type  TEXT NOT NULL
               CHECK (edge_type IN ('contains','cross_reference','non_obstante','amended_by')),
    weight     REAL NOT NULL DEFAULT 1.0
);

-- Composite edge indexes — make 1-hop expansion an index seek regardless of
-- total graph size (the key to scaling local-neighbourhood expansion).
CREATE INDEX IF NOT EXISTS idx_graph_nodes_citation ON graph_nodes(citation);
CREATE INDEX IF NOT EXISTS idx_graph_nodes_chunk    ON graph_nodes(chunk_id);
CREATE INDEX IF NOT EXISTS idx_graph_edges_from     ON graph_edges(from_node, edge_type);
CREATE INDEX IF NOT EXISTS idx_graph_edges_to       ON graph_edges(to_node, edge_type);

INSERT INTO db_config(key, value) VALUES ('schema_version', '14')
    ON CONFLICT(key) DO UPDATE SET value = excluded.value;
