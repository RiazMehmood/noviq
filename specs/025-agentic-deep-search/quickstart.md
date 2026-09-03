# Quickstart: Agentic Deep Search

## What it is

The "Deep" chat toggle, re-implemented as a bounded agentic loop (≤3 LLM calls, ≤2 retrieval rounds) that
answers broad questions across multiple context windows instead of one saturated window. Normal search is
unchanged.

## Enabling / rollback

| State | `deep` toggle | `ECHO_AGENTIC_DEEP` | Behavior |
|-------|---------------|---------------------|----------|
| Normal | off | any | Unchanged normal search. |
| Legacy deep | on | off / unset | Current one-pass deep (for A/B baseline). |
| Agentic deep | on | `1` / `on` | The new bounded loop. |

Flag lives in prod `.env` (`/opt/noviq/echoAI/.env`), loaded by the `noviq-server` service. Default **off** at
first deploy; flip on for A/B, retire the flag + legacy path once the A/B clears SC-001/002/003/006.

## Run a deep query (dev)

1. Set `ECHO_AGENTIC_DEEP=1` in the backend env.
2. In chat, toggle **Deep search** on, ask a multi-part question (e.g. *"grounds, penalties, procedure, and
   appeal under the E&D Rules"*).
3. Open the admin **query trace** → confirm the `DeepTrace`: sub-questions, per-part coverage, round-2 triggers,
   `llm_calls ≤ 3`, `retrieval_rounds ≤ 2`.

## Tests

```bash
# on the VM (no local Rust); see project notes for the known .so stderr false-error on --lib build
cargo test --lib --no-default-features --features llm,server,cuda deep_search
```

Unit coverage: facet-expansion parse + fallback ladder; coverage-report parse + bias-to-refetch; cross-round
dedup + pending-first packing; general grounding verifier (incl. a "Zoonology"-style fabrication case that must
be dropped); loop invariants (≤3 calls / ≤2 rounds, original leg present).

## A/B eval

Under `research/025-agentic-deep-search/`: run the same gold-set questions through legacy deep (flag off) and
agentic deep (flag on); compare (a) sub-part coverage vs a human-rated gold, (b) citation traceability (must be
100%), (c) latency (median/p95), (d) rater completeness preference. Gates: SC-001 ≥90% coverage, SC-002 100%
traceable, SC-003 ≤~65 s median, SC-006 ≥60% preference.

## Key files

- `services/deep_search/{mod,expand,coverage,assemble,verify}.rs` — the loop (new).
- `services/query_router.rs` — extracted `retrieve_for_query` (shared by normal + sub-questions).
- `api/query.rs` — routes `deep=true` + flag → `run_agentic_deep`.
- Reused: `intent_router::decompose`, `one_ask::{quote_verifies, foreign_number_in_prose}`,
  `summary_builder::pack_windows`, the `graph_expand`/`summary_recall` arms, the `DEEP_SEARCH` task-local.
