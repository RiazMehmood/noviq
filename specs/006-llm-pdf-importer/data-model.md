# Phase 1 Data Model — LLM PDF → IR Importer

**Feature**: 006-llm-pdf-importer
**Date**: 2026-05-29

Defines: in-memory IR types (Rust), on-disk JSON shape, SQLite schema additions, and lifecycle states.

---

## 1. In-memory IR types (Rust)

Location: `src-tauri/src/services/doc_ir/{mod.rs, types.rs}`. New module — sits beside `constitution_ir.rs`. Constitution path is unchanged.

```rust
/// Root IR — output of MistralIrBuilder, input to indexer's LLM IR path.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DocIr {
    pub doc_id: String,                  // sha256 of source PDF bytes
    pub doc_type: DocType,
    pub title: String,
    pub source_pdf: String,              // absolute path at extraction time
    pub extracted_at: chrono::DateTime<chrono::Utc>,
    pub llm_model: String,               // "mistral-7b-instruct-v0.3-q4_k_m"
    pub schema_version: u32,             // current = 1
    pub nodes: Vec<StructureNode>,
    #[serde(default)]
    pub doc_metadata: serde_json::Value, // per-doc-type top-level extras
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum DocType { Constitution, Act, Rules, Sro, Contract, Generic }

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StructureNode {
    pub id: String,                      // "n0001"-style, doc-unique
    pub parent_id: Option<String>,
    pub kind: NodeKind,
    pub number: Option<String>,          // "1", "I", "(a)", "3.2", "III-A"
    pub title: Option<String>,
    pub text: String,                    // may be "" for omissions / containers
    pub page: u32,                       // 1-based page index in source PDF
    #[serde(default)]
    pub annotations: Vec<Annotation>,
    #[serde(default)]
    pub footnote_refs: Vec<String>,      // ids of kind=footnote nodes
    #[serde(default)]
    pub metadata: serde_json::Value,     // per-doc-type overlay (see below)
    pub quality_flag: QualityFlag,
    #[serde(default)]
    pub quality_error: Option<String>,   // validator message when low_confidence
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum NodeKind {
    Part, Chapter, Section, Subsection,
    Clause, SubClause, Proviso, Explanation,
    Schedule, Preamble, Paragraph, Heading, Footnote,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum QualityFlag { Ok, LowConfidence, UserRepaired }

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Annotation {
    #[serde(rename = "type")]
    pub kind: AnnotationKind,
    pub by: String,                      // "Act XLII of 2019"
    #[serde(default)]
    pub by_date: Option<String>,         // ISO-8601 yyyy-mm-dd
    #[serde(default)]
    pub footnote_id: Option<String>,
    #[serde(default)]
    pub original: Option<String>,
    #[serde(default)]
    pub note: Option<String>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AnnotationKind {
    Insertion, Substitution, Omission, Renumbering, ProvisoAdded,
}
```

### Per-doc-type metadata overlays

The `metadata` field is `serde_json::Value` at the type level; structured loaders deserialize on demand:

| `doc_type` | Expected `metadata` keys |
|---|---|
| `constitution` | `{ article_no?, amendment_history?: [...] }` |
| `act` | `{ act_year?, enacted_by?, commencement_date?, repealed_by?: [...] }` |
| `rules` | `{ parent_act?, rule_no?, form_refs?: [...] }` |
| `sro` | `{ sro_number?, gazette_date?, issuing_authority?, amends?: [...] }` |
| `contract` | `{ party_a?, party_b?, effective_date?, clause_type? }` |
| `generic` | `{}` (no overlay) |

The same overlay applies at `DocIr.doc_metadata` for top-level fields (e.g. `act_year`).

### Validation issue (transient, not persisted in JSON)

```rust
pub struct ValidationIssue {
    pub node_id: String,
    pub severity: Severity,          // Error | Warning
    pub code: ValidationCode,        // enum: MissingTitle, CycleDetected, ...
    pub message: String,
}
```

---

## 2. On-disk JSON shape

File: `{app_data_dir}/ir_cache/<sha256>.json`. Direct serialization of `DocIr` via `serde_json` pretty-printed. Single file per source-content hash.

The format is identical to the in-memory `DocIr` plus an outer envelope wrapper for forward-compatibility:

```jsonc
{
  "envelope_version": 1,
  "doc_ir": {
    "doc_id": "abc123…",
    "doc_type": "act",
    "title": "Sindh Civil Servants Act, 1973",
    "source_pdf": "/home/user/Docs/sindh_csa_1973.pdf",
    "extracted_at": "2026-05-29T14:32:11Z",
    "llm_model": "mistral-7b-instruct-v0.3-q4_k_m",
    "schema_version": 1,
    "doc_metadata": { "act_year": 1973, "enacted_by": "Sindh Assembly" },
    "nodes": [ { ... }, ... ]
  }
}
```

Envelope version is separate from schema version: envelope bumps for breaking changes to the *file format* (e.g. swapping JSON for CBOR), schema bumps for changes to `DocIr` fields.

---

## 3. SQLite schema additions

Migration: `src-tauri/src/db/migrations/018_llm_ir_importer.sql`. Additive only (Constitution development constraint).

```sql
-- 018_llm_ir_importer.sql

-- Cache index: one row per cached IR file.
CREATE TABLE IF NOT EXISTS ir_cache_index (
    sha256                  TEXT PRIMARY KEY,             -- = DocIr.doc_id
    schema_version          INTEGER NOT NULL,
    doc_type                TEXT NOT NULL,
    nodes_total             INTEGER NOT NULL,
    nodes_ok                INTEGER NOT NULL,
    nodes_low_confidence    INTEGER NOT NULL,
    nodes_user_repaired     INTEGER NOT NULL DEFAULT 0,
    quality_score           REAL NOT NULL,                -- nodes_ok / nodes_total
    orphaned_at             INTEGER,                       -- unix ts; NULL = not orphaned
    created_at              INTEGER NOT NULL,
    updated_at              INTEGER NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_ir_cache_orphaned ON ir_cache_index(orphaned_at) WHERE orphaned_at IS NOT NULL;

-- Per-source provenance + repair counts (sources table is created by migration 001).
ALTER TABLE sources ADD COLUMN ir_cache_sha          TEXT;        -- FK → ir_cache_index.sha256
ALTER TABLE sources ADD COLUMN extraction_method     TEXT;        -- 'llm_ir' | 'rules_legacy' | 'constitution_ir' | 'unprocessable'
ALTER TABLE sources ADD COLUMN ir_quality_score      REAL;        -- snapshot at import time
ALTER TABLE sources ADD COLUMN nodes_low_confidence  INTEGER;     -- snapshot at import time

CREATE INDEX IF NOT EXISTS idx_sources_ir_cache_sha ON sources(ir_cache_sha);

-- Repair log: append-only audit of user repair saves.
CREATE TABLE IF NOT EXISTS ir_repair_log (
    id              INTEGER PRIMARY KEY AUTOINCREMENT,
    source_id       TEXT NOT NULL,                        -- FK → sources.id
    sha256          TEXT NOT NULL,                        -- FK → ir_cache_index.sha256
    node_id         TEXT NOT NULL,                        -- DocIr.nodes[].id
    action          TEXT NOT NULL,                        -- 'edit_text'|'edit_kind'|'add_sibling'|'delete'|'edit_annotation'
    before_text     TEXT,                                 -- nullable; for add_sibling
    after_text      TEXT,                                 -- nullable; for delete
    repaired_at     INTEGER NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_ir_repair_source ON ir_repair_log(source_id);
CREATE INDEX IF NOT EXISTS idx_ir_repair_sha    ON ir_repair_log(sha256);
```

Bumps `structured_importer.rs::STRUCTURED_IMPORTER_VERSION` does **not** apply here — this is a new pipeline. Add a parallel constant `pub const LLM_IR_IMPORTER_VERSION: i64 = 1;` in the new `doc_ir` module. Bumping it forces re-extraction of all `extraction_method='llm_ir'` rows on next launch (consumed by a new `enqueue_llm_ir_reextract` helper, mirroring the rule-based one).

---

## 4. Cache lifecycle states

```
        ┌─────────────────┐
        │ (no cache row)  │
        └────────┬────────┘
                 │ import succeeds
                 ▼
        ┌─────────────────┐
        │   active        │   orphaned_at IS NULL
        └────┬────────────┘
             │ source PDF removed from all watched folders
             ▼
        ┌─────────────────┐
        │   orphaned      │   orphaned_at = <ts>; JSON kept
        └────┬─────────┬──┘
             │         │ same sha re-appears within 7 days
             │         └──→ active (orphaned_at = NULL)
             │ orphaned_at < now − 7 days  (daily sweep)
             ▼
        ┌─────────────────┐
        │   deleted       │   row dropped, JSON unlinked
        └─────────────────┘
```

Sweep query:

```sql
SELECT sha256 FROM ir_cache_index
WHERE orphaned_at IS NOT NULL AND orphaned_at < strftime('%s','now','-7 days');
```

---

## 5. Node quality lifecycle

```
        ok ──────────── user edits ──────────────→ user_repaired
        │                                              │
        │ schema bump + LLM re-run                      │ (sticky — never overwritten by LLM)
        ▼                                              │
        ok / low_confidence                            │
                                                       │
                                       ┌───────────────┘
                                       ▼
                              user_repaired (preserved by id+text-match)
```

Rule (FR-018): when an LLM re-run completes, the persistence step iterates new nodes; if a `user_repaired` node with the same content match exists, the new node's `text`, `kind`, and `annotations` are overwritten by the user version and the flag stays `user_repaired`.

**Match key** (in evaluation order — first hit wins):

1. **Strict**: `(parent_chain, number, hash(text[:120]))` — `parent_chain` is the concrete `Vec<String>` of `node.number` values walked from root to this node's parent (e.g. `["III", "2"]` for a clause inside Part III Chapter 2). Identical match means same structural slot AND same content.
2. **Relaxed**: `(node.number, hash(text[:120]))` — when the LLM re-run reshuffles parents around the user-repaired node, fall back to text+number match anywhere in the doc.
3. **Text-only**: `hash(text[:120])` — last resort when both parent and number drift; treated as a soft match and logged for review.

`hash` is `blake3` truncated to 16 bytes. The 120-character window is the first 120 characters of `text` after Unicode NFC normalization + whitespace collapse. Test cases for all three tiers live in T031.

---

## 6. Validation contract

`validate_doc_ir(&DocIr) -> Vec<ValidationIssue>`. Issue codes (closed set; new codes require schema-version bump):

| Code | Severity | Meaning |
|---|---|---|
| `MissingRequiredField` | Error | Schema-required field absent |
| `InvalidEnum` | Error | `kind`/`annotation.type` not in allowed set |
| `OrphanParent` | Error | `parent_id` references nonexistent node |
| `CycleDetected` | Error | Parent chain forms a cycle |
| `DuplicateId` | Error | Two nodes share an `id` |
| `EmptyLeafText` | Error | Leaf node has empty `text` and is not an omission |
| `LowCoverage` | Error | Sum of `len(node.text)` after whitespace collapse < 90% of source PDF's non-whitespace character count (tokenizer-neutral; same measure as SC-002 / FR-007) |
| `NonMonotonicNumber` | Warning | Sibling numbering decreases unexpectedly |
| `UnresolvedFootnoteRef` | Error | `footnote_refs[]` points to no `kind=footnote` node |
| `AnnotationMissingBy` | Error | Annotation lacks `by` |
| `OmissionMissingMarker` | Error | Empty-text node lacks `annotations[].type='omission'` |
| `PageOutOfRange` | Warning | `page` exceeds source PDF page count |

Errors flag the node `low_confidence`; warnings are surfaced in the repair UI but do not flip the flag.
