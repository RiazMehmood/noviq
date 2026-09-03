---
id: 0004
title: vLLM inference engine ADR
stage: misc
date: 2026-06-28
surface: agent
model: claude-opus-4-8
feature: 016-vllm-serving
branch: 016-vllm-serving
user: Riaz Mehmood
command: /sp.adr
labels: [adr, vllm, inference, constitution-divergence]
links:
  spec: specs/016-vllm-serving/spec.md
  ticket: null
  adr: history/adr/0018-server-side-inference-engine-vllm-continuous-batching-sidecar.md
  pr: null
files:
  - history/adr/0018-server-side-inference-engine-vllm-continuous-batching-sidecar.md
tests:
  - none (documentation artifact)
---

## Prompt

start /sp.adr

(invoked as: `/sp.adr vLLM continuous-batching sidecar replaces single GGUF ModelSlot for concurrent inference serving`)

## Response snapshot

Ran the ADR workflow against feature 016 planning artifacts (plan.md, research.md,
data-model.md, contracts/). Extracted **one** architecturally significant decision cluster —
the server-side inference engine swap to vLLM — per clustering rules (the sub-decisions change
together: continuous-batching sidecar topology, Qwen3-8B AWQ/GPTQ 4-bit + FP8 KV resident
model, and the `TextGenerator` trait seam with llama.cpp fallback + `NOVIQ_INFER_ENGINE`
switch). Resisted over-granular splitting into separate engine/format/seam ADRs.

Checked existing ADRs (0001–0017): no vLLM coverage; flagged that this **supersedes ADR-0001
(single-slot hosting) for the server only** and mirrors the ADR-0014/0015 Postgres server
divergence precedent.

Created **ADR-0018** (Status: Accepted) with full Context, clustered Decision, Positive/Negative
Consequences, six Alternatives Considered (keep llama.cpp gate, custom batching, TGI/SGLang,
in-process PyO3, old cu118 vLLM, R535 driver upgrade), and References. Recorded the two
constitution divergences (Principle III + Tech-Stack Invariant), both server-only, with the
constitution amendment due at cutover.

## Outcome

- ✅ Impact: Permanent, reviewable record of the inference-engine decision + its constitution divergence; unblocks /sp.tasks.
- 🧪 Tests: N/A (documentation).
- 📁 Files: history/adr/0018-server-side-inference-engine-vllm-continuous-batching-sidecar.md
- 🔁 Next prompts: /sp.tasks (Spike 0 already GREEN, no longer a provisional gate); constitution amendment at cutover.
- 🧠 Reflection: Spike 0 + serve smoke (2026-06-28) made this ADR "Accepted" rather than "Proposed" — the CUDA risk that would have kept it tentative is retired.

## Evaluation notes (flywheel)

- Failure modes observed: none — single well-bounded cluster; avoided over-granular ADRs and the missing-alternatives trap.
- Graders run and results (PASS/FAIL): Clustered-not-atomic = PASS; ≥1 alternative with rationale = PASS (6 listed); pros+cons for chosen+alternatives = PASS; concise-but-sufficient = PASS.
- Prompt variant (if applicable): n/a
- Next experiment (smallest change to try): at cutover, draft the constitution amendment text referencing ADR-0018 so the divergence is formally ratified.
