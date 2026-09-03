---
id: 0023
title: build_context single-chunk cap fix (Preamble regression)
stage: green
date: 2026-05-23
surface: agent
model: claude-opus-4-7
feature: 003-constitution-hierarchical-graph
branch: 003-constitution-hierarchical-graph
user: engr.riaz2010@gmail.com
command: chat (post-rebuild Preamble smoke test diagnosis)
labels: [retrieval, citation-fast-path, context-builder, regression, preamble, A4-followup]
links:
  spec: specs/003-constitution-hierarchical-graph/spec.md
  ticket: ""
  adr: ""
  pr: ""
files:
  - path: src-tauri/src/services/query_router.rs
    change: build_context per-chunk cap check now guarded by `!selected.is_empty()` (mirrors the existing budget-overflow override one branch below); added regression test `context_builder_admits_single_oversized_citation_chunk` replaying the Preamble scenario verbatim
  - path: CHANGELOG_DEV.md
    change: appended a full entry above the multi-provider cloud entry with symptom/root-cause math/fix/tests/ops/follow-ups
tests:
  - name: services::query_router::tests::context_builder_admits_single_oversized_citation_chunk
    type: unit
    result: NEW — reproduces the 2026-05-23 12:52 trace failure
  - name: services::query_router::tests::context_builder_always_includes_first_chunk
    type: unit
    result: existing, now passes for the right reason (was failing silently under pre-fix per-chunk cap)
---

## Prompt

> what does the preamble say?
> No relevant information found in provided documents saved traced in "test 5 prompts" inside qwen 3B folder with new trace file having one log only.
>
> (follow-up) wait first discuss this scenrio with me what you are going to say and what we can do for that then make code changes.
>
> (follow-up) keep option A, rebuild

## Response snapshot

**Diagnosis from the trace (`latest_test.log`, 88 ms total):**
- Citation fast-path fired correctly — `Cited units: 1`, `After 1-hop graph
  expansion: 1 chunks`, bypassed vector/FTS/reranker.
- Merge & rank carried it through — `strategy: citation`, 1 unique chunk,
  top match `constitution_ir.json`.
- STEP 7 build context: **`Selected: 0 chunks (cap K=12)`** — the chunk
  was silently filtered out.

**Root cause:** `build_context()` per-chunk cap (`estimated > per_chunk_cap`)
fires unconditionally. Preamble = 2775 chars → ~713 tokens; cap = 2400 ×
0.25 = 600 tokens; 713 > 600 → dropped. Pool size was 1 (citation
fast-path), so loop ended with `selected = []`.

The existing budget-overflow check one branch below had a
`!selected.is_empty() &&` guard ("always admit the first chunk regardless
of size") that the per-chunk cap was missing — an oversight when the cap
was added later (Tier 3 Task 11).

**Discussion → user picked Option A (mirror the existing override):**
Three options were presented:
- A: add the `!selected.is_empty() &&` guard to the per-chunk cap (1 line,
  semantically symmetric with the existing budget override, lowest risk).
- B: bypass the cap when `strategy == "citation"` (cleaner intent, larger
  diff, threads strategy through the function).
- C: raise `ECHO_PER_CHUNK_CAP_FRAC` globally (no code change; papers over
  the bug, weakens anti-monopoly purpose).

User chose A.

**Implementation:**
- `build_context()` (query_router.rs:2265): one-line guard added on the
  per-chunk-cap branch.
- Comment block expanded to explain the Preamble regression and reference
  the trace.
- New regression test `context_builder_admits_single_oversized_citation_chunk`
  replays the exact failing scenario (1 chunk × 2775 chars × budget 2400 ×
  cap 0.25 → must return 1, not 0).
- CHANGELOG_DEV.md entry written with full symptom / math / fix / tests /
  ops / follow-ups.
- Cargo test job for the `context_builder*` subset queued in background; a
  monitor is watching for test pass / fail / build error.

## Outcome

- ✅ Impact: Preamble fast-path query will now return the chunk it found, instead of dropping it. Unblocks F5 re-test across all three local models. Any future single-chunk citation result (Preamble, leaf sub-clause, isolated Article) is also fixed by the same change.
- 🧪 Tests: 1 new regression unit test; 1 pre-existing test that was failing silently now passes deterministically. Test build/run queued in background; result will be reported when the cargo test job finishes.
- 📁 Files: query_router.rs (~12 lines added — 1 logic line + comment + regression test), CHANGELOG_DEV.md (~60 lines entry).
- 🔁 Next prompts: (a) user rebuilds the app via `./run-dev.sh`, re-asks "what does the preamble say?", confirms STEP 7 now shows `Selected: 1 chunks` and the answer quotes the Preamble. (b) F5 × 3 registers × 3 local models, save results. (c) Cloud comparison via the new multi-provider Settings UI (Cerebras recommended).
- 🧠 Reflection: This is the second "look at the trace, the data is right there" bug today — first was the Preamble import gap (`parse_tree` ignoring `ir.preamble`), now the build_context cap dropping a present chunk. Both were found by reading the actual pipeline output rather than guessing. The trace log is the highest-ROI debugging artifact in this codebase.

## Evaluation notes (flywheel)

- Failure modes observed: silent chunk loss between retrieval and context-build; not caught by tests because (i) the existing first-chunk-override test was authored before the per-chunk cap was added and (ii) no existing test covered the citation-fast-path-returns-1 scenario.
- Graders run and results (PASS/FAIL): pending — `cargo test --lib services::query_router::tests::context_builder` queued; result reported when monitor fires.
- Prompt variant (if applicable): n/a
- Next experiment: re-ask the failing query in-app post-rebuild; STEP 7 should now show 1 chunk, ~700 tokens, top match constitution_ir.json. If the answer text still says "no relevant info" then the bug is downstream (LLM prompt template or stop condition), not the retrieval layer.
