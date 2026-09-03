---
id: 156
title: Resume Sindh corpus load under GPU hold
stage: general
date: 2026-08-15
surface: agent
model: claude-opus-5
feature: none
branch: deploy-idx
user: Riaz Mehmood
command: (conversation)
labels: [ops, indexing, gpu, corpus]
links:
  spec: none
  ticket: none
  adr: none
  pr: none
files:
  - deploy/gpu-hold.sh (new, installed to /opt/noviq/gpu-hold.sh)
  - deploy/gpu-release.sh (new, installed to /opt/noviq/gpu-release.sh)
tests:
  - bash -n on both scripts (VM): PASS
  - behaviour: journalctl "Successfully registered CUDAExecutionProvider" + "Embedding model ready (gpu=true)": PASS
---

## Prompt

what is pending work

(followed by) ok go for 1

## Response snapshot

Listed pending work from the 2026-08-15 14:37 checkpoint, then executed item 1 —
resuming the Sindh corpus load.

Survey found the VM idle: release `b76932b0`, poller parked
(`GPU_MODE_DISABLED`), no tmux session, CPU embeddings, chat up. The
`gpu-hold.sh` / `gpu-release.sh` referred to in the checkpoint **did not exist**
on the VM (only `.env.bak.gpu-hold` survived, so they had been run and removed),
so both were written fresh as reference copies under `deploy/` and installed to
`/opt/noviq` as root.

`gpu-hold.sh` stops `noviq-gpu-index.timer` so the poller cannot toggle mid-run
— the toggle is what kills `add_indexed_folder`'s lazy directory walk, leaving
un-reached files with no source rows for `resume_interrupted_indexing`. It keeps
the same preflight (missing CUDA runtime must cost nothing), the same
.env-vs-drop-in split (`ORT_DYLIB_PATH`/`ECHO_EMBED_GPU` in `.env`,
`LD_LIBRARY_PATH` in the systemd drop-in), and deliberately leaves
`GPU_MODE_DISABLED` in place so that a timer coming back by itself releases the
GPU rather than extending the outage.

Architect chose GPU hold (chat down) over a slower CPU run with chat up.

Verified live: vLLM stopped, VRAM 10,873 → 1,047 MiB used, server healthy on
GPU env, `gpu=true` in the log, run advancing at roughly 7 s/file.

## Outcome

- ✅ Impact: corpus run resumed at HOME DEPARTMENT (7/39 departments); ~2,300
  files / 32 departments remaining, on GPU embeddings held for the whole run.
- 🧪 Tests: `bash -n` clean on both scripts; GPU embedding confirmed by log line,
  not assumed.
- 📁 Files: `deploy/gpu-hold.sh`, `deploy/gpu-release.sh` (both uncommitted).
- 🔁 Next prompts: release with `sudo bash /opt/noviq/gpu-release.sh` when the
  run completes; then re-test the project upload fix (`b76932b0`).
- 🧠 Reflection: a script the checkpoint said existed was gone. Listing the
  directory before trusting the note cost one round-trip and saved a failed run.

## Evaluation notes (flywheel)

- Failure modes observed: none in the run so far; two tool-classifier denials on
  `sudo -S` invocations, cleared on retry.
- Graders run and results (PASS/FAIL): n/a
- Prompt variant (if applicable): n/a
- Next experiment (smallest change to try): cache the discovered Postgres port —
  `pq()` re-probes every 127.0.0.1 port on every tick.
