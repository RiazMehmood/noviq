---
id: 0009
title: Draft quality fixes D1 D2 E F
stage: green
date: 2026-06-12
surface: agent
model: claude-fable-5
feature: 011-postgres-pgvector-port
branch: 011-postgres-pgvector-port
user: Riaz Mehmood
command: before proceeding to fixes i think cross verify it with the db also. / yes proceed with D1 → D2 → E → F / i dont want this strategy... perform all fixes at ones then run red-green all combine one time
labels: [010-one-ask-drafting, draft-quality, verify-gate, db-cross-verification]
links:
  spec: specs/010-one-ask-drafting/spec.md
  ticket: null
  adr: null
  pr: null
files:
  - src-tauri/src/services/drafting/one_ask/verify.rs
  - src-tauri/src/services/drafting/one_ask/research.rs
  - src-tauri/tests/fixtures/one_ask/canned_writer/legal_procedural_position.txt
  - src-tauri/tests/fixtures/one_ask/canned_writer/financial_aspect.txt
  - src-tauri/tests/fixtures/one_ask/canned_writer/proposal_recommendation.txt
  - CHANGELOG_DEV.md
tests:
  - cargo test --lib drafting::one_ask (36/36, 9 new)
  - cargo test --test one_ask_acceptance (3/3)
---

## Prompt

(1) "before proceeding to fixes i think cross verify it with the db also."
(2) "yes proceed with D1 → D2 → E → F"
(3) "i dont want this strategy that u attempt fix then test it red then its green, again
second fix then its red and green, i want you to perform all fixes at ones then run
red-green all combine one time only this will save time too much."

## Response snapshot

**DB cross-verification first** (bundled PG `echo` on port 36079, read-only): the
"same quote ×3 paragraphs" was ONE chunk/ONE card correctly capped to 2 sections by Fix A —
the leak is that `verify.rs` resolves global `[CITE:n]` ids with no section-membership check
(hallucinated id → real card from another section). Card labels parse the content prefix
("Section 2010" = mis-captured year) while `chunks.unit_label` holds the correct
"Section 6.20". 14 of 17 flags were echoes of labels/excerpts of cards cited in the same
paragraph. 1,271 duplicate-content groups in `chunks` are <200-char boilerplate (backlog).

**Batch implementation** (user directed: all fixes at once, one combined test cycle, after
D1 had already been watched RED individually):
- D1: out-of-section `[CITE:n]` → Flag.
- D2: drop citation directly following a citation + repeat cites of same card per paragraph.
- E: `card_label()` prefers `chunks.unit_label` (fetched alongside page_number), fallback
  content parse; `citation_snippet` strips the leading provision-label prefix (falls back to
  full excerpt when label-only).
- F: `foreign_number_in_prose` takes the section's assigned cards; normalized containment
  against label/excerpt allows source-backed echoes; cross-section echoes still flag.

Acceptance fixture writers cited out-of-section ids (predating section scoping); ground
truth obtained via a temporary research-dump test (removed): legal's only card is 3,
financial's is 2, proposal has 4/5. Fixtures rewritten accordingly.

## Outcome

- ✅ Impact: completes the D1–F quality batch on top of A–C; citations are section-scoped,
  unstacked, correctly labelled, label-prefix-free, and connective prose survives the gate.
- 🧪 Tests: drafting::one_ask 36/36 (9 new: out-of-section flag, stack collapse, repeat
  drop, prefix strip, label-only fallback, backed-number/figure allowed, cross-section echo
  still flagged, card_label preference); one_ask_acceptance 3/3.
- 📁 Files: verify.rs, research.rs, 3 canned fixtures, CHANGELOG_DEV.md.
- 🔁 Next prompts: live summary re-run vs the 2026-06-12 baseline (user-attended gate);
  backlog = index-time content-prefix mis-capture + duplicate boilerplate chunks (F007).
- 🧠 Reflection: the DB cross-check inverted two assumptions (dedup was fine; labels were
  our parse, not bad data) — diagnosis-before-code paid for itself again.

## Evaluation notes (flywheel)

- Failure modes observed: acceptance fixtures encoded pre-D1 semantics; caught by the gate.
- Graders run and results (PASS/FAIL): lib + acceptance suites PASS.
- Prompt variant (if applicable): n/a
- Next experiment (smallest change to try): if F's containment proves loose live, tighten
  to exact normalized label equality.
