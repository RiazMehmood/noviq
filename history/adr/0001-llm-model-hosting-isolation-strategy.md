# ADR-0001: LLM Model Hosting Isolation Strategy

- **Status:** Accepted
- **Date:** 2026-04-13
- **Feature:** 001-notebook-knowledge-engine
- **Context:** EchoAI runs LLM inference locally via llama.cpp for Tier 3 (3B orchestrator)
  and Tier 4 (7B worker) models. A design decision is required for how the inference
  process is hosted relative to the main Tauri application process. Two viable approaches
  exist: embed the model directly inside the Tauri Rust process (in-process), or run
  inference in a separate sidecar process communicating via IPC. This decision is
  cross-cutting — it affects crash isolation, memory management, restart behaviour,
  the Tauri plugin architecture, and all future feature-apps that consume LLM inference
  (App 2, 3, 4). Getting it wrong requires an expensive rewrite of the service boundary.

## Decision

**In-process model hosting via a `ModelSlot` singleton, with a dedicated vision sidecar
for Tier 5.**

Components:

- **ModelSlot** (`src-tauri/src/services/llm.rs`): A single `Arc<Mutex<ModelSlot>>`
  held in Tauri managed state. Wraps `llama-cpp-2` model state. Hosts Tier 3 (3B
  orchestrator, always warm) and Tier 4 (7B worker, demand-loaded with 90s idle eviction).
  Only one model occupies the slot at any time; loading a new model evicts the current.
- **VisionSlot** (`src-tauri/src/services/vision.rs`): Separate `Arc<Mutex<VisionSlot>>`
  for Tier 5 (Qwen2-VL 2B). Vision inference is short-lived and infrequent; keeping it
  in-process but in its own slot prevents it from evicting the orchestrator.
- **No sidecar for Tier 3/4**: The main Rust process owns all inference for Tiers 3 and 4.
  Crash recovery for LLM failures is handled at the service layer (catch panic, reset
  ModelSlot to `None`, return error to frontend with retry prompt).

## Consequences

### Positive

- **Zero IPC overhead**: Inference calls are direct Rust function calls — no serialisation,
  socket latency, or protocol framing. Critical for the < 3s query budget on 8 GB hardware.
- **Shared memory with DB layer**: The query router can interleave SQLite reads and LLM
  calls within the same async task without marshalling data across process boundaries.
- **Simpler operational model**: One process to monitor, one crash to handle, one set of
  logs. No sidecar lifecycle management (start, health-check, restart, shutdown ordering).
- **Lower RAM baseline**: No second process heap, no IPC buffer allocation. Saves ~50–100 MB
  on constrained 8 GB hardware.
- **Tauri managed state is natural fit**: `llama-cpp-2` models are Send + Sync behind a
  Mutex — idiomatic Tauri state management with no extra machinery.

### Negative

- **LLM crash takes down the UI**: If llama.cpp panics (OOM, corrupted GGUF, SIGSEGV in
  native code), the whole Tauri process dies and the user loses unsaved state. Mitigated
  by: (a) `catch_unwind` at the service boundary, (b) SQLite WAL ensures no index
  corruption on crash, (c) session history is persisted after each message.
- **No hot-reload of models without app restart**: Updating the model file requires
  restarting the app. Acceptable for v1 (model updates are infrequent and user-initiated).
- **Memory pressure is shared**: A runaway LLM call consuming all RAM affects the UI
  thread. Mitigated by the 90s eviction timer and the TaskBudget CPU cap.
- **Cannot independently scale inference**: If future hardware (e.g., a local LAN mesh)
  requires offloading inference to another machine, this architecture needs a service
  boundary added. Deferred to v2 as the "Local Mesh" feature.

## Alternatives Considered

### Alternative A: Sidecar Process (llama-server / custom Rust binary)

Run a dedicated inference process (e.g., llama.cpp's built-in `llama-server`, or a
custom Rust binary) that the Tauri app communicates with over a Unix socket or named pipe.

**Why rejected**:
- Adds ~150–200 ms round-trip latency per token batch on Unix socket (measurable on 8 GB).
- Requires sidecar lifecycle management: starting before app is ready, health-checking,
  restarting on crash, coordinating shutdown. Doubles the operational surface area.
- On Windows, named pipe IPC is more complex and less reliable than Unix sockets.
- RAM cost increases: second process heap (~50–100 MB) plus IPC buffer pool.
- Crash isolation benefit is real but overstated for v1: llama.cpp is stable for
  well-formed GGUF files; the primary crash vector (corrupted model) is handled by
  pre-flight GGUF validation before loading.
- **Could be adopted in v2** if the Local Mesh (distributed inference) feature is built.

### Alternative B: WASM-based Inference (llama.cpp via Emscripten/WASI)

Compile llama.cpp to WASM and run inference inside the Tauri WebView process.

**Why rejected**:
- WASM linear memory is limited to 4 GB on most runtimes — insufficient for 7B Q4_K_M
  (4.5 GB).
- No GPU/NPU access from WASM sandbox — all inference on CPU only, ~5–10× slower.
- llama.cpp WASM build is experimental and not production-ready as of 2026.

### Alternative C: Python llama-cpp-python subprocess

Spawn a Python subprocess running `llama-cpp-python` server.

**Why rejected**:
- Violates constitution Principle VI (no Python runtime in execution path).
- Python process startup adds 2–3 seconds to first query.
- Dependency management nightmare across Windows/macOS/Linux.

## References

- Feature Spec: `specs/001-notebook-knowledge-engine/spec.md`
- Implementation Plan: `specs/001-notebook-knowledge-engine/plan.md` (AD-2: ModelSlot Singleton)
- Research: `specs/001-notebook-knowledge-engine/research.md` (Decision 6: Model Loading/Unloading)
- Constitution: `.specify/memory/constitution.md` (Principle III: Single Model Slot)
- Related ADRs: ADR-0002 (Embedding Model Selection — shares the single-slot constraint)
- Evaluator Evidence: `history/prompts/001-notebook-knowledge-engine/003-notebook-engine-plan.plan.prompt.md`
