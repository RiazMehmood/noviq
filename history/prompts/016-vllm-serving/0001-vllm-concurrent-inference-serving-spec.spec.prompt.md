---
id: 0001
title: vLLM concurrent inference serving spec
stage: spec
date: 2026-06-27
surface: agent
model: claude-opus-4-8
feature: 016-vllm-serving
branch: 016-vllm-serving
user: Riaz Mehmood
command: /sp.specify
labels: [vllm, inference, concurrency, gpu, serving]
links:
  spec: specs/016-vllm-serving/spec.md
  ticket:
  adr:
  pr:
files:
  - specs/016-vllm-serving/spec.md
  - specs/016-vllm-serving/checklists/requirements.md
tests:
  - none (spec stage)
---

## Prompt

ok let us write specs and plan for this.

(Continuation of a session that: shipped the Qwen3 no-think switch and made Qwen3-8B
the production chat model, then explained vLLM / AWQ / GPTQ / FP8 KV cache in plain
language. The user chose "option 2" — the vLLM / 10-concurrent-user deep-dive — as the
next work. Feature description seeded into /sp.specify captured: replace the single
serialized llama-cpp generation path with a vLLM continuous-batching engine to meet the
10-concurrent-user hard requirement on the 12 GB A40-12Q, serving Qwen3-8B in AWQ/GPTQ
with FP8 KV cache, keeping retrieval/grounding/citations unchanged; server-only;
CUDA-11.4 compatibility flagged as the top risk.)

## Response snapshot

Created branch `016-vllm-serving` and wrote `spec.md` + quality checklist.

- 3 prioritized user stories: (P1) 10 officers ask at once → all get grounded cited
  answers within budget; (P2) streaming + grounding preserved under load; (P3) operator
  deploy/observe/recover the engine.
- 17 functional requirements (continuous batching, ≥10 concurrent, pipeline unchanged
  around generation, streaming, offline, loopback-only engine, 12 GB budget, quantized
  Qwen3-8B no-retrain, context cap, graceful unavailability, auto-start/deploy,
  health/observability, no-quality-regression gate, scale-out-not-precluded).
- 8 measurable success criteria (100% of 10 concurrent served, p95 ≤ 10 s, ≥2.5×
  throughput, 0 OOM in soak, no quality regression, streaming, fully offline, hands-free
  recovery). Plus Assumptions, Dependencies & Risks (CUDA/driver = HIGH), Non-Goals.
- Surfaced 3 clarifications; user resolved them: **Q1** evaluate both AWQ + GPTQ and
  launch the winner (FR-009); **Q2** p95 ≤ 10 s, context cap ≥ ~6k tuned in planning
  (FR-010, FR-016, SC-002); **Q3** keep llama.cpp as a config-switchable fallback
  (FR-017). Checklist now fully green.

## Outcome

- ✅ Impact: spec ready for `/sp.plan`; the highest-risk unknown (CUDA 11.4 vs vLLM) is
  explicitly elevated to a planning/research blocker.
- 🧪 Tests: none (spec stage).
- 📁 Files: spec.md, checklists/requirements.md.
- 🔁 Next prompts: `/sp.plan` (architecture: vLLM sidecar vs managed, CUDA strategy,
  AWQ/GPTQ eval harness, Rust client seam over OpenAI-compatible API, FP8 KV + context
  cap VRAM budget, fallback switch, load-test plan).
- 🧠 Reflection: this is an architecturally significant decision (engine swap, new
  runtime, data-path latency) → ADR likely warranted at plan time.

## Evaluation notes (flywheel)

- Failure modes observed: create-phr.sh did not write a file for this feature arg; wrote
  the PHR agent-natively instead (per CLAUDE.md fallback).
- Graders run and results (PASS/FAIL): spec quality checklist PASS (all items).
- Prompt variant (if applicable): n/a
- Next experiment (smallest change to try): in `/sp.plan`, settle CUDA-11.4 vs vLLM
  first — it gates everything else.
