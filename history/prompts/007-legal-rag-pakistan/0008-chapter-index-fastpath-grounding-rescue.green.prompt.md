---
id: 0008
title: Chapter-index fast-path + grounding overlap-rescue
stage: green
date: 2026-06-06
surface: agent
model: claude-opus-4-8
feature: 007-legal-rag-pakistan
branch: master
user: Riaz Mehmood
command: (conversation)
labels: [retrieval, grounding, constitution, noviq-eval]
links:
  spec: specs/007-legal-rag-pakistan/spec.md
  ticket: null
  adr: null
  pr: null
files:
  - src-tauri/src/services/query_router.rs
  - CHANGELOG_DEV.md
tests:
  - "cargo test --lib query_router: 57 pass / 1 pre-existing fail"
---

## Prompt

ok make those changes and create an md file also for remaining testing questions
only so we just test those to save time.

(Context: after a 3-way apples-to-apples comparison — base qwen3B vs noviq v1 vs
noviq v2 — on the 17-prompt constitution eval, noviq v2 landed ~flat at the
70.6% anchor. The recommended unlocks were: loosen the grounding validator, fix
the chapter-index resolver, and address Q13/Q17. User asked to implement those
and produce a focused re-test prompt file.)

## Response snapshot

Diagnosed root causes before coding:
- Q11/Q12 chapter-enumeration failures = retrieval-ranking, NOT data (IDX-PII-C1
  / IDX-PII-C2 exist and are correct). Fix = deterministic chapter→IDX fast-path.
- Q13 schedules = DATA GAP (no schedule body nodes in IR) → not code-fixable;
  deferred to a schedule-injection patcher.
- Q17/Q19 = deep-tier (cross_compare/procedure) gated → out of fast-tier scope.

Implemented in query_router.rs:
1. `detect_chapter_index_citation()` + `normalise_part_number()` (arabic→roman)
   → maps enumeration queries to `Article IDX-P<part>-C<chapter>`; wired at the
   top of `citation_fast_path()` with exact unit_label lookup.
2. Grounding overlap-rescue: `content_overlap_ratio()` rescues short
   uncited/bad-cited answers whose tokens overlap context ≥ 50% (marked
   uncited_only instead of replaced with fallback).
3. 7 new unit tests (chapter-index ×4, part-number ×1, grounding overlap ×2).

Created `~/Desktop/20_constitute_prompts/Qwen 2.5:3B testing/
RETEST_after_2026-06-06_fixes.md` — only the open prompts (Q11, Q12, Q9 guard,
plus tracked Q2/Q13/Q17/Q19/Q20), with rebuild + re-index prerequisites.

## Outcome

- ✅ Impact: Q11/Q12 get a deterministic IDX fast-path; correct-but-uncited
  list answers no longer nuked. Net target ~14/17 (~82%) after rebuild+re-index.
- 🧪 Tests: 57 query_router lib tests pass; 1 pre-existing unrelated failure
  (context_builder_token_budget_drops_large_chunk).
- 📁 Files: query_router.rs (resolver + wiring + rescue + tests), CHANGELOG_DEV.md.
- 🔁 Next prompts: schedule-injection patcher (Q13); ECHO_RERANK=1 (Q2);
  re-run 17-prompt eval after rebuild + in-app re-index.
- 🧠 Reflection: data-vs-retrieval diagnosis prevented a wasted "fix" — the
  chapter-index data was already correct; only ranking was wrong.

## Evaluation notes (flywheel)

- Failure modes observed: hybrid retrieval ranks wrong IDX chapter to top;
  grounding strips correct list answers to empty stub.
- Graders run and results (PASS/FAIL): unit tests PASS; full eval pending
  rebuild + re-index (user-run).
- Prompt variant (if applicable): n/a (retrieval/grounding code change).
- Next experiment (smallest change to try): in-app re-index, re-run Q11/Q12,
  confirm fast-path resolves IDX-PII-C1 / IDX-PII-C2 in the trace.
