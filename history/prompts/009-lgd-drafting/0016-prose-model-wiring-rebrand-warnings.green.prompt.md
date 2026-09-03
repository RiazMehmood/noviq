---
id: 0016
title: T017 prose model wiring + window rebrand + cargo warning cleanup
stage: green
date: 2026-06-08
surface: agent
model: claude-opus-4-8
feature: 009-lgd-drafting
branch: 009-lgd-drafting
user: Riaz Mehmood
command: "complete prose block; rename titlebar to Noviq; remove cargo warnings"
labels: [backend, rust, llm, drafting, prose, branding, warnings]
links:
  spec: specs/009-lgd-drafting/spec.md
  ticket:
  adr: history/adr/0008-deterministic-citation-integrity-for-answers-and-drafts.md
  pr:
files:
  - src-tauri/src/commands/drafting.rs
  - src-tauri/tauri.conf.json
  - index.html
  - src-tauri/src/services/analysis_engine.rs
  - src-tauri/src/services/doc_ir/validator.rs
  - src-tauri/src/commands/trial.rs
  - src-tauri/src/commands/system.rs
  - src-tauri/src/services/entity_extractor.rs
  - src-tauri/src/services/scheduler.rs
tests:
  - "cargo build --lib: 0 warnings, exit 0"
  - "cargo test --lib drafting: 31 passed"
  - "cargo test --test lgd_drafting_acceptance: 1 passed"
---

## Prompt

(1) "and also complete prose block"
(2) "still there is echoai written on top of the app, on that bar where minimize, maximize and x
button exist. change that to noviq also"
(3) "remove these warning also which always comes while we build cargo, after completion of this step."

## Response snapshot

T017 prose wiring: added `generate_prose` + `build_prose_prompt` + `PROSE_DEGRADE` to
commands/drafting.rs. Prose slots now resolve the local GGUF via
`query_router::select_model_for_tier`, run a strictly number-free ChatML prompt through
`ModelSlot::infer` (256 tok) inside `spawn_blocking`, and degrade to a clean placeholder sentence
when no model is on disk or inference fails (Principle VII). `render_one_slot` pre-generates the
prose and still feeds it through the single contract path (`render::render_slot` → `sanitize_prose`),
so FR-007 holds at the sink. `render_draft` + `revise_draft` gained `model_slot`/`tier` state and
thread them to all five call sites; no JS/command-signature change.

Window rebrand: tauri.conf.json main-window title + overlay title + tray tooltip → Noviq;
index.html <title> → Noviq. Left identifier/productName to avoid orphaning echo.db.

Warning cleanup: removed unused imports (analysis_engine resolve_model_path, validator Annotation,
entity_extractor Arc, scheduler AtomicU32/Duration/timeout), fixed a stray `///` doc comment in
trial.rs, and `#[allow(deprecated)]` on the best-effort Shell::open reveal in system.rs. Lib now
builds with zero warnings.

## Outcome

- ✅ Impact: Drafts now contain real model-written connective prose (with graceful degrade); the app
  window reads Noviq; cargo build is warning-free.
- 🧪 Tests: cargo build --lib 0 warnings; drafting 31/31; acceptance 1/1 (ONE cargo run for the batch).
- 📁 Files: 9 modified.
- 🔁 Next prompts: live `npm run tauri dev` smoke test (index Sindh corpus, render a Summary, confirm
  prose reads well and the citation popovers verify); T038/T039 polish.
- 🧠 Reflection: Pre-generating prose before the sync `render_slot` closure kept the one-contract-path
  design intact while making inference async — no duplication of the FR-007 sink.

## Evaluation notes (flywheel)

- Failure modes observed: none at build/test; prose quality unverified until a live model run.
- Graders run and results (PASS/FAIL): cargo build PASS (0 warn), drafting tests PASS, acceptance PASS.
- Prompt variant (if applicable): prose prompt v1 (number-free ChatML, 1–3 sentences).
- Next experiment (smallest change to try): run a real Summary draft and tune `build_prose_prompt`
  tone/length against the actual LGD register; raise/lower the 256-token cap if prose is clipped.
