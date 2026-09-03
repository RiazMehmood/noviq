# Phase 1 — Data Model

**Feature:** 007 Pakistan Government Legal RAG
**Date:** 2026-06-02
**Migration:** `src-tauri/src/db/migrations/020_legal_ir.sql` (additive — `TARGET_SCHEMA_VERSION` 15 → 16).

This document is the canonical reference for the entities, fields,
relationships, validation rules, and state transitions added by Feature
007. All persistent state lives in `echo.db` (constitution §IV).

---

## E1. Document tier (extends `sources`)

The application's existing **`sources`** table (from migration 001 —
`file_path`, `file_name`, `format`, `file_size_bytes`, `file_hash`,
`indexing_status`, …) is the document table. Migration 020 adds
`index_tier`. (The spec calls this entity a "document" for officer
readability; SQL identifier is `sources`.)

| Field | Type | Notes |
|---|---|---|
| `id` | TEXT PRIMARY KEY | unchanged |
| `file_path` | TEXT NOT NULL UNIQUE | unchanged |
| `file_hash` | TEXT NOT NULL | sha256; used for sidecar caching (E8) |
| `index_tier` | TEXT NOT NULL DEFAULT 'fast' CHECK(index_tier IN ('fast','deep')) | **NEW** — sticky once `deep`. |
| `… other existing columns …` | — | unchanged |

**State transitions:**

- `fast → deep`: officer triggers `deep_reindex_document`. Atomic — old chunks deleted and new chunks inserted in one transaction at the end of the pipeline. Sticky.
- `deep → deep` (re-upload, sha256 hit): JSON IR sidecar reused from disk; only re-chunk + re-embed. Stays deep.
- `deep → fast`: explicit downgrade only. Deletes all 007 deep-only artifacts for the doc (amendment_events, graph_edges where this doc is src or dst, procedures, procedure_steps) and re-runs the fast path.

---

## E2. Instrument

Each Pakistan government instrument is represented as a root node in `graph_nodes` with `kind = 'Instrument'`. The same `graph_nodes` table that 006 uses, extended with legal-domain columns.

| Field | Type | Source | Notes |
|---|---|---|---|
| `id` | TEXT PRIMARY KEY | 006 | unchanged |
| `document_id` | TEXT | 006 | unchanged |
| `kind` | TEXT | 006 | values from the legal NodeKind enum (see E3) |
| `instrument_id` | TEXT | **020** | e.g. "Act-XXXIV-of-2017"; NULL for non-legal docs |
| `legal_path` | TEXT | **020** | e.g. "Ch-III/S-12/(2)/(a)"; NULL on instrument roots |
| `in_force` | INTEGER NOT NULL DEFAULT 1 | **020** | bool; 0 if repealed |
| `source_page` | INTEGER | **020** | 1-based page of the source PDF |
| `parent_id` | TEXT | 006 | unchanged; NULL on instrument roots |
| `number` | TEXT | 006 | e.g. "25", "(2)", "(a)" |
| `title` | TEXT | 006 | e.g. "Equality of citizens" |
| `text` | TEXT | 006 | full text of this node |

**Indexes:**

- `idx_graph_nodes_instrument` on `(instrument_id)`
- `idx_graph_nodes_legal_path` on `(instrument_id, legal_path)`

Both fast-tier and deep-tier docs use `graph_nodes`; only deep docs populate `instrument_id`/`legal_path`/`in_force`/`source_page`.

---

## E3. NodeKind enum (legal)

Values written to `graph_nodes.kind` for Feature 007 documents:

| Value | Description |
|---|---|
| `Instrument` | Root node of one legal document |
| `Preamble` | The preamble of an Act, Ordinance, Constitution |
| `Chapter` | Top-level division |
| `Part` | Alt top-level division |
| `Section` | A numbered Section of an Act / Ordinance / Rules |
| `Article` | A numbered Article of the Constitution |
| `SubSection` | `(1)`, `(2)`, ... within a Section |
| `Clause` | `(a)`, `(b)`, ... within a Sub-section |
| `SubClause` | `(i)`, `(ii)`, ... within a Clause |
| `Proviso` | "Provided that ..." attached to a Section/Sub-section |
| `Explanation` | "Explanation. ..." block |
| `Illustration` | Illustrative example block |
| `Schedule` | A Schedule attached to the instrument |
| `ScheduleItem` | An item within a Schedule |
| `Definition` | Definition entry, typically under a "Definitions" Section |

Generic 006 kinds (`Section`, `Clause`, `Paragraph`, etc., from non-legal docs) coexist in the same table; deep-pipeline writes the legal subset above.

---

## E4. AmendmentEvent (new table)

One row per amendment event applied to one node.

| Field | Type | Notes |
|---|---|---|
| `id` | INTEGER PRIMARY KEY AUTOINCREMENT | |
| `node_id` | TEXT NOT NULL | FK → `graph_nodes(id)` ON DELETE CASCADE |
| `kind` | TEXT NOT NULL CHECK | `'inserted' \| 'substituted' \| 'repealed' \| 'omitted'` |
| `by_instrument` | TEXT NOT NULL | e.g. "Act-XII-of-2021" — free text v1; normalized later |
| `effective_date` | TEXT NULL | ISO `YYYY-MM-DD` if extractable |
| `source_page` | INTEGER NULL | |
| `raw_marker` | TEXT NULL | preserved literal marker from source (e.g. `1[…]1` or `[Subs. by Act 12 of 2021]`) |

**Index:** `idx_amendment_events_node` on `(node_id)`.

**Lifecycle:** Written by `legal_ir::amendment_parser` during Slice A. Read by the Difference (amendment flavor), Timeline (amendment flavor), and Lookup intents when rendering "as substituted by …" citation tails.

**Edge case:** Unparseable marker → row written with `kind = 'inserted'` placeholder (best-guess default), `by_instrument = 'UNKNOWN'`, `raw_marker` preserved. Flagged for human review in eval reports but does not block the document.

---

## E5. LegalXref (new table `legal_xrefs`)

Typed cross-reference edges between legal-IR nodes. Named `legal_xrefs`
(not `graph_edges`) to avoid collision with the pre-existing
`graph_edges` table from migration 014 (Feature 003 constitution graph),
which carries different edge semantics.

| Field | Type | Notes |
|---|---|---|
| `id` | INTEGER PRIMARY KEY AUTOINCREMENT | |
| `src_node_id` | TEXT NOT NULL | FK → `graph_nodes(id)` ON DELETE CASCADE |
| `dst_node_id` | TEXT NOT NULL | FK → `graph_nodes(id)` ON DELETE CASCADE |
| `edge_kind` | TEXT NOT NULL CHECK | `'cites' \| 'implements' \| 'under' \| 'amends'` |
| `confidence` | REAL NOT NULL DEFAULT 1.0 | 1.0 for unambiguous regex; < 1.0 for LLM-resolved |
| `source_phrase` | TEXT NULL | the literal phrase that produced the edge, e.g. "section 12 of the X Act, 2017" |

**Indexes:**
- `idx_legal_xrefs_src` on `(src_node_id, edge_kind)`
- `idx_legal_xrefs_dst` on `(dst_node_id, edge_kind)`

**Edge kinds:**
- `cites` — any node references another node by name.
- `implements` — a Rules section implements a Section of an Act.
- `under` — a Policy / Rule operates under an Act/Section.
- `amends` — an amending instrument's section amends another instrument's section. (The destination node carries an AmendmentEvent; the edge makes the reverse direction queryable.)

**Lifecycle:** Written by `legal_ir::xref_extractor` during Slice C. Consumed by Difference (instrument flavor), CrossCompare, Procedure stitching, and the "see also" surface.

---

## E6. Procedure (new table)

One process-flow per procedural section.

| Field | Type | Notes |
|---|---|---|
| `id` | INTEGER PRIMARY KEY AUTOINCREMENT | |
| `anchor_node_id` | TEXT NOT NULL | FK → `graph_nodes(id)` ON DELETE CASCADE |
| `process_name` | TEXT NOT NULL | e.g. "Grant of arms license" |
| `description` | TEXT NULL | short officer-facing summary |

**Lifecycle:** Written by `legal_ir::procedure_extractor` during Slice D, only on nodes the procedural-section classifier tagged as procedural. Consumed by the Procedure intent handler.

---

## E7. ProcedureStep (new table)

One step in a procedure.

| Field | Type | Notes |
|---|---|---|
| `id` | INTEGER PRIMARY KEY AUTOINCREMENT | |
| `procedure_id` | INTEGER NOT NULL | FK → `procedures(id)` ON DELETE CASCADE |
| `step_order` | INTEGER NOT NULL | 1-based |
| `actor` | TEXT NOT NULL | e.g. "Director", "Secretary", "Town Officer", "SHO", "DPO" — free-text v1 |
| `action` | TEXT NOT NULL | what the actor does |
| `condition` | TEXT NULL | e.g. "if amount > 5 lakh" |
| `branch_of_step_id` | INTEGER NULL | FK → `procedure_steps(id)`; non-NULL = branching child |
| `timeline` | TEXT NULL | e.g. "within 30 days" |
| `source_node_id` | TEXT NOT NULL | FK → `graph_nodes(id)`; citation anchor for this step |

**Index:** `idx_procedure_steps_proc` on `(procedure_id, step_order)`.

**Branching semantics:** A step with `branch_of_step_id = NULL` is part of the main linear path. A step with `branch_of_step_id = X` is a conditional branch hanging off step `X`. The renderer shows linear by default; "show branches" expands children.

---

## E8. JSON IR sidecar (cache, not authoritative)

On-disk cache at `{app_data_dir}/ir_cache/<sha256>.json`. Schema:

```json
{
  "schema_version": 16,
  "instrument_title": "The Right of Access to Information Act, 2017",
  "instrument_id": "Act-XXXIV-of-2017",
  "year": 2017,
  "nodes": [
    {
      "id": "n0001",
      "kind": "Instrument",
      "instrument_id": "Act-XXXIV-of-2017",
      "legal_path": "",
      "parent_id": null,
      "number": null,
      "title": "The Right of Access to Information Act, 2017",
      "text": "...",
      "source_page": 1,
      "in_force": true
    }
    /* ... */
  ]
}
```

Authoritative state lives in `echo.db` per constitution §IV; the sidecar is regenerable from the source PDF + Mistral and is treated as a cache for re-upload acceleration.

---

## E9. ChatQuery + IntentClassification (in-memory only)

Not persisted in v1 (FR-031 forbids audit logs; saved queries deferred to v2). Lives in handler scope:

```rust
struct ChatQuery {
    text: String,
    scope: Scope,                    // Both | Folder | Doc(id) | Notebook | NotebookDoc(id)
    use_cloud: bool,                 // explicit officer opt-in
}

struct IntentClassification {
    intent: Intent,
    confidence: f32,
    slots: Slots,
}

enum Intent { Lookup, ListChildren, Summarize, Difference, CrossCompare, ActionItems, Timeline, Procedure, Fallback }

struct Slots {
    instrument: Option<String>,
    section_path: Option<String>,
    topic: Option<String>,
    comparands: Vec<String>,
    timeline_anchor: Option<String>,
}
```

---

## Validation rules

| Rule | Where enforced |
|---|---|
| `documents.index_tier` ∈ `{fast, deep}` | DB CHECK constraint |
| `amendment_events.kind` ∈ `{inserted, substituted, repealed, omitted}` | DB CHECK constraint |
| `graph_edges.edge_kind` ∈ `{cites, implements, under, amends}` | DB CHECK constraint |
| `graph_edges.confidence` in `[0.0, 1.0]` | application-level (no DB CHECK to avoid blocking writes) |
| `procedure_steps.branch_of_step_id` references same `procedure_id` as the parent step | application-level invariant in `procedure_extractor` and renderer |
| Every node on a deep-indexed instrument has a valid `parent_id` chain ending at an `Instrument` node | enforced by Slice A eval gate (zero orphans) |

---

## Relationships (entity-relationship sketch)

```
documents 1—* graph_nodes
graph_nodes 1—* amendment_events   (on deep tier only)
graph_nodes 1—* graph_edges        (as src; one node may also be dst of many edges)
graph_nodes 1—* procedures         (as anchor_node)
procedures  1—* procedure_steps
procedure_steps *—1 graph_nodes    (source citation)
procedure_steps 0..1—* procedure_steps (branch_of self-relation)
```

---

## Migration ordering and rollback

Migration 020 is **additive only** — new columns are nullable or carry defaults; new tables are independent. Per constitution Development Constraints, additive migrations require no rollback plan. Downgrading a document deep → fast (E1) is the only data-removal operation Feature 007 introduces, and it is per-document, officer-initiated, and reversible by re-running Deep re-index.
