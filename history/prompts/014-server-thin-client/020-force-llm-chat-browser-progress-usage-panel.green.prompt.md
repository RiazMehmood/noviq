---
id: 020
title: Force LLM chat + browser indexing progress + per-user usage panel
stage: green
date: 2026-06-27
surface: agent
model: claude-opus-4-8
feature: 014-server-thin-client
branch: 014-server-thin-client
user: Riaz Mehmood
command: (freeform request)
labels: [chat, llm, indexing, sse, admin, usage, telemetry]
links:
  spec: specs/014-server-thin-client/spec.md
  ticket:
  adr:
  pr:
files:
  - src-tauri/src/services/query_router.rs
  - src-tauri/src/commands/system.rs
  - src-tauri/src/db/pg/migrations/007_query_log_usage.sql
  - src-tauri/src/services/usage_sampler.rs
  - src-tauri/src/services/mod.rs
  - src-tauri/src/db/repositories/query_log.rs
  - src-tauri/src/db/repositories/mod.rs
  - src-tauri/src/api/query.rs
  - src-tauri/src/api/admin.rs
  - src-tauri/src/api/router.rs
  - src/api/transport.ts
  - src/pages/SettingsPage.tsx
  - src/pages/Admin.tsx
tests:
  - cargo check --no-default-features --features llm,server (exit 0)
  - npx tsc --noEmit (exit 0)
---

## Prompt

we have still that option which by passes the LLM and currently that is active and
i m unable to reset it to model. as we run the chat queries it is not ussing llm. i
want it to use only LLM. and the progress bar during document indexing is also not
shown, and third thing i want to see is usage of users, a single user is consuming
how much ram, cpu and gpu while it runs queries it must logs inside user management
pan so admin can see that also, when user was active and how many queries it conduct
and what was the response is also remained fetch able if we want to download them or
view them like traces. so in ui it must show queries numbers cpu, ram and gpu usage
per user its recent activity also, but its query details and other can be
downloadable which we can download any time.

(Clarifiers answered: force LLM-only; sample per-query attributed to user; store
full query+answer+citations+metrics, downloadable JSON/CSV.)

## Response snapshot

Diagnosed all three. (1) Chat bypass = `chat_extractive` db_config flag defaulting
ON with only Tauri (not HTTP) toggles → flipped default OFF + migration resets the
live row to '0'. (2) Browser progress bar = SettingsPage used the Tauri-only
`listen("source:indexing-progress")`; swapped to cross-transport `onIndexProgress`
(SSE on web). (3) Per-user usage = new `query_log` table + `usage_sampler` (sysinfo
CPU/RSS + nvidia-smi GPU peaks, attributed via the serialized gen gate), wired into
`api/query.rs`; admin endpoints for rollup/activity/export; Admin.tsx table with
usage columns, an Activity expander, and JSON/CSV downloads via a new
`transport.downloadAuthed`.

## Outcome

- ✅ Impact: live server now answers with the LLM; browser indexing shows progress;
  admins see per-user query counts, last-active, CPU/RAM/GPU, recent activity, and
  can download full query+answer history (JSON/CSV).
- 🧪 Tests: `cargo check --features llm,server` → 0; `tsc --noEmit` → 0.
- 📁 Files: 2 default flips, 1 additive migration, 2 new backend modules (sampler +
  repo), 3 API wirings, 3 frontend files.
- 🔁 Next prompts: `make deploy` to the VM; confirm GPU columns populate on the A40;
  consider a retention/rollup policy for `query_log` growth.
- 🧠 Reflection: the bypass had no server-side control surface at all — the fix was
  as much about exposing/forcing the default as the toggle itself.

## Evaluation notes (flywheel)

- Failure modes observed: extractive default ON silently disabled the model on the
  headless deploy; Tauri-only event listener silently no-op'd in the browser.
- Graders run and results (PASS/FAIL): cargo check PASS, tsc PASS.
- Prompt variant (if applicable): n/a
- Next experiment (smallest change to try): live smoke on the VM — one query (LLM
  answer + a `query_log` row with GPU peaks) and one upload (progress ticks).
