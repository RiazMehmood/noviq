-- Migration 020 — Legal IR foundation (Feature 007 Slice 0).
--
-- Strictly additive: existing Feature 006 graph_nodes rows stay valid.
-- New columns are nullable or carry a DEFAULT; new tables are independent.
-- Schema version bumps 15 → 16 in src-tauri/src/db/schema.rs.
--
-- See specs/007-legal-rag-pakistan/data-model.md (E1–E7) for the
-- canonical schema description.

-- ---------------------------------------------------------------------
-- 0. Document index tier (sticky `fast` | `deep`).
--    The application's ingested-document table is `sources` (from migration 001).
--    Spec/data-model use the generic noun "document" but the SQL identifier is
--    `sources`.
-- ---------------------------------------------------------------------
ALTER TABLE sources ADD COLUMN index_tier TEXT NOT NULL DEFAULT 'fast'
  CHECK(index_tier IN ('fast', 'deep'));

-- ---------------------------------------------------------------------
-- 1. graph_nodes legal-domain columns.
-- ---------------------------------------------------------------------
ALTER TABLE graph_nodes ADD COLUMN instrument_id TEXT;
ALTER TABLE graph_nodes ADD COLUMN legal_path    TEXT;
ALTER TABLE graph_nodes ADD COLUMN in_force      INTEGER NOT NULL DEFAULT 1;
ALTER TABLE graph_nodes ADD COLUMN source_page   INTEGER;

CREATE INDEX IF NOT EXISTS idx_graph_nodes_instrument
  ON graph_nodes(instrument_id);
CREATE INDEX IF NOT EXISTS idx_graph_nodes_legal_path
  ON graph_nodes(instrument_id, legal_path);

-- ---------------------------------------------------------------------
-- 2. Amendment events — one row per insert / substitute / repeal / omit.
-- ---------------------------------------------------------------------
CREATE TABLE IF NOT EXISTS amendment_events (
  id             INTEGER PRIMARY KEY AUTOINCREMENT,
  node_id        TEXT    NOT NULL,
  kind           TEXT    NOT NULL CHECK(kind IN ('inserted', 'substituted', 'repealed', 'omitted')),
  by_instrument  TEXT    NOT NULL,
  effective_date TEXT,
  source_page    INTEGER,
  raw_marker     TEXT,
  FOREIGN KEY (node_id) REFERENCES graph_nodes(id) ON DELETE CASCADE
);
CREATE INDEX IF NOT EXISTS idx_amendment_events_node
  ON amendment_events(node_id);

-- ---------------------------------------------------------------------
-- 3. Typed cross-reference edges (cites / implements / under / amends).
--    Named `legal_xrefs` to avoid collision with the existing
--    `graph_edges` table from migration 014 (Feature 003 — Constitution v1
--    hierarchical graph) which carries different edge semantics.
-- ---------------------------------------------------------------------
CREATE TABLE IF NOT EXISTS legal_xrefs (
  id            INTEGER PRIMARY KEY AUTOINCREMENT,
  src_node_id   TEXT    NOT NULL,
  dst_node_id   TEXT    NOT NULL,
  edge_kind     TEXT    NOT NULL CHECK(edge_kind IN ('cites', 'implements', 'under', 'amends')),
  confidence    REAL    NOT NULL DEFAULT 1.0,
  source_phrase TEXT,
  FOREIGN KEY (src_node_id) REFERENCES graph_nodes(id) ON DELETE CASCADE,
  FOREIGN KEY (dst_node_id) REFERENCES graph_nodes(id) ON DELETE CASCADE
);
CREATE INDEX IF NOT EXISTS idx_legal_xrefs_src
  ON legal_xrefs(src_node_id, edge_kind);
CREATE INDEX IF NOT EXISTS idx_legal_xrefs_dst
  ON legal_xrefs(dst_node_id, edge_kind);

-- ---------------------------------------------------------------------
-- 4. Procedures + procedure steps (Slice D officer workflow flows).
-- ---------------------------------------------------------------------
CREATE TABLE IF NOT EXISTS procedures (
  id              INTEGER PRIMARY KEY AUTOINCREMENT,
  anchor_node_id  TEXT    NOT NULL,
  process_name    TEXT    NOT NULL,
  description     TEXT,
  FOREIGN KEY (anchor_node_id) REFERENCES graph_nodes(id) ON DELETE CASCADE
);

CREATE TABLE IF NOT EXISTS procedure_steps (
  id                 INTEGER PRIMARY KEY AUTOINCREMENT,
  procedure_id       INTEGER NOT NULL,
  step_order         INTEGER NOT NULL,
  actor              TEXT    NOT NULL,
  action             TEXT    NOT NULL,
  condition          TEXT,
  branch_of_step_id  INTEGER,
  timeline           TEXT,
  source_node_id     TEXT    NOT NULL,
  FOREIGN KEY (procedure_id)      REFERENCES procedures(id)       ON DELETE CASCADE,
  FOREIGN KEY (branch_of_step_id) REFERENCES procedure_steps(id),
  FOREIGN KEY (source_node_id)    REFERENCES graph_nodes(id)
);
CREATE INDEX IF NOT EXISTS idx_procedure_steps_proc
  ON procedure_steps(procedure_id, step_order);

-- ---------------------------------------------------------------------
-- 5. Schema version marker.
-- ---------------------------------------------------------------------
INSERT INTO db_config(key, value) VALUES ('schema_version', '16')
  ON CONFLICT(key) DO UPDATE SET value = excluded.value;
