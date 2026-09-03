---
id: 0011
title: Hybrid retrieval and money canonicalization
stage: green
date: 2026-06-13
surface: agent
model: claude-fable-5
feature: 011-postgres-pgvector-port
branch: 011-postgres-pgvector-port
user: Riaz Mehmood
command: yes proceed with H and F2
labels: [010-one-ask-drafting, hybrid-retrieval, rrf, relevance, money-figures]
links:
  spec: specs/010-one-ask-drafting/spec.md
  ticket: null
  adr: null
  pr: null
files:
  - src-tauri/src/services/drafting/retrieval.rs
  - src-tauri/src/services/drafting/one_ask/research.rs
  - src-tauri/src/services/drafting/one_ask/verify.rs
  - src-tauri/tests/fixtures/one_ask/canned_writer/financial_aspect.txt
  - src-tauri/tests/fixtures/one_ask/canned_writer/proposal_recommendation.txt
  - CHANGELOG_DEV.md
tests:
  - cargo test --lib drafting::one_ask (46/46, 6 new)
  - cargo test --test one_ask_acceptance (3/3)
---

## Prompt

yes proceed with H and F2

## Response snapshot

**H — hybrid retrieval**: H-diagnosis data (distances 0.2475–0.3100, irrelevant cards
interleaving with relevant) killed margin tuning. Added `retrieval::lexical()`
(websearch_to_tsquery over the generated `tsv` GIN, same filters as knn),
`lexical_query()` (OR-joined subject tokens; money/magnitude vocabulary excluded — the
discriminator), and `rrf_merge()` (k=60; both legs fetch 2× budget; both-leg chunks
outrank single-leg). Vector margin retained as safety net. Logging covers vec leg,
lex leg, and RRF picks.

**Anti-starvation** (found via the acceptance corpus): cap consumption is
first-come-first-served in skeleton order — with the hybrid leg the first two sections
claimed every fixture chunk and Financial/Proposal/Prayer drafted EMPTY. A section whose
every hit is already cap-consumed now force-adopts its top hit (one bounded exception).
Live prayer-paragraph thinness is the same failure shape.

**F2 — money canonicalization**: `money_key()` = numeric value + magnitude word
("Rs.10.00 million" ≡ "Rs. 10 million"; OCR "Rs . 10,000" ≡ "Rs. 10,000"). Used for
officer-figure equality and a figures-ONLY card-backing path; a test pins that provision
numbers ("Article 27") can never pass via money backing ("27 million").

Fixture cites re-mapped to post-hybrid assignments (financial/proposal → [CITE:1])
via the temporary dump test (removed). Branch pushed to origin mid-batch per user request.

## Outcome

- ✅ Impact: retrieval now discriminates on subject vocabulary; sections can't draft
  empty from cap exhaustion; formatting-variant figure flags eliminated.
- 🧪 Tests: lib 46/46 (lexical_query, rrf_merge ×2, money_key, figure-variant allowed,
  provision-via-money blocked), acceptance 3/3.
- 📁 Files: retrieval.rs, research.rs, verify.rs, 2 fixtures, CHANGELOG_DEV.md.
- 🔁 Next prompts: live summary re-run — if misattribution persists with relevant cards,
  narrow F's allowance to the adjacently-cited card (contract-shape decision).
- 🧠 Reflection: the tiny fixture corpus accidentally stress-tested cap exhaustion that
  production only showed as "thin paragraph" — keep it small.

## Evaluation notes (flywheel)

- Failure modes observed: hybrid leg starved later sections on the 6-chunk corpus
  (anti-starvation added); fixture cites broke twice on assignment shifts (known cost of
  global marker ids).
- Graders run and results (PASS/FAIL): lib + acceptance PASS.
- Prompt variant (if applicable): n/a
- Next experiment (smallest change to try): live run; inspect RRF-pick logs vs draft.
