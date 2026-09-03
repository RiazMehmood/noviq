# ADR-0003: Single-Process Two-Window App Integration Architecture

- **Status:** Accepted
- **Date:** 2026-04-19
- **Feature:** 002-agent-task-overlay
- **Context:** EchoAI is growing from a single-window notebook app (App 1) into a multi-app
  suite (App 2: Agent Overlay, App 3/4: TBD). A fundamental architectural question is how
  these apps coexist on the user's desktop: as independent OS processes communicating
  via IPC, or as windows within the same Tauri process sharing Rust services and state.
  This decision is cross-cutting — it determines RAM footprint, IPC complexity, crash
  boundaries, the LLM model slot (Constitution Principle III), and installer structure.
  Getting it wrong is expensive to reverse: IPC → in-process requires removing an entire
  service boundary; in-process → IPC requires adding one.

## Decision

**Single Tauri process with two independent WebviewWindows sharing all Rust services.**

Components:
- **Process**: One `tauri::App` instance creates and owns both windows.
- **Window 1 — Notebook** (`"main"`): Standard decorated window, `/` route in SolidJS.
- **Window 2 — Overlay** (`"overlay"`): `always_on_top`, `decorations(false)`, `transparent`, `skip_taskbar(true)`, `/overlay` route. Hidden at launch by default.
- **Shared ModelSlot**: `Arc<Mutex<ModelSlot>>` in Tauri managed state — single RAM resident for both windows. No two LLMs loaded simultaneously (Constitution Principle III).
- **Shared `echo.db`**: One sqlx connection pool; App 2 adds own tables via migrations, reads App 1 tables read-only.
- **Shared services**: `hardware_detector`, `query_router`, `llm`, `task_manager` all held in Tauri managed state and accessible from both windows' command handlers.
- **Installer**: Single unified package; component selection (App 1 / App 2 / both) via Tauri bundler config at build time.
- **Autostart**: App 2 overlay registers in OS autostart by default; App 1 does not.

## Consequences

### Positive

- **No IPC overhead**: Both windows share Rust `Arc` references — zero serialization, zero
  socket round-trips for LLM calls or DB queries.
- **Single model load**: The 3B/7B LLM is loaded once regardless of which window requests
  inference. Eliminates the primary OOM risk on 8–16 GB machines.
- **Simpler code**: No protocol design, no versioning, no reconnect logic. New capabilities
  added as Tauri commands visible to both windows instantly.
- **Unified installer**: One binary, one auto-updater, one OS autostart entry.
- **Shared DB migrations**: One migration history, one schema version, no sync headaches.

### Negative

- **Shared crash boundary**: A panic in the agent backend can bring down the notebook window.
  Mitigated by: Rust's memory safety (no segfaults), `catch_unwind` at Tauri command
  boundaries, and the fact that WebView crashes are isolated to their window.
- **Tighter coupling for future apps**: App 3/4 must either join the same process (growing
  the binary and startup time) or become separate processes with IPC anyway. A workspace
  refactor to a shared `echo-ai-core` library crate is deferred to when a third independent
  binary is needed.
- **Single autostart entry**: Only one OS autostart registration — App 2 is the default
  autostart entry; App 1 is opt-in. If the user wants App 1 without App 2, they start the
  process and hide the overlay — not a significant UX concern.

## Alternatives Considered

### Alternative A: Separate OS Processes with IPC

Two Tauri binaries communicate via a Unix socket or D-Bus. App 1 owns the ModelSlot and
exposes an inference RPC; App 2 calls it as a client.

**Why rejected**:
- Violates Constitution Principle III: the model would be loaded by whichever process
  starts first; there is no mechanism to share a GGUF-resident `llama_model` pointer
  across process boundaries — each process would need its own copy (×2 RAM).
- Adds a full IPC protocol, versioning, and reconnect logic for zero user-visible benefit.
- Two installer packages and two autostart entries complicate distribution.

### Alternative B: Sidecar Process for App 2 Only

App 2 runs as a Tauri sidecar (child process) that gets model access by calling back to
App 1 via a local HTTP or stdio channel.

**Why rejected**:
- Same model-RAM problem as Alternative A — the sidecar cannot share llama.cpp model state
  with the parent process at the C-FFI level.
- Adds latency to every LLM call (process boundary + serialization of token streams).
- ADR-0001 already established in-process model hosting as the accepted pattern.

### Alternative C: Cargo Workspace with Shared `echo-ai-core` Crate (Now)

Convert the single `echo-ai` crate to a Cargo workspace immediately, extract a shared
`echo-ai-core` library, and have both apps compile to separate binaries that link to it.

**Why rejected for this phase**:
- Does not solve the shared-RAM problem — separate binaries still load the model separately.
- Large refactor (moving ~10 services, fixing import paths) for no user-visible benefit now.
- Deferred to when App 3 requires it — at that point the workspace refactor is justified.

## References

- Feature Spec: `specs/002-agent-task-overlay/spec.md` (Assumptions section, Overview)
- Implementation Plan: `specs/002-agent-task-overlay/plan.md` (AD-001, Phase 1)
- Research: `specs/002-agent-task-overlay/research.md` (R-001)
- Related ADRs: ADR-0001 (LLM Model Hosting Isolation Strategy — established in-process ModelSlot, this ADR extends it to multi-window)
- Constitution: `.specify/memory/constitution.md` (Principle III: Single Model Slot)
