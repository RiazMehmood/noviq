---
id: 041
title: Tier 1 retrieval knobs — env-tunable precision sweep controls
stage: refactor
date: 2026-05-16
surface: agent
model: claude-opus-4-7
feature: 001-notebook-knowledge-engine
branch: feature/tier3-hierarchical-retrieval
user: engr.riaz2010@gmail.com
command: implement Tier 1 high-impact knobs
labels: [retrieval, knobs, tuning, tier3]
links:
  spec: specs/001-notebook-knowledge-engine/spec.md
  ticket: n/a
  adr: n/a
  pr: n/a
files:
  - path: src-tauri/src/services/query_router.rs
    change: added knob_* helpers (vec_k, fts_k, rerank_pool_k, context_k, per_chunk_cap_frac, min_rerank_score); wired into fetch_limit, fts5_retrieve, reranker, build_context, plus a confidence-gate block before LLM synthesis
  - path: CHANGELOG_DEV.md
    change: appended 2026-05-16 (late PM) entry documenting all six knobs, defaults, and suggested first sweep
tests:
  - status: cargo check clean (11s)
  - note: empirical 15-prompt sweep deferred to next session
---

## Prompt

> updated test5_logs.md, now i have another idea, we need to move all knobs to
> check precision of the results, i,e, context tokens, top-k, etc what do you
> suggest? check logs then discuss with me
>
> echoai_tier3_test_report.xlsx create one file for test according to our
> prompts so need to summerize that also, like in first test what we have done
> what were results, then in second test what we changed and now what are the
> results, in test 3 same for all others so we can note down all details also,
> and go for tier 1 implementations high impact options

## Response snapshot

Read test5_logs.md (15 v2 prompts from 2026-05-16 ~09:35). Identified that
plumbing is healthy (FTS5 24/24 every query, hybrid(rrf), pool 35–44) but the
correct chunk often ranks past position 12. Strongest signal: reranker top
score predicts correctness (≥+5 ⇒ correct, ≤-1 ⇒ wrong) with 100% accuracy
on the 13 non-cache traces.

Built /home/riaz/Desktop/echoai_tier3_test_report.xlsx — 7 sheets:
00_Overview, 01_Test_Chronology, 02_Per_Trace_Detail, 03_Pass_Fail_Grid_v2,
04_Findings_Decisions, 05_Tier1_Knobs_Plan, 06_Raw_Aggregates.

Implemented Tier 1 knobs in query_router.rs (all env-driven, defaults preserve
current behaviour):
- ECHO_VEC_K (24) — vector top-N
- ECHO_FTS_K (24) — FTS5 BM25 top-N
- ECHO_RERANK_POOL_K (12) — cross-encoder scoring count
- ECHO_CONTEXT_K (12) — final context chunks
- ECHO_PER_CHUNK_CAP_FRAC (0.25) — per-chunk size cap
- ECHO_MIN_RERANK_SCORE (unset) — fabrication gate (highest leverage)

cargo check clean (11s).

## Outcome

- ✅ Impact: precision-sweep infrastructure in place; no default-behaviour change; one new confidence-gate path returns grounding fallback with `QualityNotice{constrained: true, message: "Low retrieval confidence..."}` instead of fabricating
- 🧪 Tests: cargo check -p echo-ai clean (11.37s); empirical 15-prompt sweep deferred to next session
- 📁 Files: src-tauri/src/services/query_router.rs, CHANGELOG_DEV.md
- 🔁 Next prompts: run `ECHO_RERANK_POOL_K=24 ECHO_MIN_RERANK_SCORE=-2.0 ./run-dev.sh`, re-run 15 v2 prompts, compare against test5 baseline
- 🧠 Reflection: knobs over UI first — fast iteration, no design churn; promote winners to defaults later

## Evaluation notes (flywheel)

- Failure modes observed: F1 (24h arrest), F3 (treason inversion), F5 (Quaid) all fail across v1/v2/v3 — F3 is generation-side (negation drop), F1/F5 are reranker rank-cutoff
- Graders run and results (PASS/FAIL): manual verdict matrix in echoai_tier3_test_report.xlsx sheet 03 — 5/13 PASS, 6/13 FAIL, 2/13 cache hit
- Prompt variant (if applicable): v2 prompts (5 facts × v1 Tech / v2 Prof / v3 Plain)
- Next experiment (smallest change to try): bump ECHO_RERANK_POOL_K to 24 and set ECHO_MIN_RERANK_SCORE=-2.0 — one sweep, no code change
