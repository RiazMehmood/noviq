---
id: 0005
title: vLLM serving task list
stage: tasks
date: 2026-06-28
surface: agent
model: claude-opus-4-8
feature: 016-vllm-serving
branch: 016-vllm-serving
user: Riaz Mehmood
command: /sp.tasks
labels: [tasks, vllm, inference, concurrency]
links:
  spec: specs/016-vllm-serving/spec.md
  ticket: null
  adr: history/adr/0018-server-side-inference-engine-vllm-continuous-batching-sidecar.md
  pr: null
files:
  - specs/016-vllm-serving/tasks.md
tests:
  - none generated (limited SSE/trait unit tests scheduled as task T018)
---

## Prompt

proceed to /sp.tasks

## Response snapshot

Generated `specs/016-vllm-serving/tasks.md` from plan.md + spec.md + research.md + data-model.md
+ contracts/ + quickstart.md. 35 tasks across 6 phases, organized by the spec's 3 user stories.

- Phase 1 Setup: T001 Spike 0 marked **[X] DONE** (CUDA forward-compat + serve smoke proven
  2026-06-28); T002–T004 scaffolding (deploy/vllm, inference module, env keys).
- Phase 2 Foundational (blocking): TextGenerator trait + types, engine-selection factory,
  LlamaCppGenerator fallback, query_router call-site swap, noviq-server wiring. End state =
  engine=llamacpp behaves identically (regression-safe).
- Phase 3 US1 (P1 MVP): setup-vllm.sh, run-vllm.sh, VllmGenerator (/v1/completions SSE),
  error mapping, lift gen_gate, VRAM tune, load_test.sh, SC-001..004 validation.
- Phase 4 US2 (P2): SSE unit test, downstream cleanup+grounding preserved, chat:// events,
  refusal under load, cancellation.
- Phase 5 US3 (P3): systemd unit, ordering, deploy ensure-active, /api/health.engine, FR-011
  graceful+fallback, reboot recovery.
- Phase 6 Polish/Cutover: A/B run_ab.sh + no-regression gate (FR-014/SC-005), offline proof,
  the NOVIQ_INFER_ENGINE=vllm flip, optional query_log.engine, docs, constitution amendment.

Tests kept minimal (only the SSE/trait mapping the plan explicitly lists), not full TDD.
Marked maintenance-window dependency for T017/T030 (8B-AWQ won't fit alongside prod's ~6.3 GB).

## Outcome

- ✅ Impact: Executable, dependency-ordered task list; MVP = US1; cutover dark-shipped behind env flip.
- 🧪 Tests: SSE/trait unit tests scheduled (T018); not TDD.
- 📁 Files: specs/016-vllm-serving/tasks.md
- 🔁 Next prompts: /sp.implement (start Phase 1/2), or begin T002–T009 manually.
- 🧠 Reflection: Spike 0 being GREEN let T001 ship as done and removed the provisional CUDA gate from every downstream task.

## Evaluation notes (flywheel)

- Failure modes observed: none — all tasks have checkbox+ID+story(where applicable)+file path; user stories independently testable.
- Graders run and results (PASS/FAIL): checklist-format = PASS; per-story phases = PASS; deps/parallel/MVP sections present = PASS.
- Prompt variant (if applicable): n/a
- Next experiment (smallest change to try): during /sp.implement, confirm the LlamaCppGenerator refactor (T007) keeps llama.cpp output byte-identical before touching the vLLM path.
