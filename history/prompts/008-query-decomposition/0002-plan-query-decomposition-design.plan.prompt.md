---
id: 0002
title: plan query decomposition design
stage: plan
date: 2026-06-07
surface: agent
model: claude-opus-4-8
feature: 008-query-decomposition
branch: 008-query-decomposition
user: Riaz Mehmood
command: /sp.plan
labels: [plan, query-decomposition, intent-router, capability-gate, rag]
links:
  spec: specs/008-query-decomposition/spec.md
  ticket:
  adr:
  pr:
files:
  - specs/008-query-decomposition/plan.md
  - specs/008-query-decomposition/research.md
  - specs/008-query-decomposition/data-model.md
  - specs/008-query-decomposition/contracts/decompose-api.md
  - specs/008-query-decomposition/quickstart.md
tests:
  - "n/a (plan stage); test strategy = cargo test -p echo-ai --lib intent_router::decompose + 15-prompt eval"
---

## Prompt

Architect: "include multi-article in first cut, run /sp.plan". (Spec approved with both
shapes — (a)(b)(c) enumeration and multi-article — in the first cut.)

## Response snapshot

Ran setup-plan.sh; produced plan.md + Phase 0 research.md + Phase 1 data-model.md,
contracts/decompose-api.md, quickstart.md; updated agent context.

Design: all new logic in one new pure-ish module `intent_router/decompose.rs`
(`decomposition_enabled` gate, `detect_and_split`, `merge_subanswers`), with a single
orchestration hook in `intent_router::dispatch` wrapping the existing
`query_router::route_scoped_with_override` seam. Sub-questions answered SEQUENTIALLY
(Principle III — ≤1 model resident). Capability gate = GGUF param size ≤1.5B (via existing
`discover_local_gguf`/`parse_param_size_b`) with HardwareTier fallback (no GPU & <16GB RAM)
and `ECHO_DECOMPOSE` override. Rule-based detection: ordered `(a)(b)(c)` markers, and
`slots.section_paths.len()>=2` for multi-article. Deterministic template merge with global
[n] citation renumbering; "not found" parts rendered verbatim, uncited. MAX_SUBQUESTIONS=6
cap; over-cap or <2 sub-Qs → single-pass fallback.

Constitution Check PASS (re-checked post-design): single-model-slot preserved by
sequential loop; gate embodies hardware-adaptive Principle VII; citation remap satisfies
XI; additive, no migration, smallest diff. No violations → Complexity Tracking notes only
the justified N-calls-vs-1 tradeoff.

## Outcome

- ✅ Impact: implementation-ready design; one new file + one hook; independently testable
  units (gate / detect+split / merge).
- 🧪 Tests: planned — unit tests per pure function + 15-prompt in-app acceptance.
- 📁 Files: plan.md, research.md, data-model.md, contracts/decompose-api.md, quickstart.md.
- 🔁 Next prompts: /sp.tasks to generate the dependency-ordered TDD task list, then
  /sp.implement.
- 🧠 Reflection: keeping detect/split/merge pure isolates the brittle rule logic for TDD
  and keeps the dispatch diff tiny; sequential execution is a hard constitution constraint,
  not a perf choice.

## Evaluation notes (flywheel)

- Failure modes observed (to guard in tests): false-positive split on legal sub-clause
  numbering; citation [n] collisions across blocks; gate misfiring on hot-swapped big GGUF.
- Graders run and results (PASS/FAIL): Constitution Check PASS (13/13 principles).
- Prompt variant (if applicable): n/a.
- Next experiment (smallest change to try): /sp.tasks → first RED test = detect_and_split
  on the Q15 (a)(b)(c) string returns 3 sub-questions.
</content>
