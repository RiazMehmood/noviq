# Tasks: Agent Task Executor + Desktop Overlay

**Input**: Design documents from `specs/002-agent-task-overlay/`
**Feature branch**: `002-agent-task-overlay`
**Generated**: 2026-04-19

**Prerequisites complete**: plan.md ✅ | spec.md ✅ | data-model.md ✅ | contracts/ ✅ | research.md ✅

**Tests**: No TDD tasks generated — not requested in spec. Manual acceptance tests per story are listed in spec.md and quickstart.md.

**Format**: `[ID] [P?] [Story?] Description — file path`

- **[P]**: Parallelizable with other [P] tasks in same phase (different files, no blocking deps)
- **[Story]**: User story this task belongs to (US1–US6)

---

## Phase 1: Setup (Project Infrastructure)

**Purpose**: Add new dependencies, Tauri config, and module skeletons. No business logic yet.

- [ ] T001 Add tauri-plugin-global-shortcut 2 + tauri-plugin-tray 2 to dependencies in `src-tauri/Cargo.toml`
- [ ] T002 Add overlay window definition (always_on_top, decorations=false, transparent, skip_taskbar, visible=false, url="/overlay") to `src-tauri/tauri.conf.json`; add globalShortcut + tray permissions
- [ ] T003 Add /overlay route in `src/App.tsx`; create `src/pages/Overlay.tsx` with empty root div
- [ ] T004 Create `src-tauri/src/agent/` module: `mod.rs` (AgentState struct placeholder), `commands.rs` (empty), `paq.rs` (empty), `task_manager.rs` (empty), `persona.rs` (empty), `react_loop.rs` (empty), `rollback.rs` (empty), `tools/mod.rs` (Tool trait skeleton), `voice/mod.rs` (empty)
- [ ] T005 [P] Create `config/personas/agent-default.toml` with full TOML schema: [persona] block (id, version, system_prompt, max_iterations=20) + [[tool]] entries for all 10 tools (file_read, file_list, file_write, file_move, file_delete, shell_exec, code_exec_js, code_exec_python, code_exec_bash, web_fetch) with safety_class per tool

**Checkpoint**: `cargo check` passes. Overlay window opens to empty page via shortcut.

---

## Phase 2: Foundation (Blocking Prerequisites)

**Purpose**: DB schema + persona loading + state wiring. Nothing in Phase 3+ can run without this.

**⚠️ CRITICAL**: All Phase 3+ tasks depend on this phase being complete.

- [ ] T006 Write DB migration: create `agent_tasks` table + `agent_tasks_fts` FTS5 virtual table + `agent_steps` table + `agent_steps_fts` FTS5 virtual table — `src-tauri/src/db/migrations/<next_number>_agent_tables.sql`
- [ ] T007 [P] Write DB migration: create `pending_actions`, `execution_log`, `rollback_log`, `agent_personas` tables — `src-tauri/src/db/migrations/<next_number+1>_agent_support_tables.sql`
- [ ] T008 Implement persona TOML loader: deserialize via `serde` + `toml` crate; validate every [[tool]] name against known dispatch keys at startup; cache loaded persona in `agent_personas` table — `src-tauri/src/agent/persona.rs`
- [ ] T009 Define `AgentState` struct (holds `TaskManager`, `PaqEngine`, `PersonaConfig` as `Arc<Mutex<_>>`); register in Tauri `manage()` in `src-tauri/src/lib.rs`
- [ ] T010 Define `Tool` trait (`fn name() -> &str`, `async fn execute(&self, input: Value) -> ToolResult`) + `ToolRegistry` dispatch map (tool name → `Box<dyn Tool>`) — `src-tauri/src/agent/tools/mod.rs`

**Checkpoint**: `cargo test` passes. DB migrations run without error. Persona TOML loads and logs all 10 tool names at startup.

---

## Phase 3: User Story 1 — Task Assignment & Live Progress (Priority: P1) 🎯 MVP

**Goal**: Overlay activates via shortcut, user types a task, agent decomposes it into steps, user approves the plan, steps execute live with progress shown in overlay. File read/list tools available for acceptance test.

**Independent Test**: Press `Ctrl+Shift+Space` → overlay appears → type "List all PDF files in ~/Documents" → agent shows plan → click Run → steps execute → result shows within 5s. Pause/Resume/Cancel functional.

- [ ] T011 [US1] Implement PAQ engine: `insert_pending_action()` → status `awaiting`; `decide_action(id, Approve|Reject)` → status update → unblock waiting loop; emit `agent:approval-needed` Tauri event on insert — `src-tauri/src/agent/paq.rs`
- [ ] T012 [US1] Implement task manager: `submit_task(instruction, input_mode)` → insert agent_tasks row (status: queued) → assign queue_position; status machine: queued → planning → awaiting_approval → running ⇄ paused → completed/failed/cancelled; write execution_log on every transition — `src-tauri/src/agent/task_manager.rs`
- [ ] T013 [US1] [P] Implement READ_ONLY file tools: `file_read` (read file to string) + `file_list` (glob directory listing with optional filter) — register in ToolRegistry — `src-tauri/src/agent/tools/file_ops.rs`
- [ ] T014 [US1] Implement ReAct loop: format prompt (system_prompt + tool schema from PersonaConfig + task instruction + history); call `llm.rs` generate(); parse `Thought:` / `Action:` / `Action Input:` / `Final Answer:` tokens via regex; dispatch action to ToolRegistry; append `Observation:`; loop max 20 iterations; emit `agent:step-started` + `agent:step-completed` events per tool call — `src-tauri/src/agent/react_loop.rs`
- [ ] T015 [US1] Implement Tauri commands: `agent_submit_task`, `agent_get_task` (with steps), `agent_list_tasks` (status filter + pagination) — `src-tauri/src/agent/commands.rs`
- [ ] T016 [US1] [P] Implement Tauri commands: `agent_pause_task`, `agent_resume_task`, `agent_cancel_task` (returns rolled_back + irreversible counts) — `src-tauri/src/agent/commands.rs`
- [ ] T017 [US1] [P] Implement Tauri commands: `agent_decide_action`, `agent_show_overlay`, `agent_hide_overlay`, `agent_get_overlay_state`, `agent_save_overlay_state` — `src-tauri/src/agent/commands.rs`
- [ ] T018 [US1] Register all agent Tauri commands in app builder in `src-tauri/src/lib.rs`; wire tray icon with "Show Overlay" + "Quit" menu items
- [ ] T019 [US1] [P] Frontend: `OverlayWindow.tsx` root layout (floating panel, drag handle, minimise-to-corner button, resize handle); `TaskInput.tsx` (textarea max 2000 chars + submit button + char counter) — `src/components/overlay/`
- [ ] T020 [US1] [P] Frontend: `StepProgress.tsx` — subscribe to `agent:step-started`, `agent:step-completed`, `agent:task-completed`, `agent:task-failed` events; render step list with status icons (pending/running/done/failed) + duration badge + output text expandable — `src/components/overlay/StepProgress.tsx`
- [ ] T021 [US1] [P] Frontend: `PaqApproval.tsx` — subscribe to `agent:approval-needed`; render modal with action description + DANGEROUS/CAUTIOUS badge + Approve + Reject buttons; call `agent_decide_action` on click — `src/components/overlay/PaqApproval.tsx`
- [ ] T022 [US1] Frontend: wire `Overlay.tsx` page — compose OverlayWindow wrapping TaskInput + StepProgress + PaqApproval; load overlay state on mount; show task queue badge on tray via `agent:task-updated` events — `src/pages/Overlay.tsx`
- [ ] T023 [US1] Overlay position/size persistence: on resize/drag emit `agent_save_overlay_state`; on mount call `agent_get_overlay_state` and restore position/size; default to centre of primary screen on first launch — `src/pages/Overlay.tsx`

**Checkpoint (P1 MVP)**: Global shortcut opens overlay. Submit "List all PDF files in ~/Documents" → plan shown → Run clicked → steps visible live → result displayed. Pause halts after current step. Cancel works. Task saved to history (check DB).

---

## Phase 4: User Story 2 — File & Shell Automation (Priority: P2)

**Goal**: Full file operations (write/move/copy/delete/rename) and shell command execution with rollback on Cancel and full stdout/stderr capture.

**Independent Test**: Submit "Move all .txt files from /tmp/test-a to /tmp/test-b" → PAQ confirmation shown → Approve → files moved → verify on disk. Cancel mid-move → files restored. Shell failure → error shown in overlay with retry option.

- [ ] T024 [US2] Implement file CAUTIOUS tools: `file_write` (create/overwrite), `file_append`, `file_mkdir` — each pre-registers rollback_log entry before executing — `src-tauri/src/agent/tools/file_ops.rs`
- [ ] T025 [US2] [P] Implement file DANGEROUS tools: `file_move`, `file_copy`, `file_rename`, `file_delete` — each pre-registers rollback_log entry; file_delete moves to a temp trash path rather than hard-deleting (enables restore) — `src-tauri/src/agent/tools/file_ops.rs`
- [ ] T026 [US2] Implement rollback engine: `register_undo(task_id, step_id, op, params)` pre-call; `execute_rollback(task_id)` iterates rollback_log DESC order applying inverse ops (delete_created, move_back, restore_backup, delete_created_dir); update rollback_log.status per entry — `src-tauri/src/agent/rollback.rs`
- [ ] T027 [US2] [P] Implement shell DANGEROUS tool: `shell_exec` — spawn via `std::process::Command`; strip env to `PATH=/usr/bin:/bin`; 60s timeout via `tokio::time::timeout`; capture stdout + stderr + exit_code; classify step as `irreversible`; tag in execution_log — `src-tauri/src/agent/tools/shell.rs`
- [ ] T028 [US2] Update ReAct loop: on `agent_cancel_task` call rollback engine after status → cancelled; update irreversible step count returned from cancel command — `src-tauri/src/agent/react_loop.rs`

**Checkpoint**: Full file automation task runs and rollback works. Shell stdout/stderr shown in overlay. Cancel reverses file moves.

---

## Phase 5: User Story 5 — Voice Interaction (Priority: P2)

**Goal**: Wake word "EchoAI" activates overlay; user speaks task; TTS reads result aloud. Voice and chat fully equivalent.

**Independent Test**: Say "EchoAI" → overlay activates < 1s → say "List files in ~/Desktop" → task executes → result read aloud < 2s after completion. No mic → graceful disabled state in Settings.

**⚠️ KNOWN BLOCKER**: whisper-rs and llama-cpp-2 both bundle `ggml`, causing a link conflict when the `stt` feature is enabled. Resolve this before T029 — options: patch crate to use shared ggml, use `whisper-rs` with `--features=no-ggml`, or wait for upstream fix.

- [ ] T029 [US5] Add `ort = "2"`, `cpal = "0.15"`, `rodio = "0.19"`, `strsim = "0.11"` to `src-tauri/Cargo.toml`; re-enable `stt` feature for `whisper-rs` after resolving ggml link conflict; confirm `cargo check` passes
- [ ] T030 [US5] Implement mic capture: continuous 16 kHz mono ring buffer via `cpal`; `start()` / `stop()` / `read_chunk(n_samples)` API; runs on dedicated thread; graceful `MicrophoneUnavailable` error if no device found — `src-tauri/src/agent/voice/mic_capture.rs`
- [ ] T031 [US5] Implement Silero VAD: load `models/silero_vad.onnx` via `ort`; `is_speech(chunk: &[f32]) -> bool` per 512-sample frame (~32 ms); returns speech onset signal to wake_word pipeline — `src-tauri/src/agent/voice/vad.rs`
- [ ] T032 [US5] Implement wake-word pipeline: on VAD speech onset → buffer 3s of audio → transcribe via `whisper-rs` → Jaro-Winkler fuzzy match (≥ 0.85) against configured wake phrase → emit `agent:voice-wake` Tauri event on match — `src-tauri/src/agent/voice/wake_word.rs`
- [ ] T033 [US5] Implement full-utterance STT: post-wake, continue capturing until 1.5s silence (VAD-detected); run `whisper-rs` transcription on full utterance; emit `agent:voice-transcript { text, confidence }` — `src-tauri/src/agent/voice/stt.rs`
- [ ] T034 [US5] Implement Kokoro TTS: load `models/kokoro-v0_19.onnx` via `ort` (lazy init on first use, evict from RAM after use); synthesise 24 kHz PCM from text; play via `rodio` sink; emit `agent:tts-started` + `agent:tts-completed`; support stop mid-playback — `src-tauri/src/agent/voice/tts.rs`
- [ ] T035 [US5] Implement Tauri commands: `agent_start_listening`, `agent_stop_listening`, `agent_get_voice_status` — `src-tauri/src/agent/commands.rs`; integrate TTS call in task_manager after task completion when input_mode = 'voice'
- [ ] T036 [US5] Voice mode settings: read `voice_mode` (voice_only/chat_only/both) from `agent_settings` DB row at startup; disable wake listener and hide VoiceIndicator when `chat_only`; emit `agent:voice-error { mic_unavailable }` on startup if no device — `src-tauri/src/agent/mod.rs`
- [ ] T037 [US5] [P] Frontend: `VoiceIndicator.tsx` — shows listening (animated mic icon) / speaking (waveform) / idle states; subscribe to `agent:voice-wake`, `agent:tts-started`, `agent:tts-completed`; Stop TTS button calls `agent_stop_listening` — `src/components/overlay/VoiceIndicator.tsx`
- [ ] T038 [US5] Frontend: update `TaskInput.tsx` to adapt for voice_mode — hide text input in `voice_only` mode, show VoiceIndicator; handle `agent:voice-transcript` event to populate and auto-submit input field — `src/components/overlay/TaskInput.tsx`

**Checkpoint**: Wake word activates overlay < 1s. Spoken task executes identically to typed task. TTS response plays < 2s after completion. No mic → settings show disabled state, chat input still works.

---

## Phase 6: User Story 3 — Code Writing & Execution (Priority: P3)

**Goal**: Agent writes Python/JS/Bash scripts, shows code before execution, runs in sandbox, auto-retries on failure.

**Independent Test**: Submit "Sum the revenue column in /tmp/test.csv" → agent shows generated Python script → Approve (PAQ) → runs → result "1500" shown in overlay.

- [ ] T039 [US3] Add `wasmtime = "20"` (with `wasmtime-wasi` feature) to `src-tauri/Cargo.toml`; embed `qjs.wasm` (QuickJS WASI build, ~1 MB) in `src-tauri/assets/qjs.wasm`; confirm `cargo check`
- [ ] T040 [US3] Implement JavaScript executor: load `qjs.wasm` into Wasmtime store with WASI preview 1 (no host FS/net capabilities); inject script via stdin; capture stdout/stderr; 60s timeout — `src-tauri/src/agent/tools/code_exec.rs`
- [ ] T041 [US3] Implement Python subprocess executor: `Command::new("python3")` with env stripped to `PATH=/usr/bin:/bin`; apply `RLIMIT_AS=256MB`, `RLIMIT_CPU=30s`, `RLIMIT_NOFILE=64` via `libc::setrlimit` before exec; capture stdout/stderr/exit_code — `src-tauri/src/agent/tools/code_exec.rs`
- [ ] T042 [US3] [P] Implement Bash subprocess executor (identical structure to Python executor) — `src-tauri/src/agent/tools/code_exec.rs`
- [ ] T043 [US3] Update ReAct loop: on tool error from code_exec tools, re-generate corrected script (inject error into prompt as Observation) + re-dispatch; max 3 auto-retries; after 3 failures surface "Agent could not fix the script" to user — `src-tauri/src/agent/react_loop.rs`
- [ ] T044 [US3] Frontend: render `generated_code` field in StepProgress before execution — show syntax-highlighted code block with "Edit" toggle (inline editable textarea) + "Approve Edit" button that updates step input_json before PAQ approval — `src/components/overlay/StepProgress.tsx`

**Checkpoint**: Python "sum CSV column" task executes correctly. Bad script retried up to 3 times. Code shown to user before execution. Wasmtime JS execution returns correct output.

---

## Phase 7: User Story 4 — Web Content Extraction (Priority: P4)

**Goal**: Agent fetches and extracts structured data from user-provided URLs via local headless Chromium — zero external proxies.

**Independent Test**: Submit "Get the page title from https://en.wikipedia.org/wiki/Rust_(programming_language)" → agent fetches via local Chromium → extracted title shown → zero external calls (verify with `ss -tp`).

- [ ] T045 [US4] Add `chromiumoxide = "0.7"` to `src-tauri/Cargo.toml`; detect system Chromium at app startup (`which chromium` / `which chromium-browser`); store availability flag in AgentState — `src-tauri/src/agent/mod.rs`
- [ ] T046 [US4] Implement web_fetch tool: spawn `chromium --headless --remote-debugging-port=0`; connect via chromiumoxide CDP; navigate to URL; wait for DOM stabilization (networkidle or 3s max); extract `document.body.innerText`; kill subprocess after capture; 15s total timeout — `src-tauri/src/agent/tools/web_scrape.rs`
- [ ] T047 [US4] LLM-driven extraction: pass page text + user extraction prompt to LLM (Tier 3); parse structured output; return as Observation to ReAct loop — `src-tauri/src/agent/tools/web_scrape.rs`
- [ ] T048 [US4] Graceful degradation: if Chromium not found → return `ToolError::ChromiumUnavailable` → ReAct loop surfaces "Web scraping requires Chromium to be installed" to user; disable web_fetch tool in ToolRegistry — `src-tauri/src/agent/tools/web_scrape.rs`

**Checkpoint**: Wikipedia page title extracted correctly. `tcpdump -i any -n not port 22` shows only the Wikipedia connection — no external proxy calls. Chromium absent → clear user message.

---

## Phase 8: User Story 6 — Task History & Replay (Priority: P6)

**Goal**: History panel shows all past tasks with step drill-down, keyword search, and one-click replay.

**Independent Test**: Run 3 tasks → open History → all 3 listed → search by keyword → result in < 1s → replay one → instruction pre-filled → runs again correctly.

- [ ] T049 [US6] Frontend: `HistoryPanel.tsx` — task list with status badges + timestamps; click task to expand step-by-step log with reversibility tags; irreversible steps flagged in red; copy output button per step; Replay button pre-fills TaskInput — `src/components/overlay/HistoryPanel.tsx`
- [ ] T050 [US6] [P] Implement Tauri command: `agent_search_history(query, limit)` — FTS5 MATCH over `agent_tasks_fts` (instruction) + `agent_steps_fts` (output_text); UNION result; rank by bm25 score + recency; return within 1s for 500+ tasks — `src-tauri/src/agent/commands.rs`
- [ ] T051 [US6] [P] Implement Tauri command: `agent_replay_task(task_id)` — fetch original instruction from history; call `submit_task(instruction, 'chat')`; return new_task_id — `src-tauri/src/agent/commands.rs`
- [ ] T052 [US6] History retention enforcement in task_manager: after every task insert, check total count against `history_limit` setting; auto-purge oldest rows when exceeded; `agent_export_task(task_id)` command returns JSON of full task + steps — `src-tauri/src/agent/task_manager.rs`

**Checkpoint**: History shows all tasks. FTS keyword search returns results < 1s. Replay re-runs task from history. Auto-purge works when over limit.

---

## Phase 9: Polish & Cross-Cutting Concerns

**Purpose**: Settings UI, path restrictions, hardware-adaptive flags, installer config.

- [ ] T053 Implement Tauri commands: `agent_get_settings`, `agent_update_settings` (persist to echo.db settings table) — all fields: wake_phrase, voice_mode, tts_enabled, history_limit, allowed_paths, overlay_start_visible — `src-tauri/src/agent/commands.rs`
- [ ] T054 [P] Frontend: Settings screen (gear icon in OverlayWindow) — form with all 6 settings; voice_mode toggle disables voice controls; allowed_paths add/remove list; history limit number input; save calls `agent_update_settings` — `src/components/overlay/`
- [ ] T055 [P] Allowed-paths enforcement: in `file_ops.rs` + `shell.rs`, if `allowed_paths` is non-empty, check every path argument against the list before executing; return `ToolError::PathNotAllowed` if outside allowed list; PAQ shows path restriction warning — `src-tauri/src/agent/tools/`
- [ ] T056 [P] Hardware-adaptive feature flags: on startup, check mic availability (disable voice features gracefully), Python3 availability (disable code_exec_python/bash), Chromium availability (disable web_fetch); notify user in overlay settings with actionable fix messages — `src-tauri/src/agent/mod.rs`
- [ ] T057 Overlay startup visibility: read `overlay_start_visible` from settings on app launch; if true call `overlay_window.show()` before Tauri app runs, otherwise keep hidden — `src-tauri/src/lib.rs`
- [ ] T058 [P] Installer component selection: add Tauri bundler config for optional App 2 component; ensure App 2 overlay window does not register in autostart when not selected — `src-tauri/tauri.conf.json`

---

## Dependencies Graph

```
Phase 1 (Setup)
    └─▶ Phase 2 (Foundation)
            └─▶ Phase 3 (US1 P1 MVP) ◀── GATE: P1 acceptance test must pass
                    ├─▶ Phase 4 (US2 P2 File/Shell)
                    ├─▶ Phase 5 (US5 P2 Voice)   ◀── requires T029 blocker resolved
                    ├─▶ Phase 6 (US3 P3 Code Exec)
                    ├─▶ Phase 7 (US4 P4 Web Scrape)
                    └─▶ Phase 8 (US6 P6 History)
                              (all above) ─▶ Phase 9 (Polish)
```

**Phases 4–8 can begin in parallel after Phase 3 (US1) is accepted.**

---

## Parallel Execution Summary

Within each phase, all tasks marked `[P]` can run concurrently because they touch different files.

| Phase | Parallel opportunities |
|-------|----------------------|
| Phase 1 | T005 alongside T001–T004 |
| Phase 2 | T007 alongside T006 |
| Phase 3 | T013, T016, T017, T019, T020, T021 all parallelizable after T010–T012, T014, T015 |
| Phase 4 | T025, T027 parallelizable after T024 |
| Phase 5 | T037, T038 frontend work parallelizable with T029–T036 backend |
| Phase 6 | T042 alongside T040–T041; T044 (frontend) alongside T039–T043 |
| Phase 8 | T050, T051 alongside T049 |
| Phase 9 | T054, T055, T056, T058 all parallelizable |

**Phases 4, 5, 6, 7, 8 can all run in parallel** — each touches separate tool files and separate frontend components.

---

## Implementation Strategy

### MVP (Phase 1–3): 23 tasks
Delivers US1 fully: overlay activates, task decomposes, ReAct loop executes, PAQ guards destructive steps, live progress shown, file listing works. Independently shippable and demonstrable without voice, code execution, or web scraping.

### Increment 2 (Phase 4): +5 tasks
Adds full file/shell automation with rollback. Covers the widest everyday automation use cases.

### Increment 3 (Phase 5): +10 tasks
Adds voice — hands-free wake word + TTS responses. High differentiator. Requires ggml conflict resolution first.

### Increment 4 (Phase 6): +6 tasks
Adds code execution — Python/JS/Bash scripts in sandbox. Requires Wasmtime + QuickJS.wasm.

### Increment 5 (Phase 7): +4 tasks
Adds web scraping. Requires system Chromium.

### Increment 6 (Phase 8): +4 tasks
Adds history panel and replay. Pure UI + FTS queries.

### Final (Phase 9): +6 tasks
Settings, restrictions, hardware flags, installer.

---

## Task Count Summary

| Phase | Story | Count |
|-------|-------|-------|
| Phase 1 | Setup | 5 |
| Phase 2 | Foundation | 5 |
| Phase 3 | US1 P1 | 13 |
| Phase 4 | US2 P2 | 5 |
| Phase 5 | US5 P2 | 10 |
| Phase 6 | US3 P3 | 6 |
| Phase 7 | US4 P4 | 4 |
| Phase 8 | US6 P6 | 4 |
| Phase 9 | Polish | 6 |
| **Total** | | **58** |
