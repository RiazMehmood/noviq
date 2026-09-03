---
id: "0003"
title: "App 2 Agent Overlay Architecture Plan"
stage: plan
date: 2026-04-19
surface: agent
model: claude-sonnet-4-6
feature: 002-agent-task-overlay
branch: 002-agent-task-overlay
user: Riaz Mehmood
command: /sp.plan
labels: ["plan","agent","overlay","react-loop","paq","voice","tauri","architecture"]
links:
  spec: specs/002-agent-task-overlay/spec.md
  ticket: null
  adr: null
  pr: null
files:
  - specs/002-agent-task-overlay/plan.md
  - specs/002-agent-task-overlay/research.md
  - specs/002-agent-task-overlay/data-model.md
  - specs/002-agent-task-overlay/contracts/tauri-commands.md
  - specs/002-agent-task-overlay/contracts/tauri-events.md
  - specs/002-agent-task-overlay/quickstart.md
tests:
  - none at plan stage
---

## Prompt

sync specs with constitute also check any gaps, then go for /sp.plan

## Response snapshot

Spec synced with constitution (7 gaps fixed: removed stale Out-of-Scope voice entry, fixed FR-020/021 ordering, added PAQ FR-014 alignment, added FR-028 Persona-as-Config, replaced espeak with Kokoro TTS, replaced isolated subprocess with Wasmtime/WASI, added 90s model eviction to assumptions). Then ran /sp.plan and generated full architecture plan:

- **plan.md**: Technical context, Constitution Check (all 8 principles PASS), architecture decisions AD-001–AD-006, 6 implementation phases (T001–T041), dependency additions
- **research.md**: 8 research decisions resolved (Tauri multi-window, ReAct loop, VAD+whisper wake-word, Kokoro TTS via ort, Wasmtime+subprocess, chromiumoxide, PAQ design, Persona TOML schema)
- **data-model.md**: 6 new SQLite tables (agent_tasks, agent_steps, pending_actions, execution_log, rollback_log, agent_personas) with full SQL DDL, FTS5 virtual tables, and validation rules
- **contracts/tauri-commands.md**: 18 Tauri invoke() contracts across task management, PAQ, voice, overlay state, and settings
- **contracts/tauri-events.md**: 12 emit() event contracts for real-time step progress, approvals, voice pipeline, overlay activation
- **quickstart.md**: P1–P4 test flows, model download steps, settings reference, troubleshooting table

Key architectural decisions:
- Single Tauri process, two windows (AD-001) — shares ModelSlot, no IPC needed
- Rust-native ReAct loop, TOML personas (AD-002, AD-006) — no Python orchestration
- VAD + whisper-rs wake word pipeline (AD-003) — fully offline, fuzzy match
- Kokoro TTS via ort crate (AD-004) — reuses ONNX Runtime already linked via fastembed
- Wasmtime for JS, restricted subprocess for Python/Bash (AD-005)
- DB-backed PAQ with Tauri event unblocking (AD-007)

## Outcome

- ✅ Impact: Full architecture plan for App 2; all 41 tasks identified across 6 phases; ready for /sp.tasks
- 🧪 Tests: none at plan stage
- 📁 Files: plan.md, research.md, data-model.md, contracts/tauri-commands.md, contracts/tauri-events.md, quickstart.md
- 🔁 Next prompts: /sp.tasks → generate T001–T041 with acceptance criteria → begin implementation from T001
- 🧠 Reflection: Single-process two-window architecture is the clean choice — avoids IPC and model duplication. PAQ + TOML personas are the two highest-leverage constitution-compliance decisions.

## Evaluation notes (flywheel)

- Failure modes observed: none
- Graders run and results (PASS/FAIL): n/a
- Prompt variant (if applicable): n/a
- Next experiment (smallest change to try): run /sp.tasks to decompose T001–T041 with full acceptance criteria
