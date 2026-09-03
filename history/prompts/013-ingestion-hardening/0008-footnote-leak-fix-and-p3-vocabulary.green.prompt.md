---
id: 0008
title: Footnote-leak fix + Phase 5 vocabulary resolver (D1)
stage: green
date: 2026-06-20
surface: agent
model: claude-opus-4-8
feature: 013-ingestion-hardening
branch: 013-ingestion-hardening
user: engr.riaz2010@gmail.com
command: "fix the footnote leak tests then move to phase 5 …"
labels: [ingestion, chrome, doc-profiles, vocabulary, US1]
links:
  spec: specs/013-ingestion-hardening/spec.md
  ticket:
  adr:
  pr:
files:
  - src-tauri/src/services/parsers/chrome.rs
  - src-tauri/src/services/doc_profiles/mod.rs
  - src-tauri/src/services/heading_detector/regex_pass.rs
  - src-tauri/src/services/structured_importer.rs
  - specs/013-ingestion-hardening/tasks.md
  - MODULAR_IMPROVEMENT_LOG.md
tests:
  - "cargo test --lib parsers::chrome: 22/22 (2 footnote tests fixed)"
  - "cargo test --lib: 502 passed / 0 failed"
---

## Prompt

fix the footnote leak tests then move to phase 5 do we need to reindex for phase 5
and after phase 5 how many phases/tasks are remaining?

## Response snapshot

Footnote leak: the 2 red `chrome.rs` tests traced to one regex bug in
`is_footnote_apparatus` — `\b(subs\.|ins\.|…)\b` placed a `\b` after the literal
`.`, which can't be a word boundary before a space, so "Subs. by …" never matched.
Rewrote to `\b(?:subs|ins|ord|…)\b\.?`. 22/22 chrome.

Phase 5 (D1 vocabulary): added `CitationUnit{Word|Unresolved}` +
`resolve_citation_unit` (profile default → self-reference evidence → unresolved on
conflict) in `doc_profiles/mod.rs`; `scan_with_profile_unit` re-mints Article/Clause
labels under the resolved unit (`swap_unit_word`); wired into `structured_importer`.
fundamental_rights (`manual`→"Section") now resolves to "Article". Full lib 502/0.

Answered: re-index IS needed to see Phase 2–5 effects on the live DB (all are
index-time); one re-index covers P0–P3; must force sources to `pending` to defeat
the content-hash skip. After Phase 5, 3 phases remain (6 US4 boundaries, 7 US5
capture, 8 polish/gate) plus the acceptance harness + a few deferred flags.

## Outcome

- ✅ Impact: footnote apparatus now isolated from body; provision labels resolve to
  the document's true unit word ("Article") instead of the profile default leak.
- 🧪 Tests: chrome 22/22; full lib 502 pass / 0 fail.
- 📁 Files: chrome.rs (regex), mod.rs (resolver+tests), regex_pass.rs (override+test),
  structured_importer.rs (wiring), tasks.md, MODULAR_IMPROVEMENT_LOG.md.
- 🔁 Next prompts: force-pending re-index + live-verify D1/P2/footnotes; Phase 6 US4.
- 🧠 Reflection: a single mis-placed `\b` masked as a "7-provision leak"; reading the
  regex semantics beat re-running the pipeline.

## Evaluation notes (flywheel)

- Failure modes observed: word-boundary-after-dot regex anti-pattern (recurs across
  abbreviation matching).
- Graders run and results (PASS/FAIL): full lib gate PASS (502/0).
- Prompt variant (if applicable): n/a
- Next experiment (smallest change to try): re-index the control set, query the live
  DB for `unit_label LIKE 'Section %'` (expect 0) and Article 7–40 presence.
