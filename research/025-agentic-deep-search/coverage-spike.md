# T008 — Coverage-check spike (GO/No-Go for the coverage design)

**Date**: 2026-07-13 · **Model**: Qwen3-8B-AWQ (prod vLLM, `127.0.0.1:8000`) · **max_tokens** 256, temp 0,
`/no_think`.

**Method**: ran the coverage-check prompt (contract: `coverage-check.md`) against 3 hand-built
(sub-questions, draft) cases with known ground truth — a partial draft (2 of 4 parts), a fully-covered draft,
and an empty/no-answer draft. Measured JSON parseability, per-sub-question answered/pending accuracy, and
latency. Harness: `/tmp/cov_spike.py` on the VM.

## Result: **GO**

| Metric | Result | Verdict |
|--------|--------|---------|
| Latency (per call) | avg **2.9 s**, max **3.7 s** | ✅ well within the "short call #2" budget (≤5 s) |
| JSON parseability | 3/3 parsed cleanly | ✅ no parse failures |
| Classification accuracy | **9/11 = 81%** | ✅ acceptable, and see below |
| Error direction | **2/2 misses were FALSE-PENDING** | ✅✅ the SAFE direction |

## The key finding: errors are all in the safe direction

Both misclassifications were **false-pending** (marked a covered part as `pending`), never **false-answered**:

- `partial-2of4` id1 (grounds): draft covered it → model said `pending`.
- `all-covered` id0 (the original-query leg): draft covered everything → model said `pending`.

A false-pending only triggers a (cheap, bounded) round-2 re-fetch — it **cannot cause an omission**. A
false-*answered* would be the dangerous error (a part silently dropped), and we saw **zero** of those. This is
exactly the bias-to-refetch behavior FR-007 mandates: the model is conservative, erring toward fetching more.

## Design implications (fold into T014)

1. **Keep the design.** The short structured call is fast (~3 s), parseable, and conservative — validated.
2. **The original-query leg (id 0) skews to `pending`.** It's broad, so the model rarely calls it "answered".
   Cheap options: (a) exclude id 0 from the coverage check (it is always retrieved in round 1 anyway), or
   (b) leave it — an extra round-2 pass on the original query is harmless. Lean (a) to save one re-fetch.
3. **Slightly trigger-happy on `pending`** overall → more round-2 fetches than strictly necessary, but round 2
   is bounded to one pass, so the cost is capped and the safety is worth it.
4. Parser must keep the **bias-to-refetch on any parse failure / missing id** (contract) — untested here only
   because all 3 parsed, but it is the backstop.

## Not covered (deferred to loop integration)

- Real drafts from actual round-1 retrieval (these drafts were hand-built). The full loop (US1) will feed real
  round-1 material; re-measure accuracy then against the 8-question `gold.jsonl`.
- End-to-end latency of all 3 calls + 2 retrieval rounds (this spike measured only call #2).
