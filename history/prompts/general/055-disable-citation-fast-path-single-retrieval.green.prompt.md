---
id: 055
title: Disable citation fast-path — single hybrid retrieval path
stage: green
date: 2026-07-03
surface: agent
model: claude-opus-4-8
feature: general
branch: release
user: Riaz Mehmood
command: (freeform) diagnose banner loop + SPPRA refusal, resolve
labels: [retrieval, query-router, sppra, citation, hybrid]
links:
  spec: null
  ticket: null
  adr: null
  pr: null
files:
  - src-tauri/src/services/query_router.rs
  - CHANGELOG_DEV.md
tests:
  - none (Rust builds on VM only; behaviour verified via live DB + trace)
---

## Prompt

wi reindexed sppra via browser, now two problems i m facing one is that after
some time and in a loop our app shows banner of updating, and other is i think
it did not properly indexed or what it is refusing the answer. so search the db
and resolve these two issues.
(+ follow-ups: "we are using hybrid union currently"; "we dont need citation
fast path disable this we only need a single path for all queries.")

## Response snapshot

Searched live PG on the GPU VM (echo DB, port 44539).

Banner: no server-side cause — /api/health returns ok in 6–8 ms, deploy poller
ran 610×/24h all "up to date" (never writes the DEPLOYING sentinel / never
rebuilds), server up 15h with no restarts since 18:02 the prior day. The banner
correlated with the reindex/deploy restart cluster (10:30–11:47 07-02) and is
not currently reproducing. Frontend banner is driven purely by health != ok or a
failed poll (transient during heavy load).

Refusal: SPPRA indexed correctly — Rule 16 = "Alternate Methods of Procurements",
`Part_II/Rule_16/(1)/(a)/(ii)` present, 607 chunks, all embedded, maxlen 1411
(nesting fix verified live; 209 tiny <120c junk chunks noted but not the cause).
Trace 3fb64c8d showed STEP 6 citation fast-path fired on bare "Rule 16",
bypassed vector+reranker+hybrid, and resolved to the Secretariat Manual's
"Rule 16 — Steno Typist" (label collides across 4 instruments; generic-word
pollution in source_affinity_tokens gave the wrong doc overlap>0 so the
scope-guard fallthrough never fired). LLM then correctly refused.

Fix: new gate `citation_fast_path_enabled()` (`ECHO_CITATION_FAST_PATH`, default
OFF); call site forces `citation_ids = None` so every query uses the single
hybrid-union path. CHANGELOG_DEV.md entry added.

## Outcome

- ✅ Impact: bare "Rule N" queries now route through hybrid union (cosine-first)
  which already answers general SPPRA questions with correct citations; no more
  cross-instrument label hijack.
- 🧪 Tests: none added; Rust compiles on VM only. Verified via live DB queries +
  retrieval trace.
- 📁 Files: query_router.rs (gate + call-site), CHANGELOG_DEV.md.
- 🔁 Next prompts: compile-verify on VM + deploy (push release) → re-ask "rule 16
  of sppra"; optionally enable ECHO_RERANK; header-noise leaf filter for the 209
  tiny SPPRA chunks.
- 🧠 Reflection: indexing was fine; the bug was a routing short-circuit. Trace
  logs (STEP 6) pinpointed it faster than DB spelunking alone.

## Evaluation notes (flywheel)

- Failure modes observed: unit_label collisions across instruments defeat
  affinity disambiguation when the query lacks strong discriminator tokens.
- Graders run and results (PASS/FAIL): n/a (manual).
- Prompt variant (if applicable): n/a.
- Next experiment (smallest change to try): after deploy, confirm SPPRA Rule 16
  answers; if reranker enabled, re-check no regression on Constitution citations.
