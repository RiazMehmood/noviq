---
id: 0005
title: ADR decomposition gating
stage: misc
date: 2026-06-08
surface: agent
model: claude-opus-4-8
feature: 009-lgd-drafting
branch: 009-lgd-drafting
user: Riaz Mehmood
command: /sp.adr
labels: [adr, 008, gating, hardware-adaptive]
links:
  spec: specs/008-query-decomposition/spec.md
  ticket:
  adr: history/adr/0009-query-decomposition-gated-to-capacity-limited-generators.md
  pr:
files:
  - history/adr/0009-query-decomposition-gated-to-capacity-limited-generators.md
tests:
  - none (ADR)
---

## Prompt

ADR B of the 4-primary batch: document Feature 008's decision to gate query decomposition to capacity-limited generators (≤1.5B param size primary signal; HardwareTier fallback; ECHO_* override), dynamic per deployment, not always-on.

## Response snapshot

Created ADR-0009 "Query Decomposition Gated to Capacity-Limited Generators" (Accepted). Decision: enable decomposition by active GGUF param size (≤1.5B on, 3B+ off), HardwareTier fallback when size unknown, operator env override; encoded in `decomposition_enabled`. Pros (binary adapts, no overhead on capable models, honors VII/II, testable override), cons (threshold heuristic, two code paths, capacity is a proxy, weaker fallback signal). 4 alternatives (always-on, never, compile-time flag, hardware-as-primary) rejected with rationale.

## Outcome

- ✅ Impact: Documents the deferred gating decision flagged in memory; ADR archive now covers 008's runtime behavior.
- 🧪 Tests: none (ADR)
- 📁 Files: history/adr/0009-query-decomposition-gated-to-capacity-limited-generators.md
- 🔁 Next prompts: ADR C (006), ADR D (005).
- 🧠 Reflection: Param size is the more direct proxy than hardware tier for the capability that drives multi-fact collapse.

## Evaluation notes (flywheel)

- Failure modes observed: none.
- Graders run and results (PASS/FAIL): ADR significance checklist — PASS.
- Prompt variant (if applicable): n/a
- Next experiment (smallest change to try): proceed to ADR C.
