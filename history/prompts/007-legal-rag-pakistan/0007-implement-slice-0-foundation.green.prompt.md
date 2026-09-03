---
id: "0007"
title: "implement slice 0 foundation"
stage: green
date: 2026-06-02
surface: agent
model: claude-opus-4-7
feature: 007-legal-rag-pakistan
branch: 007-legal-rag-pakistan
user: Riaz Mehmood
command: /sp.implement
labels: [feature-007, slice-0, foundation, green]
links:
  spec: specs/007-legal-rag-pakistan/spec.md
  ticket: ""
  adr: ""
  pr: ""
files:
  - path: CHANGELOG_DEV.md
    change: modified
  - path: src-tauri/src/db/schema.rs
    change: modified
  - path: src-tauri/src/db/migrations/020_legal_ir.sql
    change: created
  - path: src-tauri/src/services/mod.rs
    change: modified
  - path: src-tauri/src/services/citation/mod.rs
    change: created
  - path: src-tauri/src/services/citation/types.rs
    change: created
  - path: src-tauri/src/services/citation/formatter.rs
    change: created
  - path: src-tauri/tests/citation_golden_test.rs
    change: created
  - path: src-tauri/tests/migration_020_test.rs
    change: created
  - path: src-tauri/tests/schema_wipe_test.rs
    change: modified
  - path: tools/migrate_generic_to_legal_ir.py
    change: created
  - path: specs/007-legal-rag-pakistan/data-model.md
    change: modified
  - path: specs/007-legal-rag-pakistan/tasks.md
    change: modified
tests:
  - name: citation_golden_test
    status: PASS (12/12)
  - name: migration_020_test
    status: PASS
  - name: schema_wipe_test
    status: PASS (now at v16)
---

## Prompt

`/sp.implement` invoked (no user-input block). Goal per the SDD workflow: execute tasks.md task-by-task. This PHR captures execution of Phase 1 — Setup (Slice 0 Foundation), tasks T001–T014.

## Response snapshot

**Setup verification:**
- Prereq check: all artifacts present.
- Checklist: requirements.md 16/16 PASS.
- Ignore files: `.gitignore` present with all essential Rust+TS patterns.

**Working-tree handling (T001 prep):**
Found 12 uncommitted modifications carrying Feature 006 pre-merge work (CHANGELOG, schema.rs, doc_ir/*, indexer.rs, llm.rs, query_router.rs, schema_wipe_test, plus untracked migration 019 + progress_store.rs). User had previously authorized "treat 006 as finished". Committed the 006 batch as `feat(006): pre-merge batch — progress_store + migration 019 + indexer/llm/query_router` (commit `9e380f2d`) before starting 007 edits so Slice 0 diffs come in clean.

**T001–T013 executed sequentially or in batches:**

- T001 ✓ Branch verified at `007-legal-rag-pakistan`.
- T002 ✓ CHANGELOG_DEV.md Slice 0 entry prepended.
- T003 ✓ `src-tauri/src/db/migrations/020_legal_ir.sql` created with five sections (sources.index_tier; graph_nodes legal columns; amendment_events; legal_xrefs; procedures + procedure_steps). Schema_version row bumped to 16.
- T004 ✓ `TARGET_SCHEMA_VERSION` 15 → 16.
- T005 ✓ `schema_wipe_test.rs` expected value 15 → 16.
- T006 ✓ `migration_020_test.rs` created; GREEN.
- T007–T009 ✓ Citation module skeleton + types + mod registration.
- T010 ✓ `citation_golden_test.rs` with 12 golden cases.
- T011 ✓ `citation/formatter.rs` implemented; 12/12 goldens GREEN.
- T012 ✓ `tools/migrate_generic_to_legal_ir.py` created and smoke-tested.
- T013 ✓ SpecKit Plus scaffolding confirmed in place.
- T014 ✓ Slice 0 committed as `c62f9897`.

**Two corrections discovered during execution (both surface in tasks.md edits):**

1. **`documents` → `sources`.** The actual document table is `sources` (from migration 001); my spec and migration mistakenly used the noun "documents" as if it were a SQL identifier. Data-model E1 now says `sources` explicitly and notes the spec/SQL terminology gap. Migration ALTERs `sources`. Test queries `sources`.

2. **`graph_edges` → `legal_xrefs`.** A `graph_edges` table already exists from migration 014 (Feature 003 — constitution v1 hierarchical graph) with completely different shape (`from_node`/`to_node`/`edge_type`/`weight`). My migration would have collided. Renamed the new table to `legal_xrefs` and updated the indexes (`idx_legal_xrefs_src`/`_dst`). Data-model E5 renamed `GraphEdge` → `LegalXref` with an inline note.

Neither correction changes the feature's behaviour — they're SQL-level identifier choices. The intent router and procedure stitching layers (Slices C/D) will reference `legal_xrefs` instead of `graph_edges`.

**Test summary at end of Slice 0:**

| Test | Status |
|---|---|
| `citation_golden_test` | 12/12 PASS (officer-grade goldens covering Constitution Article, Act Section through Sub-clause, three amendment kinds, Rules-instrument, Chapter-only, Schedule Item, short form) |
| `migration_020_test` | PASS (sources.index_tier shape + CHECK, four new tables, two more CHECKs) |
| `schema_wipe_test` | PASS at v16 |

**Phase 1 acceptance met.** Slice 0 PR-ready.

## Outcome

- ✅ Impact: Slice 0 Foundation merged onto the 007 branch (`c62f9897`). Tests GREEN; constitution §XI single-citation-formatter rule enforced from day one. Ready to begin Phase 2 (Foundational module skeletons).
- 🧪 Tests: 3/3 Slice-0 tests PASS. No regressions in pre-existing test suite (citation + migration + schema_wipe all clean).
- 📁 Files: 7 new (migration, citation/{mod,types,formatter}.rs, two test files, Python tool); 6 modified (CHANGELOG, schema.rs, services/mod.rs, schema_wipe_test, data-model.md, tasks.md).
- 🔁 Next prompts: Continue `/sp.implement` for Phase 2 (T015–T021 — module skeletons + LlmTier refactor + indexer tier-check). After Phase 2 lands, Phase 3 (US1 fast-path intent router + 4 handlers) is the MVP shipping milestone.
- 🧠 Reflection: Two real corrections during impl (sources vs documents; legal_xrefs vs graph_edges) — both due to assuming SQL identifiers that didn't exist in the actual DB. The data-model.md is now grounded in the real schema; future slices will not hit similar surprises. Caught both via the migration_020_test before they could land in code.

## Evaluation notes (flywheel)

- Failure modes observed: two SQL-identifier mismatches caught at test time, fixed inline (sources, legal_xrefs).
- Graders run and results (PASS/FAIL): citation goldens 12/12 PASS; migration_020_test PASS; schema_wipe_test PASS.
- Prompt variant (if applicable): n/a.
- Next experiment (smallest change to try): for Phase 2's indexer refactor (T021), run the existing indexer integration test suite first (read-only baseline) before introducing the tier-check branch, so any regression is attributable to the new branch only.
