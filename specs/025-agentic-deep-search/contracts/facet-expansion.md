# Contract: Facet Expansion (LLM call #1)

**Purpose**: turn the officer's question into 4–6 distinct sub-questions plus the mandatory original-query leg.
**Cost**: short structured call — small `max_tokens` (~256). Free-text JSON only (no llguidance).

## Input

- `original_query: String` — the officer's question (post masking / carry-over, as the normal path receives it).
- `slots: Slots` — from `decompose::slots::extract` (may be empty).

## Output (model returns)

```json
{ "sub_questions": ["…facet 1…", "…facet 2…", "…facet 3…", "…facet 4…"] }
```

- 0–6 strings. Each a short, self-contained question covering a **distinct** facet, requirement, exception, or
  related provision — including synonyms/related terms the officer did not phrase.

## Post-processing (deterministic, in code)

1. Prepend the original query as `SubQuestion{ id: 0, is_original: true }` — **always**, regardless of output.
2. Append parsed facets as ids 1..=N, capped at 6; trim; drop empties; dedupe near-identical (case/whitespace).
3. **Fallback ladder** (FR-012):
   - unparseable / empty output → use `decompose::detect_and_split(original, slots)`;
   - that also empty → `[SubQuestion id 0 only]` (single-facet degrade → near single-pass).

## Acceptance checks

- [ ] Original query is present as id 0 in **every** case (incl. total LLM failure).
- [ ] Total sub-questions ∈ [1, 7]; facets ≤ 6.
- [ ] Unparseable output never panics — falls back deterministically.
- [ ] Near-duplicate facets are collapsed.
