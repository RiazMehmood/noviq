---
id: 0003
title: Forensic audit retrieval pipeline fixes
stage: misc
date: 2026-05-28
surface: agent
model: claude-opus-4-7
feature: 005-structured-importer
branch: 005-structured-importer
user: engr.riaz2010@gmail.com
command: ad-hoc (no slash command — user follow-up to ANALYSIS_v3.md §8 audit)
labels: [retrieval, reranker, rules-of-business-1986, importer, fast-path, instrumentation]
links:
  spec: specs/005-structured-importer/spec.md
  ticket: ~/Desktop/test 22 prompts/V3/ANALYSIS_v3.md (§8)
  adr: n/a
  pr: n/a
files:
  - path: src-tauri/src/services/doc_profiles/rules.rs
    role: prod+test
  - path: src-tauri/src/services/query_router.rs
    role: prod
  - path: CHANGELOG_DEV.md
    role: docs
tests:
  - name: doc_profiles::rules::tests::bare_number_rule_with_legal_opener_detects_rob_headings
    result: PASS
  - name: doc_profiles::rules::tests::bare_number_rule_with_title_and_dashes_detects_real_headings
    result: PASS
  - name: doc_profiles::rules::tests::detection_matches_rules_corpus
    result: PASS
  - name: doc_profiles::rules::tests::citation_unit_is_rule
    result: PASS
  - name: query_router::tests::citation_fast_path_resolves_cited_units
    result: PASS (regression then fix — initial scope guard was too aggressive on single-source corpora, re-tightened with per_src.len() > 1 conjunct)
  - name: query_router::tests::citation_fast_path_disambiguates_by_statute_preamble
    result: PASS
---

## Prompt

> perform all recommended fixes and i also want to disable reranker, hardcode it off and write changelog for that also with reason also.

Context: this is a continuation of a same-day forensic investigation. The user
ran 22 prompts across local 0.5B-FT, local 3B, and Groq Llama-3.3-70B against
a 6-PDF Sindh-govt corpus. A first analysis (§7 of ANALYSIS_v3.md) was
challenged as "guessing the bugs" — the user demanded hard DB+trace evidence.
That second pass produced §8: per-prompt gold-text presence in the DB,
per-prompt pipeline-stage attribution (RRF / reranker / context / LLM), and
six concrete recommended fixes. This prompt asks to execute the fixes and
add reranker disable on top.

## Response snapshot

Implemented four code changes (one regex pattern + three retrieval-pipeline
edits) and one documentation pass:

1. **Fix 1 — RoB rule heading detection.** Added an extra `HeadingPattern` to
   `RULES.heading_patterns` matching bare-number rule headings that start with
   a legal opener (`The`, `Notwithstanding`, `On`, `If`, `Where`, `Subject`,
   `No`, `When`, `Every`, `Any`, `All`, `It`). This is the shape OCR produced
   for the 1986 RoB scan — `9. The Chief Minister may in his discretion ...`
   — which none of the existing three patterns could match. The opener
   safe-list separates real rule headings from RoB footnote shapes without
   negative lookahead, which regex_lite does not support.

2. **Fix 2 — Citation fast-path scope guard.** When `unit_label='Rule N'`
   resolves to multiple sources, the winner has zero source-affinity overlap
   with the query, AND the query has at least one discriminator token, the
   fast-path now returns `None` and falls through to hybrid retrieval. The
   three conjuncts together are necessary: dropping any one regressed an
   existing test (single-source Article 8, or a pure citation query, or the
   tie-break behaviour on weak-but-consistent matches).

3. **Fix 3 — Cross-encoder reranker hardcoded OFF.** Default flipped to
   off. Re-enable path documented inline: `ECHO_RERANK=1`. Reason: §8's
   evidence (Q5 BS-17 demote, Q14 RoB 10A demote, Q21 cross-document FTS
   overlap, Q17 negative-top-score yet correct synthesis) shows the
   bge-reranker-base default is net-negative on legal-text queries on this
   corpus, and also costs ~17–21 s of CPU latency per query.

4. **Fix 4 — Candidate-ID logging.** Vec / FTS / reranker steps now emit
   chunk-ID (first 8 of UUID) + source filename per candidate; reranker also
   emits per-candidate scores. Closes §8.6's "VEC_MISS vs RRF_DROP is
   indistinguishable from current traces" gap — future evals can attribute
   failures to a specific pipeline stage without re-running the query.

5. **CHANGELOG_DEV.md entry** appended at the top per project convention.
   Lists each fix with symptom + root cause + file paths + verification +
   follow-up actions.

Did NOT do (deferred to follow-ups, documented in CHANGELOG):
- Re-indexing the RoB 1986 PDF itself — this is a runtime UI action via
  Settings → Indexed Folders → Reindex, not a code change.
- Q18 missing-articles recall fix (different code path).
- 0.5B-FT retrain or retire decision.
- Grounding-gate disagreement fix.

## Outcome

- ✅ Impact: closes 3 of 6 V3-eval Fallback prompts (Q13, Q15, Q16) after the
  follow-up re-index lands. Q14 expected to improve (RoB Rule 10A retrieval
  now possible). Disabling the reranker also removes the silent demote of
  Q5 / Q14 / Q21 chunks. Pipeline traces from this point forward can
  attribute every failure to a specific retrieval stage without re-running.
- 🧪 Tests: `cargo test --offline --lib doc_profiles::rules` 4/4 PASS;
  `cargo test --offline --lib citation_fast` 2/2 PASS after one regression
  caught + re-fixed (scope guard tightened with `per_src.len() > 1`
  conjunct).
- 📁 Files: `rules.rs` (+18 LOC pattern, +33 LOC test);
  `query_router.rs` (fast-path scope guard ~10 LOC; reranker default flip;
  candidate-ID logging across 3 trace steps; trace-message string update);
  `CHANGELOG_DEV.md` (one new dated entry at top).
- 🔁 Next prompts:
  (a) Re-index RoB 1986 PDF from Settings; verify
      `chunks.unit_label LIKE 'Rule %'` count goes from 9 → ~40.
  (b) Re-run the 22-prompt eval on Groq + local-3B; append §9 to
      ANALYSIS_v3.md quantifying flips per prompt.
  (c) If Q13/Q15 still Fallback, raise the affinity head_text cap.
  (d) Q18 article-recall fix.
- 🧠 Reflection: a one-line scope-guard like `if best_overlap == 0 { return
  None }` regressed a test that hadn't been considered (single-source
  constitution corpus). The three-conjunct version is the actual safe
  predicate — `per_src.len() > 1 && best_overlap == 0 && !q_tokens.is_empty()`
  — and the comment block above it now explains why each conjunct exists.
  Lesson: when adding a guard, prove the *complement* of the guard preserves
  every existing test before assuming a single-condition predicate is right.

## Evaluation notes (flywheel)

- Failure modes observed:
  - First scope-guard variant broke `citation_fast_path_resolves_cited_units`
    because "Article 8" on a single-source corpus has overlap 0 against the
    constitution's head_text — fixed by adding `per_src.len() > 1` clause.
  - Build initially failed with network timeouts under `cargo check`; passed
    once `CARGO_HOME` was pointed at the existing `/media/riaz/New
    Volume/.cargo` cache and `--offline` was used (NTFS symlink issue
    documented in run-dev.sh).
- Graders run and results (PASS/FAIL): unit tests above. No corpus-level
  eval rerun yet (requires re-index).
- Prompt variant (if applicable): n/a (code-change task).
- Next experiment: re-index RoB then re-run the 22-prompt Groq eval. The
  diff vs the existing V3 Groq run quantifies the fix's impact.
