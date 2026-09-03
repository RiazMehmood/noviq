---
description: "Task list for Answer Feedback Loop (Beta) — US1 MVP"
---

# Tasks: Answer Feedback Loop (Beta)

**Input**: Design documents from `specs/018-answer-feedback-loop/`
**Prerequisites**: plan.md, spec.md, research.md, data-model.md, contracts/feedback-api.md, quickstart.md

**Tests**: Backend repository unit tests are included (explicitly called for in plan.md/quickstart.md:
insert / supersede / summary). No frontend test harness exists in the repo → browser verification.

**Scope**: This plan implements **User Story 1 (P1)** only — the two-question citation feedback MVP,
plus the FR-009 operator aggregate as a separable cross-cutting phase. US2 (implicit signals) and
US3 (SME queue + golden/regression sets) are **out of scope** here (separate plan/tasks).

**Rust builds on the GPU VM** (no Rust toolchain on Windows) — see the VM compile runbook in memory.

## ⏱️ Checkpoint — 2026-07-03 (progress saved)

**DONE (code written on `release`, frontend typechecks `tsc --noEmit` EXIT 0; Rust NOT yet compiled):**
- T002 migration `008_answer_feedback.sql`
- T003 repository `answer_feedback.rs` (`upsert` supersede/merge + `current_for` + `summary`)
- T004 module wiring in `db/repositories/mod.rs`
- T006 `api/feedback.rs` `submit` handler (+ `bad_request` helper in `api/mod.rs`)
- T007 `pub mod feedback;` + routes `/feedback` & `/submit_feedback` in `router.rs`
- T009 `submitFeedback` + `FeedbackInput`/enum types in `src/api/assistant.ts`
- T010 `src/components/CitationFeedback.tsx` (two one-tap chip rows, locked labels)
- T011 mounted `<CitationFeedback>` in `src/pages/ChatPage.tsx` (under Sources/badges)
- T012 `admin::feedback_summary` handler + T013 route `/admin/feedback/summary`

**REMAINING (resume here):**
- **T005** repo unit tests — harness is `crate::db::test_pool().await` (runs migrations, so `008`
  applies; `answer_feedback` has no FK so `upsert` takes any `message_id`). Cover insert / supersede /
  merge / summary-current-only.
- **T015** VM compile: `cargo check -p noviq-server --features server` + `cargo test answer_feedback`.
- **T016** run `quickstart.md` (psql `\d`, curl taps, browser flow, offline check).
- **T017** CHANGELOG_DEV entry (DONE at checkpoint — see 2026-07-03 entry).
- **T018** green PHR (DONE at checkpoint).
- **T008** desktop Tauri `submit_feedback` command — DEFERRED (web is the beta surface).
- **T014** admin FE panel for the summary — DEFERRED (endpoint ready; UI later).
- **T019** reload re-hydration via `current_for` — DEFERRED follow-up.

**Not yet done: commit.** All changes are unstaged on `release`.

## Format: `[ID] [P?] [Story] Description`

- **[P]**: Can run in parallel (different files, no dependencies)
- **[Story]**: US1 for user-story tasks; Setup/Foundational/Aggregate/Polish carry no story label

---

## Phase 1: Setup

**Purpose**: Branch off for the feature work (repo is already initialized).

- [ ] T001 Create branch `018-answer-feedback-loop` from `release` and confirm `specs/018-answer-feedback-loop/` artifacts are present

---

## Phase 2: Foundational (Blocking Prerequisites)

**Purpose**: The `answer_feedback` storage + repository. BLOCKS all user-story work.

**⚠️ CRITICAL**: No US1 task can begin until this phase is complete.

- [ ] T002 Create additive migration `src-tauri/src/db/pg/migrations/008_answer_feedback.sql` — table `answer_feedback` (id, message_id, conversation_id, user_id, citation_accuracy, citation_completeness, supersedes, superseded_at, created_at), the three CHECK constraints, `uq_answer_feedback_current` partial unique index, and `idx_answer_feedback_message` / `idx_answer_feedback_created`, exactly per `data-model.md`
- [ ] T003 Create repository `src-tauri/src/db/repositories/answer_feedback.rs` — `upsert(pool, message_id, conversation_id, user_id, accuracy?, completeness?)` implementing latest-wins supersede (find current row → merge other set column → stamp `superseded_at` on prior → INSERT new current row with `supersedes`), `current_for(pool, message_id, user_id)` read, and `summary(pool, from?, to?)` returning the two GROUP BY distributions + `total_current`
- [ ] T004 Register the repository: add `pub mod answer_feedback;` to `src-tauri/src/db/repositories/mod.rs`
- [ ] T005 [P] Add `#[cfg(test)]` unit tests in `src-tauri/src/db/repositories/answer_feedback.rs` covering: (a) first insert creates one current row; (b) second tap on same message supersedes (old row gets `superseded_at`, new row `supersedes` set, still one current row); (c) accuracy-then-completeness merges into one current row with both columns; (d) `summary` counts only current rows

**Checkpoint**: `answer_feedback` migrates and the repository round-trips (insert/supersede/summary).

---

## Phase 3: User Story 1 - Two-question citation feedback (Priority: P1) 🎯 MVP

**Goal**: On every answer, the officer taps one chip in each of two rows (citation accuracy +
completeness); the choice persists to `answer_feedback` keyed to `chat_message.id`.

**Independent Test**: Ask a question in the browser, tap a chip in each row, confirm a current
`answer_feedback` row holds both values for that `message_id` (per quickstart §3–4).

### Backend (endpoint)

- [ ] T006 [US1] Create `src-tauri/src/api/feedback.rs` — `submit` handler behind the `AuthUser` extractor: parse `{message_id, conversation_id, citation_accuracy?, citation_completeness?}` (serde aliases for camelCase like `/api/query`), validate message_id present + at least one value + enum membership (400 on bad enum), derive `user_id` from the session, call `answer_feedback::upsert`, return `{ "ok": true }`
- [ ] T007 [US1] Wire the route: add `pub mod feedback;` to `src-tauri/src/api/mod.rs` and `.route("/feedback", post(feedback::submit))` to the authenticated group in `src-tauri/src/api/router.rs`
- [ ] T008 [US1] Add the desktop Tauri command alias `submit_feedback` routing to the same handler via the compat layer (mirror how `send_message` aliases `/api/query`), so the shared transport works on desktop

### Frontend (two-chip control)

- [ ] T009 [P] [US1] Add `FeedbackInput` type + `submitFeedback(f)` to `src/api/transport.ts` — Tauri → `invoke("submit_feedback", …)`; web → `POST /api/feedback` with the bearer token (per contracts/feedback-api.md)
- [ ] T010 [P] [US1] Create `src/components/CitationFeedback.tsx` — two single-select chip rows using the locked labels (accuracy: All citations correct / Some citations correct / Citations wrong/missing / Not sure → `all|some|wrong|unsure`; completeness: Fully cited / Some citations missing / Not applicable → `yes|missing|na`); tracks local selection, highlights the chosen chip, calls `submitFeedback` on each tap (re-tap moves selection); no free-text, no 👍/👎
- [ ] T011 [US1] Mount `<CitationFeedback messageId={resp().message_id} conversationId={resp().conversation_id} />` under the citations block in `src/pages/ChatPage.tsx` (assistant turns only, when `resp().message_id` is non-empty)

**Checkpoint**: US1 fully functional — chips render under every answer, taps persist, MVP demoable.

---

## Phase 4: Operator aggregate view (FR-009) — cross-cutting

**Purpose**: Let an operator see the distribution of both citation metrics to prioritize fixes.
Separable from the MVP; can ship in a second pass.

- [ ] T012 Add `feedback_summary` handler in `src-tauri/src/api/admin.rs` behind the `AdminUser` extractor — optional `from`/`to` epoch-ms bounds, returns `{ accuracy:{…}, completeness:{…}, total_current }` from `answer_feedback::summary`
- [ ] T013 Wire the route: add `.route("/admin/feedback/summary", get(admin::feedback_summary))` to the admin group in `src-tauri/src/api/router.rs`
- [ ] T014 [P] Surface the two distributions in the existing admin usage panel (frontend admin page) — small read-only bar/summary reusing the current admin fetch pattern

---

## Phase 5: Polish & Cross-Cutting Concerns

- [ ] T015 On the GPU VM (throwaway worktree + persistent `CARGO_TARGET_DIR` per runbook): `cargo check -p noviq-server --features server` and `cargo test -p echo-ai answer_feedback` — both EXIT 0
- [ ] T016 Run `quickstart.md` verification: `\d answer_feedback` + migration `008` present, curl accuracy/completeness taps → one merged current row, admin summary returns distributions, browser US1 flow, and confirm zero external network egress (Principle I / SC-006)
- [ ] T017 [P] Append a dated entry to `CHANGELOG_DEV.md` (symptom/why → fix with file paths+functions → cross-cutting impact/follow-ups) per CLAUDE.md
- [ ] T018 [P] Create the `/sp.tasks` PHR and, after implementation, the green PHR under `history/prompts/018-answer-feedback-loop/`
- [ ] T019 (DEFERRED follow-up) Re-hydrate a user's prior selection across page reload — add a `current_for` read endpoint (`GET /api/feedback/current?message_id=…`) and highlight the stored chip on `ChatPage` load; MVP keeps in-session selection only

---

## Dependencies & Execution Order

### Phase dependencies

- **Setup (P1)**: none.
- **Foundational (P2)**: after Setup. **BLOCKS US1 and the aggregate** (both need the table + repo).
- **US1 (P3)**: after Foundational. Delivers the MVP.
- **Aggregate (P4)**: after Foundational (needs `answer_feedback::summary`); independent of US1 UI.
- **Polish (P5)**: after the phases it validates.

### Within Foundational

- T002 (migration) → T003 (repo uses the table) → T004 (module wiring) → T005 (tests run against repo).

### Within US1

- Backend T006 → T007 (route needs the handler) → T008 (desktop alias).
- Frontend T009 and T010 are independent [P]; T011 mounts the component (needs T010) and exercises
  the endpoint (needs T006–T007 to test end to end).

### Parallel opportunities

- T005 [P] runs alongside the rest of Foundational once T003 lands.
- T009 and T010 [P] (transport vs component — different files) can be built together.
- T014 [P] (admin FE) and T017/T018 [P] (docs/PHR) are independent of code paths.

---

## Parallel Example: User Story 1

```bash
# Frontend seam + component in parallel (different files, no shared deps):
Task: "Add FeedbackInput + submitFeedback to src/api/transport.ts"          # T009
Task: "Create src/components/CitationFeedback.tsx (two chip rows)"          # T010
```

---

## Implementation Strategy

### MVP first (US1 only)

1. Phase 1 Setup → Phase 2 Foundational (migration + repo + tests).
2. Phase 3 US1 (endpoint + transport + component + mount).
3. **STOP & VALIDATE**: browser tap → `answer_feedback` current row (quickstart §3–4).
4. Deploy to `release` (auto-deploys to the VM); demo.

### Incremental delivery

1. Foundational → US1 → **ship the MVP** (collection only).
2. Add Phase 4 aggregate (FR-009) → operator can see distributions → ship.
3. Later plans: US2 implicit signals, US3 SME queue + golden/regression sets, FR-005 per-citation,
   FR-014 beta-exit thresholds, T019 reload re-hydration.

---

## Notes

- [P] = different files, no incomplete-task dependency.
- Feedback writes are off the chat hot path — a feedback failure must never break a chat turn.
- Migration `008` is additive and auto-applied by `sqlx::migrate!("./src/db/pg/migrations")`
  (`db/schema.rs:53`); an older binary stays valid against the new schema (safe rollback).
- Anchor is `chat_message.id` (already in `TurnResponse.message_id`) — no change to the query path.
- Commit after each task or logical group.
