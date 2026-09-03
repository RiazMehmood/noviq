---
id: 126
title: GPU reindex canary and completion fixes
stage: misc
date: 2026-07-29
surface: agent
model: claude-fable-5
feature: none
branch: release-deploy
user: Riaz Mehmood
command: none
labels: [ops, gpu, reindex, corpus-quality]
links:
  spec: none
  ticket: none
  adr: none
  pr: none
files:
- deploy/gpu-reindex.sh
- /opt/noviq/gpu-reindex.sh (live, out of tree; backup .bak-2026-07-29)
tests:
- bash -n syntax check on the patched live script
- CANARY OK observed on the 02:14 run (CUDAExecutionProvider registered)
---

## Prompt

i want to reindex all using gpu. → "reindexing done, check it" →
"save progress upto now we will start tomorrow"

## Response snapshot

The GPU re-index had failed at 23:54 and would have failed again. Diagnosed
two independent defects in the out-of-tree `/opt/noviq/gpu-reindex.sh`:

1. **Canary timing** — a fixed `sleep 14` then a grep for ort's
   "Successfully registered `CUDAExecutionProvider`". The embedder loads ON
   FIRST USE and the canary source (largest document) parses ~60s first, so
   the log was empty and a healthy run aborted with chat already down. The GPU
   stack itself was fine (`libonnxruntime_providers_cuda.so`, 767 MB, present).
   Fixed by polling every 5s up to CANARY_MAX=300. Result: **CANARY OK** on the
   next run.

2. **Completion signal** — the wait loop watched the unembedded-chunk count,
   which is BLIND while a document parses (no chunks yet). It read 0 between
   documents, and three such polls ended the run 65 s in; the restore restarted
   the server and killed 13 of 17 in-flight re-indexes (only Constitution,
   SPPRA Act 2009, APT Rules 1974, RoB 1986 completed; total chunks never moved
   from 13,960). Fixed by requiring every admin source to be `indexed` with
   `indexed_at` (MILLISECONDS) newer than a RUN_START_MS stamped before the
   canary. MAX_WAIT 1800 → 5400 to cover parse + embed.

Both patched live (backup kept), syntax-checked, and ported into the repo
reference copy `deploy/gpu-reindex.sh` so they survive.

## Outcome

- ✅ Impact: the GPU re-index path works and can now finish a full corpus run.
- 🧪 Tests: bash -n; CANARY OK observed live; completion loop NOT yet exercised end-to-end.
- 📁 Files: deploy/gpu-reindex.sh (ported), live script patched.
- 🔁 Next: run the GPU re-index (13 documents still on old parsing), then re-run scripts/corpus_gap_check.sql.
- 🧠 Reflection: a completion heuristic must observe the WORK (sources), not a side-effect (unembedded chunks) that is legitimately zero mid-flight.

## Evaluation notes (flywheel)

- Failure modes observed: two different "looks finished / looks broken" heuristics both fired on an empty signal window.
- Graders run and results (PASS/FAIL): n/a
- Prompt variant (if applicable): n/a
- Next experiment (smallest change to try): full GPU run, watch `not-yet-reindexed` count down.
