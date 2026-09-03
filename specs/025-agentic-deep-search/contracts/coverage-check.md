# Contract: Coverage Check (LLM call #2) — the new component

**Purpose**: after a short draft, decide per sub-question whether it is answered or still pending; the pending
set drives retrieval round 2. **Routing only — emits no answer content** (so it cannot introduce hallucination).
**Cost**: short structured call — small `max_tokens` (~256). Free-text JSON only.

## Input

- `draft: String` — a SHORT draft answer produced from round-1 passages (not the full final answer).
- `sub_questions: [{id, text}]` — from facet expansion.

## Output (model returns)

```json
{ "coverage": [
  { "id": 0, "status": "answered" },
  { "id": 1, "status": "pending" },
  { "id": 2, "status": "answered" }
] }
```

- `status ∈ {"answered","pending"}`. `answered` = the draft substantively addresses that sub-question from the
  evidence; `pending` = not yet covered / thin / uncertain.

## Post-processing (bias-to-refetch — FR-007)

- Unparseable JSON, missing id, or unrecognized status → that sub-question = **`pending`**.
- Any sub-question absent from `coverage` → **`pending`**.
- `pending_ids` = ids with `pending`. Empty → **skip round 2**, go straight to synthesis (FR-004).
- The original-query leg (id 0) participates like any other.

## Acceptance checks

- [ ] Missing/garbled coverage for a sub-question yields `pending`, never silent `answered`.
- [ ] Empty pending set skips round 2 (no wasted retrieval).
- [ ] The call's `max_tokens` is small; output contains no answer prose (ids + status only).
- [ ] `pending_ids ⊆ sub_question ids`.
