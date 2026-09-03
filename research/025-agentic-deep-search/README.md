# Feature 025 — Agentic Deep Search: A/B eval harness

Compares **legacy one-pass deep** (`ECHO_AGENTIC_DEEP` off) vs **agentic deep**
(`ECHO_AGENTIC_DEEP=1`) on the same corpus, for the gates in `spec.md`:

| Gate | Metric | Target |
|------|--------|--------|
| SC-001 | sub-part coverage vs human gold | ≥ 90%, and > legacy |
| SC-002 | citation traceability | 100% (zero fabricated) |
| SC-003 | latency median / p95 | ≤ ~65 s / ≤ ~90 s, bounded |
| SC-004 | enumerate-a-table correct items | materially > legacy |
| SC-006 | rater completeness preference | ≥ 60% |

## Files

- `gold.jsonl` — the evaluation questions (multi-part legal + one enumerate-a-table
  case), each with the distinct sub-parts a rater expects. **Placeholder — populate
  with 5–10 real questions before running (T003/T027).**
- `coverage-spike.md` — output of the T008 de-risk spike (coverage-check latency +
  answered/pending accuracy on the gold questions). GO/No-Go for the coverage design.
- `run.sh` — (to be written in T027) drives both modes through the query API and
  records coverage / citations / latency.

## How to run (once populated)

1. On the VM, set `ECHO_AGENTIC_DEEP` per arm and restart `noviq-server`.
2. Feed `gold.jsonl` through the query API with `deep=true`.
3. Score coverage against each question's expected sub-parts; audit every citation
   for traceability; record latency.
4. Compare arms; a pass requires SC-001/002/003/006 to clear.
