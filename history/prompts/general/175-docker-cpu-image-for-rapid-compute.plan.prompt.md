---
id: 175
title: Single Docker image for the Rapid Compute CPU test
stage: plan
date: 2026-09-01
surface: agent
model: claude-opus-5
feature: general
branch: 028-noviq-pk-cutover
user: Riaz Mehmood
command: (discussion)
labels: [docker, deployment, cpu-inference, rapid-compute]
links:
  spec: n/a
  ticket: n/a
  adr: n/a
  pr: n/a
files:
  - deploy/docker/Dockerfile
  - deploy/docker/entrypoint.sh
  - deploy/docker/env.example
  - deploy/docker/README.md
  - .dockerignore
  - .gitattributes
tests:
  - none (build is the verification; runs on VM-2)
---

## Prompt

Three turns, verbatim in order:

1. "we already work on rapid compute vm and deployed LLm model there and try to connect to iba vm, but we made a change in testing, we asked rapid campute to create another vm on their own network, so tomorrow we are going to deploy our backend and db on their vm temprorly and connect their two vm's on their network easily and test LLM inference on cpu. i want this process to be fast enought so what we can do now? discuss first"

2. "can we create the docker images and then just open them on vm so easily we can install this? discuss"

3. "i will install docker on vm, and there is internet on the vm. so first check this on cpu then create a image and proper plan so by just running single dockeer image installs complete app, we want to run that on web."

## Response snapshot

Read the deploy path before answering. Established by code-read:

- PostgreSQL is EMBEDDED in the binary (`db/pg/engine.rs:120-160`), self-provisioned,
  `listen_addresses=127.0.0.1`, password stamped at initdb into `{pg_root}/.pgpass`.
  No `DATABASE_URL` anywhere → app + DB are one process tree → ONE image, no compose.
- `query_router.rs:4194` gates generation on `engine_from_env().is_http()` and returns
  from the HTTP path BEFORE touching `ModelSlot`; `EngineKind::Remote.is_http()` is true
  and `NOVIQ_VLLM_FALLBACK` is off by default. So `NOVIQ_INFER_ENGINE=remote` makes the
  `llm` feature unnecessary at runtime → build `--no-default-features --features server`,
  dropping CMake/ggml/nvcc and the `-C target-cpu=native` build-host==run-host constraint
  that `deploy/deploy.sh:88` carries.
- `build.rs` already guards `tauri_build::build()` behind `CARGO_FEATURE_DESKTOP`.
- reqwest is `rustls-tls` → no OpenSSL in the runtime image.
- Shelled-out binaries: pdftotext/pdfinfo/pdftoppm/pdfseparate/pdfunite (poppler-utils),
  tesseract.
- `NOVIQ_PGVECTOR_DIR` / `NOVIQ_WEB_DIR` / `NOVIQ_DATA_DIR` / `NOVIQ_BIND` env overrides
  exist; `seed_admin_if_configured` seeds an admin from `NOVIQ_ADMIN_USER`/`_PASS` on
  first boot only.
- `NOVIQ_REMOTE_URL` is a host ROOT; `/v1` is appended via `NOVIQ_REMOTE_PREFIX`.
- `embedder.rs cache_dir()` has NO env override — it walks up from the exe for an existing
  `.fastembed_cache`, so the entrypoint symlinks `/opt/noviq/.fastembed_cache` onto the volume.

This machine has neither Docker nor a Rust toolchain, so the build (which IS the CPU
verification) must run on VM-2. Wrote a 3-stage Dockerfile (node → rust → debian-slim),
an entrypoint that parks the embedding cache on the volume and drops root→uid 10001 via
gosu (PostgreSQL refuses to run as root), an env template for the two-VM topology, and a
runbook with a Plan B (`--features "llm,server"` + cmake/libclang) if the no-llm build fails.

Also flagged: CPU cost is dominated by PREFILL, not decode — the 19.5 tok/s figure is a
decode number and a full RAG prompt will not behave like it.

## Outcome

- ✅ Impact: a `docker build` + `docker run` path that stands up backend + DB + web client
  on a CPU VM with generation delegated to VM-1. Doubles as the on-prem install artifact.
- 🧪 Tests: none run — no Docker or cargo on this machine. The image build on VM-2 is the
  first real verification of the no-`llm` build.
- 📁 Files: 4 new under `deploy/docker/`, plus `.dockerignore` and `.gitattributes`.
- 🔁 Next prompts: run the build on VM-2; if it fails, apply Plan B; then decide the corpus
  seeding strategy (fresh index vs. volume snapshot from IBA).
- 🧠 Reflection: the embedded-Postgres design decided the whole shape — one image, one
  volume, non-root. Reading `engine.rs` first avoided proposing a compose stack that could
  never have worked.

## Evaluation notes (flywheel)

- Failure modes observed: none yet — the plan is unexecuted. The no-`llm` build is verified
  statically only; that is the standing risk.
- Graders run and results (PASS/FAIL): n/a
- Prompt variant (if applicable): n/a
- Next experiment (smallest change to try): run only the `builder` stage
  (`docker build --target builder`) on VM-2 first — it isolates the one unproven step.
