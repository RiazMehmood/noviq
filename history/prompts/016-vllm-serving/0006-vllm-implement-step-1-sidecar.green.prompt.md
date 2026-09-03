---
id: 0006
title: vLLM implement step 1 sidecar
stage: green
date: 2026-06-28
surface: agent
model: claude-opus-4-8
feature: 016-vllm-serving
branch: 016-vllm-serving
user: Riaz Mehmood
command: /sp.implement
labels: [implement, vllm, sidecar, provisioning, tooling]
links:
  spec: specs/016-vllm-serving/spec.md
  ticket: null
  adr: history/adr/0018-server-side-inference-engine-vllm-continuous-batching-sidecar.md
  pr: null
files:
  - deploy/vllm/setup-vllm.sh
  - deploy/vllm/run-vllm.sh
  - deploy/vllm/noviq-vllm.service
  - tools/vllm_eval/load_test.sh
  - specs/016-vllm-serving/tasks.md
  - CHANGELOG_DEV.md
tests:
  - bash -n clean on all three shell scripts (systemd unit verifies on the VM)
---

## Prompt

start /sp.implement for 016

## Response snapshot

Ran /sp.implement. Checklist gate: requirements.md 15/15 → PASS. Read tasks.md + plan +
contracts + data-model + quickstart, and the existing seam points (api/query.rs gen_gate +
run_turn, api/auth.rs login = bearer token, api/router.rs routes, deploy conventions).

Executed the **no-Rust-compile** tasks (verifiable on the Windows dev box) as the first
increment — provisioning + tooling that encode the Spike-0 learnings:
- T010 deploy/vllm/setup-vllm.sh (uv py3.10 venv, vllm==0.8.5 + pinned transformers 4.51.3 +
  setuptools, local cuda-compat-12-4 extract, downloads AWQ + GPTQ).
- T011 deploy/vllm/run-vllm.sh (vllm serve from .env NOVIQ_VLLM_*, loopback, fp8 KV, compat
  LD_LIBRARY_PATH, HF_HUB_OFFLINE).
- T023 deploy/vllm/noviq-vllm.service (loopback sidecar unit).
- T016 tools/vllm_eval/load_test.sh (N-concurrent /api/query → p50/p95/throughput/failures/
  peak GPU mem; engine-agnostic for the SC-003 baseline-vs-vLLM gate).
- T002 dirs. Marked [X] in tasks.md. bash -n clean. Committed aebaf709.

**Stopped at a checkpoint** before the Rust TextGenerator seam (T003–T009 foundational +
T012–T014): it threads through gen_gate/run_turn/assistant/llm.rs and **needs the
llama-cpp/CUDA toolchain to compile-verify, which builds on the VM, not this Windows box** —
a real approach decision for the architect.

## Outcome

- ✅ Impact: vLLM sidecar can be stood up on the VM from these artifacts; load harness ready.
- 🧪 Tests: bash -n clean; provisioning + load run pending VM + maintenance window.
- 📁 Files: deploy/vllm/{setup-vllm.sh,run-vllm.sh,noviq-vllm.service}, tools/vllm_eval/load_test.sh.
- 🔁 Next prompts: decide Rust-seam build/verify approach (write-local-verify-on-VM vs drive-on-VM vs provision-VM-first), then implement T003–T014.
- 🧠 Reflection: split the feature by "compile-needed" vs not — shipped the verifiable half now, gated the rest on the toolchain decision.

## Evaluation notes (flywheel)

- Failure modes observed: none — artifacts syntax-clean; tasks marked.
- Graders run and results (PASS/FAIL): checklist gate PASS (15/15); bash -n PASS (3/3 scripts).
- Prompt variant (if applicable): n/a
- Next experiment (smallest change to try): a minimal `cargo check --no-default-features --features server` on the VM to confirm the seam compiles incrementally as it's written.
