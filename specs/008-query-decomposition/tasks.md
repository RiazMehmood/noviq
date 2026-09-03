# Tasks: Query Decomposition for Multi-Fact Questions

**Input**: Design documents from `/specs/008-query-decomposition/`
**Prerequisites**: plan.md, spec.md, research.md, data-model.md, contracts/decompose-api.md

**Tests**: INCLUDED — the architect chose TDD. Every behavioural function gets a failing
unit test first (RED) before implementation (GREEN). Unit tests live inline in
`decompose.rs` under `#[cfg(test)] mod tests` (matching the `slots.rs` convention).

**Organization**: by user story. All four stories are P1. Foundational holds the shared
pure functions (gate, merge) and the dispatch orchestration skeleton that every story
needs; the two split branches are split across US1 (enumeration) and US2 (multi-article).

## Format: `[ID] [P?] [Story] Description`

- **[P]**: parallelizable (different file/region, no dependency on an incomplete task)
- Paths are absolute-from-repo-root; the only files touched are
  `src-tauri/src/services/intent_router/decompose.rs` (NEW) and
  `src-tauri/src/services/intent_router/mod.rs` (one hook).

---

## Phase 1: Setup (Shared Infrastructure)

**Purpose**: create the module and its types.

- [X] T001 Create `src-tauri/src/services/intent_router/decompose.rs` and register it with `mod decompose;` in `src-tauri/src/services/intent_router/mod.rs`.
- [X] T002 In `decompose.rs`, define the types from data-model.md: `Capability`, `SubQuestion`, `SubAnswer`, `MergedAnswer`, and `const MAX_SUBQUESTIONS: usize = 6;`. Reuse `query_router::Citation` (import; do not redefine).

---

## Phase 2: Foundational (Blocking Prerequisites)

**Purpose**: the capability gate, the deterministic merge, and the dispatch orchestration
skeleton — all shared by US1–US4. MUST complete before the story phases.

- [X] T003 [P] RED: in `decompose.rs` tests, add `gate_enabled_only_for_small_models` covering the FULL matrix (this is the single source of truth for the gate logic) — `Some(0.5)`→true, `Some(1.5)`→true, `Some(3.0)`→false, `Some(7.0)`→false, `None`+`{has_discrete_gpu:false, ram_gb:8}`→true, `None`+`{has_discrete_gpu:true}`→false, plus the `ECHO_DECOMPOSE=1`→true / `=0`→false override cases. Run; watch it fail (function absent).
- [X] T004 Implement `decomposition_enabled(model_size: Option<f32>, tier: &HardwareTier) -> bool` in `decompose.rs` per contract (env `ECHO_DECOMPOSE` override → size ≤1.5 → hardware fallback). Make T003 GREEN.
- [X] T005 [P] RED: add `merge_renumbers_citations_and_labels_blocks` — two `SubAnswer`s each with a local `[1]` must merge to blocks labelled in order with global `[1]`/`[2]`; `merge_synthesizes_marker_for_markerless_block` — a `SubAnswer` with NO `[n]` in text but a resolved citation gets a synthesized trailing `[n]`; plus `merge_renders_not_found_block_uncited` for a not-found sub-answer. Watch all fail.
- [X] T006 Implement `merge_subanswers(&[SubAnswer]) -> MergedAnswer` + private `remap_citations` in `decompose.rs` (one labelled block per answer; global sequential `[n]`; rewrite local markers; not-found verbatim, no fabricated cite). Make T005 GREEN.
- [X] T007 In `intent_router::dispatch` (`mod.rs`), add the orchestration skeleton: after `slots::extract`, compute `model_size` as `query_router::discover_local_gguf().and_then(|(_, label)| label.parse::<f32>().ok())` (the label already carries the size; `parse_param_size_b` is private to query_router — do NOT call it), then `decomposition_enabled(...)`; if enabled, call `decompose::detect_and_split(&query.text, &slots)` (stub returning `None` for now), and when it returns `Some(subs)` with `len>=2`, loop **sequentially** over `route_scoped_with_override(sq.text, …)` (shared cancel token; return cancelled `ChatResponse` on cancel — FR-010), collect `SubAnswer`s (citations are `query_router::Citation`), call `merge_subanswers`, then map the merged citations via the existing `qr_citation_to_chat` when building `ChatResponse`. Otherwise the existing single call runs unchanged.
- [X] T008 Add `detect_and_split(query, slots) -> Option<Vec<SubQuestion>>` stub in `decompose.rs` returning `None`; confirm `cargo test -p echo-ai --lib intent_router` is green (21/21 + new) and the single-pass path compiles/works (skeleton inert until US1/US2).

**Checkpoint**: gate + merge proven by unit tests; dispatch wired but inert (every query still single-passes because the splitter is a stub).

---

## Phase 3: User Story 1 — Enumerated `(a)(b)(c)` questions (Priority: P1) 🎯 MVP

**Goal**: a question with ≥2 ordered lettered markers is split into one sub-question per
facet (stem + facet), answered separately, merged into labelled cited blocks.

**Independent test**: ask the Q15 Principles-of-Policy `(a)(b)(c)` question → trace shows
3 sub-questions; merged answer states Article 31 / 32 / 39, one labelled block each.

- [X] T009 [P] [US1] RED: in `decompose.rs` tests, add `enumeration_splits_into_three_subquestions` — `detect_and_split` on the Q15 string returns `Some` of 3 `SubQuestion`s, labels `(a)/(b)/(c)`, each `text` = stem + that facet (e.g. "…dealing with the Islamic way of life"). Watch it fail (stub returns None).
- [X] T010 [US1] Implement the enumeration branch in `detect_and_split`: regex for ≥2 ordered `(\([a-z]\))` markers; `stem` = text before first marker; facet = text between markers; build `SubQuestion{label, text}`. Respect `MAX_SUBQUESTIONS` (over-cap → `None`). Make T009 GREEN.
- [X] T011 [P] [US1] RED→GREEN: add `enumeration_single_marker_falls_back` (a string with only `(a)` → `None`) and `enumeration_ignored_when_subclause_numbering` (e.g. "Article 7(1)(a)" alone does not trigger) to lock the false-positive boundary.

**Checkpoint**: US1 independently testable end-to-end via the Q15 quickstart step.

---

## Phase 4: User Story 2 — Multi-article questions (Priority: P1)

**Goal**: a question naming ≥2 provisions is split one sub-question per provision,
answered from that provision, merged into labelled cited blocks; an absent provision is
reported not-found.

**Independent test**: ask the Q9 Article 23/24 question → 2 sub-questions; Article 23 =
property, Article 24 = its protection, neither conflated.

- [X] T012 [P] [US2] RED: add `multi_article_splits_per_provision` — `detect_and_split` with `slots.section_paths = ["Article 23","Article 24"]` returns 2 `SubQuestion`s, each labelled by its provision and text covering that clause. Watch it fail.
- [X] T013 [US2] Implement the multi-article branch in `detect_and_split`: trigger on `slots.section_paths.len() >= 2`; split the query on clause conjunctions (`, and `/`; `/` and `), keep clauses containing a section ref; if clause-count ≠ provision-count, fall back to per-article template `"What does {Article N} provide?"`. Enumeration branch takes precedence when both match. Make T012 GREEN.
- [X] T014 [P] [US2] RED→GREEN: add `merge_reports_absent_provision_not_found` — a `SubAnswer` carrying the grounded not-found fallback renders that part verbatim and uncited in the merged output (FR-008), exercised through `merge_subanswers`.

**Checkpoint**: US1 + US2 both deliver correct multi-fact answers on small models.

---

## Phase 4b: User Story 2b — Single-article two-facet questions (Priority: P1)

**Goal**: a question asking for two facets of ONE provision (Q4 "two protections in
Article 14 concerning dignity and torture"; Q6 "freedom of assembly under Article 16, and
its condition") is split into two single-facet sub-questions. Conservative trigger only.

**Independent test**: ask Q4 → 2 sub-questions (dignity / torture); ask Q6 → 2
sub-questions (the freedom / its condition).

- [X] T021 [P] [US2] RED: add `single_article_two_facets_splits` (Q4 → 2 subs, texts name Article 14 + dignity/torture) and `single_article_and_its_qualifier_splits` (Q6 → 2 subs); plus `single_article_no_cue_no_split` ("about law and order" with no count cue → None). Watch fail.
- [X] T022 [US2] Implement `split_single_article_facets` in `decompose.rs`: require exactly one `section_path`; Pattern A = count cue (`two|three|both`) + `(?i)(?:concerning|about|regarding|relating to|as to) X and Y` (facets ≤6 words) → `"What does {Article} say about {facet}?"` ×2; Pattern B = `…, and its (condition|exception|limitation|proviso|restriction)` → main question + `"What {qualifier} applies to {Article}?"`. Wire as the lowest-precedence branch in `detect_and_split` (after enumeration, after multi-article). Make T021 GREEN.

---

## Phase 5: User Story 3 — Single-fact questions unaffected (Priority: P1)

**Goal**: prove the common path is byte-for-byte unchanged and fast.

**Independent test**: a single-fact question yields no split and the existing single pass.

- [X] T015 [P] [US3] RED→GREEN: add `single_fact_query_yields_no_split` — `detect_and_split` on "What does Article 9 guarantee about security of person?" (with its real single-element `slots`) returns `None`; and confirm `cargo test -p echo-ai --lib intent_router` stays green (no regression in slots/follow-up suites).

---

## Phase 6: User Story 4 — Capable deployments bypass (Priority: P1)

**Goal**: decomposition never triggers for ≥3B / GPU deployments or when overridden off.

**Independent test**: with a ≥3B model (or `ECHO_DECOMPOSE=0`), a multi-fact question is
answered in a single native pass.

- [X] T016 [US4] The gate predicate (incl. the `ECHO_DECOMPOSE` override and ≥3B→false) is fully unit-covered by T003 — no separate unit test of `dispatch` (it requires DB/embedder/model and cannot be a pure unit test). Confirm the T003 matrix asserts the bypass logic (≥3B / override-off ⇒ false ⇒ dispatch takes the single-pass branch by construction).
- [ ] T017 [US4] Manual (dispatch-level verification of the gate): per quickstart, run once with a `*0.5b*`/`*1.5b*` GGUF (decompose on) and once with `ECHO_DECOMPOSE=0` (off) on the Q15 question; confirm trace differs (3 sub-questions vs single pass). This is the verification for SC-006/SC-007 and for FR-010 cancellation (cancel a multi-fact query mid-run; confirm no partial answer commits). Record results in the eval notes.

---

## Phase 7: Polish & Cross-Cutting

- [X] T018 [P] Add a `QueryTrace` step in the decomposition loop ("Decomposition: N sub-questions" + per-sub note) so the trace shows the split (reuses existing trace API; no new fields).
- [X] T019 [P] Append a dated `CHANGELOG_DEV.md` entry (symptom/why, files+functions, gating + sequential-execution invariants, follow-ups).
- [ ] T020 Acceptance: rebuild (`npm run tauri dev`), re-run the 15-prompt fundamental_rights eval on the 0.5B; verify SC-001 (Q4/Q9/Q15 correct facts/figures), SC-002 (single-fact unchanged), SC-006 (gate off on big model). Save a V4 traces/analysis alongside V3.

---

## Dependencies & Execution Order

- **Setup (T001–T002)** → **Foundational (T003–T008)** → stories.
- **US1 (T009–T011)** and **US2 (T012–T014)** both depend only on Foundational; once
  T008's stub exists they can proceed in parallel (different branches of one function —
  coordinate edits to `detect_and_split` to avoid a merge conflict; otherwise sequential).
- **US3 (T015)** depends on Foundational + at least the stub; independent of US1/US2.
- **US4 (T016–T017)** depends on Foundational (gate + dispatch hook).
- **Polish (T018–T020)** last; T020 is the full acceptance gate.

## Parallel Opportunities

- T003 ∥ T005 (gate test ∥ merge test — different regions).
- After Foundational: T009 (US1 RED) ∥ T012 (US2 RED) ∥ T015 (US3) ∥ T016 (US4) — all
  test-writing tasks in different test fns. Implementation tasks T010/T013 both edit
  `detect_and_split`, so serialize those two.

## Implementation Strategy

- **MVP = US1** (the worst-failing, most-detectable `(a)(b)(c)` class — Q15). Shippable
  and demonstrable alone.
- Then **US2** (multi-article — Q9), **US3** (regression guard), **US4** (gate acceptance).
- TDD throughout: RED (watch fail) → GREEN (minimal) → next. No production code without a
  failing test first.

## Format validation

All tasks use `- [ ] [TaskID] [P?] [Story?] description + file path`; Setup/Foundational/
Polish carry no story label; US phases carry `[US1]`–`[US4]`.
</content>
