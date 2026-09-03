---
id: 074
title: Admin inference engine toggle (local GPU vs Groq)
stage: green
date: 2026-07-08
surface: agent
model: claude-opus-4-8
feature: general
branch: release
user: Riaz Mehmood
command: admin dashboard toggle for groq api vs local gpu with api key insert
labels: [admin, inference, groq, runtime-config, frontend]
links:
  spec:
  ticket:
  adr:
  pr:
files:
  - src-tauri/src/services/inference/runtime.rs
  - src-tauri/src/services/inference/mod.rs
  - src-tauri/src/services/inference/remote.rs
  - src-tauri/src/services/query_router.rs
  - src-tauri/src/api/admin.rs
  - src-tauri/src/api/router.rs
  - src-tauri/src/api/health.rs
  - src-tauri/src/bin/noviq-server.rs
  - src/pages/Admin.tsx
  - CHANGELOG_DEV.md
tests:
  - "VM cargo check --no-default-features --features llm,server,cuda: EXIT=0"
  - "Frontend npm run build (tsc + vite): EXIT=0"
  - "Groq live end-to-end proven earlier (remote::live_provider_stream)"
---

## Prompt

now as u integrated this, now i want in admin dashboard an option from where we can toggle groq
api usage or local gpu access by inserting groq api key. so do that then i will insert and activate
groq api

## Response snapshot

Added a runtime-mutable inference config (`services/inference/runtime.rs`, `RwLock` over engine +
RemoteConfig + cached `Arc<dyn TextGenerator>`) so the engine can be switched without a restart;
`engine_from_env()` now returns the runtime-active engine so every gate honours a live flip.
Persisted to `db_config` (engine/url/model/endpoint/api_key). Admin API `GET/POST /api/admin/inference`
(GET never returns the key — only `has_api_key`; POST merges onto stored config, rejects remote
without a key, persists + applies + audits). Startup `load_from_db` applies a saved override (no-op
otherwise). Admin.tsx gains an "Inference engine" card: Local GPU (vLLM) ⇄ Hosted API (Groq) select
+ write-only key field + model/base-url/endpoint. Backend cargo check EXIT=0; frontend build EXIT=0.
Inert by default (prod stays vLLM until an admin flips it).

## Outcome

- ✅ Impact: admin can switch chat between local A40 (vLLM) and Groq API + insert the key, live, from the dashboard.
- 🧪 Tests: backend cargo check EXIT=0; frontend build EXIT=0; Groq path proven live earlier.
- 📁 Files: new runtime module + seam/admin/router/health/bootstrap wiring + Admin.tsx card.
- 🔁 Next prompts: deploy (push to release → rebuild incl. frontend); then admin inserts the Groq key + flips to Hosted API; verify a chat turn routes to Groq via /api/health.engine=remote.
- 🧠 Reflection: the toggle is small; the real work was converting env-captured OnceLock engine selection into a runtime-mutable RwLock config that every existing gate reads through `engine_from_env()`, so no call-site changes were needed.

## Evaluation notes (flywheel)

- Failure modes observed: `Arc<Concrete>`→`Arc<dyn>` needed explicit `as` coercion through `Some(...)`; re-added `TextGenerator` import for the `Arc<dyn>` method call. Both caught locally before VM check.
- Graders run and results (PASS/FAIL): compile PASS (backend + frontend). No live UI test yet.
- Prompt variant (if applicable): n/a
- Next experiment (smallest change to try): deploy, insert Groq key in the dashboard, flip to Hosted API, run one chat turn, confirm `/api/health` reports `engine.kind=remote` and the answer streams from Groq.
