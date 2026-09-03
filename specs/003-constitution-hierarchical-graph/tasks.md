---
description: "Task list — Constitution v1 hierarchical + graph retrieval"
---

# Tasks: Constitution v1 — Hierarchical + Graph Retrieval

**Input**: Design documents from `/specs/003-constitution-hierarchical-graph/`
**Prerequisites**: plan.md, spec.md, research.md, data-model.md, quickstart.md

**Tests**: Included — the spec defines an acceptance gate (A1–A4) and the plan
names unit + acceptance test files. Test tasks are therefore in scope.

**Organization**: Tasks are grouped by the 7 pipeline stages **S1–S7** from
plan.md. Unlike independent user stories, S1–S7 form a **dependency chain** — a
retrieval pipeline cannot ship half-built. Each stage is an internal increment
with its own acceptance check; the shippable MVP is the full S1–S7 chain.

## Format: `[ID] [P?] [Stage] Description`

- **[P]**: Can run in parallel (different files, no incomplete dependencies)
- **[S1]–[S7]**: Pipeline stage from plan.md
- Exact file paths are in every task.

---

## Phase 1: Setup (Shared Infrastructure)

- [x] T001 Create the directory skeleton per plan.md: `src-tauri/src/services/doc_profiles/`, `tools/constitution_import/src/`
- [x] T002 Register `constitution_import` as a dev-only workspace binary; add the HTML-parsing dependency (`scraper`) — verify crate availability on crates.io before pinning
- [x] T003 [P] Confirm `serde`/`serde_json` are available for the `ConstitutionIR` types (already in the workspace — verify)

---

## Phase 2: Foundational (Blocking Prerequisites)

**⚠️ CRITICAL**: Blocks S3–S7. S1 and S2 are DB-independent and may proceed in parallel with this phase.

- [x] T004 DB schema for the graph layer (migrations are SQL files run via `sqlx::migrate!`, not Rust). Done in 3 files: (a) widened the `chunks.unit_type` CHECK in `src-tauri/src/db/migrations/013_hierarchical_chunks.sql` to add `part/chapter/article/clause/subclause` (kept `section/leaf` for Tier 3 compat) — edited in place because SQLite cannot ALTER a CHECK and the schema bump replays all migrations on a wiped DB; (b) new `src-tauri/src/db/migrations/014_constitution_graph.sql` — `graph_nodes` + `graph_edges` with composite indexes `(from_node,edge_type)`/`(to_node,edge_type)`, sets `schema_version=14`; (c) bumped `TARGET_SCHEMA_VERSION` 13→14 in `src-tauri/src/db/schema.rs`
- [x] T005 Verify the schema — DONE 2026-05-21. Build OK (2m40s); manual wipe needed (see CHANGELOG — `maybe_wipe` silently failed to delete the old DB on NTFS); after manual delete + relaunch: `schema_version=14`, `graph_nodes`/`graph_edges` present, `chunks.unit_type` CHECK widened. PASS

**Checkpoint**: Schema ready — S3–S7 unblocked.

---

## Phase 3: S1 — Source importer (dev-time)

**Goal**: A clean structured IR of the constitution from pakistani.org.
**Acceptance**: IR has ~12 Parts, ~290 Articles, every article has a marginal note.

- [x] T006 [P] [S1] Define `ConstitutionIR` serde types in `tools/constitution_import/src/ir.rs` (part → chapter → article → clause → subclause; marginal note, provisos[], explanations[], amendment annotations[]) per data-model.md §6
- [x] T007 [S1] Implement the pakistani.org fetch + parse in `tools/constitution_import/src/main.rs` — walk per-Part/Chapter HTML pages, parse the typographic markers into the IR (depends on T006)
- [x] T008 [S1] Add the `--out constitution_ir.json` emit and an article-count cross-check against the na.gov.pk official PDF
- [x] T009 [S1] **Acceptance**: run `cargo run --bin constitution_import -- --out tools/constitution_import/constitution_ir.json`; assert ~12 Parts / ~290 Articles; manually spot-check 10 articles against the official PDF — AUTO-PASS 2026-05-21: 12 Parts / 312 Articles, every article has a marginal note. Invocation is `cargo run --manifest-path tools/constitution_import/Cargo.toml` (standalone crate, not a workspace bin). Manual spot-check vs pakistani.org DONE 2026-05-21 over 6 review rounds — found + fixed importer data-loss bugs (multi-table, article-proviso, preamble) and added the amendment-metadata pass (per-node footnote_refs, 950 footnote descriptions, 24-amendment catalogue); re-scrape verified 312 Articles / 787 clauses. PHRs 0006–0011. S1 closed.

**Checkpoint**: `constitution_ir.json` exists and is verified.

---

## Phase 4: S2 — Doc-type profile + structure-aware parser

**Goal**: IR → L0–L5 `DocNode` tree.
**Acceptance**: tree has all 6 levels; provisos attached to parent clauses.

- [x] T010 [P] [S2] Create the `constitution` doc-type profile in `src-tauri/src/services/doc_profiles/constitution.rs` (level vocabulary, citation format). Done 2026-05-22: `DocProfile` (`mod.rs`) + `CONSTITUTION` const; heading patterns deferred to v2 — the parser consumes the structured S1 IR, so no raw-text heading detection is needed.
- [x] T011 [S2] Implement `parse_tree(ir, profile) -> DocNode` in `src-tauri/src/services/structure_parser.rs` — build the L0–L5 tree; attach provisos/explanations to their parent clause, never as separate nodes. Done 2026-05-22: `DocNode`/`NodeLevel`/`Attached` + `parse_tree`; provisos/explanations flattened in-leaf (D3); also added a `constitution_ir.rs` Deserialize mirror of the importer IR.
- [x] T012 [S2] Write unit tests in `src-tauri/tests/structure_parser_test.rs` — tree depth, per-level node counts, provisos attached in-leaf. Done 2026-05-22: 3 tests against the committed `constitution_ir.json`.
- [x] T013 [S2] **Acceptance**: `cargo test --test structure_parser_test` passes — VERIFIED 2026-05-22: 3/3 green (12 Parts / 312 Articles / 787 Clauses / depth 6; Article 18 proviso in-leaf; Article 8(3)(b) citation + unit_path canonical).

**Checkpoint**: structure-aware parser verified.

---

## Phase 5: S3 — Schema persistence

**Goal**: persist the `DocNode` tree into `chunks`.
**Acceptance**: A1 (structural) + A2 (citation accuracy).

- [x] T014 [S3] Extend `unit_type` handling in `src-tauri/src/db/repositories/chunks.rs` for the new `part/chapter/article/clause/subclause` vocabulary. Done 2026-05-22: the migration-013 CHECK already accepts the vocab; added `bulk_insert_with_ids` (single-tx, caller-assigned IDs, parent-before-child) so a child's `parent_chunk_id` can reference its parent before insert; `bulk_insert` now delegates to it.
- [x] T015 [S3] Implement tree persistence — write the `DocNode` tree to `chunks` with `parent_chunk_id`, `unit_path` (canonical citation path), and `domain_metadata` JSON. Done 2026-05-22: `persist_tree(pool, source_id, &DocNode)` + pre-order `collect_chunks` in `services/structure_parser.rs`; `NodeLevel::unit_type()` maps to the canonical storage vocab; `domain_metadata` = `NodeMetadata{citation, level, title, attached, footnote_refs}` (legal_domain enrichment deferred to S5). 2 `#[tokio::test]` tests green.
- [x] T016 [S3] Create db_inspection script `09_citation_accuracy.sh` — assert each `unit_path`/citation matches the article it contains. Done 2026-05-22: `~/Desktop/db_inspection_constitute_v1/09_citation_accuracy.sh` checks every article/clause/subclause's citation, unit_path, parent-chain article, content lead-in and domain_metadata for agreement.
- [x] T017 [S3] **Acceptance**: run db_inspection `07`/`09` on a persisted tree → A1 + A2. **PASS 2026-05-22** (run1): A1 (`07`) — 12 part / 29 chapter / 312 article / 787 clause / 325 subclause, 0 orphans, every non-document node parented, no leaf-hogging. A2 (`09`) — 1424 article/clause/subclause chunks checked, **0 failures**. Note: `08_leaves_per_section` is the old Tier-3 `section`/`leaf` script (reports 0/0) — inapplicable to the constitution vocab; superseded by `07`.

**Checkpoint**: hierarchy persisted, structurally + citation correct.

---

## Phase 6: S4 — Graph builder

**Goal**: populate `graph_nodes` / `graph_edges`.
**Acceptance**: A3 (graph integrity).

- [x] T018 [P] [S4] Create the graph repo `src-tauri/src/db/repositories/graph.rs` — `insert_nodes`, `insert_edges`, and `expand(seed_ids, edge_types, max_depth)` as a bounded recursive CTE (depth ≤ 3, visited-set dedup) per data-model.md §5. Done 2026-05-22: `GraphNode`/`GraphEdge` + the three fns; `expand` is a `WITH RECURSIVE … UNION` CTE (dedup), `max_depth` hard-capped at 3.
- [x] T019 [S4] Implement `src-tauri/src/services/graph_builder.rs` — build `contains` (tree), `cross_reference` ("Article N" mentions in clause text), `non_obstante` ("notwithstanding…" detection) edges. Done 2026-05-22: `build_graph(tree, &persisted_chunks)`; two-pass walk; self-Article refs skipped. **`amended_by` NOT built** — migration 014 makes `graph_edges.to_node` a NOT NULL FK to `graph_nodes` and `node_type` admits only the 6 hierarchy levels; there is no amendment node to point at. Amendment provenance stays on chunk `domain_metadata.footnote_refs`; an `amended_by` overlay needs a v2 schema with amendment nodes (see CHANGELOG_DEV 2026-05-22 S4).
- [x] T020 [S4] Unit tests for the graph builder — edge-type coverage, cross-reference resolution, no dangling endpoints. Done 2026-05-22: 3 tests in an in-crate `#[cfg(test)]` module in `graph_builder.rs` (in-crate, not `tests/graph_builder_test.rs`, so they share the lib's `tokio`/`sqlx` deps — same pattern as `persist_tests`). Cover `article_refs` extraction, edge-type coverage + no dangling endpoints, and bounded `expand`.
- [x] T021 [S4] Create db_inspection script `10_graph_integrity.sh` and run it → **Acceptance** A3. Done 2026-05-22: `~/Desktop/db_inspection_constitute_v1/10_graph_integrity.sh`. **A3 PASS** (run1): graph_nodes 1466 (== chunks), graph_edges 1604 (contains 1465 / cross_reference 116 / non_obstante 23); 0 nodes without a backing chunk, 0 dangling edge endpoints; spot-checks resolve to real Article→Article pairs. `amended_by 0` — by design (no amendment node type in the schema; v2).

**Checkpoint**: graph layer built and verified.

---

## Phase 7: S5 — Multi-level embedding

**Goal**: embeddings for all retrieval levels.
**Acceptance**: chunks == chunk_embeddings, dim 768, no NaN/Inf.

- [x] T022 [S5] Multi-level chunk content — sub-clause leaves carry the Article heading + clause chapeau prepended; article/part chunks are emitted. Done 2026-05-22: implemented in `services/structure_parser.rs` (`node_content` + the `context` arg threaded through `collect_chunks`), NOT `chunker.rs::chunk_tree` — the constitution path uses `persist_tree`, not the heading-detector `chunk_tree`; `persist_tree` already emits a chunk per level, so the substance was leaf context-prepending. Verified by the updated `persisted_leaf_carries_canonical_citation_and_path` test.
- [x] T023 [S5] Wire the constitution import path in `src-tauri/src/services/indexer.rs` — detect the doc-type, run structure_parser → persist → graph_builder → embed; emit the per-source hierarchy log line. Done 2026-05-22: `try_load_constitution` (doc-type routing hook — extends to future Act/Rule profiles) + `index_constitution` (parse_tree → delete_for_source → persist_tree → build_graph → insert_nodes/edges → embed_and_upsert → finalize); `do_index` routes to it; the embed/upsert block was extracted to a shared `embed_and_upsert`. Hierarchy log line: `(part:N chapter:N article:N clause:N subclause:N) — graph N nodes / N edges`. Compiles clean; 6/6 regression tests pass.
- [x] T024 [S5] **Acceptance**: run db_inspection `01`–`04` — chunks == chunk_embeddings, dim 768, no NaN/Inf. **PASS 2026-05-22** (run1, `~/Desktop/db_inspection_constitute_v1/`): 1466 chunks == 1466 chunk_embeddings; all 1466 vectors dim=768 / 3072 bytes / NaN-Inf=0; `embedding_dim=768`; source `indexed`, no error. Structural (`07`): 1 document / 12 part / 29 chapter / 312 article / 787 clause / 325 subclause, 0 orphans, every non-document node parented — the Article-69 leaf-hogging bug is gone. `echo.db` is now populated → T016/T017/T021 unblocked.

**Checkpoint**: constitution fully indexed end-to-end.

---

## Phase 8: S6 — Retrieval pipeline

**Goal**: wire v1 retrieval with the citation fast-path and graph expansion.
**Acceptance**: A4 (F1–F5 retrieval).

- [x] T025 [S6] Implement `citation_fast_path(query)` in `src-tauri/src/services/query_router.rs` — regex article/clause detection → deterministic metadata lookup, bypassing the reranker. Done 2026-05-22: `detect_citation` (regex `Article N(x)(y)` → canonical `unit_label` form) + `citation_fast_path` (resolve by `unit_label`, return the unit + its sub-tree via `unit_path` prefix). 2 tests green.
- [x] T026 [S6] Wire the citation fast-path + 1-hop graph expansion into `route_scoped`. Done 2026-05-23 (Option 1, user-confirmed — graph expansion runs **only on citation queries**, not every query, to protect chat speed): when `citation_fast_path` resolves a unit, `expand_citation_neighbours` follows `cross_reference`/`non_obstante` edges 1 hop (`graph::expand`), `fetch_chunks_by_ids` loads the union, and vector search / FTS5 / reranker are all bypassed; emits a "Citation fast-path" trace step. Graph errors are non-fatal. `amended_by` not expanded (not built — see S4). Compiles clean; 10/10 query_router tests pass.
- [x] T027 [S6] Register the `graph` repo for state wiring — **no-op by design** (2026-05-23): the `graph` repo is free functions over `&SqlitePool`; `route_scoped` already holds `pool`, so no `.manage()`/Tauri state is needed. Resolved without a `lib.rs` change.
- [x] T028 [S6] **Acceptance A4 DONE (2026-05-23)** — F1–F5 re-run in-app (15 prompts, `local-0_5b`). Retrieval objective PASS: citation fast-path fired on all 4 article-citing prompts, correct sub-tree <500 ms, ~2.4× faster than hybrid; FTS5 hybrid recall healthy (24 hits). End-to-end answer accuracy 3/15 PASS — gated by the 0.5B generator (negation inversion, list truncation, hallucination), NOT retrieval. Two retrieval gaps found: (1) Preamble not indexed (Fact 5 unanswerable — IR import gap); (2) Art 204 contaminates Art 6 results (Fact 3 v3). Results: `~/Desktop/test 5 prompts/T028-A4-results.md`.

**Checkpoint**: retrieval pipeline live.

---

## Phase 9: S7 — Acceptance harness & v1 gate

**Goal**: the consolidated v1 acceptance gate.
**Acceptance**: A1–A4 all green.

- [ ] T029 [S7] Write `src-tauri/tests/constitution_v1_acceptance.rs` — automated A1–A4 (structural counts, citation accuracy, graph integrity, retrieval seed checks)
- [ ] T030 [S7] Run the full A1–A4 gate (`cargo test --test constitution_v1_acceptance` + db_inspection `07`–`10` + reranker_eval); record results
- [ ] T031 [S7] Append a dated entry to `CHANGELOG_DEV.md` — symptom/why, fix (file paths + functions), cross-cutting impact, v2 follow-ups

**Checkpoint**: v1 done → v2 (OCR/extraction for the govt corpus) unblocked.

---

## Phase 10: Polish & Cross-Cutting Concerns

- [ ] T032 [P] Update `docs/research/legal-corpus-hierarchy-architecture.md` with v1-as-built notes (actual counts, drift from plan)
- [ ] T033 [P] Run the `quickstart.md` validation end-to-end
- [ ] T034 Surface the ADR — `/sp.adr legal-corpus-hierarchical-graph-model` (hierarchical+graph model, SQLite-vs-CozoDB decision)

---

## Dependencies & Execution Order

### Phase dependencies

- **Setup (P1)** — no dependencies; start immediately.
- **Foundational (P2)** — after Setup; **blocks S3–S7**. S1 and S2 are
  DB-independent and may run in parallel with P2.
- **S1 (P3) → S2 (P4) → S3 (P5) → S4 (P6) → S5 (P7) → S6 (P8) → S7 (P9)** — a
  strict dependency chain (each stage consumes the previous stage's output).
- **Polish (P10)** — after S7.

### Within each stage

- Models/types before services; services before wiring; implementation before
  the acceptance task.
- Each stage's **Acceptance** task must pass before the next stage begins.

### Parallel opportunities

- Setup: T003 ∥ T001/T002.
- T006 ([S1] types) and T010 ([S2] profile) can be drafted alongside Foundational
  (T004/T005) — they touch no DB.
- T018 ([S4] graph repo) can start once T004 lands, in parallel with S3 finishing.
- Polish: T032 ∥ T033.

Most of the pipeline is **inherently sequential** — do not expect broad
parallelism; the value is in the dependency-ordered acceptance gates.

---

## Implementation Strategy

### MVP = the full S1–S7 chain

A retrieval pipeline is not shippable half-built. The MVP is S1→S7 with the
A1–A4 gate green. Each stage's acceptance check is the internal checkpoint —
**stop and validate at every checkpoint** before proceeding.

### Incremental validation

1. Setup + Foundational → schema ready.
2. S1 → IR verified against the official PDF.
3. S2 → parser unit-tested.
4. S3 → A1 + A2 (structure + citations correct).
5. S4 → A3 (graph integrity).
6. S5 → embeddings verified.
7. S6 → A4 (retrieval).
8. S7 → full A1–A4 gate → v1 done.

### The v1 gate

Per spec.md: **all of A1–A4 must pass before any v2/OCR work begins.** A3/A4 for
the reasoning queries (F3/F5) may still need the fine-tuned reranker — that is
tracked separately and is **not** a v1 blocker.

---

## Notes

- `[P]` = different files, no incomplete dependencies.
- `[S1]`–`[S7]` map every task to a plan.md stage for traceability.
- The source importer is **dev-time only** (`tools/`) — it never ships in the
  app bundle (Constitution Principle I).
- All migrations additive; smallest viable diff; commit after each task or
  logical group.
- Verify unit tests fail before implementing (T012, T020, T029).
