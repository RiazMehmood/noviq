# Phase 1 Data Model: LGD Summary & Note Drafting

Persisted entities live in `echo.db` via one additive migration. Per-draft entities are in-memory working state (not persisted in the first cut). Gold-example file content stays on disk; only metadata is in the DB.

## Persisted entities (SQLite, additive migration)

### `template_schema`
A reusable structure derived from an uploaded format.

| Field | Type | Notes |
|-------|------|-------|
| id | INTEGER PK | |
| name | TEXT NOT NULL | officer-facing template name |
| instrument_type | TEXT NOT NULL | `summary` \| `note` |
| source_format_ref | TEXT | reference to the uploaded format (on-disk path/sha), for re-confirmation |
| created_at | TEXT NOT NULL | ISO-8601 |
| updated_at | TEXT NOT NULL | overwritten on re-confirm (no version history v1) |

**Validation**: `instrument_type` ∈ {summary, note}; `name` unique per instrument_type.

### `template_slot`
An ordered region of a template.

| Field | Type | Notes |
|-------|------|-------|
| id | INTEGER PK | |
| template_id | INTEGER FK → template_schema.id | cascade with template |
| ordering | INTEGER NOT NULL | render order within the template |
| kind | TEXT NOT NULL | `boilerplate` \| `cited` \| `prose` |
| label | TEXT | section label (e.g. "POLICY / RULES") |
| boilerplate_text | TEXT | verbatim text when kind = boilerplate |
| required_source_category | TEXT | when kind = cited; the source KIND/category (e.g. "financial-powers rules", "scheme cost / PC-I") — never a filename. **Derived from the matched provision (R5a), not heading tokens.** |
| prose_guidance | TEXT | when kind = prose; bounded instruction for connective narrative |
| provision_ref | TEXT | **(migration 024, additive)** when kind = cited and detected at ingestion: the verbatim provision reference from the source (e.g. "Rule-6(c) of SPSC Regulations-2023"), gate-verified (R5a). Anchors cited-slot retrieval queries and audit. Nullable. |

**Validation**:
- kind = boilerplate ⇒ boilerplate_text present.
- kind = cited ⇒ required_source_category present; boilerplate_text/prose_guidance null.
- kind = prose ⇒ prose_guidance present; required_source_category null.
- (template_id, ordering) unique.

**State**: a template is *Proposed* (in-memory, from ingestion) → *Confirmed* (persisted on officer confirm). Re-confirm overwrites the persisted slots in place.

**ProposedSlot (in-memory ingestion shape, R5a)**: the model-proposed slot before officer confirmation additionally carries `low_confidence: bool` + `rejection_reason: Option<String>` — set when the deterministic verbatim-verify gate rejects the slot (text/provision not verbatim in source). Low-confidence slots are surfaced for manual classification, never silently dropped (FR-010a). These fields are not persisted; only confirmed slots reach `template_slot`.

### `gold_example`
An uploaded filled instrument, retained for structure/style + evaluation, **excluded from retrieval**.

| Field | Type | Notes |
|-------|------|-------|
| id | INTEGER PK | |
| template_id | INTEGER FK → template_schema.id NULL | optional association |
| instrument_type | TEXT NOT NULL | summary \| note |
| content_ref | TEXT NOT NULL | on-disk path/sha of the example content |
| created_at | TEXT NOT NULL | |

**Invariant (enforced in code + asserted in tests)**: no `gold_example` content is ever inserted into the retrieval corpus tables (chunks / FTS / vec) — SC-007.

### `chat_message` (migration 023, additive — conversational session)
One durably-stored turn in a drafting session's conversation. Append-only.

| Field | Type | Notes |
|-------|------|-------|
| id | TEXT PK | uuid |
| draft_session_id | TEXT NOT NULL | FK → `draft_session.id` (migration 022) |
| seq | INTEGER NOT NULL | monotonic order within the session |
| role | TEXT NOT NULL | `officer` \| `system` |
| content | TEXT NOT NULL | raw turn text |
| token_count | INTEGER | estimated tokens, for compaction accounting |
| summarized | INTEGER NOT NULL DEFAULT 0 | 1 once folded into the rolling summary (raw row retained) |
| created_at | TEXT NOT NULL | ISO-8601 |

**Validation**: `role` ∈ {officer, system}; `(draft_session_id, seq)` unique. **Invariant**: rows are never deleted by compaction — `summarized` only flips 0→1 (FR-031).

### `conversation_summary` (migration 023, additive)
The rolling compacted memory for a draft session. One row per session (upserted).

| Field | Type | Notes |
|-------|------|-------|
| draft_session_id | TEXT PK | FK → `draft_session.id` |
| summary_text | TEXT NOT NULL | running summary of all `summarized=1` turns |
| covers_upto_seq | INTEGER NOT NULL | highest `chat_message.seq` folded in |
| updated_at | TEXT NOT NULL | advanced on each compaction |

**Model-context rule (FR-033)**: the prompt supplied to the single model slot = `summary_text` + `chat_message` rows with `seq > covers_upto_seq` — never the full transcript.

> **Note on `draft_session`**: US5 already persists per-draft working state via migration 022 (`draft_session`) and `session.rs` `DraftState` (selected template, intent, confirmed docs, gaps, rendered blocks, finalize flag). The "in-memory working entities" below are the *shape* of that state; conversation tables above link to it by `draft_session_id`.

## In-memory working entities (per draft — shape of the persisted `DraftState`, migration 022)

### `DraftRequest`
- `template_id` — selected template
- `intent: IntentAnswers` — { instrument_type, addressee/routing, purpose, scheme/subject }
- `candidate_docs: Vec<CorpusDocRef>` — proposed pool from the whole corpus
- `confirmed_docs: Vec<CorpusDocRef>` — officer-curated set; each tagged `role: rules | case`
- `gaps: Vec<Gap>` — detected missing required-source categories
- `gap_resolutions: Vec<GapResolution>` — supplied | proceed-without per gap

### `CorpusDocRef` (view over existing indexed documents — not a new table)
- `doc_id`, `title`, `source_path`, `role: rules | case` (heuristic, officer-correctable)

### `RequiredSourceCategory`
- `text` — the category string from a cited slot, used to build the scoped retrieval query and to match present/missing in the confirmed set.

### `Gap` *(reshaped 2026-06-10 — two-tier, FR-019/R11)*
- `slot_id`, `required_source_category`
- `tier`: `Suggestion { suggested_docs: Vec<CorpusDocRef> }` — category covered (semantic, content-level) by indexed document(s) not in the confirmed set; one-turn officer accept adds them
- OR `tier`: `Missing { destination_folder }` — category covered by no indexed document; officer indexes into the watched folder or proceeds-without
- Coverage test = embedded `required_source_category` vs the document's chunk embeddings, threshold-gated (R11) — never title tokens.
- `fact:` slots with no covering case document take the officer-supplied channel (R12), not a Missing gap.

### `GapResolution`
- `gap` + one of: `Supplied { new_doc_id }` (after watch+index) | `AcceptedSuggestion { doc_ids }` (tier-a one-turn accept) | `OfficerSupplied { value }` (fact slots only — typed value, rendered with "officer-supplied" attribution, FR-008a) | `ProceedWithout`

### `DraftOutput`
Ordered list of rendered blocks; one per slot:
- `BoilerplateBlock { label, text }`
- `CitedBlock { label, quote, citation }` — citation = deterministic hierarchical path from chunk metadata (Principle XI)
- `FactBlock { label, value, attribution }` — extractive value + case-doc attribution; OR officer-typed value with explicit `attribution: "officer-supplied"`, visually distinct from cited content, never dressed as a citation (FR-008a)
- `ProseBlock { label, text }` — generated; contains no number/citation
- `FlaggedBlock { label, reason }` — "unverified — source not available" (refuse-or-flag), visually distinct (FR-003, FR-006)

## Relationships

```
template_schema 1 ──< template_slot          (ordered slots)
template_schema 1 ──< gold_example (0..*)      (optional style/eval refs)
DraftRequest  *── template_schema              (selects one)
DraftRequest  *── CorpusDocRef                 (candidate / confirmed, role-tagged)
template_slot(cited) → RequiredSourceCategory → Gap (when absent from confirmed set)
DraftRequest → DraftOutput (one block per slot)
```

## Mapping to functional requirements

- FR-009/009a/010/012 → `template_schema` + `template_slot` (Proposed→Confirmed), ingestion via reused extraction + **model-assisted segmentation with deterministic verbatim-verify gate (R5a; heading_detector heuristic removed)**.
- FR-010a → `ProposedSlot.low_confidence`/`rejection_reason` + `template_slot.provision_ref` (migration 024).
- FR-010b → ingestion refuses with actionable error when the segmentation model is missing/unparseable; no schema produced.
- FR-011 → `template_slot.required_source_category` (category, not filename; derived from matched provision).
- FR-008a → `GapResolution::OfficerSupplied` + officer-supplied `FactBlock` attribution.
- FR-013 + SC-007 → `gold_example` + the no-retrieval invariant.
- FR-014 → `BoilerplateBlock` verbatim.
- FR-004/005/006 → `CitedBlock` (deterministic citation via decompose helpers) + `FlaggedBlock`.
- FR-007 → `ProseBlock` constraint.
- FR-008 → `CorpusDocRef.role` split (rules vs case).
- FR-017/018/022 → `DraftRequest.intent`, `candidate_docs`/`confirmed_docs`.
- FR-019/020/021 → two-tier `Gap` (semantic content coverage, R11) / `GapResolution` + watched-folder destination.
- FR-027/028/029/030 → `DraftAction` (parsed from each turn) dispatched to existing `start_draft`/`update_doc_set`/`resolve_gap`/`revise_draft`/`finalize_draft`; pipeline artifacts surfaced from `DraftState` (FR-029); unparseable turn → clarifying `system` message (FR-030).
- FR-031/032 → `chat_message` (append-only) + resume from `draft_session` + replay of conversation.
- FR-033/034 → `conversation_summary` + the model-context rule; decision-relevant state preserved because it lives in `DraftState`, not only in prose turns.
