# Tasks: Re-chunk & Re-index for Citation-Grade Retrieval (010 extension)

**Extends**: Feature `010-one-ask-drafting` | **Branch**: `011-postgres-pgvector-port` (no new branch)
**Spec**: [spec-rechunk-reindex-extension.md](spec-rechunk-reindex-extension.md) | **Plan**: [plan-rechunk-reindex-extension.md](plan-rechunk-reindex-extension.md)

**Tech**: Rust 1.87 (`src-tauri`), existing services only (`structure_parser`, `text_normalizer`, `intent_router::decompose`, `indexer`, `fastembed`, bundled Postgres+pgvector). No new module, migration, or re-index mechanism.

**Tests**: TDD requested — unit tests (pure, no DB) on coalescing/repair/label + two goldens (Constitution no-regress; fragmented-rule → coherent unit). Build cadence per architect: batch a slice, ONE `cargo test` at slice end (NTFS target is slow).

**Conventions**: `[P]` = parallelizable (different file / no incomplete dep). `[US#]` maps to spec user stories.

---

## Phase 1: Setup

- [X] T001 Pin coalescing thresholds in `src-tauri/src/services/structure_parser.rs` as named consts (`COALESCE_MIN_LEAF`, `COALESCE_TARGET`, `COALESCE_HARD_MAX` ≈ 300/600/800 chars) with a doc-comment stating the SC-001 (median up, <50c share down) / SC-005 (no Constitution regression) validation method and that values are tuned empirically on a sample re-index.
- [X] T002 [P] ~~Add test fixtures under `src-tauri/tests/fixtures/rechunk/`~~ — superseded: the fragmented-rule and clean-multi-clause scenarios are encoded as inline test data in the `structure_parser` test module (`scrap_leaf`/`rule_parent` builders), which is more robust than `.txt` fixtures routed through the heading detector.

---

## Phase 2: Foundational

*(none — this extension is additive to the existing chunk-tree finalize step; no blocking infrastructure)*

---

## Phase 3: User Story 1 (coherent provisions) + User Story 4 (safe re-index) (Priority: P1) 🎯 MVP

> US1 (coalescing → coherent provisions) and US4 (applying it via a safe re-index) are co-P1 and share this phase: [US1] = chunking transform + goldens; [US4] = the re-index command, §V gate, and safety verification.

**Goal**: A provision spanning several markers becomes ONE coherent, correctly-bounded chunk (US1), and the improvement is applied to the live corpus via a safe re-index that keeps the old corpus usable and recovers from interruption (US4).

**Independent test**: Re-index the corpus; the Direct-Contracting rule retrieves as one coherent unit for the Canon-scanner query (SC-002); median chunk length rises and <50c share drops (SC-001); Constitution not regressed (SC-005); a query still works during re-index and an interrupted re-index recovers on next launch (SC-006).

### Tests (RED first)

- [X] T003 [P] [US1] Unit test in `src-tauri/src/services/structure_parser.rs` (`#[cfg(test)]`): `coalesce_undersized_leaves` merges adjacent under-`MIN` sibling leaves under the SAME parent up to `TARGET`, never merges across a structural/non-leaf boundary, never merges two distinct named provisions, leaves a genuinely short standalone provision intact, and never exceeds `HARD_MAX`. Expect FAIL until T006.
- [X] T003a [P] [US1] Regression assertion (FR-003): in the same test module, confirm an OVER-`HARD_MAX` leaf still splits via `split_oversized_leaves` AFTER coalesce is wired (split-then-coalesce must not disable oversize splitting). Expect FAIL until T006/T007.
- [X] T004 [P] [US1] Golden — covered by `structure_parser::tests::coalesce_merges_run_of_scrap_leaves_into_one_unit`: the Direct-Contracting scraps ("Direct Contracting" + "single source" + "obtainable only from one source") coalesce into ONE leaf inheriting the parent citation "Rule 42" (non-bare). Equivalent to the specced golden, exercised directly on the post-order transform.
- [X] T005 [P] [US1] Golden — covered by `structure_parser::tests::coalesce_does_not_merge_substantial_clauses_noregress` + `coalesce_leaves_isolated_short_clause_untouched`: substantial (≥min) sibling clauses are NOT merged and an isolated short clause is preserved — the no-over-merge / no-regress assertion (SC-005).

### Implementation

- [X] T006 [US1] Implement `coalesce_undersized_leaves(node, min, target, hard_max)` in `src-tauri/src/services/structure_parser.rs` as a post-order tree transform mirroring `split_oversized_leaves` (same-parent siblings only; boundary-safe; merged leaf inherits parent citation + `unit_path` `/m{n}`; idempotent on already-coherent trees).
- [X] T007 [US1] Wire `coalesce_undersized_leaves` into the tree-finalize step immediately AFTER `split_oversized_leaves` at every finalize call site in `structure_parser.rs` (≈ lines 147, 376, 470) so split-then-coalesce is the single chunking path.
- [ ] T008 [US4] Add a whole-corpus "Re-index corpus" command in `src-tauri/src/commands/` that reuses `indexer::do_index` (FK-safe chunk/embedding/graph wipe, `importer://progress` events, resumability); keep the prior corpus queryable until completion (FR-008/009); register in `lib.rs`.
- [ ] T009 [US4] Classify the re-index command per constitution §V (CAUTIOUS/DANGEROUS) and verify the safety classifier picks it up; require explicit user confirmation before wipe.
- [ ] T010 [P] [US4] Frontend: add a "Re-index corpus" action + progress display to Settings (`src/components/settings/…` + `src/api/…`), reusing the existing indexing progress channel; confirm `npx tsc --noEmit` clean.
- [ ] T011 [US1] GREEN (one cargo run): `cargo test --lib structure_parser` + `cargo test --test rechunk_fragmented_rule --test rechunk_constitution_noregress` all pass; tsc clean.
- [ ] T012 [US1] Live gate (user-attended): trigger re-index; confirm SC-001 (median length up, <50c share well under 20%), SC-002 (Canon-scanner query cites Direct-Contracting as a coherent unit — re-run the standing query and read `logs/dev-*.log` `one_ask research` lines), SC-005 (Constitution retrieval/labels not regressed).
- [ ] T012a [US4] Re-index safety gate (FR-008/009, SC-006, user-attended): during a re-index confirm a query still returns answers from the prior corpus (or a clear "indexing in progress" state); then kill the app mid-re-index and confirm on next launch the corpus recovers to a consistent state (prior corpus intact OR resume completes) — never a silently empty/half-rebuilt corpus presented as complete.

**Checkpoint**: MVP — fragmentation cured for retrieval; governing provisions are citable; re-index is safe (FR-009/SC-006 verified).

---

## Phase 4: User Story 2 — Clean, paste-ready quotes (Priority: P2)

**Goal**: Repair extraction artifacts so quoted text reads correctly, without changing the actual words.

**Independent test**: Re-index; previously-garbled citations read correctly in ≥90% of a sample (SC-003); the verbatim citation gate still holds (FR-011).

### Tests (RED first)

- [X] T013 [P] [US2] Unit test in `src-tauri/src/services/text_normalizer.rs` (`#[cfg(test)]`): intra-word/broken-line repair turns "o ne hundred"→"one hundred", "sha ll"→"shall", rejoins hyphen/line-broken words; is idempotent; leaves already-clean text byte-unchanged; does NOT join across legitimate word boundaries. Expect FAIL until T014.

### Implementation

- [X] T014 [US2] Extend `src-tauri/src/services/text_normalizer.rs` with a conservative, verbatim-safe intra-word-space / broken-line repair pass that only reforms splits into a known word/legal token; runs before structure detection; keep the existing ≤50-entry misread dictionary intact.
- [ ] T015 [US2] GREEN (one cargo run): `cargo test --lib text_normalizer` passes; re-index a sample and confirm SC-003 on a garbled-citation sample; confirm the one_ask verify gate still flags any unverifiable quote (FR-011) — no fabrication.

**Checkpoint**: Quotes are paste-ready; verbatim contract intact.

---

## Phase 5: User Story 3 — Correct hierarchical citation labels (Priority: P3)

**Goal**: Units carry their real provision label ("Rule 42(1)(ii)"), and a year in a heading is never mistaken for the provision number.

**Independent test**: Re-index; previously bare/year-like labels show the correct hierarchical reference in ≥90% of a sample (SC-004).

### Tests (RED first)

- [X] T016 [P] [US3] Unit test for label resolution (in `src-tauri/src/services/intent_router/decompose.rs` and/or `one_ask/research.rs::card_label` `#[cfg(test)]`): a sub-clause resolves to "Rule 42(1)(ii)" from its hierarchy; a heading containing a year ("…2017…", "…2010…") does NOT yield "Rule 2017"/"Section 2010". Expect FAIL until T017.

### Implementation

- [X] T017 [US3] Harden label capture: add a year-not-an-ordinal guard and hierarchical qualification in the `decompose` label helpers / `card_label` so bare sub-clause markers are qualified by their parent and years are never taken as provision numbers. (Hierarchical qualification already shipped in `card_label`; this adds the `looks_like_year` guard in `provision_label`.)
- [ ] T018 [US3] GREEN (one cargo run): label unit tests pass; re-index a sample and confirm SC-004 on a previously bare/year-like label sample.

**Checkpoint**: Labels are citation-grade.

---

## Phase 5b: User Story 5 — Corpus-rarity weighting of the officer's lexical terms (Priority: P2)

**Goal**: The lexical leg is built from the officer's rarest (most-discriminating) subject words so a coherent provision is not out-ranked by common-word boilerplate (`ts_rank_cd` has no IDF). Pure corpus-statistics; no hardcoded lists.

**Independent test**: Unit tests over document-frequency inputs confirm ubiquitous (>50% corpus) and zero-match terms are dropped, rarest are kept and capped, and a failed DF lookup falls back to the plain token query (SC-007 / FR-013).

### Tests (RED first)

- [X] T022 [P] [US5] Unit tests in `src-tauri/src/services/drafting/one_ask/research.rs` (`#[cfg(test)]`): `select_discriminative_terms` prefers rare, drops zero-df + ubiquitous (>50%), caps to `LEXICAL_MAX_TERMS`, and falls back to rarest-non-zero when all terms are ubiquitous.

### Implementation

- [X] T023 [US5] Split `lexical_query` into pure `lexical_tokens` + `select_discriminative_terms` (pure, DF-driven) + async `discriminative_lexical_query` (one corpus-DF lookup per draft via `unnest + plainto_tsquery`), with graceful fallback to the plain token OR on any DB hiccup. Wire into `research()` so the lexical leg uses the discriminative query (computed once, section-independent).
- [X] T024 [US5] GREEN: `cargo test --lib one_ask::research` passes (9/9, incl. 2 new discriminative-terms tests). Live SC-007 confirmation folded into the T012 re-index gate.

**Checkpoint**: Lexical leg favours the officer's discriminating words; retrieval degrades gracefully.

---

## Phase 5c: User Story 6 — Draft is about the officer's matter (Priority: P1)

**Goal**: The per-section writer prompt carries the officer's request as the subject so the draft is about THIS matter (Canon scanner / e-dafter / direct contracting), not a generic digest of the retrieved rules. Confirmed defect: `build_section_prompt` never received the ask (`mod.rs:113` dropped `st.intent`), so the subject's distinctive nouns appeared zero times.

**Independent test**: Prompt-construction unit test asserts the officer's subject is present and that a figure in the ask is neutralised (no raw number in the injected subject); live draft no longer omits the subject (SC-008 / FR-014).

### Tests (RED first)

- [ ] T025 [P] [US6] Unit tests in `src-tauri/src/services/drafting/one_ask/compose.rs` (`#[cfg(test)]`): `build_section_prompt` includes the officer's subject text and an instruction to write about THAT matter; `sanitized_subject` replaces a figure span ("Rs 2 million") with a neutral phrase (no digits) so the raw number is never injected as prose. Update the 2 existing prompt tests for the new signature. Expect FAIL until T026.

### Implementation

- [ ] T026 [US6] `compose.rs`: add `sanitized_subject(&OneAskIntent)` (strip/neutralise figure spans via `intent::figure_regex`) and thread the subject into `build_section_prompt` as a "Subject of this summary (every paragraph MUST be about THIS matter; provisions are its legal basis)" block + a guidance rule forbidding abstract rule-description. Wire the caller at `mod.rs` to compute the subject once and pass it per section.
- [ ] T027 [US6] GREEN: `cargo test --lib services::` passes (compose subject tests + no regression); live re-run of the Canon-scanner ask shows the subject woven through the draft (SC-008), with no raw figure leaked as prose (FR-011/FR-014).

**Checkpoint**: Drafts are about the officer's matter, not the rules in the abstract.

---

## Phase 6: Polish & Cross-Cutting

- [ ] T019 Decide re-index trigger scope (default: whole-corpus only; per-doc deferred) and finalize Settings UX copy ("old corpus stays available until the new one is ready").
- [ ] T020 Append a dated entry to `CHANGELOG_DEV.md` (coalescing + verbatim-safe repair + label fix + re-index-path reuse; cross-ref the fragmentation finding) and update memory.
- [ ] T021 Full suite (one cargo run): `cargo test` across `structure_parser`, `text_normalizer`, `decompose`, and the two goldens — all green; `npx tsc --noEmit` clean.

---

## Dependencies & Execution

- **T001** (thresholds) blocks T003/T003a/T006. **T002** (fixtures) blocks T004/T005/T012.
- **Phase 3 = US1 (coalescing: T003–T007, T011–T012) + US4 (safe re-index: T008–T010, T012a) is the MVP** and must land first — US2 and US3 validate against a re-indexed corpus produced by this pipeline. US4's re-index command (T008) is what applies US1's chunker to the live corpus, so they ship together.
- **US2 (T013–T015)** and **US3 (T016–T018)** are independent of each other; either may follow the MVP in either order.
- **Polish (T019–T021)** last.

**Parallel opportunities**: T003 / T003a / T004 / T005 together (distinct test scopes); T010 (frontend, [US4]) alongside T006–T009 (backend); within later slices T013 and T016 are independent.

**MVP scope**: Phase 3 (US1 + US4) delivers the core value — provisions become citable and are safely applied to the live corpus; ship it before US2/US3.
