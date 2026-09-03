---
id: 071
title: Remote OpenAI-compatible LLM API inference engine
stage: green
date: 2026-07-08
surface: agent
model: claude-opus-4-8
feature: general
branch: release
user: Riaz Mehmood
command: start work on api integration part
labels: [inference, api-integration, engine-seam, F016]
links:
  spec: specs/016-vllm-serving/contracts/text-generator-trait.md
  ticket:
  adr: history/adr/0018-server-side-inference-engine-vllm-continuous-batching-sidecar.md
  pr:
files:
  - src-tauri/src/services/inference/remote.rs
  - src-tauri/src/services/inference/mod.rs
  - src-tauri/src/services/query_router.rs
  - src-tauri/src/services/assistant/mapreduce.rs
  - src-tauri/src/services/assistant/mod.rs
  - src-tauri/src/api/query.rs
  - src-tauri/src/api/health.rs
  - deploy/noviq-server.service
  - CHANGELOG_DEV.md
tests:
  - src-tauri/src/services/inference/remote.rs (9 unit tests: chatml->messages parse, body shape per endpoint, SSE chat/completions delta parse, keepalive/roleless skip)
  - src-tauri/src/services/inference/mod.rs (engine_from_env remote/api mapping, is_http predicate)
---

## Prompt

start work on api integration part.
(Clarified via question → "llm inference api integration": add an external hosted LLM
API as an inference engine alongside vLLM/llama.cpp.)

## Response snapshot

Added a third inference backend `EngineKind::Remote`, selected with
`NOVIQ_INFER_ENGINE=remote|api`, that generates through a hosted OpenAI-compatible API
(OpenRouter/Groq/Together/…). New `RemoteGenerator` implements the existing
`TextGenerator` seam — mirrors `vllm.rs` (raw-byte SSE buffering, cancel, per-delta
`on_token`) plus bearer auth (`NOVIQ_REMOTE_API_KEY`, env-only) and two endpoint shapes
(`chat` default / `completions`). Introduced `EngineKind::is_http()` to replace the
scattered `== Vllm` gates (their real meaning is "out-of-process HTTP engine"), a
`http_generator() -> Option<&dyn TextGenerator>` dispatch point, and routed `vllm_infer`
through it so both HTTP engines share one path. Wired health, trace labels, map-reduce/
window budgeting, and the gen-gate permit skip. Documented env in the systemd unit +
CHANGELOG.

## Outcome

- ✅ Impact: server can run fully on a remote free/hosted LLM API with no local weights; desktop default (llama.cpp) unchanged; vLLM path untouched.
- 🧪 Tests: 9 new unit tests in remote.rs + engine-mapping/predicate tests in mod.rs. NOT yet compiled (no Rust on Windows) or live-smoked against a provider.
- 📁 Files: 1 new module + 6 wiring edits + service doc + changelog.
- 🔁 Next prompts: VM `cargo check` (server + llm,server); `cargo test services::inference`; live streamed turn against one free API (set NOVIQ_INFER_ENGINE=remote + keys in .env); verify citation quality chat-vs-completions.
- 🧠 Reflection: the `== Vllm` checks were secretly an "is-HTTP-engine" predicate; folding them into `is_http()` made the third engine a small additive change rather than a scatter of new branches.

## Evaluation notes (flywheel)

- Failure modes observed: none yet (unverified — pending VM compile + provider smoke).
- Graders run and results (PASS/FAIL): none run.
- Prompt variant (if applicable): n/a
- Next experiment (smallest change to try): compile on VM, then `NOVIQ_INFER_ENGINE=remote` against a free OpenRouter model and stream one legal Q end-to-end; confirm chat-mode ChatML->messages produces a cited answer.
