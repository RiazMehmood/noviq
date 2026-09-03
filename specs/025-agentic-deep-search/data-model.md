# Phase 1 Data Model: Agentic Deep Search

**All types are in-memory, per-turn. No database entities, no migration** (the loop is stateless — FR-010,
Out of Scope). Types live in `services/deep_search/`.

## SubQuestion

One facet of the officer's question.

| Field | Type | Notes |
|-------|------|-------|
| `id` | `u8` | Stable index; `0` is reserved for the original-query leg (FR-001). |
| `text` | `String` | The sub-question query text (verbatim original for id 0). |
| `is_original` | `bool` | True only for id 0; guarantees the safety leg is always present. |
| `slots` | `Slots` (existing) | Optional structured slots from `decompose::slots::extract`, when available. |

**Validation**: 2–7 items total (original + 1–6 facets). Near-duplicate `text` deduped. If expansion yields 0
facets → the single `[original]` leg (degrade path).

## CoverageItem / CoverageReport

Output of the coverage-check call (LLM call #2). The **one new component**.

| Field | Type | Notes |
|-------|------|-------|
| `sub_question_id` | `u8` | Must match a `SubQuestion.id`. |
| `status` | `Coverage` enum: `Answered` \| `Pending` | Drives round 2. |

**CoverageReport** = `Vec<CoverageItem>`.

**Parsing rules (bias-to-refetch, FR-007)**:
- Unparseable JSON, missing id, or unrecognized status → that sub-question defaults to **`Pending`**.
- A sub-question absent from the report → **`Pending`**.
- `pending_ids()` = ids with `status == Pending` → the round-2 fetch set. Empty → skip round 2 (FR-004).

## Passage / PassageSet

The retrieved evidence, tagged by round for packing precedence.

**Passage**

| Field | Type | Notes |
|-------|------|-------|
| `chunk_id` | `String` | Dedup key across rounds. |
| `content` | `String` | Passage text (for packing + synthesis + verification). |
| `citation` | `Citation` (existing) | Hierarchical legal path (Constitution XI). |
| `round` | `u8` (1 or 2) | Round 2 = gap-filling → higher packing priority. |
| `for_sub_question` | `u8` | Which sub-question retrieved it (trace + prioritization). |

**PassageSet** — union of all rounds.
- `dedup()`: keep first-seen `chunk_id` (a round-2 re-find does not duplicate; keeps its earliest round tag).
- `packed(budget)`: order **round-2 / previously-pending passages first**, then the rest by rank; pack with
  `summary_builder::pack_windows` to the answer-window budget (FR-013). Feeds synthesis (call #3) and is the
  corpus the grounding gate verifies against (R5).

## DeepTrace

Observability payload surfaced to the query trace/debug view (FR-011, User Story 4).

| Field | Type | Notes |
|-------|------|-------|
| `sub_questions` | `Vec<String>` | The facets (incl. original leg). |
| `coverage` | `Vec<(String, Coverage)>` | Per-sub-question answered/pending after the draft. |
| `round2_triggers` | `Vec<String>` | The pending sub-questions that caused round 2 (empty if skipped). |
| `llm_calls` | `u8` | Must be ≤ 3 (invariant assertion). |
| `retrieval_rounds` | `u8` | Must be ≤ 2 (invariant assertion). |
| `grounding` | `{cites_verified, cites_dropped, figures_flagged}` | From the grounding gate. |

## Loop state machine (bounded — FR-005)

```text
EXPAND (LLM #1)
  → RETRIEVE round 1  (per sub-question, existing hybrid+graph+summary)
  → DRAFT + COVERAGE (LLM #2, short)
  → [pending?] ── yes ─→ RETRIEVE round 2 (pending only)
        │                     │
        └────── no ───────────┤
                              ▼
             SYNTHESIZE (LLM #3, full)  →  GROUNDING GATE  →  answer + DeepTrace
```

Invariants (asserted in tests): LLM calls ≤ 3; retrieval rounds ≤ 2; original-query leg always present;
`pending` set ⊆ sub-question ids; final answer carries ≥1 verified citation or degrades to grounded-fallback.
Cancellation checked at each `→` boundary.
