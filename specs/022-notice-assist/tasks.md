# Tasks: Notice Assist — /help Officer-Defense

**Feature**: `022-notice-assist` | **Spec**: [spec.md](./spec.md) | **Plan**: [plan.md](./plan.md)
**Input**: plan.md, spec.md, research.md, data-model.md, contracts/, quickstart.md

Task IDs are sequential in execution order. `[P]` = parallelizable (different file, no dependency
on an incomplete task). `[USn]` = the user story a task serves. Setup/Foundational/Polish carry no
story label. All backend paths under `src-tauri/src/`, frontend under `src/`.

---

## Phase 1: Setup

- [X] T001 Create the capability module skeleton `src-tauri/src/services/assistant/capabilities/notice_assist/{mod.rs,frame.rs,extract.rs,session.rs,ask.rs,compose.rs}` with empty pub items + module wiring in `src-tauri/src/services/assistant/capabilities/mod.rs`.
- [ ] T002 [P] Add config fragment stubs for the analysis prompt and the two reply templates in `src-tauri/src/config/personas/` (new `notice_assist.toml` or capability fragment), referenced by the persona loader (Principle VI).
- [X] T003 [P] Add the additive migration file for `notice_assist_session(conversation_id text PK, state_json text, updated_at)` under `src-tauri/src/db/pg/migrations/` (next migration number), and register it in the migration runner.

---

## Phase 2: Foundational (blocking prerequisites for all stories)

- [X] T004 [P] Define the data-model types in `src-tauri/src/services/assistant/capabilities/notice_assist/frame.rs`: `NoticeAssistState`, `Frame`, `Allegation`, `SlotQuestion`, `CitedRule`, `Phase`, `DocKind`, `SlotStatus` — all `serde` with `#[serde(default)]`, tolerant of unknown keys (per data-model.md).
- [X] T005 Implement the session repository seam in `notice_assist/session.rs`: `save/load/delete(pool, conversation_id, &NoticeAssistState)` over `notice_assist_session.state_json` (mirror `services/drafting/one_ask/state.rs`).
- [X] T006 [P] Add `intent::is_notice_assist_command(text) -> bool` in `src-tauri/src/services/assistant/intent.rs` (leading `/help|/reply|/audit`, case-insensitive, tolerant of trailing text) + unit tests (positive tokens; plain queries do NOT match).
- [X] T007 Add a local-model extraction generator accessor in `notice_assist/extract.rs`: resolve the **local Qwen3-8B** generator via the inference seam honoring `NOVIQ_NOTICE_EXTRACT_ENGINE` (default `local`) and `NOVIQ_NO_THINK=1`; return a graceful "unavailable" signal (feeds R4 fallback), do NOT fall through to a remote engine.
- [X] T008 Wire the `run_turn` intercept in `src-tauri/src/services/assistant/mod.rs`: before capability dispatch, if `intent::is_notice_assist_command(text)` OR an open `notice_assist_session` exists, route to `notice_assist::run(...)` (contract: notice-assist-commands.md). Precedence alongside transform/additive.
- [X] T009 Scaffold `notice_assist::run(pool, generator, embedder, retriever, app, conversation_id, text, scope_ids) -> Result<TurnResponse,String>` in `notice_assist/mod.rs` as the phase dispatcher (Analyze→Gather→Ground→Draft→Done), loading/saving session state each turn (returns a stub `TurnResponse` until stories fill phases).

**Checkpoint**: `run_turn` routes `/help` and open sessions into the phase dispatcher; state persists; nothing user-visible beyond a stub. Foundation complete.

---

## Phase 3: User Story 1 — Start assistance with one command (P1)

**Goal**: `/` opens a command menu; `/help` enters assist mode for the conversation.
**Independent test**: Type `/` → menu lists `/help`/`/reply`/`/audit`; pick `/help` → Noviq
acknowledges assist mode and, with no attachment, asks the officer to attach/paste the notice.

- [X] T010 [P] [US1] Add the `/` command palette to `src/components/ConfidentialComposer.tsx`: detect `/` at input start → open a static, keyboard-navigable command list (reuse the `@`-mention open/query/activeIdx/pick machinery); picking a command sets composer text to the token.
- [X] T011 [P] [US1] Add the command catalogue + optional prop wiring (commands list) in `ConfidentialComposer.tsx` and pass it from `src/pages/ChatPage.tsx`; no corpus-scope emission from `/`.
- [X] T012 [US1] Implement the **Analyze-entry** branch in `notice_assist/mod.rs`: on `/help` with NO usable attachment text → create/reset session and return a `TurnResponse` asking the officer to attach or paste the notice (FR-003); flag `notice_assist:gathering`.
- [X] T013 [P] [US1] Render an assist-mode banner in `src/pages/ChatPage.tsx` keyed off the `notice_assist:gathering` flag (questions/answers render as normal messages — no new message type).
- [X] T014 [P] [US1] Unit test `intent::is_notice_assist_command` command variants (already added in T006 — extend with `/reply`/`/audit` + trailing text); frontend `/` palette validated via `npm run build` (tsc+vite).

**Checkpoint**: US1 demoable — command entry works end-to-end with a stub "attach the notice" response.

---

## Phase 4: User Story 2 — Derive fields from the document (P1)

**Goal**: local Qwen3-8B reads the attached notice and produces the `Frame` (allegations + invoked
rule/authority + relief + forum + deadline + **per-allegation questions**), no predefined fields.
**Independent test**: attach a show-cause notice → ≥1 allegation + non-empty questions; attach a
2-objection audit para → 2 objections with distinct questions; bad model output → graceful fallback.

- [X] T015 [P] [US2] Write the analysis prompt fragment (T002 stub) per contracts/extraction-json.md: strict reading-comprehension, emit ONLY one JSON object, FORBID inventing rules/law/citations, generate per-allegation questions.
- [X] T016 [US2] Implement document-text assembly in `notice_assist/extract.rs`: pull the attachment's extracted text (F021 chunks/IR); if over budget, window + heading-cue selection (`show ?cause|explanation|audit para|objection|charge|memo|notice`) per research R3.
- [X] T017 [US2] Implement the Qwen extraction call in `extract.rs` (via T007 generator, `NO_THINK`) returning raw text.
- [X] T018 [US2] Implement tolerant JSON parsing in `extract.rs` (research R2): strip fences, first balanced `{…}`, `serde_json`→`Frame` with defaults, clamp `kind_confidence`, unknown `document_kind`→`other`, retry-once terse reminder, then fallback; assign stable ids; log raw on failure.
- [X] T019 [US2] Implement the **Analyze** phase in `notice_assist/mod.rs`: run extraction once per attachment, cache `Frame` in session, transition to Gather; empty/failed → fallback single-question frame (research R4, FR-005).
- [X] T020 [P] [US2] Unit tests in `notice_assist/extract.rs` (`#[cfg(test)]`) over fixture notices: a show-cause sample, a 2-objection audit para, and malformed model output → fallback (no crash, no fabricated frame). Fixtures under `src-tauri/tests/fixtures/notices/` or inline strings.

**Checkpoint**: US2 verifiable — attaching a document yields a document-derived `Frame` with generated questions; robust to bad output.

---

## Phase 5: User Story 3 — Intelligent minimal fact-gathering (P1)

**Goal**: ask only open questions (1–2 at a time), record answers, support "N/A"→gap, persist/resume.
**Independent test**: answered questions aren't re-asked; "N/A" marks a gap; leaving+returning resumes.

- [X] T021 [US3] Implement `ask.rs`: `next_open_question(&Frame)`, `record_answer(&mut Frame, answer)` (mark `Filled`), `mark_gap(&mut Frame)` for "N/A / I don't have that", and completion check (no `Open` left OR officer says "draft").
- [X] T022 [US3] Implement the **Gather** phase in `notice_assist/mod.rs`: emit next question(s) as the assistant answer with `notice_assist:gathering`; on subsequent turns route the officer's text into `record_answer`/`mark_gap`; save session each turn.
- [X] T023 [US3] Handle the "draft now" / completion transition Gather→Ground (officer intent detection: "draft"/"that's all"/no open questions).
- [X] T024 [P] [US3] Add an "N/A / skip" detector (regex_lite) for gap-marking and a guard so a `Filled`/`Gap` slot is never re-asked (US3 AS-1/AS-2).
- [X] T025 [P] [US3] Unit tests for `ask.rs` + Gather transitions: fill→not-re-asked, N/A→gap, resume from persisted state (uses PG test engine; VM `NOVIQ_TEST_PG_INSTALL_DIR`).

**Checkpoint**: US3 verifiable — a full gather loop over a `Frame`, resumable.

---

## Phase 6: User Story 4 — Grounded cited reply + checklist (P1)

**Goal**: retrieve the governing rule verbatim from the corpus (cited), draft the templated reply +
documents-to-attach checklist, flag gaps honestly, export as masked plain text.
**Independent test**: reply quotes the cited rule verbatim with a citation; a corpus miss is flagged;
gaps show ⚠; export is plain text; confidential details masked.

- [X] T026 [US4] Implement grounding in `notice_assist/mod.rs` **Ground** phase: for each `invoked_rule` (+ officer-named rules) retrieve verbatim from the **corpus** via the existing retrieval + citation gate (research R7); fill `CitedRule { found, verbatim, citation }`; misses → `found=false`.
- [ ] T027 [P] [US4] Write the show-cause/warning reply template fragment (T002) per contracts/extraction-json.md (Facts → Governing rule quoted → Application/authority → No mala fide & loss + written-orders → Relief → Documents-to-attach).
- [X] T028 [US4] Implement the **Draft** phase in `compose.rs`: assemble the reply from `Frame` + `CitedRule` via `services/drafting` compose + citation gate; render ⚠ gap lines for unfilled slots and `found=false` rules (FR-009/011); cache in `state.drafted_reply`; transition Done.
- [X] T029 [P] [US4] Ensure export works: the drafted reply is a normal assistant message exportable via existing `export_draft` (verify message shape); no new export path.
- [ ] T030 [P] [US4] Verify F020 masking covers the flow (officer answers pass through the existing client-side mask before submit; reply re-fill for display) — add a note/test in `src/pages/ChatPage.tsx` path; no new masking code expected.
- [X] T031 [P] [US4] Unit tests: template render with all-filled vs gapped slots (⚠ present), a `found=false` rule renders the "supply the text" line and never quotes from memory (FR-008), and a grounded rule renders verbatim + citation.

**Checkpoint**: US4 verifiable — end-to-end grounded, cited, exportable reply with honest gaps. **MVP (US1–US4) complete.**

---

## Phase 7: User Story 5 — Audit-para variant (P2)

**Goal**: when the document is an audit para, adapt the frame questions and the reply to the
DAC/PAC audit-response structure.
**Independent test**: an audit para yields audit-style questions and the audit reply structure.

- [X] T032 [P] [US5] Write the audit-para reply template fragment (Para & objection → Factual position → Regulatory position quoted → Status sought → Documents-to-attach) per contracts/extraction-json.md.
- [X] T033 [US5] Branch template selection in `compose.rs` on `Frame.document_kind == AuditPara` (or `/audit` alias hint) to the audit template; keep show-cause as default.
- [X] T034 [P] [US5] Add an audit-para fixture + test asserting the audit structure is used (not show-cause) and objection-scoped questions were generated.

**Checkpoint**: US5 verifiable — audit paras get the correct structure.

---

## Phase 8: Polish & Cross-Cutting

- [ ] T035 [P] Add the `/help` flow to `quickstart.md` acceptance smoke run and confirm the 6 checks pass manually (web).
- [X] T036 Run backend verification on the VM (glibc-2.31 runbook, `NOVIQ_TEST_PG_INSTALL_DIR`): `cargo check --no-default-features --features llm,server,cuda` + `cargo test --lib -- assistant::capabilities::notice_assist assistant::intent`.
- [X] T037 [P] Frontend `npm run build` (tsc + vite) green with the `/` palette.
- [ ] T038 [P] Prototype-eval the extraction prompt on 3–5 real sample notices/paras; record JSON-parse success + allegation recall (research "next experiment"); tune the prompt fragment.
- [ ] T039 [P] Append a dated `CHANGELOG_DEV.md` entry (symptom/why, files+functions, follow-ups) per CLAUDE.md.
- [ ] T040 [P] Confirm corpus rulebook coverage GA-gate (E&D/audit-PAC/Rules of Business/Civil Servants Act indexed) with the architect (research R8) — not a code task; a launch checklist item.

---

## Dependencies & completion order

- **Setup (P1)** → **Foundational (P2)** must complete before any user story.
- **US1 (P3)** depends on Foundational (T006 command detect, T008 intercept, T009 dispatcher).
- **US2 (P4)** depends on Foundational (T004 types, T005 session, T007 generator) — independent of US1's frontend.
- **US3 (P5)** depends on US2 (needs a `Frame` to gather against).
- **US4 (P6)** depends on US3 (needs gathered facts) + Foundational grounding/retrieval.
- **US5 (P7)** depends on US4 (extends the compose/template step).
- **Polish (P8)** last.

MVP = **US1 + US2 + US3 + US4** (Phases 1–6). US5 is an independent P2 increment.

## Parallel execution examples

- Setup: T002 ‖ T003 (config vs migration).
- Foundational: T004 ‖ T006 (types vs intent) before T005/T008/T009.
- US1: T010 ‖ T011 ‖ T013 (frontend) alongside backend T012.
- US2: T015 (prompt) ‖ T020 (tests scaffolding) around T016–T019.
- US4: T027 (template) ‖ T031 (tests) around T026/T028.

## Task summary

- Total: 40 tasks. Setup 3 · Foundational 6 · US1 5 · US2 6 · US3 5 · US4 6 · US5 3 · Polish 6.
- Independent test criteria stated per phase (checkpoints).
- Suggested MVP: US1–US4 (Phases 1–6). Tests included for extraction parse, slot loop, command
  detection, and template/grounding (spec acceptance + quickstart entry points).
