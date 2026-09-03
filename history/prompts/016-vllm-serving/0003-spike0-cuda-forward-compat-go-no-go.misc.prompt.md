---
id: 0003
title: Spike 0 — CUDA-12 forward-compat go/no-go on the R470 vGPU
stage: misc
date: 2026-06-27
surface: agent
model: claude-opus-4-8
feature: 016-vllm-serving
branch: 016-vllm-serving
user: Riaz Mehmood
command: (free-form) "start working on 016 vllm serving work"
labels: [spike, vllm, cuda, gpu, infra]
links:
  spec: specs/016-vllm-serving/spec.md
  ticket:
  adr:
  pr:
files:
  - CHANGELOG_DEV.md
  - C:/Users/hp/.claude/projects/D--echo-AI/memory/project_echoai-headless-deployment.md
  - C:/Users/hp/.claude/projects/D--echo-AI/memory/MEMORY.md
  - (VM, out-of-tree) ~/vllm-spike/{venv310,venv38,compat-extract,launch.sh}
tests:
  - "torch cu121 GPU init: cuda.is_available()=True + real matmul on A40-12Q — PASS"
  - "vllm 0.8.5 import with compat libcuda: CUDA detected — PASS"
  - "vllm.get_tokenizer (fast+slow) with transformers 4.51.3 — PASS"
  - "vllm serve + generation smoke — NOT RUN (VPN dropped launch SSH; resume tomorrow)"
---

## Prompt

start working on 016 vllm serving work

(Follow-ups during the session: chose "Run Spike 0 now" when asked how to start;
then "save progress upto now we will test tomorrow with fresh connection" and
"also stop any running commands or terminals".)

## Response snapshot

Read CHANGELOG_DEV.md + the 016 artifacts (spec/plan/research/quickstart/contracts).
tasks.md and the ADR are not yet created. Identified Spike 0 (CUDA forward-compat
go/no-go) as the hard blocker per plan.md and executed it on the GPU VM
(<user>@<PRIVATE_IP>, A40-12Q, driver 470.223.02).

DECISIVE RESULT — forward-compat WORKS: extracted cuda-compat-12-4
(libcuda.so.550.90.12) locally via `dpkg-deb -x` (no sudo); with LD_LIBRARY_PATH to
it, torch 2.4.1+cu121 reported cuda.is_available()=True and ran a real GPU matmul on
the A40-12Q, and vllm 0.8.5 (torch 2.6+cu124) imports + detects CUDA. ⇒ No IBA-IT
driver upgrade needed; R535 fallback dropped.

Gotchas solved: system Python 3.8 too old → used uv for standalone CPython 3.10.20;
uv pulled transformers 5.12.1 which breaks vllm 0.8.5 tokenizer cache → pinned
transformers==4.51.3 (+ tokenizers 0.21.4); only ~5.8 GB VRAM free (prod uses ~6.3)
so the real 8B-AWQ fit test needs a prod-stop window.

Not finished: actual `vllm serve` + one generation — the Easy Connect VPN repeatedly
dropped the launch SSH (exit 255; command never executed, serve.log mtime frozen).
Stopped at user's request; spike workspace preserved at ~/vllm-spike/. Saved progress
to CHANGELOG_DEV.md + both memory files. VM left clean (no stray proc, GPU baseline).

## Outcome

- ✅ Impact: Largest project risk (CUDA-12 on R470 vGPU) is RETIRED — vLLM path is viable without a driver upgrade. Unblocks /sp.tasks + /sp.adr.
- 🧪 Tests: forward-compat torch + vllm-import + tokenizer = PASS; serve+gen smoke deferred (VPN).
- 📁 Files: changelog + 2 memory files updated; VM spike workspace ~/vllm-spike (out-of-tree). No app code changed.
- 🔁 Next prompts: finish serve smoke via ~/vllm-spike/launch.sh (poll :8009/health, curl /v1/completions); real 8B-AWQ fit test in a prod-stop window; then /sp.adr + /sp.tasks.
- 🧠 Reflection: Decouple risks — proved CUDA forward-compat cheaply on py3.8+torch before the heavier py3.10+vllm path. Local .deb extraction kept the spike non-mutating.

## Evaluation notes (flywheel)

- Failure modes observed: (1) inline python missing LD_LIBRARY_PATH → undefined symbol cuTensorMapEncodeTiled; (2) uv over-resolved transformers 5.x → vllm tokenizer AttributeError; (3) Easy Connect VPN intermittently kills SSH (255) on longer/launch commands.
- Graders run and results (PASS/FAIL): CUDA forward-compat PASS; vLLM import PASS; tokenizer PASS; end-to-end serve PENDING.
- Prompt variant (if applicable): n/a
- Next experiment (smallest change to try): on a fresh VPN connection, run `~/vllm-spike/launch.sh` and curl the 0.5B completion; if green, schedule the prod-stop 8B-AWQ fit test.
