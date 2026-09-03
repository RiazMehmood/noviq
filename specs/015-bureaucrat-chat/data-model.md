# Phase 1 Data Model: Unified Bureaucrat Chat Assistant

Storage: bundled PostgreSQL 17 + pgvector (single source of truth). One **additive** migration.
Reuses existing `chat_message` and `conversation_summary` (migration 023) and the graph store
(`amended_by` edges, `footnote_refs`). No table drops (additive policy).

## Entities

### Conversation turn — REUSE `chat_message` (migration 023)
- Fields: `id`, `conversation_id`, `seq`, `role` (user|assistant), `content`, `created_at`.
- Used by: every capability; the allocator packs the most recent turns verbatim (priority 6).
- **DECISION (resolves analyze-I1)**: `chat_message` is today `draft_session`-scoped. F015 adds an **additive `conversation_id` UUID column** to `chat_message` (same additive migration as `pinned_case_fact`); for existing rows `conversation_id` backfills from `draft_session_id`. All F015 commands key on `conversation_id`. No table/column drops (additive policy). `conversation_summary` and `pinned_case_fact` also key on `conversation_id`.

### Conversation summary — REUSE `conversation_summary` (migration 023)
- Fields: conversation id, `summary` (running prose), `updated_at`.
- Eviction target: when older turns won't fit, they are folded here (priority 7). One compact block, always present.

### Pinned case fact — NEW table `pinned_case_fact`
- Purpose: load-bearing situational facts the user stated (FR-018), kept verbatim, never summarized, supersedable.
- Fields (ids are TEXT to match the existing `chat_message.draft_session_id` TEXT id):
  - `id` (text, pk, default `gen_random_uuid()::text`)
  - `conversation_id` (text — logical fk to the conversation)
  - `key` (text — normalized fact type, e.g. `grade`, `service_years`, `leave_type`, `designation`)
  - `value` (text — verbatim as stated)
  - `source_seq` (int — the `chat_message.seq` it was extracted from)
  - `superseded` (bool, default false — a newer restatement sets older rows true)
  - `created_at` (timestamptz)
- Rules: latest non-superseded row per `key` wins; always loaded into the allocator's pinned slot (priority 3); never evicted, never paraphrased.

### Context budget plan — IN-MEMORY (not persisted)
- Computed per turn by the allocator: `n_ctx`, `gen_reserve`, and the per-component real-token counts (persona, pinned facts, question, retrieved law, recent turns, summary).
- Invariant: `Σ(kept components) + gen_reserve ≤ n_ctx` BEFORE prompt build (FR-011). Emitted to the query trace for auditability.

### Retrieved provision — IN-MEMORY (atomic)
- A whole unit of law for the turn, with its citation. Included whole or omitted — never sliced (FR-014). Replaces the per-chunk 25%-of-budget cap.

### Amendment link — DERIVED from graph (not a new table)
- Source: `amended_by` edges + `footnote_refs` + F013 amendment-history footnote chunks.
- Shape (in-memory): `{ amended_provision, amending_instrument, date, citation }`, ordered oldest→recent (FR-005f).

### Capability intent — IN-MEMORY (transient)
- The classified capability for a turn (guidance|draft|summarize_doc|compare|conflict|amendment_chain|action_items) + parsed document scope.

## Migration (additive)

`src-tauri/src/db/pg/migrations/NNN_pinned_case_fact.sql`:
```sql
CREATE TABLE IF NOT EXISTS pinned_case_fact (
    id              UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    conversation_id UUID NOT NULL,
    key             TEXT NOT NULL,
    value           TEXT NOT NULL,
    source_seq      INTEGER,
    superseded      BOOLEAN NOT NULL DEFAULT FALSE,
    created_at      TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE INDEX IF NOT EXISTS idx_pinned_fact_conv
    ON pinned_case_fact (conversation_id) WHERE superseded = FALSE;

-- Additive conversation identity on chat_message (resolves analyze-I1)
ALTER TABLE chat_message ADD COLUMN IF NOT EXISTS conversation_id UUID;
UPDATE chat_message SET conversation_id = draft_session_id WHERE conversation_id IS NULL;
CREATE INDEX IF NOT EXISTS idx_chat_message_conv ON chat_message (conversation_id, seq);
```
- Additive only (Constitution: no drops). All F015 tables key on `conversation_id`; existing `chat_message` rows backfill from `draft_session_id`.

## State transitions

- **Pinned fact**: `active` → (user restates same `key`) → prior rows `superseded=true`, new row `active`.
- **Conversation history**: `recent verbatim` → (window pressure) → oldest turn folded into `conversation_summary`, removed from verbatim set. Pinned facts are exempt from this transition.
- **Context-fill notice**: `below threshold` → (utilization ≥ threshold) → notice shown; user may start a new conversation that copies non-superseded `pinned_case_fact` rows + a recap (FR-018a/b).
