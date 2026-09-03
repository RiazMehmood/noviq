---
id: "0005"
title: "App 2 Agent Overlay 58 Tasks"
stage: tasks
date: 2026-04-19
surface: agent
model: claude-sonnet-4-6
feature: 002-agent-task-overlay
branch: 002-agent-task-overlay
user: Riaz Mehmood
command: /sp.tasks
labels: ["tasks","agent","overlay","react-loop","voice","sandbox","tauri","solidjs"]
links:
  spec: specs/002-agent-task-overlay/spec.md
  ticket: null
  adr: null
  pr: null
files:
  - specs/002-agent-task-overlay/tasks.md
tests:
  - none at tasks stage
---

## Prompt

/sp.tasks

## Response snapshot

Generated tasks.md with 58 tasks across 9 phases for feature 002-agent-task-overlay:

- **Phase 1 (Setup, 5 tasks)**: Cargo.toml deps, tauri.conf.json overlay window, SolidJS /overlay route, agent/ module skeleton, persona TOML skeleton
- **Phase 2 (Foundation, 5 tasks)**: DB migrations (6 tables + 2 FTS5 virtual tables), persona TOML loader, AgentState managed state, Tool trait + ToolRegistry
- **Phase 3 (US1 P1 MVP, 13 tasks)**: PAQ engine, task manager, file READ_ONLY tools, ReAct loop (LLM + parse + dispatch), 3 Tauri command groups, 4 frontend components (OverlayWindow, TaskInput, StepProgress, PaqApproval), overlay persistence
- **Phase 4 (US2 P2, 5 tasks)**: File write/move/delete/rename tools + rollback engine + shell executor
- **Phase 5 (US5 P2 Voice, 10 tasks)**: mic capture, Silero VAD, wake word pipeline, full-utterance STT, Kokoro TTS, voice commands, VoiceIndicator frontend — includes ⚠️ whisper-rs/llama-cpp-2 ggml link conflict blocker
- **Phase 6 (US3 P3, 6 tasks)**: Wasmtime/WASI + QuickJS.wasm, Python subprocess, Bash subprocess, auto-retry, code display frontend
- **Phase 7 (US4 P4, 4 tasks)**: chromiumoxide, headless Chromium CDP, LLM extraction, graceful degradation
- **Phase 8 (US6 P6, 4 tasks)**: HistoryPanel, FTS search, replay, retention enforcement
- **Phase 9 (Polish, 6 tasks)**: Settings screen, allowed-paths enforcement, hardware-adaptive flags, installer config

MVP = Phases 1–3 (23 tasks). Phases 4–8 parallelizable after Phase 3 acceptance.

## Outcome

- ✅ Impact: 58 tasks fully decomposed; Phase 3 = independently testable P1 MVP; all tasks have exact file paths
- 🧪 Tests: no TDD tasks generated (not requested in spec)
- 📁 Files: specs/002-agent-task-overlay/tasks.md
- 🔁 Next prompts: Begin implementation starting T001 — or run /sp.tasks with --tdd flag for test stubs
- 🧠 Reflection: Key blocker flagged: whisper-rs + llama-cpp-2 ggml symbol conflict must be resolved before Phase 5 (voice). Phases 4–8 can proceed in parallel after Phase 3 MVP acceptance.

## Evaluation notes (flywheel)

- Failure modes observed: none
- Graders run and results (PASS/FAIL): format validated — all 58 tasks have checkbox + ID + labels + file paths
- Prompt variant (if applicable): n/a
- Next experiment (smallest change to try): begin T001 implementation
