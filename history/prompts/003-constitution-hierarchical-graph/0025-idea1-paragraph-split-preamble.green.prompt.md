---
id: 0025
title: Idea 1 — paragraph-level splitting of long structural chunks
stage: green
date: 2026-05-23
surface: agent
model: claude-opus-4-7
feature: 003-constitution-hierarchical-graph
branch: 003-constitution-hierarchical-graph
user: engr.riaz2010@gmail.com
command: chat (user-proposed root-cause fix after deep diagnosis)
labels: [retrieval, preamble, chunking, root-cause, F5-imperative, doc-agnostic]
links:
  spec: specs/003-constitution-hierarchical-graph/spec.md
  ticket: ""
  adr: ""
  pr: ""
files:
  - path: src-tauri/src/services/structure_parser.rs
    change: New long_chunk_threshold() knob (ECHO_LONG_CHUNK_THRESHOLD, default 1000 chars, floor 100). New split_long_text() helper — paragraph→sentence→char fallback chain. build_preamble() rewritten to emit article-level parent + clause-level paragraph children when text exceeds threshold; parent stays as citation marker (unit_label='Preamble', text='Preamble'), children carry actual content with same label and prefixed unit_path. 6 new unit/integration tests; updated persist test to assert parent+children topology.
  - path: src-tauri/src/services/query_router.rs
    change: citation_fast_path_resolves_cited_units test updated — "Preamble = 1 chunk" → "Preamble = parent + paragraph children (≥2)". No production code changed; fast-path's existing unit_path LIKE 'p/%' returns the new sub-tree naturally.
  - path: CHANGELOG_DEV.md
    change: appended Idea 1 entry above S3b entry with full rationale + design + math + tests + ops + follow-ups.
tests:
  - name: structure_parser::persist_tests::split_returns_input_unchanged_when_under_threshold
    type: unit
    result: NEW
  - name: structure_parser::persist_tests::split_uses_paragraph_boundaries_when_available
    type: unit
    result: NEW
  - name: structure_parser::persist_tests::split_falls_back_to_sentences_when_no_paragraphs
    type: unit
    result: NEW
  - name: structure_parser::persist_tests::split_falls_back_to_char_chunks_for_single_huge_sentence
    type: unit
    result: NEW
  - name: structure_parser::persist_tests::split_drops_empty_pieces
    type: unit
    result: NEW
  - name: structure_parser::persist_tests::build_preamble_splits_real_constitution_preamble_correctly
    type: integration
    result: NEW — loads real IR, asserts ≥5 children + Quaid-i-Azam in exactly one
  - name: structure_parser::persist_tests::persist_tree_writes_every_node_with_resolving_parents
    type: integration
    result: UPDATED — adds parent/children topology assertions
  - name: query_router::tests::citation_fast_path_resolves_cited_units
    type: integration
    result: UPDATED — Preamble assertion now ≥2 chunks
---

## Prompt

> I have two solution in mind also, check whether they are feasable or not? first one is that can we break preamble large chunk into small chunks and connect them as part 1 part 2 etc, and do that for each chunk which touches a certain limit so we break that i,e if we set limit to 100 chars per chunks and found a chunk having 300 chars so we break that into 3 small chunks and connect them with part type or what else possible, and for NER it is only extracting, wo we over lap this cycle make it into percentage with 10% and run it through the complete document and pause it while using chat option which i already created with type option i think which gives priority to chat and block kg screen while extraction in process.
>
> (follow-up) ship Idea 1 first, alone and also set aside existing db so we get fresh db now.

## Response snapshot

**Pre-implementation evaluation:** Both user-proposed ideas confirmed feasible.
Idea 1 (sub-chunking long chunks) is the same as my Option ε from the deep
diagnosis — architecturally better than my recommended Option α (canonical
fallback). Idea 2 (NER percentage + chat-priority pause) is feasible too;
the chat-priority pause is **already implemented** via
`services/scheduler.rs::WorkloadKind::Pass2Ner` (verified). Three
refinements proposed for Idea 1's raw shape:
  1. Threshold should be ~1000 chars, not 100 — mean chunk size is 253;
     a 100-char limit would fragment 91% of the corpus.
  2. Split on semantic boundaries (paragraph → sentence → char), not
     character count alone — character-cut butchers sentences and hurts
     both retrieval and generation.
  3. Citation fast-path interaction handled by keeping the parent's
     `unit_label='Preamble'` (so lookups still find the parent) and
     using `unit_path='Preamble/pN'` for children (so the fast-path's
     existing `LIKE 'Preamble/%'` returns the sub-tree without code
     change).

User confirmed: ship Idea 1 first alone. Asided the DB
(`echo.db.bak-pre-idea1-20260523-…`).

**Implementation:**
1. DB triplet moved aside (verified no live `echo.db*` remains).
2. Added `long_chunk_threshold()` knob (default 1000, floor 100).
3. Added `split_long_text()` helper — three-tier boundary fallback
   (paragraph / sentence / char), drops empty pieces, returns input
   unchanged when ≤ threshold. Generic for any future doc.
4. Rewrote `build_preamble()` — short preamble keeps single-leaf
   shape; long preamble emits article-level parent (text="Preamble"
   marker) + clause-level paragraph children (unit_label="Preamble"
   shared, unit_path="Preamble/pN", parent_chunk_id linked via
   collect_chunks recursion).
5. Updated `persist_tree_writes_every_node_with_resolving_parents`
   for the parent + children topology.
6. Added 5 unit tests for `split_long_text` (all boundary paths).
7. Added 1 integration test `build_preamble_splits_real_constitution_preamble_correctly`
   loading the real IR.
8. Updated `query_router::tests::citation_fast_path_resolves_cited_units`
   for the new "Preamble = parent + children" semantics.
9. CHANGELOG_DEV.md entry written with full rationale.
10. Test build queued in background; result reported when monitor fires.

## Outcome

- ✅ Impact: Direct root-cause fix for F5-imperative. The Preamble's content is now in ~14 paragraph-level chunks; each ~200 char chunk has a focused embedding and no BM25 length penalty. The "Founder of Pakistan, Quaid-i-Azam Mohammad Ali Jinnah" paragraph should win vector + FTS5 retrieval on its own merits, no boost needed. Salience boost remains as a safety net (children inherit unit_label='Preamble'). Generic — any future doc with a long structural opening (Abstract, Foreword, Statement of Objects) gets the same treatment without per-doc-type code.
- 🧪 Tests: 6 new + 2 updated. Pre-existing test count was 50; expected new count ≈ 56 (test job in flight, will confirm).
- 📁 Files: structure_parser.rs (~170 lines added/changed), query_router.rs (~10 lines updated), CHANGELOG (~85 lines), PHR (this file).
- 🔁 Next prompts: (a) user rebuilds via `./run-dev.sh`, re-imports constitution (expected ~1480 chunks total), runs the smoke test (peek DB topology + ask "what does the preamble say" + ask "Who is called the founder of Pakistan?"). If both work end-to-end, run F5 × 3 registers × 3 local models, save as `preamble_v2_idea1.odt`. (b) If Idea 1 alone fixes F5-imperative on retrieval: ship Idea 2 next (NER percentage + always-sample salient chunks). (c) If F5-imperative still fails despite Preamble entering the rerank pool: fall back to Option α canonical fallback as the safety net.
- 🧠 Reflection: The deep diagnosis was the right move. Without measuring "Preamble is rank >48 on both signals", I would have shipped Option α (canonical fallback), tested empirically, and found it works only when the cross-encoder cooperates. Idea 1 removes the dependency on the cross-encoder by giving vec+FTS5 a focused unit to retrieve naturally. One simple fix at the right layer beats four sequential boosts at the wrong layer.

## Evaluation notes (flywheel)

- Failure modes observed pre-Idea-1: Preamble (2775 char, 11× mean chunk length) lost vector+FTS5 to short Articles 276/1/175* on every imperative phrasing. All four prior boosts (S1/S2/S3a/S3b) reported `no-op` because Preamble never entered the post-RRF pool of 83 unique chunks.
- Graders run and results (PASS/FAIL): unit/integration tests pending in flight; in-app re-test pending user rebuild + re-import. Decisive test: trace STEP 8 on imperative F5 should show top match is a Preamble paragraph chunk, not Article 276.
- Prompt variant (if applicable): n/a
- Next experiment: after re-index + F5 re-run, compare `preamble_v1.*` (pre-Idea-1) vs `preamble_v2_idea1.*` (post-Idea-1). Expected: F5-imperative passes on all 3 models at retrieval; 3B passes on generation; 0.5B/1.5B may still fail on phrasing.
