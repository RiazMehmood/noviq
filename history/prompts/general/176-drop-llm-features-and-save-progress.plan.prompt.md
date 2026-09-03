---
id: 176
title: Drop three LLM features permanently; save Docker progress
stage: plan
date: 2026-09-01
surface: agent
model: claude-opus-5
feature: general
branch: 028-noviq-pk-cutover
user: Riaz Mehmood
command: (discussion)
labels: [docker, deployment, scope-decision, remote-inference]
links:
  spec: n/a
  ticket: n/a
  adr: n/a
  pr: n/a
files:
  - CHANGELOG_DEV.md
  - deploy/docker/Dockerfile
  - deploy/docker/entrypoint.sh
  - deploy/docker/env.example
  - deploy/docker/README.md
  - .dockerignore
  - .gitattributes
tests:
  - none (nothing compiled — no Docker or cargo on this machine)
---

## Prompt

Three turns, verbatim:

1. "we dont need, Ai powered pdf importer, one-ask and drafting, these features are no more required."
2. "how will then we connect LLM via api?"
3. "save progress upto now, we will do that tomorrow."

## Response snapshot

**Scope decision recorded.** The three `llm`-gated features are permanently dropped, not
deferred. No Dockerfile change needed — the no-`llm` build already excludes them. Code
left compiled-out rather than deleted; offered removal as a future cleanup, architect did
not ask for it.

**Checked the one `llm`-gated helper that is not a named feature.**
`assistant/n_ctx.rs::for_model` (reads the trained window off the GGUF) is feature-gated,
and its stub returns `FALLBACK_TRAINED = 4096` — that would have silently halved the
prompt budget and degraded answers with no error. It never fires:
`assistant/mod.rs:420` returns early for `is_http()` engines and reads
`NOVIQ_REMOTE_MAX_MODEL_LEN` (default 8192) instead. No regression. Logged the residual
trap: switching the engine back to llamacpp in a no-llm build silently yields a 4096 window.

**Answered the API question.** Removing the in-process engine does not remove API access —
the API path is what remains. `NOVIQ_REMOTE_URL` is the host ROOT; the client composes
`{base_url}{api_prefix}{route}` = `/v1` + `/chat/completions` (`remote.rs:59,:110`).
Two failure modes flagged: llama.cpp binds localhost unless started with `--host 0.0.0.0`
(the exact mirror of the `NOVIQ_BIND` container issue), and a URL ending in `/v1` becomes
`/v1/v1/...`. Pre-flight from VM-2: `curl http://<vm1-ip>:8000/v1/models`.

**Saved progress**: changelog entry prepended, memory file + index updated, PHR written.

## Outcome

- ✅ Impact: scope narrowed with the build already matching it; the silent-4096 trap ruled
  out by code-read; the connection contract documented with its two real failure modes.
- 🧪 Tests: none. Nothing has been compiled or run — the VM-2 build is the first execution.
- 📁 Files: changelog entry; `deploy/docker/*` unchanged from PHR-175.
- 🔁 Next prompts: `docker build --target builder` on VM-2, then the full build and run.
- 🧠 Reflection: the useful move was not accepting the feature drop at face value but
  checking what *else* rode on the same feature flag. `n_ctx` would have degraded answer
  quality without producing an error anywhere.

## Evaluation notes (flywheel)

- Failure modes observed: none — nothing executed.
- Graders run and results (PASS/FAIL): n/a
- Prompt variant (if applicable): n/a
- Next experiment (smallest change to try): builder-stage-only build on VM-2.
