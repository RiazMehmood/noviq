# Implementation Plan: Agent Task Executor + Desktop Overlay

**Branch**: `002-agent-task-overlay` | **Date**: 2026-04-19 | **Spec**: [spec.md](spec.md)
**Input**: Feature specification from `specs/002-agent-task-overlay/spec.md`

---

## Summary

Extend the existing EchoAI Tauri process with a second always-on-top overlay window that exposes a local AI agent capable of executing file operations, shell commands, sandboxed code, and web scraping. The agent runs a Rust-native ReAct loop driven by TOML-defined personas, guarded by a Pending Actions Queue, and communicates with the user via both chat and local voice I/O (whisper-rs STT + Kokoro TTS). All state lives in the shared `echo.db`; the overlay window and notebook window coexist in one process sharing one LLM model slot.

---

## Technical Context

**Language/Version**: Rust 1.87 (stable) + TypeScript (SolidJS frontend)
**Primary Dependencies**:
- Tauri 2.0 — multi-window desktop shell
- SolidJS + Tailwind CSS — frontend
- llama-cpp-2 — LLM inference (ReAct loop, task planning)
- whisper-rs — local STT (transcription + wake-word pipeline)
- Kokoro TTS via ONNX Runtime (`ort` crate) — local TTS
- fastembed-rs — embeddings (already present, reused)
- sqlx + sqlite-vec + FTS5 — database
- Wasmtime 20 (WASI preview 1) — sandboxed JS execution (QuickJS.wasm)
- chromiumoxide 0.7 — headless Chromium CDP for web scraping
- tauri-plugin-global-shortcut 2 — global hotkey
- tauri-plugin-tray 2 — system tray
- rodio — audio playback for TTS output
- cpal — microphone capture for STT/voice

**Storage**: SQLite (`echo.db`) — App 2 adds 6 new tables via additive migrations; reads App 1 tables (chunks, sources, notebooks) read-only.

**Testing**: `cargo test` unit tests; Tauri integration tests via Tauri's test harness; manual acceptance testing per spec scenarios.

**Target Platform**: Linux (this phase). Window manager must support always-on-top compositing (X11 or Wayland with XWayland).

**Project Type**: Extension to existing single-crate Tauri project. New modules added under `src-tauri/src/agent/`. No workspace conversion required for this phase — workspace refactor is deferred to when a third independent binary is needed.

**Performance Goals**:
- Overlay shows + accepts input: < 1s from global shortcut press
- Wake word detected → listening state: < 1s after utterance ends
- Voice-to-plan shown (wake word → STT → decomposition): < 5s
- TTS response begins: < 2s after task completion
- First execution step begins: < 3s after user clicks Run
- History search (500+ tasks): < 1s

**Constraints**:
- Zero outbound network — offline-only (except user-provided URLs in tasks)
- Single model slot — at most one LLM in RAM at a time; worker (7B) evicted after 90s idle
- All state in `echo.db` — no separate DB processes
- PAQ gate non-bypassable — DANGEROUS actions require user approval before execution
- Smallest viable diff — new code in `src-tauri/src/agent/`; existing services untouched unless explicitly noted

**Scale/Scope**: Single user, single device, sequential task queue.

---

## Constitution Check

*GATE: Must pass all 8 principles before implementation begins.*

| Principle | Status | Evidence |
|-----------|--------|---------|
| I. Privacy-First, Offline-Always | ✅ PASS | FR-017 blocks all outbound calls; STT/TTS/LLM all local; web fetch only to user-provided URLs |
| II. Tiered Intelligence | ✅ PASS | ReAct loop uses same tiered query router from App 1 (via shared services); task planning starts at Tier 3 (3B orchestrator) |
| III. Single Model Slot | ✅ PASS | Shared Tauri process = shared `ModelSlot`; worker (7B) evicted after 90s idle; spec assumption documents this explicitly |
| IV. SQLite Source of Truth | ✅ PASS | 6 new tables added via migrations to `echo.db`; no separate DB file |
| V. Pending Actions Queue | ✅ PASS | FR-014 mandates PAQ with DANGEROUS/CAUTIOUS/READ-ONLY classification; non-bypassable; `pending_actions` table + Tauri event flow |
| VI. Persona-as-Config | ✅ PASS | FR-028 requires TOML-defined agent tool schema, system prompt, capability list, and safety tier; no hardcoded behaviour |
| VII. Hardware-Adaptive | ✅ PASS | Inherits `hardware_detector.rs` from App 1; voice/code features degrade gracefully if mic/runtime absent; quality notice shown |
| VIII. Incremental Delivery | ✅ PASS | P1 (Task Assignment + Live Progress) is a fully usable MVP; P2–P6 add capability independently |

**Gate result: ALL PASS. Proceed to design.**

---

## Project Structure

### Documentation (this feature)

```text
specs/002-agent-task-overlay/
├── plan.md              ← this file
├── research.md          ← technology decisions
├── data-model.md        ← DB schema + entity relationships
├── quickstart.md        ← how to run and test App 2
├── contracts/
│   ├── tauri-commands.md     ← all invoke() contracts
│   └── tauri-events.md       ← all emit() event contracts
└── tasks.md             ← (created by /sp.tasks — not yet)
```

### Source Code Layout

```text
src-tauri/src/
├── lib.rs                          # existing — add mod agent;
├── commands/                       # existing App 1 commands
│   └── ...
├── services/                       # existing App 1 services (unchanged)
│   ├── hardware_detector.rs
│   ├── llm.rs
│   ├── query_router.rs
│   └── ...
└── agent/                          # NEW — all App 2 code lives here
    ├── mod.rs                      # module root + AgentState
    ├── commands.rs                 # Tauri #[command] handlers for App 2
    ├── react_loop.rs               # ReAct agent loop (Thought→Action→Obs)
    ├── task_manager.rs             # Task lifecycle, queue, status transitions
    ├── paq.rs                      # Pending Actions Queue engine
    ├── rollback.rs                 # Reverse file ops on Cancel
    ├── persona.rs                  # TOML persona loader (FR-028)
    ├── tools/
    │   ├── mod.rs                  # Tool trait + dispatch
    │   ├── file_ops.rs             # FR-010 file operations
    │   ├── shell.rs                # FR-011 shell command executor
    │   ├── code_exec.rs            # FR-012 Wasmtime/WASI + Python/Bash subproc
    │   └── web_scrape.rs           # FR-013 chromiumoxide headless fetch
    └── voice/
        ├── mod.rs
        ├── mic_capture.rs          # cpal microphone capture
        ├── vad.rs                  # Silero VAD (ONNX via ort) — speech detection
        ├── wake_word.rs            # VAD+whisper pipeline for wake-word detection
        ├── stt.rs                  # Full-utterance transcription via whisper-rs
        └── tts.rs                  # Kokoro TTS synthesis + rodio playback

src/                                # SolidJS frontend
├── App.tsx                         # existing — add overlay route
├── components/
│   ├── ...existing notebook...
│   └── overlay/                    # NEW overlay UI components
│       ├── OverlayWindow.tsx        # Root overlay layout
│       ├── TaskInput.tsx            # Chat + voice input bar
│       ├── StepProgress.tsx         # Live step-by-step progress feed
│       ├── PaqApproval.tsx          # Approval dialog for DANGEROUS actions
│       ├── VoiceIndicator.tsx       # Listening/speaking state indicator
│       └── HistoryPanel.tsx         # Task history + replay
└── pages/
    └── Overlay.tsx                  # /overlay route served to overlay window

config/personas/                    # NEW TOML persona definitions (FR-028)
└── agent-default.toml              # default agent persona

src-tauri/
├── Cargo.toml                      # add: wasmtime, ort, chromiumoxide, cpal, rodio,
│                                   #      tauri-plugin-global-shortcut, tauri-plugin-tray
└── tauri.conf.json                 # add: overlay window definition, tray config,
                                    #      global shortcut permissions
```

---

## Architecture Decisions

### AD-001: Single Tauri Process, Two Windows

**Decision**: One Tauri app creates two `WebviewWindow` instances — `"main"` (Notebook) and `"overlay"` (Agent). The overlay uses `always_on_top(true)`, `decorations(false)`, `transparent(true)`, `skip_taskbar(true)`.

**Why chosen**: Eliminates IPC overhead; the existing `ModelSlot` in `llm.rs` is naturally shared as a single `Arc<Mutex<ModelSlot>>`; shared DB connection pool; zero RAM duplication for services.

**Alternative rejected**: Separate processes — would require IPC protocol design and load the LLM model twice, violating Principle III (Single Model Slot).

**Tradeoff**: If the overlay crashes, the notebook also crashes. Mitigated by stable Rust backend; frontend panics are sandboxed to the WebView.

---

### AD-002: ReAct Loop in Rust, No Python Orchestration

**Decision**: Implement a Rust-native ReAct (Reason + Act) agent loop in `agent/react_loop.rs`. The loop formats a prompt with tool schemas from TOML, calls the LLM via `llm.rs`, parses the structured output (Thought / Action / Input), dispatches to the appropriate tool, appends the observation, and repeats until a `Final Answer` token is produced.

**Why chosen**: Constitution Principle VI requires no Python orchestration frameworks. Rust keeps it in-process, low-latency, and memory-safe.

**Alternative rejected**: LangChain/LangGraph Python bridge — violates no-Python constraint; adds IPC overhead; breaks offline guarantee.

**Prompt format**: Standard ReAct template injected with persona TOML's tool list. Max iterations: 20 per task (configurable in persona TOML). Final answer detection: regex on `Final Answer:` prefix.

---

### AD-003: Wake Word Detection — VAD + Whisper-rs Buffer Strategy

**Decision**: Continuous microphone capture via `cpal` → Silero VAD (ONNX via `ort`) detects speech onset → buffer 3-second audio window → run `whisper-rs` transcription on buffer → string-match for wake phrase (configurable, default "EchoAI", case-insensitive, fuzzy match ≥ 0.85 Jaro-Winkler).

**Why chosen**: No pre-trained wake-word engine supports custom phrases offline. Silero VAD is a 1.8 MB ONNX model with < 5 ms inference per 512-sample frame. Whisper on a 3-second buffer runs in < 800 ms on Tier 2+ hardware, staying within SC-009's 1-second target.

**Alternative rejected**: Porcupine Wake Word SDK — proprietary, requires API key for custom wake words, violates Privacy-First principle.

**Tradeoff**: ~1% false positive rate on similar-sounding phrases. Mitigated by the grace-period cancel indicator (edge case documented in spec).

---

### AD-004: Kokoro TTS via ONNX Runtime

**Decision**: Use `ort` (ONNX Runtime Rust bindings) to load the Kokoro TTS ONNX model. Audio synthesis output is played via `rodio`. TTS is initialised lazily on first voice task and evicted from RAM when not needed.

**Why chosen**: Constitution locks TTS to Kokoro (local neural, no cloud). The `ort` crate is already used by `fastembed-rs` in this codebase — ONNX Runtime is already linked. No additional FFI plumbing needed.

**Model**: `kokoro-v0_19.onnx` (~83 MB, Apache 2.0) — stored in `models/` alongside LLM weights.

**Alternative rejected**: espeak — low quality, robotic voice degrades UX. Cloud TTS — violates Principle I.

---

### AD-005: Wasmtime for JS, Subprocess for Python/Bash

**Decision**: JavaScript execution uses Wasmtime WASI with QuickJS compiled to WASM (`qjs.wasm`, ~1 MB). Python and Bash run as restricted subprocesses with a stripped environment (only `PATH=/usr/bin:/bin`, no HOME propagation, `ulimit -v 256000 -t 30`). All three are classified DANGEROUS in persona TOML and require PAQ approval before execution.

**Why chosen**: True WASM sandbox for JS matches constitution lock. Python and Bash cannot be practically sandboxed in WASM for this phase without compiling a full Python interpreter to WASM (CPython.wasm is 50+ MB, still experimental). Subprocess approach with resource limits is the pragmatic choice; it is documented as a known limitation.

**Future**: Replace Python subprocess with CPython WASM when stable. Add seccomp profile to subprocess executor.

---

### AD-006: Persona-as-Config TOML Schema

**Decision**: Each agent persona is defined in a TOML file under `config/personas/`. The schema:

```toml
[persona]
id = "agent-default"
version = "1.0.0"
system_prompt = """..."""
max_iterations = 20

[[tool]]
name = "file_read"
description = "Read file contents"
safety_class = "READ_ONLY"
params = [{ name = "path", type = "string", required = true }]

[[tool]]
name = "shell_exec"
description = "Run a shell command"
safety_class = "DANGEROUS"
params = [{ name = "command", type = "string", required = true }]
```

Safety classes map directly to PAQ behaviour: `READ_ONLY` → immediate, `CAUTIOUS` → brief confirm, `DANGEROUS` → PAQ approval gate.

---

## Implementation Phases

### Phase 1 — Foundation & P1 MVP (Task Assignment + Live Progress)

**Goal**: Overlay opens via shortcut, user types a task, agent decomposes it, user approves, execution runs step-by-step with live progress. Chat input only (no voice). File/shell tools only.

| # | Task | Files |
|---|------|-------|
| T001 | DB migrations: 6 new tables (agent_tasks, agent_steps, pending_actions, execution_log, agent_personas, rollback_log) | `src-tauri/src/db/migrations/` |
| T002 | Overlay window config (always-on-top, transparent, skip-taskbar, global shortcut) | `tauri.conf.json`, `src-tauri/src/lib.rs` |
| T003 | System tray icon + show/hide overlay menu | `src-tauri/src/agent/commands.rs`, `tauri.conf.json` |
| T004 | Persona TOML loader (`agent/persona.rs`) | `src-tauri/src/agent/persona.rs`, `config/personas/agent-default.toml` |
| T005 | PAQ engine: `pending_actions` table + approve/reject flow + Tauri events | `src-tauri/src/agent/paq.rs` |
| T006 | Task manager: submit/queue/status transitions + `agent_tasks`/`agent_steps` writes | `src-tauri/src/agent/task_manager.rs` |
| T007 | ReAct loop: LLM prompt format + tool dispatch + iteration cap + Final Answer parse | `src-tauri/src/agent/react_loop.rs` |
| T008 | File ops tool (create/read/write/move/copy/rename/delete/listdir) | `src-tauri/src/agent/tools/file_ops.rs` |
| T009 | Shell tool (stdout/stderr/exit capture, env strip, timeout 60s) | `src-tauri/src/agent/tools/shell.rs` |
| T010 | Rollback engine (reverse file ops on Cancel, irreversible tagging) | `src-tauri/src/agent/rollback.rs` |
| T011 | Tauri commands: submit_task, pause/resume/cancel, get_task, list_tasks, decide_action | `src-tauri/src/agent/commands.rs` |
| T012 | Frontend: OverlayWindow layout + TaskInput + StepProgress + PaqApproval | `src/components/overlay/`, `src/pages/Overlay.tsx` |
| T013 | Frontend: real-time step updates via `agent:step-*` events | `src/components/overlay/StepProgress.tsx` |
| T014 | Overlay position/size persistence (saved to `echo.db` settings table) | `src-tauri/src/agent/commands.rs` |

**P1 Acceptance**: Submit "List all PDF files in ~/Documents" → see decomposed plan → click Run → steps show live → result displays in overlay. Pause/Resume/Cancel functional.

---

### Phase 2 — Voice Interaction (P2)

**Goal**: Wake word activates overlay, user speaks task, result read aloud via TTS.

| # | Task | Files |
|---|------|-------|
| T015 | Microphone capture (cpal continuous ring buffer) | `src-tauri/src/agent/voice/mic_capture.rs` |
| T016 | Silero VAD ONNX integration (speech onset detection) | `src-tauri/src/agent/voice/vad.rs` |
| T017 | Wake-word pipeline (VAD + whisper-rs buffer + fuzzy phrase match) | `src-tauri/src/agent/voice/wake_word.rs` |
| T018 | Full-utterance STT pipeline (post-wake whisper-rs transcription) | `src-tauri/src/agent/voice/stt.rs` |
| T019 | Kokoro TTS ONNX model integration (`ort` crate + rodio playback) | `src-tauri/src/agent/voice/tts.rs` |
| T020 | Voice mode settings (voice-only/chat-only/both) + graceful mic-absent degradation | `src-tauri/src/agent/commands.rs` |
| T021 | Frontend: VoiceIndicator (listening/speaking states) + input mode UI adaptation | `src/components/overlay/VoiceIndicator.tsx` |
| T022 | Cargo.toml: add `ort`, `cpal`, `rodio` dependencies | `src-tauri/Cargo.toml` |
| T023 | Download Silero VAD + Kokoro TTS ONNX models to `models/` | `models/silero_vad.onnx`, `models/kokoro-v0_19.onnx` |

**P2 Acceptance**: Say "EchoAI, list files in my Desktop" → overlay activates within 1s → task executes → result read aloud within 2s.

---

### Phase 3 — Code Execution (P3)

**Goal**: Agent writes Python/JS/Bash scripts, user sees code before execution, sandbox runs it.

| # | Task | Files |
|---|------|-------|
| T024 | Wasmtime dependency + WASI runtime setup | `src-tauri/Cargo.toml`, `src-tauri/src/agent/tools/code_exec.rs` |
| T025 | QuickJS.wasm bundled + JS execution via Wasmtime | `src-tauri/src/agent/tools/code_exec.rs`, `src-tauri/assets/qjs.wasm` |
| T026 | Python subprocess executor (ulimit, stripped env, 60s timeout, stdout/stderr capture) | `src-tauri/src/agent/tools/code_exec.rs` |
| T027 | Bash subprocess executor (same constraints as Python) | `src-tauri/src/agent/tools/code_exec.rs` |
| T028 | Auto-retry on script failure (3 retries: LLM re-generates fix + re-runs) | `src-tauri/src/agent/react_loop.rs` |
| T029 | Frontend: show generated code in overlay before execution + allow edit | `src/components/overlay/StepProgress.tsx` |

---

### Phase 4 — Web Content Extraction (P4)

**Goal**: Agent fetches and extracts structured data from user-provided URLs via local headless browser.

| # | Task | Files |
|---|------|-------|
| T030 | chromiumoxide dependency + CDP connection setup | `src-tauri/Cargo.toml`, `src-tauri/src/agent/tools/web_scrape.rs` |
| T031 | Page fetch with JS DOM stabilization wait (max 15s timeout) | `src-tauri/src/agent/tools/web_scrape.rs` |
| T032 | LLM-driven data extraction from page DOM/text | `src-tauri/src/agent/tools/web_scrape.rs` |
| T033 | Graceful degradation when system Chromium not found | `src-tauri/src/agent/tools/web_scrape.rs` |

---

### Phase 5 — Task History & Replay (P6)

**Goal**: History panel shows all past tasks with full logs; user can search and replay.

| # | Task | Files |
|---|------|-------|
| T034 | History panel: task list with step drill-down + output display | `src/components/overlay/HistoryPanel.tsx` |
| T035 | FTS5 keyword search across `agent_tasks.instruction` + `agent_steps.output_text` | `src-tauri/src/agent/commands.rs` |
| T036 | Replay: pre-fill instruction from history → new task submission | `src/components/overlay/HistoryPanel.tsx` |
| T037 | History retention enforcement (auto-purge at limit) + export | `src-tauri/src/agent/task_manager.rs` |

---

### Phase 6 — Settings & Polish

| # | Task | Files |
|---|------|-------|
| T038 | Settings screen: wake word, input mode, history limit, allowed paths, overlay startup | `src/components/overlay/` |
| T039 | Allowed-paths enforcement in file_ops + shell tool | `src-tauri/src/agent/tools/file_ops.rs`, `shell.rs` |
| T040 | Hardware-adaptive feature flags (disable voice if no mic, code exec if no runtime) | `src-tauri/src/agent/mod.rs` |
| T041 | Installer component selection flag (App 2 optional in tauri bundler config) | `tauri.conf.json` |

---

## Non-Functional Considerations

### Observability
- All task state changes write to `execution_log` table (audit trail)
- Tauri events (`agent:task-updated`, `agent:step-*`) are the observable surface for frontend + testing
- `env_logger` spans for ReAct loop iteration timing

### Security
- PAQ gate is the primary security boundary; tool safety classes defined in TOML
- Subprocess executors strip environment variables and apply resource limits
- Allowed-paths restriction (optional, off by default) provides additional containment
- No eval() in frontend; all task execution is Rust-side

### Rollback Strategy
- File ops register undo entries in `rollback_log` before executing
- On Cancel: rollback engine processes entries in reverse order
- Shell/code/web steps: tagged irreversible; history records full command + output

### Dependencies Not Yet in Cargo.toml (to add in T022, T024, T030)

| Crate | Version | Purpose |
|-------|---------|---------|
| `tauri-plugin-global-shortcut` | 2 | Global keyboard shortcut |
| `tauri-plugin-tray` | 2 | System tray |
| `ort` | 2 | ONNX Runtime (Kokoro TTS + Silero VAD) |
| `cpal` | 0.15 | Microphone capture |
| `rodio` | 0.19 | Audio playback |
| `wasmtime` | 20 | WASI sandbox for JS |
| `chromiumoxide` | 0.7 | Headless Chromium CDP |
| `strsim` | 0.11 | Jaro-Winkler fuzzy match for wake word |

---

## Complexity Tracking

> No Constitution violations. All 8 principles pass. No justifications required.

---

## ADR Suggestions

📋 **Architectural decision detected: Single process two-window architecture for App 1 + App 2** — Document shared model slot, window lifecycle, and crash boundary tradeoffs. Run `/sp.adr single-process-two-window-arch`

📋 **Architectural decision detected: ReAct loop in Rust with TOML-defined personas** — Document tool dispatch pattern, iteration cap, and Python-orchestration rejection. Run `/sp.adr react-loop-rust-persona-toml`

📋 **Architectural decision detected: VAD + whisper-rs wake-word strategy** — Document why no pre-trained wake-word SDK, Silero VAD choice, and latency budget. Run `/sp.adr vad-whisper-wake-word`
