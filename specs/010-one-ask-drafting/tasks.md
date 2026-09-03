# Tasks: One-Ask Drafting

**Input**: Design documents from `/specs/010-one-ask-drafting/`
**Prerequisites**: plan.md, spec.md, research.md (R1–R9), data-model.md, contracts/one-ask-commands.md, quickstart.md, ADR-0013, ADR-0014

**Tests**: TDD per constitution/user request. All CI-runnable tests use canned writer output and fixture corpora — **no live LLM in any cargo test**. Live-model validation is a manual gate (Phase 7).

**Organization**: Phases by user story (US1 one-ask draft, US2 refuse-or-flag, US3 revision+resume, US4 progress/cancel). ADR-0014 seam rules bind every task: vector search ONLY via `retrieval.rs`; `one_ask_session` access ONLY via `state.rs`; ANSI-portable SQL (TEXT ISO timestamps, TEXT uuid ids); pool handles passed in, never constructed.

## Format: `[ID] [P?] [Story?] Description with file path`

All backend paths under `src-tauri/`, frontend under `src/`. Cargo commands assume the usual NTFS env (`CARGO_TARGET_DIR=/media/riaz/New Volume/echo-AI-target`, `CARGO_HOME` set).

---

## Phase 1: Setup

- [X] T001 Create module scaffolding: `src-tauri/src/services/drafting/one_ask/mod.rs` (declaring `intent`, `skeleton`, `research`, `compose`, `verify`, `state` submodules, empty), `src-tauri/src/services/drafting/retrieval.rs` (empty), wire both into `src-tauri/src/services/drafting/mod.rs`; `cargo check` green.
- [X] T002 Add additive migration `src-tauri/src/db/migrations/025_one_ask_session.sql` per data-model.md (CREATE TABLE IF NOT EXISTS one_ask_session — draft_id TEXT PK, state_json TEXT NOT NULL, updated_at TEXT NOT NULL); register in the migrations list the same way 024 was; do NOT bump TARGET_SCHEMA_VERSION beyond the established additive pattern.
- [X] T003 [P] Create fixture corpus `src-tauri/tests/fixtures/one_ask/corpus.md` — synthetic mini-corpus (3 fake "documents" with rule-numbered provisions, e.g. "Rule 7(2) … Delegation of Financial Powers", deterministic content, NO real gold text) + `src-tauri/tests/fixtures/one_ask/canned_writer/` directory with 5 per-section canned model outputs containing `[CITE:n]`/`[FIG:n]` markers, one malformed variant (stray "Article 99" + invented "Rs. 9 crore"), and one marker-free variant.

**Checkpoint**: `cargo check` green; migration applies on a fresh DB.

---

## Phase 2: Foundational (blocking all stories)

### retrieval.rs — the KNN seam (R1, ADR-0014) — gates everything downstream

- [X] T004 RED: write `knn_parity` test in `src-tauri/tests/one_ask_knn_parity.rs`: build an in-memory/temp SQLite DB with vec0 `chunk_embeddings` + `chunks` + `sources` fixture rows (deterministic small vectors), assert `retrieval::knn()` top-k ids+order ≡ brute-force `vec_distance_cosine ORDER BY` top-k. Test fails (module empty).
- [X] T005 GREEN: implement `src-tauri/src/services/drafting/retrieval.rs`: `pub struct Hit{chunk_id, source_id, file_name, file_path, distance}`; `pub async fn knn(pool, embedding:&[f32], k:usize, overfetch:usize) -> Result<Vec<Hit>,String>` — step 1 `SELECT chunk_id, distance FROM chunk_embeddings WHERE embedding MATCH ?1 AND k = ?2` (k×overfetch), step 2 hydrate+filter (`indexing_status='indexed'`, under a watched folder) preserving KNN order, truncate to k. If MATCH/k ranking diverges in T004, fall back per PHR-0002: MATCH over-fetch + in-Rust cosine re-rank. knn_parity green.
- [X] T006 [P] Add intent unit: RED tests then GREEN impl `src-tauri/src/services/drafting/one_ask/intent.rs` — `parse_intent(text) -> OneAskIntent` (regex_lite): figures (`Rs.?`/digits/decimal + billion|million|crore|lakh, verbatim capture), action keyword list, raw passthrough; tests: "4 billion" verbatim, multiple figures, no-figure sentence, empty→Err.
- [X] T007 [P] Implement `src-tauri/src/services/drafting/one_ask/skeleton.rs` — `SectionId` enum + `pub const SKELETON: [Section; 5]` per data-model (title, guidance, role_hints, wants_figures); unit test: 5 sections, order, FinancialAspect.wants_figures.
- [X] T008 [P] Implement state per data-model: `src-tauri/src/services/drafting/one_ask/state.rs` — serde `OneAskState`/`Status`/`OneAskIntent`/`Figure`/`ProvisionCard`/`Paragraph`/`Run` (tagged enum, `rename_all="camelCase"`), `save/load/delete(pool, draft_id)` against `one_ask_session` (ANSI SQL, ISO timestamps); roundtrip unit test incl. unknown-future-field tolerance (`#[serde(default)]`/version field).
- [X] T009 Make `resolve_provision_label` (and the provision-number regex helpers needed by verify) reachable from `one_ask`: change `pub(crate)`→`pub` (or re-export via `intent_router::mod`) in `src-tauri/src/services/intent_router/decompose.rs` — smallest visibility change, no logic edits; existing decompose tests stay green.

**Checkpoint**: foundation green — `cargo test --test one_ask_knn_parity` + new lib units pass; no story work started.

---

## Phase 3: User Story 1 — One sentence in, finished summary content out (P1) 🎯 MVP

**Goal**: officer sentence → researched, composed, verified 5-section numbered draft (middle content only).
**Independent test**: fixture corpus + canned writer ⇒ full pipeline produces verified paragraphs with zero unflagged foreign numbers; manual quickstart §1 with live corpus.

- [X] T010 [P] [US1] RED: research tests in `src-tauri/src/services/drafting/one_ask/research.rs` `#[cfg(test)]` — given a stub embedder + fixture pool: per-section queries built from intent.raw + role_hints; cards have verbatim excerpts (substring of chunk), labels via resolve_provision_label, dedupe across sections, cap 6/section.
- [X] T011 [US1] GREEN: implement `research.rs` — `pub async fn research(pool, embed:&dyn Fn(&str)->Result<Vec<f32>,String>, intent, skeleton) -> Result<Vec<ProvisionCard>,String>` using ONLY `retrieval::knn` (ADR-0014); embedder injected as closure/trait so tests need no fastembed.
- [X] T012 [P] [US1] RED: compose tests in `one_ask/compose.rs` `#[cfg(test)]` — `build_section_prompt(section, cards, figures)` contains [INST], guidance, numbered card list, FIG table, and the never-write-numbers instruction; empty-figures intent + FinancialAspect ⇒ prompt instructs "state that financial details are to be supplied; write NO figures" (vague-ask edge case — omit, never invent); `parse_markers(raw)` extracts ordered Prose/[CITE:n]/[FIG:n] runs from canned outputs (incl. marker-free → single Prose run; out-of-range n → Flag run).
- [X] T013 [US1] GREEN: implement `compose.rs` — prompt builder + `parse_markers(raw) -> Vec<RawRun>`; writer invocation behind `pub trait SectionWriter { fn write(&self, prompt:&str) -> Result<String,String> }` so tests use canned strings and production wraps the ModelSlot.
- [X] T014 [P] [US1] RED: verify tests in `one_ask/verify.rs` `#[cfg(test)]` using T003 canned outputs — splice produces Citation runs with verified=true only when quoted text is (whitespace/hyphen-normalised) substring of card excerpt; stray "Article 99"/"Rs. 9 crore" in prose ⇒ downgraded to Flag (data-model invariants 1–4); officer figures pass through verbatim as Figure runs; paragraph numbering continuous across sections.
- [X] T015 [US1] GREEN: implement `verify.rs` — `assemble_and_verify(sections_raw, cards, intent) -> Vec<Paragraph>` (splice at markers, verbatim gate, stray-number post-scan via the decompose regex + figure regex, Flag downgrade, numbering). Constitution §XI canonical citation form: card with resolvable hierarchical label ⇒ full label; card WITHOUT one (fast-indexed source) ⇒ `rendered_label` = instrument name + page/span ref + "hierarchical detail unavailable" disclosure — never a bare or invented hierarchy.
- [X] T016 [US1] Pipeline orchestrator in `one_ask/mod.rs`: `pub async fn run_pipeline(pool, embed, writer:&dyn SectionWriter, draft_id, on_stage:&dyn Fn(Stage)) -> Result<(),String>` — Researching→Composing(i)→Verifying→Ready with state saved via `state.rs` at every transition; cancellation checked between sections (token passed in); interrupted load → Failed{interrupted} per data-model.
- [X] T017 [US1] Tauri command `start_one_ask` in `src-tauri/src/commands/one_ask.rs` per contract: validations (non-empty, corpus indexed, tier can host 7B — reuse hardware_detector), create session row, append chat turns via `conversation::append_message`, register cancel token, spawn pipeline with production `SectionWriter` (Mistral via `discover_ir_builder_gguf` + ModelSlot, [INST], n_ctx 6144) and fastembed embedder; emit `drafting://one-ask-progress` + `drafting://one-ask-complete`; register command in `src-tauri/src/lib.rs`.
- [X] T018 [US1] Acceptance test `src-tauri/tests/one_ask_acceptance.rs` (canned writer, stub embedder, temp DB with fixture corpus): full run_pipeline ⇒ Ready state; assert: 5 sections present, numbered continuously, every Citation run verified, zero Prose runs matching provision-number/figure regex, "4 billion" appears as Figure run, no boilerplate strings (letterhead/signature) anywhere — covers SC-001/002 shape and US1 AC 1–3.
- [X] T019 [P] [US1] Frontend API `src/api/oneAsk.ts`: `startOneAsk`, `getOneAsk`, event subscriptions (`one-ask-progress`, `one-ask-complete`) with camelCase types mirroring data-model view model.
- [X] T020 [US1] Frontend `src/components/drafting/OneAskView.tsx`: ask input + submit; draft pane rendering paragraphs/runs (Citation hover → source_title+label+quoted, fast-tier citations show the §XI "hierarchical detail unavailable" disclosure; Flag visually distinct; Figure styled); **Copy** button serialising clean numbered text with flags as bracketed notices (FR-011). SolidJS rules: Show accessor pattern, onCleanup before await.
- [X] T021 [US1] Route the drafting tab to OneAskView and remove officer-path entries for TemplateManager/GapPanel/ChatSession (FR-015 — routing/nav only, component files retained); `npx tsc --noEmit` clean.

**Checkpoint**: US1 independently testable — `cargo test --test one_ask_acceptance` green + quickstart §1 manually runnable.

---

## Phase 4: User Story 2 — Refuse-or-flag when sources are missing (P1)

**Goal**: unsupported asks produce flags/refusal, never invented citations.
**Independent test**: off-corpus ask on fixture corpus ⇒ flags + zero fabricated numbers (SC-007).

- [ ] T022 [P] [US2] RED: extend `verify.rs` tests — section with zero cards ⇒ section yields a Flag-led paragraph ("no source available…") not silent omission; canned writer output citing [CITE:7] when only 3 cards exist ⇒ Flag.
- [ ] T023 [US2] GREEN: implement no-coverage handling in `research.rs`/`verify.rs` — empty card set for a section flows through compose (prompt says "no provisions available — write only officer-fact narrative, or state none") and verify emits the inline Flag; whole-ask zero cards across all sections ⇒ pipeline sets Failed{no_corpus_support} with plain-language reason (refusal path).
- [ ] T024 [US2] Extend `one_ask_acceptance.rs` with the unsupported-subject scenario (fixture corpus, canned outputs): assert explicit flags present, zero unflagged provision-numbers, refusal variant surfaces human-readable reason — SC-007 automated analogue.
- [ ] T025 [P] [US2] Frontend: flag styling + refusal state in `src/components/drafting/OneAskView.tsx` (distinct flag chip + tooltip reason; refusal renders as message with remediation, not an empty draft).

**Checkpoint**: US1+US2 ship together as the accuracy-complete MVP.

---

## Phase 5: User Story 3 — Conversational revision + resume (P2)

**Goal**: targeted per-section revision; full persistence across restart.
**Independent test**: revise para 3 ⇒ only owning section changes, citations re-verified; reload restores conversation+draft (quickstart §3).

- [ ] T026 [P] [US3] RED: revision-routing tests in `one_ask/mod.rs` `#[cfg(test)]` — `route_revision(instruction, paragraphs) -> Option<SectionId>`: "make paragraph 3 shorter"→owning section; "add the procurement angle"→best hint-token match; gibberish→None.
- [ ] T027 [US3] GREEN: implement deterministic `route_revision` (regex para-number + hint-token overlap; no model) + `run_revision(...)` re-running research(optional: only when instruction introduces new topic tokens)+compose+verify for the target section, preserving other paragraphs byte-identical, renumbering only if paragraph count changed.
- [ ] T028 [US3] Commands `revise_one_ask`, `get_one_ask`, `loadOneAskSessions`, `retry_one_ask`, `finalize_one_ask` in `src-tauri/src/commands/one_ask.rs` per contract (clarify-on-unparseable via chat message; finalized rejects revision with remediation; retry re-runs from stored intent); register in lib.rs.
- [ ] T029 [US3] Extend acceptance test: revision scenario (canned alternate section output) — only target section's runs differ, re-verification enforced, state persisted; interrupted-state scenario — state saved mid-Composing loads as Failed{interrupted}, retry path reaches Ready.
- [ ] T030 [P] [US3] Frontend: revision input (chat-style under the draft), session list/resume on drafting tab load (`loadOneAskSessions` + `getOneAsk`), finalize button + locked state, in `src/components/drafting/OneAskView.tsx` and `src/api/oneAsk.ts`.

**Checkpoint**: US3 independently testable on top of US1 (quickstart §3 runnable).

---

## Phase 6: User Story 4 — Progress visibility + cancel (P3)

**Goal**: staged progress, ≤10 s event gaps, clean cancel.
**Independent test**: quickstart §4; contract acceptance checks.

- [ ] T031 [P] [US4] RED: pipeline test — `on_stage` callback fires for every transition incl. per-section Composing{index,total,title}; cancel token set between sections ⇒ status Cancelled, terminal stage emitted, state saved.
- [ ] T032 [US4] GREEN: wire cancel-token registry (F006 pattern) into commands (`cancel_one_ask` per contract, idempotent) + heartbeat detail event at least every 10 s during a model call (timer task around `writer.write`); register command in lib.rs.
- [ ] T033 [P] [US4] Frontend: stage indicator (researching / composing i of 5 / verifying), heartbeat-driven "still working" detail, Cancel button, failed/cancelled states with retry, in `src/components/drafting/OneAskView.tsx`.

**Checkpoint**: all 4 stories complete against canned-model tests.

---

## Phase 7: Polish & live validation (manual gates — needs running app + 7B)

- [ ] T034 [P] Error remediation audit: missing GGUF, empty corpus, low-RAM tier each produce distinct human-readable message+remediation (constitution Development Constraints); covered by unit tests on the validation helpers in `commands/one_ask.rs`.
- [ ] T035 Run quickstart §1–§5 live (10-PDF Sindh corpus, real Mistral-7B): record the perf table — research ≤15 s (SC-005), first draft ≤15 min, revision ≤5 min (SC-003), max event gap ≤10 s; spot-check 3 citations verbatim against PDFs (SC-002). Fix what fails; re-run.
- [ ] T036 [P] Full regression: `cargo test --lib drafting` (009 suite stays green — backend untouched), all `--test one_ask_*`, `lgd_drafting_acceptance`, `npx tsc --noEmit`.
- [ ] T037 Append CHANGELOG_DEV.md entry (symptom/why = 009 template mismatch + 281 s scan; fix = 010 pipeline, files+functions; cross-cutting = ADR-0013/0014, KNN seam debt in query_router noted) and update memory per session rules.
- [ ] T038 Pilot evaluation (SC-004/SC-006 — needs the user/officer): run ≥3 realistic asks end-to-end on the live Sindh corpus; record per ask (a) wall-clock from sentence to officer-approved text incl. review (target ≤1 working day) and (b) structural-rework verdict yes/no (target ≥80% no rework); capture results in a dated report under `~/Desktop/Noviq Documents reports/` and summarise in CHANGELOG_DEV.md.

---

## Dependencies & execution order

```text
Phase 1 (T001–T003) → Phase 2 (T004→T005 gates retrieval; T006/T007/T008 [P]; T009)
  → US1 (T010→T011, T012→T013, T014→T015 [pairs parallel]; → T016 → T017/T018; T019 [P] → T020 → T021)
    → US2 (T022→T023→T024; T025 [P])         # builds on verify.rs + acceptance harness
    → US3 (T026→T027→T028→T029; T030 [P])    # builds on pipeline + commands
    → US4 (T031→T032; T033 [P])              # builds on pipeline events
      → Phase 7 (T034 [P], T035, T036 [P], T037)
```

- **knn_parity (T004/T005) blocks T011** and everything after — implement first (PHR-0002 risk gate).
- US2 depends on US1's verify/acceptance scaffolding (same files) — sequential after US1.
- US3 and US4 are independent of each other after US1; can be parallelized across sessions.

## Parallel opportunities

- Phase 2: T006, T007, T008 in parallel after T001/T002 (different files).
- US1: the three RED tasks T010/T012/T014 in parallel; T019 frontend API parallel to backend T016–T018.
- Frontend tasks (T020/T025/T030/T033) parallel to the next story's backend once their command contracts exist.

## Implementation strategy

**MVP = Phase 1 + 2 + US1 + US2** (accuracy gate is not optional — US2 ships with US1). Then US3 (revision/resume), then US4 (progress polish), then Phase 7 live gates. Each checkpoint leaves the tree green and the app demoable.

**Task counts**: Setup 3 · Foundational 6 · US1 12 · US2 4 · US3 5 · US4 3 · Polish 5 = **38 tasks**.
