# Phase 0 Research: Agent Task Executor + Desktop Overlay

**Branch**: `002-agent-task-overlay` | **Date**: 2026-04-19

---

## R-001: Tauri 2.0 Multi-Window Always-On-Top Overlay

**Decision**: Use `WebviewWindowBuilder` with `always_on_top(true)`, `decorations(false)`, `transparent(true)`, `skip_taskbar(true)` for the overlay window. Global shortcut registered via `tauri-plugin-global-shortcut`.

**Rationale**: Tauri 2.0 fully supports multiple named windows in one process. The overlay window config maps directly to all spec requirements (FR-001, FR-002, FR-003, FR-005). Position/size persisted via `echo.db` settings table (reuses existing pattern from App 1).

**Alternatives considered**:
- Separate OS window manager tooltip/widget — not supported cross-platform in Tauri, no Rust control surface
- Electron overlay — wrong tech stack; constitution locks Tauri

**Key APIs**:
```rust
WebviewWindowBuilder::new(&app, "overlay", WebviewUrl::App("/overlay".into()))
    .always_on_top(true)
    .decorations(false)
    .transparent(true)
    .skip_taskbar(true)
    .visible(false)          // hidden at start per FR-027
    .build()?;
```

---

## R-002: ReAct Agent Loop Pattern

**Decision**: Implement a Rust-native ReAct loop. Format: `Thought:` (LLM internal reasoning) → `Action:` (tool name) → `Action Input:` (JSON params) → `Observation:` (tool result appended by loop) → repeat until `Final Answer:` detected.

**Rationale**: ReAct is the standard pattern for tool-using agents with a single LLM. It requires no additional orchestration library — just prompt templating + string parsing + a dispatch table. Aligns with Constitution Principle VI (no Python frameworks).

**Prompt structure**:
```
<system_prompt from persona TOML>

Tools available:
{tool_name}: {description}
  Parameters: {params}
  Safety: {safety_class}

Use this format:
Thought: <reasoning>
Action: <tool_name>
Action Input: <json>
Observation: <will be filled by system>
...
Final Answer: <result>

Task: {user_instruction}
```

**Iteration cap**: 20 (configurable per persona TOML). Hard stops at cap; surfaces partial result + "Agent reached iteration limit" to user.

**Alternatives considered**:
- LangChain Python bridge — violates no-Python constraint; violates offline principle if cloud LangSmith is enabled
- Function calling / JSON mode — requires model-specific format; llama-cpp-2 supports it but ReAct is more model-agnostic for Qwen2.5

---

## R-003: Wake Word Detection Strategy

**Decision**: Continuous mic capture (cpal) → Silero VAD ONNX (speech onset) → 3-second audio buffer → whisper-rs transcription → Jaro-Winkler fuzzy match (≥ 0.85) against configured wake phrase.

**Rationale**: No offline, open-source wake-word engine supports arbitrary custom phrases. Silero VAD (1.8 MB ONNX) achieves 95%+ precision on speech/silence classification at 16 kHz with 512-sample frames (~32 ms per frame, < 5 ms inference). Whisper on a 3-second window (48 kB WAV) runs in ~400–700 ms on Tier 2 hardware. Total pipeline: < 1s from end of utterance → matches SC-009.

**Model**: `silero_vad.onnx` — Apache 2.0, 1.8 MB, single ONNX file, runs via `ort` crate.

**Fuzzy match**: Jaro-Winkler via `strsim` crate. Threshold 0.85 balances false-positive suppression against accent tolerance.

**Alternatives considered**:
- Porcupine SDK — proprietary, requires API key for custom wake words, violates Privacy-First
- Vosk small model — 40 MB, lower accuracy than whisper on short phrases, separate ONNX toolchain
- Always-on full whisper transcription — too CPU-intensive; would burn CPU continuously

---

## R-004: Kokoro TTS Integration

**Decision**: `ort` crate (ONNX Runtime Rust bindings v2) loads `kokoro-v0_19.onnx`; output PCM samples played via `rodio`.

**Rationale**: `ort` is already transitively present via `fastembed-rs` (fastembed uses ONNX Runtime). Kokoro ONNX produces 24 kHz mono PCM. The `rodio` crate handles audio sink management cleanly. Total model size: 83 MB (Apache 2.0). Synthesis of a typical 20-word response: < 1.5s on Tier 2+ hardware — comfortably within SC-011's 2-second target.

**Model location**: `models/kokoro-v0_19.onnx` (downloaded by user; same pattern as LLM weights).

**Alternatives considered**:
- espeak — robotic quality, degrades UX significantly; constitution explicitly names Kokoro TTS
- Coqui TTS — heavier (300 MB+), Python dependency, abandoned upstream
- System TTS (dbus org.freedesktop.speech-dispatcher) — no guarantee of quality or availability; cloud-proxied on some distros

---

## R-005: Wasmtime / WASI for Code Execution

**Decision**: JavaScript via Wasmtime + `qjs.wasm` (QuickJS compiled to WASM preview 1, ~1 MB). Python and Bash via restricted subprocess (`Command::new` with stripped env + `ulimit` wrapper).

**Rationale**: QuickJS compiles cleanly to WASI preview 1 and runs deterministically in Wasmtime. JS is the primary "glue" language for data manipulation tasks. Python and Bash cannot be practically sandboxed via WASM for this phase (CPython WASM is still experimental at ~60 MB). Subprocess restriction (stripped env, `RLIMIT_AS=256MB`, `RLIMIT_CPU=30s`) is the pragmatic Linux-native alternative. Both Python/Bash steps are classified DANGEROUS in persona TOML → PAQ gate required before execution.

**Resource limits for subprocess**:
```rust
// Applied via setrlimit before exec
RLIMIT_AS  = 256 MB  // virtual address space cap
RLIMIT_CPU = 30 s    // CPU time hard limit
RLIMIT_NOFILE = 64   // file descriptor cap
env: PATH=/usr/bin:/bin only (no HOME, no DBUS_SESSION, etc.)
```

**Alternatives considered**:
- nsjail / bubblewrap — more robust Linux sandbox; adds a system dependency; deferred to future phase
- Full CPython WASM — ~60 MB experimental; not production-ready as of 2026
- Docker/container — too heavy; violates lightweight-app UX requirement

---

## R-006: Chromium Headless Web Scraping

**Decision**: `chromiumoxide` 0.7 (CDP client) with system-installed Chromium. Launches `chromium --headless --remote-debugging-port=0` subprocess; connects via WebSocket CDP.

**Rationale**: chromiumoxide is the most maintained pure-Rust CDP client. Full JS rendering supported. System Chromium (pre-installed on most Linux desktops) avoids bundling a 150 MB browser. Feature gracefully disabled with clear message if Chromium not found (hardware-adaptive, Principle VII).

**Alternatives considered**:
- fantoccini (geckodriver/Firefox) — less maintained, larger deps, similar pattern
- Bundled Chromium — 150 MB added to installer; acceptable for future phase only
- HTTP-only fetch (reqwest) — can't render JS-heavy pages; fails SC for modern sites

---

## R-007: Pending Actions Queue (PAQ) Design

**Decision**: `pending_actions` table in SQLite. When a DANGEROUS/CAUTIOUS tool call is about to execute, a row is inserted and a `agent:approval-needed` Tauri event fired to frontend. Frontend renders the approval dialog. User clicks Approve/Reject → `decide_action` Tauri command → DB update → ReAct loop unblocks.

**Rationale**: DB-backed PAQ is durable: if the app crashes mid-approval, the pending action is recoverable. Tauri events (not polling) deliver sub-100ms latency from action queued to dialog shown.

**State machine**:
```
Queued → AwaitingApproval → Approved → Executing → (step done)
                         ↘ Rejected → (step skipped, task cancelled)
```

**Alternatives considered**:
- In-memory channel only — not durable; pending approvals lost on crash
- Modal OS dialog — blocks the ReAct loop thread; no cancel timeout possible

---

## R-008: Persona TOML Schema

**Decision**: TOML files under `config/personas/`. Schema includes `[persona]` header (id, version, system_prompt, max_iterations) and `[[tool]]` array (name, description, safety_class, params). The Rust `persona.rs` module deserializes via `serde` + `toml` crate.

**Rationale**: TOML is human-readable, Rust-native (first-class serde support), and trivially extensible — adding a tool requires only a new `[[tool]]` block with no code change (FR-028, Constitution Principle VI).

**Crate**: `toml` 0.8 (already transitively present via Tauri build tooling; add explicit dependency).

**Alternatives considered**:
- JSON — less readable for humans editing persona files; TOML standard for Rust config
- YAML — Rust YAML parsers are heavier; TOML preferred for Rust ecosystem

---

## All NEEDS CLARIFICATION Items: RESOLVED

| Item | Resolution |
|------|-----------|
| App architecture (separate vs shared process) | Single Tauri process, two windows (clarification session 2026-04-19) |
| IPC mechanism | Not needed — shared Rust service instances in same process |
| Code execution sandbox | Wasmtime/WASI for JS; restricted subprocess for Python/Bash |
| TTS engine | Kokoro TTS (ONNX, constitution-locked) |
| Wake word approach | VAD + whisper-rs buffer pipeline |
| History retention default | 500 tasks, configurable, unlimited option |
| PAQ approval flow | Explicit approve/reject per action; DB-backed |
| Overlay startup default | Hidden (tray-only); opt-in to start visible in Settings |
