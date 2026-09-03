---
id: 017
title: Tauri-decoupling shared-layer complete
stage: refactor
date: 2026-06-26
surface: agent
model: claude-opus-4-8
feature: 014-server-thin-client
branch: 014-server-thin-client
user: Riaz Mehmood
command: (resume mid-task)
labels: [f014, decoupling, headless, compat, tauri, feature-gate]
links:
  spec: specs/014-server-thin-client/spec.md
  ticket:
  adr: ADR-0017
  pr:
files:
  - src-tauri/src/services/analysis_engine.rs
  - src-tauri/src/services/entity_extractor.rs
  - src-tauri/src/services/intent_router/mod.rs
  - src-tauri/src/services/cloud_llm.rs
  - src-tauri/src/services/assistant/mod.rs
  - src-tauri/src/services/assistant/mapreduce.rs
  - src-tauri/src/services/assistant/capabilities/action_items.rs
  - src-tauri/src/services/assistant/capabilities/conflict.rs
  - src-tauri/src/services/assistant/capabilities/compare.rs
  - src-tauri/src/services/assistant/capabilities/summarize_doc.rs
  - src-tauri/src/services/assistant/capabilities/mod.rs
  - src-tauri/src/services/assistant/capabilities/draft.rs
  - src-tauri/src/services/assistant/capabilities/guidance.rs
  - src-tauri/src/services/doc_ir/progress.rs
  - src-tauri/src/services/doc_ir/cache.rs
  - src-tauri/src/services/doc_ir/dispatch.rs
tests:
  - none (no local Rust toolchain — cargo check runs on the GPU VM)
---

## Prompt

sorry laptop shutdown due to low battery we were in the middle of remaining work start again

## Response snapshot

Resumed the F014 Tauri-decoupling refactor from `RESUME_DECOUPLING.md`. Re-derived
the frontier by sweeping for `tauri::` refs: confirmed the prior-session files
(Cargo.toml, build.rs, compat.rs, lib.rs, services/mod.rs, indexer.rs, query_trace.rs,
llm.rs, query_router.rs, scheduler.rs) were already complete, then finished the
remaining shared-layer files:

- `analysis_engine.rs` (3 sites), `entity_extractor.rs`, `intent_router/mod.rs`:
  pure `tauri::AppHandle` → `crate::compat::AppHandle` swaps.
- `cloud_llm.rs`: import split + desktop-gated `settings_path` body with a
  `NOVIQ_DATA_DIR` headless fallback; kept `load_settings`/`save_settings` defined
  in both builds (entity_extractor's `Some(a)` branch must compile on the server).
- `assistant/{mod,mapreduce}.rs` + all 7 `capabilities/*.rs`: AppHandle/Emitter → compat.
- `doc_ir/{progress,cache,dispatch}.rs`: AppHandle/Emitter → compat; `cache_dir` gated
  like `settings_path`. doc_ir is server-reachable (indexer → dispatch) so the module
  is NOT gated — only the `Manager` import and the path-resolver body are.

Verified by full sweep that every `tauri::` path reachable from `--features server`
is now compat-shimmed or `#[cfg(feature="desktop")]`-gated; remaining raw refs live
only in desktop-only modules (commands/*, meeting, batch_processor, stt_sidecar) or
behind explicit desktop cfgs. Confirmed `api/query.rs` passes `None` for the new
`Option<&compat::AppHandle>` sig. Updated CHANGELOG_DEV.md, RESUME_DECOUPLING.md, and memory.

## Outcome

- ✅ Impact: headless `noviq-server` shared service layer is decoupled from Tauri/GTK/WebKit — code-complete; unblocks the Ubuntu-20.04 GPU-VM build.
- 🧪 Tests: none locally (no Rust toolchain); next = `cargo check --no-default-features --features server` on the VM.
- 📁 Files: 16 service files this session (+10 prior); compat shim drives both builds.
- 🔁 Next prompts: scp `src-tauri/{src,Cargo.toml,build.rs}` to VM → cargo check server → llm,server → CUDA build.
- 🧠 Reflection: the `Some(app)` branches that are dead on the server still must type-check, so functions like `load_settings`/`cache_dir` were body-gated rather than whole-function-gated.

## Evaluation notes (flywheel)

- Failure modes observed: RESUME doc module paths were slightly off (everything is under `services/`); grep on the real tree corrected it.
- Graders run and results (PASS/FAIL): N/A (compile verification deferred to VM).
- Prompt variant (if applicable): N/A
- Next experiment (smallest change to try): run the VM `cargo check --features server` and fix any compiler-surfaced missed site.
