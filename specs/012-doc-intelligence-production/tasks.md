# Tasks: Document-Intelligence Production Overhaul

**Feature:** `012-doc-intelligence-production` · **Branch:** `012-doc-intelligence-production`
**Input:** spec.md, plan.md, research.md, data-model.md, contracts/, quickstart.md

## Guardrails (apply to every task)
- **G1** — verbatim-substring invariant: every emitted number/quote/ref/cell is a verbatim substring of the source digital text (verify-gate enforced).
- **G2** — golden regression gate: frozen real-corpus fixtures (provision label, table rows, chrome exclusion) are the release gate; run before every attended re-index.
- Refuse-or-flag over confidently-wrong; conservative keep-bias (a missed table/un-stripped chrome is acceptable; a scramble or a stripped real provision is not).
- Smallest viable diff; **NO per-document keyword lists**; **NO lopdf**.

## Build ops (from quickstart.md)
- Before any `cargo`: `ps -eo pid,ppid,args | grep '[c]argo'` → kill any rust-analyzer-parented cargo holding the target lock.
- Env: `CARGO_HOME="/media/riaz/New Volume/.cargo"`. Prefer standalone-logic verification during iteration; one full build per wave slice-end.

---

## Phase 1: Setup (shared infrastructure)

- [X] T001 Added additive migration `003_chunks_provenance.sql` (`block_class`, `label_status`, `raw_marker`, `extraction_method`, `reconstruction_method`, `boundary_confidence` + `idx_chunks_block_class`). Reused existing `page_number`/`char_start`/`char_end` instead of new page/char_span columns.
- [~] T002 [P] G2 golden cases for chrome are inline as `chrome::tests` (pure, real shapes: masthead/P-a-g-e/contact/forms). Live-DB harness spanning provisions+tables deferred (needs a fixtures PG); pure cases cover US1 now.

**Checkpoint:** migration applies cleanly; harness compiles.

---

## Phase 2: Foundational (blocking prerequisites for US1/US2/US3/US5)

- [X] T003 Added `BlockClass { Provision, Chrome, TableRow }`, `ChromeReason`, `LabelStatus` enums (in `parsers/chrome.rs`) with serde + `as_str()` TEXT mapping matching migration `003`.
- [~] T004 `block_class`/`label_status` populated via targeted post-insert UPDATEs (`persist_chrome_chunks` sets `block_class='chrome'`+`is_boilerplate=TRUE`; `flag_unresolved_labels` sets `label_status='unresolved'`) — avoids the `NewChunk` ripple across `chunker`/`structure_parser`. Audit fields (`raw_marker`, `extraction_method`, `reconstruction_method`, `boundary_confidence`) columns exist; population deferred.

**Checkpoint:** `cargo test --lib structured_importer` green; new columns populated with defaults; no behavior change yet.

---

## Phase 3: US1 — Upstream chrome-strip (P0, fixes D3) — Wave A

**Goal:** page chrome isolated as a non-provision class *before* tree-build so it never inherits a provision label.
**Independent test:** masthead `Rule 345(3)(b)` / "P a g e" / "Contact Us" → `block_class='chrome'`, excluded from retrieval; golden provisions unaffected.

- [X] T005 [US1] Created `parsers/chrome.rs`: pure `classify_blocks(blocks, total_pages)` with signal-1 `repeated_across_pages` (digit-stripped key, k=max(3,40%)), signal-2 margin geometry (geometry-optional `y_band`), signal-3 `is_form_field` (fill-blanks/dotted leaders, contact email/URL/phone shape, isolated date line, all-caps block). NO keyword lists. Provision-keep invariant enforced.
- [X] T006 [P] [US1] 14 unit tests in `parsers::chrome` (repetition, margin, form-field, provision-keep, threshold scaling, `strip_chrome` masthead-removal/keep/no-offsets) — all green.
- [X] T007 [US1] Wired via the text pipeline (not pdf.rs): `chrome::strip_chrome` runs in `structured_importer::import` (threaded `parsed.page_offsets` from `indexer`) BEFORE detection/tree-build; stripped chrome persisted as separate `block_class='chrome'`+`is_boilerplate=TRUE` chunks (kept, not deleted). Env-gated `ECHO_CHROME_STRIP`.
- [X] T008 [US1] Superseded the downstream flag: removed the persist-time `flag_boilerplate_chunks` call + `has_provision_identity` + `dry_run_boilerplate`/`BoilerplateCandidate`. `boilerplate.rs` kept (unused) for reference; migration `002` `is_boilerplate` column retained as the retrieval exclusion, now populated upstream from chrome.
- [~] T009 [P] [US1] Chrome golden cases inline in `chrome::tests` (masthead/`P a g e`/contact/forms). Live `Rule 345(3)(b)` masthead case to be confirmed at the Wave A re-index.

**Checkpoint:** `cargo test --lib parsers::chrome` + chrome golden fixtures green.

---

## Phase 4: US2 — Resolved labels + provenance (P0, fixes D5) — Wave A

**Goal:** `unit_label` is the resolved hierarchical citation; raw marker + provenance persisted; no silent synthetic id.
**Independent test:** no chunk cites a bare `(i)` or `m66`; ambiguous hierarchy → `label_status='unresolved'`.

- [X] T010 [US2] `structure_parser::resolve_bare_citations` runs in `build_tree_from_headings` before coalescing: a bare sub-marker citation is qualified by its nearest named ancestor (`(i)` under `Rule 27` → `Rule 27(i)`); a named provision is untouched; no named ancestor → left bare (flagged, never invented).
- [~] T011 [US2] Silent synthetic id pinned at `structure_parser.rs:420` (`/m{seq}` — an internal `unit_path` segment, NOT a rendered citation; merged-node citation already inherits the resolved parent). Net-new: `flag_unresolved_labels` sets `label_status='unresolved'` (+ warn) on chunks whose `unit_label` is still bare. The literal `/m{seq}` rename left as-is (cosmetic; not surfaced as a citation).
- [X] T012 [P] [US2] Unit tests added: `resolve_bare_citations_qualifies_against_hierarchy` + `resolve_bare_citations_leaves_orphan_marker_bare` (green). Full-suite compile confirmed `flag_unresolved_labels`/`persist_chrome_chunks` persist path.
- [~] T013 [P] [US2] Provision resolved-label cases via the pure resolve tests; live verbatim-content fixtures deferred to the Wave A re-index.

**Checkpoint:** `cargo test --lib structure_parser structured_importer` + provision golden fixtures green.

---

## Phase 5: US3 — Robust heading detection (P0, fixes D4) — Wave A

**Goal:** numbering-sequence prior + boundary-stop + label-content split action + confidence.
**Independent test:** golden headings incl. degraded `N. Title - body` detected; zero label≠content on golden provisions.

- [~] T014 [US3] Partially shipped already: `drop_out_of_sequence_articles` (sequence-prior restart-rejection) + `looks_like_year` guard + the bare-number `N. Title - body` detection are in `regex_pass.rs` (committed, tested). NET-NEW DEFERRED: `apply_sequence_prior` *recovery* of a missed heading + `confidence: f32` on `LevelledHeading`.
- [~] T015 [US3] DEFERRED — boundary-stop / mid-block recovery in tree-build (the existing sequence drop covers restart-rejection).
- [X] T016 [US3] `detect_label_mismatches` promoted from warning to a **flag action**: `structured_importer::flag_label_content_mismatches` sets `label_status='unresolved'` on any chunk whose content begins with a provision number disagreeing with its label (refuse-or-flag). Full re-segmentation split deferred (more invasive); flagging satisfies the spec's "low confidence → flag, not silent mislabel". Index-time.
- [~] T017 [P] [US3] Existing sequence/year tests green (`rules_profile_*`, `year_numbered_heading_*`); split-action/confidence tests deferred with T015/T016.
- [ ] T018 [P] [US3] DEFERRED with the live golden harness (T002).

**Checkpoint (Wave A gate):** full `cargo test --lib` = **467 passed, 0 failed** (was 451; +16: 14 chrome + 2 US2 resolve). US1 + US2-core GREEN. ▶ **NEXT = attended one re-index + full re-embed**, then live-verify per quickstart (chrome excluded, no bare `(i)`/`m66` surfaced, no `Rule <year>`, golden provisions label==content). US3 split-action/confidence is a small follow-up pass.

---

## Phase 6: US4 — Deterministic drafting (P0, fixes D7) — Wave B (parallel, NO re-index)

**Goal:** compose from fixed slot+template; LLM authors no paragraph prose; verify is a decidable whitelist check.
**Independent test:** zero fabricated glosses/advocacy/scaffolding; prayer one sentence; every number/quote/ref passes G1.

- [X] T019 [US4] Reused the existing US10 `SKELETON` (7 single-move sections) as the template frame set; slots are the function args of `compose_deterministic` (subject, cards, figures, urgency, stakeholders).
- [X] T020 [US4] `compose::compose_deterministic(section, subject, cards, figures, urgency, stakeholders) -> Vec<RawRun>` — fixed secretariat templates + verbatim slots, **no LLM-authored prose**; `[CITE:n]`/`[FIG:n]` markers placed deterministically, per-section cap honored. `run_pipeline` gained a `deterministic` param (production=true via `commands/one_ask.rs`); legacy model lane retained as fallback. Financial omit preserved in `mod.rs` (FR-022).
- [~] T021 [US4] Verification reuses `verify::assemble_and_verify` (the existing decidable gate: cites resolve to real section cards + verbatim splice, over-cap/0-cap dropped, non-existent flagged, foreign numbers in prose flagged). Since deterministic prose carries NO numbers/quotes, G1 holds by construction — a separate `verify_whitelist` was unnecessary (the existing gate is already decidable).
- [X] T022 [P] [US4] 6 unit tests in `compose::tests` (cite-cap, no-digit-in-prose, fig markers, absence-statement, one-sentence prayer, stakeholder join) + acceptance `deterministic_draft_never_calls_writer_and_verifies` (PanicWriter proves no LLM call).

**Checkpoint (Wave B gate):** `cargo test --lib` = **476 passed**; `cargo test --test one_ask_acceptance` = **4 passed** (incl. PanicWriter). NO re-index needed. ✅ DONE. Live: re-run the Canon-scanner ask to see the deterministic draft.

---

## Phase 7: US5 — Rust-native table engine (P1, fixes D1) — Wave C (one re-index)

**Goal:** projection-profile columns + cross-page stitch → verbatim row-units.
**Independent test:** Delegation-of-Financial-Powers rows discrete/in-order/correctly-labeled, every cell verbatim; non-table prose unchanged.

- [X] T023 [US5] `detect_columns` reimplemented as a **projection profile**: project every word's x-extent from tabular lines onto the x-axis, merge into occupied bands, split on whitespace gutters ≥ col_gap → column starts. Replaces the per-line cell-start clustering; `WordBox` intake + region detection kept.
- [X] T024 [US5] `build_rows` (shared: non-empty leading col starts a row; empty leading col = wrapped continuation; verbatim cells) + `detect_continuation(words, cols)` (relaxed, anchor-keyed cross-page parser) + the stitch loop in `reflow_tables` (carry column signature across `\x0C`, drop repeated header via `take_header`, **preserve continuation post-prose**).
- [X] T025 [US5] `reflow_tables` is confident-only: a page anchors with the strong ≥3-multi-line threshold, continuations align to its columns; any ambiguous region keeps the original `-layout` text. (Row-units flow through the existing chunk path; a dedicated `block_class='table_row'`/`reconstruction_method` tag is a follow-up — the reflowed text already de-garbles the rows.)
- [X] T026 [P] [US5] `parsers::table` tests: cross-page stitch (`cross_page_table_stitches_rows_across_break`), single-page projection (`delegation_grid_reflows...`), prose-unchanged, too-few-lines fallback — all green (477 lib total).
- [~] T027 [P] [US5] Delegation golden = the synthetic `delegation_words` + cross-page fixtures (pure); live Delegation-PDF verbatim-cell verification at the Wave C re-index.

**Checkpoint (Wave C gate):** `cargo test --lib` = **477 passed, 0 failed**; confident-only ⇒ no prose regression. ▶ **NEXT = one re-index**; verify the Delegation-of-Financial-Powers rows reconstruct (cross-page) and non-table prose is unchanged.

> ⚠️ **SUPERSEDED by live-verify (2026-06-18).** The Wave C re-index showed the recurrence/
> projection detector emits **confident garble** on the real Delegation schedules (121 chunks
> carry the linearizer signature; rows scrambled, headings linearized as rows, sparse `Extent`
> column dropped). Root cause + decisions in `docs/US5_table_reflow_diagnosis.md`,
> `docs/US5_table_reflow_design_review.md`, `docs/US5_table_engine_FINAL_DECISIONS.md`.
> Wave D below replaces this approach (delete recurrence → header+enumerator-anchored).

---

## Phase 9: US5 v2 — schema-anchored table engine + safety contract (P0, fixes D1 properly) — Wave D

**Goal:** stop confident corruption first (verifiable-function contract → fallback), then
reconstruct from the document's own repeated header + `(1)(2)(3)(4)` enumerator schema.
**Independent test:** real page-8 geometry (giant wrapped col-3, sparse `Extent`, heading-in-band)
either reconstructs verbatim & in-order OR falls back to `-layout`; **never** emits a scramble.
**Decisions:** `docs/US5_table_engine_FINAL_DECISIONS.md` §A/§B.

- [X] T031 [US5] Safety-contract tests in `parsers::table::tests`: `heading_inside_table_band_forces_fallback` + `non_monotonic_serials_force_fallback` — both assert byte-for-byte `-layout` fallback. Verified RED first (both failed: heading folded into row; serials emitted 1,3,2), now GREEN.
- [X] T032 [US5] `reflow_tables` is now a **total verifiable function** (FINAL §B): `verify_table(rows)` gates the emit before `linearize` — pass → emit, fail → keep the run's `-layout` text. Added `verify_table` (≥2 rows, strictly-increasing integer serials in col-0, reject `contains_heading_run` in any cell) + `parse_serial` + `contains_heading_run` (≥2 consecutive ALL-CAPS ≥4-letter words) in `src-tauri/src/services/parsers/table.rs`. `cargo test --lib parsers::table` = 7/7. (Heading-lexicon refinement folds into T038.)
- [X] T033 [P] [US5] Visibility: reconstructed row-units now tagged `block_class='table_row'` + `reconstruction_method='schema_anchored'` at persist time. `reflow_tables_with_rows` returns the emitted verbatim rows; threaded via new `ParsedDocument.reconstructed_rows` (pdf.rs reflow path; all other parsers `Vec::new()`) → `persist(…, reconstructed_rows)` → `tag_table_row_chunks` (UPDATE mirroring `persist_chrome_chunks`; rows normalized to match persisted content). Pure `chunk_is_table_row` (verbatim substring — rewrite-stable, normalize-safe) unit-tested. Kept retrievable (NOT boilerplate). Lib compiles; `parsers::table` 7/7 + `chunk_is_table_row` green. Takes effect on next re-index.
- [X] T034 [US5] **DELETED** the recurrence path: removed `detect_columns` (projection/frequency), `detect_continuation`, `line_aligns`, `take_header`, `starts_with_digit`, and the `head.cols` cross-page stitch loop in `reflow_tables`. `reflow_tables_with_rows` is now a simple per-page loop.
- [X] T035 [US5] Added `detect_schema(lines)` (enumerator `(1)…(N)` line whose preceding line is an N-cell header → ncols + labels), `group_cells` (header→cells), `body_column_lefts(anchor_rows, ncols)` (cluster serial-led rows only; keep N strongest left-edges; **no support threshold**; assign via `col_index`), `is_serial`. `detect_page_table` rewired: schema → serial-led anchor rows → body lefts → region-bound (serial/wrapped, STOP at non-serial leading col) → `build_rows` → `verify_table` → `linearize(schema.labels)`. `cargo test --lib parsers::table` = 8/8 (incl. `table_without_enumerator_falls_back` RED→GREEN + real page-8 geometry).
- [~] T036 [US5] DEFERRED (low value / risk-managed): a cell spilling the page break currently orphans its tail to pre-prose — **contract-safe, never scrambled**. Re-adding cross-page column state right before a single attended re-index is not worth the regression risk; revisit if live verify shows material loss.
- [~] T037 [P] [US5] `real_delegation_page8_reconstructs_first_row` (verbatim page-8 bbox: sparse Extent kept, col-3 wrap folded, monotonic 1..4) now passes under schema-anchoring; `heading_inside_table_band_forces_fallback` (COLLEGE EDUCATION DEPARTMENT) + `table_without_enumerator_falls_back` + `each_page_self_anchors_with_repeated_header` added. Live Delegation-PDF verbatim-cell golden at the next re-index.

**Checkpoint (Wave D-safety gate, ship-first):** after T031–T033, `cargo test --lib parsers::table` green; re-index ⇒ the 121 confident-garble chunks collapse to honest `-layout` fallback (verify via `reconstruction_method` populated; zero un-verified linearizer signatures). After T034–T037, schema-anchored reconstruction green on all 55 schedule pages.

---

## Phase 10: US6 — schedule / annexure boundary detection (P0, fixes label misattribution) — Wave D

**Goal:** `SECOND SCHEDULE / PART-I / ANNEXURE / APPENDIX / FORM` open new structural nodes and
**reset provision-label inheritance**, so schedule content stops inheriting the preceding `Rule`.
**Independent test:** no schedule/table chunk carries a `Rule N` label; Delegation Second Schedule
rows cite as `Second Schedule › Part-I › <Section>`. **Decisions:** FINAL §C.

- [X] T038 [US6] `scan_structural_boundaries` in `heading_detector/regex_pass.rs`: ALL-CAPS standalone-line lexicon (`FIRST..TENTH SCHEDULE`, `SCHEDULE/ANNEXURE/APPENDIX`, `PART-<n>`, `FORM <id>`) with heading-vs-reference disambiguation (the all-caps full-line anchor rejects mid-sentence mixed-case references). Merged into `scan_with_profile`. NO per-document keywords.
- [X] T039 [US6] Boundaries emitted at `NodeLevel::Part` (shallower than `Article`) → `build_tree_from_headings` opens a new container AND `drop_out_of_sequence_articles` resets the run; `resolve_bare_citations`'s nearest-named ancestor becomes the schedule, so schedule content stops inheriting `Rule N`. (Reused `Part` level instead of net-new node kinds — smallest viable diff, FINAL §C.4.) `titlecase_boundary` normalizes labels (`SECOND SCHEDULE`→`Second Schedule`, `PART-I`→`Part-I`).
- [X] T040 [P] [US6] `detects_schedule_boundaries_not_inline_references`: `SECOND SCHEDULE`/`PART-I` → Part-level boundaries; mid-sentence "Second Schedule" reference → none. Full `cargo test --lib` = 483 passed.

**Checkpoint:** `cargo test --lib structure_parser` green; re-index ⇒ assert `SELECT count(*) FROM chunks WHERE block_class IN ('table_row') AND unit_label LIKE 'Rule %'` = 0.

---

## Phase 11: US5 backstop — manual curation of critical schedules (P1) — Wave D

**Goal:** guarantee citation-grade on the financial tables now + golden ground-truth. **Decisions:** FINAL §D.

- [~] T041 [US5] DEFERRED (blocked on data): the sidecar mechanism is inert until curated content exists (T042), and adding untested plumbing before the single re-index adds risk for no behavior change. Implement together with T042 once the live re-index shows which schedule pages still need curation.
- [~] T042 [P] [US5] DEFERRED (needs manual data + post-reindex evidence): curate the Delegation Second Schedule after this re-index identifies the residual fallback pages; lock as golden. Pairs with T041.

---

## Phase 8: Polish & cross-cutting

- [X] T028 [P] Added `AND c.is_boilerplate = FALSE` to all 4 chat retrieval queries (FTS scoped/unscoped + vector scoped/unscoped) in `query_router.rs` — chrome excluded from chat ranking; `table_row` chunks (is_boilerplate=FALSE) stay retrievable.
- [ ] T029 [P] Append a dated `CHANGELOG_DEV.md` entry per shipped wave (symptom/why, files+functions, cross-cutting impact, follow-ups).
- [ ] T030 [P] Update MEMORY topic file `project_noviq_fragmentation_confirmed_blocker.md` resume state after each wave (waves done, next gate).

---

## Dependencies & execution order

- **Setup (T001–T002)** → **Foundational (T003–T004)** block everything.
- **Wave A** = US1 (T005–T009) → US2 (T010–T013) → US3 (T014–T018), then the **single re-index + re-embed** gate. US1/US2/US3 share the persist schema (Foundational); sequence them in this order (US2's resolution consumes US1's block class; US3's split writes `label_status`).
- **Wave B** = US4 (T019–T022) — independent of Wave A re-index; can run in parallel with Wave A coding (compose-time only).
- **Wave C** = US5 (T023–T027) — after Foundational; its own re-index. Independent of US4.
- **Polish (T028–T030)** after the relevant wave.

## Parallel opportunities
- T002 ∥ T001 (different files).
- Within each story, `[P]` test/fixture tasks (T006, T009, T012, T013, T017, T018, T022, T026, T027) run parallel to each other once their module exists.
- Wave B (US4) ∥ Wave A coding.
- Polish T028/T029/T030 are mutually `[P]`.

## Implementation strategy (MVP-first)
- **MVP = Wave A** (US1+US2+US3, one re-index): clean ingestion is the foundation everything else depends on and delivers the citation-grade chunk store on its own.
- Then **Wave B (US4)** to make drafting structurally non-fabricating (no re-index — fast win, shippable independently).
- Then **Wave C (US5)** for tables (gated by G1 + golden fixtures).
- KG deferred to Feature 013+.

## Task summary
- **Total:** 42 tasks. Setup 2 · Foundational 2 · US1 5 · US2 4 · US3 5 · US4 4 · US5 (Wave C) 5 · **US5 v2 (Wave D) 7 (T031–T037)** · **US6 schedule-boundaries 3 (T038–T040)** · **US5 curation 2 (T041–T042)** · Polish 3.
- **Re-index waves:** A (after T018), C (after T027 — superseded), **D (after T037 safety gate, again after T040 boundaries)**. Wave B no re-index.
- **Wave D ordering (FINAL_DECISIONS):** safety-first → T031–T033 (stop garble, ship-first) → T034–T037 (delete recurrence, schema-anchored) → T038–T040 (schedule boundaries) → T041–T042 (curation backstop).
