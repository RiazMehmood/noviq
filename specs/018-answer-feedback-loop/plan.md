# Implementation Plan: Answer Feedback Loop (Beta)

**Branch**: `018-answer-feedback-loop` | **Date**: 2026-07-03 | **Spec**: [spec.md](./spec.md)
**Input**: Feature specification from `specs/018-answer-feedback-loop/spec.md`

## Summary

Add lightweight in-product feedback on every answer: **two single-select citation questions**
(accuracy + completeness) rendered as one-tap chips under the assistant turn in the web chat.
The response persists to a new additive PostgreSQL table `answer_feedback`, keyed to the exact
`chat_message.id` already returned to the frontend in `TurnResponse.message_id`. The full answer
trace required for diagnosis/training (FR-004) already exists — `query_log` (one row per turn:
question, answer, citations, latency, resource peak) plus the on-disk query-trace files exposed by
`/api/traces/*` — so this feature reuses it rather than building new trace capture. Scope of this
plan is **User Story 1 (P1) only**; US2 (implicit signals) and US3 (SME review queue + golden/
regression sets) are outlined as follow-on phases and are not built here.

Technical approach: one migration (`008_answer_feedback.sql`), one repository
(`db/repositories/answer_feedback.rs`), one route (`POST /api/feedback`), one transport helper
(`submitFeedback`) and one small SolidJS component (`CitationFeedback.tsx`) mounted in
`ChatPage.tsx` beside the existing citations block. Latest-write-wins on re-tap (supersede).

## Technical Context

**Language/Version**: Rust 1.87 (`src-tauri`, `noviq-server` bin) + TypeScript / SolidJS (frontend)
**Primary Dependencies**: axum (HTTP), sqlx 0.8 (postgres, runtime queries — no compile-time macros),
Tauri transport seam (`src/api/transport.ts`), existing `run_turn` assistant pipeline. No new crates.
**Storage**: Bundled PostgreSQL 17 + pgvector at `{app_data_dir}/pg/data`. New **additive** migration
`008_answer_feedback.sql` auto-applied by `sqlx::migrate!("./src/db/pg/migrations")` (`db/schema.rs:53`).
**Testing**: `cargo test` (repository insert/supersede/aggregate; compiled on the GPU VM — no Rust on
Windows). Frontend: manual browser verification of the two-chip control (no FE test harness in repo).
**Target Platform**: Headless Linux server (F014) on the GPU VM, browser client; desktop Tauri shares
the same transport seam and component.
**Project Type**: Web (Rust backend + SolidJS frontend), single repo.
**Performance Goals**: Feedback write is off the chat hot path (separate POST); target < 50 ms insert,
never blocks or fails the chat turn.
**Constraints**: Offline / local-only — **zero external telemetry** (Principle I, FR-011). Additive
migration only (Principle: Development Constraints, FR-013). Officer-first one-tap UX (Principle X).
**Scale/Scope**: Beta cohort (tens of users); one feedback row per rated turn; unbounded but tiny.

## Constitution Check

*GATE: Must pass before Phase 0 research. Re-check after Phase 1 design.*

| Principle | Status | Note |
|-----------|--------|------|
| I. Privacy-First, Offline-Always | ✅ PASS | Feedback stored in local PG; no network calls (FR-011, SC-006). |
| II. Tiered Intelligence | ✅ N/A | No LLM call added. |
| III. Single Model Slot | ✅ N/A | No model loaded. |
| IV. SQLite as Single Source of Truth | ⚠️ DIVERGENCE (pre-existing) | Storage is bundled PostgreSQL, not SQLite — the F011/F012 port already diverged here; amendment pending. This feature adds **one additive table** to the existing PG store; it does **not** widen the divergence. See Complexity Tracking. |
| V. Pending Actions Queue | ✅ N/A | No dangerous tool call. |
| VI. Persona-as-Config | ✅ N/A | No agent/tool change. |
| VII. Hardware-Adaptive | ✅ PASS | Trivial insert; no tier gating. |
| VIII. Incremental Feature-App Delivery | ✅ PASS | US1 ships independently as MVP; US2/US3 deferred. |
| IX. Pakistan Legal Domain | ✅ PASS | Feedback is about legal citations specifically. |
| X. Officer-First UX | ✅ PASS | Two one-tap questions, no free-text burden; citation focus is the officer's core concern. |
| XI. Citation Discipline | ✅ REINFORCES | The feedback *measures* citation accuracy/completeness — directly serves this principle. |
| XII. Latest Text + Amendment Chain | ✅ N/A | Unaffected. |
| XIII. Cross-Reference Graph | ✅ N/A | Unaffected. |

**Gate result**: PASS (one pre-existing, already-documented divergence on Principle IV; not widened).

## Project Structure

### Documentation (this feature)

```text
specs/018-answer-feedback-loop/
├── spec.md              # Feature spec (clarified 2026-07-03)
├── plan.md              # This file
├── research.md          # Phase 0 — anchor choice, trace reuse, supersede semantics
├── data-model.md        # Phase 1 — answer_feedback schema + enums
├── quickstart.md        # Phase 1 — how to run/verify end to end
├── contracts/
│   └── feedback-api.md  # POST /api/feedback + GET /api/admin/feedback/summary
└── tasks.md             # Phase 2 — /sp.tasks (NOT created here)
```

### Source Code (repository root)

```text
src-tauri/src/
├── db/
│   ├── pg/migrations/
│   │   └── 008_answer_feedback.sql        # NEW — additive table + indexes
│   └── repositories/
│       ├── mod.rs                          # + pub mod answer_feedback;
│       └── answer_feedback.rs              # NEW — upsert (supersede) + summary
├── api/
│   ├── feedback.rs                         # NEW — POST /api/feedback handler
│   ├── admin.rs                            # + GET /api/admin/feedback/summary (FR-009)
│   ├── router.rs                           # + .route("/feedback", post(...)) etc.
│   └── mod.rs                              # + pub mod feedback;

src/                                         # SolidJS frontend
├── api/
│   ├── transport.ts                        # + submitFeedback() web/desktop seam
│   └── assistant.ts                        # + FeedbackInput type + call wrapper
├── components/
│   └── CitationFeedback.tsx                # NEW — two one-tap chip rows
└── pages/
    └── ChatPage.tsx                        # mount <CitationFeedback> under citations
```

**Structure Decision**: Web app layout (existing). This feature slots into the established F014
server API + shared transport seam; no new top-level directories. The feedback UI reuses the
per-turn `resp` object already held in `ChatPage.tsx` (carries `message_id` + `citations`).

## Phases

- **Phase 0 (research.md)** — resolve: (a) feedback anchor = `chat_message.id` vs `query_log.id`;
  (b) is FR-004 trace already satisfied; (c) re-tap / supersede semantics; (d) unauthenticated /
  desktop (NULL user) handling.
- **Phase 1 (data-model.md, contracts/, quickstart.md)** — `answer_feedback` schema + two enums;
  `POST /api/feedback` and `GET /api/admin/feedback/summary` contracts; end-to-end verify steps.
- **Phase 2 (/sp.tasks)** — dependency-ordered tasks (migration → repo → route → transport → UI →
  admin summary → VM compile + browser verify). Not produced by this command.

**Scope for this plan**: User Story 1 (P1). Follow-on (separate plan/tasks):
- US2 implicit signals (copy / regenerate / follow-up) — additive columns or sibling table.
- US3 SME review queue + golden/regression set promotion + eval-harness consumption.
- FR-005 per-citation feedback (v1.1).
- FR-014 beta-exit thresholds dashboard (needs architect-set numbers).

## Complexity Tracking

| Violation | Why Needed | Simpler Alternative Rejected Because |
|-----------|------------|--------------------------------------|
| Storage on PostgreSQL, not SQLite (Principle IV) | The deployed F014 server already runs on bundled PostgreSQL (F011/F012 port; ADR-0015). Feedback must live beside `chat_message`/`query_log` it references. | Writing feedback to a separate SQLite file would fork the source of truth and break the FK/JOIN to `chat_message` and `query_log` — strictly worse. Divergence is pre-existing and amendment-pending; this feature only adds one additive table. |
