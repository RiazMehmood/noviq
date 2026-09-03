---
description: "Task list — Fast-Tier Ingestion Hardening (Citation-Grade)"
---

# Tasks: Fast-Tier Ingestion Hardening (Citation-Grade)

**Input**: Design documents from `/specs/013-ingestion-hardening/`
**Prerequisites**: plan.md, spec.md, research.md, data-model.md, contracts/ingestion-stages.md, quickstart.md

**Tests**: INCLUDED — the spec mandates a frozen-fixture regression (FR-012, SC-001…SC-009) and the
plan enforces `cargo test --lib` + the acceptance gate per phase.

**Organization**: Grouped by the plan's **LOCKED phase order** (clean signal → structure → vocabulary
→ boundaries → relationships), which is a hard constraint. Each phase is tagged with the user story it
serves. Unlike the generic template, phases here are *sequential* because P1 zoning must precede the P2
structural pass (clean body text before heading logic), so US2/US3 (P0/P1) physically land before
US1 (P2/P3). Each phase remains independently testable against the frozen fixture.

## Format: `[ID] [P?] [Story] Description`

- **[P]**: Can run in parallel (different files, no dependencies)
- **[Story]**: User story served (US1–US5); Setup/Foundational/Polish carry no story label
- All paths are repo-relative; backend lives under `src-tauri/src/services/`

## Phase → defect → story map

| Phase | Defects | Story | Goal |
|---|---|---|---|
| Foundational (P0) | D9, D12 | (US2/US3 support) | stop quote corruption; populate page_number |
| Phase 3 (P1) | D5, D6 | US2, US3 | geometry zoning: footnotes + footer page-numbers |
| Phase 4 (P2) | D2, D3, D4, D10 | US1 | structural tree (identifier + level only) |
| Phase 5 (P3) | D1 | US1 | vocabulary resolver (word, or unresolved) |
| Phase 6 (P4) | D7, D8 | US4 | clause + amendment-bracket boundaries |
| Phase 7 | FR-014/15 | US5 | capture amendment + cross-reference edges |

---

## Phase 1: Setup (Shared Infrastructure)

**Purpose**: Frozen-fixture harness so every later phase can diff against the baseline.

- [X] T001 Created `src-tauri/tests/ingestion_hardening_acceptance.rs` — runs the REAL deterministic `import`/`persist` pipeline on the `fundamental_rights` layout fixture (NO `#[ignore]` stubs; assertions live, filled this phase).
- [X] T002 [P] Added the source layout fixture `src-tauri/tests/fixtures/ingestion/fundamental_rights_layout.txt` (the `-layout` text drives the real pipeline; the frozen 177-chunk baseline diff is the `T042` manual re-index step, not a committed asset).
- [X] T003 [P] `norm_ws` whitespace-normalized verbatim-substring helper in the harness (used by SC-004/006).

---

## Phase 2: Foundational (P0 — D9, D12) — Blocking Prerequisites

**Purpose**: Two isolated, orthogonal fixes on the shared normalize/persist path that every story
flows through. Pulled forward because they don't interact with structure logic.

**⚠️ CRITICAL**: Complete before any user-story phase.

- [X] T004 D9: remove `"shallnot"` from `JOIN_TARGETS` and audit the full list in `src-tauri/src/services/text_normalizer.rs` (keep only true intra-word repairs e.g. "o ne"→"one", "sha ll"→"shall"; drop any entry fusing two real words across a legitimate space)
- [X] T005 [P] D9 guard test in `src-tauri/src/services/text_normalizer.rs` `#[cfg(test)]`: `repair_spaced_words` keeps "shall not" intact AND a property check that no rule turns a 2-token legal phrase into a non-dictionary single token (added `never_fuses_two_word_legal_phrase_into_one_token`)
- [X] T006 D12 (Approach B — path→offset map, no DocNode struct change): `build_tree_from_headings` now returns `(DocNode, HashMap<unit_path, offset>)`; `strip_chrome` returns chrome-aligned `cleaned_page_offsets` (`chrome.rs`, `page_for` made `pub`); `import()` builds `page_by_path` and stores it on `ImportOutcome`; added `persist_tree_with_pages` (sibling to `persist_tree`, other callers untouched) called from `structured_importer::persist`
- [X] T007 D12: `collect_chunks` stamps `page_number` from `page_by_path` keyed by `unit_path`, inheriting the nearest ancestor's page for split/merged/container nodes (NULL only when no mapped ancestor) — `structure_parser.rs`
- [ ] T008 [P] D12 test in `ingestion_hardening_acceptance.rs` (SC-005): every provision chunk has non-null `page_number` within `[1, page_count]` and non-decreasing in reading order — PENDING the fixture harness (T001–T003) + end-of-session build/re-index verify

**Checkpoint**: re-index control set → no `shallnot`; all provisions carry a valid page_number; verbatim-substring still holds.

---

## Phase 3: User Stories 2 & 3 — Geometry Zoning (P1 — D5, D6) (Priority: P1/P2)

**Goal**: Isolate footnotes (`block_class='footnote'`, retrievable) and footer page-numbers using
layout geometry — never a text regex — so the structural parser sees clean body text.

**Independent Test**: No provision chunk contains amendment-history text or a footer page integer;
footnote chunks exist, each with a page_number and an anchor (or `unresolved`).

### Tests for Phase 3 ⚠️ (write first, must fail)

- [~] T009 [P] [US2] Footnote-isolation test — UNIT-level done in `chrome.rs` (`footnote_apparatus_is_classified_footnote`, `footnotes_are_isolated_from_body_but_kept`, `ordinary_prose_saying_substituted_is_not_a_footnote`). Acceptance-harness (SC-003) version PENDING T001–T003.
- [~] T010 [P] [US3] Page-number-exclusion test — UNIT-level done in `chrome.rs` (`bare_page_number_run_across_pages_is_chrome`). Acceptance-harness version PENDING.
- [~] T010a [P] [US3] Bare-integer-survival test — UNIT-level done in `chrome.rs` (`lone_bare_integer_survives_as_content`, `legal_phrase_with_a_number_is_never_a_page_number`). Acceptance version PENDING.
- [ ] T010b [P] [US2] Running-header exclusion test (SC-003) — covered by existing `masthead_repeated_across_pages_is_chrome`; end-to-end acceptance assertion PENDING harness.

### Implementation for Phase 3 (Approach B — text patterns, fast path has no geometry)

- [X] T011 [US2] Added `BlockClass::Footnote` (+ `as_str` "footnote") and `ChromeReason::PageNumber` in `chrome.rs`; keep invariant (`has_provision_identity` → provision) preserved (checked first)
- [X] T012 [US3] Page-number band: `page_number_blocks` classifies digit-only lines that recur across ≥2 distinct pages (a footer run) as `Chrome(PageNumber)`. NOT a regex over numbers inside text — only a line that is ENTIRELY one integer; a lone bare integer survives — `chrome.rs`
- [X] T013 [US2] Footnote band: `is_footnote_apparatus` classifies amendment-history lines (amendment verb + instrument/effect token, or `w.e.f`) as `BlockClass::Footnote` — `chrome.rs` (Approach B: vocabulary, since fast path carries no `y_band`)
- [X] T014 [US2] `strip_chrome` separates footnote lines into `StripResult.footnotes` (removed from body, offsets realigned); added `persist_footnote_chunks` writing `block_class='footnote'`, `page_number`, KEPT retrievable (not boilerplate) — `chrome.rs` + `structured_importer.rs`
- [X] T015 [US2] Footnotes flagged `label_status='unresolved'` (marker anchor not yet resolved) — never anchored by same-page heuristic. Full marker→provision anchoring deferred to a refinement / US5 (Phase 7)

**Checkpoint**: footnotes isolated + retrievable; page-numbers gone from provisions; Doc 1 prose stays clean (SC-008).

---

## Phase 4: User Story 1 — Structural Tree (P2 — D2, D3, D4, D10) (Priority: P1) 🎯 MVP

**Goal**: Produce the correct hierarchy with identifier + `NodeLevel` only (no citation word):
all Articles 7–40 (incl. 10A/19A/25A), real Chapter nodes, catchline+body merged, correct `unit_type`.

**Independent Test**: Every Article 7–40 present as an Article node; Art 7 ∈ Part II; Art 8–28 ∈
Chapter 1; Art 29–40 ∈ Chapter 2; no orphan bare-label chunks; 25/25A flagged not reordered.

### Tests for Phase 4 ⚠️ (write first, must fail)

- [ ] T016 [P] [US1] Article-presence test (SC-001) in `ingestion_hardening_acceptance.rs`: all of 7,8,9,10,10A,11…19,19A,20…25,25A,26…40 present as `unit_type='article'` nodes, none absorbed into a neighbour
- [ ] T017 [P] [US1] Chapter-parentage test (SC-001) in `ingestion_hardening_acceptance.rs`: Art 7 parent = Part II; Art 8–28 parent-chain includes Chapter 1; Art 29–40 includes Chapter 2; no orphan bare-label chunk
- [ ] T018 [P] [US1] 25/25A anomaly test (SC-007) in `ingestion_hardening_acceptance.rs`: the out-of-order 25/25A case is `label_status='unresolved'`, NOT silently reordered

### Implementation for Phase 4

- [X] T019 [US1] Bare-number + alpha-suffix article detection — implemented as `(?P<num>\d+[A-Z]?)` in the `manual` profile patterns (captures 10A/19A/25A) — `src-tauri/src/services/doc_profiles/manual.rs`. Lib-tested (`manual_profile_captures_alpha_suffix_articles_and_arabic_chapters`).
- [X] T020 [US1] Guard article detection with sequence validation — already provided by `drop_out_of_sequence_articles` (Chapter reset + half-of-max restart heuristic); verified compatible with alpha-suffix ordinals — `src-tauri/src/services/heading_detector/regex_pass.rs`
- [X] T021 [US1] Chapter detection accepts arabic `CHAPTER 1.`/`2.` (and roman) + detection pattern — `src-tauri/src/services/doc_profiles/manual.rs`. Lib-tested.
- [X] T022 [US1] Catchline↔body merge — root-caused the lost Articles 9/33/34/35/39 to a catchline+body double-match collision; collapse consecutive same-label headings (`dedup_by`) in `scan_with_profile` so each provision is one Article node spanning catchline+body — `src-tauri/src/services/heading_detector/regex_pass.rs`. Lib-tested (`catchline_and_prose_body_collapse_to_one_article`). NOTE: catchline stored in node `text` (content), not yet split into a `title` metadata key — refinement.
- [X] T023 [US1] Correct `unit_type` — delivered by T022: recovered provisions are now `NodeLevel::Article` → `unit_type='article'` (were `clause`/unresolved only because no Article node existed). Existing `NodeLevel::unit_type()` mapping unchanged.
- [~] T024 [US1] 25/25A: flag-don't-correct — DEFERRED. Post-fix both 25 and 25A exist as Article nodes (were both missing); residual is only the order-ambiguity flag, which needs a `label_status` thread detection→DocNode→persist (more invasive). Refinement, not a correctness gap. — where source ordering is genuinely ambiguous, mark `label_status='unresolved'` rather than synthesize an order — `src-tauri/src/services/structure_parser.rs`

**Checkpoint**: full correct tree with identifiers; verbatim-substring holds; no citation word applied yet.

---

## Phase 5: User Story 1 — Vocabulary Resolver (P3 — D1) (Priority: P1)

**Goal**: Bind the citation word (Article/Section/Rule) by confidence layering; conflict → unresolved.
Stops the `generic.rs` "Section {num}" leak at the correct layer.

**Independent Test**: No chunk emits `Section N` for an Articles document; each emits `Article N` or
`unresolved`, never the wrong word.

### Tests for Phase 5 ⚠️ (write first, must fail)

- [ ] T025 [P] [US1] Vocabulary test (SC-002) in `ingestion_hardening_acceptance.rs`: zero chunks labelled `Section N`; all provision labels are `Article N` or `label_status='unresolved'` with `citation_unit` omitted

### Implementation for Phase 5

- [X] T026 [US1] `resolve_citation_unit(text, profile) -> CitationUnit{Word|Unresolved}` in `doc_profiles/mod.rs`, layered: profile default → internal linguistic evidence (≥2× dominance) → unresolved on conflict. Lib-tested ×3.
- [X] T027 [US1] No profile blindly mints the unit word anymore — `scan_with_profile_unit` re-mints Article/Clause labels under the resolved unit for ANY profile (generic + manual), superseding the per-profile `citation_fmt` default during indexing. `generic.rs`/`manual.rs` templates left intact as the default fed to the resolver.
- [X] T028 [P] [US1] Internal linguistic-evidence detector `unit_evidence` ("<Unit> N", "this <Unit>") in `doc_profiles/mod.rs`; unit-tested on the real fundamental_rights phrases ("Article 251", "this Article", "New Article 19A").
- [~] T029 [US1] Resolver wired at INDEX time — `structured_importer.rs` computes the unit, passes the override to `scan_with_profile_unit`; `Word(w)` applies the word, `Unresolved` keeps the default + logs. DEFERRED sub-step: on `Unresolved`, persist-flag the affected provision chunks `label_status='unresolved'` (currently only logged).

**Checkpoint**: correct citation word everywhere or explicit unresolved; US1 fully delivered.

---

## Phase 6: User Story 4 — Clause & Amendment Boundaries (P4 — D7, D8) (Priority: P2)

**Goal**: Split sub-clauses at correct boundaries and preserve amendment brackets verbatim, now that
input is clean.

**Independent Test**: Each sub-clause is its own unit ((a) doesn't swallow (b)); bracketed amendment
text appears verbatim.

### Tests for Phase 6 ⚠️ (write first, must fail)

- [X] T030 [P] [US4] Clause-boundary test — implemented as `clause_marker_split_keeps_subclauses_separate` (unit test in `structure_parser.rs`): `(a)` does not contain `(b)`'s opening even when `(b)` is amendment-bracketed and not blank-line-separated; `(3)` is its own piece. (Acceptance-harness SC version still pending T001–T003.)
- [X] T031 [P] [US4] Amendment-bracket test — `amendment_bracket_preserved_verbatim_through_split` (unit test): inline `1[ … ]` preserved exactly + every piece is a verbatim contiguous slice of source. (Acceptance-harness version pending harness.)

### Implementation for Phase 6

- [X] T032 [US4] Marker-aware splitter `split_at_clause_markers` — split a provision body at line-start `(2)(3)`, `(a)(b)`, `(i)(ii)` markers FIRST (before blank-line/sentence fallbacks), so a sub-clause never swallows the next — `src-tauri/src/services/structure_parser.rs::split_long_text`.
- [X] T033 [US4] Amendment-bracket handling — the marker regex tolerates a leading `[` (the live `2\n[(b) …` shape) so a bracketed sub-clause opens its own piece; `[ … ]` and the footnote-ref digit are kept VERBATIM in the body (pieces are contiguous slices; nothing tokenizes on `[`). NOTE: physically stripping/recording the footnote-ref digit into metadata (vs leaving it verbatim in content) is deferred — removing it would violate the G1 verbatim-substring invariant, so it needs a separate metadata path.

**Checkpoint**: clause counts match source; brackets verbatim; no regression in prior phases.

---

## Phase 7: User Story 5 — Amendment & Cross-Reference Capture (FR-014/15) (Priority: P3)

**Goal**: Capture amendment events and in-text cross-references into the existing graph store, verbatim
or flagged — capture only, no graph querying.

**Independent Test**: An Eighteenth-Amendment footnote → amendment event on the right provision; an
in-text "Article N" reference → `cross_reference` edge between the two provisions; zero synthesized
identifiers.

### Tests for Phase 7 ⚠️ (write first, must fail)

- [X] T034 [P] [US5] Cross-reference test — `graph_builder::article_refs_extracts_distinct_numbers` + `build_graph_covers_edge_types_with_no_dangling_endpoints` (existing) assert `cross_reference` edges + no dangling endpoints; unresolvable targets are simply NOT emitted (never invented). (Acceptance-harness SC-009 version pending T001–T003.)
- [X] T035 [P] [US5] Amendment-event test — `amendment_parser` unit tests (×5) cover kind + verbatim `by_instrument` + `w.e.f` date + non-amendment → None; anchor logic tested by `amendment_anchor_only_when_page_has_one_article`. (DB round-trip version deferred to the acceptance harness.)

### Implementation for Phase 7

- [X] T036 [US5] `cross_reference` edges — ALREADY built in `graph_builder::collect_edges` (unit → "Article N" it names, verbatim identifier; self-reference skipped; non-existent target not invented). Verified by existing tests; left intact.
- [X] T037 [US5] Deterministic amendment-event parser `legal_ir/amendment_parser.rs::parse_amendment` → `AmendmentEvent{kind ∈ inserted/substituted/repealed/omitted, by_instrument (verbatim), effective_date}`. NO model. NOTE: no `amended_by` graph edge — the instrument is not a `graph_nodes` row (`graph_edges.to_node` FK), so capture lives in `amendment_events` (consistent with the existing graph_builder decision).
- [X] T038 [US5] Wired into `structured_importer::persist` via `persist_amendment_events`: parse each isolated footnote, anchor to the SINGLE distinct Article on its page (`anchor_citation_for_page`), insert `amendment_events` row; ambiguous/absent anchor or unparseable footnote is SKIPPED (footnote already flagged `unresolved`) — never an invented link. CONSERVATIVE anchor (one-Article-per-page); richer footnote-marker-position anchoring is a follow-up.
- [~] T039 [US5] (conditional) Additive migration — NOT needed: `amendment_events` (node_id, kind, by_instrument, effective_date, source_page, raw_marker) already exists in `001_baseline.sql`; no new query column required. Left unwritten by design.

**Checkpoint**: relationships captured verbatim/flagged; graph store populated; no query-path changes.

---

## Phase 8: Polish & Cross-Cutting

**Purpose**: Final gate + bookkeeping per the modular protocol.

- [X] T040 Acceptance gate `cargo test --test ingestion_hardening_acceptance` asserts SC-001 (article presence + chapter parentage), SC-002 (no Section leak), SC-004/006 (verbatim-substring), SC-005 (page_number) + SC-009 (graph integrity, capture ran) on the REAL pipeline — no `#[ignore]` stubs.
- [X] T041 [P] Verbatim-substring covered by `sc004_006_every_chunk_is_verbatim_substring_of_source` (tree-wide) + full `cargo test --lib` confirmation each phase.
- [~] T041a [P] Fast-tier assertion — the harness runs the deterministic `import` (NO model) by construction, proving fast-tier ingestion. The `index_tier='fast'` log-marker/DB-flag check is index-orchestrator scope (`indexer.rs`), deferred to the live re-index (T042).
- [~] T042 Live re-index of the 3-doc control folder (force pending) + diff vs the Desktop baseline — MANUAL step, requires the running app/DB; NOT run this session. The acceptance harness now substitutes an automated end-to-end check for `fundamental_rights` (the hardest control doc).
- [X] T043 [P] Dated entries appended to `MODULAR_IMPROVEMENT_LOG.md` per phase (P2/P3/footnote/P4/US5/Phase8). Tracker xlsx update pending the live re-index numbers.

---

## Dependencies & Execution Order

### Phase Dependencies (LOCKED — sequential)

- **Setup (Phase 1)**: no dependencies.
- **Foundational P0 (Phase 2)**: after Setup. Blocks all later phases.
- **Phase 3 (P1 zoning)**: after P0. **Must precede Phase 4** (clean body text before heading logic).
- **Phase 4 (P2 tree)**: after Phase 3.
- **Phase 5 (P3 vocabulary)**: after Phase 4 (needs the structure tree).
- **Phase 6 (P4 boundaries)**: after Phase 3 (clean input); naturally after Phase 4/5.
- **Phase 7 (US5 capture)**: after Phase 3 (footnotes) + Phase 6 (brackets).
- **Phase 8 (Polish)**: after all desired phases.

### Within Each Phase

- Tests written first and FAIL before implementation.
- Regex/detection before tree assembly; tree before vocabulary; vocabulary before relationships.
- Re-index + fixture diff at each checkpoint before advancing.

### Parallel Opportunities

- T002/T003 (setup assets/helpers) in parallel.
- Within a phase, the `[P]` test tasks run in parallel (independent assertions in the same test file
  by distinct functions); implementation tasks touching the **same file** (e.g. T022/T023/T024 in
  `structure_parser.rs`) are sequential.
- Cross-phase parallelism is intentionally NOT used — the locked order forbids it.

---

## Implementation Strategy

### MVP scope

US1 (trustworthy provision identity) is the citation-grade core, but it depends on P0 + P1 clean
signal. **MVP = Phase 1 → Phase 2 (P0) → Phase 3 (P1) → Phase 4 (P2) → Phase 5 (P3)** — at that point
every Article is present, correctly numbered, nested, page-stamped, footnote-clean, and correctly
labelled or flagged. Phases 6–7 (US4, US5) are incremental value on top.

### Incremental delivery (per modular protocol)

1. P0 → re-index → fixture diff → log.
2. P1 → re-index → fixture diff → log.
3. P2 → P3 → US1 complete (MVP). Demo.
4. P4 (US4) → US5. Each phase keeps all prior phases green.

---

## Notes

- Deterministic fast tier only — **no LLM** anywhere in these tasks (FR-013).
- Invariants enforced every phase: verbatim-substring; flag-don't-fabricate (`unresolved`/NULL);
  additive migrations only; smallest viable diff.
- Record work in `MODULAR_IMPROVEMENT_LOG.md`, NOT `CHANGELOG_DEV.md`, for this workstream.
- Schema is already in place (page_number, block_class, label_status, raw_marker, boundary_confidence,
  graph_edges, amendment_events); T039 migration is conditional only.
