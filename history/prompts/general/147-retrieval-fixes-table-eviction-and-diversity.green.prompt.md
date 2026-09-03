---
id: 147
title: Retrieval fixes — table-completion eviction, source diversity, title boxes
stage: green
date: 2026-08-06
surface: agent
model: claude-opus-5
feature: general
branch: 027-my-projects
user: Riaz Mehmood
command: "plan for retrieval fixes in depth with code and these findings also"
labels: [retrieval, query-router, proxima, table-completion, ranking]
links:
  spec: docs (Noviq_Issues_and_Fixes_2026-08-06.pdf items 3.1/3.2/3.3)
  ticket: Proxima AI QA findings #4 (Critical) and #7 (High)
  adr: none (no new architectural decision — bounds on an existing mechanism)
  pr: none (branch tmp-retrieval-check)
files:
  - src-tauri/src/db/repositories/graph.rs
  - src-tauri/src/services/query_router.rs
  - src-tauri/src/services/parsers/table.rs
  - CHANGELOG_DEV.md
tests:
  - "lib suite on VM: 869/77 baseline → 883/77 (+14 new, same 77 pre-existing DB failures)"
  - "sabotage: diversity no-op fails ONLY source_diversity_caps_a_monopolising_source"
  - "sabotage: fingerprint removed fails ONLY context_builder_drops_exact_duplicate_content"
  - "table.rs guard tests — PENDING (classifier outage blocked the run)"
---

## Prompt

"now plan for retrieval fixes in depth with code and these findings also. then
share the details with me." — following a router flowchart artifact and a reading
of `Noviq_Issues_and_Fixes_2026-08-06.pdf`.

Decisions taken during planning: all four passes including the re-index; the
countable-noun widening deferred ("we know the limitations of the model, and
counting exact 39 table rows is not necessary for now"); source diversity ships
flag-gated OFF.

## Response snapshot

Explored the retrieval path with three parallel Explore agents and one Plan
agent, then verified every load-bearing claim against the **production database**
before writing code — which corrected the plan twice.

Implemented Pass 0 (label normalisation), Pass 1a (bounded table completion via
new `graph::table_chunks_for_best_seed`), Pass 1b (`apply_source_diversity`,
default off), Pass 1c (`content_fingerprint`), Pass 2 (`ECHO_SALIENT_BOOST`
1.3 → 1.0), Pass 3a (`SINGLE_COL_MIN_ROWS = 6`). Full detail in `CHANGELOG_DEV.md`
2026-08-06.

## Outcome

- ✅ Impact: **DEPLOYED 2026-08-07 — `release` = `d07daa47` (Pass 0 + Pass 1).
  Proxima finding #7 is FIXED in production**: the SPPRA board question now cites
  Section 6(2)(i) in 23.8 s with the real composition, where it previously refused
  with 0 citations. Finding #4 improved (Rule 13 leads, 8 citations) but not fixed
  — the top-K still spans 13 documents and `ECHO_SOURCE_CAP` is unset, so Pass 1b
  has not yet been exercised. Pass 2 + Pass 3a remain undeployed.
- 🧪 Tests: 869/77 → 883/77 on the VM, +14 = exactly the new tests. Two sabotage
  runs each failed exactly one test. `parsers::table` run still pending.
- 📁 Files: `graph.rs` (+1 fn), `query_router.rs` (bounds, diversity, fingerprint,
  knobs, 14 tests), `table.rs` (guard + 4 tests).
- 🔁 Next prompts: deploy Pass 0 alone → instrument eval → deploy Pass 1 → re-run
  both traced failures → A/B `ECHO_SOURCE_CAP=auto` → re-index for Pass 3a.
- 🧠 Reflection: **the prod DB was worth more than the report and the plan
  combined.** Three planned changes were wrong and only querying found it.

## Evaluation notes (flywheel)

- Failure modes observed:
  - ⚠️⚠️ **I nearly shipped a change that would have deepened the bug it fixed.**
    A length floor on the vector arm looked obviously right (FTS already has one).
    APT **Rule 16 is 85 chars** — the floor would have deleted the very eligibility
    rules finding #4 is about. Caught only by querying the corpus for the chunks
    the fix would touch. **Never add a filter without listing what it removes.**
  - ⚠️ **I built two bad verification queries and briefly reported a wrong
    conclusion from them** — md5 exact-match to find "the same text twice" (the
    table rendering has pipes, so it is not byte-identical) and `content LIKE '%|%'`
    to detect single-column (markdown pipes appear in single-column tables too).
    Both said "no such thing exists"; both were wrong. The title box IS there.
  - ⚠️ Two of my own new tests failed first run, both self-inflicted: identical
    row content collided with the fingerprint I had just added, and one diversity
    assertion described wave-2 behaviour incorrectly.
  - ⚠️ `cargo check --features llm,server,cuda` fails on this VM (CMake 3.16 <
    3.18 for ggml-cuda); prod uses `--no-default-features`. `/tmp` is a separate
    4.7 G partition and the target dir filled it — build under `$HOME`.
- Graders run and results: lib suite PASS (delta clean); sabotage PASS ×2;
  `parsers::table` PENDING.
- Prompt variant: n/a.
- Next experiment (smallest change to try): re-diagnose the duplicated title text
  at `table.rs:608` (`out[idx] = above + rows + below`) — the planned
  front-matter cause is disproven by `0` cross-`block_class` duplicates.
