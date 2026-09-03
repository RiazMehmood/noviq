---
id: 0012
title: US3 doc-set curation (propose over whole corpus → officer confirms = scope)
stage: green
date: 2026-06-08
surface: agent
model: claude-opus-4-8
feature: 009-lgd-drafting
branch: 009-lgd-drafting
user: Riaz Mehmood
command: continue on US3 for 009 feature
labels: [feature-009, lgd-drafting, us3, docset, backend]
links:
  spec: specs/009-lgd-drafting/spec.md
  ticket:
  adr: history/adr/0008-deterministic-citation-integrity-for-answers-and-drafts.md
  pr:
files:
  - src-tauri/src/services/drafting/docset.rs
  - src-tauri/src/services/drafting/mod.rs
  - src-tauri/src/commands/drafting.rs
  - src-tauri/src/lib.rs
  - specs/009-lgd-drafting/tasks.md
  - CHANGELOG_DEV.md
tests:
  - "cargo test --lib drafting: 21 passed (4 new docset tests)"
  - "cargo test --test lgd_drafting_acceptance: 1 passed"
---

## Prompt

continue on US3 for 009 feature

## Response snapshot

Implemented US3 (T028–T030) backend for the LGD drafting feature; deferred the
T031 frontend to the Phase 7 UI wave.

- `docset.rs` (pure/testable; DB/embedder stay in command, mirroring `render.rs`):
  `intent_query` (generic concat of purpose/scheme/addressee), `tag_role`
  (generic instrument-class lexicon → Rules else Case; officer-correctable
  proposal, not a routing key), `propose_candidates` (dedupe-to-document,
  preserve retrieval order).
- `commands/drafting.rs`: `DraftState` + `DraftSessions = Mutex<HashMap<..>>`
  in-memory session store; `retrieve_whole_corpus` (FR-016, indexed+watched
  scope, K=60); `start_draft → DraftSession{draftId,candidateDocs}`;
  `update_doc_set → DocSetUpdate{gaps:[]}` (US4 fills gaps).
- `mod.rs`: added `DraftSession` + `DocSetUpdate` camelCase output types.
- `lib.rs`: registered both commands + `app.manage(DraftSessions::default())`.

`render_draft` left unchanged — the deferred frontend splits `confirmedDocs` by
role into the `rules_source_ids`/`case_source_ids` it already takes.

## Outcome

- ✅ Impact: officer now drives provenance — propose over whole corpus, retag,
  confirm; only the confirmed set scopes the draft (FR-016/018).
- 🧪 Tests: `cargo test --lib drafting` 21/21 (4 new); acceptance 1/1.
- 📁 Files: docset.rs implemented, mod.rs +2 types, command +2 cmds +session
  store, lib.rs registration, tasks.md T028–T030 ✅, changelog entry.
- 🔁 Next prompts: US4 gap detection (`gaps.rs` + `resolve_gap` +
  `drafting://docset-updated` watch), then Noviq UI.
- 🧠 Reflection: kept docset pure and pushed retrieval to the command, matching
  the established US1 render pattern — keeps the core unit-testable with no DB.

## Evaluation notes (flywheel)

- Failure modes observed: none — clean compile + green on first test run.
- Graders run and results (PASS/FAIL): drafting lib PASS, acceptance gate PASS.
- Prompt variant (if applicable): n/a
- Next experiment (smallest change to try): wire `gaps::detect` into
  `update_doc_set` (US4) and verify gaps shrink on `docset-updated`.
