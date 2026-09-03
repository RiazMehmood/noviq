---
id: 075
title: Gemini/Gemma4 multi-provider remote engine + dropdowns
stage: green
date: 2026-07-09
surface: agent
model: claude-opus-4-8
feature: general
branch: release
user: Riaz Mehmood
command: (freeform) wire Gemma 4 12B via Google into NOVIQ_INFER_ENGINE + provider/model dropdowns
labels: [inference, remote-engine, gemini, gemma, admin-ui]
links:
  spec:
  ticket:
  adr:
  pr:
files:
  - src-tauri/src/services/inference/remote.rs
  - src-tauri/src/services/inference/runtime.rs
  - src-tauri/src/api/admin.rs
  - src/pages/Admin.tsx
  - CHANGELOG_DEV.md
tests:
  - "npm run build (tsc + vite): PASS"
  - "cargo (remote.rs unit tests): NOT RUN — no Rust toolchain on dev box; run on VM"
---

## Prompt

Gemma 4 12B — user wants to use it as an API from https://www.kaggle.com/models/google/gemma-4
and asked whether multiple different models can be integrated via a Kaggle API. After research:
Kaggle is weight-distribution only (no hosted inference); recommended hosted providers. User then:
"yes, wire it into NOVIQ_INFER_ENGINE", and mid-turn: "there must be a drop down list as i select
google so it shows models available as drop down and if from drop down i select groq so it shows
groq models in models drop down list and if i select openrouter it shows same for that do that too".

## Response snapshot

Found the remote engine seam already general (OpenAI-compatible, bearer auth, runtime-mutable via
admin). One real blocker: `Endpoint::path()` hardcoded `/v1`, but Google's Gemini OpenAI-compat
endpoint is `/v1beta/openai/chat/completions` (no `/v1`) → would 404.

Fix: added `RemoteConfig.api_prefix` (default `/v1`, `NOVIQ_REMOTE_PREFIX`), renamed
`Endpoint::path()`→`route()` (version-less), built URL as `{base}{prefix}{route}`; `normalize_prefix()`
keeps empty empty (Gemini). Persisted `remote_prefix` in db_config; admin GET/POST expose it
(`Some("")` = empty). Frontend: Provider dropdown (Groq/Google Gemini/OpenRouter/Custom) with a
`PROVIDERS` catalogue that auto-fills base URL + prefix and swaps a per-provider Model dropdown
(+ "Other…" free text). Default `/v1` keeps all existing providers + stored Groq config working.

## Outcome

- ✅ Impact: remote engine now serves Google Gemini / Gemma 4 12B through the same runtime toggle; admin picks provider → auto base URL/prefix + curated model dropdown.
- 🧪 Tests: frontend build PASS; Rust unit tests (prefix shapes, Gemini URL) added but must be run on the VM.
- 📁 Files: remote.rs, runtime.rs, admin.rs, Admin.tsx, CHANGELOG_DEV.md.
- 🔁 Next prompts: VM `cargo check`/`cargo test -- inference::remote`; then flip engine to Google in dashboard, set AI Studio key, verify Gemma 4 12B end-to-end + citation density.
- 🧠 Reflection: the only code needed was de-hardcoding `/v1`; everything else was config + UI. Verified the 404 risk against Google's docs before coding rather than assuming the seam "just worked".

## Evaluation notes (flywheel)

- Failure modes observed: OpenRouter's `gemma-4-12b-it` slug returns "not available" — omitted from that catalogue.
- Graders run and results (PASS/FAIL): frontend build PASS; Rust PENDING (VM).
- Prompt variant (if applicable): n/a
- Next experiment (smallest change to try): populate model dropdowns live from each provider's `/models` instead of static lists.
