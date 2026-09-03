# Contract: Loop Orchestration (bounded invariants)

**Entry**: `deep_search::run_agentic_deep(...)`, invoked from `api/query.rs` when `deep == true` **and**
`ECHO_AGENTIC_DEEP` is on. Otherwise the existing paths run (one-pass deep when flag off; normal when
`deep == false`).

## Bounded control flow (FR-005)

```text
1. EXPAND            — LLM call #1 (short)         [cancellation check]
2. RETRIEVE round 1  — per sub-question, existing hybrid+graph+summary, concurrent   [cancel check]
3. DRAFT + COVERAGE  — LLM call #2 (short)         [cancellation check]
4. RETRIEVE round 2  — pending sub-questions only  (SKIPPED if none pending)          [cancel check]
5. SYNTHESIZE        — LLM call #3 (full)  →  GROUNDING GATE  →  answer + DeepTrace
```

## Invariants (asserted in tests / debug assertions)

- **INV-1**: `llm_calls ≤ 3` for any turn (expand, coverage, synthesize). Draft is produced within call #2, not
  a separate call.
- **INV-2**: `retrieval_rounds ≤ 2`. Round 2 runs only when `pending_ids` is non-empty.
- **INV-3**: original-query leg (SubQuestion id 0) is always retrieved in round 1.
- **INV-4**: final answer carries ≥ 1 verified citation, or the turn returns the grounded-fallback message
  (never ungrounded prose) — Constitution XI, FR-006.
- **INV-5**: no shared mutable state — all working data is task-local/stack; safe under concurrent officers
  (FR-010).
- **INV-6**: cancellation set at any boundary → the turn stops before the next step and returns promptly
  (FR-014).
- **INV-7**: Deep OFF, or flag OFF, path is byte-for-byte the pre-feature behavior (FR-008) — no shared-code
  regression (guarded by the extracted `retrieve_for_query` being behavior-preserving for the normal path).

## Degrade rules (FR-012)

- Single facet (expansion → 1 leg) → round 1 + synthesize; coverage may still run but round 2 is typically
  skipped. Near single-pass cost.
- Greeting / intro / `@document not found` fast-paths fire **before** the loop (unchanged).

## Trace (FR-011)

Populate `DeepTrace { sub_questions, coverage, round2_triggers, llm_calls, retrieval_rounds, grounding }` and
attach to the existing query-trace steps so the admin debug panel renders it.

## Acceptance checks

- [ ] A 4-facet question runs exactly: 3 LLM calls, ≤2 retrieval rounds.
- [ ] All-covered-after-round-1 question runs: 3 LLM calls (expand, coverage, synth), **1** retrieval round.
- [ ] Flag OFF → identical output+latency to current one-pass deep on the same query.
- [ ] Cancellation mid-loop returns without starting the next step.
- [ ] Trace shows sub-questions, per-part coverage, and round-2 triggers.
