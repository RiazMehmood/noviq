---
id: 0024
title: Issue-driven facet expansion (deep search, flag-gated)
stage: green
date: 2026-07-18
surface: agent
model: claude-opus-4-8
feature: 025-agentic-deep-search
branch: 025-agentic-deep-search
user: engr.riaz2010@gmail.com
command: build the deterministic version behind the flag
labels: [deep-search, facet-expansion, issue-spotting, retrieval, flag-gated]
links:
  spec: specs/025-agentic-deep-search/issue-driven-facet-expansion.design.md
  ticket:
  adr:
  pr:
files:
  - src-tauri/src/services/deep_search/mod.rs
  - src-tauri/src/services/deep_search/expand.rs
  - CHANGELOG_DEV.md
tests:
  - "deep_search::expand (lib): 6/6 (3 new)"
  - "cargo check --features llm,server --bin noviq-server: exit 0"
---

## Prompt

build the deterministic version behind the flag

(Follows the SLGA-2013 case-study audit: deep search fixed retrieval breadth but both normal and deep
missed conflict-of-interest + quorum because `expand_query` is item-driven. The user chose the
deterministic template approach over an LLM issue-spotter, behind a flag.)

## Response snapshot

Added a gated, deterministic issue-spotting stage to deep search's decomposer.

- `deep_search/mod.rs::issue_spotting_enabled()` — `ECHO_DEEP_ISSUE_SPOTTING`, default OFF,
  mirrors `agentic_enabled()`.
- `deep_search/expand.rs`:
  - `spot_issues(query, instrument)` — four trigger→template rules (quorum, conflict of interest,
    write-off/sanction limit, omnibus/Government sanction). Emits a sub-question ONLY when the
    trigger fact is present in the prompt (inverse of the padding bug → stays faithful). Cap
    `ISSUE_MAX=3`.
  - `detect_instrument(query)` — acronym+year ("SLGA 2013"), "<Name> Act/Ordinance/Rules…",
    Constitution, or a generic fallback, so templates read correctly.
  - Wired into `expand_query` after the item facets (flag-gated), deduped, within `MAX_FACETS`.
    Enlarged sub-question set flows unchanged through round-1 retrieval, coverage, synthesis.

Deliberately NOT done by loosening faithful decomposition (that reintroduces the off-domain padding
regression). Design doc: `specs/025-agentic-deep-search/issue-driven-facet-expansion.design.md`.

## Outcome

- ✅ Impact: with the flag on, deep now spins facets for latent legal issues implied by facts in the
  prompt — the case study yields quorum + conflict-of-interest sub-questions, whose governing rules
  the item-driven decomposition never retrieved. Default OFF = zero behaviour change.
- 🧪 Tests: `deep_search::expand` 6/6 (3 new: case-study spotting, no-trigger→empty, instrument
  detection); `cargo check --bin noviq-server` exit 0 (VM worktree at `e01d1ff3`, `--features
  llm,server`). Worktree restored clean.
- 📁 Files: mod.rs (flag), expand.rs (spot_issues + detect_instrument + wiring + tests), CHANGELOG.
- 🔁 Next prompts: flip `ECHO_DEEP_ISSUE_SPOTTING=1` in a VM worktree, run the case study through
  deep, confirm the trace's sub-question list gains the quorum + conflict facets and synthesis
  addresses both; A/B completeness vs the current deep run before defaulting on.
- 🧠 Reflection: the discipline that made this safe was NOT relaxing the anti-padding fix — issues are
  added only with a present trigger fact, keeping decomposition faithful while covering implied issues.

## Evaluation notes (flywheel)

- Failure modes observed (pre-fix): item-driven decomposition omits implied issues (quorum, conflict
  of interest) buried in prompt facts.
- Graders run and results (PASS/FAIL): unit PASS (6/6 expand). Live A/B pending (flag off in prod).
- Prompt variant (if applicable): deterministic templates (chosen) vs LLM issue-spotter (deferred).
- Next experiment (smallest change to try): grow the trigger→template table from real misses, the same
  evidence-driven way the enumeration keyword list grew; consider an LLM issue-spotter only if templates
  prove too rigid.
