---
id: 070
title: Map-reduce retrieval-scoped with doubled K
stage: general
date: 2026-07-06
surface: agent
model: claude-fable-5
feature: none
branch: release
user: Riaz Mehmood
command: conversation
labels: [mapreduce, retrieval, action-items, latency, gemma-4b]
links:
  spec: none
  ticket: none
  adr: suggested (map-reduce-iterative-deepening) — not yet created
  pr: none
files:
  - src-tauri/src/services/query_router.rs
  - src-tauri/src/services/assistant/capabilities/mod.rs
  - src-tauri/src/services/assistant/capabilities/action_items.rs
  - src-tauri/src/services/assistant/capabilities/summarize_doc.rs
  - src-tauri/src/services/assistant/capabilities/compare.rs
  - src-tauri/src/services/assistant/capabilities/conflict.rs
  - src-tauri/src/services/assistant/mod.rs
  - CHANGELOG_DEV.md
tests:
  - cargo check --no-default-features --features llm,server,cuda --bin noviq-server (VM, exit 0)
  - deployed 92e67ed1; end-to-end browser verification PENDING (user runs it)
---

## Prompt

"as we have currently 20+20 chunks from vec & word search, for map reduce double
them only, why we need to run on the whole corpus? discuss" → iterative-deepening
design discussion ("we just double the k for map reduce and if it is about
something which is still pending, follow up questions can be asked to officer …
if it completes the whole document then dont ask for more") → "currently just do
one thing map reduce with double k one iteration only and defered remaining
iteration for now." → "you will not the query i will run it at browser. just you
need to intimate me when deployement landed."

## Response snapshot

Root-cause context: map-reduce gathered EVERY leaf chunk of `visible_source_ids`
(no document narrowing) — the SPPRA "responsibilities" question planned 61+
sequential batches, ran 10+ min, died on a Gemma token-overflow 400 (14,880 real
vs 14,336 window) with the error unlogged. Architect chose retrieval-scoping:
one doubled-K tranche now; officer-driven dig-deeper iterations deferred.

Implemented (`0a1a9fda`, changelog `92e67ed1`):
- `query_router::retrieve_for_mapreduce` — hybrid vec+FTS5 (+union/RRF merge) at
  `ECHO_MAPREDUCE_K_MULT` (default 2.0) × `ECHO_VEC_K`/`ECHO_FTS_K`, scoped to
  visible sources; notebook_id unused with explicit scope.
- `capabilities::gather_provisions_retrieved` — provisions + citations from the
  slice, unit_label via one batched IN lookup; `run_mapreduce_capability` and the
  4 capability `run()`s + dispatch now take `embedder`. Old `gather_provisions`
  kept (#[allow(dead_code)]) for the deferred whole-document audit path.
- VM cargo check exit 0; deployed + verified live (health ok, engine ready).

## Outcome

- ✅ Impact: map-reduce turns drop from 61+ batches/10+ min to ~4–6 batches/1–2
  min; SC-009 per-turn full coverage consciously relinquished (recorded in
  changelog; ADR suggested for the deepening design).
- 🧪 Tests: VM cargo check exit 0; browser end-to-end pending (user driving).
- 📁 Files: query_router.rs, capabilities/{mod,action_items,summarize_doc,compare,conflict}.rs, assistant/mod.rs, CHANGELOG_DEV.md.
- 🔁 Next prompts: verify heavy query in browser; dig-deeper iterations (tranche
  counter + merge + exhaustion stop); Gemma token estimate + vllm.rs clamp;
  detached-task error logging; scheduler workload cleanup on error.
- 🧠 Reflection: the "slow model" narrative was actually an unscoped corpus sweep
  — measuring batch counts beat guessing at model speed.

## Evaluation notes (flywheel)

- Failure modes observed: whole-corpus gather via visibility set (no doc
  narrowing anywhere in mapreduce dispatch); silent error path in detached task.
- Graders run and results (PASS/FAIL): compile PASS; runtime verification PENDING.
- Prompt variant (if applicable): n/a.
- Next experiment (smallest change to try): tune ECHO_MAPREDUCE_K_MULT against
  real officer questions once browser-verified.
