---
id: 0003
title: agentic-deep-search-adr
stage: misc
date: 2026-07-13
surface: agent
model: claude-opus-4-8
feature: 025-agentic-deep-search
branch: 025-agentic-deep-search
user: Riaz Mehmood
command: /sp.adr agentic-multipass-deep-search
labels: [deep-search, agentic-rag, adr, architecture]
links:
  spec: specs/025-agentic-deep-search/spec.md
  ticket:
  adr: history/adr/0021-bounded-agentic-multi-pass-deep-retrieval.md
  pr:
files:
  - history/adr/0021-bounded-agentic-multi-pass-deep-retrieval.md
  - specs/025-agentic-deep-search/plan.md
tests:
  - none (ADR documentation)
---

## Prompt

/sp.adr agentic-multipass-deep-search — analyze the feature 025 planning artifacts and document the
architecturally significant decision(s) as ADR(s), clustered (not atomic), with alternatives + tradeoffs.

## Response snapshot

Scanned 20 existing ADRs (0001–0020). Nearest neighbors — 0009 (query decomposition), 0013 (one-ask
composition), 0019 (structured synthesis), 0020 (graph retrieval), 0018 (vLLM) — touch adjacent pieces but
none cover the multi-pass agentic retrieval decision. Extracted ONE clustered decision (passes all three
significance tests: structural impact, real alternatives, will be revisited).

Created **ADR-0021 "Bounded Agentic Multi-Pass Deep Retrieval"** (Status: Proposed). The cluster: the
≤3-LLM-call / ≤2-retrieval-round loop; facet-expansion decompose (keep original leg); coverage-check as the
routing gate (the new component; emits ids+status only → can't fabricate; biases to re-fetch); reuse boundary
(query_router hybrid path via extracted `retrieve_for_query`, NOT drafting-coupled `one_ask::research`;
one_ask verify primitives + pack_windows); hard grounding gate; stateless + task-local concurrency; flag-gated
(`ECHO_AGENTIC_DEEP`) A/B hard-cutover vs legacy deep.

Documented positive + negative consequences (incl. the ~65 s latency deviation as an accepted negative) and
five alternatives with rejection rationale (bigger window/GPU; single-pass+wider pool; unbounded agent;
map-reduce; reuse one_ask wholesale). Linked related ADRs 0009/0013/0018/0019/0020. Backlinked the ADR in
plan.md.

Grader checklist: PASS — decision is clustered (not atomic); ≥1 alternative with rationale (five given); pros
AND cons for chosen + alternatives; concise but sufficient.

## Outcome

- ✅ Impact: Permanent record of the multi-pass-vs-single-pass-vs-bigger-window decision; unblocks /sp.tasks.
- 🧪 Tests: none (documentation).
- 📁 Files: history/adr/0021-bounded-agentic-multi-pass-deep-retrieval.md; plan.md (ADR backlink).
- 🔁 Next prompts: `/sp.tasks` to generate the dependency-ordered task list. Flip ADR status Proposed→Accepted
  once the architect signs off.
- 🧠 Reflection: Clustering kept this to ONE ADR instead of fragmenting into decompose/coverage/verify/flag
  ADRs — they change together and belong to one decision.

## Evaluation notes (flywheel)

- Failure modes checked: over-granular ADRs (avoided by clustering); missing alternatives (five listed).
- Graders run and results (PASS/FAIL): ADR significance + completeness checklist — PASS.
- Prompt variant (if applicable): n/a.
- Next experiment (smallest change to try): on /sp.tasks, front-load the coverage-check prototype task so its
  latency/accuracy is measured before the full loop is wired (de-risks the one novel component).
