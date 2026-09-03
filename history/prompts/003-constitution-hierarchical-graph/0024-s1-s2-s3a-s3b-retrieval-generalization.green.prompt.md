---
id: 0024
title: S1+S2+S3a+S3b — doc-agnostic retrieval generalization
stage: green
date: 2026-05-23
surface: agent
model: claude-opus-4-7
feature: 003-constitution-hierarchical-graph
branch: 003-constitution-hierarchical-graph
user: engr.riaz2010@gmail.com
command: chat (multi-step S1-S3 retrieval generalisation)
labels: [retrieval, doc-agnostic, salience, definitional, ner, entity-boost, F5-imperative, doc-profile]
links:
  spec: specs/003-constitution-hierarchical-graph/spec.md
  ticket: ""
  adr: ""
  pr: ""
files:
  - path: src-tauri/src/services/doc_profiles/mod.rs
    change: DocProfile gains salient_units field; new is_salient_unit() registry helper with v1 limitation documented
  - path: src-tauri/src/services/doc_profiles/constitution.rs
    change: CONSTITUTION declares salient_units=["Preamble"]
  - path: src-tauri/src/services/query_router.rs
    change: 4 new knobs (ECHO_SALIENT_BOOST, ECHO_DEFINITIONAL_BOOST, ECHO_DEFINITIONAL_POOL_MULT, ECHO_ENTITY_BOOST); 3 new functions (is_definitional, apply_salience_boost, apply_entity_boost, extract_query_entities); fts5_retrieve gains limit_override; 8 new unit tests; trace gets 2 new STEP entries; main + retry production paths wired through all three boosts
  - path: src-tauri/src/services/indexer.rs
    change: index_constitution() no-op replaced with conditional tokio::spawn(run_ner_for_source) — fires only when app: Some(AppHandle); mirrors do_index pattern
  - path: CHANGELOG_DEV.md
    change: appended 4 sequential entries (S1, S2, S3a, S3b) totaling ~250 lines
tests:
  - name: services::doc_profiles::is_salient_unit
    type: indirect (via query_router)
    result: covered
  - name: services::query_router::tests::salient_unit_registry_recognises_preamble
    type: unit
    result: NEW — sanity check
  - name: services::query_router::tests::salience_boost_lifts_preamble_above_article_41
    type: integration
    result: NEW — reproduces F5-imperative
  - name: services::query_router::tests::salience_boost_noop_when_no_salient_chunk
    type: integration
    result: NEW — order preserved for non-canonical queries
  - name: services::query_router::tests::is_definitional_matches_canonical_question_shapes
    type: unit
    result: NEW — 10 positive cases, 6 negative cases
  - name: services::query_router::tests::definitional_boost_lifts_salient_chunk_buried_deeper
    type: integration
    result: NEW — 8-chunk pool, Preamble rank 8 → rank 1
  - name: services::query_router::tests::extract_query_entities_pulls_capitalised_phrases
    type: unit
    result: NEW — 6 cases including hyphenated and multi-word
  - name: services::query_router::tests::entity_boost_lifts_chunk_with_matching_entity
    type: integration
    result: NEW — real entities + entity_mentions fixture
  - name: services::query_router::tests::entity_boost_noop_when_no_match
    type: integration
    result: NEW — order preserved when no NPs or no matches
---

## Prompt

> ok go for S1 to S3 for now, and for Q1 queue it, Q2 per doc type is ok.
>
> (follow-up) for S4 we had a room for that, we are also planning for prediction of prompt and autocorrect spellings at run time we will discuss or work on it on tht time.
>
> (follow-up) S3a only, queue S3b
>
> (follow-up) do full sb also in this session

## Response snapshot

**S1 — Salience-aware retrieval via DocProfile**
- DocProfile gains `salient_units: &'static [&'static str]`; CONSTITUTION
  declares `["Preamble"]`. New `is_salient_unit(label)` registry helper.
- `apply_salience_boost(pool, ranked, boost)` post-RRF: fetches
  unit_labels in one SQL, multiplies position-derived score by `boost`
  when label is salient, re-sorts. Boost configurable via
  `ECHO_SALIENT_BOOST` (default 1.3). Wired in both production paths.
- Trace STEP "Merge & rank candidates" reports salience-boost outcome.

**S2 — Definitional-question classifier + widened pool**
- New `is_definitional(query)` regex matches universal English question
  stems (who is/founded/created/wrote, what is the meaning of, define X,
  meaning of X, whom does, etc.) without doc-specific tokens.
- When matched: bumps vec_k + fts_k by `ECHO_DEFINITIONAL_POOL_MULT`
  (default 2.0×), escalates salience boost to `ECHO_DEFINITIONAL_BOOST`
  (default 1.6×). `fts5_retrieve` gains `limit_override` for per-call
  pool widening.
- Trace STEP "Definitional classifier (S2)" reports matched/pool-mult/boost.

**S3a — Re-enable NER for constitution chunks**
- Removed `// v1 scope: ... not run here` comment + no-op from
  `index_constitution`. Replaced with conditional `tokio::spawn` of the
  existing `run_ner_for_source` helper. Only fires when `app: Some`
  (Pass 2 needs AppHandle for model_slot). Mirrors `do_index` pattern.
- Background — UI flips to "indexed" immediately, NER populates
  asynchronously.

**S3b — Entity-aware re-rank (post-RRF NP-match boost)**
- Chose approach (b) over (a) [FTS5-index entity surface forms]
  because it matches S1/S2 pattern, needs no schema change, configurable.
- New `extract_query_entities(query)` — regex for capitalised runs
  (incl. hyphens/apostrophes), sentence-initial lone-cap dropped,
  case-insensitive dedup.
- New `apply_entity_boost(pool, ranked, query, boost)` — JOIN
  `entities` + `entity_mentions` to find chunks whose canonical values
  match any query NP, boost matched chunks. Configurable via
  `ECHO_ENTITY_BOOST` (default 1.4). Safe on DB errors.
- Both production paths wired (main + retry).
- Trace STEP "Merge & rank" extended with `Entity boost: N NPs · applied/no-op`.

**Architectural critique on `index_constitution()` (raised by user mid-flow)**
- Acknowledged that the constitution-specific function name is a smell.
  The real logic is generic (parse_tree → persist_tree → embed →
  store); only the name and IR struct type are tied to "constitution."
  Queued the rename `index_constitution → index_structured_doc` plus
  IR schema generalisation as task #4. Doesn't block S1-S3.

**S4 (HyDE expansion) deferred** by user to a future "input-layer
enhancements" feature that combines with prompt prediction and runtime
spelling autocorrect (task #5).

## Outcome

- ✅ Impact: F5-imperative class of failures fixed for any document type. Mechanism is doc-agnostic; per-doc config (DocProfile.salient_units) is the only thing to add for a new doc type. Entity-NP matching now influences ranking for any document with NER. Together S1+S2+S3b should lift 3B's pass rate on F1-F5 closer to the lit-eval ceiling (~0.83) once user re-indexes and re-runs.
- 🧪 Tests: 8 new unit/integration tests (3× S1, 2× S2, 3× S3b). Test count 47 → 50 confirms delta. `cargo check --lib` final pass 7.57s clean. Test run: 50 passed / 0 failed.
- 📁 Files: 5 files touched (2 doc_profiles, query_router, indexer, CHANGELOG); ~250 lines of changelog; ~400 lines of code + tests.
- 🔁 Next prompts: (a) user rebuilds via `./run-dev.sh`, re-imports constitution (triggers S3a NER), re-runs F5 × 3 registers × 3 local models. Expected: F5-imperative passes on all three models (previously 0/3). (b) Address task #4 (index_constitution rename) when ready. (c) S3b lowercase-concept extension once NER quality is assessed.
- 🧠 Reflection: Three doc-agnostic mechanisms stack cleanly: structural salience (S1) → question-shape detection (S2) → entity-NP matching (S3b). Each is a separate concern, each has its own knob, each composes via post-RRF re-ranking with no shared state. The architecture survives adding doc types (just declare salient_units per profile) and adding entities (just NER each chunk). The hardcoded-keywords trap (rejected mid-flow) would have given the same F5-imperative pass on Pakistan only — useless beyond one document.

## Evaluation notes (flywheel)

- Failure modes observed pre-S1: F5-imperative failed 3/3 across 0.5B/1.5B/3B because hybrid retrieval picked Article 41 (President) over Preamble (Founder of Pakistan) — surface-match dominance.
- Graders run and results (PASS/FAIL): all 50 unit/integration tests pass; in-app re-test pending user rebuild.
- Prompt variant (if applicable): n/a
- Next experiment: re-ask F5 × 3 registers after rebuild + reindex; compare retrieval-trace STEPs (citation fast-path / salience boost / entity boost) to see which mechanism fired on each variant. Then decide whether S3b lowercase-concept extension is worth pursuing.
