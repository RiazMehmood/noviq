---
id: 0005
title: Fix A + Fix B — multi-article detection and deterministic title-matched provision number
stage: green
date: 2026-06-07
surface: agent
model: claude-opus-4-8
feature: 008-query-decomposition
branch: 008-query-decomposition
user: Riaz Mehmood
command: (conversation)
labels: [tdd, decomposition, citation-grade, slots, fix-a, fix-b]
links:
  spec: specs/008-query-decomposition/spec.md
  ticket:
  adr:
  pr:
files:
  - src-tauri/src/services/intent_router/slots.rs
  - src-tauri/src/services/intent_router/decompose.rs
  - src-tauri/src/services/intent_router/mod.rs
  - CHANGELOG_DEV.md
tests:
  - "cargo test --lib intent_router:: → 40 passed"
---

## Prompt

(User, paraphrased across the session.) First create the V4 analysis doc, then discuss
Fix A & B in plain language before deciding. Mid-session the user added the primary product
contract: the app must write copy-paste-ready summaries for directors/secretaries with proper
citations, rules and regulations — zero tolerance, 100% accurate. Then: "first apply fixes
then we will discuss more about summaries." For Fix B chunk selection the user chose
"title-match + refuse".

## Response snapshot

- Wrote V4 analysis doc (pure eval record) at `…/V4/ANALYSIS_0.5B_V4_2026-06-07.md`.
- Captured the citation-grade accuracy contract as a durable memory; it promotes Fix B to the
  core citation-integrity layer and adds a refuse-or-flag gate.
- Fix A (TDD): `slots::extract` now iterates `captures_iter` so every separately-phrased
  "<head> N" provision is captured (Q9 → ["Article 23","Article 24"]); sibling bare-number
  scan retained, anchored on the first match. New test `two_separate_article_phrases_both_captured`.
- Fix B (TDD): `provision_label`, `resolve_provision_label` (title-match by content-token
  majority, NOT rank — correct chunk was rank 2 in V4), `authoritative_body`, `NUMBER_UNVERIFIED`,
  and `SubQuestion.number_topic`. Wired in `dispatch`: enumeration sub-answers take the
  deterministic label (number never from the model) or the refuse flag. 5 new tests.

## Outcome

- ✅ Impact: multi-fact questions naming 2+ separate provisions now decompose (Q9); enumeration
  provision numbers are read deterministically from the matched chunk, removing the model from
  the citation-number loop (the V4 wall).
- 🧪 Tests: 40/40 intent_router (34 → +1 Fix A, +5 Fix B). RED verified before each GREEN.
- 📁 Files: slots.rs, decompose.rs, mod.rs, CHANGELOG_DEV.md (two dated entries).
- 🔁 Next prompts: decide Section→Article corpus relabel vs head-noun-from-query; settle
  summary output shape (extractive vs fact-locked prose); rebuild + re-run V5 eval to confirm
  Q9/Q15 flip in-app.
- 🧠 Reflection: diagnosing the actual retrieval order (correct chunk not rank 1) changed Fix B
  from a one-line top-chunk read into a title-match + refuse design — diagnose before code paid off.

## Evaluation notes (flywheel)

- Failure modes observed: 0.5B confabulates citation numbers even with the correct labelled
  chunk in context (Q15 29/28/29 vs 31/32/39).
- Graders run and results (PASS/FAIL): unit suite PASS (40/40). In-app V5 eval NOT yet run.
- Prompt variant (if applicable): n/a (deterministic, non-LLM path).
- Next experiment (smallest change to try): rebuild app, re-ask Q9 + Q15, confirm deterministic
  numbers and the refuse path; then decide Section/Article head-noun rendering.
