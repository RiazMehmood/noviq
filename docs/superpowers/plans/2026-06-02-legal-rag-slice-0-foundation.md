# Legal RAG Slice 0 — Foundation Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Land Feature 006 on master, branch `007-legal-rag-pakistan`, amend the SDD constitution for the legal pivot, add migration 020 (additive legal-IR schema), and ship a fully-tested officer-grade citation formatter — the foundation Slice A will build on.

**Architecture:** One PR-sized slice. No new pipelines, no LLM calls, no UI. Pure foundation: constitution + migration + citation library + a JSON-to-legal-IR migration helper for existing sidecars. Engine reuse from 006 (`mistral_builder`, `progress_store`, llguidance) lands as part of the 006 merge; this slice does not move those files, only verifies they exist on master.

**Tech Stack:** Rust 1.87 + sqlx 0.8 (SQLite); existing 006 engine pieces inherited via merge; serde for citation types; chrono for amendment dates.

**Design spec:** `docs/superpowers/specs/2026-06-02-legal-rag-pakistan-design.md`

---

## File Structure

**New files:**
- `src-tauri/src/db/migrations/020_legal_ir.sql` — additive schema (graph_nodes columns + amendment_events + graph_edges + procedures + procedure_steps)
- `src-tauri/src/services/citation/mod.rs` — module entry, public API
- `src-tauri/src/services/citation/types.rs` — `LegalCitation`, `PathSegment`, `AmendmentNote` types
- `src-tauri/src/services/citation/formatter.rs` — officer / short / inline format functions
- `src-tauri/tests/citation_golden_test.rs` — 12 golden citation cases
- `src-tauri/tests/migration_020_test.rs` — runs migration, asserts new columns/tables exist
- `tools/migrate_generic_to_legal_ir.py` — one-shot converter for old `constitution_ir.json`-style sidecars
- `specs/007-legal-rag-pakistan/spec.md` — feature spec scaffold (links to design doc)
- `specs/007-legal-rag-pakistan/plan.md` — slice index (links to per-slice plan docs)
- `specs/007-legal-rag-pakistan/tasks.md` — Slice 0 task checklist (mirrors this plan)

**Modified files:**
- `src-tauri/src/db/schema.rs:15` — bump `TARGET_SCHEMA_VERSION` 15 → 16
- `src-tauri/src/services/mod.rs` — add `pub mod citation;`
- `src-tauri/tests/schema_wipe_test.rs` — update expected target version to 16
- `.specify/memory/constitution.md` — five new principles per design §2
- `/home/riaz/.claude/projects/-media-riaz-New-Volume-echo-AI/memory/project_echoai_domain_scope.md` — prepend SUPERSEDED header
- `/home/riaz/.claude/projects/-media-riaz-New-Volume-echo-AI/memory/feedback_llm_ir_only_pdf_scanned_images.md` — prepend SUPERSEDED header (generic-domain clause only)
- `/home/riaz/.claude/projects/-media-riaz-New-Volume-echo-AI/memory/MEMORY.md` — annotate superseded entries

---

## Task 1: Land Feature 006 on master

**Files:**
- Modify: working tree on `006-llm-pdf-importer` (currently has uncommitted retrieval-knob + progress_store batch)
- Touch: `master` via merge

- [ ] **Step 1: Verify working tree state on 006 branch**

Run:
```bash
cd "/media/riaz/New Volume/echo-AI" && git status --short && git log --oneline -5
```
Expected: current branch is `006-llm-pdf-importer`; modified files include `CHANGELOG_DEV.md`, `src-tauri/src/db/schema.rs`, `src-tauri/src/services/doc_ir/*`, `src-tauri/src/services/indexer.rs`, `src-tauri/src/services/llm.rs`, `src-tauri/src/services/query_router.rs`, `src-tauri/tests/schema_wipe_test.rs`; untracked includes `019_doc_ir_progress.sql` and `progress_store.rs`. Last commit is `004f9735 feat(006): LLM PDF→JSON importer — Phase 3 Slice A+B through round 8`.

- [ ] **Step 2: Append Slice 0 entry to CHANGELOG_DEV.md**

Open `CHANGELOG_DEV.md` and prepend at the top:

```markdown
## 2026-06-02 — Feature 006 final pre-merge batch

**Symptom/why:** 006 (LLM PDF→JSON importer) had partial test coverage but proven end-to-end on Mistral 7B for Sindh constitution (full-doc verification still pending). Treating as "complete enough" so Feature 007 (legal RAG pivot) can branch off a clean master.

**Fix:** Commit retrieval-knob batch + progress_store + 019 migration + schema bump 14→15. Merge `006-llm-pdf-importer` into `master`. Full end-to-end run on the 274-chunk PDF stays open as follow-up.

**Cross-cutting:** Master now carries `services/doc_ir/{mistral_builder,progress_store,prompts,types}.rs`, llguidance grammar binding on `llm.rs`, `indexer::do_index` Mistral dispatch path, and `query_router` context-budget knob. Feature 007 will reuse all of these.

**Follow-ups:** Full 274-chunk Mistral run for verification; classifier prompt rewrite (Slice C of 006) deferred or absorbed into 007 Slice A.
```

- [ ] **Step 3: Stage and commit 006 final batch**

Run:
```bash
cd "/media/riaz/New Volume/echo-AI" && git add CHANGELOG_DEV.md src-tauri/src/db/schema.rs src-tauri/src/db/migrations/019_doc_ir_progress.sql src-tauri/src/services/doc_ir/mistral_builder.rs src-tauri/src/services/doc_ir/mod.rs src-tauri/src/services/doc_ir/prompts.rs src-tauri/src/services/doc_ir/types.rs src-tauri/src/services/doc_ir/progress_store.rs src-tauri/src/services/indexer.rs src-tauri/src/services/llm.rs src-tauri/src/services/query_router.rs src-tauri/tests/schema_wipe_test.rs && git commit -m "$(cat <<'EOF'
feat(006): retrieval-knob + progress_store + migration 019 — pre-merge batch

Final 006 batch before merging to master ahead of Feature 007 legal pivot.

Co-Authored-By: Claude Opus 4.7 <noreply@anthropic.com>
EOF
)"
```
Expected: one new commit on `006-llm-pdf-importer`.

- [ ] **Step 4: Run the test suite that exists today**

Run:
```bash
cd "/media/riaz/New Volume/echo-AI" && cargo test --manifest-path src-tauri/Cargo.toml --lib 2>&1 | tail -30
```
Expected: at least the 47 doc_ir lib tests + 40 integration tests pass (per memory: `Tests: 47/47 doc_ir lib + 3 importer + 40 integration GREEN`). Two pre-existing failures are tolerated (`doc_profiles::manual::detection_matches_manual_corpus`, `query_router::context_builder_token_budget_drops_large_chunk`). If any *new* failure appears, stop and investigate — do not merge.

- [ ] **Step 5: Merge 006 into master**

Run:
```bash
cd "/media/riaz/New Volume/echo-AI" && git checkout master && git merge --no-ff 006-llm-pdf-importer -m "$(cat <<'EOF'
Merge branch '006-llm-pdf-importer' — LLM PDF→JSON importer landed

Feature 006 brings Mistral 7B + llguidance grammar IR build, progress_store
resume, and the per-chunk classifier scaffold. Full 274-chunk verification
remains open as follow-up; engine is stable enough for Feature 007 (legal
pivot) to branch off master and reuse.

Co-Authored-By: Claude Opus 4.7 <noreply@anthropic.com>
EOF
)"
```
Expected: clean merge commit on `master`. No conflicts (006 is a strict-ahead branch).

- [ ] **Step 6: Push master to origin**

Run:
```bash
cd "/media/riaz/New Volume/echo-AI" && git push origin master
```
Expected: push succeeds.

---

## Task 2: Branch `007-legal-rag-pakistan` off master

**Files:** branch only.

- [ ] **Step 1: Create and check out the 007 branch**

Run:
```bash
cd "/media/riaz/New Volume/echo-AI" && git checkout -b 007-legal-rag-pakistan && git log --oneline -3
```
Expected: `On branch 007-legal-rag-pakistan` with the 006 merge commit at HEAD.

- [ ] **Step 2: Cherry-pick the existing 007 design doc commit forward**

The design doc was committed on `006-llm-pdf-importer` as `1a10dc33`. The 006 merge in Task 1 carries it into master and therefore into 007. Verify:

Run:
```bash
cd "/media/riaz/New Volume/echo-AI" && ls docs/superpowers/specs/2026-06-02-legal-rag-pakistan-design.md
```
Expected: file exists. If missing, run `git cherry-pick 1a10dc33`.

---

## Task 3: Amend the SDD constitution

**Files:**
- Modify: `.specify/memory/constitution.md`

- [ ] **Step 1: Read the existing constitution**

Run:
```bash
cd "/media/riaz/New Volume/echo-AI" && wc -l .specify/memory/constitution.md && head -40 .specify/memory/constitution.md
```
Expected: file exists; note current section headings so the amendment slots in without breaking structure.

- [ ] **Step 2: Add a new top-level section "Domain & user principles (added 2026-06-02 — legal pivot)"**

Insert this block immediately after the existing "Core policies" section (or at the top of the principles list if that section doesn't exist):

```markdown
## Domain & user principles (added 2026-06-02 — legal pivot)

These principles take precedence over older domain-neutral language elsewhere
in this repo. Supersedes: `project_echoai_domain_scope.md`, and the
generic-domain clause of `feedback_llm_ir_only_pdf_scanned_images.md`.

### P1. Domain
EchoAI is a Pakistan government legal RAG system. Primary artifact types:
Constitution, Acts, Ordinances, Rules, Regulations, Policies, and their
cross-references. Out of scope: case law / judgments, foreign jurisdictions,
private legal practice.

### P2. Primary user
Government officer / policy analyst. All defaults (citation format, answer
length, export shape, multi-turn behavior) optimize for this persona. Citizen
and lawyer personas are supported as secondary modes, not first-class defaults.

### P3. Citation
Every answer carries a precise legal citation: instrument name + year + full
hierarchical path (Chapter → Section → Sub-section → Clause → Sub-clause) +
amendment status as of today. No paraphrase without citation.

### P4. Temporal model
Latest consolidated text is the primary truth. Each section carries its
amendment chain (who inserted/substituted/repealed it, by which Act, when).
Point-in-time reconstruction is out of scope for v1.

### P5. Cross-references
Acts ↔ Rules ↔ Policies form a first-class graph, not an afterthought.
"Rules under Section X of Act Y" and "Policies citing Section X" are core
retrieval surfaces.
```

- [ ] **Step 3: Commit**

Run:
```bash
cd "/media/riaz/New Volume/echo-AI" && git add .specify/memory/constitution.md && git commit -m "$(cat <<'EOF'
constitution: add domain + user + citation + temporal + xref principles

Hard pivot to Pakistan government legal RAG. Supersedes the
domain-neutral memos in the user memory tree (see Slice 0 Task 4).

Co-Authored-By: Claude Opus 4.7 <noreply@anthropic.com>
EOF
)"
```
Expected: one commit on `007-legal-rag-pakistan`.

---

## Task 4: Mark superseded memos

**Files:**
- Modify: `/home/riaz/.claude/projects/-media-riaz-New-Volume-echo-AI/memory/project_echoai_domain_scope.md`
- Modify: `/home/riaz/.claude/projects/-media-riaz-New-Volume-echo-AI/memory/feedback_llm_ir_only_pdf_scanned_images.md`
- Modify: `/home/riaz/.claude/projects/-media-riaz-New-Volume-echo-AI/memory/MEMORY.md`

These files live outside the repo (user memory tree). They are not git-tracked.

- [ ] **Step 1: Prepend SUPERSEDED header to `project_echoai_domain_scope.md`**

Read the file first (to preserve frontmatter), then prepend this immediately after the frontmatter block:

```markdown
> **⚠ SUPERSEDED — 2026-06-02.** EchoAI is now a Pakistan government legal
> RAG product. See `.specify/memory/constitution.md` §P1 (Domain principle).
> The "globally applicable, do not narrow to legal" guidance below is no
> longer in force.
```

- [ ] **Step 2: Prepend partial-supersede header to `feedback_llm_ir_only_pdf_scanned_images.md`**

The PDF/scanned/image scope rule is still good. Only the "do not narrow to legal" intent is retired. Prepend:

```markdown
> **⚠ PARTIALLY SUPERSEDED — 2026-06-02.** The PDF / scanned / image scope
> rule below is still in force. The implicit "domain stays generic" framing
> is superseded by `.specify/memory/constitution.md` §P1 (Domain principle).
```

- [ ] **Step 3: Update MEMORY.md index entries**

Find the two lines referencing these files and append ` — **SUPERSEDED 2026-06-02 by constitution §P1**` (full or partial as appropriate) to each.

- [ ] **Step 4: No commit (memory tree is not git-tracked)**

Verify with:
```bash
cd "/media/riaz/New Volume/echo-AI" && git status
```
Expected: no changes shown (memory tree is outside the repo).

---

## Task 5: Migration 020 — additive legal-IR schema

**Files:**
- Create: `src-tauri/src/db/migrations/020_legal_ir.sql`

- [ ] **Step 1: Write the migration SQL**

Create `src-tauri/src/db/migrations/020_legal_ir.sql` with this exact content:

```sql
-- Migration 020 — Legal IR foundation (Feature 007 Slice 0)
--
-- Strictly additive. Generic graph_nodes rows from Feature 006 stay valid:
-- the new columns are nullable / defaulted. Schema bumps 15 → 16.

-- 0. Tag each document with its current index tier (fast = default
--    pipeline; deep = Mistral legal-IR pipeline). Sticky: once deep,
--    re-uploads stay deep unless the officer explicitly downgrades.
ALTER TABLE documents ADD COLUMN index_tier TEXT NOT NULL DEFAULT 'fast'
  CHECK(index_tier IN ('fast','deep'));

-- 1. Extend graph_nodes with legal-domain columns
ALTER TABLE graph_nodes ADD COLUMN instrument_id TEXT;
ALTER TABLE graph_nodes ADD COLUMN legal_path TEXT;
ALTER TABLE graph_nodes ADD COLUMN in_force INTEGER NOT NULL DEFAULT 1;
ALTER TABLE graph_nodes ADD COLUMN source_page INTEGER;

CREATE INDEX IF NOT EXISTS idx_graph_nodes_instrument
  ON graph_nodes(instrument_id);
CREATE INDEX IF NOT EXISTS idx_graph_nodes_legal_path
  ON graph_nodes(instrument_id, legal_path);

-- 2. Amendment events — one row per insert / substitute / repeal / omit
CREATE TABLE IF NOT EXISTS amendment_events (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  node_id TEXT NOT NULL,
  kind TEXT NOT NULL CHECK(kind IN ('inserted','substituted','repealed','omitted')),
  by_instrument TEXT NOT NULL,
  effective_date TEXT,
  source_page INTEGER,
  raw_marker TEXT,
  FOREIGN KEY (node_id) REFERENCES graph_nodes(id) ON DELETE CASCADE
);
CREATE INDEX IF NOT EXISTS idx_amendment_events_node
  ON amendment_events(node_id);

-- 3. Typed cross-reference edges
CREATE TABLE IF NOT EXISTS graph_edges (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  src_node_id TEXT NOT NULL,
  dst_node_id TEXT NOT NULL,
  edge_kind TEXT NOT NULL CHECK(edge_kind IN ('cites','implements','under','amends')),
  confidence REAL NOT NULL DEFAULT 1.0,
  source_phrase TEXT,
  FOREIGN KEY (src_node_id) REFERENCES graph_nodes(id) ON DELETE CASCADE,
  FOREIGN KEY (dst_node_id) REFERENCES graph_nodes(id) ON DELETE CASCADE
);
CREATE INDEX IF NOT EXISTS idx_graph_edges_src
  ON graph_edges(src_node_id, edge_kind);
CREATE INDEX IF NOT EXISTS idx_graph_edges_dst
  ON graph_edges(dst_node_id, edge_kind);

-- 4. Procedure layer
CREATE TABLE IF NOT EXISTS procedures (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  anchor_node_id TEXT NOT NULL,
  process_name TEXT NOT NULL,
  description TEXT,
  FOREIGN KEY (anchor_node_id) REFERENCES graph_nodes(id) ON DELETE CASCADE
);

CREATE TABLE IF NOT EXISTS procedure_steps (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  procedure_id INTEGER NOT NULL,
  step_order INTEGER NOT NULL,
  actor TEXT NOT NULL,
  action TEXT NOT NULL,
  condition TEXT,
  branch_of_step_id INTEGER,
  timeline TEXT,
  source_node_id TEXT NOT NULL,
  FOREIGN KEY (procedure_id) REFERENCES procedures(id) ON DELETE CASCADE,
  FOREIGN KEY (branch_of_step_id) REFERENCES procedure_steps(id),
  FOREIGN KEY (source_node_id) REFERENCES graph_nodes(id)
);
CREATE INDEX IF NOT EXISTS idx_procedure_steps_proc
  ON procedure_steps(procedure_id, step_order);
```

- [ ] **Step 2: Verify the migration loader picks it up automatically**

EchoAI's migration loader reads all files matching `src-tauri/src/db/migrations/*.sql` in lexical order. Verify by listing:

```bash
cd "/media/riaz/New Volume/echo-AI" && ls src-tauri/src/db/migrations/ | tail -5
```
Expected output ends with `019_doc_ir_progress.sql` then `020_legal_ir.sql`.

- [ ] **Step 3: Commit**

```bash
cd "/media/riaz/New Volume/echo-AI" && git add src-tauri/src/db/migrations/020_legal_ir.sql && git commit -m "$(cat <<'EOF'
db(007): migration 020 — additive legal-IR schema

Adds graph_nodes columns (instrument_id, legal_path, in_force,
source_page), amendment_events, graph_edges, procedures,
procedure_steps. Strictly additive — Feature 006 generic rows
stay valid.

Co-Authored-By: Claude Opus 4.7 <noreply@anthropic.com>
EOF
)"
```

---

## Task 6: Bump TARGET_SCHEMA_VERSION 15 → 16

**Files:**
- Modify: `src-tauri/src/db/schema.rs:15`

- [ ] **Step 1: Edit the constant**

In `src-tauri/src/db/schema.rs`, change:

```rust
pub const TARGET_SCHEMA_VERSION: u32 = 15;
```
to:
```rust
pub const TARGET_SCHEMA_VERSION: u32 = 16;
```

- [ ] **Step 2: Update the schema_wipe_test expected value**

Open `src-tauri/tests/schema_wipe_test.rs`, find any assertion of the form `assert_eq!(... 15)` or `TARGET_SCHEMA_VERSION` reference checking 15, change to 16.

Run:
```bash
cd "/media/riaz/New Volume/echo-AI" && grep -n "15" src-tauri/tests/schema_wipe_test.rs
```
Expected: shows the assertion line(s); update them to 16.

- [ ] **Step 3: Run the schema wipe test**

Run:
```bash
cd "/media/riaz/New Volume/echo-AI" && cargo test --manifest-path src-tauri/Cargo.toml --test schema_wipe_test 2>&1 | tail -10
```
Expected: all tests pass. The wipe-on-mismatch path runs (current `echo.db` reports 15 < 16 target → wipes), then migrations 001-020 apply cleanly.

- [ ] **Step 4: Commit**

```bash
cd "/media/riaz/New Volume/echo-AI" && git add src-tauri/src/db/schema.rs src-tauri/tests/schema_wipe_test.rs && git commit -m "$(cat <<'EOF'
db(007): bump TARGET_SCHEMA_VERSION 15 → 16 for migration 020

Co-Authored-By: Claude Opus 4.7 <noreply@anthropic.com>
EOF
)"
```

---

## Task 7: Migration 020 integration test

**Files:**
- Create: `src-tauri/tests/migration_020_test.rs`

- [ ] **Step 1: Write the failing integration test**

Create `src-tauri/tests/migration_020_test.rs`:

```rust
//! Migration 020 — assert legal-IR columns and tables are present after init.

use echo_ai_lib::db::schema::init_db;
use echo_ai_lib::services::hardware_detector::HardwareTier;
use sqlx::Row;
use tempfile::TempDir;

#[tokio::test]
async fn migration_020_adds_legal_columns_and_tables() {
    let tmp = TempDir::new().unwrap();
    let db_path = tmp.path().join("echo.db");
    let db_path_str = db_path.to_str().unwrap();

    let pool = init_db(db_path_str, &HardwareTier::Cpu)
        .await
        .expect("init_db should succeed");

    // 1. graph_nodes has the new legal columns
    let cols: Vec<(String,)> = sqlx::query_as(
        "SELECT name FROM pragma_table_info('graph_nodes') \
         WHERE name IN ('instrument_id','legal_path','in_force','source_page')"
    )
    .fetch_all(&pool)
    .await
    .unwrap();
    assert_eq!(cols.len(), 4, "expected 4 new legal columns on graph_nodes");

    // 1b. documents has index_tier column with correct default
    let tier_col: Option<(String, String, i64, Option<String>)> = sqlx::query_as(
        "SELECT name, type, \"notnull\", dflt_value FROM pragma_table_info('documents') \
         WHERE name='index_tier'"
    )
    .fetch_optional(&pool)
    .await
    .unwrap();
    let (name, ty, notnull, default) = tier_col.expect("documents.index_tier missing");
    assert_eq!(name, "index_tier");
    assert_eq!(ty.to_ascii_uppercase(), "TEXT");
    assert_eq!(notnull, 1, "index_tier should be NOT NULL");
    assert_eq!(default.as_deref(), Some("'fast'"), "default should be 'fast'");

    // 1c. CHECK constraint rejects bad tier values
    let bad_tier = sqlx::query(
        "INSERT INTO documents (id, path, index_tier) VALUES ('d-bad', '/tmp/x.pdf', 'bogus')"
    )
    .execute(&pool)
    .await;
    assert!(bad_tier.is_err(), "CHECK constraint should reject bogus index_tier");

    // 2. Each new table exists
    for table in ["amendment_events", "graph_edges", "procedures", "procedure_steps"] {
        let row: Option<(String,)> = sqlx::query_as(
            "SELECT name FROM sqlite_master WHERE type='table' AND name=?1"
        )
        .bind(table)
        .fetch_optional(&pool)
        .await
        .unwrap();
        assert!(row.is_some(), "table {table} missing after migration 020");
    }

    // 3. CHECK constraints reject bad values
    let bad_amendment = sqlx::query(
        "INSERT INTO amendment_events (node_id, kind, by_instrument) \
         VALUES ('n1', 'bogus', 'Act-1-of-2020')"
    )
    .execute(&pool)
    .await;
    assert!(bad_amendment.is_err(), "CHECK constraint should reject bogus kind");

    let bad_edge = sqlx::query(
        "INSERT INTO graph_edges (src_node_id, dst_node_id, edge_kind) \
         VALUES ('n1', 'n2', 'bogus')"
    )
    .execute(&pool)
    .await;
    assert!(bad_edge.is_err(), "CHECK constraint should reject bogus edge_kind");
}
```

- [ ] **Step 2: Run it — expect a compilation or behavior failure if migration didn't land**

Run:
```bash
cd "/media/riaz/New Volume/echo-AI" && cargo test --manifest-path src-tauri/Cargo.toml --test migration_020_test 2>&1 | tail -15
```
Expected: PASS (Task 5 already created the SQL file; this test simply verifies it).

If FAIL: check that the migration loader picks up `020_legal_ir.sql` (it should — naming convention matches existing migrations).

- [ ] **Step 3: Commit**

```bash
cd "/media/riaz/New Volume/echo-AI" && git add src-tauri/tests/migration_020_test.rs && git commit -m "$(cat <<'EOF'
test(007): integration test for migration 020

Asserts graph_nodes columns + 4 new tables + CHECK constraints.

Co-Authored-By: Claude Opus 4.7 <noreply@anthropic.com>
EOF
)"
```

---

## Task 8: Citation module — types

**Files:**
- Create: `src-tauri/src/services/citation/mod.rs`
- Create: `src-tauri/src/services/citation/types.rs`
- Modify: `src-tauri/src/services/mod.rs`

- [ ] **Step 1: Add module declaration**

Open `src-tauri/src/services/mod.rs` and add (alphabetically, before whatever follows `c`):

```rust
pub mod citation;
```

- [ ] **Step 2: Create `src-tauri/src/services/citation/mod.rs`**

```rust
//! Officer-grade legal citation rendering.
//!
//! Single source of truth for how a citation is formatted anywhere in the
//! app — chat answers, "see also" lists, exports, copy-as-note button.
//! Every legal citation in the UI flows through one of the format_* methods
//! on [`LegalCitation`].

pub mod formatter;
pub mod types;

pub use types::{AmendmentNote, LegalCitation, PathSegment};
```

- [ ] **Step 3: Create `src-tauri/src/services/citation/types.rs`**

```rust
//! Citation data types. Pure data; no formatting logic (that lives in
//! `formatter.rs`).

use serde::{Deserialize, Serialize};

/// One legal citation, fully resolved.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LegalCitation {
    /// Full title of the instrument, e.g. "The Right of Access to
    /// Information Act, 2017".
    pub instrument_title: String,

    /// Official short identifier if present, e.g. "Act No. XXXIV of 2017".
    /// `None` for the Constitution and for instruments without numbering.
    pub instrument_id: Option<String>,

    /// Year of enactment, if known.
    pub year: Option<u32>,

    /// Hierarchical path from root toward the cited node, in order.
    /// Empty vec = citation of the instrument as a whole.
    pub path: Vec<PathSegment>,

    /// If the cited node has been amended, the latest amendment note.
    pub amendment: Option<AmendmentNote>,
}

/// One segment in the path to a cited node.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum PathSegment {
    Preamble,
    Chapter(String),
    Part(String),
    Section(String),
    Article(String),
    SubSection(String),
    Clause(String),
    SubClause(String),
    Proviso(String),
    Explanation(String),
    Illustration(String),
    Schedule(String),
    ScheduleItem(String),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AmendmentNote {
    pub kind: AmendmentKind,
    pub by_instrument: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AmendmentKind {
    Inserted,
    Substituted,
    Repealed,
    Omitted,
}

impl AmendmentKind {
    pub fn as_verb_past(self) -> &'static str {
        match self {
            AmendmentKind::Inserted => "inserted",
            AmendmentKind::Substituted => "substituted",
            AmendmentKind::Repealed => "repealed",
            AmendmentKind::Omitted => "omitted",
        }
    }
}
```

- [ ] **Step 4: Verify it compiles**

Run:
```bash
cd "/media/riaz/New Volume/echo-AI" && cargo build --manifest-path src-tauri/Cargo.toml --lib 2>&1 | tail -5
```
Expected: build succeeds.

- [ ] **Step 5: Commit**

```bash
cd "/media/riaz/New Volume/echo-AI" && git add src-tauri/src/services/mod.rs src-tauri/src/services/citation/ && git commit -m "$(cat <<'EOF'
feat(007): citation module skeleton with data types

Pure data: LegalCitation, PathSegment, AmendmentNote, AmendmentKind.
Formatter logic lands in the next task.

Co-Authored-By: Claude Opus 4.7 <noreply@anthropic.com>
EOF
)"
```

---

## Task 9: Citation formatter — 12 golden tests (failing)

**Files:**
- Create: `src-tauri/tests/citation_golden_test.rs`

- [ ] **Step 1: Write the failing golden tests**

Create `src-tauri/tests/citation_golden_test.rs`:

```rust
//! Golden tests for the officer-grade citation formatter.
//!
//! Each test fixes one (input → expected formatted string) pair. The
//! formatter implementation in `services::citation::formatter` must produce
//! the exact string shown.

use echo_ai_lib::services::citation::{
    types::AmendmentKind, AmendmentNote, LegalCitation, PathSegment,
};

fn act_rti() -> LegalCitation {
    LegalCitation {
        instrument_title: "The Right of Access to Information Act, 2017".into(),
        instrument_id: Some("Act No. XXXIV of 2017".into()),
        year: Some(2017),
        path: vec![],
        amendment: None,
    }
}

#[test]
fn case_01_constitution_article() {
    let c = LegalCitation {
        instrument_title: "The Constitution of the Islamic Republic of Pakistan, 1973".into(),
        instrument_id: None,
        year: Some(1973),
        path: vec![PathSegment::Article("25".into())],
        amendment: None,
    };
    assert_eq!(
        c.format_officer(),
        "The Constitution of the Islamic Republic of Pakistan, 1973, Article 25",
    );
}

#[test]
fn case_02_act_section_only() {
    let mut c = act_rti();
    c.path = vec![PathSegment::Section("6".into())];
    assert_eq!(
        c.format_officer(),
        "The Right of Access to Information Act, 2017 (Act No. XXXIV of 2017), Section 6",
    );
}

#[test]
fn case_03_act_section_subsection() {
    let mut c = act_rti();
    c.path = vec![
        PathSegment::Section("6".into()),
        PathSegment::SubSection("2".into()),
    ];
    assert_eq!(
        c.format_officer(),
        "The Right of Access to Information Act, 2017 (Act No. XXXIV of 2017), Section 6(2)",
    );
}

#[test]
fn case_04_act_section_subsection_clause() {
    let mut c = act_rti();
    c.path = vec![
        PathSegment::Section("6".into()),
        PathSegment::SubSection("2".into()),
        PathSegment::Clause("a".into()),
    ];
    assert_eq!(
        c.format_officer(),
        "The Right of Access to Information Act, 2017 (Act No. XXXIV of 2017), Section 6(2)(a)",
    );
}

#[test]
fn case_05_act_full_path_to_subclause() {
    let mut c = act_rti();
    c.path = vec![
        PathSegment::Section("6".into()),
        PathSegment::SubSection("2".into()),
        PathSegment::Clause("a".into()),
        PathSegment::SubClause("i".into()),
    ];
    assert_eq!(
        c.format_officer(),
        "The Right of Access to Information Act, 2017 (Act No. XXXIV of 2017), Section 6(2)(a)(i)",
    );
}

#[test]
fn case_06_amended_substituted() {
    let mut c = act_rti();
    c.path = vec![
        PathSegment::Section("6".into()),
        PathSegment::SubSection("2".into()),
        PathSegment::Clause("a".into()),
    ];
    c.amendment = Some(AmendmentNote {
        kind: AmendmentKind::Substituted,
        by_instrument: "Act XII of 2021".into(),
    });
    assert_eq!(
        c.format_officer(),
        "The Right of Access to Information Act, 2017 (Act No. XXXIV of 2017), Section 6(2)(a) — substituted by Act XII of 2021",
    );
}

#[test]
fn case_07_inserted_new_section() {
    let mut c = act_rti();
    c.path = vec![PathSegment::Section("6A".into())];
    c.amendment = Some(AmendmentNote {
        kind: AmendmentKind::Inserted,
        by_instrument: "Act XII of 2021".into(),
    });
    assert_eq!(
        c.format_officer(),
        "The Right of Access to Information Act, 2017 (Act No. XXXIV of 2017), Section 6A — inserted by Act XII of 2021",
    );
}

#[test]
fn case_08_repealed_section() {
    let mut c = act_rti();
    c.path = vec![PathSegment::Section("7".into())];
    c.amendment = Some(AmendmentNote {
        kind: AmendmentKind::Repealed,
        by_instrument: "Ordinance V of 2022".into(),
    });
    assert_eq!(
        c.format_officer(),
        "The Right of Access to Information Act, 2017 (Act No. XXXIV of 2017), Section 7 — repealed by Ordinance V of 2022",
    );
}

#[test]
fn case_09_rules_instrument() {
    let c = LegalCitation {
        instrument_title: "The Sindh Local Government Rules, 2018".into(),
        instrument_id: None,
        year: Some(2018),
        path: vec![
            PathSegment::Section("4".into()),
            PathSegment::SubSection("2".into()),
        ],
        amendment: None,
    };
    assert_eq!(
        c.format_officer(),
        "The Sindh Local Government Rules, 2018, Rule 4(2)",
    );
}

#[test]
fn case_10_chapter_only() {
    let mut c = act_rti();
    c.path = vec![PathSegment::Chapter("III".into())];
    assert_eq!(
        c.format_officer(),
        "The Right of Access to Information Act, 2017 (Act No. XXXIV of 2017), Chapter III",
    );
}

#[test]
fn case_11_schedule_item() {
    let mut c = act_rti();
    c.path = vec![
        PathSegment::Schedule("II".into()),
        PathSegment::ScheduleItem("3".into()),
    ];
    assert_eq!(
        c.format_officer(),
        "The Right of Access to Information Act, 2017 (Act No. XXXIV of 2017), Schedule II, Item 3",
    );
}

#[test]
fn case_12_short_form_inline() {
    let mut c = act_rti();
    c.path = vec![
        PathSegment::Section("6".into()),
        PathSegment::SubSection("2".into()),
        PathSegment::Clause("a".into()),
    ];
    assert_eq!(c.format_short(), "RTI Act 2017 §6(2)(a)");
}
```

- [ ] **Step 2: Run — expect FAIL because format_officer / format_short don't exist yet**

Run:
```bash
cd "/media/riaz/New Volume/echo-AI" && cargo test --manifest-path src-tauri/Cargo.toml --test citation_golden_test 2>&1 | tail -15
```
Expected: compilation FAIL with errors like "no method named `format_officer` found for struct `LegalCitation`".

This is the red phase. Do not commit yet.

---

## Task 10: Citation formatter implementation

**Files:**
- Create: `src-tauri/src/services/citation/formatter.rs`
- Modify: `src-tauri/src/services/citation/types.rs` (add `impl` blocks)

- [ ] **Step 1: Implement `format_officer` and `format_short` on `LegalCitation`**

Add to `src-tauri/src/services/citation/types.rs` (at the bottom of the file):

```rust
impl LegalCitation {
    /// Officer-grade citation — the long, fully-qualified form used in chat
    /// bubbles and exported notes.
    pub fn format_officer(&self) -> String {
        let mut s = String::new();
        s.push_str(&self.instrument_title);
        if let Some(id) = &self.instrument_id {
            s.push_str(" (");
            s.push_str(id);
            s.push(')');
        }
        s.push_str(&crate::services::citation::formatter::format_path_segment_list(
            &self.path,
            &self.instrument_title,
        ));
        if let Some(a) = &self.amendment {
            s.push_str(" — ");
            s.push_str(a.kind.as_verb_past());
            s.push_str(" by ");
            s.push_str(&a.by_instrument);
        }
        s
    }

    /// Short / inline form. Used for citation pills in dense UI.
    pub fn format_short(&self) -> String {
        let mut s = crate::services::citation::formatter::abbreviate_title(
            &self.instrument_title,
            self.year,
        );
        s.push(' ');
        s.push_str(&crate::services::citation::formatter::format_short_path(&self.path));
        s
    }
}
```

- [ ] **Step 2: Implement `formatter.rs`**

Create `src-tauri/src/services/citation/formatter.rs`:

```rust
//! Path and title formatting helpers. Pure functions; no I/O.

use super::types::PathSegment;

/// Joins all path segments into the comma-prefixed suffix of an officer-grade
/// citation. Returns "" for an empty path. The instrument_title is used to
/// pick between "Section" (Acts/Ordinances) and "Rule" (Rules instruments)
/// — Pakistan style.
pub fn format_path_segment_list(path: &[PathSegment], instrument_title: &str) -> String {
    if path.is_empty() {
        return String::new();
    }
    let is_rules = instrument_title_is_rules(instrument_title);
    let mut out = String::new();
    let mut first = true;
    let mut idx = 0;
    while idx < path.len() {
        let seg = &path[idx];
        match seg {
            PathSegment::Preamble => {
                push_comma(&mut out, &mut first);
                out.push_str("Preamble");
            }
            PathSegment::Chapter(n) => {
                push_comma(&mut out, &mut first);
                out.push_str("Chapter ");
                out.push_str(n);
            }
            PathSegment::Part(n) => {
                push_comma(&mut out, &mut first);
                out.push_str("Part ");
                out.push_str(n);
            }
            PathSegment::Article(n) => {
                push_comma(&mut out, &mut first);
                out.push_str("Article ");
                out.push_str(n);
            }
            PathSegment::Section(n) => {
                push_comma(&mut out, &mut first);
                if is_rules {
                    out.push_str("Rule ");
                } else {
                    out.push_str("Section ");
                }
                out.push_str(n);
                idx += 1;
                // Greedily absorb consecutive Sub-section / Clause / Sub-clause
                // segments into parenthesized suffixes.
                while idx < path.len() {
                    match &path[idx] {
                        PathSegment::SubSection(v)
                        | PathSegment::Clause(v)
                        | PathSegment::SubClause(v) => {
                            out.push('(');
                            out.push_str(v);
                            out.push(')');
                            idx += 1;
                        }
                        _ => break,
                    }
                }
                continue;
            }
            PathSegment::SubSection(v) => {
                // Bare sub-section without preceding Section — print as "(v)".
                push_comma(&mut out, &mut first);
                out.push('(');
                out.push_str(v);
                out.push(')');
            }
            PathSegment::Clause(v) => {
                push_comma(&mut out, &mut first);
                out.push_str("clause (");
                out.push_str(v);
                out.push(')');
            }
            PathSegment::SubClause(v) => {
                push_comma(&mut out, &mut first);
                out.push_str("sub-clause (");
                out.push_str(v);
                out.push(')');
            }
            PathSegment::Proviso(n) => {
                push_comma(&mut out, &mut first);
                out.push_str("Proviso ");
                out.push_str(n);
            }
            PathSegment::Explanation(n) => {
                push_comma(&mut out, &mut first);
                out.push_str("Explanation ");
                out.push_str(n);
            }
            PathSegment::Illustration(n) => {
                push_comma(&mut out, &mut first);
                out.push_str("Illustration ");
                out.push_str(n);
            }
            PathSegment::Schedule(n) => {
                push_comma(&mut out, &mut first);
                out.push_str("Schedule ");
                out.push_str(n);
            }
            PathSegment::ScheduleItem(n) => {
                push_comma(&mut out, &mut first);
                out.push_str("Item ");
                out.push_str(n);
            }
        }
        idx += 1;
    }
    out
}

fn push_comma(out: &mut String, first: &mut bool) {
    out.push_str(", ");
    *first = false;
}

fn instrument_title_is_rules(title: &str) -> bool {
    let t = title.to_ascii_lowercase();
    t.contains(" rules,") || t.ends_with(" rules") || t.contains(" rules ")
}

/// Build the short pill form like "RTI Act 2017 §6(2)(a)".
pub fn format_short_path(path: &[PathSegment]) -> String {
    let mut out = String::from("§");
    let mut idx = 0;
    while idx < path.len() {
        match &path[idx] {
            PathSegment::Section(n) | PathSegment::Article(n) => {
                out.push_str(n);
                idx += 1;
                while idx < path.len() {
                    match &path[idx] {
                        PathSegment::SubSection(v)
                        | PathSegment::Clause(v)
                        | PathSegment::SubClause(v) => {
                            out.push('(');
                            out.push_str(v);
                            out.push(')');
                            idx += 1;
                        }
                        _ => break,
                    }
                }
                return out;
            }
            _ => idx += 1,
        }
    }
    out
}

/// Heuristic abbreviation of instrument titles for the short form.
/// "The Right of Access to Information Act, 2017" → "RTI Act 2017".
pub fn abbreviate_title(title: &str, year: Option<u32>) -> String {
    // Strip leading "The ".
    let t = title.strip_prefix("The ").unwrap_or(title);
    // If it ends in ", YYYY", split.
    let (name_part, _) = match t.rsplit_once(", ") {
        Some((n, y)) if y.chars().all(|c| c.is_ascii_digit()) => (n, Some(y)),
        _ => (t, None),
    };
    let acronym = build_acronym(name_part);
    match year {
        Some(y) => format!("{acronym} {y}"),
        None => acronym,
    }
}

fn build_acronym(name_part: &str) -> String {
    // Title ends in something like "... Information Act" or "... Rules".
    // Strategy: take the first letter of each capitalized word in the
    // descriptive portion before "Act" / "Ordinance" / "Rules" / "Policy",
    // then append the kind word.
    let lower = name_part.to_ascii_lowercase();
    let kind_word = ["act", "ordinance", "rules", "policy", "regulations"]
        .iter()
        .find(|k| lower.contains(*k))
        .copied();
    let Some(kind) = kind_word else {
        return name_part.to_string();
    };
    let cap_kind = match kind {
        "act" => "Act",
        "ordinance" => "Ordinance",
        "rules" => "Rules",
        "policy" => "Policy",
        "regulations" => "Regulations",
        _ => kind,
    };
    let descriptive_end = lower.find(kind).unwrap();
    let descriptive = &name_part[..descriptive_end];
    let mut acronym = String::new();
    for word in descriptive.split_whitespace() {
        // Skip stopwords.
        let lw = word.to_ascii_lowercase();
        if matches!(lw.as_str(), "of" | "to" | "the" | "and" | "for" | "on" | "in") {
            continue;
        }
        if let Some(c) = word.chars().next() {
            acronym.push(c.to_ascii_uppercase());
        }
    }
    if acronym.is_empty() {
        cap_kind.to_string()
    } else {
        format!("{acronym} {cap_kind}")
    }
}
```

- [ ] **Step 3: Run the golden tests — expect PASS**

Run:
```bash
cd "/media/riaz/New Volume/echo-AI" && cargo test --manifest-path src-tauri/Cargo.toml --test citation_golden_test 2>&1 | tail -20
```
Expected: all 12 cases PASS.

If any case FAILS, the most likely culprits are:
- Stopword list in `build_acronym` (case 12: "Right of Access to Information Act" → "RAI Act"? expected "RTI Act"). If "RTI" isn't produced because the algorithm yields "RAI", patch the stopword logic or hardcode a small alias map at the top of `abbreviate_title`. Treat the goldens as the spec.
- Comma vs space spacing around path segments.
- Greedy Sub-section/Clause absorption order.

Fix until all 12 pass. Do not soften the goldens.

- [ ] **Step 4: Commit**

```bash
cd "/media/riaz/New Volume/echo-AI" && git add src-tauri/src/services/citation/ src-tauri/tests/citation_golden_test.rs && git commit -m "$(cat <<'EOF'
feat(007): officer-grade citation formatter — 12 golden tests GREEN

Single source of truth for citation rendering. format_officer() produces
the long form for chat/exports; format_short() produces the inline pill.

Co-Authored-By: Claude Opus 4.7 <noreply@anthropic.com>
EOF
)"
```

---

## Task 11: JSON sidecar migration tool — generic → legal IR

**Files:**
- Create: `tools/migrate_generic_to_legal_ir.py`

This tool converts an existing 006-era JSON sidecar (generic NodeKind: Section/Clause/Paragraph) into the v007 legal schema. Best-effort — unrecognized shapes are flagged, not silently dropped.

- [ ] **Step 1: Create the script**

Create `tools/migrate_generic_to_legal_ir.py`:

```python
#!/usr/bin/env python3
"""
Migrate a Feature 006 generic IR JSON sidecar into the Feature 007 legal IR
schema.

Usage:
    python3 tools/migrate_generic_to_legal_ir.py INPUT.json OUTPUT.json \\
        --instrument-title "The Constitution of the Islamic Republic of Pakistan, 1973" \\
        --instrument-id "" \\
        --year 1973

Mapping rules:
  Generic NodeKind            → Legal NodeKind
  "Section"                   → "Section" (or "Article" if instrument is Constitution)
  "Clause" (numbered (1),(2)) → "SubSection"
  "Clause" (lettered (a),(b)) → "Clause"
  "Paragraph"                 → kept as-is; flagged for review
  unknown                     → kept verbatim; printed to stderr

Output adds: instrument_id, legal_path (best-effort path inference),
in_force=True, source_page (preserved if present).

The script is intentionally simple and non-destructive — it does NOT mutate
the input file. Review the output before pointing the indexer at it.
"""
from __future__ import annotations

import argparse
import json
import re
import sys
from pathlib import Path

NUMBER_RE = re.compile(r"^\((\d+)\)$")
LETTER_RE = re.compile(r"^\(([a-z]+)\)$")
ROMAN_RE = re.compile(r"^\(([ivxlcdm]+)\)$", re.IGNORECASE)


def reclassify_kind(node: dict, is_constitution: bool) -> str:
    kind = node.get("kind") or node.get("node_kind") or "Paragraph"
    number = (node.get("number") or "").strip()

    if kind == "Section":
        return "Article" if is_constitution else "Section"
    if kind == "Clause":
        if NUMBER_RE.match(number):
            return "SubSection"
        if LETTER_RE.match(number):
            return "Clause"
        if ROMAN_RE.match(number):
            return "SubClause"
        return "Clause"
    if kind in {"Section", "Clause", "Paragraph", "Schedule", "Definition",
                "Chapter", "Part"}:
        return kind
    print(f"  ! unknown kind {kind!r} on node {node.get('id')}; kept verbatim",
          file=sys.stderr)
    return kind


def build_legal_path(nodes: list[dict], by_id: dict[str, dict]) -> None:
    """Walk parent_id chains and set legal_path on every node."""
    for node in nodes:
        segments: list[str] = []
        cur = node
        while cur is not None:
            kind = cur.get("legal_kind", cur.get("kind"))
            num = (cur.get("number") or "").strip().lstrip("(").rstrip(")")
            if kind == "Chapter" and num:
                segments.append(f"Ch-{num}")
            elif kind == "Part" and num:
                segments.append(f"Pt-{num}")
            elif kind in {"Section", "Article"} and num:
                segments.append(f"S-{num}")
            elif kind == "SubSection" and num:
                segments.append(f"({num})")
            elif kind == "Clause" and num:
                segments.append(f"({num})")
            elif kind == "SubClause" and num:
                segments.append(f"({num})")
            parent_id = cur.get("parent_id")
            cur = by_id.get(parent_id) if parent_id else None
        node["legal_path"] = "/".join(reversed(segments))


def main() -> int:
    p = argparse.ArgumentParser()
    p.add_argument("input", type=Path)
    p.add_argument("output", type=Path)
    p.add_argument("--instrument-title", required=True)
    p.add_argument("--instrument-id", default="")
    p.add_argument("--year", type=int, default=None)
    args = p.parse_args()

    data = json.loads(args.input.read_text(encoding="utf-8"))
    nodes = data.get("nodes") or data.get("graph_nodes") or []
    if not isinstance(nodes, list):
        print("error: input has no 'nodes' or 'graph_nodes' array", file=sys.stderr)
        return 2

    is_constitution = "constitution" in args.instrument_title.lower()
    instrument_id_str = args.instrument_id or args.instrument_title

    for n in nodes:
        n["legal_kind"] = reclassify_kind(n, is_constitution)
        n["instrument_id"] = instrument_id_str
        n["in_force"] = True

    by_id = {n["id"]: n for n in nodes if "id" in n}
    build_legal_path(nodes, by_id)

    out = {
        "schema_version": 16,
        "instrument_title": args.instrument_title,
        "instrument_id": instrument_id_str,
        "year": args.year,
        "nodes": nodes,
    }
    args.output.write_text(json.dumps(out, ensure_ascii=False, indent=2),
                           encoding="utf-8")
    print(f"wrote {args.output} ({len(nodes)} nodes)")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
```

- [ ] **Step 2: Smoke-test on a tiny synthetic input**

Run:
```bash
cd "/media/riaz/New Volume/echo-AI" && python3 -c "
import json, pathlib
pathlib.Path('/tmp/generic_ir_smoke.json').write_text(json.dumps({
    'nodes': [
        {'id':'n1','kind':'Section','number':'25','parent_id':None,'text':'Equality'},
        {'id':'n2','kind':'Clause','number':'(1)','parent_id':'n1','text':'sub'},
    ]
}, indent=2))
" && python3 tools/migrate_generic_to_legal_ir.py /tmp/generic_ir_smoke.json /tmp/legal_ir_smoke.json --instrument-title "The Constitution of the Islamic Republic of Pakistan, 1973" --year 1973 && cat /tmp/legal_ir_smoke.json
```
Expected: output JSON shows `legal_kind: "Article"` on the first node, `legal_kind: "SubSection"` on the second, and `legal_path` reading `"S-25"` and `"S-25/(1)"` respectively.

- [ ] **Step 3: Commit**

```bash
cd "/media/riaz/New Volume/echo-AI" && git add tools/migrate_generic_to_legal_ir.py && git commit -m "$(cat <<'EOF'
tools(007): generic→legal IR sidecar migration script

Best-effort converter for Feature 006-era JSON sidecars.
Reclassifies node kinds, infers legal_path, sets instrument_id.
Non-destructive — input is never mutated.

Co-Authored-By: Claude Opus 4.7 <noreply@anthropic.com>
EOF
)"
```

---

## Task 12: SpecKit Plus scaffolding for Feature 007

**Files:**
- Create: `specs/007-legal-rag-pakistan/spec.md`
- Create: `specs/007-legal-rag-pakistan/plan.md`
- Create: `specs/007-legal-rag-pakistan/tasks.md`

- [ ] **Step 1: Create `specs/007-legal-rag-pakistan/spec.md`**

```markdown
# Feature 007 — Legal RAG for Pakistan Government Documents

**Status:** In progress — Slice 0 implementing.
**Design doc:** [`docs/superpowers/specs/2026-06-02-legal-rag-pakistan-design.md`](../../docs/superpowers/specs/2026-06-02-legal-rag-pakistan-design.md)
**Branch:** `007-legal-rag-pakistan` (off `master`, after 006 merge)

## What we're building (one paragraph)

A Pakistan government legal RAG product for government officers / policy
analysts. Officers drop PDFs of Acts, Ordinances, Rules, Regulations, and
Policies. The system extracts a legal IR (Instrument → Chapter → Section →
Sub-section → Clause → Sub-clause + Procedures), an amendment chain, and a
typed cross-reference graph. A single chat box accepts natural language; an
intent classifier routes the query to one of eight handlers (Lookup,
ListChildren, Summarize, Difference, CrossCompare, ActionItems, Timeline,
Procedure). Every answer carries an officer-grade citation. Default model
tier is offline (Qwen-0.5B-FT / 3B / Mistral-7B); Groq/Cerebras are
eval-only.

## Out of scope (v1)

- Case law / judgments, foreign jurisdictions, private legal practice
- Point-in-time queries ("as of date X")
- Bilingual (Urdu) ingestion or retrieval
- Audit log / saved queries
- Editable workflow diagrams

## Acceptance gate (end of Slice D)

See design doc §6 "Final acceptance gate".
```

- [ ] **Step 2: Create `specs/007-legal-rag-pakistan/plan.md`**

```markdown
# Feature 007 Implementation Plan — Slice Index

Each slice ships as its own PR with its own eval gate. Per-slice plans live
in `docs/superpowers/plans/`.

| Slice | Title                                           | Plan doc                                                                 | Status      |
|-------|-------------------------------------------------|--------------------------------------------------------------------------|-------------|
| 0     | Foundation (this slice)                         | `docs/superpowers/plans/2026-06-02-legal-rag-slice-0-foundation.md`      | In progress |
| A     | Legal IR + amendment chain                      | written when Slice 0 is merged                                           | Pending     |
| B     | Intent router + 4 read-side intents             | written when Slice A is merged                                           | Pending     |
| C     | Cross-ref graph + 3 comparison intents          | written when Slice B is merged                                           | Pending     |
| D     | Procedure extraction + flow intent (v1 ship)    | written when Slice C is merged                                           | Pending     |

Why slice-by-slice planning: writing all four plans up front would lock in
file/API decisions that the slices themselves are supposed to discover.
Each slice's plan is written after its predecessor lands.
```

- [ ] **Step 3: Create `specs/007-legal-rag-pakistan/tasks.md` mirroring Slice 0**

```markdown
# Slice 0 — Foundation, Task Checklist

Source plan: `docs/superpowers/plans/2026-06-02-legal-rag-slice-0-foundation.md`

- [ ] Task 1: Land Feature 006 on master
- [ ] Task 2: Branch `007-legal-rag-pakistan` off master
- [ ] Task 3: Amend SDD constitution with P1–P5
- [ ] Task 4: Mark superseded memos (user memory tree, no git commit)
- [ ] Task 5: Migration 020 — additive legal-IR schema
- [ ] Task 6: Bump `TARGET_SCHEMA_VERSION` 15 → 16
- [ ] Task 7: Migration 020 integration test
- [ ] Task 8: Citation module skeleton + types
- [ ] Task 9: Citation 12 golden tests (failing)
- [ ] Task 10: Citation formatter implementation (golden tests green)
- [ ] Task 11: Generic → legal IR sidecar migration tool
- [ ] Task 12: SpecKit Plus scaffolding (this file + spec.md + plan.md)
- [ ] Task 13: Final verification + acceptance check
- [ ] Task 14: Slice 0 PHR + CHANGELOG_DEV.md entry
```

- [ ] **Step 4: Commit**

```bash
cd "/media/riaz/New Volume/echo-AI" && git add specs/007-legal-rag-pakistan/ && git commit -m "$(cat <<'EOF'
docs(007): SpecKit Plus scaffolding for Feature 007

spec.md (one-paragraph what + out-of-scope + gate pointer),
plan.md (slice index — per-slice plans written incrementally),
tasks.md (Slice 0 checklist mirroring the plan doc).

Co-Authored-By: Claude Opus 4.7 <noreply@anthropic.com>
EOF
)"
```

---

## Task 13: Final verification

**Files:** none (read-only checks).

- [ ] **Step 1: Full test suite must be green or only-pre-existing-failures**

Run:
```bash
cd "/media/riaz/New Volume/echo-AI" && cargo test --manifest-path src-tauri/Cargo.toml 2>&1 | tail -40
```
Expected: `migration_020_test` PASSES; `citation_golden_test` PASSES (12/12); `schema_wipe_test` PASSES. Pre-existing failures (`doc_profiles::manual::detection_matches_manual_corpus`, `query_router::context_builder_token_budget_drops_large_chunk`) may still fail — record but do not block.

- [ ] **Step 2: Manual DB init smoke test on a copy of `echo.db`**

Run:
```bash
cd "/media/riaz/New Volume/echo-AI" && cp echo.db /tmp/echo_slice0_smoke.db && sqlite3 /tmp/echo_slice0_smoke.db "SELECT value FROM db_config WHERE key='schema_version';" 2>/dev/null || echo "(no echo.db yet — skip)"
```
Expected: either the value `15` (DB pre-bump; will wipe on next app start) or `(no echo.db yet — skip)`. If `16` is already there, the app has booted at least once since the bump and migration 020 has run.

- [ ] **Step 3: Verify clean git state on `007-legal-rag-pakistan`**

Run:
```bash
cd "/media/riaz/New Volume/echo-AI" && git status && git log --oneline master..007-legal-rag-pakistan
```
Expected: working tree clean; ~9 commits ahead of master covering Tasks 3, 5, 6, 7, 8, 10, 11, 12, and the CHANGELOG/PHR entry from Task 14.

---

## Task 14: PHR + CHANGELOG entry

**Files:**
- Create: `history/prompts/legal-rag-pakistan/0001-slice-0-foundation.misc.prompt.md`
- Modify: `CHANGELOG_DEV.md` (prepend Slice 0 entry)

- [ ] **Step 1: Append CHANGELOG_DEV.md entry**

Prepend to `CHANGELOG_DEV.md`:

```markdown
## 2026-06-02 — Feature 007 Slice 0 — Foundation

**Symptom/why:** Hard pivot from generic-domain RAG to Pakistan government
legal RAG. Need a clean foundation before Slice A introduces the new IR
pipeline.

**Fix:**
- Merged Feature 006 to master (engine pieces — `mistral_builder`,
  `progress_store`, llguidance binding — now live on master).
- Branched `007-legal-rag-pakistan` off master.
- Constitution amended with P1–P5 (domain, user, citation, temporal, xref).
- Superseded the "domain is generic" memo and the generic-domain clause of
  the PDF/scanned/image scope memo.
- Migration 020 (additive): `graph_nodes` legal columns, `amendment_events`,
  `graph_edges`, `procedures`, `procedure_steps`. Schema 15 → 16.
- Citation module: `LegalCitation` + officer-grade and short formatters,
  12 golden tests green.
- Generic → legal IR sidecar migration tool for existing JSON assets.
- SpecKit Plus scaffolding for `specs/007-legal-rag-pakistan/`.

**Cross-cutting impact:** Every legal citation in 007 must flow through
`services::citation::LegalCitation::format_*`. The next slice (Slice A —
legal IR + amendment chain) lands on this foundation.

**Follow-ups:** 006's full 274-chunk Mistral run is still pending; not
blocking 007 but tracked. JSON sidecars built for the old generic schema
must be re-migrated with `tools/migrate_generic_to_legal_ir.py` before
Slice A's indexer will accept them.
```

- [ ] **Step 2: Create the PHR**

Read template:
```bash
cd "/media/riaz/New Volume/echo-AI" && ls .specify/templates/phr-template.prompt.md templates/phr-template.prompt.md 2>/dev/null | head -1
```
Use whichever path exists. Fill placeholders with: ID=0001; TITLE="Slice 0 foundation"; STAGE=misc; DATE_ISO=2026-06-02; SURFACE=agent; MODEL=claude-opus-4-7; FEATURE=legal-rag-pakistan; BRANCH=007-legal-rag-pakistan; USER=Riaz Mehmood; COMMAND="brainstorming → writing-plans → Slice 0 implementation"; LABELS=[feature-007, slice-0, foundation]; LINKS=[design doc, plan doc]; FILES_YAML=list of files touched in Slice 0; TESTS_YAML=`[citation_golden_test, migration_020_test, schema_wipe_test]`; PROMPT_TEXT="(see conversation transcript — brainstorming session that produced the design)"; RESPONSE_TEXT="Slice 0 plan executed task-by-task"; OUTCOME=success.

Save as `history/prompts/legal-rag-pakistan/0001-slice-0-foundation.misc.prompt.md`.

- [ ] **Step 3: Commit**

```bash
cd "/media/riaz/New Volume/echo-AI" && git add CHANGELOG_DEV.md history/prompts/legal-rag-pakistan/ && git commit -m "$(cat <<'EOF'
docs(007): Slice 0 PHR + CHANGELOG entry

Co-Authored-By: Claude Opus 4.7 <noreply@anthropic.com>
EOF
)"
```

- [ ] **Step 4: Push the branch**

```bash
cd "/media/riaz/New Volume/echo-AI" && git push -u origin 007-legal-rag-pakistan
```

---

## Slice 0 acceptance

A Slice 0 PR ships when ALL of the following hold:

1. `cargo test --test migration_020_test` is GREEN.
2. `cargo test --test citation_golden_test` is GREEN (12/12).
3. `cargo test --test schema_wipe_test` is GREEN at target version 16.
4. `.specify/memory/constitution.md` contains P1–P5.
5. `specs/007-legal-rag-pakistan/{spec,plan,tasks}.md` exist.
6. CHANGELOG_DEV.md has the Slice 0 entry at the top.
7. Branch is pushed to `origin/007-legal-rag-pakistan`.
8. No regressions in the previously-passing test set (pre-existing failures tolerated).
