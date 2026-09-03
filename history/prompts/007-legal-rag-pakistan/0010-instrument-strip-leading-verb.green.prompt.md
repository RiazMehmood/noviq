---
id: 0010
title: Instrument regex strips sentence-leading command verb (Q15 "Identify …")
stage: green
date: 2026-06-07
surface: agent
model: claude-opus-4-8
feature: 007-legal-rag-pakistan
branch: 007-legal-rag-pakistan
user: Riaz Mehmood
command: (conversation — Fix 2 via TDD)
labels: [intent-router, slots, regex, bugfix, tdd, fundamental-rights]
links:
  spec: specs/007-legal-rag-pakistan/spec.md
  ticket:
  adr:
  pr:
files:
  - src-tauri/src/services/intent_router/slots.rs
  - CHANGELOG_DEV.md
  - history/prompts/007-legal-rag-pakistan/0010-instrument-strip-leading-verb.green.prompt.md
tests:
  - "cargo test -p echo-ai --lib intent_router — 21/21 pass"
---

## Prompt

> do Fix 2 next, same TDD approach

(Fix 2 of the Q12/Q15 diagnosis: the instrument regex captured the sentence-leading
imperative verb "Identify" into the named instrument, poisoning Q15's system prompt.)

## Response snapshot

RED → GREEN. RED: added `instrument_drops_leading_command_verb` asserting
`extract("Identify the Principles of Policy …").instrument == "Principles of Policy"`;
watched it fail (instrument leaked "Identify"), 11 prior slots tests passing.

GREEN: added `strip_leading_command_verb()` helper — drops a leading known command
verb (identify/list/explain/describe/summarize/compare/define/give/show/tell/find/
name/state/outline/discuss/provide/quote/analyse) plus any dangling leading article
(the/a/an), then wired it into the instrument-extraction loop before the label is
built. "Identify the Principles of Policy" → "Principles of Policy".

No regressions; the change also cleans the two existing instrument tests for free
(RTI Act cases). `intent_router` 21/21 green.

## Outcome

- ✅ Impact: removes the second half of Q15's garbage system prompt (after Fix 1
  removed "Article de"). Instrument now resolves to the correct chapter name.
- 🧪 Tests: `intent_router` 21/21 (1 new). Watched RED fail first.
- 📁 Files: `slots.rs` (helper + wiring + 1 test), `CHANGELOG_DEV.md` entry.
- 🔁 Next prompts: Fix 3 — `query_router.rs:235` `is_followup_query` false-positive
  on intra-sentence pronouns (Q12). Then rebuild + re-run the 15-prompt eval to
  measure how far Q12/Q15 move.
- 🧠 Reflection: a verb blacklist is a blunt instrument but matches the bounded set
  of imperative openers seen in officer queries; revisit if a legit instrument ever
  starts with one of these words.

## Evaluation notes (flywheel)

- Failure modes observed: capitalised sentence-initial tokens satisfying an
  `[A-Z]`-anchored named-entity group — a recurring NER/regex hazard.
- Graders run and results (PASS/FAIL): unit tests PASS (21/21). Full 15-prompt eval
  re-run pending after Fix 3.
- Prompt variant (if applicable): n/a (code fix).
- Next experiment (smallest change to try): Fix 3 — gate `is_followup_query` so a
  pronoun does not flag a self-contained question that names its own subject.
</content>
