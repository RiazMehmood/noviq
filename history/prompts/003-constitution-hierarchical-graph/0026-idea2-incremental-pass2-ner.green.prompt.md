---
id: 0026
title: Idea 2 — incremental full-coverage Pass-2 NER (10% batches, 30s rest, until 100%)
stage: green
date: 2026-05-23
surface: agent
model: claude-opus-4-7
feature: 003-constitution-hierarchical-graph
branch: 003-constitution-hierarchical-graph
user: engr.riaz2010@gmail.com
command: chat (user-revised Idea 2 spec — iterative coverage, not fixed sample)
labels: [ner, pass2, incremental, coverage, scheduler, salient-priority, doc-agnostic]
links:
  spec: specs/003-constitution-hierarchical-graph/spec.md
  ticket: ""
  adr: ""
  pr: ""
files:
  - path: src-tauri/src/db/migrations/015_ner_pass2_progress.sql
    change: NEW migration — chunks.ner_pass2_done INTEGER NOT NULL DEFAULT 0 CHECK ((0,1)); partial index on (source_id, ner_pass2_done) WHERE ner_pass2_done=0 for cheap pending lookup.
  - path: src-tauri/src/services/indexer.rs
    change: NEW knobs (ECHO_PASS2_BATCH_PCT default 10%, ECHO_PASS2_SLEEP_SECS default 30s). NEW helpers pass2_batch_size, pass2_sleep_secs, select_next_pass2_batch (even-sample + always-include salient). NEW run_ner_loop_for_source (iterative loop with scheduler pause + per-chunk ner_pass2_done=1 marking on Pass-2 success). NEW resume_pending_pass2_ner (startup recovery). index_constitution() now spawns run_ner_loop_for_source instead of single-shot run_ner_for_source. Legacy ECHO_PASS2_CHUNK_LIMIT kept as deprecated absolute-count override. 7 new tests in pass2_coverage_tests module.
  - path: src-tauri/src/lib.rs
    change: setup_models_and_watchers calls resume_pending_pass2_ner after the existing pending-NER recovery block, so any source with pending Pass-2 chunks (from previous session OR this migration) gets the loop spawned automatically on startup.
  - path: CHANGELOG_DEV.md
    change: appended full Idea 2 entry above Idea 1's entry.
tests:
  - name: services::indexer::pass2_coverage_tests::batch_size_pct_default_is_ten_percent
    type: unit
    result: NEW
  - name: services::indexer::pass2_coverage_tests::batch_size_floors_at_one
    type: unit
    result: NEW
  - name: services::indexer::pass2_coverage_tests::legacy_absolute_count_overrides_pct
    type: unit
    result: NEW
  - name: services::indexer::pass2_coverage_tests::sleep_secs_respects_range
    type: unit
    result: NEW
  - name: services::indexer::pass2_coverage_tests::first_batch_includes_salient_even_when_offset
    type: integration
    result: NEW — Preamble at offset index 1 is included on top of even-sample (0,10,20,…), 11 chunks total
  - name: services::indexer::pass2_coverage_tests::second_batch_excludes_first_batch_chunks
    type: integration
    result: NEW — after marking 10 done, second batch is disjoint and covers 10% of remaining 90 = 9 chunks
  - name: services::indexer::pass2_coverage_tests::empty_batch_when_all_done
    type: integration
    result: NEW — loop terminator
---

## Prompt

> yes, ship Idea 2
>
> (follow-up) I want to include complete document, at first it extract 10% remaining 90%, then after first extraction complete it started extraction of 10% again now remaining 80% and go along the line till 100% document extraction complete this is idea 2
>
> (follow-up) or we can use 5% bench mark for this also
>
> (follow-up) 10% batches, 30s sleep, go ahead

## Response snapshot

**Mid-implementation user re-spec:** Initially Idea 2 was framed as a single sampling pass at higher percentage. User clarified mid-implementation that the intent is **iterative full coverage** — pass 1 covers 10%, pass 2 covers next 10% of remaining, …, pass 10 covers final remaining → 100% of chunks have entity data. Stopped coding, surfaced the schema change (need per-chunk progress column) + lifecycle change (single-shot → loop) + startup-resume requirement, got sign-off on parameters (10% batches, 30s sleep), then implemented the full design in one focused pass.

**Implementation:**
1. Added migration `015_ner_pass2_progress.sql` with the `ner_pass2_done` column + partial index for cheap pending lookups.
2. Added knobs `ECHO_PASS2_BATCH_PCT` (default 10) and `ECHO_PASS2_SLEEP_SECS` (default 30) with range clamping. Kept `ECHO_PASS2_CHUNK_LIMIT` as a deprecated absolute-count override for one release (logs warning when used).
3. Added `select_next_pass2_batch`: even-sample across pending chunks UNION with all pending salient chunks (via the doc-agnostic `doc_profiles::is_salient_unit` helper from S1). No constitution-specific code.
4. Added `run_ner_loop_for_source`: runs Pass-1 regex on all chunks once, then loops calling `select_next_pass2_batch` → process each chunk → mark `ner_pass2_done = 1` on success → emit `kg://progress` event → sleep → repeat. Exits when no pending chunks remain → marks `sources.ner_status='done'` (final). Registers with `WorkloadKind::Pass2Ner` scheduler so it pauses when Chat takes foreground (existing infrastructure, no new work).
5. Added `resume_pending_pass2_ner`: startup recovery that scans for sources with any `ner_pass2_done = 0` chunks and spawns the loop for each. Idempotent.
6. Wired `index_constitution` to spawn `run_ner_loop_for_source` instead of single-shot `run_ner_for_source`. Left `do_index`'s inline path unchanged for this round (queued as a follow-up to unify both code paths).
7. Wired `lib.rs::setup_models_and_watchers` to call `resume_pending_pass2_ner` after the existing pending-NER recovery block.
8. Added 7 new unit/integration tests covering knob parsing, batch-size math, salient-priority union, iteration disjointness, and the empty-batch terminator.
9. Full services test suite: 125/125 pass (118 pre-Idea-2 + 7 new).

## Outcome

- ✅ Impact: Pass-2 NER coverage goes from a fixed-cap 0.7% (10 / 1467 chunks) to 100% over multiple bounded passes. Salient chunks (Preamble) get entity rows in the very first batch — S3b entity-boost starts paying off within minutes of indexing, not after 10 sessions of waiting. Chat-priority pause prevents the loop from disrupting UX. Generic: any future doc type with salient_units declared in its DocProfile benefits identically.
- 🧪 Tests: 7 new, all passing. Full services suite 125/125 green.
- 📁 Files: 1 new migration (15 lines), 1 indexer.rs section (~360 lines added across knobs, helpers, loop, resume, tests), 1 lib.rs startup hook (3 lines), CHANGELOG (~90 lines).
- 🔁 Next prompts: (a) user rebuilds via ./run-dev.sh (migration applies), re-imports constitution or lets the existing data be processed by startup loop. (b) Watch `kg://progress` events flowing every batch; chat-priority pause should be observable when chatting interrupts the loop. (c) After ~10 batches (a few hours of idle time), query the DB to confirm `SELECT COUNT(*) FROM chunks WHERE source_id=? AND ner_pass2_done=1` equals total. (d) Re-run F5 suite — Preamble entities now populated, S3b entity-boost should fire and offer redundant guarantee on top of Idea 1's chunking fix.
- 🧠 Reflection: User's clarification mid-implementation was correct and improved the design substantially. The fixed-cap pass-once design would have left S3b structurally blocked; the iterative loop makes the entity layer actually useful. Cost of pausing to confirm spec parameters (10% vs 5%, 30s sleep) was small; cost of building the wrong thing would have been large.

## Evaluation notes (flywheel)

- Failure modes observed pre-Idea-2: Preamble had ZERO entity_mentions despite NER being "done"; all S3b entity-boost calls returned `no-op`; coverage stuck at 0.7% no matter how many times sources were re-indexed.
- Graders run and results (PASS/FAIL): all 7 new unit/integration tests pass; full services suite 125/125 pass; in-app validation pending user rebuild + a few sessions of background convergence.
- Prompt variant (if applicable): n/a
- Next experiment: after a few sessions, run `SELECT canonical_value, entity_type FROM entities e JOIN entity_mentions em ON em.entity_id = e.id JOIN chunks c ON c.id = em.chunk_id WHERE c.unit_label='Preamble'` — expect rows for Quaid-i-Azam, Mohammad Ali Jinnah, Pakistan, Founder of Pakistan, Allah, Islam, Holy Quran. Then re-run F5 imperative — Idea 1 already passes it; this is the redundancy layer.
