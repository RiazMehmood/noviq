---
id: 0013
title: US4 gap detection + resolve + watch re-proposal
stage: green
date: 2026-06-08
surface: agent
model: claude-opus-4-8
feature: 009-lgd-drafting
branch: 009-lgd-drafting
user: Riaz Mehmood
command: continue on US4 for 009 feature
labels: [feature-009, lgd-drafting, us4, gaps, backend]
links:
  spec: specs/009-lgd-drafting/spec.md
  ticket:
  adr: history/adr/0008-deterministic-citation-integrity-for-answers-and-drafts.md
  pr:
files:
  - src-tauri/src/services/drafting/gaps.rs
  - src-tauri/src/services/drafting/docset.rs
  - src-tauri/src/commands/drafting.rs
  - src-tauri/src/lib.rs
  - specs/009-lgd-drafting/tasks.md
  - CHANGELOG_DEV.md
tests:
  - "cargo test --lib drafting: 26 passed (5 new: 4 gaps + 1 newly_added)"
  - "cargo test --test lgd_drafting_acceptance: 1 passed"
---

## Prompt

continue on US4 for 009 feature

## Response snapshot

Implemented US4 (T032–T034) backend; deferred T034's auto-trigger and the T035
frontend.

- `gaps.rs`: `detect_gaps(slots, confirmed, destination_folder)` — category-level
  coverage by required ROLE (plain cited → rules, `fact:` → case) + salient-token
  overlap on doc titles; generic, no hardcoded category→doc mapping. Uncovered
  cited slot → `Gap`.
- `docset.rs`: `newly_added(old, new)` diff helper for the watch re-proposal.
- `commands/drafting.rs`: `update_doc_set` now async, returns real gaps
  (`gaps_for` = template slots + `primary_watched_folder`); `resolve_gap`
  records `proceedWithout` in session `gap_resolutions`; `refresh_doc_set`
  re-proposes, auto-adds new docs to confirmed (FR-020), recomputes gaps, emits
  `drafting://docset-updated`. `DraftState` gained `gap_resolutions`.

Process note: the user corrected me for running multiple separate cargo
invocations — batched to ONE run per milestone going forward
([[feedback-batch-builds]]). Killed stray monitor-loop shells; only one cargo
run remained.

## Outcome

- ✅ Impact: refuse-or-flag loop is productive end-to-end — gaps reported with a
  destination folder, supplied docs auto-join the set, proceed-without flags.
- 🧪 Tests: `cargo test --lib drafting` 26/26 (5 new); acceptance 1/1; lib builds
  clean.
- 📁 Files: gaps.rs implemented, docset.rs +diff helper, command +2 cmds +async
  update_doc_set +session gap_resolutions, lib.rs registration, tasks/changelog.
- 🔁 Next prompts: US5 iterative revision + Noviq UI shell (Phase 7).
- 🧠 Reflection: kept gap detection pure on titles (no DB content read) to stay
  testable; coverage is a category-level heuristic the officer resolves — true
  to the clarification and the diagnose-before-code rule.

## Evaluation notes (flywheel)

- Failure modes observed: title-only coverage can yield false gaps when a doc's
  relevance lives in body text not its title; acceptable (officer curates /
  proceeds). Process: avoid multiple cargo runs per milestone.
- Graders run and results (PASS/FAIL): drafting lib PASS, acceptance gate PASS.
- Prompt variant (if applicable): n/a
- Next experiment (smallest change to try): wire `refresh_doc_set` to a
  file_watcher/indexer completion hook (T034 auto-trigger) once the UI exists.
