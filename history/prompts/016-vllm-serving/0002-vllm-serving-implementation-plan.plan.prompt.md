---
id: 0002
title: vLLM serving implementation plan
stage: plan
date: 2026-06-27
surface: agent
model: claude-opus-4-8
feature: 016-vllm-serving
branch: 016-vllm-serving
user: Riaz Mehmood
command: /sp.plan
labels: [vllm, inference, concurrency, cuda, plan]
links:
  spec: specs/016-vllm-serving/spec.md
  ticket:
  adr:
  pr:
files:
  - specs/016-vllm-serving/plan.md
  - specs/016-vllm-serving/research.md
  - specs/016-vllm-serving/data-model.md
  - specs/016-vllm-serving/contracts/text-generator-trait.md
  - specs/016-vllm-serving/contracts/vllm-openai-api.md
  - specs/016-vllm-serving/quickstart.md
tests:
  - none (plan stage); Spike 0 go/no-go + A/B + load test defined for implementation
---

## Prompt

ok let us write specs and plan for this. → (then) Plan feature 016-vllm-serving … [honor
AWQ+GPTQ eval, p95 ≤10s, keep llama.cpp fallback; resolve CUDA/driver, process arch, Rust
seam, VRAM budget, A/B harness, load test; flag Principle-III divergence + ADR].

## Response snapshot

Generated full plan set on branch `016-vllm-serving`. Web research nailed the gating risk:

- **CUDA blocker (R1):** Qwen3 needs vLLM ≥ 0.8.4, which dropped CUDA 11.x; last cu118 wheel
  (0.6.1) predates Qwen3 → no prebuilt wheel runs Qwen3 on driver 470. **Path:** CUDA
  forward-compat `cuda-compat-12-x` on the R470 *datacenter* driver (A40 datacenter, R470 ≥
  R450 → in scope) to run a CUDA-12.x vLLM with **no guest driver change** — proven by a
  **Spike 0 go/no-go** before any code; fallback = IBA-IT host vGPU/driver upgrade to R535.
- **Models (R2):** Qwen/Qwen3-8B-AWQ + RedHatAI/Qwen3-8B-quantized.w4a16 both vLLM-loadable.
- **FP8 KV on Ampere (R4):** storage-mode only (no native FP8 compute on sm_86) → still
  halves KV memory (the win); VRAM budget ~9.3 GB at 10 users fits 12 GB.
- **Arch (R3):** vLLM as a loopback systemd **sidecar** (OpenAI API), provisioned out-of-tree
  like F006 sidecars; deploy poll-timer gains an "ensure active" check, not a rebuild.
- **Seam (R6):** new `TextGenerator` trait, `VllmGenerator` (raw `/v1/completions` + SSE→
  `chat://`) vs `LlamaCppGenerator` (fallback); env switch `NOVIQ_INFER_ENGINE`. query_router
  unchanged.

Artifacts: plan.md (Constitution Check = PASS with 2 declared **server-only** divergences —
Principle III + Tech-Stack Invariant — justified in Complexity Tracking, ADR + amendment at
cutover), research.md, data-model.md (no DB schema change; env config + runtime entities),
contracts/ (trait + vLLM API), quickstart.md (Spike-0-first runbook w/ rollback). Phasing:
Spike 0 → sidecar → Rust seam → A/B gate → load test → cutover.

## Outcome

- ✅ Impact: implementation-ready plan; the make-or-break unknown (CUDA on vGPU) is isolated
  as a no-code Spike 0 gate, so we fail fast before investing in the Rust seam.
- 🧪 Tests: defined (Spike 0, A/B no-regression gate FR-014, load test SC-001..004).
- 📁 Files: plan + research + data-model + 2 contracts + quickstart; agent context updated.
- 🔁 Next prompts: `/sp.adr` (server-side inference-engine divergence), then `/sp.tasks`.
- 🧠 Reflection: this is the archetypal ADR case (framework + platform + cross-cutting); the
  divergence mirrors the existing Postgres-vs-SQLite server exception.

## Evaluation notes (flywheel)

- Failure modes observed: none in planning; main external dependency (IBA-IT driver upgrade)
  is explicitly the fallback, not the default path.
- Graders run and results (PASS/FAIL): Constitution Check PASS-with-justified-divergence.
- Prompt variant (if applicable): n/a
- Next experiment (smallest change to try): execute Spike 0 on the VM — it gates all else.
