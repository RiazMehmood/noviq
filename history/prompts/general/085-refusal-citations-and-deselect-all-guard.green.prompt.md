---
id: 085
title: No-citation refusals + refuse on explicit deselect-all
stage: green
date: 2026-07-11
surface: agent
model: claude-opus-4-8
feature: scope-refusal-citations
branch: release
user: engr.riaz2010@gmail.com
command: (bug reports from live testing)
labels: [chat, citations, scope, guidance, frontend]
links:
  spec:
  ticket:
  adr:
  pr:
files:
  - src-tauri/src/services/assistant/capabilities/guidance.rs
  - src/pages/ChatPage.tsx
  - CHANGELOG_DEV.md
tests:
  - "cargo check --no-default-features --features llm,server --bin noviq-server: PASS"
  - "cargo test guidance:: : 7 passed / 0 failed (incl. 2 new refusal tests)"
  - "npm run build: PASS (built in 2.47s)"
---

## Prompt

deploy complete, no flag shown. Found bugs in last two queries: (1) deselecting all
documents still answers from the corpus — should flag "no document selected, please
select or attach one"; (2) department count from Rules of Business shows 24 not 39;
(3) selecting a doc and asking about another cleanly refuses (good) BUT still shows
citations — a definite refusal should have no citations.

## Response snapshot

Diagnosed three distinct issues:
- (1) `ChatPage.effectiveSourceIds()` returned undefined for an empty selection and
  `sendMessage` drops empty arrays → backend saw "no restriction" → corpus. Fixed with a
  composer guard: explicit deselect-all (no @-mention/attachment) refuses locally.
- (3) citations are built from retrieval before generation and passed straight through;
  a refusal carried them. Fixed in `guidance.rs build_response` via `is_bare_refusal`
  (no `[n]` AND not-in-source language → clear citations); precise so a negation that
  still cites is untouched; gate still grounds on all retrieved passages.
- (2) NOT fixed — "department" is not a countable structural/corpus unit, so it never
  hits the deterministic count path and falls to the LLM over fragmented RoB Schedule-I
  table chunks. This is the blocked Track-3 department-tree work (parse-time table
  chunking + structured extraction), documented in the RoB fragmentation memory.

VM-verified (cargo check clean, 7 guidance tests, npm build OK), merged to `release`
(`f59bcaf9`), deployed. Memory updated; this PHR written.

## Outcome

- ✅ Impact: refusals no longer show stray citations; deselect-all no longer silently
  answers the whole corpus.
- 🧪 Tests: cargo check + 7 guidance tests + frontend build all pass.
- 📁 Files: guidance.rs (refusal detection), ChatPage.tsx (scope guard).
- 🔁 Next prompts: Track 2 synthesis; Track 3 (incl. RoB table chunking → real
  department count).
- 🧠 Reflection: the first `[n]`-only citation filter was too aggressive (broke valid
  prose citations by name); requiring BOTH no-`[n]` and refusal language made it precise.

## Evaluation notes (flywheel)

- Failure modes observed: strict `[n]` citation filter would hide legitimate citations
  that reference provisions by name; caught by existing guidance unit tests before deploy.
- Graders run and results (PASS/FAIL): cargo check PASS; 7 guidance tests PASS; FE PASS.
- Prompt variant (if applicable): n/a
- Next experiment (smallest change to try): live-test a cross-document refusal and a
  deselect-all query on the deployed build.
