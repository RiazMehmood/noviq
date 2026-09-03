# Implementation Plan: One-Ask Drafting

**Branch**: `010-one-ask-drafting` | **Date**: 2026-06-11 | **Spec**: [spec.md](spec.md)
**Input**: Feature specification from `/specs/010-one-ask-drafting/spec.md`

## Summary

Officer types one sentence; the app autonomously researches the indexed Sindh legal corpus and generates the complete middle content of a government summary (numbered paragraphs, five-section flow, no boilerplate) ready to copy-paste — replacing the 009 template/slot officer path. Technical approach: deterministic intent parse (regex) → per-section vec0 **KNN** retrieval (fixes the 281 s brute-force scan) → verbatim **provision cards** labelled via the existing `decompose` citation resolver → per-section **Mistral-7B free-text** prose with `[CITE:n]`/`[FIG:n]` markers only → deterministic splice + draft-level verbatim-verify gate (numbers/quotes only from cards or officer figures; everything else flagged, never invented) → persisted conversation with targeted per-section revision, progress events, and cancel.

## Technical Context

**Language/Version**: Rust 1.87 (workspace `echo-ai`, `src-tauri`) + TypeScript with SolidJS (frontend), Tauri 2.0
**Primary Dependencies**: sqlx 0.8 (SQLite), sqlite-vec (vec0), fastembed 4, llama-cpp-2 0.1.143 (free-text only — llguidance crashes, proven twice), regex_lite, serde/serde_json, tokio. Reused services: `doc_ir::mistral_builder::discover_ir_builder_gguf`, single ModelSlot (Principle III), `intent_router::decompose` citation helpers (hyphen-tolerant `resolve_provision_label`), drafting `conversation.rs` (chat_message), cancellation-token registry pattern (F006), `drafting://` event channels
**Storage**: `echo.db` (single source of truth, Principle IV). Reuses `chat_message` (migration 023). New **additive migration 025** `one_ask_session(draft_id PK, state_json, updated_at)`. 009 tables untouched; `template_schema` rows retained as future style examples
**Testing**: cargo test (lib + integration with canned model output — no live LLM in CI tests), `npx tsc --noEmit`; KNN↔brute-force parity test on fixture corpus; acceptance test per quickstart
**Target Platform**: Linux desktop (Tauri), CPU-only inference; Windows later per existing backlog
**Project Type**: Single Tauri app (Rust backend `src-tauri/` + SolidJS frontend `src/`)
**Performance Goals**: research stage ≤ 15 s (SC-005, via vec0 KNN `MATCH ?/k=?`); end-to-end draft ≤ 15 min, revision turn ≤ 5 min on CPU (SC-003); no silent wait > 10 s (progress events)
**Constraints**: fully offline (Principle I); single model slot, 7B demand-loaded/evicted (Principle III); additive migrations only; zero unflagged fabricated citations (SC-002, Principle XI); smallest viable diff — 009 backend left in place, only officer-facing path replaced (FR-015)
**Scale/Scope**: pilot corpus ~10 legal PDFs / ~100k+ chunk embeddings (65 GB DB observed); single officer per machine; one draft session active at a time
**Portability (ADR-0014)**: a SQLite → PostgreSQL(+pgvector) client/server re-platform is planned post-drafting (parked; needs constitution §IV amendment). 010 designs accordingly: ALL vector search goes through the single `retrieval.rs` seam (vec0 `MATCH/k` inside, pgvector-swappable); `one_ask_session` access confined to `state.rs` with versioned `state_json` (→ JSONB); new SQL stays ANSI-portable (TEXT ISO timestamps, TEXT uuid ids); no inlined vec0 SQL at call sites; pool handles passed in, never constructed

## Constitution Check

*GATE: evaluated pre-Phase-0 and re-checked post-Phase-1 — **PASS** (no violations; no Complexity Tracking entries).*

| # | Principle | Status | Notes |
|---|-----------|--------|-------|
| I | Privacy-First, Offline-Always | ✅ | All inference local (Mistral-7B GGUF on disk); zero cloud. User explicitly chose local over cloud. |
| II | Tiered Intelligence | ✅ | Intent parse = regex (Tier 1); research = ANN (Tier 2); prose = Tier 4 worker, justified — spike proved 3B/0.5B unfaithful for connective prose; no lower tier can satisfy SC-002 risk posture. |
| III | Single Model Slot | ✅ | 7B demand-loaded through the existing ModelSlot, evicted after idle; never co-resident with another model. |
| IV | SQLite Single Source of Truth | ✅ | Migration 025 additive table in `echo.db`; no new files/DBs; state_json + chat_message. |
| V | Pending Actions Queue | ✅ N/A | No DANGEROUS tool calls; feature is read-corpus → write-DB-state only. |
| VI | Persona-as-Config | ✅ N/A | No agent personas; writer prompt is a versioned Rust const (not an agent persona). |
| VII | Hardware-Adaptive | ✅ | Tiers that cannot host the 7B fail fast with human-readable remediation (no silent degrade to unfaithful writer). |
| VIII | Incremental Delivery | ✅ | US1 (one ask → draft) alone is a usable MVP; US2 gate ships with it; US3/US4 layered after. |
| IX | Pakistan Govt Legal Domain | ✅ | Summary instrument for Sindh secretariat practice; corpus = Acts/Rules/Regulations/Manuals. |
| X | Officer-First UX | ✅ | The whole feature exists because officers rejected the slot ceremony; one-ask + copy-paste is the officer default. |
| XI | Citation Discipline | ✅ | Citations only from provision cards (verbatim chunk + resolved label); canonical render = label + source doc + hover-traceable matched text; unverifiable ⇒ flagged, release-blocking otherwise. |
| XII | Latest Text + Amendment Chain | ✅ | Cards carry the indexed chunk's label/amendment info as stored by existing indexing; no point-in-time reconstruction. |
| XIII | Cross-Reference Graph | ➖ deferred | Graph-aided retrieval ("Rules under Section X") noted in research R5 as enhancement; per-section ANN retrieval does not re-derive graph edges, it simply doesn't consume them yet. No violation. |

## Project Structure

### Documentation (this feature)

```text
specs/010-one-ask-drafting/
├── spec.md
├── plan.md              # This file
├── research.md          # Phase 0 (complete)
├── data-model.md        # Phase 1 (complete)
├── quickstart.md        # Phase 1 (complete)
├── contracts/
│   └── one-ask-commands.md
├── checklists/requirements.md
└── tasks.md             # Phase 2 (/sp.tasks — NOT created by /sp.plan)
```

### Source Code (repository root)

```text
src-tauri/src/
├── db/migrations/
│   └── 025_one_ask_session.sql        # NEW additive
├── services/drafting/
│   ├── mod.rs                          # add module exports
│   ├── one_ask/                        # NEW module
│   │   ├── mod.rs
│   │   ├── intent.rs                   # R6: regex intent parse (subject, figures, action)
│   │   ├── skeleton.rs                 # R4: 5-section const skeleton + guidance + role hints
│   │   ├── research.rs                 # R5: per-section queries → provision cards (uses retrieval::knn)
│   │   ├── compose.rs                  # R2: writer prompts, [CITE:n]/[FIG:n] marker parse
│   │   ├── verify.rs                   # R3: splice + verbatim gate + stray-number scan → flags
│   │   └── state.rs                    # OneAskState (serde), save/load via one_ask_session
│   ├── retrieval.rs                    # NEW shared vec0 KNN MATCH/k=? helper (R1)
│   └── conversation.rs                 # REUSE as-is (chat_message)
├── services/intent_router/decompose.rs # REUSE: resolve_provision_label (pub(crate) → pub as needed)
└── commands/
    └── one_ask.rs                      # NEW: start/revise/cancel/get commands + events

src/ (frontend)
├── components/drafting/
│   ├── OneAskView.tsx                  # NEW: ask box + progress stages + draft pane
│   │                                   #   (verified citation hover, flag styling, copy button)
│   └── (TemplateManager/GapPanel/ChatSession removed from officer routing — files retained)
└── api/oneAsk.ts                       # NEW invoke wrappers + event subscriptions

src-tauri/tests/
├── one_ask_acceptance.rs               # canned-model end-to-end (no LLM)
└── fixtures/one_ask/                   # fixture corpus + canned writer outputs
```

**Structure Decision**: Single Tauri app (existing layout). New code is isolated in `services/drafting/one_ask/` + one new command module + one new frontend view; the only edits to existing files are module wiring (`mod.rs`, `lib.rs` invoke handler, drafting tab routing) and making `resolve_provision_label` reachable. 009 modules are not modified (FR-015 removal is frontend routing only) — smallest viable diff.

## Architecture & Flow (Phase 1 design summary)

```text
officer sentence
  └─ intent.rs (regex): subject / figures[] / action          [deterministic]
       └─ research.rs: per skeleton section →
            embed(query) → retrieval::knn(k≈40, over-fetch)   [≤15 s total]
            → hydrate/filter indexed sources
            → provision cards (verbatim excerpt + label via decompose + source)
       └─ compose.rs: per section → 7B [INST] prompt
            (guidance + cards + [FIG:n] table) → free text with markers
       └─ verify.rs: splice cards at [CITE:n], figures at [FIG:n];
            verbatim substring gate per quote;
            stray number/figure scan → CitationFlag runs
       └─ state.rs: OneAskState{status, paragraphs[runs]} → one_ask_session
            every stage → emit drafting://one-ask-progress
            done → emit drafting://one-ask-complete
revision turn ("make para 3 shorter")
  └─ deterministic turn parse → owning section → re-run compose+verify for
     that section only → re-emit complete                     [FR-009]
```

**Citation-grade invariant (unchanged from 009/ADR-0012 philosophy)**: the model proposes *placement* (markers); deterministic code owns *content* (verbatim splice + gate). A fabricated number can only ever appear as a flagged run, never as a citation.

## Complexity Tracking

*No constitution violations — table intentionally empty.*
