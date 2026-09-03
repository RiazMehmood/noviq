---
id: 0010
title: Draft quality G I and H diagnosis
stage: green
date: 2026-06-13
surface: agent
model: claude-fable-5
feature: 011-postgres-pgvector-port
branch: 011-postgres-pgvector-port
user: Riaz Mehmood
command: yes proceed with G + I and run the H diagnosis
labels: [010-one-ask-drafting, draft-quality, unit-path-labels, relevance-diagnosis]
links:
  spec: specs/010-one-ask-drafting/spec.md
  ticket: null
  adr: null
  pr: null
files:
  - src-tauri/src/services/drafting/one_ask/research.rs
  - src-tauri/src/services/drafting/one_ask/verify.rs
  - src-tauri/src/services/drafting/one_ask/compose.rs
  - CHANGELOG_DEV.md
tests:
  - cargo test --lib drafting::one_ask (40/40, 5 new)
  - cargo test --test one_ask_acceptance (3/3)
---

## Prompt

yes proceed with G + I and run the H diagnosis

## Response snapshot

**G**: `card_label(unit_label, unit_path, content)` — DB showed `unit_path` fully populated
(`Rule_27/(5)/(i)/p2`), so bare "(…)" labels are qualified from the path (→ "Rule 27(5)(i)");
noun leaves ("Section 2.10") stand alone; `pN` page-marker segments dropped; no named
ancestor → content-prefix fallback → bare label. **I**: cap-cut snippets back off stranded
connectives and append a display "…" (`quote_verifies` strips it); `parse_markers` scrubs
malformed marker shapes from prose. **H**: per-candidate KNN distance logging in research
(no behavior change). Commit `c0b60448`; lib 40/40, acceptance 3/3.

Next live run confirmed G+I working (labels "Rule 27(5)(i)"/"Rule 4(1)"/"Part II(1)(i)",
ellipses on cuts, hallucinated [FIG:14] flagged, flags 7→5). **H diagnosis from the run's
logs: every one of 30 candidates KEPT — distances cluster 0.2475–0.3100 across all five
sections, and irrelevant cards INTERLEAVE with relevant ones (forest-bond clause at 0.2696
beats relevant Independent-Project-Director at 0.2764). The relative margin is the wrong
tool: embedding distance does not encode the relevance distinction at this corpus size.**

## Outcome

- ✅ Impact: labels carry authority and feed F's echo-matching; quotes cut cleanly;
  malformed markers never ship; relevance problem precisely characterized with live data.
- 🧪 Tests: 40/40 lib (5 new), 3/3 acceptance.
- 📁 Files: research.rs, verify.rs, compose.rs, CHANGELOG_DEV.md.
- 🔁 Next prompts: H decision — recommended hybrid lexical+vector (existing `chunks.tsv`
  GIN + RRF pattern) over margin tuning; also F2 candidate: money-figure normalization
  ("Rs. 10 million" vs "Rs.10.00 million" echo variants still flag).
- 🧠 Reflection: instrument before tuning — the margin everyone would have "tightened"
  never fired once; the data killed that whole approach in one run.

## Evaluation notes (flywheel)

- Failure modes observed: verbatim-backed misattribution persists (writer stitches wrong
  claims around irrelevant cards) — gating is mechanical, error is semantic.
- Graders run and results (PASS/FAIL): lib + acceptance PASS; live gate = visual diff vs
  baseline (improved, relevance still blocking).
- Prompt variant (if applicable): n/a
- Next experiment (smallest change to try): hybrid tsv+KNN RRF in one_ask research.
