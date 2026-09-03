# Implementation Plan: LGD Summary & Note Drafting — Conversational Session enhancement

**Branch**: `009-lgd-drafting` | **Date**: 2026-06-09 (amended 2026-06-10) | **Spec**: [spec.md](./spec.md)
**Input**: Feature specification from `/specs/009-lgd-drafting/spec.md`

> **Scope of this refresh.** Feature 009's backend (US1–US5: render/export, template ingestion, doc-set curation, gap detection, iterative refinement) is already implemented and GREEN. This plan refresh adds the **conversational drafting session** (FR-027–FR-030), **chat persistence** (FR-031–FR-032), and **rolling-summary context compaction** (FR-033–FR-034) on top of that existing pipeline. It is an **enhancement to Feature 009, not a new feature** — all artifacts amend `specs/009-lgd-drafting/`.

> **Amendment 2026-06-10 — two live-test defects (clarify session 2026-06-10).** Live testing on the 8-PDF Sindh corpus plus a real-instrument gold diagnostic proved two shipped components defective: (D1) the heading-heuristic template ingestion (`ingest.rs`) produces garbage slots on real numbered-paragraph instruments — replaced by **Option C: Mistral-7B free-text segmentation + deterministic verbatim-verify gate** (FR-010/010a/010b, research R5a; llguidance grammar path is dead — hard abort proven in spike; refuse-not-degrade when the model is missing); (D2) gap-detection coverage matches document *titles*, not content — replaced by **two-tier content-semantic coverage** (FR-019, R11: embed `required_source_category` vs chunk embeddings; covered-by-unconfirmed-doc → one-turn suggestion, covered-by-nothing → true gap). Plus (D3) the **officer-supplied fact channel** (FR-008a, R12). Spike evidence: `~/Desktop/Noviq Documents reports/option_c_ingestion_spike_20260610/`.

## Summary

Replace the static intent form (`IntentForm.tsx`) and panel-stepped flow with a **single continuous conversational session** that drives the *existing* deterministic drafting pipeline. The officer talks; Noviq asks one question at a time and, by parsing each turn into a structured action, advances the same intent → doc-set → gap → render → refine machine already built (`session.rs`, `docset.rs`, `gaps.rs`, drafting commands). Every chat turn is persisted in `echo.db`; when the dialogue grows past a token threshold, older turns are compacted into a rolling summary so the single resident model only ever sees `summary + recent turns`. The conversation is an **interaction layer only** — it adds no generative authority over citations, numbers, or facts; the citation-grade contract (FR-004–FR-008, FR-025) is unchanged.

## Technical Context

**Language/Version**: Rust 1.87 (`src-tauri`) + TypeScript with SolidJS (frontend), Tauri 2.0
**Primary Dependencies**: sqlx 0.8 (SQLite); existing single model slot (`llama-cpp-2` 0.1.143) for prose + summarization, **plus demand-loaded Tier-4 worker Mistral-7B-Instruct-v0.3 Q4_K_M (already on disk at `models/ir_builder/` from F006) for one-time template-ingestion segmentation — free-text JSON only, NO llguidance**; existing **fastembed** embedder + `chunk_embeddings` for semantic gap coverage; existing drafting services (`services/drafting/{session,docset,gaps,ingest}.rs`, `commands/drafting.rs`); `regex_lite` for turn-action parsing; `serde`/`serde_json`; reused `decompose` citation helpers (extended hyphen-tolerant)
**Storage**: `echo.db` (single source of truth, Principle IV). **Additive migration 023** for `chat_message` + `conversation_summary`; **additive migration 024** adds `template_slot.provision_ref` (R5a); reuses migration 022 `draft_session` (already persisted via `session.rs` `DraftState`)
**Testing**: `cargo test --lib drafting` (unit), `tests/lgd_drafting_acceptance.rs` (acceptance, no-LLM); frontend exercised in `npm run tauri dev`
**Target Platform**: Tauri desktop (Linux / Windows), fully offline
**Project Type**: Web-shaped desktop app — SolidJS frontend (`src/`) + Rust backend (`src-tauri/`)
**Performance Goals**: turn-action parse is regex/deterministic (sub-ms, no model call where possible); summarization is a **single** model-slot call fired only on threshold crossing; bounded model context (keep within ~3584 `n_ctx` of the small generator)
**Constraints**: offline-always (I); at most one resident model — summarization reuses the warm slot, never loads a second model (III); additive migration only (Dev Constraints); smallest viable diff; citation-grade contract preserved on every turn
**Scale/Scope**: one officer per session; tens of turns per draft; conversation + summary retained indefinitely per session

## Constitution Check

*GATE: must pass before Phase 0. Re-checked after Phase 1.*

| Principle | Status | Notes |
|-----------|--------|-------|
| I. Privacy-First, Offline-Always | ✅ | Chat, parsing, and summarization run locally; no network. |
| II. Tiered Intelligence | ✅ | Turn parsing tries deterministic/regex (Tier 1) before any model use; summarization uses the warm orchestrator tier, not a 7B escalation. **Ingestion's Tier-4 escalation (2026-06-10) is justified empirically: Tier-1 heuristic segmentation demonstrably cannot satisfy the request (13 garbage slots on a flawless real instrument).** |
| III. Single Model Slot (NON-NEGOTIABLE) | ✅ | Summarization reuses the single resident slot. **No second model may be loaded.** Ingestion demand-loads the 7B worker exclusively for the one-time segmentation (slot occupied for the duration, evicted after) — never concurrent with another model, never on the per-draft path. |
| IV. SQLite as Single Source of Truth | ✅ | `chat_message` + `conversation_summary` added via additive migration 023 in `echo.db`. No new DB file. (The separate Postgres pivot is OUT OF SCOPE here and would require an amendment.) |
| V. Pending Actions Queue | ✅ (n/a) | No new DANGEROUS tool calls; finalize/export already gated in US5. |
| VI. Persona-as-Config | ✅ (n/a) | No new agent persona. |
| VII. Hardware-Adaptive | ✅ | Compaction threshold derived from the active tier's context budget; degrades by summarizing sooner on smaller tiers. **Ingestion on RAM tiers that cannot host the 7B: unavailable with a user-visible actionable error (FR-010b refusal) — an explicit graceful degradation, never a silent garbage schema.** |
| VIII. Incremental Delivery | ✅ | Phased: intent-chat MVP ships first, then doc-set/gap/refine fold in. |
| IX. Pakistan Govt Legal Domain | ✅ | No scope broadening. |
| X. Officer-First UX | ✅ | The conversational session is the officer-first interaction the spec targets. |
| XI. Citation Discipline (NON-NEGOTIABLE) | ✅ | FR-028 forbids the conversation from generating/altering any citation, number, or fact; all citations still resolve deterministically from chunk metadata. |
| XII. Latest Text + Amendment Chain | ✅ (n/a) | Unchanged. |
| XIII. Cross-Reference Graph | ✅ (n/a) | Unchanged. |

**Result: PASS** — no violations, no Complexity Tracking entries required.

## Project Structure

### Documentation (this feature)

```text
specs/009-lgd-drafting/
├── plan.md              # This file (refreshed)
├── research.md          # Phase 0 — appended: conversation decisions
├── data-model.md        # Phase 1 — appended: chat_message, conversation_summary
├── quickstart.md        # Phase 1 — appended: conversational flow walkthrough
├── contracts/
│   └── conversation.md  # Phase 1 — new: chat command contracts
└── tasks.md             # Phase 2 (/sp.tasks — NOT created here)
```

### Source Code (repository root)

```text
src-tauri/src/
├── db/migrations/
│   ├── 023_draft_chat.sql            # additive: chat_message + conversation_summary (SHIPPED, Phase A)
│   └── 024_slot_provision.sql        # NEW additive: template_slot.provision_ref (R5a)
├── services/drafting/
│   ├── session.rs                    # extend DraftState ↔ persisted session; resume conversation; OfficerSupplied resolution
│   ├── chat.rs                       # turn-action parser (regex_lite) → DraftAction enum (SHIPPED, Phase A; extend in Phase B)
│   ├── conversation.rs               # append/load messages (SHIPPED, Phase A); rolling-summary compaction (Phase C)
│   ├── ingest.rs                     # REWRITE (Phase D): model-assisted segmentation + verbatim-verify gate (R5a)
│   ├── docset.rs                     # reused (driven via actions)
│   └── gaps.rs                       # REWRITE coverage (Phase E): two-tier semantic content coverage (R11)
└── commands/
    └── drafting.rs                   # send_chat_turn/load_conversation (SHIPPED); ingest_template reshape (D); update_doc_set/resolve_gap reshape (E)

src/
├── pages/Drafting.tsx                # center column → conversation transcript + live draft/doc panels
├── components/drafting/
│   ├── ChatSession.tsx               # NEW: transcript + composer (replaces IntentForm as entry)
│   ├── DocSet.tsx / GapPanel.tsx / DraftView.tsx  # become live, chat-driven surfaces (FR-029)
│   └── IntentForm.tsx                # retired once ChatSession reaches parity
├── stores/drafting.ts                # add conversation state
└── api/drafting.ts                   # add sendChatTurn / loadConversation bindings
```

**Structure Decision**: Extend the existing Tauri front/back split. Backend gains two focused services (`chat.rs` = parse, `conversation.rs` = persist+compact) plus a migration; the existing `session.rs`/`docset.rs`/`gaps.rs` keep their contracts and are *invoked by parsed actions* rather than by panel callbacks. Frontend replaces the form entry with a transcript while keeping doc/draft panels as live, chat-synced surfaces.

## Implementation Phasing (carried into /sp.tasks)

The spec scope is full-B, but implementation is phased per Principle VIII so each step is shippable:

1. **Phase A — Intent-chat MVP, end-to-end. ✅ SHIPPED `ee4964c6`.** Migration 023; `chat.rs` parse for intent fields; `conversation.rs` append+load (no compaction yet); `send_chat_turn` command; `ChatSession.tsx` collecting intent conversationally and calling existing `start_draft`. Replaces `IntentForm` at parity. Persistence proven on the smallest surface.
2. **Phase B — Fold the pipeline into the transcript.** Extend the action parser + command to cover confirm/add/remove document, resolve/declare-unavailable gap, accept-suggestion, officer-supplied fact value, targeted revision, finalize — each mapped to the existing deterministic path. DocSet/Gap/DraftView become live chat-synced panels (FR-029). Ambiguous-turn clarifying questions (FR-030).
3. **Phase C — Rolling-summary compaction.** `conversation.rs` threshold + single-slot summarization; supply `summary + recent turns` to the model; preserve decision-relevant state (FR-033/034); resume restores conversation + state (FR-032).
4. **Phase D — Ingestion rewrite (Option C, defect D1).** Migration 024 (`provision_ref`); `ingest.rs` rewrite: Tier-4 Mistral-7B free-text segmentation prompt + tolerant JSON parse + deterministic verbatim-verify gate (hyphen-tolerant `resolve_provision_label`, category-from-provision, boilerplate exclusion, low-confidence flag-for-review); `ingest_template` contract reshape; refuse-not-degrade on missing model (FR-010b). Acceptance anchor = the real gold (GOLD_SPSC_age_relaxation_2025): actual citations land in slots, boilerplate never cited. Independent of B/C — can ship first.
5. **Phase E — Semantic two-tier gap coverage + officer-supplied facts (defects D2+D3).** `gaps.rs` coverage rewrite (embed category vs chunk embeddings, threshold calibrated on the 8-PDF live corpus); two-tier `Gap` shape + `acceptSuggestion`/`officerSupplied` resolutions through `update_doc_set`/`resolve_gap`; officer-supplied `FactBlock` attribution in render. Independent of B/C; pairs naturally with D.

**Recommended order: D → E → B → C** — D and E fix shipped defects blocking the live pilot (the user's corpus is indexed and waiting); B and C extend UX on top of corrected foundations.

*Deferred (not this cut):* semantic retrieval over stored turns (rolling summary only); vec0 ANN for `retrieve_whole_corpus` (brute-force scan fine at pilot scale).

## Complexity Tracking

No constitution violations — section intentionally empty.
