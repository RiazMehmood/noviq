---
id: 018
title: VM standup — GPU LLM + focal PG + ORT + systemd
stage: green
date: 2026-06-26
surface: agent
model: claude-opus-4-8
feature: 014-server-thin-client
branch: 014-server-thin-client
user: Riaz Mehmood
command: "start working on remaining work"
labels: [deploy, gpu, cuda, postgres, onnxruntime, systemd, vm]
links:
  spec: specs/014-server-thin-client/spec.md
  ticket:
  adr:
  pr:
files:
  - deploy/deploy.sh
  - deploy/noviq-server.service
  - deploy/noviq-deploy.service
  - deploy/noviq-deploy-check.service
  - CHANGELOG_DEV.md
tests:
  - "manual: /api/health -> {gpu:true,layers:999}"
  - "manual: /api/login + /api/me (200), unauth /api/me (401)"
  - "manual: /api/query -> HTTP 200 with model offloaded 25/25 to A40"
---

## Prompt

start working on remaining work

(Then, mid-session: "save the progress so we start new session cache tokens are too much for these.")

## Response snapshot

Took the green CUDA build to a fully running, durable headless server on the IBA A40 GPU VM
(Ubuntu 20.04). Resolved five runtime blockers, all on the VM, mostly without code changes:

1. **CUDA build** — focal CMake 3.16 too old for ggml-cuda → installed CMake 3.30 to
   `~/.local/cmake-current` (PATH in `~/.profile`); cleared a stale cmake cache; nvcc host objects
   needed `CUDAFLAGS="-Xcompiler -fPIC"`. Built `--no-default-features --features llm,server,cuda
   --bin noviq-server`, `CMAKE_CUDA_ARCHITECTURES=86`.
2. **deploy.sh** was missing `--no-default-features` (would re-pull webkit on the headless box) — fixed,
   plus exports the CUDA arch / PIC / native-CPU env.
3. **bundled PG vs focal glibc 2.31** — theseus PG17 needs glibc 2.34 and PGDG dropped focal, so built
   **PostgreSQL 17.10 + pgvector 0.8.1 from source** into the install dir; postgresql_embedded reuses it
   (no code change). `CREATE EXTENSION vector` + halfvec HNSW now succeed.
4. **SIGILL on model load** — the vCPU is a masked pre-AVX Core2; `RUSTFLAGS="-C target-cpu=native"`
   → `GGML_NATIVE=ON` so ggml matches the host ISA.
5. **ONNX Runtime missing** — vendored `libonnxruntime.so.1.20.1`, `ORT_DYLIB_PATH` in `.env`.

Made it durable with systemd (`noviq-server.service`, User=appadmin, enabled, Restart=on-failure);
verified `/api/health`, auth, and `/api/query` (HTTP 200) under systemd. Found a product gap (T053):
the headless API has no create-notebook route, so browser clients can't index → T028 latency deferred.

## Outcome

- ✅ Impact: F014 headless server runs end-to-end on the A40 (GPU LLM + focal-native PG/pgvector +
  ORT embedder), durable under systemd. Core deployment goal met.
- 🧪 Tests: manual API smokes green (health gpu:true, login/me/401, query 200 with 25/25 GPU offload).
  `#[ignore]`d server-side DB tests not yet run on the VM.
- 📁 Files: deploy.sh + 3 systemd units (User/login-shell/build env), CHANGELOG_DEV.md (full standup).
  Commits local (73444af, 3f64bda), NOT pushed.
- 🔁 Next prompts: T053 create-notebook endpoint → T028 latency → server tests (T021/T048) →
  poll-timer + SC-001..010 acceptance (T052) → finalize quickstart (T051) → push.
- 🧠 Reflection: most blockers were environment, not code — an old VM (glibc 2.31, masked Core2 vCPU)
  forced source builds and native-CPU codegen. Keeping the bundled-PG architecture worked because
  postgresql_embedded reuses a pre-placed version-matched install.

## Evaluation notes (flywheel)

- Failure modes observed: stale cmake cache, non-PIC CUDA objects, glibc mismatch, AVX SIGILL,
  missing ORT dylib, orphan PG after crash, notebook FK (no create route), nested-shell quote breakage.
- Graders run and results (PASS/FAIL): manual smoke PASS (health/auth/query); automated tests PENDING.
- Prompt variant (if applicable): n/a
- Next experiment (smallest change to try): add `POST /api/create_notebook` (or upsert default notebook
  in indexing) to unblock browser indexing + T028.
