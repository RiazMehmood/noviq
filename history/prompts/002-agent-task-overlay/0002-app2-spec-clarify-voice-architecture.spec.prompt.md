---
id: "0002"
title: "App2 Spec Clarify Voice Architecture"
stage: spec
date: 2026-04-19
surface: agent
model: claude-sonnet-4-6
feature: 002-agent-task-overlay
branch: 002-agent-task-overlay
user: Riaz Mehmood
command: /sp.clarify
labels: ["spec","clarify","voice","architecture","overlay"]
links:
  spec: specs/002-agent-task-overlay/spec.md
  ticket: null
  adr: null
  pr: null
files:
  - specs/002-agent-task-overlay/spec.md
  - specs/002-agent-task-overlay/checklists/requirements.md
tests:
  - none
---

## Prompt

/sp.clarify on 002-agent-task-overlay spec. User answers:
Q1 (architecture): Single Tauri process, two windows, shared model slot, shared DB (different tables), Cargo workspace, unified installer, App 2 autostart default, App 1 optional. IPC model server must be IN scope — solved by single process (no IPC needed). Loose coupling = per-window show/hide, not per-process.
Q2 (plan approval): Option A — explicit "Run" button before any step executes.
Q3 (rollback): Option B + history — file ops rolled back; shell side-effects logged as irreversible for user review.
Q4 (history retention): Option D — configurable, default 500 tasks, unlimited option in Settings.
Q5 (startup visibility): Option C — configurable, default hidden (tray only).
Additional: Voice interaction — wake word "EchoAI", local STT (whisper-rs reuse), local TTS response, voice+chat both as equivalent input modes.

## Response snapshot

All 5 clarification questions answered and integrated into spec.md. Voice interaction added as new User Story 5 (P2), promoted above history/replay. 6 new FRs (FR-022–FR-027), 4 new SCs (SC-009–SC-012), voice edge cases, TTS/STT assumptions, Task entity updated. Architecture clarified: single Tauri process + two windows = no IPC needed + shared RAM for LLM.

## Outcome

- ✅ Impact: Spec fully clarified; all critical ambiguities resolved; ready for /sp.plan
- 🧪 Tests: none at spec stage
- 📁 Files: specs/002-agent-task-overlay/spec.md (updated), checklists/requirements.md
- 🔁 Next prompts: /sp.plan
- 🧠 Reflection: Voice requirement added mid-clarification — significant feature that reuses whisper-rs from App 1. Single-process architecture elegantly solves the RAM/IPC conflict.

## Evaluation notes (flywheel)

- Failure modes observed: none
- Graders run and results (PASS/FAIL): n/a
- Prompt variant (if applicable): n/a
- Next experiment (smallest change to try): /sp.plan
