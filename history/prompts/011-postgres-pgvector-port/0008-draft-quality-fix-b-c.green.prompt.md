---
id: 0008
title: Draft quality Fix B and C
stage: green
date: 2026-06-12
surface: agent
model: claude-fable-5
feature: 011-postgres-pgvector-port
branch: 011-postgres-pgvector-port
user: Riaz Mehmood
command: perform fix B and C now
labels: [010-one-ask-drafting, draft-quality, verify-gate, tdd]
links:
  spec: specs/010-one-ask-drafting/spec.md
  ticket: null
  adr: null
  pr: null
files:
  - src-tauri/src/services/drafting/one_ask/verify.rs
  - CHANGELOG_DEV.md
tests:
  - services::drafting::one_ask::verify::tests::citation_quote_is_short_snippet_not_whole_excerpt
  - services::drafting::one_ask::verify::tests::citation_quote_stops_at_first_sentence_boundary
  - services::drafting::one_ask::verify::tests::flags_collect_at_paragraph_end_not_mid_sentence
  - cargo test --lib drafting::one_ask (27/27)
  - cargo test --test one_ask_acceptance (3/3)
---

## Prompt

perform fix B and C now

## Response snapshot

Executed the two queued draft-quality fixes from the 2026-06-12 Fix A changelog entry,
one TDD cycle and one commit each, on branch 011-postgres-pgvector-port:

**Fix B (`8d510b1e`)** — `verify.rs` spliced the WHOLE chunk excerpt at every `[CITE:n]`
marker (`quoted = card.excerpt.clone()`), dumping raw multi-sentence chunks with OCR
artifacts mid-paragraph. New `citation_snippet()`: quote only the leading operative span —
first sentence boundary (`.`/`;`, abbreviation-guarded so "Rs."/"No." don't cut) once past
SNIPPET_MIN_WORDS (6), hard cap SNIPPET_MAX_WORDS (30); returned as an exact char-slice of
the excerpt so the verbatim invariant holds by construction. `rendered_label` unchanged.
Two tests watched RED (whole 46-word excerpt spliced; no sentence cut) before implementing.

**Fix C (`0d7ed1ba`)** — Flag runs replaced their offending run IN PLACE, chopping
sentences ("Under ⚑flag, the authority…"). `assemble_and_verify` now stable-partitions
each paragraph's runs: prose/citation/figure first (original order), flags appended at
paragraph end (original order). Never silent (FR-007), just not mid-sentence. Test watched
RED (Flag at runs[1]) before the partition. Frontend renders runs in order — no UI change.

Acceptance tests are order-independent (`.iter().any`), verified before designing Fix C.

## Outcome

- ✅ Impact: completes the queued Fix B+C pair behind Fix A (622fce77); draft paragraphs
  now carry short verbatim quotes and uninterrupted prose, with flags grouped at the end.
- 🧪 Tests: 3 new (each watched RED first); drafting::one_ask 27/27; one_ask_acceptance 3/3.
- 📁 Files: src-tauri/src/services/drafting/one_ask/verify.rs; CHANGELOG_DEV.md (2 entries).
- 🔁 Next prompts: ONE live summary re-run vs the 2026-06-12 baseline draft (user-attended
  empirical gate), then resume 011 US2 packaging → US4 parity + constitution §IV amendment.
- 🧠 Reflection: char-slice snippet (not word-rejoin) keeps the verbatim gate trivially
  true; stable partition preserves both groups' internal order so no other test moved.

## Evaluation notes (flywheel)

- Failure modes observed: none in suite; real gate is the live re-run vs baseline.
- Graders run and results (PASS/FAIL): cargo lib + acceptance suites PASS.
- Prompt variant (if applicable): n/a
- Next experiment (smallest change to try): live summary re-run; if quotes still read
  rough, consider preferring the span around the resolved provision label over the lead.
