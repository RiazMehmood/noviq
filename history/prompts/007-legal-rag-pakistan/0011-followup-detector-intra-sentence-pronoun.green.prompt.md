---
id: 0011
title: Follow-up detector ignores intra-sentence pronouns (Q12)
stage: green
date: 2026-06-07
surface: agent
model: claude-opus-4-8
feature: 007-legal-rag-pakistan
branch: 007-legal-rag-pakistan
user: Riaz Mehmood
command: (conversation — Fix 3 via TDD)
labels: [query-router, followup, chat-memory, bugfix, tdd, fundamental-rights]
links:
  spec: specs/007-legal-rag-pakistan/spec.md
  ticket:
  adr:
  pr:
files:
  - src-tauri/src/services/query_router.rs
  - CHANGELOG_DEV.md
  - history/prompts/007-legal-rag-pakistan/0011-followup-detector-intra-sentence-pronoun.green.prompt.md
tests:
  - "cargo test -p echo-ai --lib query_router::tests::followup — 3/3 pass"
  - "cargo test -p echo-ai --lib query_router — 58 pass, 1 pre-existing fail (token budget, unrelated)"
---

## Prompt

> do Fix 3 next, same TDD approach

(Fix 3 of the Q12/Q15 diagnosis: `is_followup_query` false-fired on the intra-sentence
pronouns "it"/"them" in Q12, polluting retrieval with the prior turn's education
keywords and injecting 2 prior Q/A turns into the prompt.)

## Response snapshot

RED → GREEN. RED: added `followup_detector_skips_query_that_names_its_own_subject`
asserting `!is_followup_query("What are the Principles of Policy, and whose
responsibility is it to act on them?")`; watched it fail (returns true), the 2 prior
follow-up tests passing.

GREEN: in `is_followup_query`, moved the continuation-prefix check ahead (still
unconditional), then added `names_own_subject()` — true when a capitalised content
word (≥3 alpha chars) appears past the sentence-initial token. When the query names
its own subject, the anaphora-token signal is suppressed → self-contained. This fixes
both the retrieval keyword carry-over and the prior-turn prompt injection, which share
the predicate.

Verified existing tests still hold: anaphora-without-named-subject still flags
("Does this rule apply to women officers?"), self-contained still skips. 3/3 follow-up
green. The single module failure is the pre-existing
`context_builder_token_budget_drops_large_chunk` (token-budget logic, unrelated).

## Outcome

- ✅ Impact: Q12 retrieval no longer polluted by Q11's education keywords; prompt no
  longer carries unrelated prior turns. Completes the 3-fix Q12/Q15 batch.
- 🧪 Tests: 3/3 follow-up green; query_router otherwise unchanged. Watched RED first.
- 📁 Files: `query_router.rs` (predicate edit + `names_own_subject` + 1 test),
  `CHANGELOG_DEV.md` entry.
- 🔁 Next prompts: rebuild the app + re-run the 15-prompt eval (V3) to measure how
  far Q12/Q15 (and the rest) move now that all three fixes are in. Compare against
  V2's ~9–10/15.
- 🧠 Reflection: case-dependent named-subject detection is a deliberate trade-off —
  fully lowercased queries that name a subject could still false-positive; acceptable
  because carry-over is retrieval-only and eval/officer queries are capitalised.

## Evaluation notes (flywheel)

- Failure modes observed: anaphora heuristics cannot distinguish intra-sentence from
  cross-turn reference without an antecedent model; capitalisation is the cheap proxy.
- Graders run and results (PASS/FAIL): unit PASS (3/3 follow-up). Full 15-prompt eval
  re-run pending (V3).
- Prompt variant (if applicable): n/a (code fix).
- Next experiment (smallest change to try): rebuild + re-index fundamental_rights.pdf,
  re-run the 15 grounded prompts on the 0.5B FT, score Q12/Q15 specifically.
</content>
