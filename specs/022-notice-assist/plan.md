# Implementation Plan: Notice Assist — /help Officer-Defense

**Branch**: `022-notice-assist` | **Date**: 2026-07-10 | **Spec**: [spec.md](./spec.md)
**Input**: Feature specification from `/specs/022-notice-assist/spec.md`

## Summary

A `/help` slash-command (the twin of the shipped `@`-mention) puts a chat into "assist mode"
for responding to a show-cause notice, warning, or audit para. The **local Qwen3-8B** model
(single model slot, via the existing inference seam, `NOVIQ_NO_THINK=1`, free-text JSON — not
the Mistral IR worker) reads the attached document and DERIVES the specific allegations + the
per-allegation questions (no predefined domain fields, only a universal argument *frame*). A
resumable slot-filling loop gathers the officer's facts (asking only gaps), corpus retrieval
supplies the governing rules verbatim-cited, and a template assembles a grounded reply + a
documents-to-attach checklist — flagging weak points honestly (integrity leash + written-orders
doctrine). Reuses doc-scoping/`seeks_legal_authority`, one-ask session state, drafting/export,
and F020 masking.

## Technical Context

**Language/Version**: Rust 1.87 (`src-tauri`) backend; TypeScript + SolidJS frontend (Tauri 2.0).
**Primary Dependencies**: existing only — `services/inference` (Qwen3-8B via vLLM, F016) as the
extraction generator; `query_router` retrieval + citation gate; `serde`/`serde_json` (tolerant
JSON parse); `sqlx` (Postgres); `regex_lite` (command/`/` parse); drafting `export`.
**Storage**: bundled PostgreSQL 17 + pgvector (F011/F012). New **additive migration**
`notice_assist_session(conversation_id PK, state_json JSONB/text, updated_at)`.
**Testing**: `cargo test --lib` (extraction-parse unit tests with fixture notices; slot-loop
state transitions; template rendering; grounding gap-flag). Frontend `npm run build` (tsc+vite);
`/` palette interaction mirrors the tested `@` pattern.
**Target Platform**: Linux server (headless `noviq-server`, F014) + web (SolidJS) primary; desktop
Tauri shares the same `run_turn` seam.
**Project Type**: web (frontend + backend) over the shared command/transport seam.
**Performance Goals**: one Qwen extraction call per attached document (cached in session);
`NOVIQ_NO_THINK=1`; reply generation within the existing p95 (<~18s observed on vLLM).
**Constraints**: offline-capable extraction on the **local** model (Principle I); single model
slot (Principle III); every legal statement cited verbatim (Principle XI); no llguidance.
**Scale/Scope**: one assist session per conversation; documents up to the attachment size cap
(window + heading-cued section selection when long).

## Constitution Check

*GATE: Must pass before Phase 0 research. Re-check after Phase 1 design.*

| # | Principle | Status | Notes |
|---|-----------|--------|-------|
| I | Privacy-First, Offline-Always | ✅ (feature) | Extraction pinned to the **local** Qwen3-8B; no new outbound calls. (Prod's optional remote chat engine, Groq/Gemini, is a separate pre-accepted toggle — Principle I permits user-configured endpoints.) |
| II | Tiered Intelligence | ✅ | Grounding still goes FTS/ANN first; the single Qwen call is the escalation, only for document comprehension where cheaper tiers can't parse an arbitrary notice. |
| III | Single Model Slot | ✅ | **Reuses the one resident model** (Qwen3-8B) — this is exactly why we chose it over demand-loading the Mistral 7B worker (no second model, no eviction churn). |
| IV | SQLite single source of truth | ⚠ divergence (pre-accepted) | Codebase is Postgres+pgvector (F011/F012, already tracked). New table is an **additive** Postgres migration. See Complexity Tracking. |
| V | Pending Actions Queue | ✅ n/a | No DANGEROUS tools; output is draft text the officer reviews/sends. |
| VI | Persona-as-Config (TOML) | ✅ | Extraction/analysis prompt + reply templates land as config fragments (persona/capability TOML), not hardcoded strings. |
| VII | Hardware-Adaptive | ✅ | Local model = whatever the tier serves (Qwen3-8B on the A40); degrade to single-question fallback if the model/engine is unavailable. |
| VIII | Incremental Delivery | ✅ | P1 (show-cause) ships as a usable MVP; audit-para (US5) is an independent parallel template (P2). |
| IX | Pakistan Govt Legal Domain | ✅ | Squarely in domain (E&D rules, audit/PAC, Rules of Business, Civil Servants Act). No case-law reasoning (leash forbids inventing precedent). |
| X | Officer-First UX | ✅ | The entire feature is an officer-defense workflow. |
| XI | Citation Discipline | ✅ | Governing rules quoted verbatim with corpus citations via the existing citation gate; misses flagged, never filled from memory. |
| XII | Latest Text + Amendment Chain | ✅ | Retrieved provisions are the latest consolidated text (existing behavior). |
| XIII | Cross-Reference Graph | ✅ (optional) | May use the graph for "Rules under Section X" grounding; not required for v1. |

**Gate result: PASS** (one pre-accepted divergence, §IV, logged below; no new violations).

## Project Structure

### Documentation (this feature)

```text
specs/022-notice-assist/
├── plan.md              # This file
├── research.md          # Phase 0 output
├── data-model.md        # Phase 1 output
├── quickstart.md        # Phase 1 output
├── contracts/           # Phase 1 output
│   ├── notice-assist-commands.md   # send_message routing + notice_assist_* contract
│   └── extraction-json.md          # Qwen extraction JSON schema + tolerant-parse rules
└── tasks.md             # Phase 2 (/sp.tasks — NOT created here)
```

### Source Code (repository root)

```text
src-tauri/src/
├── services/assistant/
│   ├── intent.rs                  # + is_notice_assist_command() ("/help" etc.)
│   ├── mod.rs                     # run_turn: route to NoticeAssist before capability dispatch
│   └── capabilities/
│       └── notice_assist/         # NEW capability module
│           ├── mod.rs             # orchestration: analyze → ask-loop → ground → draft
│           ├── frame.rs           # universal Frame + slot types (serde)
│           ├── extract.rs         # Qwen3-8B call + tolerant JSON parse (free-text)
│           ├── session.rs         # notice_assist_session state_json seam (save/load/delete)
│           ├── ask.rs             # next-question / record-answer / gap logic
│           └── compose.rs         # reply templates (show-cause | audit-para) via drafting
├── db/pg/migrations/              # + additive: notice_assist_session table
├── config/personas/               # + notice_assist analysis prompt + reply-template fragments (TOML)
└── api/ (query.rs)                # unchanged wire; command flows through existing send_message

src/                               # frontend (SolidJS)
├── components/ConfidentialComposer.tsx   # + "/" command palette (mirror the @ typeahead)
└── pages/ChatPage.tsx                    # surface assist-mode banner + generated questions/answers
```

**Structure Decision**: Web app over the shared `run_turn`/transport seam. NoticeAssist is a new
**capability** under `services/assistant/capabilities/`, intercepted in `run_turn` (like
transform/additive) when the turn is in assist mode or opens with `/help`. Frontend adds only a
`/` palette to the existing composer — no new page.

## Complexity Tracking

| Violation | Why Needed | Simpler Alternative Rejected Because |
|-----------|------------|--------------------------------------|
| §IV SQLite → Postgres (new `notice_assist_session` table on PG) | The whole app moved to bundled Postgres+pgvector (F011/F012); a SQLite table would fork the datastore | Using SQLite here would re-introduce a second datastore the platform already retired; the migration is additive and matches every other table |
| §II/§III "Tier-4 Worker 7B (Mistral, demand-loaded)" → main Qwen3-8B for extraction | Architect directive + it BETTER honors §III (no second model resident); Qwen3-8B is sharper and already warm on vLLM | Demand-loading the Mistral 7B worker would load a second model (eviction churn, extra RAM/VRAM) for a task the resident model does better |
| Tech-stack table lists `llama-cpp-2`; extraction runs through vLLM/inference seam | Serving moved to vLLM (F016, ADR-0018) already | Reverting to in-process llama-cpp for this one call would fork the serving path the platform standardized on |
