---
id: 0002
title: agentic-deep-search-plan
stage: plan
date: 2026-07-13
surface: agent
model: claude-opus-4-8
feature: 025-agentic-deep-search
branch: 025-agentic-deep-search
user: Riaz Mehmood
command: /sp.plan
labels: [deep-search, agentic-rag, retrieval, plan]
links:
  spec: specs/025-agentic-deep-search/spec.md
  ticket:
  adr:
  pr:
files:
  - specs/025-agentic-deep-search/plan.md
  - specs/025-agentic-deep-search/research.md
  - specs/025-agentic-deep-search/data-model.md
  - specs/025-agentic-deep-search/contracts/facet-expansion.md
  - specs/025-agentic-deep-search/contracts/coverage-check.md
  - specs/025-agentic-deep-search/contracts/loop-orchestration.md
  - specs/025-agentic-deep-search/quickstart.md
tests:
  - none (planning phase — no code); test plan captured in quickstart.md + contracts acceptance checks
---

## Prompt

/sp.plan for feature 025. Produce the implementation plan mapping the 14 FRs to concrete reuse
(query_router.rs, intent_router::decompose, services/drafting/one_ask/{research,verify}.rs, graph/summary arms,
DEEP_SEARCH task-local, api/query.rs). Resolve: (1) bounded loop orchestrator location + exactly-3-LLM /
2-round control flow; (2) LLM facet-expansion on decompose (structured JSON, keep original as a leg); (3) the
coverage-check contract (the new component: per-sub-question answered/pending, bias-to-refetch, cheap); (4)
reuse of one_ask research per sub-question with graph+summary; (5) cross-round dedup + budget packing via
pack_windows (pending-first); (6) grounding gate via one_ask verify; (7) Deep toggle routes into the loop,
normal untouched; (8) trace fields; (9) cancellation at round boundaries; (10) latency budget keeping #1/#2
short. No new schema (stateless), no window/GPU change. Include a feature-flag/rollback plan for A/B vs current
one-pass deep.

## Response snapshot

Wrote plan.md + research.md + data-model.md + 3 contracts + quickstart.md.

**Key planning correction (from reading the code, not the seed):** `one_ask::research()` and
`assemble_and_verify()` are **drafting-coupled** (OneAskIntent, Section skeleton, DocType, ProvisionCard,
per-section citation caps) — NOT drop-in Q&A functions. So per-sub-question retrieval reuses the **query_router
hybrid path** (extract a shared `retrieve_for_query`), and one_ask contributes only low-level primitives
(`rrf_merge`, `relevant_hits`, `quote_verifies`, `foreign_number_in_prose`). This is the one structural change
to shared code; the rest is call-site reuse.

- **Structure**: new feature-local `services/deep_search/{mod,expand,coverage,assemble,verify}.rs`; extract
  `retrieve_for_query` from `route_scoped_with_override`; route `api/query.rs` `deep=true` + `ECHO_AGENTIC_DEEP`
  → `run_agentic_deep`.
- **Loop (bounded)**: expand(#1) → retrieve r1 (per sub-q, concurrent, graph+summary) → draft+coverage(#2,
  short) → retrieve r2 (pending only, skip if none) → synthesize(#3, full) → grounding gate. Invariants:
  ≤3 LLM calls, ≤2 rounds, original leg always present, ≥1 verified citation or grounded-fallback, task-local
  concurrency, cancel at boundaries.
- **Coverage-check** = the only new component: short structured call, ids+status only (no answer prose →
  can't hallucinate), bias-to-refetch on any parse/uncertainty.
- **Flag/rollback**: `ECHO_AGENTIC_DEEP` (default off) → A/B agentic vs legacy one-pass deep on same corpus;
  retire legacy only after A/B clears SC-001/002/003/006.
- **Constitution Check: PASS** with ONE justified deviation — Deep's ~65 s median exceeds the ≤10 s Tier-3/4
  dev-constraint budget; justified because Deep is opt-in "thorough" mode and normal queries are unchanged
  (documented in Complexity Tracking). Strengthens XI (citation) + XIII (graph).

## Outcome

- ✅ Impact: Full plan + Phase 0/1 artifacts ready; `/sp.tasks` can proceed. Agent context updated with 025 tech.
- 🧪 Tests: none yet; test plan = facet-expansion parse/fallback, coverage parse/bias-to-refetch, dedup/packing
  precedence, general grounding verifier (incl. Zoonology-style fabrication drop), loop invariants.
- 📁 Files: plan.md, research.md, data-model.md, contracts/{facet-expansion,coverage-check,loop-orchestration},
  quickstart.md.
- 🔁 Next prompts: `/sp.adr agentic-multipass-deep-search` (owed — significant, cross-cutting, alternatives
  existed); then `/sp.tasks`. Before full wiring: prototype the coverage-check latency/accuracy on 5–10
  multi-part questions.
- 🧠 Reflection: The spec seed's "reuse one_ask::research" was too optimistic; reading the signatures showed the
  clean reuse is the query_router retrieval core + one_ask's small verify primitives. Extracting a shared
  retrieval fn keeps normal and deep retrieval identical and is the smallest safe change.

## Evaluation notes (flywheel)

- Failure modes observed: none (planning). Designed-against: coverage mis-judgment (bias-to-refetch), bad
  decomposition (original-query leg), cross-round overflow (dedup + pending-first packing), fabrication
  (grounding gate → refuse-or-flag).
- Graders run and results (PASS/FAIL): Constitution Check — PASS (1 justified deviation).
- Prompt variant (if applicable): n/a.
- Next experiment (smallest change to try): implement + measure the coverage-check call alone (latency +
  answered/pending accuracy vs a 5–10 question gold) before building the rest of the loop.
