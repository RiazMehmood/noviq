---
description: "Task list — Feature 006 LLM PDF→IR Importer"
---

# Tasks: LLM-Driven PDF → Hierarchical JSON Importer

**Branch**: `006-llm-pdf-importer`
**Input**: Design documents in `/specs/006-llm-pdf-importer/`
**Prerequisites**: spec.md, plan.md, research.md, data-model.md, contracts/, quickstart.md (all present)

**Tests**: Included. The plan committed to three test targets (`llm_ir_importer_acceptance`, `ir_validator`, `ir_cache_lifecycle`) and the spec's acceptance scenarios serve as the test contract.

**Organization**: Grouped by user story so each P-story (P1=auto-import, P2=repair, P3=cache lifecycle) is independently shippable per Constitution VIII.

## Format

`[ID] [P?] [Story?] Description`

- **[P]** — parallelizable (different files, no incomplete-task dependencies)
- **[Story]** — `US1` / `US2` / `US3` — user story phase only; setup/foundational/polish have no story label
- File paths are absolute-from-repo-root

## Path conventions

Single project. Rust backend under `src-tauri/src/`, SolidJS frontend under `src/`, tests under `src-tauri/tests/`.

---

## Phase 1: Setup (Shared Infrastructure)

**Purpose**: Wire dependencies and scaffold the new module so every later task lands in a place that already compiles.

- [X] T001 Add Rust dependencies to `src-tauri/Cargo.toml`: `jsonschema = "0.18"`, `async-trait = "0.1"`, ensure `chrono` has the `serde` feature
- [X] T002 [P] Create empty module skeleton at `src-tauri/src/services/doc_ir/mod.rs` exporting the 13 sub-modules listed in plan.md §Project Structure (each file empty with `// TODO: T###` header)
- [X] T003 [P] Register `pub mod doc_ir;` in `src-tauri/src/services/mod.rs`
- [X] T004 [P] Add `STRUCTURED_IMPORTER`-style version constant `pub const LLM_IR_IMPORTER_VERSION: i64 = 1;` to `src-tauri/src/services/doc_ir/mod.rs`

**Checkpoint**: `cargo build --manifest-path src-tauri/Cargo.toml` succeeds with the empty module.

---

## Phase 2: Foundational (Blocking Prerequisites)

**Purpose**: Types, schema, validator, migration, and dispatch skeleton — every user story phase depends on these.

**⚠️ CRITICAL**: No user story work can begin until this phase is complete.

- [X] T005 Write SQL migration in `src-tauri/src/db/migrations/018_llm_ir_importer.sql` (full contents from data-model.md §3 — `ir_cache_index` + `ir_repair_log` tables + 4 additive columns on `sources` + 3 indices). Verify `sqlx::migrate!` picks it up on `cargo test`.
- [X] T006 [P] Author canonical JSON schema at `src-tauri/src/services/doc_ir/schema.json` from `contracts/ir_json_schema.md`. Embed via `include_str!` in `types.rs`.
- [X] T007 [P] Implement IR types in `src-tauri/src/services/doc_ir/types.rs`: `DocIr`, `StructureNode`, `NodeKind`, `QualityFlag`, `Annotation`, `AnnotationKind`, `DocType`. Match data-model.md §1 exactly (serde rename_all = "snake_case" on enums). Add `#[serde(deny_unknown_fields)]` on struct types except `metadata`/`doc_metadata`.
- [X] T008 [P] Implement `ValidationIssue`, `Severity`, `ValidationCode` (closed enum, 12 codes from data-model.md §6) in `src-tauri/src/services/doc_ir/validator.rs` — types only, no logic yet.
- [X] T009 Implement `validate_doc_ir(&DocIr) -> Vec<ValidationIssue>` in `src-tauri/src/services/doc_ir/validator.rs`. Two passes: (1) `jsonschema` against `schema.json`, (2) semantic — parent integrity / cycles / dup ids / monotonic numbering / ≥90% text coverage / footnote refs / omission marker / page bounds. Depends on T006, T007, T008.
- [X] T010 [P] [tests] Write `src-tauri/tests/ir_validator.rs` — unit cases for each of the 12 issue codes (positive + negative). Use small handcrafted `DocIr` fixtures; no model dependency.
- [X] T010b [P] [tests] Write `src-tauri/tests/ir_chunker.rs` — unit cases for the chunker (T017): heading-only doc, doc that hits the 2400-token budget mid-section, doc with no headings (falls back to paragraph splits), breadcrumb propagation across chunks. Closes regression risk on a pure function. **DEFERRED to Phase 3 — depends on T017.**
- [X] T010c [P] [tests] Write `src-tauri/tests/ir_converter_parity.rs` — drives `constitution_ir_to_doc_ir` (T013) on the existing `constitution_ir.json` fixture and asserts: (a) every Part/Chapter/Article/Clause/SubClause/Proviso/Explanation/Footnote becomes one node, (b) parent_id chain matches the nested-tree structure, (c) footnotes emerge as `kind: footnote` nodes with footnote_refs back-pointing correctly. Independent of LLM.
- [X] T011 [P] Implement `IrBuilder` trait + `BuilderInput` / `BuilderOutput` / `IrBuildError` / `RetryCounts` in `src-tauri/src/services/doc_ir/builder.rs` per `contracts/ir_builder_trait.md`. Trait body only; no impls yet.
- [X] T012 [P] Implement `ProgressSink` enum + `emit_progress(...)` helper in `src-tauri/src/services/doc_ir/progress.rs`. Two variants: `Noop` (for tests) and `Tauri(AppHandle)` emitting `importer://progress` per `contracts/tauri_commands.md` §1.
- [X] T013 [P] Implement converter `constitution_ir_to_doc_ir(&ConstitutionIr) -> DocIr` in `src-tauri/src/services/doc_ir/converter_from_constitution.rs`. Maps Part/Chapter/Article/Clause/SubClause/Proviso/Explanation/Footnote into flat `nodes[]` with `parent_id`. Used by tests and as a fallback.
- [X] T014 [P] Implement cache index repo at `src-tauri/src/db/repositories/ir_cache.rs`: `insert_or_update`, `mark_orphaned(sha)`, `clear_orphan(sha)`, `find_orphans_older_than(cutoff)`, `delete(sha)`, `get(sha) -> Option<CacheRow>`, `list_needing_review() -> Vec<…>`. SQL-only; no JSON file IO here.

**Checkpoint**: `cargo test --test ir_validator` passes; migration applies cleanly to `sqlite::memory:`; module compiles with empty builder/cache impls.

---

## Phase 3: User Story 1 — Import a previously-unsupported PDF and get clean structure (P1) 🎯 MVP

**Goal**: When a user drops a structured PDF in a watched folder and the Mistral 7B GGUF is present, the importer converts it to `DocIr`, validates, and feeds the existing `graph_builder + indexer` so the document becomes searchable with correct parent/child structure, footnotes, and amendment annotations.

**Independent Test**: Per spec.md §US1: pick one Sindh PDF that fails feature 005's rule-based parser at V5 baseline `2f6a6d77`; run it through this pipeline; verify ≥90% text coverage in indexed `chunks.content`, parent/child relationships match the document outline, and the 22-prompt-eval questions targeting that document return correct answers grounded in the right sections.

### Tests for User Story 1

- [X] T015 [P] [US1] Write golden capture instructions + the captured `specs/006-llm-pdf-importer/golden/sindh_csa_1973.docir.json` (produced from a manual run against the real Mistral 7B; committed as a frozen artefact)
- [X] T016 [P] [US1] Write `src-tauri/tests/llm_ir_importer_acceptance.rs` — "P1 gate" subset: load golden DocIr as if returned by a stub `IrBuilder`, run through `do_index` dispatch, assert: (a) `sources.extraction_method = 'llm_ir'`, (b) ≥90% coverage hold, (c) constitution-test-style structural counts (sections / clauses / footnotes), (d) FR-027 fallback path triggers when stub returns `IrBuildError::ModelUnavailable`. Pure cargo test; no Mistral required.

### Implementation for User Story 1

- [X] T017 [P] [US1] Implement chunker in `src-tauri/src/services/doc_ir/chunker.rs`: reuse `heading_detector::regex_pass::scan_with_profile` for boundary detection; accumulate to 2400-token budget; emit `Vec<Chunk { text, breadcrumb, page_start, page_end }>`. Pure function; testable.
- [ ] T018 [P] [US1] Implement system prompts per `DocType` in `src-tauri/src/services/doc_ir/prompts.rs`. Versioned constants `IR_BUILDER_SYSTEM_PROMPT_V1 = ...`; one prompt per `DocType` enum value; CoT-style with one few-shot example per type (per `feedback_prompt_engineering` memory).
- [ ] T019 [US1] Implement grammar binding in `src-tauri/src/services/doc_ir/grammar.rs`: `pub fn build_ir_sampler(model: &LlamaModel) -> Result<LlamaSampler>`. Wraps `llama_cpp_2::llguidance::create_llg_sampler` with `grammar_kind = "json_schema"`, `grammar_data = include_str!("schema.json")`. Add penalties/temp inline (temp=0 for grammar mode).
- [ ] T020 [US1] Implement classifier in `src-tauri/src/services/doc_ir/mistral_builder.rs::classify_doc_type` — feed first ~600 tokens of `extracted_text` through Mistral with a constrained-output sub-schema (`{"doc_type": "constitution|act|rules|sro|contract|generic"}`); fall back to `doc_profiles::detect_profile` rule on classifier failure. Returns `DocType` + provenance.
- [ ] T021 [US1] Implement `MistralIrBuilder` struct + `IrBuilder` impl in `src-tauri/src/services/doc_ir/mistral_builder.rs`. Steps: (a) `available()` returns true iff **(i)** GGUF exists at `models/mistral-7b-instruct-v0.3.Q4_K_M.gguf` AND **(ii)** `hardware_detector::detect_tier()` returns ≥ `HardwareTier::Tier3` (≥12 GB RAM). On hardware-tier failure, emit `importer://llm-unavailable { reason: "hardware_tier", remediation: "Mistral 7B IR builder needs ≥12 GB RAM (Tier 3)." }` once per session and return false. (b) `build()` runs classify → chunk → per-chunk constrained generate → validate → assemble. Uses `priority_model_slot::infer_with_cancel` at `n_ctx=4096`, `max_prompt_tokens=3200`. Depends on T017, T018, T019, T020. Closes Constitution VII gap (G2).
- [ ] T022 [US1] Implement retry loop in `src-tauri/src/services/doc_ir/retry.rs`: `retry_chunk(builder, chunk, last_error, attempts_remaining)` — max 3 per chunk at temp=0 with validator-message reprompt; max 2 full-doc retries. Nodes that exhaust retries get `quality_flag = LowConfidence` + `quality_error = <msg>`. Called from `mistral_builder::build`.
- [X] T023 [US1] Implement cache write helper `cache::write_doc_ir(app, &DocIr) -> Result<PathBuf>` in `src-tauri/src/services/doc_ir/cache.rs`. Writes `{app_data_dir}/ir_cache/<sha>.json` envelope-wrapped (envelope_version=1). Updates `ir_cache_index` row via T014's repo with `quality_score = nodes_ok / nodes_total`. Depends on T014.
- [X] T024 [US1] Wire IR-path dispatch in `src-tauri/src/services/indexer.rs::do_index`: between existing `try_load_constitution` branch (preserve as-is) and the structured-importer call, add `if MistralIrBuilder::default().available() && extracted_text.len() >= 200 { route to LLM IR path }`. On success: call `cache::write_doc_ir`, then call existing `structure_parser::persist_tree` (after `DocIr → DocNode` adaptation) and `graph_builder::build_graph`. Set `sources.extraction_method = 'llm_ir'`, `sources.ir_cache_sha = <sha>`, `sources.ir_quality_score`, `sources.nodes_low_confidence`.
- [X] T025 [P] [US1] Implement `DocIr → structure_parser::DocNode` adapter in `src-tauri/src/services/doc_ir/mod.rs` (`fn to_doc_node_tree(&DocIr) -> DocNode`) so existing `persist_tree` consumes IR without changes. Hides the flat-vs-tree mismatch behind a 30-line conversion.
- [X] T026 [US1] Implement fallback notification (FR-027): in dispatch, when `MistralIrBuilder::default().available()` returns false AND `extracted_text.len() >= 200`, emit `importer://llm-unavailable` Tauri event once per session (use `OnceCell<AtomicBool>` in `lib.rs`); fall through to existing `structured_importer::import` path. Wire in `indexer::do_index`.
- [X] T027 [US1] Implement `MIN_TEXT_CHARS = 200` short-circuit (FR-028): if `extracted_text.len() < 200`, set `sources.extraction_method = 'unprocessable'` and return without invoking LLM. Wire in `indexer::do_index` before the GGUF check.
- [X] T028 [US1] Emit `importer://progress` and `importer://done` events from `mistral_builder::build` and `do_index` (per `contracts/tauri_commands.md` §1). Phase sequence: classifying → chunking → converting → validating → indexing.
- [ ] T029 [US1] Register `cancel_import` Tauri command in `src-tauri/src/commands/importer.rs` (new file) and `lib.rs`. Holds a `RwLock<HashMap<source_id, CancellationToken>>` keyed by `source_id`; `cancel_import` flips the token; in-flight `infer_with_cancel` returns; partial state rolled back (graph rows deleted, source row reverted).
- [X] T029b [US1] Implement single-flight import queue in `src-tauri/src/services/doc_ir/queue.rs`. Tokio `Mutex<VecDeque<QueueEntry>>` keyed by `source_id`; one active dispatch at a time. On enqueue, broadcast `importer://queued { source_id, position, queue_length }`; on dequeue/start, re-broadcast position for every still-waiting entry so the UI reflects "advance". Wired by `indexer::do_index` so it acquires a queue slot before invoking `MistralIrBuilder`. Satisfies FR-024 (one-at-a-time + position visibility). Closes G1.
- [X] T030 [US1] Extend acceptance test `llm_ir_importer_acceptance.rs` to assert end-to-end golden replay produces the exact same chunk/graph counts as a constitution-style structured import on the same content — the parity check.

**Checkpoint**: P1 ships. Drop a Sindh PDF → indexed via LLM path. Toggle GGUF away → falls back to rule-based path with a one-time notification. Cancel mid-import → clean rollback. SC-001, SC-002, SC-003, SC-008 measurable.

---

## Phase 4: User Story 2 — Review and repair low-confidence sections (P2)

**Goal**: After import, the user receives a notification with a count of sections needing review. Document is already searchable; a doc badge shows the count. Opening the repair view shows the rendered PDF page next to an editable structured representation; saving validates and incrementally re-indexes only the changed nodes; repairs survive re-runs.

**Independent Test**: Per spec.md §US2: import a doc that produces at least one low-confidence section. Verify (a) confident sections searchable immediately, (b) notification + badge surface count, (c) repair view opens with correct PDF page + editor, (d) save updates only that section in the index, (e) re-running the importer preserves the human edits.

### Tests for User Story 2

- [ ] T031 [P] [US2] Add P2 cases to `src-tauri/tests/llm_ir_importer_acceptance.rs`: (a) stub returns IR with 1 low-confidence node; assert `sources.nodes_low_confidence = 1`, `importer://done` carries `outcome: "partial"`; (b) call `save_repair` with a valid edit; assert affected chunks re-embedded, `quality_flag` flips to `user_repaired`, `ir_repair_log` has 1 row; (c) stub re-run preserves the user-repaired node.

### Implementation for User Story 2

- [ ] T032 [P] [US2] Add page-rasterization helper in `src-tauri/src/services/doc_ir/page_render.rs`: `render_page(app, sha, page) -> Result<PathBuf>`. Shells out to `pdftoppm -r 150 -png -f <p> -l <p>` (reuse the Feature-004 OCR shell pattern). LRU cache at `{app_data_dir}/page_cache/<sha>/p<N>.png`, cap 200 pages (sweep on insert).
- [ ] T033 [P] [US2] Register Tauri command `render_pdf_page` in `src-tauri/src/commands/importer.rs` (delegates to T032). Per `contracts/tauri_commands.md` §2.
- [ ] T034 [P] [US2] Register Tauri commands `get_doc_ir` and `get_low_confidence_nodes` in `src-tauri/src/commands/importer.rs`. `get_doc_ir` reads cache JSON for the source's `ir_cache_sha`; `get_low_confidence_nodes` filters in-memory.
- [ ] T035 [P] [US2] Register Tauri command `list_docs_needing_review` in `src-tauri/src/commands/importer.rs` — query `sources` JOIN `ir_cache_index` WHERE `nodes_low_confidence > 0`. Used for badge + toast counts.
- [ ] T036 [US2] Implement `save_repair` backend in `src-tauri/src/services/doc_ir/repair.rs`: validate each `NodeEdit` (re-run `validate_doc_ir` on the patched IR scoped to affected nodes' subtree); on pass — rewrite cache JSON, flip `quality_flag` to `UserRepaired`, append to `ir_repair_log`, return changed-node ids. Depends on T009, T014, T023.
- [ ] T037 [US2] Implement partial re-index in `src-tauri/src/services/doc_ir/repair.rs::reindex_changed`: delete affected chunks via `chunks::delete_by_ids`; re-run `structure_parser::persist_tree` scoped to changed-node subtree (new helper `persist_subtree`); rebuild only incident graph edges; re-embed changed chunks via existing embedder. Depends on T036.
- [ ] T038 [US2] Register Tauri command `save_repair` in `src-tauri/src/commands/importer.rs` (delegates to T036 + T037).
- [ ] T039 [US2] Implement sticky-repair preservation in `cache::write_doc_ir` (extend T023): before write, load existing cache JSON if present; for any node where existing `quality_flag = UserRepaired` matches `(parent_breadcrumb, number, hash(text[:120]))` of an incoming node, overwrite the incoming node with the user version and keep `UserRepaired` flag. Satisfies FR-018.
- [ ] T040 [P] [US2] Add SolidJS `ReviewBadge.tsx` at `src/components/DocCard/ReviewBadge.tsx` — props `{ count: number }`, renders chip "N sections need review" when count > 0; nothing otherwise.
- [ ] T041 [P] [US2] Wire ReviewBadge into existing DocCard. (Minor edit, no new file.)
- [ ] T042 [P] [US2] Add typed wrappers at `src/services/importer.ts`: `getDocIr`, `getLowConfidenceNodes`, `renderPdfPage`, `saveRepair`, `listDocsNeedingReview`, `cancelImport`. One file, ~80 lines.
- [ ] T043 [P] [US2] Add `useImporterProgress.ts` hook at `src/hooks/useImporterProgress.ts`: SolidJS `createEffect` subscribes to `importer://progress` and `importer://done` events; exposes `progress(): Accessor<ImporterProgress|null>` and `done(): Accessor<ImporterDone|null>`.
- [ ] T043b [P] [US2] Extend `useImporterProgress.ts` to also subscribe to `importer://queued`; expose `queueState(): Accessor<{ position: number, queue_length: number } | null>`. Render queue position as a sub-line in the progress card ("Queued — 2 of 3 ahead") for non-running entries. Satisfies FR-024's "queue position visible" half.
- [ ] T044 [US2] Add toast notification on `importer://done` with `outcome === "partial"` — wire in `src/App.tsx` or existing notification host. Message: "Import done — N sections need your review." Click → navigates to repair route.
- [ ] T045 [P] [US2] Build `PdfPagePane.tsx` at `src/components/RepairView/PdfPagePane.tsx` — receives `{sourceId, page}`; calls `renderPdfPage` on mount/change; renders `<img>` at natural width, container-scrollable.
- [ ] T046 [P] [US2] Build `IrEditorPane.tsx` at `src/components/RepairView/IrEditorPane.tsx` — receives `{node, schema}`; renders text field for `text`, dropdowns for `kind` (enum from schema) and `quality_flag`, annotations sub-editor. Live validation hint as the user types (client-side jsonschema-js or manual).
- [ ] T047 [US2] Build `RepairView.tsx` container at `src/components/RepairView/RepairView.tsx` — loads `getDocIr` + `getLowConfidenceNodes`, drives the `current` index (◀ Prev / Next ▶), wires save button calling `saveRepair`. Solid `<Show>`-accessor pattern per `feedback_solidjs_show_accessor`. Async-onMount cleanup per `feedback_solidjs_async_onmount`.
- [ ] T048 [US2] Add route `src/routes/docs/[id]/repair.tsx` mounting `RepairView`. Reachable from doc card badge click and from toast click.

**Checkpoint**: P2 ships. Doc card shows badge; clicking it opens repair view with PDF + JSON; save validates + partial-reindexes; user repairs survive re-import. SC-004, SC-005, SC-009 measurable.

---

## Phase 5: User Story 3 — Trust that cached structure stays in sync with the file system (P3)

**Goal**: Cache reuse skips the LLM on identical-content re-imports (any filename). Removed PDFs leave their cached IR orphaned; a daily sweep deletes orphans older than 7 days; re-adding within the grace window reactivates.

**Independent Test**: Per spec.md §US3. Steps: import → observe `ir_cache_index` row written; delete PDF → observe `orphaned_at` set; re-import (same content, different filename) within 7 days → verify LLM not invoked; advance clock past 7 days → row + JSON file removed.

### Tests for User Story 3

- [ ] T049 [P] [US3] Write `src-tauri/tests/ir_cache_lifecycle.rs` — drive `cache` + `ir_cache` repo through the full state machine from data-model.md §4: insert → orphan → reactivate (within 7d) → sweep (past 7d). Uses `sqlite::memory:` and `tempfile::TempDir` for the JSON dir; no LLM, no Tauri.

### Implementation for User Story 3

- [ ] T050 [P] [US3] Implement `cache::lookup_by_sha(app, sha) -> Result<Option<DocIr>>` in `src-tauri/src/services/doc_ir/cache.rs`. Reads disk JSON if `ir_cache_index` row exists, schema_version matches, and `orphaned_at` IS NULL (or reactivates by clearing `orphaned_at` and returning).
- [ ] T051 [US3] Wire cache-hit short-circuit in `indexer::do_index`: compute sha256 of the source PDF bytes; call `cache::lookup_by_sha`; if `Some(ir)`, skip LLM entirely, run `persist_tree` + `graph_builder` from the cached IR. Satisfies FR-019, FR-022, SC-006.
- [ ] T052 [P] [US3] Implement `cache::mark_orphaned(sha)` hook called by the existing `file_watcher` removal path. Wire the call in `file_watcher.rs` when a watched-folder removal is detected — single one-line call after `sources` row deletion.
- [ ] T053 [P] [US3] Implement `cache::sweep_orphans(app, cutoff: SystemTime)` in `src-tauri/src/services/doc_ir/cache.rs` — calls `find_orphans_older_than(now - 7d)`, deletes each JSON file, then deletes index rows in one transaction. Returns count.
- [ ] T054 [US3] Schedule the daily sweep in `src-tauri/src/services/scheduler.rs`: tick once at app start and every 24h thereafter. Reuses existing scheduler primitives — minimal addition.
- [ ] T055 [P] [US3] Register Tauri devtools commands `ir_cache_stats` and `force_orphan_sweep` in `src-tauri/src/commands/importer.rs` per `contracts/tauri_commands.md` §3.
- [ ] T056 [US3] Implement schema-version bump path: when `cache::lookup_by_sha` finds a row with `schema_version < LLM_IR_IMPORTER_VERSION`, treat as miss and re-run. After re-run, call `repair.rs` migration helper that text-matches existing `UserRepaired` nodes from the old cache file into the new IR. Satisfies FR-023.

**Checkpoint**: P3 ships. Same-sha re-import skips Mistral; orphan + 7-day sweep verified; devtools `force_orphan_sweep` available. SC-006, SC-007 measurable.

---

## Phase 6: Polish & Cross-Cutting

- [ ] T057 [P] Update `CHANGELOG_DEV.md` with a feature-006-complete entry (only after acceptance test 5/5 GREEN per the existing convention from features 003-005).
- [ ] T058 [P] Add `enqueue_llm_ir_reextract` helper in `src-tauri/src/services/indexer.rs` mirroring the rule-based one — consulted at startup when `LLM_IR_IMPORTER_VERSION` bumps.
- [ ] T059 [P] Run `npm run tauri dev`, exercise the quickstart.md smoke tests end-to-end (golden path + fallback + repair + cache-hit + orphan sweep). Capture a screenshot of the repair view for the PR description.
- [ ] T060 [P] Performance sanity: time a cached-reimport (must be <5s for SC-006) and a single-node repair save (must be <10s for SC-005). Record in CHANGELOG_DEV.md entry.
- [ ] T061 Audit `cargo test --manifest-path src-tauri/Cargo.toml` — all three new test targets GREEN; existing constitution + 005 + OCR acceptance gates still GREEN (regression guard).
- [ ] T062 [P] Update memory file `project_echoai_llm_pdf_to_json_importer_design.md` to "closed" status with merge commit pointer.

---

## Dependencies & Execution Order

### Phase Dependencies

- **Setup (Phase 1)**: No prior dependencies
- **Foundational (Phase 2)**: Depends on Phase 1 complete — BLOCKS all user stories
- **US1 (Phase 3)**: Depends on Phase 2 complete
- **US2 (Phase 4)**: Depends on Phase 2 complete; soft-depends on US1 for end-to-end demo but is independently testable via stub IR
- **US3 (Phase 5)**: Depends on Phase 2 complete; soft-depends on US1 for end-to-end demo but cache machinery is independently testable
- **Polish (Phase 6)**: Depends on at least US1 complete; tighter dependencies noted per-task

### User Story Independence

Each story can be implemented and tested with a stub `IrBuilder`. The plan's `llm_ir_importer_acceptance.rs` is structured as three subsets (US1, US2, US3 gates) that pass independently. US2 and US3 do not require US1's `MistralIrBuilder` to function — they exercise the IR cache, validator, partial-reindex, and repair UI against pre-recorded golden JSON.

### Within-Story Order

- Models / types before services
- Services before commands / endpoints
- Backend before frontend within a story (so the frontend has something to call)
- Tests for the story written alongside or just before the implementation that satisfies them

### Parallel Opportunities

- **Phase 1**: T002, T003, T004 in parallel (different files)
- **Phase 2**: T005 must land first (others reference the migration); T006, T007, T008, T010, T011, T012, T013, T014 then parallel
- **Phase 3 (US1)**: T015, T016, T017, T018, T025 parallel; T019→T020→T021→T022 serial; T024 serial after T021+T023; T026, T027, T028, T029 mostly parallel after T024
- **Phase 4 (US2)**: T032, T034, T035, T040, T042, T043, T045, T046 parallel (different files); T036→T037→T038 serial; T039 serial after T023+T036; T044, T047, T048 sequential at end
- **Phase 5 (US3)**: T050, T052, T053, T055 parallel; T051, T054, T056 serial
- **Phase 6**: T057, T058, T059, T060, T062 parallel; T061 last

---

## Parallel Example: Phase 2 fan-out

```text
After T005 (migration) merges:

# Parallel batch — all touch different files, all depend only on T005:
Task: T006 — write src-tauri/src/services/doc_ir/schema.json
Task: T007 — write src-tauri/src/services/doc_ir/types.rs
Task: T008 — write validator type skeleton in validator.rs
Task: T011 — write builder trait in builder.rs
Task: T012 — write progress sink in progress.rs
Task: T013 — write converter_from_constitution.rs
Task: T014 — write src-tauri/src/db/repositories/ir_cache.rs

# Then sequentially:
Task: T009 — implement validate_doc_ir (depends on T006+T007+T008)
Task: T010 — write ir_validator.rs tests (depends on T009)
```

---

## Implementation Strategy

### MVP first (US1 only)

1. Phases 1 + 2 complete (30 mins of cargo green checks)
2. Phase 3 — implement, end with `cargo test --test llm_ir_importer_acceptance -- p1_` green
3. **STOP. Validate**: drop a Sindh PDF, observe correct indexing; toggle GGUF away, observe rule-based fallback + one-time notification.
4. Demo / merge to master as MVP.

### Incremental delivery

1. MVP (US1) → merge → users get auto-import for structured PDFs
2. US2 (repair UI) → merge → users get incremental quality
3. US3 (cache + sweep) → merge → users get speed + storage hygiene

Each phase is an independently-shippable feature increment per Constitution VIII.

### Solo / pair strategy

Realistically one developer. Execute phases sequentially; within a phase, batch all `[P]` tasks in one session, then walk the serial chain. Expect Phase 2 in one sitting, Phase 3 across 2–3 sittings (Mistral integration is the heavy part), Phase 4 across 2 sittings (split-screen UI), Phase 5 in one sitting.

---

## Notes

- Tasks with `[P]` touch disjoint files and have no incomplete prerequisites — safe to batch.
- Each `[Story]` task maps cleanly to a single user story for traceability against spec.md.
- Phase 2 is the only true blocker — once green, the three user stories proceed independently.
- Existing services (`indexer.rs`, `scheduler.rs`, `file_watcher.rs`, `structure_parser.rs`, `graph_builder.rs`) are touched only minimally: dispatch hook, sweep tick, removal hook, subtree persist helper, no changes respectively. Smallest viable diff (CLAUDE.md).
- Avoid: a "let's also refactor constitution_ir.rs while we're here" temptation — explicitly out of scope per plan.md Complexity Tracking row 2.
