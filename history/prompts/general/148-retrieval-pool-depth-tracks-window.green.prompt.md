---
id: 148
title: Retrieval pool depth tracks the window
stage: green
date: 2026-08-07
surface: agent
model: claude-opus-5
feature: general
branch: 027-my-projects
user: Riaz Mehmood
command: (conversational)
labels: [retrieval, recall, union-merge, ctx-top-k, proxima-4, invariant]
links:
  spec: n/a
  ticket: Proxima finding #4 (Critical) — under-retrieval of the governing instrument
  adr: n/a
  pr: n/a
files:
  - src-tauri/src/services/query_router.rs
  - CHANGELOG_DEV.md
tests:
  - merge_depth_never_starves_the_window
  - merge_depth_preserves_the_operator_ratio
  - merge_depth_redistributes_a_short_arm
  - merge_depth_cannot_exceed_what_was_fetched
  - merge_depth_never_zeroes_a_live_arm
  - merge_depth_degenerate_shares_do_not_empty_the_pool
  - merge_depth_clamps_overfetch_below_one
---

## Prompt

> recall memory what we were working on?

> yes, start on retrieval recall depth but we dont want fix to a specific case
> only, we want it dynamic for any future case occured also.

> we set its ratio 80% vec & 20% fts
> therefore it is getting only 7 fts

> yes, implement it that way

## Response snapshot

Diagnosed on prod before writing code (the discipline that the previous two
sessions' dead premises earned). Read the running `.env` and a live trace first:

- Prod runs `ECHO_VEC_K=28`, `ECHO_FTS_K=7` — **not** the code defaults of 24/24.
- Trace `2026-08-06_19-42-10_0b2a564c`: vector fetch returned **72**, FTS fetch
  returned **72**, `union_merge` emitted **33 unique**, `ctx_top_k` = **36**.

So 144 candidates were fetched from Postgres and 111 discarded before ranking.
`union_merge` read the two knobs as absolute per-arm caps, which meant the
architect's deliberate 80/20 **ratio** silently also pinned total pool **depth**
at 35 — below a selection target that is computed per query from the token
budget and had grown to 36.

⭐ The consequence found in the same trace was larger than the missing chunks:
with pool ≤ K the merge *is* the selection, so `Salience boost: no-op`,
`Entity boost: no-op`, `Instrument boost: top document unchanged`,
`diversity off` — the entire ranking apparatus was **inert in production**. This
retroactively explains the `ECHO_SOURCE_CAP` A/B no-op as the standing state of
the pipeline rather than a quirk of one query.

Fix keeps the ratio as policy and derives depth from the live window:
`depth = ceil(ctx_top_k × ECHO_POOL_OVERFETCH)` floored at `ctx_top_k`, split by
`ECHO_VEC_K : ECHO_FTS_K`. At K=36 that is 58/14 — FTS goes 7 → 14 not because
20% was wrong but because 20% of a depth that tracks the window is 14. No `.env`
edit needed. Applied at all three vec/FTS merge sites, each keyed to its own
selection target (`ctx_top_k`, `retry_k`, `top_k`).

Added a self-reporting invariant to STEP 12 (`Candidate pool: N vs top-K=M`,
flagged when `N < M`), because this starvation was invisible for weeks precisely
because the two numbers live in different files and nothing compared them.

## Outcome

- ✅ Impact: pool at K=36 goes 33 → ~72 candidates, restoring headroom for
  ranking. Structural, not case-specific: any future window change carries the
  pool with it, and any recurrence announces itself in the trace.
- 🧪 Tests: **894 passed / 77 failed** on the VM (`--no-default-features
  --features server --lib`). The 77 are the standing DB-dependent baseline,
  unchanged; 887 + exactly the 7 new tests = 894. **Sabotage-tested twice** —
  reinstating the fixed-cap shape fails 6 of 7; removing the overfetch clamp
  fails the clamp test.
- 📁 Files: `query_router.rs` (`merge_depth` + `merge_depth_from` +
  `knob_pool_overfetch`, 3 call sites, STEP 12 trace field), `CHANGELOG_DEV.md`.
- 🔁 Next prompts: A/B on the VM against finding #4's question before deploying;
  then Build Summaries (`graph_summary` = 0) for topic→instrument scoping.
- 🧠 Reflection: the third consecutive session where **the running config
  differed from the code default**, and the first where reading it *first*
  changed the design instead of wasting the work. `.env` overrides are invisible
  in the repo — a code default proves nothing about prod.

## Evaluation notes (flywheel)

- Failure modes observed: a stated invariant ("candidate supply must exceed the
  selection target") written as a comment at the fetch site and violated 200
  lines later at the merge site — comments cannot enforce anything. Two numbers
  owned by different mechanisms with no comparison between them.
- Graders run and results (PASS/FAIL): unit suite PASS (no new failures);
  sabotage PASS (6/7 and 1/7 respectively). End-to-end retrieval quality
  **NOT YET MEASURED** — requires the VM A/B.
- Prompt variant (if applicable): n/a
- Next experiment (smallest change to try): deploy to a shadow ref and re-run
  finding #4's question plus the 4-query instrument eval, comparing STEP 12
  source mix and the citation set before/after.
