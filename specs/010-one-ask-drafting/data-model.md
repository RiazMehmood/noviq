# Data Model: One-Ask Drafting (010)

**Date**: 2026-06-11 · **Plan**: [plan.md](plan.md)

## Storage overview

- `echo.db` only (Principle IV). One new **additive migration 025**.
- Conversation turns reuse `chat_message` (migration 023) keyed by `draft_id` — unchanged schema.
- 009 tables (`template_schema`, `template_slot`, `draft_session`) untouched.

## Migration 025 (additive)

```sql
-- 025_one_ask_session.sql
CREATE TABLE IF NOT EXISTS one_ask_session (
    draft_id    TEXT PRIMARY KEY,           -- uuid
    state_json  TEXT NOT NULL,              -- serde OneAskState (shape below)
    updated_at  TEXT NOT NULL
);
```

Rationale (research R7): parallel table avoids mixing serde shapes with 009 `draft_session.state_json`; resume (FR-010) stays deterministic for both old and new sessions.

## Rust entities (serde, stored in `state_json`)

### OneAskState — root aggregate

| Field | Type | Notes |
|---|---|---|
| version | u32 | state-shape version, starts at 1 |
| status | Status | `Researching` → `Composing{section}` → `Verifying` → `Ready` \| `Failed{reason}` \| `Cancelled` |
| intent | OneAskIntent | parsed from the officer sentence |
| cards | Vec\<ProvisionCard\> | indexed by position = CITE id `n` |
| paragraphs | Vec\<Paragraph\> | the draft body |
| finalized | bool | copy is allowed any time; finalize locks revisions |

State transitions: `Researching → Composing(1..=5) → Verifying → Ready`; any state → `Failed`/`Cancelled`; `Ready → Composing(s)` on a revision turn targeting section *s* (re-verify follows); `Ready → Ready(finalized=true)` on finalize. Mid-generation restart ⇒ session reloads as `Failed{interrupted}` and is re-runnable (edge case, spec).

### OneAskIntent (FR-003)

| Field | Type | Notes |
|---|---|---|
| raw | String | the officer's sentence, verbatim — also the retrieval seed |
| figures | Vec\<Figure\> | `Figure{ text: String /* verbatim, e.g. "4 billion" */, normalized: Option<String> }` — indexed by position = FIG id `n` |
| action | Option\<String\> | matched action keyword (get/sanction/approve/…) |

Validation: `raw` non-empty after trim; figures captured losslessly (verbatim substring of `raw`).

### ProvisionCard (FR-004/005 — the only citation source)

| Field | Type | Notes |
|---|---|---|
| chunk_id | String | FK by value to `chunks.id` |
| source_id | String | FK by value to `sources.id` |
| source_title | String | file_name at research time |
| excerpt | String | verbatim chunk text (or sub-span) |
| label | Option\<String\> | resolved provision identifier via `decompose::resolve_provision_label` (e.g. "Rule 6(c), SPSC Regulations 2023"); None ⇒ card usable only as quoted text with source, not as a numbered citation |
| section_ids | Vec\<SectionId\> | which skeleton sections retrieved it (deduped across sections) |

Invariant: `excerpt` MUST be a verbatim (whitespace/hyphen-normalised) substring of the indexed chunk — enforced at construction and re-checked by the verify gate.

### Skeleton (compile-time const, FR-016 — not stored)

`SectionId` enum: `BackgroundPurpose`, `LegalProceduralPosition`, `FinancialAspect`, `ProposalRecommendation`, `PrayerForApproval`. Each const entry: `title`, `guidance` (writer instruction), `role_hints` (retrieval query fragments), `wants_figures: bool` (FinancialAspect true).

### Paragraph / Run (the draft body, FR-002/006/007/008)

| Paragraph field | Type | Notes |
|---|---|---|
| number | u32 | 1-based, continuous across sections |
| section | SectionId | owning skeleton section (revision routing) |
| runs | Vec\<Run\> | ordered inline content |

`Run` (tagged enum):

| Variant | Payload | Meaning |
|---|---|---|
| Prose | text | model narrative, post-gate (no foreign numbers) |
| Citation | card_idx, rendered_label, quoted: Option\<String\>, verified: bool | spliced from `cards[card_idx]`; `verified` MUST be true to render as citation |
| Figure | fig_idx, text | officer-stated figure, verbatim from intent |
| Flag | reason, original: Option\<String\> | unverifiable content (FR-007) — visually distinct, never silent |

Invariants (verify.rs gate, SC-002):
1. Every `Citation.quoted` is a verbatim substring of `cards[card_idx].excerpt`.
2. No `Prose` run contains a provision-number pattern or monetary figure (those become `Citation`/`Figure`/`Flag` runs).
3. `Figure.text` ∈ `intent.figures[*].text`.
4. A failed check downgrades the run to `Flag` — it never disappears silently and never ships as `Citation`.

### ChatMessage (reuse, FR-010)

Existing `chat_message(draft_id, role, content, created_at)`; roles `officer` | `system`. Officer ask, system stage notices, revision turns, and completion notices are appended — append-only, restored on resume alongside `OneAskState`.

## Relationships

```text
one_ask_session 1 ── 1 OneAskState
OneAskState 1 ── * ProvisionCard ── (by value) ──> chunks/sources (echo.db)
OneAskState 1 ── * Paragraph 1 ── * Run ── card_idx ──> ProvisionCard
chat_message * ── draft_id ── 1 one_ask_session
```

## Frontend view model (TypeScript, camelCase via serde rename_all)

`OneAskView` consumes: `{ draftId, status, paragraphs: [{ number, section, runs: [{ kind: 'prose'|'citation'|'figure'|'flag', ... }] }], cards }` — citation runs render with hover→(source_title, label, quoted); flag runs render visually distinct (FR-008); copy action serialises paragraphs to clean numbered text (FR-011) skipping nothing (flags appear as bracketed notices so the officer sees them in the paste too).
