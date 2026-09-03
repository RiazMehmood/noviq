---
id: 0014
title: US5 iterative refinement until finalize (persistent revisable session)
stage: green
date: 2026-06-08
surface: agent
model: claude-opus-4-8
feature: 009-lgd-drafting
branch: 009-lgd-drafting
user: Riaz Mehmood
command: continue on US5 for 009 feature
labels: [feature-009, lgd-drafting, us5, draft-session, backend]
links:
  spec: specs/009-lgd-drafting/spec.md
  ticket:
  adr: history/adr/0008-deterministic-citation-integrity-for-answers-and-drafts.md
  pr:
files:
  - src-tauri/src/db/migrations/022_draft_session.sql
  - src-tauri/src/services/drafting/session.rs
  - src-tauri/src/services/drafting/render.rs
  - src-tauri/src/services/drafting/mod.rs
  - src-tauri/src/commands/drafting.rs
  - src-tauri/src/lib.rs
  - specs/009-lgd-drafting/tasks.md
  - CHANGELOG_DEV.md
tests:
  - "cargo test --lib drafting: 31 passed (5 new: 3 session + 2 render_slot)"
  - "cargo test --test lgd_drafting_acceptance: 1 passed"
---

## Prompt

continue on US5 for 009 feature

## Response snapshot

Implemented US5 (T044–T047) backend; deferred T048 frontend.

- Migration 022 `draft_session` (additive, no version bump) + `session.rs`:
  canonical serde `DraftState` (template, intent, candidate+confirmed docs,
  blocks, prose_edits, gap_resolutions, finalized); save/load/delete →
  resume-after-restart (FR-023). DraftState moved out of commands; US3/US4 now
  persist through `get_state`/`put_state` (cache + write-through DB).
- `render.rs`: extracted `render_slot` — the ONE contract code path reused by
  full render AND targeted re-render (T045).
- Commands: reshaped `render_draft` to session form `{draftId, confirmed:true}`
  (FR-022 gate, stores blocks, preserves prose edits); `revise_draft` (tagged
  Revision enum) enforcing FR-025 (cited re-resolves deterministically, editProse
  prose-only + sanitized, only affected blocks re-render); `finalize_draft`
  (FR-026 lock; revise errors after finalize).

Process: single batched cargo run at the end ([[feedback-batch-builds]]).

## Outcome

- ✅ Impact: officer iterates a draft to a converged, finalized, exportable
  instrument; the citation-grade contract holds on every revision.
- 🧪 Tests: `cargo test --lib drafting` 31/31 (5 new); acceptance 1/1; crate
  compiles. ONE cargo invocation.
- 📁 Files: migration 022, session.rs (new), render.rs (render_slot), command
  reshape + revise/finalize, lib.rs registration, tasks/changelog.
- 🔁 Next prompts: Phase 7 Noviq UI shell + per-story frontends, then Polish.
- 🧠 Reflection: one render path (render_slot) means the contract can't drift
  between first render and revisions; write-through persistence keeps the
  in-memory cache and DB in sync without reworking US3/US4 call sites.

## Evaluation notes (flywheel)

- Failure modes observed: none on test; prose is still a sanitized placeholder
  (model wiring T017 deferred). Process corrected earlier: batch cargo runs.
- Graders run and results (PASS/FAIL): drafting lib PASS, acceptance gate PASS.
- Prompt variant (if applicable): n/a
- Next experiment (smallest change to try): wire the prose ModelSlot generator
  (T017) behind `render_one_slot`'s prose closure with tier-degrade.
