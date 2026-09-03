# Feature Specification: Agent Task Executor + Desktop Overlay

**Feature Branch**: `002-agent-task-overlay`
**Created**: 2026-04-19
**Status**: Draft

---

## Overview

An offline-first desktop application with a persistent floating overlay that lets users delegate tasks to a local AI agent. The agent interprets natural language instructions, breaks them into executable steps, and autonomously carries them out — file operations, shell commands, code execution, and web scraping — while the user continues working. Progress is shown live in the overlay without interrupting the user's active application.

App 1 (Notebook) and App 2 (Agent Overlay) are delivered as a single Tauri process with two independent windows. They share one LLM model slot (one RAM load), one SQLite database (App 2 adds its own tables), and all core services via a shared `echo-ai-core` Rust library. Each window can be shown or hidden independently; the process stays alive via the system tray until the user explicitly quits.

---

## Clarifications

### Session 2026-04-19

- Q: App architecture — separate process or shared? → A: Single Tauri process, two independent windows (Notebook + Agent Overlay). Shared LLM model slot (1× RAM), shared `echo.db` (App 2 adds own tables), shared `echo-ai-core` crate. Each window independently show/hide via tray. One unified installer. App 2 overlay auto-starts by default; App 1 notebook window does not.
- Q: Task plan approval before execution — explicit approve or auto-start? → A: Explicit approve — agent shows decomposed step plan; user must click "Run" before any execution step begins.
- Q: Rollback behaviour on Cancel → A: File-op rollback (moves/copies/creates/renames reversed); shell command side-effects are not undone but every executed command with full stdout/stderr is written to history tagged "irreversible" so users have a record to work from.
- Q: History retention policy → A: Configurable — default 500 tasks (oldest auto-purged when exceeded); user can change limit in Settings including unlimited; export available before purge.
- Q: Overlay startup visibility → A: Configurable — default is hidden (tray-only at login); user can change to start visible in Settings.
- Additional requirement (user-specified): Voice interaction — wake-word activation ("EchoAI"), local STT (reusing whisper-rs from App 1), local TTS response, voice and chat are fully equivalent input modes; user can configure voice-only, chat-only, or both.

---

## User Scenarios & Testing

### User Story 1 — Task Assignment & Live Progress (Priority: P1)

The user presses a global shortcut to bring up the overlay, types a plain-language instruction (e.g. "Rename all PNG files in my Downloads folder to include today's date"), and watches the agent execute it step by step with each action and outcome shown in the overlay. When done, a summary is shown and the overlay can be dismissed or minimised.

**Why this priority**: This is the core loop of the entire product. Without it, nothing else has value.

**Independent Test**: Press shortcut → type task → agent executes → overlay shows each step and final result. Delivers standalone value with no other story implemented.

**Acceptance Scenarios**:

1. **Given** the overlay is open, **When** the user submits "List all PDF files in ~/Documents", **Then** the agent shows a step "Scanning ~/Documents…", completes within 5s, and displays a result list in the overlay.
2. **Given** the agent is executing, **When** the user clicks Pause, **Then** execution halts after the current step and resumes from that point when Resume is clicked.
3. **Given** the agent completes a task, **When** the result is shown, **Then** the user can copy the output or dismiss the overlay and the task is saved to history.
4. **Given** the agent encounters an ambiguous instruction, **When** it cannot proceed confidently, **Then** it surfaces a clarifying question in the overlay rather than guessing.

---

### User Story 2 — File & Shell Automation (Priority: P2)

The user describes a file/system task in plain language (e.g. "Move all files older than 30 days from ~/Downloads to ~/Archive and create a summary text file") and the agent handles it — creating folders, moving files, running shell commands, and writing output.

**Why this priority**: File and shell operations cover the widest range of everyday automation tasks.

**Independent Test**: Assign a file-organisation task on a test folder → verify files moved, folders created, output file written — all matching the instruction.

**Acceptance Scenarios**:

1. **Given** a folder with mixed files, **When** the user asks to "organise files by extension into subfolders", **Then** the agent creates subfolders per extension and moves files correctly; no file is lost.
2. **Given** a task with a destructive step (delete, overwrite, format), **When** the agent reaches that step, **Then** it pauses and requests explicit user confirmation before proceeding.
3. **Given** a shell command fails, **When** the error occurs, **Then** the agent shows the error, explains what went wrong, and offers to retry or skip.
4. **Given** a shell command produces output, **When** it completes, **Then** full stdout/stderr is captured and shown in the overlay with the ability to copy it.

---

### User Story 3 — Code Writing & Execution (Priority: P3)

The user describes a computation task (e.g. "Parse this CSV and show me the top 10 rows by revenue") and the agent writes a script, executes it in a Wasmtime/WASI sandbox, and shows the output — without the user needing to open an IDE.

**Why this priority**: Extends automation to data work and dynamic logic that pure file/shell operations cannot cover.

**Independent Test**: Ask agent to "sum a column in a CSV file" → agent writes a script, runs it, returns the correct result.

**Acceptance Scenarios**:

1. **Given** a task requiring computation, **When** the agent writes a script, **Then** the generated code is shown to the user before execution and can be edited.
2. **Given** a script fails, **When** a runtime error occurs, **Then** the agent shows the error, attempts a fix automatically, and re-runs (max 3 auto-retries before surfacing to user).
3. **Given** code execution exceeds 60 seconds, **When** the timeout is reached, **Then** the agent alerts the user and offers to cancel.
4. **Given** a script produces output, **When** execution completes, **Then** the result is shown in the overlay and saved to task history.

---

### User Story 4 — Web Content Extraction (Priority: P4)

The user provides a URL and describes what to extract (e.g. "Get the article titles and dates from this news page") and the agent uses a local headless browser to fetch and extract the data — with no cloud proxy or external API calls.

**Why this priority**: Useful but requires an additional browser dependency; lower priority than core automation.

**Independent Test**: Provide a public URL + extraction description → agent returns structured data matching visible page content — verified zero external API calls.

**Acceptance Scenarios**:

1. **Given** a valid URL and extraction request, **When** the agent fetches the page, **Then** it does so via a locally running headless browser with zero cloud or external calls.
2. **Given** a page requires JavaScript rendering, **When** the agent loads it, **Then** it waits for the DOM to stabilise before extracting.
3. **Given** a page is unreachable, **When** the request times out after 15 seconds, **Then** the agent reports the failure and suggests next steps.
4. **Given** extracted data, **When** the task completes, **Then** the output is shown in the overlay and optionally saved to a user-specified file.

---

### User Story 5 — Voice Interaction (Priority: P2)

The user says "EchoAI, create a summary of all PDFs in my Downloads folder" and the overlay activates, transcribes the instruction, confirms the plan aloud, executes the task, and reads the result back — all without the user touching the keyboard. Alternatively, the user types the same instruction in the chat input and receives a text response. Both modes are fully equivalent.

**Why this priority**: Voice is a primary differentiator for a desktop overlay — it makes the agent hands-free and truly ambient. Promoted to P2 because it is a first-class input method, not an add-on.

**Independent Test**: Say "EchoAI, list files in my Desktop" → overlay activates → task executes → result read aloud. Delivers standalone value independently of other stories.

**Acceptance Scenarios**:

1. **Given** the app is running in the background, **When** the user says "EchoAI" (wake word), **Then** the overlay becomes visible and begins listening within 1 second, with a visible listening indicator.
2. **Given** the overlay is listening, **When** the user speaks a task instruction, **Then** the transcribed text appears in the input field and the agent proceeds as if the user had typed it.
3. **Given** a task completes and input was via voice, **When** the result is ready, **Then** the agent reads the result aloud via text-to-speech; the spoken response can be stopped by the user at any time.
4. **Given** Settings are open, **When** the user selects input mode, **Then** they can choose voice-only, chat-only, or both; the overlay UI adapts to show/hide the text input accordingly.
5. **Given** no microphone is detected at startup, **When** the app initialises, **Then** voice input is disabled gracefully with a clear message; chat input remains fully functional.

---

### User Story 6 — Task History & Replay (Priority: P6)

The user opens the history panel, sees a log of all executed tasks with steps and results, and can rerun any past task with one click (optionally editing the instruction first).

**Why this priority**: Productivity multiplier — not required for initial value but important for daily use.

**Independent Test**: Run 3 tasks → open history → verify all 3 listed with steps and results → replay one → verify it runs correctly again.

**Acceptance Scenarios**:

1. **Given** the history panel is open, **When** the user selects a past task, **Then** they see the full step-by-step log, irreversible step flags, and final output.
2. **Given** a completed task in history, **When** the user clicks Replay, **Then** the instruction is pre-filled in the overlay input and can be edited before running.
3. **Given** history with 500+ tasks, **When** the user searches by keyword, **Then** matching tasks appear within 1 second.

---

### Edge Cases

- What happens when the system shuts down mid-execution? (Task marked interrupted; partial actions logged; no silent partial changes left behind.)
- What if the user gives contradictory instructions? (Agent surfaces the ambiguity before starting, not mid-execution.)
- What if two queued tasks both try to write the same file? (Sequential execution prevents conflict; tasks run one at a time.)
- What if the LLM generates a command that exceeds the allowed scope? (Blocked before execution; user notified with explanation.)
- What if the overlay is on a disconnected screen? (Overlay resets to primary screen centre on next launch.)
- What if Python or Bash is not installed? (Code execution is disabled gracefully with a clear message; other capabilities remain functional.)
- What if no microphone is available or permission is denied? (Voice input disabled gracefully; chat input remains fully functional; user notified on startup.)
- What if the wake word is triggered accidentally by ambient speech? (A visible cancel indicator appears with a brief grace period before transcription begins; user can dismiss immediately.)
- What if TTS is speaking and the user says the wake word again? (TTS stops immediately and the overlay enters listening mode.)

---

## Requirements

### Functional Requirements

**Overlay & UX**
- **FR-001**: System MUST provide a floating always-on-top overlay window that remains visible across all application windows and virtual desktops.
- **FR-002**: The overlay MUST be activatable via a configurable global keyboard shortcut without stealing focus from the user's active application.
- **FR-003**: The overlay MUST support resize, reposition, and minimise-to-corner; last position and size MUST persist across sessions.
- **FR-004**: The overlay MUST show real-time step-by-step execution progress with step name, status (pending / running / done / failed), and duration per step.
- **FR-005**: System MUST provide a system tray icon for quick access to show/hide the overlay and view active task status.

**Task Input & Agent Reasoning**
- **FR-006**: System MUST accept natural language task descriptions up to 2000 characters.
- **FR-007**: The agent MUST decompose each task into an ordered list of steps before execution begins, display that plan for user review, and require the user to click an explicit "Run" button before any step executes; the user may cancel at this stage without any action being taken.
- **FR-008**: System MUST support a task queue; users MUST be able to submit additional tasks while one is running.
- **FR-009**: The agent MUST surface a clarifying question when instruction intent is ambiguous rather than proceeding with a guess.

**Execution Capabilities**
- **FR-010**: System MUST support file operations: create, read, write, append, move, copy, rename, delete, and recursive directory listing.
- **FR-011**: System MUST execute shell commands and capture full stdout, stderr, and exit code.
- **FR-012**: System MUST write and execute code (Python, Bash, JavaScript) in a Wasmtime/WASI sandbox; generated code MUST be shown to the user before execution.
- **FR-013**: System MUST support local headless-browser web content extraction; all page fetches MUST go through the local browser with zero external proxy or cloud calls.

**Safety & Approval**
- **FR-014**: Any tool call classified as DANGEROUS (file delete, overwrite, format, system-level shell command, code execution with write access) MUST be placed in the Pending Actions Queue (PAQ) and require explicit user approval before execution; CAUTIOUS tool calls (file read, directory list, web fetch) display a brief confirmation; READ-ONLY tool calls execute immediately. This gate MUST NOT be bypassable programmatically.
- **FR-015**: The agent MUST have unrestricted filesystem and shell access by default (matching the logged-in user's permissions); users MAY optionally configure an allowed-paths list in Settings to restrict the agent to specific folders — when a restriction is active, any step targeting a path outside the list MUST be blocked and the user notified before proceeding.
- **FR-016**: System MUST allow the user to Pause, Resume, or Cancel any running task at any time. On Cancel: file operations (create, move, copy, rename) MUST be reversed in reverse order; shell command side-effects MUST NOT be silently discarded — every executed shell command, its arguments, stdout/stderr, and exit code MUST be written to the execution history with an explicit "irreversible" flag so the user has a full record to reason from and take corrective action manually.

**Agent Configuration**
- **FR-028**: The agent's system prompt, tool schema, capability list, and safety classification per tool MUST be defined in TOML configuration files at runtime; no agent behaviour may be hardcoded. Adding a new persona or tool capability requires only a TOML edit, not a code change.

**Voice Interaction**
- **FR-022**: System MUST support wake-word activation; default wake phrase is "EchoAI" and MUST be configurable by the user in Settings.
- **FR-023**: Speech-to-text MUST run fully locally using the whisper-rs transcription engine (reused from App 1); no audio is sent to any external service.
- **FR-024**: Text-to-speech response MUST run fully locally; when a task was submitted via voice, the agent MUST read the result aloud; TTS can be toggled off in Settings.
- **FR-025**: Voice and chat input MUST be fully equivalent — any task submittable via text MUST be submittable via voice and produce identical behaviour.
- **FR-026**: Users MUST be able to set input mode in Settings: voice-only, chat-only, or both; the overlay UI adapts to reflect the active mode.

**Overlay & UX (startup)**
- **FR-027**: Overlay startup visibility MUST default to hidden (tray icon only at login); users MUST be able to change this to start visible in Settings.

**Offline & Privacy**
- **FR-017**: All LLM inference, code execution, and web scraping MUST occur locally; zero outbound network calls are permitted except to URLs explicitly provided by the user in a task.
- **FR-018**: Task history, step logs, and all outputs MUST be stored locally; no data leaves the device.

**History & Persistence**
- **FR-019**: System MUST persist a full execution log for every task: instruction, steps, outputs, duration, and final status (completed / failed / cancelled / interrupted). Each step MUST be tagged as reversible (file op) or irreversible (shell command, code execution, web fetch) so users can identify what requires manual remediation after a Cancel.
- **FR-020**: History retention MUST default to 500 tasks (oldest auto-purged when limit is exceeded); users MUST be able to change this limit in Settings (any value including unlimited). Users MUST be able to export any task record before it is purged.
- **FR-021**: System MUST allow the user to search history by keyword and replay any past task.

---

### Key Entities

- **Task**: A user-submitted natural language instruction (via chat or voice) with queue position, status, input mode (voice/chat), timestamps, and a link to its execution log.
- **Step**: A single atomic action within a task (shell command, file op, code run, web fetch) with status, output, and duration.
- **ExecutionLog**: The complete ordered record of steps for a task including all outputs and errors.
- **ApprovalRequest**: A pause point requiring explicit user confirmation before a destructive or ambiguous step proceeds.
- **TaskResult**: The final structured output of a completed task, stored in history and displayable in the overlay.

---

## Success Criteria

### Measurable Outcomes

- **SC-001**: User submits a task and sees the first execution step begin within 3 seconds of confirmation.
- **SC-002**: The overlay remains visible and interactive while the user works in any other application, with zero focus-stealing behaviour.
- **SC-003**: File and shell tasks involving up to 1000 files complete without memory exhaustion or application freeze.
- **SC-004**: Zero outbound network calls occur during any task that does not include a user-provided URL, verified by network monitor.
- **SC-005**: The agent correctly identifies and flags destructive operations for approval in 100% of test cases.
- **SC-006**: Task history search returns results within 1 second for a history of 500+ tasks.
- **SC-007**: Overlay window position and size are restored correctly after app restart in 100% of cases.
- **SC-008**: When a path restriction is configured in Settings, code execution and file operations targeting paths outside the allowed list are blocked in 100% of test cases.
- **SC-009**: Wake word is detected and the overlay enters listening state within 1 second of the utterance completing.
- **SC-010**: Voice-to-task round trip (wake word → transcription → plan shown) completes within 5 seconds on the same hardware as App 1.
- **SC-011**: TTS response begins playing within 2 seconds of task completion when voice mode is active.
- **SC-012**: Voice and chat input modes produce identical task execution results in 100% of equivalent test cases.

---

## Assumptions

- App 1 (Notebook) and App 2 (Agent Overlay) are delivered as a **single Tauri process** with two independent windows; the Rust codebase is organised as a Cargo workspace with a shared `echo-ai-core` library crate used by both windows.
- The process is distributed as a **single unified installer** with component selection (user may choose to enable the Notebook window, the Agent Overlay, or both).
- **App 2 overlay window registers in OS autostart by default**; App 1 notebook window does not autostart by default (opt-in from Settings).
- One **shared `echo.db`** SQLite database; App 2 adds its own tables via new migrations and reads App 1's tables (chunks, sources, notebooks) in read-only mode for task context.
- **One LLM model slot** shared across both windows within the same process — the orchestrator (3B) stays warm; the worker (7B) is demand-loaded and evicted after 90 seconds of idle (Constitution Principle III). No two LLM instances may reside in RAM simultaneously.
- The local LLM already present from App 1 (qwen2.5-3b or 7b) is reused; no new model download required.
- Python and Bash are available on the host; app checks at startup and disables code execution gracefully if absent.
- Microphone access is available on the host; app checks at startup and disables voice input gracefully if absent or denied.
- Whisper-rs (already in App 1) is reused for local STT; no additional model download required for basic transcription.
- Kokoro TTS (local neural, no cloud) is used for all voice responses; this is a constitution-locked invariant.
- Headless browser for web scraping is Chromium-based (system-installed or bundled); JavaScript-rendered pages are supported.
- Task scheduling (run at a specific time / repeat on interval) is out of scope for this phase — all tasks are on-demand.
- Concurrent task execution is out of scope; tasks run sequentially from the queue.
- Targets Linux platform for this phase; Windows/macOS adaptation is a future concern.

---

## Out of Scope

- Cloud-based agent execution or remote task delegation
- Integration with external services (GitHub, Google Drive, Slack, etc.)
- Task scheduling / cron-style recurring tasks
- Plugin marketplace for custom agent tools
- Multi-user or shared task queues
- Windows / macOS support (this phase)
