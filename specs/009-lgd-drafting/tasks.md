---
description: "Task list for Feature 009 — LGD Summary & Note Drafting"
---

# Tasks: LGD Summary & Note Drafting

**Input**: Design documents from `/specs/009-lgd-drafting/`
**Prerequisites**: plan.md, spec.md, research.md, data-model.md, contracts/drafting-commands.md
**ADRs**: ADR-0008 (deterministic citation integrity — cited-slot path), ADR-0011 (indexing routing), ADR-0007 (hierarchical/graph IR providing chunk labels)

**Tests**: Included for the citation-grade contract (deterministic, no-LLM-in-assertion) — the core acceptance gate. Broad UI tests are out of scope for the first cut.

**Scope note (2026-06-08)**: First cut focuses on **Summary/Proposal**. Noting (5 Ps) reuses the identical engine as a second template right after — the `instrument_type` (summary|note) field stays, so no code is thrown away. Build/fixture/gold target Summary first.

**Branding note**: The app is **Noviq** (not "Echo AI"). Use the Noviq logo (`/home/riaz/Downloads/noviq_logo_3.png`) at the top/sidebar brand lockup, per `UI Design.jpeg`. Build as much UI as feasible; tasks marked **[DEFER]** are acceptable to carry to the next session.

## Format: `[ID] [P?] [Story] Description`
- **[P]**: Can run in parallel (different files, no dependencies)
- **[Story]**: US1–US4 (maps to spec.md user stories)

## Path Conventions
Single Tauri app: Rust backend `src-tauri/src/`, SolidJS frontend `src/`.

---

## Phase 1: Setup (Shared Infrastructure)

**Purpose**: Project + branding + module scaffolding.

- [X] T001 Create `src-tauri/src/services/drafting/` module with `mod.rs`, `template.rs`, `ingest.rs`, `slot.rs`, `docset.rs`, `gaps.rs`, `render.rs`, `gold.rs`; register `pub mod drafting;` in `src-tauri/src/services/mod.rs` ✅ compiles
- [X] T002 Created `src-tauri/src/commands/drafting.rs` (render_draft, export_draft, confirm/list/get/delete_template) and registered it in `commands/mod.rs` + the Tauri `invoke_handler` (lib.rs) ✅ full lib compiles
- [X] T003 [P] Add the Noviq logo asset: copy `/home/riaz/Downloads/noviq_logo_3.png` into `src/assets/` (or `public/`) as the app brand image
- [X] T004 [P] Replace user-facing "Echo AI" strings with "Noviq" across the frontend brand lockup/header (sidebar top per `UI Design.jpeg`); leave internal identifiers untouched
- [X] T005 [P] Add green-theme design tokens (forest/lime per the design) to the Tailwind config / theme used by `src/components/`

**Checkpoint**: Module + commands wired (compiling stubs); app shows the Noviq logo/name.

---

## Phase 2: Foundational (Blocking Prerequisites)

**Purpose**: Storage, types, fixtures, and the corpus-first entry that every story depends on.

**⚠️ CRITICAL**: No user-story work begins until this phase is complete.

- [X] T006 Added additive migration `021_lgd_drafting.sql` creating `template_schema`, `template_slot`, `gold_example` (no drops; TARGET_SCHEMA_VERSION NOT bumped → no DB wipe) ✅ applies in test_pool
- [X] T007 [P] Defined slot types in `slot.rs`: `SlotKind` (Boilerplate|Cited|Prose), `Slot` + validation rules ✅ 3 unit tests pass
- [X] T008 [P] Template persistence in `template.rs`: `TemplateSchema`/`TemplateHeader` + sqlx save/list/get/delete (overwrite-on-reconfirm) ✅ 2 sqlx tests pass
- [X] T009 [P] Per-draft working types in `mod.rs`: `DocRole`, `CorpusDocRef`, `IntentAnswers`, `Gap`, `GapResolution`, `DraftOutput`/`Block` enum (Boilerplate|Cited|Fact|Prose|Flagged) ✅ compiles
- [X] T010 **Gold-no-retrieval invariant** (SC-007) in `gold.rs`: record/list gold metadata only (content stays on disk, never indexed) ✅ guard test passes
- [X] T011 [P] Created synthetic fixture under `src-tauri/tests/fixtures/lgd/`: `rules.txt` (labelled provisions Rule 28/Rule 12/Section 17 + Preamble), `case.txt` (PC-I cost line + ADP line), `template.json` (6-slot Summary schema incl. one unanchorable cited slot) ✅
- [X] T012 Frontend: add a **Drafting** entry to the top nav (alongside Workspace/Library/Analysis/Archive) and create `src/pages/Drafting.tsx` + `src/stores/drafting.ts` + `src/api/drafting.ts` invoke wrappers (stubs)

**Checkpoint**: Schema migrated, types compile, fixture available, drafting surface reachable.

---

## Phase 3: User Story 1 — Citation-grade draft from a confirmed set (Priority: P1) 🎯 MVP

**Goal**: Given a saved template + a confirmed document set, render a draft whose cited content is verbatim+attributed from indexed chunks, facts are extractive from case docs, prose carries no numbers, and unanchored cited slots are flagged.

**Independent Test**: Run `cargo test --test lgd_drafting_acceptance` against the synthetic fixture: every cited block's number+quote appears verbatim in a source chunk; an unsatisfiable cited slot renders as flagged; no prose block contains a number/citation.

### Tests for User Story 1 ⚠️ (write first, ensure they FAIL)

- [X] T013 [P] [US1] Acceptance test `src-tauri/tests/lgd_drafting_acceptance.rs` (registered as a `[[test]]` target): asserts SC-001 (cited quote verbatim + number lifted from chunk), SC-002 (unanchorable slot → Flagged), FR-008 (fact extractive from case docs), FR-007 (prose number-free even with a misbehaving generator), using the T011 fixture, no live LLM ✅ pass
- [X] T014 [P] [US1] Unit tests in `render.rs` for the cited-slot path: `cited_slot_quotes_verbatim_and_reads_number_from_chunk` (title-match picks correct chunk, not rank-1) + `cited_slot_flags_when_unanchored` (`None` → flagged), per ADR-0008 / 008 Fix B ✅ pass

### Implementation for User Story 1

- [X] T015 [US1] Implemented cited-slot fill in `render.rs` (`fill_cited_slot`): scoped retrieval (command layer `retrieve_scoped` over confirmed **rules** docs) → select via `resolve_provision_label` → verbatim quote + deterministic citation; `None` → `Block::Flagged` (R1, ADR-0008) ✅
- [X] T016 [US1] Implemented fact-slot fill in `render.rs` (`fill_fact_slot`): extractive value + attribution from confirmed **case** docs (FR-008), never model-invented ✅
- [X] T017 [US1] Prose-slot fill: `sanitize_prose` + `contains_provision_number` backstop guarantee FR-007 (number/citation-free) ✅; **model wiring DONE 2026-06-08** — `commands/drafting.rs::generate_prose` resolves the GGUF via `query_router::select_model_for_tier`, builds a number-free ChatML prompt (`build_prose_prompt`), runs `ModelSlot::infer` (256 tok) inside `spawn_blocking`; graceful degrade to `PROSE_DEGRADE` sentence when no model on disk / inference fails (Principle VII). `render_one_slot` pre-generates prose then feeds it through the one contract path (still sanitized). `render_draft`/`revise_draft` now take `model_slot`+`tier` state. 31/31 lib + 1/1 acceptance green
- [X] T018 [US1] Implemented boilerplate rendering + ordered block assembly in `render.rs` (`render_draft`): one `Block` per slot in order (FR-002, FR-014) ✅. `drafting://progress` per-slot streaming DEFERRED (render is fast/synchronous for the fixture; revisit with model wiring)
- [X] T019 [US1] Implemented `render_draft` command in `commands/drafting.rs` returning `DraftOutput` (per-slot async scoped retrieval) ✅. `confirmed:true` gate (FR-022) lands with the `DraftSession` in US3/US5
- [X] T020 [US1] Implemented `export_draft` command (txt/md via `drafting/export.rs`, user-initiated save path, Principle V) ✅. PDF export reuse of `services/exporters` DEFERRED to Polish
- [X] T021 [P] [US1] Frontend cited-block **draft view** in `src/components/drafting/DraftView.tsx`: render block types per design (green citation highlights, `CONFIDENCE`/flag chips), wire progress events, and an **Export** action (maps to the design's Synthesis Tool/Export PDF)

**Checkpoint**: With a template + confirmed set (constructed in-test/fixture), a citation-grade draft renders and exports. MVP demoable.

---

## Phase 4: User Story 2 — Template setup from an uploaded format (Priority: P2)

**Goal**: Upload a format (blank or filled), auto-propose a slot schema (boilerplate/cited/prose + per-cited required-source category), correct once, save reusable; filled examples stored as gold (excluded from retrieval).

**Independent Test**: Upload a blank skeleton and a filled example; verify a proposed schema is returned, edits persist, the saved template is selectable, and the filled example never appears in retrieval.

### Tests for User Story 2 ⚠️

- [X] T022 [P] [US2] Covered by lib tests: `template::save_list_get_delete_roundtrip` + `template::overwrite_on_reconfirm_replaces_slots` (round-trip) and `gold::gold_example_recorded_but_absent_from_retrieval` (SC-007 exclusion) ✅ pass

### Implementation for User Story 2

- [X] T023 [US2] Implemented template ingestion in `ingest.rs` (`propose_schema(text, instrument_type)`): segments regions via a generic heading heuristic + `heading_detector::regex_pass::scan`; classifies boilerplate/cited(rule)/cited(`fact:`)/prose; derives each cited slot's `required_source_category` from its OWN heading/text (generic legal+financial lexicon, no hardcoded doc names — honors diagnose-before-code); flags low-confidence fill regions in `flaggedForReview` ✅ 4 unit tests pass
- [X] T024 [US2] Implemented `ingest_template` command (extract via `parsers::{pdf,document,text}` by `FileFormat`; PDF/DOCX/TXT/MD only with a remediation error otherwise) returning `ProposedSchema` (name from file stem; `goldExampleRef` set when `isFilledExample`) ✅
- [X] T025 [US2] Extended `confirm_template` command: `save_template` (validate + insert/overwrite `template_schema`/`template_slot`, overwrite-on-reconfirm) then, if `goldExampleRef` present, `gold::record_gold_example` (METADATA only — SC-007 invariant from T010) ✅ registered in invoke_handler
- [X] T026 [P] [US2] `list_templates` / `get_template` / `delete_template` commands (landed in the US1 command batch) ✅
- [X] T027 [P] [US2] Frontend **Template manager** in `src/components/drafting/TemplateManager.tsx`: upload (Upload Doc styling), proposed-schema review/edit (re-tag region, edit required-source), confirm & save, list/select/delete

**Checkpoint**: Officer can create and reuse templates; gold stays out of retrieval.

---

## Phase 5: User Story 3 — Curate the document set for a draft (Priority: P2)

**Goal**: After intent Q&A, app proposes a whole-corpus document set (role-tagged rules/case); officer adds/removes by selection; only the confirmed set is used.

**Independent Test**: Run intent Q&A → proposed list shown → add/remove updates the working set → only confirmed docs feed the draft.

### Implementation for User Story 3

- [X] T028 [US3] Implement candidate proposal in `docset.rs`: `intent_query` + `tag_role` (generic instrument-class lexicon, officer-correctable, R2) + `propose_candidates` (dedupe-to-document, preserve order). Pure/testable; retrieval lives in the command. ✅ 5 lib tests
- [X] T029 [US3] Implement `start_draft` command (template + intent) → `DraftSession { draftId, candidateDocs }`. Whole-corpus retrieval (`retrieve_whole_corpus`, FR-016, indexed+watched scope) → `propose_candidates` → in-memory `DraftSessions` store (uuid key) ✅
- [X] T030 [US3] Implement `update_doc_set` command: persist confirmed set in session; return `DocSetUpdate { gaps: [] }` placeholder (gap logic lands in US4) ✅
- [X] T031 [P] [US3] Frontend **intent Q&A** form + **doc-set curation** list in `src/components/drafting/IntentForm.tsx` and `DocSet.tsx`: numbered add/remove/retag; "confirm document set" gate (FR-022) before render **[DEFER → Phase 7 UI wave]**

**Checkpoint**: Officer controls provenance; only the confirmed set drives the draft.

---

## Phase 6: User Story 4 — Detect & resolve missing required documents (Priority: P3)

**Goal**: Missing required-source categories are reported with an openable link to the watched corpus folder; supplying a doc updates the set and notifies; "proceed without" flags the affected slots.

**Independent Test**: A template cited slot with no available source → gap reported with destination folder → dropping the doc updates the set (notification) → or proceed-without yields flagged slots.

### Implementation for User Story 4

- [X] T032 [US4] Implement gap detection in `gaps.rs`: `detect_gaps(slots, confirmed, destination_folder)` — category-level coverage by required ROLE (rules vs `fact:`→case) + salient-token overlap on doc titles (generic, no hardcoded mapping); uncovered cited slot → `Gap { slotOrdering, requiredSourceCategory, destinationFolder }` (R6) ✅ 4 lib tests
- [X] T033 [US4] Wired `update_doc_set` to return real gaps (`gaps_for` fetches template + `primary_watched_folder`); implemented `resolve_gap` command (proceedWithout recorded in session `gap_resolutions`; render already flags unanchored slots) ✅
- [X] T034 [US4] `refresh_doc_set` command: re-propose over whole corpus, `docset::newly_added` diff → auto-add new docs to confirmed set (FR-020), recompute gaps, emit `drafting://docset-updated { draftId, addedDocs, updatedGaps }` (SC-005) ✅. **[DEFER]** auto-trigger from `file_watcher`/`indexer` completion → lands with running-UI integration (indexer doesn't yet know open draft sessions); frontend/watcher calls `refresh_doc_set`.
- [X] T035 [P] [US4] Frontend **gap panel** in `src/components/drafting/GapPanel.tsx`: list gaps, openable folder link, "I don't have it — proceed" action, live update on `docset-updated` **[DEFER → Phase 7 UI wave]**

**Checkpoint**: The refuse-or-flag loop is productive end-to-end.

---

## Phase 6b: User Story 5 — Iterative refinement until finalize (Priority: P1)

**Goal**: A draft is a living, revisable session — the officer refines (swap cited source, regenerate/edit prose, change intent, re-curate docs) with re-render after each change, until an explicit finalize. Every revision keeps the citation-grade contract.

**Independent Test**: Render → swap a cited slot's source → only that block re-resolves; regenerate a prose block → only it changes, still number-free; hand-edit prose → preserved; edit on a cited block cannot alter its number; finalize → locked + exportable.

- [X] T044 [US5] Persistent + revisable session in `services/drafting/session.rs`: `DraftState` (template, intent, candidate+confirmed docs, current blocks, per-prose edits, gap resolutions, `finalized`) with serde; `save`/`load`/`delete` over additive `draft_session` table (migration 022, one JSON blob; did NOT bump TARGET_SCHEMA_VERSION) → unfinished draft resumes after restart (FR-023). Commands write-through (`put_state`) + load-on-miss (`get_state`). ✅ 3 lib tests
- [X] T045 [US5] Targeted re-render in `render.rs`: `render_slot(slot, intent, retrieve, prose)` — the ONE contract code path used by full render AND every revision; `render_draft` refactored to use it. Command-side `render_one_slot` does async scoped retrieval for a single slot. ✅ 2 lib tests
- [X] T046 [US5] `revise_draft` command (tagged `Revision`: reresolveCited{slotOrdering,newDocId?} | regenerateProse | editProse | updateIntent | updateDocSet) enforcing FR-025: cited re-resolves deterministically (number from chunk, never hand-set), prose sanitized number-free, editProse rejected on non-prose; only affected block(s) re-render. ✅
- [X] T047 [US5] `finalize_draft` command (FR-026): sets `finalized`, returns locked blocks for `export_draft`; `revise_draft` errors after finalize. Also reshaped `render_draft` to session form `{draftId, confirmed:true}` (FR-022 gate) storing blocks + preserving prose edits. ✅
- [X] T048 [P] [US5] Frontend revision controls in `src/components/drafting/DraftView.tsx`: per-block actions (re-resolve/swap source, regenerate, edit-prose), a session indicator, and a Finalize button; re-render affected blocks in place **[DEFER → Phase 7 UI wave]**

**Checkpoint**: Officer can iterate to a converged, finalized, exportable draft.

---

## Phase 7: UI shell alignment & Index-Folder-first-screen (Cross-cutting)

**Purpose**: Bring the app surface to the `UI Design.jpeg` target and put corpus indexing first.

- [X] T036 Frontend: align the app shell to the 3-column research-station layout (left sidebar nav + files tree, center canvas, right insights rail) using the green theme tokens (T005)
- [X] T037 Frontend: add an **Index Folder** option on the first screen (sidebar, alongside/leading "Upload Doc") wired to the existing folder-index command (`commands/folders.rs`); first-run empty-state prompts the officer to index a folder before drafting
- [ ] T038 [P] [DEFER] Frontend: right-rail "Research Insights"/Categorized-Themes polish + Synthesis-Tool card styling to match the design
- [ ] T039 [P] [DEFER] Layout-faithful document export (downloadable doc matching the uploaded format) — future enhancement beyond the first-cut block output

---

## Phase 8: Polish & Cross-Cutting Concerns

- [ ] T040 [P] Add human-readable error + remediation messages on every gap/flag/validation path (Development Constraint)
- [X] T041 Append a `CHANGELOG_DEV.md` entry (symptom/why, files+functions, cross-cutting impact) per CLAUDE.md
- [ ] T042 Run `quickstart.md` validation (SC smoke checks) against the fixture; record results
- [ ] T043 [DEFER] Replace synthetic fixture acceptance with the real sanitized LGD summaries/notes once provided (true SC-003 gate)

---

## Phase A — Conversational intent session (FR-027/028/030 subset · FR-031/032 append+load)

> Conversational enhancement, **Phase A only** (intent-chat MVP). Replaces `IntentForm` at parity: the officer answers intent turn-by-turn in a transcript; on completion the system runs the *existing* `start_draft` proposal and hands off to the existing `DocSet` panel. **Persistence is append+load only — NO compaction** (Phase C) and **NO doc-set/gap/refine in the transcript yet** (Phase B). TDD: tests before implementation. Reuse `session.rs` (`save`/`load`, `DraftState`, `IntentAnswers`) and the `start_draft` proposal path — do not duplicate.

### Schema (blocks Phase A)

- [X] T049 [CONV] Add additive migration `src-tauri/src/db/migrations/023_draft_chat.sql` creating `chat_message` (id TEXT PK, draft_session_id TEXT, seq INTEGER, role TEXT, content TEXT, token_count INTEGER, summarized INTEGER DEFAULT 0, created_at TEXT; UNIQUE(draft_session_id, seq)) and `conversation_summary` (draft_session_id TEXT PK, summary_text TEXT, covers_upto_seq INTEGER, updated_at TEXT) per `data-model.md`. Additive only; follow the 021/022 convention (do not bump TARGET_SCHEMA_VERSION beyond the additive pattern those used). `conversation_summary` is created now but unused until Phase C.

### Tests first (RED)

- [X] T050 [P] [CONV] Add `#[cfg(test)]` unit tests for the turn-action parser in `src-tauri/src/services/drafting/chat.rs`: free-text turns map to `DraftAction::SetIntent { field, value }` for instrument_type / addressee / purpose / scheme; an unrecognised turn maps to `DraftAction::Clarify`. (FR-028/030)
- [X] T051 [P] [CONV] Add tests for append+load in `src-tauri/src/services/drafting/conversation.rs` `#[cfg(test)]` (in-memory sqlite): appending N messages assigns monotonic `seq`, `load_conversation` returns them ordered, and no row is deleted. (FR-031)
- [X] T052 [CONV] Add a conversation acceptance test `src-tauri/tests/lgd_conversation.rs`: drive `send_chat_turn` through the four intent turns and assert (a) the resulting `DraftState.intent` + proposed `candidate_docs` equal those from a direct `start_draft` call with the same intent (parity), and (b) every officer+system turn is persisted in `chat_message`. (FR-027/031/032)

### Implementation (GREEN)

- [X] T053 [CONV] Create `src-tauri/src/services/drafting/chat.rs`: `enum DraftAction` (Phase A variants: `SetIntent{field,value}`, `Clarify`) + `parse_turn(state: &DraftState, text: &str) -> DraftAction` using `regex_lite`/deterministic matching against the next unanswered intent field; export from `services/drafting/mod.rs`. Intent fields only this phase.
- [X] T054 [CONV] Create `src-tauri/src/services/drafting/conversation.rs`: `struct ChatMessage` (serde), `append_message(pool, draft_id, role, content) -> seq` (monotonic seq via MAX(seq)+1), `load_conversation(pool, draft_id) -> Vec<ChatMessage>`; no compaction. Export from `mod.rs`.
- [X] T055 [CONV] Refactor `start_draft` in `src-tauri/src/commands/drafting.rs` (≈345): extract the propose-into-session step (`docset::intent_query` → `retrieve_whole_corpus` → `propose_candidates` → fill DraftState) into a reusable `async fn propose_for_session(pool, embedder, sessions, draft_id, &mut DraftState)` so the conversation can fill an **existing** `draft_id`; keep `start_draft`'s external behaviour unchanged (it calls the new helper).
- [X] T056 [CONV] Add command `send_chat_turn(pool, embedder, sessions, draft_id: Option<String>, text) -> { reply, action, state }` in `commands/drafting.rs`: mint a `draft_id` + empty-intent `DraftState` on first turn; append officer turn (T054); `parse_turn` (T053); apply `SetIntent` to `DraftState.intent` via `get_state`/`put_state`; when all intent fields present call `propose_for_session` (T055) and set step→docset; build + append the system reply (next question, or "proposed N documents"); return state mirroring `DraftState`. Register in `commands/mod.rs` + `lib.rs` invoke_handler.
- [X] T057 [CONV] Add command `load_conversation(pool, sessions, draft_id) -> { messages, state }` in `commands/drafting.rs` (no `summary` field in Phase A); register in `commands/mod.rs` + `lib.rs`.
- [X] T058 [P] [CONV] Add bindings in `src/api/drafting.ts`: `sendChatTurn(draftId, text)` and `loadConversation(draftId)` with TS types matching the Rust serde shape — verify `rename_all`/camelCase so fields aren't `undefined` across `invoke()`.
- [X] T059 [P] [CONV] Extend `src/stores/drafting.ts`: `messages` signal, current `draftId` signal, and an append helper; keep existing `step`/`session`/`confirmedDocs` signals intact.
- [X] T060 [CONV] Create `src/components/drafting/ChatSession.tsx`: transcript + composer; on mount greet and ask the first intent question; send turns via `sendChatTurn`; render system replies; when returned `state.step === "docset"` set the store step so the existing `DocSet` panel takes over (parity with `IntentForm` calling `setStep("docset")`).
- [X] T061 [CONV] Wire `src/pages/Drafting.tsx`: render `ChatSession` (not `IntentForm`) at `step() === "intent"`; leave `DocSet`/`DraftView` and the Index-Folder-first empty-state gate unchanged. Leave `IntentForm.tsx` in place but unreferenced until parity is confirmed, then it can be retired.

### Verify

- [X] T062 [CONV] Run `cargo test drafting` + `cargo test --test lgd_conversation` (GREEN); manually verify the quickstart "Conversational session walkthrough" steps 1–2 (intent collected via chat → doc panel fills) reach the same handoff as the old form.
- [X] T063 [CONV] Append a `CHANGELOG_DEV.md` entry (symptom/why, files+functions, follow-ups: Phase B fold-in, Phase C compaction). Confirm no second model is loaded (n/a Phase A — no summarization yet; deterministic parser).

**Deferred (NOT in Phase A):** Phase B = fold confirm/add/remove-doc, gap resolve, targeted revision, finalize into the transcript + live panels (FR-029, rest of FR-028). Phase C = rolling-summary compaction (FR-033/034) using `conversation_summary` + the single model slot. Tasks for B/C generated in a later `/sp.tasks` run.

## Phase D — Ingestion rewrite: Option C, model-assisted + verbatim-verify gate (FR-010/010a/010b · ADR-0012 · R5a)

> Fixes live-test Defect 1: the heading-heuristic `ingest.rs` produced 13 garbage slots on a flawless real instrument (letterhead/dateline/C.C. became cited slots; the real citations landed in no slot). Replace segmentation with Mistral-7B free-text JSON (model already at `models/ir_builder/Mistral-7B-Instruct-v0.3-Q4_K_M.gguf`, reuse spike prompt/params: `infer_with_cancel_temp`, n_ctx 6144, max_gen 1536) + a deterministic verbatim-verify gate. NO llguidance (hard abort, ADR-0012). Refuse-not-degrade when the model is missing (FR-010b). One-time offline op — the per-draft path stays model-free. TDD: gate and fixture tests first; the gate is pure/deterministic so everything except the live model call is testable without LLM.

### Schema (blocks D)

- [X] T064 [D] Add additive migration `src-tauri/src/db/migrations/024_slot_provision.sql`: `ALTER TABLE template_slot ADD COLUMN provision_ref TEXT;` (nullable, additive only, follow the 021–023 convention — no TARGET_SCHEMA_VERSION bump beyond the additive pattern). Add `provision_ref: Option<String>` to the `Slot` struct in `src-tauri/src/services/drafting/slot.rs` (serde camelCase `provisionRef`) and thread it through `template.rs` insert/select.

### Tests first (RED)

- [X] T065 [P] [D] Add a synthetic real-instrument fixture `src-tauri/tests/fixtures/lgd_letter_fixture.md` replicating the gold's failure shapes (NOT the real gold — that stays out of the repo): letterhead block with `(REGULATIONS WING)`, `Karachi, dated …` dateline, numbered running paragraphs with long lines and inline hyphenated citations (`Rule-6(c) of the XYZ Regulations-2023`, `Rule-23 of the ABC Rules 2020`), a `C.C:` list and a signature block. This is the Phase D acceptance anchor (quickstart FR-010/010a check).
- [X] T066 [P] [D] Add `#[cfg(test)]` unit tests for the gate in `src-tauri/src/services/drafting/ingest.rs` (new `verify` section): (a) slot text that is a verbatim substring (whitespace-normalized) passes; paraphrased text fails with `rejection_reason`; (b) hyphenated provisions `Rule-6(c)` / `Regulations-2023` are detected and matched verbatim; (c) a slot whose region matches a structural-boilerplate anchor (letterhead/dateline/`C.C`/signature/addressee) is never accepted as `cited`; (d) `required_source_category` derives from the matched provision text, not heading tokens; (e) a gate-rejected slot carries `low_confidence: true` + reason — never dropped from the proposal. (FR-010/010a)
- [X] T067 [P] [D] Add tolerant-parse unit tests in `ingest.rs`: model output variants (fenced ```json, leading prose, trailing commas, literal newlines in strings) all parse to the proposed-slot list; unparseable output after the lenient chain returns `Err` (not a partial schema). Reuse the F006 lenient-parse approach.
- [X] T068 [D] Add ingestion acceptance test `src-tauri/tests/lgd_ingest_gate.rs` (no LLM — feed a canned model response): run the gate pipeline over the T065 fixture with a hand-written "model proposal" JSON and assert (a) both inline citations land in `cited` slots with verbatim `provision_ref`; (b) letterhead/dateline/C.C./signature regions are boilerplate or flagged, never cited; (c) every accepted slot's text is a verbatim substring of the fixture. (FR-010/010a; contract checks in `contracts/drafting-commands.md`)

### Implementation (GREEN)

- [X] T069 [D] Rewrite `src-tauri/src/services/drafting/ingest.rs` proposal stage: add `build_segmentation_prompt(text, instrument_type) -> String` (Mistral `[INST]` format from the spike — copy-verbatim instructions, `{kind, label, text, provision?}` regions) and `parse_model_slots(raw: &str) -> Result<Vec<ModelProposedSlot>, String>` (tolerant chain, T067). Delete the heuristic segmentation path (`segment_regions`, `is_heading_line`, `classify`, heading-token `derive_category`) — keep `salient_tokens`/regex helpers only where the gate reuses them. Update/remove the `ingest.rs` unit tests that asserted heuristic behaviour.
- [X] T070 [D] Implement the gate in `ingest.rs`: `verify_slots(source_text: &str, proposed: Vec<ModelProposedSlot>) -> Vec<ProposedSlot>` — whitespace-normalized verbatim-substring check for text + provision; hyphen-tolerant provision matching (extend the `provision_number_re` family: optional `[-\s]` between noun and number, e.g. `Rule-6(c)`, `Regulations-2023`); category derived from the matched provision; structural-boilerplate anchors (To/SUBJECT/C.C/dateline/signature) force non-cited; failures → `low_confidence: true` + `rejection_reason`, slot retained in the proposal (FR-010a). `ProposedSlot` = `Slot` + `low_confidence` + `rejection_reason` (in-memory only, not persisted).
- [X] T071 [D] Reshape `ingest_template` in `src-tauri/src/commands/drafting.rs` (≈862): take `model_slot: State<Arc<ModelSlot>>` + tier like `render_draft` (≈251); resolve the segmentation GGUF from `models/ir_builder/` (reuse the F006 discovery pattern); if absent → `Err` with the actionable message naming the model, NO schema, NO heuristic fallback (FR-010b); else extract → prompt → `spawn_blocking` infer (`infer_with_cancel_temp`, n_ctx 6144, max_gen 1536; bounded retries on parse failure with temp escalation à la F006; exhaustion → `Err`, FR-010b) → `parse_model_slots` → `verify_slots`; emit `drafting://progress` at extract/infer/verify stages. Return slots incl. `lowConfidence`/`rejectionReason`/`provisionRef` (serde camelCase).
- [X] T072 [D] Persist `provision_ref` through confirm: `confirm_template` (≈893) + `template.rs` write/read the new column (T064); `get_template` returns it. Optionally let `render.rs`'s cited-slot query use `provision_ref` to sharpen scoped retrieval (behaviour-compatible).
- [X] T073 [P] [D] Frontend: `src/api/drafting.ts` extend `Slot` type with `provisionRef?`, `lowConfidence?`, `rejectionReason?`; `src/components/drafting/TemplateManager.tsx` render low-confidence proposals with the rejection reason and require an explicit officer classification before confirm (FR-010a); surface the model-missing error verbatim (FR-010b). `tsc --noEmit` clean.

### Verify (D)

- [ ] T074 [D] **(automated half DONE 2026-06-10: lib 47/47 + lgd_ingest_gate 2/2 + acceptance 1/1 GREEN — remaining: manual real-gold ingest via the running app)** Run `cargo test drafting` + `cargo test --test lgd_ingest_gate` + `cargo test --test lgd_drafting_acceptance` (all GREEN, ONE cargo run — batch-build discipline). Then manually ingest the REAL gold (`~/Desktop/Noviq templetes/GOLD_SPSC_age_relaxation_2025.md`) through the app once and confirm the quickstart FR-010/010a checks (real citations in slots, boilerplate never cited, ~18 min acceptable, progress visible).
- [X] T075 [D] Delete the spike file `src-tauri/examples/option_c_spike.rs` (results preserved at `~/Desktop/Noviq Documents reports/option_c_ingestion_spike_20260610/`); append a `CHANGELOG_DEV.md` entry (Defect 1 symptom, Option C fix, ADR-0012, files+functions, FR-010b behaviour change: ingestion now requires the 7B).

## Phase E — Two-tier semantic gap coverage + officer-supplied facts (FR-019 · FR-008a · R11/R12)

> Fixes live-test Defect 2: `gaps.rs::category_covered` matched category words against doc TITLES (title-token overlap) → ~10–11 false gaps on an 8-document corpus ("consultations" can never match the title "Rules of Business 1986" even though Rule 17 covers it). Coverage becomes content-semantic (embed category vs `chunk_embeddings`), two-tier (suggest indexed-but-unconfirmed docs in one turn; only corpus-wide absence is a true gap). Fact slots with no covering case doc take the officer-supplied channel instead of a gap. Calibration anchor: on the live 8-PDF corpus, "consultations" must resolve to RoB 1986 as a suggestion.

### Tests first (RED)

- [X] T076 [P] [E] Rewrite `#[cfg(test)]` tests in `src-tauri/src/services/drafting/gaps.rs` for the new contract, injecting a fake coverage closure (no embedder in unit tests): (a) category covered by a confirmed doc → no gap; (b) covered only by an unconfirmed corpus doc → `Gap` with `tier: Suggestion { suggested_docs }`; (c) covered by nothing → `tier: Missing { destination_folder }`; (d) `fact:` slot with no covering case doc → `tier: OfficerInput` (prompt shape, not a Missing gap); (e) boilerplate/prose slots ignored (keep). Delete the title-token tests.
- [X] T077 [P] [E] Add session/render tests: `GapResolution::OfficerSupplied { value }` round-trips serde in `src-tauri/src/services/drafting/session.rs` (resume-safe, FR-032); a fact slot resolved officer-supplied renders as a `fact` block with `attribution == "officer-supplied"` and the value verbatim — never as a `cited` block, prose sanitizer untouched (FR-008a) — tests in `src-tauri/src/services/drafting/render.rs`.

### Implementation (GREEN)

- [X] T078 [E] Rewrite `src-tauri/src/services/drafting/gaps.rs`: `detect_gaps` takes a per-doc coverage provider (`covers(category, doc) -> bool` over confirmed + whole-corpus candidates) instead of title-token overlap; emit the two-tier `Gap` shape (`Suggestion { suggested_docs }` | `Missing { destination_folder }` | `OfficerInput` for fact slots) per data-model. Keep `detect_gaps` pure/sync for testability; embedding lookups live in the command layer.
- [X] T079 [E] Implement the semantic coverage provider in `src-tauri/src/commands/drafting.rs`: extend `gaps_for` (≈543) to embed each cited slot's `required_source_category` once (`embedder.embed_one`) and compute max cosine vs each candidate doc's `chunk_embeddings` rows (SQL scoped by `source_id`, reuse the `retrieve_scoped` query pattern ≈307); doc covers category when similarity ≥ threshold (`const`, overridable via `ECHO_GAP_COVER_THRESHOLD` env for calibration); tier-a scans the WHOLE indexed corpus (not just confirmed) for suggestion candidates. Update `update_doc_set` (≈559) + `refresh_doc_set` (≈623) signatures/state to pass the embedder and return the reshaped `gaps` (serde camelCase `tier`/`suggestedDocs`/`destinationFolder`).
- [X] T080 [E] Extend `resolve_gap` (≈584): decisions `"acceptSuggestion"` (move the gap's `suggested_docs` into `confirmed_docs` via the existing `put_state` write-through, recompute gaps, return the updated set) and `"officerSupplied"` with `value: String` (fact slots only — store `GapResolution::OfficerSupplied { value }` in `DraftState.gap_resolutions`); the fact path in `render.rs` `render_slot` consumes it as the verbatim value with `attribution: "officer-supplied"` (FR-008a). Reject `officerSupplied` on non-fact slots with a remediation message.
- [X] T081 [P] [E] Frontend: `src/api/drafting.ts` Gap type gains `tier`/`suggestedDocs`; `src/components/drafting/GapPanel.tsx` renders suggestions with one-click "Add to set" (`resolve_gap acceptSuggestion`), missing gaps with the folder link (unchanged), and fact-slot prompts with a text input ("type the value or point to a case document") calling `resolve_gap officerSupplied`; `src/components/drafting/DraftView.tsx` renders officer-supplied fact blocks visually distinct from cited blocks (FR-003/008a). `tsc --noEmit` clean.

### Verify (E)

- [ ] T082 [E] **(automated half DONE 2026-06-10: tests GREEN — remaining: live-corpus threshold calibration via the running app)** Run `cargo test drafting` + acceptance tests (GREEN, ONE cargo run). Calibrate the threshold on the LIVE 8-PDF corpus (`/home/riaz/.local/share/com.echoai.app/echo.db`): "consultations mandatory where applicable" must yield a Suggestion for Rules of Business 1986 and the false ~10–11 gaps must collapse; run the quickstart FR-019 + FR-008a smoke checks.
- [X] T083 [E] Append the Phase E `CHANGELOG_DEV.md` entry (Defect 2 symptom with live-test numbers, semantic coverage fix, files+functions, chosen threshold value, follow-ups: vec0 ANN for whole-corpus scans at scale).

## Dependencies & Execution Order

- **Phase A (CONV)** → after Foundational (P2) + US3 (`start_draft`/`docset` exist) + US5 (`session.rs` `save`/`load`). T049 blocks all CONV; T050–T052 (RED) before T053–T061 (GREEN); T062–T063 last. T058/T059 are [P] (different frontend files).
- **Phase D (ingestion rewrite)** → independent of Phases B/C; only needs US2's existing `ingest.rs`/`template.rs`. T064 blocks T072; T065–T068 (RED) before T069–T073 (GREEN); T074–T075 last. **Recommended FIRST of the remaining work** (live-pilot blocker, ADR-0012).
- **Phase E (semantic gaps + officer facts)** → independent of B/C/D (different files: `gaps.rs`/`render.rs` vs `ingest.rs`); needs US4's existing `gaps.rs` + US5's `session.rs`. T076–T077 (RED) before T078–T081 (GREEN); T082–T083 last. **Recommended order: D → E → B → C** (per amended plan).
- **Setup (P1)** → no deps.
- **Foundational (P2)** → after Setup; **blocks all stories** (schema, types, fixture, surface).
- **US1 (P3 phase)** → after Foundational. **MVP.** Independently testable via fixture (does not require US2/US3 UI; template + confirmed set constructed in-test).
- **US2, US3** → after Foundational; independent of each other; both feed real (non-fixture) drafts into US1's render path.
- **US4** → after US3 (needs a confirmed set + gaps shape).
- **Phase 7 shell** → can proceed in parallel with stories (different files); Index-Folder (T037) only needs existing folder command.
- **Polish (8)** → after desired stories.

## Parallel Opportunities

- T003/T004/T005 (branding/theme) in parallel.
- T007/T008/T009 (types) in parallel; T011 fixture in parallel.
- T013/T014 (US1 tests) in parallel before T015–T020.
- Frontend component tasks (T021, T027, T031, T035) are [P] across stories (different files).
- Phase D RED tasks T065/T066/T067 in parallel (different test surfaces); T073 [P] frontend alongside T071/T072 backend.
- Phase E RED tasks T076/T077 in parallel; T081 [P] frontend alongside T079/T080 backend. Phases D and E touch disjoint backend files and can be worked as parallel tracks if desired (shared file: `commands/drafting.rs` — coordinate T071 vs T079/T080).

## Implementation Strategy

**MVP**: Phase 1 → Phase 2 → Phase 3 (US1), then STOP and validate `cargo test --test lgd_drafting_acceptance`. That proves the citation-grade contract end-to-end on a fixture.

**Then incrementally**: US2 (real templates) → US3 (real doc sets) → US4 (gaps/watch) → Phase 7 shell + Index-Folder → Polish. Carry **[DEFER]** items to the next session as needed.

## Notes
- Cited numbers/citations come ONLY from chunk metadata via `resolve_provision_label` (ADR-0008) — never from the model.
- Migration is additive only (Constitution IV / Development Constraint).
- Gold examples never enter retrieval (SC-007) — guarded in T010, asserted in T022.
- Real LGD gold (from the user) is required for the true acceptance gate (T043); fixture unblocks everything else.
