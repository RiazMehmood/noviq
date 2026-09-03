---
id: "0004"
title: "App 2 Four Architecture Decision Records"
stage: plan
date: 2026-04-19
surface: agent
model: claude-sonnet-4-6
feature: 002-agent-task-overlay
branch: 002-agent-task-overlay
user: Riaz Mehmood
command: /sp.adr single-process-two-windows-arch
labels: ["plan","adr","architecture","agent","overlay","voice","sandbox"]
links:
  spec: specs/002-agent-task-overlay/spec.md
  ticket: null
  adr: history/adr/0003-single-process-two-window-app-integration-architecture.md
  pr: null
files:
  - history/adr/0003-single-process-two-window-app-integration-architecture.md
  - history/adr/0004-react-agent-loop-with-toml-persona-configuration.md
  - history/adr/0005-offline-voice-io-pipeline-vad-whisper-kokoro-tts.md
  - history/adr/0006-code-execution-sandbox-wasmtime-and-subprocess.md
tests:
  - none at plan/adr stage
---

## Prompt

/sp.adr single-process-two-windows-arch

## Response snapshot

Ran /sp.adr workflow. Extracted 6 architectural decisions from plan.md (AD-001–AD-006), clustered into 4 ADRs (none overlapped existing ADR-0001/0002 which cover App 1 LLM hosting and embedding strategy). All 4 clusters passed significance test (cross-cutting impact, alternatives documented, likely to be revisited).

- **ADR-0003**: Single-Process Two-Window App Integration Architecture — one Tauri process, two WebviewWindows sharing ModelSlot + echo.db + services. Rejected: separate processes (double RAM), sidecar (IPC overhead), workspace-now (no benefit yet).
- **ADR-0004**: ReAct Agent Loop with TOML Persona Configuration — Rust ReAct loop reads TOML tool schema + safety_class; no Python orchestration. Rejected: LangChain (violates Principle VI + I), JSON grammar mode (deferred), hardcoded prompts (violates Principle VI).
- **ADR-0005**: Offline Voice I/O Pipeline — Silero VAD + whisper-rs wake word (3s buffer, fuzzy match) + Kokoro TTS via ort. Rejected: Porcupine (proprietary, cloud), Vosk (larger, separate toolchain), espeak (constitution-locked to Kokoro), always-on whisper (CPU burn).
- **ADR-0006**: Code Execution Sandbox — Wasmtime/WASI + QuickJS.wasm for JS; setrlimit subprocess for Python/Bash; all DANGEROUS-class in PAQ. Rejected: CPython WASM (experimental, 60 MB), Docker (not desktop-installable), no restrictions (safety floor).

## Outcome

- ✅ Impact: 4 ADRs created documenting all major App 2 architectural decisions; team can review before /sp.tasks
- 🧪 Tests: none
- 📁 Files: history/adr/0003–0006
- 🔁 Next prompts: /sp.tasks to generate T001–T041 with acceptance criteria
- 🧠 Reflection: ADR-0003 is the most consequential — it extends ADR-0001's ModelSlot decision to the multi-window topology. ADR-0005 notes the whisper-rs/llama-cpp-2 ggml symbol conflict as a known blocker for voice features.

## Evaluation notes (flywheel)

- Failure modes observed: none
- Graders run and results (PASS/FAIL): all 4 ADRs pass significance checklist (cross-cutting, alternatives, tradeoffs documented)
- Prompt variant (if applicable): n/a
- Next experiment (smallest change to try): /sp.tasks decomposition
