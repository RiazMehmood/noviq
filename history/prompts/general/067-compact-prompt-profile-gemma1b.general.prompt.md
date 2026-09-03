---
id: 067
title: Compact prompt profile for Gemma 1B
stage: general
date: 2026-07-06
surface: agent
model: claude-fable-5
feature: none
branch: release (via compact-prompt-profile)
user: Riaz Mehmood
command: conversation
labels: [prompt-engineering, gemma, small-models, grounding, vllm]
links:
  spec: none
  ticket: none
  adr: none
  pr: none
files:
  - src-tauri/src/services/llm.rs
  - src-tauri/src/services/assistant/capabilities/guidance.rs
  - src-tauri/src/services/query_router.rs
  - CHANGELOG_DEV.md
tests:
  - services::query_router::tests::compact_prompt_keeps_refusal_sentence_and_demo_trigger
  - prompt filter 15/15, guidance filter 11/11 on VM
---

## Prompt

write another prompt for gemma 1B at which it can give correct answers, brief current prompt into
acceptable prompt for gemma 1B, first share prompt with me then as i allow to integrate then
integrate it.

ok integrate it

## Response snapshot

Iterated the prompt live against the prod vLLM sidecar (3 versions): v1 compact rules alone still
over-refused; v2 + literal `[n]` + demo fixed refusals but the one-line demo anchored one-sentence
answers; v3 with a worked demo (cited condition list + refine offer) produced structured cited
answers. Shared v3 with test evidence; user approved; integrated as env-gated
`NOVIQ_PROMPT_PROFILE=compact`: `GROUNDING_SYSTEM_PROMPT_COMPACT` (v5.5.0-compact, versioned +
audit-logged), guidance::system_prompt absorbs persona+refine-offer under the profile, richer
citation demo in query_router. Canonical refusal sentence kept byte-identical to
INSUFFICIENT_INFO_MSG; literal `[n]` kept as demo-injection trigger (both under unit test).
Deployed `242699a2`; verified on prod /api/query: both previously-refusing SPPRA queries now
answer with citations; out-of-corpus query still refuses.

## Outcome

- ✅ Impact: Gemma-1B refusal failure fixed in prod via ~140-token prompt profile; default profile
  byte-identical for all other deployments.
- 🧪 Tests: VM cargo check + prompt/guidance/inference test filters green (26 tests); new
  invariant test for refusal sentence + `[n]` trigger.
- 📁 Files: llm.rs, guidance.rs, query_router.rs, CHANGELOG_DEV.md.
- 🔁 Next prompts: 30-question A/B vs 8B baseline; decide whether residual 1B comprehension
  errors (invented labels, partial lists) are acceptable or revert to Qwen3-8B.
- 🧠 Reflection: the demo shape mattered more than the rules — few-shot anchoring dominated 1B
  behavior; iterate prompts against the live engine before integrating.

## Evaluation notes (flywheel)

- Failure modes observed: refusal-rule attractor (up-front refusal instruction), few-shot length
  anchoring, citation-index misattribution (persists — model ceiling).
- Graders run and results (PASS/FAIL): prod smoke 3/3 behavioral PASS (2 answers + 1 correct
  refusal); factual precision still imperfect (invented method label in Q2).
- Prompt variant (if applicable): v5.5.0-compact.
- Next experiment (smallest change to try): demo with two context passages to teach per-index
  attribution ([1] vs [2]).
