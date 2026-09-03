# Phase 1 Data Model — Constitution v1

Entities, schema additions (migration 014), and Rust module interfaces. All
storage is in the single `echo.db` (Constitution Principle IV).

## 1. Hierarchy node — extends the existing `chunks` table

The Tier 3 `chunks` table already has `parent_chunk_id`, `unit_type`,
`unit_label`, `unit_path`, `domain_metadata`. v1 changes:

- **`unit_type` vocabulary** extended to: `document` · `part` · `chapter` ·
  `article` · `clause` · `subclause` (plus `section`/`leaf` kept for Tier 3
  compat). Done by widening the CHECK list in migration `013` in place —
  SQLite cannot ALTER a CHECK; safe because the schema bump replays all
  migrations on a wiped DB.
- **`unit_path`** = canonical citation path, e.g. `PartII/Ch1/Art-19A/cl-(1)/sub-(a)`.
- **`domain_metadata`** (JSON) carries per-node:
  - `citation` — human form, e.g. `"Article 19A(1)(a)"`
  - `part_no`, `part_title`, `chapter_no`, `chapter_title`, `article_no`,
    `article_title` (marginal note), `clause_id`, `subclause_id`
  - `legal_domain[]` — e.g. `fundamental-rights`, `judiciary`, `emergency`
  - `amendment` — `{ inserted_by?, substituted_by?, omitted_by?, in_force_from?, is_repealed }`
  - `attached` — provisos/explanations carried in this leaf (text + kind)

**Validation**: every non-`document` node has a `parent_chunk_id` resolving to a
real chunk; `document` node has none; orphans = 0.

## 2. `graph_nodes` (migration 014)

| Column | Type | Notes |
|---|---|---|
| `id` | TEXT PK | UUID |
| `chunk_id` | TEXT | FK → `chunks.id` (the hierarchy node this represents) |
| `node_type` | TEXT | `document/part/chapter/article/clause/subclause` |
| `citation` | TEXT | canonical citation string (indexed) |
| `metrics` | TEXT (JSON) | reserved — precomputed offline analytics (centrality…) |

Index: `graph_nodes(citation)`.

## 3. `graph_edges` (migration 014)

| Column | Type | Notes |
|---|---|---|
| `id` | TEXT PK | UUID |
| `from_node` | TEXT | FK → `graph_nodes.id` |
| `to_node` | TEXT | FK → `graph_nodes.id` |
| `edge_type` | TEXT | `contains` · `cross_reference` · `non_obstante` · `amended_by` |
| `weight` | REAL | optional; default 1.0 |

Indexes (critical for scale): `graph_edges(from_node, edge_type)` and
`graph_edges(to_node, edge_type)` — make 1-hop expansion an index seek
independent of total graph size.

**Validation**: `cross_reference`/`non_obstante` edges resolve to real
`graph_nodes`; no dangling endpoints.

## 4. Edge semantics

| Edge | From → To | Built by |
|---|---|---|
| `contains` | parent → child (tree) | S4 from the hierarchy |
| `cross_reference` | clause/sub-clause → referenced Article | S4, parsing "Article N" in clause text |
| `non_obstante` | clause → overridden Article | S4, "notwithstanding…" detection |
| `amended_by` | node → amendment marker | S4, from IR amendment annotations |

## 5. Module interfaces (in lieu of API contracts — internal feature)

```text
services/doc_profiles/constitution.rs
  DocProfile { level_vocab: [&str;6], heading_patterns, citation_format }

services/structure_parser.rs
  parse_tree(ir: &ConstitutionIR, profile: &DocProfile) -> DocNode
    // DocNode = recursive L0–L5 tree; provisos/explanations attached to clauses

services/graph_builder.rs
  build_graph(tree: &DocNode) -> (Vec<GraphNode>, Vec<GraphEdge>)

db/repositories/graph.rs
  insert_nodes(&pool, Vec<GraphNode>) -> Result<()>
  insert_edges(&pool, Vec<GraphEdge>) -> Result<()>
  expand(&pool, seed_ids: &[String], edge_types: &[&str], max_depth: u8)
      -> Result<Vec<GraphNode>>
    // bounded recursive CTE, depth <= 3, visited-set dedup

query_router.rs
  citation_fast_path(query: &str) -> Option<Vec<ChunkId>>
    // regex "Article N", "N(c)(a)" -> deterministic metadata lookup
```

## 6. Intermediate representation (S1 dev-tool output)

`ConstitutionIR` — JSON, bundled as a static asset. Tree of
`part → chapter → article → clause → subclause`, each node with raw text,
marginal note, provisos[], explanations[], amendment annotations[]. Produced
once, offline, by `tools/constitution_import/`; never fetched at runtime.

## 7. Estimated v1 volumes

~1 document · ~12 parts · ~30 chapters · ~290 articles · ~1,500 clauses ·
~3,000–5,000 sub-clause leaves → ~1.8k `graph_nodes`, ~4k `graph_edges`.
