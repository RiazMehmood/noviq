---
description: "Task list — Feature 007 Pakistan Government Legal RAG"
---

# Tasks: Pakistan Government Legal RAG

**Input**: Design documents from `/specs/007-legal-rag-pakistan/`
**Prerequisites**: spec.md, plan.md, research.md, data-model.md, contracts/, quickstart.md (all present)

**Tests**: TDD-style tests are included where the spec requires golden-corpus or eval-set verification (Slice 0 citation goldens, migration tests, slice-A re-index, slice-B/C/D evals, final acceptance gate). Pure helper functions do not get redundant test scaffolding.

**Organization**: Tasks are grouped by user story (US1 → US2 → US3). Setup (Phase 1) and Foundational (Phase 2) tasks are story-agnostic and unblock everything downstream. Polish (Phase 6) is the v1-ship acceptance + eval archive.

## Format: `[ID] [P?] [Story] Description`

- **[P]**: Can run in parallel (different files, no incomplete-task dependency)
- **[Story]**: User story label (US1/US2/US3). Setup, Foundational, and Polish phases carry no story label.
- Paths are absolute or repo-relative; the repo root is `/media/riaz/New Volume/echo-AI`.

---

## Phase 1: Setup (Slice 0 — Foundation)

**Purpose:** Branch, constitution, migration, citation module + 12 goldens, sidecar migration tool. No application behaviour changes. PR-sized.

- [X] T001 Verify branch `007-legal-rag-pakistan` is checked out and contains the latest spec/plan/research/data-model/contracts/quickstart (`git status`, `git log --oneline -10`).
- [X] T002 Append a Slice 0 dated entry to `CHANGELOG_DEV.md` at the repo root: symptom/why, fix, cross-cutting impact, follow-ups. See CLAUDE.md "Session-start mandatory".
- [X] T003 Create `src-tauri/src/db/migrations/020_legal_ir.sql` with the exact SQL from `specs/007-legal-rag-pakistan/data-model.md` (E1–E7): `ALTER TABLE sources ADD COLUMN index_tier`, `ALTER TABLE graph_nodes ADD COLUMN` for `instrument_id` / `legal_path` / `in_force` / `source_page`, four new tables (`amendment_events`, `legal_xrefs`, `procedures`, `procedure_steps`), all indexes. Renamed `documents → sources` (actual SQL identifier) and `graph_edges → legal_xrefs` (collision with mig-014).
- [X] T004 Bump `TARGET_SCHEMA_VERSION` 15 → 16 in `src-tauri/src/db/schema.rs:15`.
- [X] T005 Update `src-tauri/tests/schema_wipe_test.rs`: change expected schema-version assertion from 15 → 16.
- [X] T006 [P] Create `src-tauri/tests/migration_020_test.rs` per plan §Tests: assert `sources.index_tier` exists with NOT NULL + DEFAULT 'fast'; assert four new tables exist; assert CHECK constraints reject bogus `kind` / `edge_kind` / `index_tier` values. `cargo test --test migration_020_test` GREEN.
- [X] T007 [P] Create `src-tauri/src/services/citation/mod.rs` with `pub mod formatter; pub mod types;` re-exports for `LegalCitation`, `PathSegment`, `AmendmentNote`, `AmendmentKind`.
- [X] T008 [P] Create `src-tauri/src/services/citation/types.rs` with the data types: `LegalCitation`, `PathSegment` (13-variant enum), `AmendmentNote`, `AmendmentKind`. Includes `AmendmentKind::as_verb_past()` and `impl LegalCitation { format_officer, format_short }`.
- [X] T009 Add `pub mod citation;` to `src-tauri/src/services/mod.rs`.
- [X] T010 [P] Create `src-tauri/tests/citation_golden_test.rs` with 12 golden cases.
- [X] T011 Implement `src-tauri/src/services/citation/formatter.rs` with `format_path_segment_list`, `format_short_path`, `abbreviate_title`, `build_acronym`. `cargo test --test citation_golden_test` 12/12 GREEN.
- [X] T012 [P] Create `tools/migrate_generic_to_legal_ir.py`. Smoke-tested on synthetic two-node input — `legal_kind` and `legal_path` populated correctly.
- [X] T013 [P] SpecKit Plus index files already in place: `specs/007-legal-rag-pakistan/{spec,plan,tasks,research,data-model,quickstart}.md` + `contracts/` + `checklists/`.
- [ ] T014 Commit Slice 0 with `git commit -m "feat(007-slice-0): foundation — migration 020, citation module, schema bump"` (Co-Authored-By: Claude Opus 4.7).

**Slice 0 acceptance:** `cargo test --test citation_golden_test` 12/12 GREEN; `cargo test --test migration_020_test` GREEN; `cargo test --test schema_wipe_test` GREEN at v16.

---

## Phase 2: Foundational (Module skeletons + shared refactors)

**Purpose:** Set up module skeletons + the two cross-cutting refactors (`llm.rs` explicit Tier param, `indexer.rs` tier check) that every downstream user story depends on. No new behaviour; just structure.

- [X] T015 `src-tauri/src/services/legal_ir/mod.rs` created with `pub mod types/schema/prompts/amendment_parser/xref_extractor/procedure_extractor;` plus `pub use crate::services::doc_ir::{mistral_builder, progress_store};` re-exports. All submodule files are dated stubs that compile clean.
- [X] T016 `src-tauri/src/services/deep_reindex/mod.rs` created. `DeepReindexHandle` struct defined. `pub async fn run` returns `anyhow::bail!()` until Slice A T050. `chunks_replace.rs` stub present.
- [X] T017 `src-tauri/src/services/intent_router/mod.rs` + `types.rs` created. Full type set defined: `Intent` (9-variant), `Scope` (discriminated union), `Slots`, `ChatQuery`, `ChatResponse`, `ChatBubble`, `Citation` (with `from_legal()` helper routing through `services::citation`), `UpgradePrompt`, `FlowDiagram`, `FlowStep`. `dispatch` stub bails until Slice B T032.
- [X] T018 `intent_router/handlers/mod.rs` + 9 handler stub files (lookup/list_children/summarize/action_items/difference/cross_compare/timeline/procedure/fallback) created.
- [X] T019 `pub mod legal_ir; pub mod deep_reindex; pub mod intent_router;` added to `services/mod.rs`.
- [X] T020 `LlmTier` enum + `requires_cloud_opt_in()` helper added to `services/llm.rs` (constitution §I gate). Existing entry-point signatures left untouched — each Slice B/C/D handler will pass the explicit tier at its call site (minimal-diff per constitution Development Constraints). Full lib build still green; pre-existing tests unchanged.
- [X] T021 `indexer::do_index` reads `sources.index_tier` at the top of the function. Deep-tier branch logs a warning and falls through to fast for now (Slice A T050 wires the dispatch to `deep_reindex::run` once it's implemented). Behaviour unchanged for all current docs (default `fast`).

**Phase 2 acceptance:** repo compiles (`cargo build --manifest-path src-tauri/Cargo.toml`); all previously-passing tests still pass; no new behaviour introduced.

---

## Phase 3: User Story 1 (P1) — Officer asks plain questions of a freshly uploaded instrument

**Story goal (from spec §US1):** Officer uploads a Pakistan government PDF; within 60 s it is queryable; officer asks Lookup / ListChildren / Summarize / ActionItems and gets answers with citations to the right instrument + page/span.

**Independent test:** quickstart.md "Story 1" sequence end-to-end.

**Maps to design slice:** Slice B (intent router + 4 read-side intents).

### Tests-first for US1

- [X] T022 [P] [US1] Create `src-tauri/tests/intent_classifier_test.rs` — initial 40-prompt set (10 each for Lookup / ListChildren / Summarize / ActionItems) drawn from quickstart.md Story 1 + the prior 22-prompt eval pool. Each row is `(query_text, expected_intent, expected_slots_subset)`. The test asserts the rule-first classifier reaches the right intent on ≥ 75% of prompts on offline-mid; record per-row failures in a fixture file under `specs/007-legal-rag-pakistan/eval/us1_classifier_baseline.json`. Run — expect FAIL until T024/T025 land.
- [ ] T023 [P] [US1] Create `src-tauri/tests/us1_acceptance_test.rs` with five end-to-end checks corresponding to the five US1 acceptance scenarios in spec.md. Each fixture uses a small synthetic Pakistan-format PDF generated under `tests/fixtures/us1/`. Expect FAIL until T031–T036 land.

### Intent router classifier + slots

- [X] T024 [US1] Implement `src-tauri/src/services/intent_router/classifier.rs` rule-first pass per research §R9: regex patterns mapped to each intent (Lookup / ListChildren / Summarize / Difference / CrossCompare / ActionItems / Timeline / Procedure / Fallback). Return `IntentClassification` matching `contracts/intent-classifier.schema.json`. Confidence is 1.0 for unambiguous regex matches; 0.0 = fallback.
- [ ] T025 [US1] Implement small-LLM tiebreak path in the same file: when rule-first confidence < 0.6, dispatch to `LlmTier::OfflineSmall` (Qwen 0.5B) with the grammar from `contracts/intent-classifier.schema.json` via `llama-cpp-2` + `llguidance`. Reuse the sampler-factory closure pattern from `services::doc_ir::mistral_builder`.
- [X] T026 [US1] Implement `src-tauri/src/services/intent_router/slots.rs` slot extractor: instrument name (regex against known Pakistan-style titles + a small lookup of indexed instruments by short form), section_path (regex `(?i)\b(section|article|rule|clause)\s+\d+([a-z]?)(\(\d+\))?(\([a-z]+\))?(\([ivx]+\))?\b`), topic (everything-else heuristic), comparands (split on " and " / " vs "), timeline_anchor (regex on "history of …", "amendment to …").

### Per-intent handlers (US1's four)

- [ ] T027 [P] [US1] Implement `handlers/lookup.rs`: build retrieval query from `slots.instrument` + `slots.section_path`, run FTS5 + sqlite-vec ANN search scoped to the active `Scope`, format top-1 result as a `ChatBubble` with citation rendered via `services::citation::LegalCitation::format_officer`. On fast tier, citation includes "(page X)" and "hierarchical detail unavailable on fast tier" disclosure per FR-023.
- [ ] T028 [P] [US1] Implement `handlers/list_children.rs`: SQL `SELECT id, kind, number, title FROM graph_nodes WHERE document_id = ? AND parent_id = ? ORDER BY rowid`. Renders an ordered list with one citation per child. On fast tier where hierarchy is absent, fall back to TOC extraction (the existing structured-importer path from Feature 005).
- [ ] T029 [P] [US1] Implement `handlers/summarize.rs`: retrieve all chunks under the requested scope/path, dispatch to `LlmTier::OfflineSmall` (Qwen 3B) with a strict short-officer-summary prompt template. Cite the source path + page range.
- [ ] T030 [P] [US1] Implement `handlers/action_items.rs`: retrieve chunks under the requested scope, dispatch to `LlmTier::OfflineSmall` with an actor-grouped prompt template ("List duties, powers, timelines, penalties by actor"). Each line in the answer carries a citation to its source node/page.
- [ ] T031 [US1] Implement `handlers/fallback.rs`: when classifier confidence is low or the intent is `Fallback`, run a vanilla retrieval + summarize blend (today's behaviour) and surface a "did you mean: difference / timeline / procedure?" chip via `ChatBubble.see_also` repurposed with a special marker. No citation, but the chunk excerpt cited normally.

### Wire to Tauri commands + UI

- [X] T032 [US1] Implement the dispatch logic in `src-tauri/src/services/intent_router/mod.rs`: classify → route to handler → return `ChatResponse`. Honour `query.use_cloud` only when true (per constitution §I).
- [ ] T033 [US1] Extend `src-tauri/src/services/query_router.rs` to delegate to `intent_router::dispatch` instead of its current path. Keep existing public API stable so the frontend doesn't break; only the inner routing changes.
- [X] T034 [US1] Extend the existing `chat_query` Tauri command in `src-tauri/src/commands/` (or wherever it lives) to accept the new `ChatQueryInput` shape from `contracts/tauri-commands.md` C4: `text`, `scope`, `use_cloud?`, `thread_id?`. Return the full `ChatResponse`.
- [X] T035 [US1] Update `src/components/ChatBubble.tsx` to render the new `ChatBubble` shape: headline, body (markdown), citation block (calls a TS-side helper that just displays `citation.officer_format`), optional "see also" pills, optional `upgrade_prompt` chip (gated to US2 visibility — render the chip even if Deep re-index isn't wired in yet so US2 testing has a UI hook).
- [X] T036 [US1] Update `src/pages/Chat.tsx` to consume the new `ChatResponse` and pass scope state from the existing scope picker (FR-025). Verify the scope-switch round-trip stays under 2 s (SC-009) on a 50-doc corpus.

### Eval + US1 acceptance

- [ ] T037 [P] [US1] Hand-build 40-prompt officer eval set under `specs/007-legal-rag-pakistan/eval/us1_prompts.jsonl` (10 per intent across Lookup / ListChildren / Summarize / ActionItems), grounded on Sindh LG Act 2013, Sindh RTI Act 2017, and Sindh LG Rules 2018. Each row: `{query, intent, scope, expected_instrument, expected_path_or_page, expected_keywords}`.
- [ ] T038 [P] [US1] Create `tools/run_us1_eval.py` to drive the chat-query command (via `tauri` CLI bridge or a Rust test harness) across the 40-prompt set on each of `offline-small`, `offline-mid`, `cloud(groq)` baselines. Write a markdown report to `specs/007-legal-rag-pakistan/eval/us1_report.md` with per-intent + per-tier OK rates.
- [ ] T039 [US1] Run T038. Gate: offline-small ≥ 60% OK · offline-mid ≥ 75% OK · cloud ≥ 90% OK; citation accuracy ≥ 95% on Lookup. If any gate fails, iterate on the relevant handler (T027–T031) and rerun.
- [ ] T040 [US1] Run `cargo test --test us1_acceptance_test` — 5/5 GREEN.
- [ ] T041 [US1] Run `cargo test --test intent_classifier_test` — classifier passes 40-prompt baseline at ≥ 75% offline-mid.
- [ ] T042 [US1] Commit US1 with `git commit -m "feat(007-us1): fast-path intent router + Lookup/ListChildren/Summarize/ActionItems"`.

**US1 acceptance:** spec.md US1 acceptance scenarios 1–5 pass via quickstart.md Story 1 walkthrough; T039–T041 all green.

---

## Phase 4: User Story 2 (P2) — Officer requests a deeper view of an important instrument

**Story goal (from spec §US2):** Officer clicks "Deep re-index with Mistral" on a chosen document; pipeline runs reproducibly with progress + cancel; on completion the document gains full hierarchical citations and the amendment-aware Difference/Timeline answers become available. Sticky tier.

**Independent test:** quickstart.md "Story 2" sequence.

**Maps to design slice:** Slice A (legal IR + amendment chain + deep re-index orchestrator) + parts of Slice C (Difference amendment-flavor + Timeline amendment-flavor).

### Legal IR types + schema + prompts

- [ ] T043 [P] [US2] Implement `src-tauri/src/services/legal_ir/types.rs`: legal `NodeKind` enum (15 variants per data-model §E3); `LegalIRNode { id, kind, parent_id, number, title, text, source_page, in_force, amendments }`; `LegalIRDocument { schema_version: u32, instrument_title, instrument_id, year, nodes }`. Derive `Serialize / Deserialize`. Custom `Deserialize` for `NodeKind` to fall back to `Paragraph` on unknown variants (pattern proven in Feature 006).
- [ ] T044 [P] [US2] Implement `src-tauri/src/services/legal_ir/schema.rs`: load `contracts/legal-ir.schema.json` at compile time (`include_str!`) and expose as the llguidance JSON-schema string for grammar-constrained sampling.
- [ ] T045 [P] [US2] Implement `src-tauri/src/services/legal_ir/prompts.rs`: per-chunk Mistral prompt tuned for Pakistan-instrument shape (numbered Sections, Sub-sections in parens, Clauses `(a)`, schedule format, `X[…]` amendment markers preserved verbatim). Include the `[INST]` markers Mistral requires. Reuse the prompt-truncation guards from 006.

### Amendment parser (deterministic)

- [ ] T046 [P] [US2] Implement `src-tauri/src/services/legal_ir/amendment_parser.rs` per research §R6: deterministic regex pass producing `Vec<AmendmentEvent>` from a node's text. Patterns: `X[…]`, `[Substituted by Act … of …]`, `[Subs. by Act … of …]`, `[Inserted by Act … of …]`, `[Ins. by …]`, `[Omitted by …]`, `[Repealed by …]`. Unrecognized markers → `kind: 'inserted', by_instrument: 'UNKNOWN', raw_marker: <literal>`.
- [ ] T047 [P] [US2] Unit test `amendment_parser` against a fixture file with ≥ 20 hand-curated marker variants from Sindh Acts. Place fixture at `src-tauri/tests/fixtures/us2/amendment_markers.txt`. Assert ≥ 90% of obvious markers parse correctly into the expected `(kind, by_instrument)` shape.

### Deep re-index orchestrator + chunks replace

- [ ] T048 [US2] Implement `src-tauri/src/services/legal_ir/mistral_builder.rs` adapter: re-parameterize the inherited 006 `mistral_builder` with the legal schema (T044) and legal prompts (T045). Reuse `infer_with_grammar` and the 6-shape × strict/json5 tolerant parser.
- [ ] T049 [US2] Implement `src-tauri/src/services/deep_reindex/chunks_replace.rs`: in one SQL transaction — DELETE existing `chunks` / `chunk_embeddings` / `graph_nodes` for the doc, INSERT new chunks/nodes/embeddings derived from the legal IR. Section-boundary-respecting chunking per research §R5 (one chunk per Section, with sub-section merging when Section is short).
- [ ] T050 [US2] Implement `src-tauri/src/services/deep_reindex/mod.rs::run` orchestrator: classify (offline-small grammar) → IR build (offline-mid mistral_builder, per-chunk with retry at temperatures 0.4 / 0.7 / 0.9 from 006) → amendment_parser pass → write `amendment_events` rows → cache JSON IR sidecar to `{app_data_dir}/ir_cache/<sha256>.json` → `chunks_replace` → re-embed via existing embedder → set `documents.index_tier = 'deep'`. Use `progress_store` (006-inherited) for resumability between passes.
- [ ] T051 [US2] Emit `importer://progress` events per `contracts/tauri-commands.md` C1 at every phase boundary plus a `phase_tick` every ≤ 10 seconds during long phases. Honour the cancellation token between chunks and between passes (≤ 5 s to take effect per SC-003). On cancel, rollback any partial inserts and leave `documents.index_tier` at the prior value.
- [ ] T052 [US2] JSON IR sidecar drop-in path (research outstanding item resolution): in the upload flow, if a `<sha256>.json` exists in `ir_cache/` and parses against the legal-IR schema, route directly to `chunks_replace` + re-embed without running Mistral. Document at `documents.index_tier = 'deep'` immediately.

### Tauri commands for deep re-index

- [ ] T053 [US2] Create `src-tauri/src/commands/deep_reindex.rs` with `deep_reindex_document(doc_id, force: bool)`, `cancel_deep_reindex(job_id)`, `downgrade_document_to_fast(doc_id)` per `contracts/tauri-commands.md` C1–C3. Wire them in `src-tauri/src/lib.rs` and register in the Tauri builder. The downgrade handler runs a single SQL transaction that deletes all 007 deep-only rows scoped to the doc, sets `index_tier = 'fast'`, and re-runs `indexer::do_index`.
- [ ] T054 [US2] Add per-command Pending-Actions-Queue tiering per constitution §V: `deep_reindex_document` → CAUTIOUS (officer-initiated; cancellable); `downgrade_document_to_fast` → DANGEROUS (UI confirmation modal mandatory); `cancel_deep_reindex` → READ-ONLY. Verify the safety classifier picks them up correctly.

### Deep tier handlers (amendment-flavor only — instrument-flavor lands in US3)

- [ ] T055 [P] [US2] Implement `handlers/difference.rs` for the **amendment flavour**: given `slots.section_path` + `slots.instrument`, fetch the latest text of the section AND walk `amendment_events` rows to reconstruct the prior version (best-effort — note: full historical reconstruction is out-of-scope FR-030; what we render is the amendment diff between latest text and the immediately-prior-marker context preserved in `raw_marker`). Output a side-by-side `ChatBubble` body with two cited spans.
- [ ] T056 [P] [US2] Implement `handlers/timeline.rs` for the **amendment flavour**: given a section, fetch all `amendment_events` rows ordered chronologically; render as a numbered chat bubble timeline with one citation per event.
- [ ] T057 [US2] Upgrade `handlers/lookup.rs` (already implemented in US1) to detect when the active document is `deep` and render the full hierarchical citation (Chapter → Section → Sub-section → Clause), plus an amendment tail ("substituted by Act XII of 2021") when the matched node has amendments. The fast-tier path continues unchanged.
- [ ] T058 [US2] Upgrade `handlers/lookup.rs` further: when the query requires amendment data but the doc is `fast`, return a brief best-effort answer + populate `ChatResponse.upgrade_prompt` with `{ doc_ids: [doc_id], reason: "amendment history requires Deep re-index" }` per FR-017 / SC-010. Make the upgrade prompt mandatory (100%) in this branch.

### Deep re-index UI

- [ ] T059 [P] [US2] Create `src/components/DeepReindexProgress.tsx`: subscribes to the `importer://progress` event channel; renders per-phase progress bar (classify / ir_build / amendment_parse / xref_extract / procedure_extract / chunks_replace / reembed — though only the first three + chunks_replace + reembed run in US2; xref/procedure phases just emit `phase_done` immediately if not yet enabled). Cancel button calls `cancel_deep_reindex`. Modal-style overlay.
- [ ] T060 [P] [US2] Update `src/components/DocActions.tsx`: add "Deep re-index with Mistral" button per document card. Disabled when `index_tier === 'deep'` (replaced by "Re-run Mistral" + a "Downgrade to fast" secondary action with DANGEROUS confirmation modal per constitution §V).
- [ ] T061 [P] [US2] Frontend: render the `upgrade_prompt` chip from `ChatResponse` when present — clickable chip says "Deep re-index for full amendment history" and triggers `deep_reindex_document` for the named docs.

### Tests + eval + US2 acceptance

- [ ] T062 [P] [US2] Create `src-tauri/tests/legal_ir_slice_a_test.rs`: re-index Sindh LG Act 2013 + Sindh RTI Act 2017 + Sindh LG Rules 2018 (placed under `tests/fixtures/us2/sindh_corpus/`). Assertions: ≥ 90% of top-level Sections captured (vs hand-built index at `tests/fixtures/us2/golden_index.json`); 100% of obvious `X[…]` markers parsed; zero orphan nodes. Skip-with-warning if the fixture corpus isn't on the developer machine.
- [ ] T063 [P] [US2] Create `src-tauri/tests/us2_acceptance_test.rs` covering US2 acceptance scenarios 1–5 from spec.md (Deep re-index progress + cancel + atomicity; amendment diff; amendment history; sticky re-upload; upgrade prompt on fast-tier).
- [ ] T064 [US2] Hand-build 10-prompt amendment eval set at `specs/007-legal-rag-pakistan/eval/us2_amendment_prompts.jsonl`. Extend `tools/run_us1_eval.py` (or fork to `run_us2_eval.py`) to cover Difference and Timeline. Gate: amendment-diff ≥ 80% OK on offline-mid.
- [ ] T065 [US2] Run T062–T064. Iterate on `amendment_parser` regex widening (research §R6 says this is expected during Slice A) and `mistral_builder` prompt tuning until gates pass.
- [ ] T066 [US2] Run `cargo test --test legal_ir_slice_a_test --test us2_acceptance_test` — both GREEN.
- [ ] T067 [US2] Commit US2 with `git commit -m "feat(007-us2): legal IR + amendment chain + Deep re-index orchestrator + Difference/Timeline (amendment)"`.

**US2 acceptance:** spec.md US2 acceptance scenarios 1–5 pass; T065–T066 green.

---

## Phase 5: User Story 3 (P3) — Officer plans an approval path spanning multiple instruments

**Story goal (from spec §US3):** With ≥ 2 deep-indexed instruments, officer asks for an approval flow that spans an Act + its Rules; system returns ordered actor-step path stitched across instruments with citations per step. Also CrossCompare across N instruments and thematic Timeline.

**Independent test:** quickstart.md "Story 3" sequence.

**Maps to design slice:** rest of Slice C (xref + Difference-instrument + CrossCompare + Timeline-thematic) + Slice D (Procedure).

### Cross-reference graph extraction

- [ ] T068 [P] [US3] Implement `src-tauri/src/services/legal_ir/xref_extractor.rs` Stage 1 (regex). Phrase patterns: `(?i)\bsection\s+\d+\w*(?:\s*\([0-9a-z]+\))*\s+of\s+the\s+([\w\s,]+?act,?\s*\d{4})`, similarly for `Rule`, `under section ...`, `as amended by ...`. Each match produces a `GraphEdge` row with `edge_kind` derived from the surrounding phrasing.
- [ ] T069 [P] [US3] Implement Stage 2 (small-LLM disambiguator) in the same file: when a phrase resolves to multiple candidate `dst_node_id`s (e.g. two Acts with the same short title in scope), dispatch to `LlmTier::OfflineSmall` with a strict choice prompt. Confidence drops to the LLM-reported probability or 0.7 by default.
- [ ] T070 [US3] Wire `xref_extractor` into the deep_reindex orchestrator (T050) as a new pass after `amendment_parser`. Run only on deep-indexed nodes. Emit `importer://progress` with `phase: 'xref_extract'`.
- [ ] T071 [P] [US3] Create `src-tauri/tests/xref_extractor_test.rs`: 100-edge human-graded sample at `tests/fixtures/us3/xref_golden.jsonl`. Each row: `{src_text, expected_dst_node_id_pattern, expected_edge_kind}`. Gate: precision ≥ 85% (true-positives / total-extractions).

### Deep tier handlers — instrument flavours

- [ ] T072 [P] [US3] Implement `handlers/difference.rs` **instrument flavour**: given two `slots.comparands` (instrument names), retrieve top-k chunks from each on the active topic, then send to `LlmTier::OfflineMid` with a structured-comparison prompt template. Output a two-column `ChatBubble` body with one citation per row.
- [ ] T073 [P] [US3] Implement `handlers/cross_compare.rs`: variant of T072 for N comparands (≤ 8 per the classifier schema). Parallel retrieval per instrument; zipper-merge into a structured comparison table.
- [ ] T074 [P] [US3] Implement `handlers/timeline.rs` **thematic flavour**: given `slots.topic` (e.g. "right to information"), retrieve instruments whose `instrument_id` or `title` matches the topic context across the scope, order by enactment `year`. Render as a horizontal timeline with one citation per instrument.
- [ ] T075 [US3] Implement the "see also" surface in handlers: after building the main `ChatBubble`, run a `graph_edges` query for `cites`/`implements`/`under` edges connected to any node returned and populate `ChatBubble.see_also` with the top 3 most-confident edges, each as a `Citation`.

### Procedure extraction + handler

- [ ] T076 [P] [US3] Implement `src-tauri/src/services/legal_ir/procedure_extractor.rs` Stage 1 (heuristic procedural-section classifier): title keywords ("procedure", "manner of", "application for", "grant of", "approval of"), structural cues (nested-list-heavy sections with imperative verbs). Returns the candidate `Vec<node_id>`.
- [ ] T077 [P] [US3] Implement Stage 2 (LLM extraction) in the same file: for each candidate node, dispatch to `LlmTier::OfflineMid` with the grammar from `contracts/procedure.schema.json` and the node text + parent context. Map the resulting `ProcedureExtraction.steps[]` to `procedures` + `procedure_steps` rows. Resolve `source_node_id` by joining on the node's `legal_path`.
- [ ] T078 [US3] Implement cross-instrument stitching in `procedure_extractor`: when a step's text references "in accordance with the Rules" or similar, walk `graph_edges` `implements` to find the Rules counterpart and inline its steps into the parent procedure (Procedure → ProcedureStep rows that point to a different instrument's section). Mark stitched steps with their distinct `source_node_id`.
- [ ] T079 [US3] Wire `procedure_extractor` into deep_reindex orchestrator (T050) as the last pass before `chunks_replace`. Emit `phase: 'procedure_extract'` progress events.
- [ ] T080 [US3] Implement `handlers/procedure.rs`: given `slots.topic` (e.g. "arms license approval"), find the matching `procedures.process_name` (fuzzy match) under scope, fetch all `procedure_steps` ordered by `step_order`, build a `FlowDiagram` with linear `steps[]` and empty `branches[]` (officer expands on demand via a separate UI action).
- [ ] T081 [P] [US3] Frontend: create `src/components/FlowDiagram.tsx`. Linear render: numbered actor → action rows, with condition + timeline as italic annotations, citation pill per row. "Show branches" button calls a follow-up Tauri command (or just re-renders with `step.branches` materialized from the existing `procedure_steps.branch_of_step_id` data). Branching view = collapsible tree.
- [ ] T082 [P] [US3] Update `src/components/ChatBubble.tsx` to render `bubble.flow` as a `<FlowDiagram>` when present.

### Tests + eval + US3 acceptance

- [ ] T083 [P] [US3] Create `src-tauri/tests/procedure_extractor_test.rs`: 15 procedure queries built from Sindh corpus (license issuance, RTI request flow, NOC, building plan approval, etc.) at `tests/fixtures/us3/procedure_prompts.jsonl`. Gate: ≥ 70% return a procedure with all key steps + correct actors + correct citation per step.
- [ ] T084 [P] [US3] Extend the eval pipeline (`tools/run_us2_eval.py` or new `tools/run_us3_eval.py`) for the 30 new US3 prompts (10 Difference-instrument, 10 CrossCompare, 10 Timeline-thematic) at `specs/007-legal-rag-pakistan/eval/us3_prompts.jsonl`. Write the report to `specs/007-legal-rag-pakistan/eval/us3_report.md`.
- [ ] T085 [P] [US3] Create `src-tauri/tests/us3_acceptance_test.rs` covering US3 acceptance scenarios 1–4 from spec.md (linear flow with citations; conditional branching expand; cross-compare across 3 instruments; missing-scope CrossCompare refusal).
- [ ] T086 [US3] Run T083–T085. Iterate on classifier + step-extraction prompts until gates pass.
- [ ] T087 [US3] Run `cargo test --test xref_extractor_test --test procedure_extractor_test --test us3_acceptance_test` — all GREEN.
- [ ] T088 [US3] Commit US3 with `git commit -m "feat(007-us3): xref graph + Difference-instrument + CrossCompare + Timeline-thematic + Procedure + FlowDiagram UI"`.

**US3 acceptance:** spec.md US3 acceptance scenarios 1–4 pass; T086–T087 green.

---

## Phase 6: Polish & Cross-Cutting Concerns

**Purpose:** Final v1 acceptance gate, full eval archive, CHANGELOG entry, manual memory follow-ups.

- [ ] T089 Create the final acceptance harness `src-tauri/tests/legal_rag_v1_acceptance.rs` with six gates:
   1. Migration 020 runs clean on a copy of master `echo.db`.
   2. Sindh LG Act + Sindh RTI Act + Sindh LG Rules re-index without error; node + edge + procedure counts match goldens within ±5%.
   3. Citation formatter produces correct officer-grade strings for 12 golden cases.
   4. Intent classifier ≥ 90% accuracy on 70-prompt eval (offline-mid).
   5. End-to-end: 5 hand-picked officer queries (one per major intent including Procedure) return correct answer + correct citation on offline-mid.
   6. **Network-isolation gate (SC-007).** The harness runs in a sandbox with outbound network blocked (Linux: `unshare -rn` wrapper invoked by the test runner, OR an in-process `socket2`-based interceptor that fails any TCP/UDP connect). Across gates 1–5 the test asserts ZERO outbound connection attempts. Cloud-tier code paths MUST be reachable only when `use_cloud == true` and are explicitly NOT exercised by this harness.
- [ ] T090 Run `cargo test --test legal_rag_v1_acceptance` — 5/5 GREEN.
- [ ] T091 [P] Archive the full 70-prompt evaluation report covering `offline-small` / `offline-mid` / `cloud(groq)` at `specs/007-legal-rag-pakistan/eval/v1_final_report.md`. This is not a CI gate; it's the evidence file referenced by the spec's SC-002 / SC-004 / SC-006 measurements.
- [ ] T092 [P] Append the Slice D + v1-ship dated entry to `CHANGELOG_DEV.md` per CLAUDE.md "Session-start mandatory": symptom/why, fix (every Tauri command + module + migration touched), cross-cutting impact (every UI component changed), follow-ups (≤ 3 items for v2: bilingual, LAN multi-officer, normalized actor vocab).
- [ ] T093 [P] Manual memory tree follow-ups (outside repo — see constitution v1.1.0 Sync Impact Report): prepend SUPERSEDED header to `~/.claude/.../memory/project_echoai_domain_scope.md`; prepend PARTIALLY SUPERSEDED header to `~/.claude/.../memory/feedback_llm_ir_only_pdf_scanned_images.md`; annotate the corresponding MEMORY.md index lines.
- [ ] T094 Open Pull Request from `007-legal-rag-pakistan` → `master` titled "Feature 007 — Pakistan Government Legal RAG (v1)". Body links the spec, plan, design doc, eval reports, and the final acceptance harness output.
- [ ] T095 After PR review + merge: tag the merge commit `007-legal-rag-v1` and push the tag.
- [ ] T096 [P] Settings UI for "use cloud LLM" toggle per FR-029: add a single boolean row to the existing Settings panel (`src/pages/Settings.tsx` or the project's existing settings surface) labelled "Use cloud LLM (for testing only)". Persist via the existing app-settings store. When the user clicks the row, fire a confirmation toast ("Cloud LLM enabled — queries will now route to the cloud"); when unticked, fire the inverse toast. Wire the resolved boolean into every `chat_query` invocation as `use_cloud`. NO per-query chip in `Chat.tsx`. Update `src/pages/Chat.tsx` to read the current setting at query time (single Tauri command call). Add a short integration test in `src-tauri/tests/settings_use_cloud_test.rs` that asserts: (a) when off, `chat_query` calls `LlmTier::OfflineSmall`/`OfflineMid`; (b) when on, `chat_query` calls `LlmTier::CloudEval`; (c) toggling the setting takes effect on the next query without restart.

---

## Dependencies

```
Phase 1 (Setup, Slice 0) ─────────────────┐
                                          │
Phase 2 (Foundational) ───────────────────┤
                                          │
                ┌─────────────────────────┴────────────────────────┐
                ▼                                                  ▼
Phase 3 (US1, fast-path queries)              Phase 4 (US2, Deep re-index + amendment)
                │                                                  │
                └─────────────────────┬────────────────────────────┘
                                      ▼
                          Phase 5 (US3, cross-doc + procedure)
                                      │
                                      ▼
                          Phase 6 (Polish + v1 acceptance + PR)
```

Within each phase, [P]-tagged tasks may run concurrently. Across phases:

- **Phase 1 must complete fully before Phase 2.** Migration 020 + citation module are foundational.
- **Phase 2 must complete fully before Phases 3, 4, 5.** Module skeletons + Tier param + indexer tier-check are prerequisites.
- **Phase 3 (US1) and Phase 4 (US2) can run in parallel** once Phase 2 is done — US1 only touches fast-path code + intent router; US2 only touches deep pipeline + amendment + chunks_replace. Both extend `handlers/lookup.rs` (T027 vs T057), so coordinate that file's history; otherwise non-overlapping.
- **Phase 5 (US3) depends on US2.** Cross-references and procedures only exist on deep-indexed instruments.
- **Phase 6 depends on US3.** Final acceptance gate runs against the complete feature.

## Parallel execution examples

**Within Phase 3 (US1)** — after T024–T026 land (classifier + slots), these four handlers are mutually independent (different files, no shared mutable state):
- T027 [P] handlers/lookup.rs
- T028 [P] handlers/list_children.rs
- T029 [P] handlers/summarize.rs
- T030 [P] handlers/action_items.rs

**Within Phase 4 (US2)** — after schema/types/prompts land (T043–T045), these are independent:
- T046 [P] amendment_parser
- T048 [US2] mistral_builder adapter (depends on T044/T045 only)
- T055 [P] handlers/difference.rs (amendment flavor) — depends on amendment_events table existing (T003), not on T046

**Within Phase 5 (US3)** — after xref_extractor lands (T068–T070):
- T072 [P] handlers/difference.rs (instrument flavor)
- T073 [P] handlers/cross_compare.rs
- T074 [P] handlers/timeline.rs (thematic flavor)
- T076 [P] procedure_extractor Stage 1

## Implementation strategy

**MVP slice = Phase 1 + Phase 2 + Phase 3 (US1).** A user can upload a Pakistan PDF and ask basic chat questions with citations within a fast-path-indexed view. This is the shippable smallest increment.

**Incremental from MVP:**
1. MVP → ship to a single officer for fast-path feedback.
2. Add Phase 4 (US2 deep re-index) → officer can selectively deep-index high-value Acts.
3. Add Phase 5 (US3 cross-doc + procedure) → officer can plan approval paths and run cross-comparisons.
4. Phase 6 final acceptance + PR to `master`.

This matches the project's "ship one thing at a time" feedback. Each user-story phase ends with a green test suite + an eval report, so any single phase merging alone leaves the product in a usable state.

## Task count summary

| Phase | Task IDs | Story | Count |
|---|---|---|---|
| 1 — Setup (Slice 0) | T001–T014 | — | 14 |
| 2 — Foundational | T015–T021 | — | 7 |
| 3 — US1 fast-path | T022–T042 | US1 | 21 |
| 4 — US2 deep re-index + amendment | T043–T067 | US2 | 25 |
| 5 — US3 cross-doc + procedure | T068–T088 | US3 | 21 |
| 6 — Polish + v1 ship | T089–T096 | — | 8 |
| **Total** | | | **96** |

Parallel opportunities: 39 of 96 tasks carry the `[P]` marker (file-independent). Approximate critical-path length (no parallelism gains assumed): ≈ 61 tasks.

Independent-test criteria per story are inlined in each phase header (citing the matching User Story + quickstart.md walkthrough).
