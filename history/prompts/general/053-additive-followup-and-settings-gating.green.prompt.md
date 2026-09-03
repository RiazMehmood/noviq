---
id: 053
title: Additive follow-up and settings gating
stage: green
date: 2026-07-02
surface: agent
model: claude-fable-5
feature: general
branch: release
user: Riaz Mehmood
command: continue work (additive follow-up + hide Indexed Folders)
labels: [assistant, additive, retrieval, citations, settings, web-ui]
links:
  spec: none
  ticket: none
  adr: none
  pr: none
files:
  - src-tauri/src/services/assistant/capabilities/additive.rs (new)
  - src-tauri/src/services/assistant/capabilities/mod.rs
  - src-tauri/src/services/assistant/intent.rs
  - src-tauri/src/services/assistant/mod.rs
  - src/pages/SettingsPage.tsx
  - CHANGELOG_DEV.md
tests:
  - additive::tests::merge_renumbers_new_citations_after_prior
  - additive::tests::merge_reuses_prior_number_for_same_chunk
  - additive::tests::remap_rewrites_only_mapped_markers
  - intent::tests::detects_additive_requests
  - intent::tests::plain_and_transform_queries_are_not_additive
  - intent::tests::anaphoric_expand_stays_transform_topical_expand_does_not
---

## Prompt

tested in browser, works — start on the additive follow-up. and one more thing
to notice the indexed documents are not shown in user pan i just want to remove
index folder from setting only not from the ui of chat of user.

## Response snapshot

Two deliverables. (1) Additive follow-up capability: "also cover X" / "add a
section on Y" / "expand on <topic>" now EXTENDS the previous answer. Design:
run the new aspect through the full existing guidance pipeline (one LLM pass,
all retrieval boosts + budget + citation gate intact), then merge
deterministically — prior answer kept verbatim, new answer appended with its
[n] markers remapped to continue after the prior citations (loaded from
query_log.citations_json); a re-retrieved chunk (same verbatim) reuses its
prior number. Detection is `intent::is_additive_request`, checked after
transform in run_turn, both guarded on a prior assistant turn. Transform's
regex was narrowed ("expand/elaborate on it/this" = transform; "expand on
<topic>" = additive) since a rewrite is forbidden from adding facts. (2)
Settings: wrapped the Indexed Folders section in `showAdvanced()` (desktop or
admin) so regular web users no longer see folder management; the chat page's
DocumentTree (left rail) is untouched and still shows the corpus. Committed
`535b181c` + changelog `eb9da4c5`, pushed to `transform-verify`, VM
deploy-parity check + lib tests launched via the established runbook.

## Outcome

- ✅ Impact: additive follow-ups preserve delivered content + citations in one
  generation; users can't touch index-folder management but keep corpus
  browsing in chat.
- 🧪 Tests: 3 merge/remap unit tests + 3 intent detection tests; VM verification
  in flight at time of writing.
- 📁 Files: 6 (1 new handler, 3 Rust edits, 1 TSX edit, changelog).
- 🔁 Next prompts: VM green → push release → browser test ("also cover the
  penalties" → prior answer kept, new section appended, citations continue).
- 🧠 Reflection: reusing guidance::run wholesale kept the additive diff small
  and inherited every proven safeguard; the deterministic merge avoids a
  second LLM pass and the fidelity risk of asking the model to reproduce the
  prior answer.

## Evaluation notes (flywheel)

- Failure modes observed: none new; user declined local `npx tsc` run (deploy
  build covers it).
- Graders run and results (PASS/FAIL): pending VM run.
- Prompt variant (if applicable): none.
- Next experiment (smallest change to try): dedup citations by chunk-id rather
  than verbatim equality if excerpt drift produces duplicates in practice.
