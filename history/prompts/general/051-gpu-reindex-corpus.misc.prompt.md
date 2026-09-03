---
id: 051
title: GPU re-index of the 11-doc corpus with the chunking fixes (proven runbook)
stage: misc
date: 2026-07-01
surface: agent
model: claude-opus-4-8
feature: general
branch: release
user: noviq.ai.pk@gmail.com
command: (freeform) "go for indexing via gpu ... from admin it must stop the server and flag the users indexing in progress"
labels: [gpu, embeddings, onnxruntime, cuda, reindex, ops, vm]
links:
  spec: docs/chunking-audit-2026-06-29.md
  ticket:
  adr:
  pr:
files: []
tests: []
---

## Prompt

now go for indexing via gpu issue then we come back remaining two chunk issues
... set it like that while it runs indexing via gpu from admin it must stop the
server and flag the users that indexing is in progress wait a while
... now save progress upto now and close all things we will start working from here next

## Response snapshot

Ran the GPU-embed re-index of all 11 docs on the live VM (no code change — ops +
the already-shipped chunk fixes). Diagnosed the two blockers that made GPU embed
"not work" previously:
- spike GPU `libonnxruntime.so.1.20.1` base lib needs only GLIBC 2.27 (the
  2026-06-30 panic was a DIFFERENT .so needing 2.34);
- `providers_cuda` needs `libcublasLt.so.12` on `LD_LIBRARY_PATH` (spike venv
  `nvidia/cublas/lib`) or the CUDA EP silently falls back to CPU.

Runbook executed (saved as `/tmp/gpu_setup.sh` + `/tmp/gpu_teardown.sh`): stop
deploy-timer + vLLM (free VRAM), `touch DEPLOYING` (user banner via /api/health
status), `.env` ORT→GPU + systemd drop-in `LD_LIBRARY_PATH`, reindex folder
`91217974-…` via `retry_folder_source`, then teardown reverses everything and
restarts vLLM. Verified `Successfully registered CUDAExecutionProvider`;
constitution 2761 chunks embedded in 9.2s (vs ~230s CPU).

Live validation after re-index (10,019 chunks): maxlen 42,866→6,467 (oversize
CLOSED), amendment_events 23→107 (RC-F), APT unresolved rows 58%→12% (RC-E);
Delegation rows still ~89% unresolved (tables orphaned from named provisions —
deeper structural fix owed). Production fully restored (server+vLLM+timer active,
health ok, banner cleared).

## Outcome

- ✅ Impact: corpus now carries all chunking fixes on GPU-computed embeddings;
  oversize/RC-F/RC-E(APT) confirmed on live data; GPU-embed runbook proven+recorded.
- 🧪 Tests: n/a (ops); live DB queries used as validation.
- 📁 Files: none (VM ops); code was 56c5fe7c + ab2b67c1 from earlier.
- 🔁 Next prompts: RC-E Delegation (structural table→provision attachment), RC-T
  (sidecar page-spanning header + merged cells), LG-Act page overshoot; optionally
  productize GPU-reindex as an admin button.
- 🧠 Reflection: bash `SUDO(){echo PW|sudo -S…}` wrapper clobbers piped stdin —
  write to /tmp then `sudo cp`. Saved to memory.

## Evaluation notes (flywheel)

- Failure modes observed: silent CPU fallback when a CUDA dependency lib is off
  LD_LIBRARY_PATH ("gpu=true" log reflects the flag, not actual EP registration).
- Graders run and results (PASS/FAIL): live-DB validation PASS for oversize/RC-F/RC-E(APT).
- Prompt variant: n/a
- Next experiment: measure Delegation parent_chunk_id chain to design the
  structural table attachment.
