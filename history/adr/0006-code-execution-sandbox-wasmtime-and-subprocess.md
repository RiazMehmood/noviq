# ADR-0006: Code Execution Sandbox — Wasmtime/WASI + Restricted Subprocess

- **Status:** Accepted
- **Date:** 2026-04-19
- **Feature:** 002-agent-task-overlay
- **Context:** App 2's agent can write and execute code (Python, Bash, JavaScript) on
  behalf of the user. This raises a critical security question: what sandbox boundary
  prevents agent-generated code from doing unintended damage (deleting system files,
  making network calls, spawning background processes)? The constitution locks the answer
  to "Wasmtime / WASI" — but this must be qualified per language because true WASM
  sandboxing is only practical for JavaScript in 2026. The decision determines what
  resource limits apply, what languages are supported, and how the PAQ safety gate
  (Principle V) integrates with execution.

## Decision

**JavaScript via Wasmtime/WASI (QuickJS.wasm); Python and Bash via restricted subprocess.**

Components:
- **JavaScript** (`tools/code_exec.rs` — Wasmtime path):
  - `wasmtime` 20 crate with WASI preview 1 runtime.
  - `qjs.wasm` (~1 MB, QuickJS compiled to WASM) bundled in `src-tauri/assets/`.
  - Runs in a Wasmtime store with no host filesystem or network capabilities exposed —
    true sandbox: the WASM module cannot access the host OS regardless of what code runs.
  - Stdout/stderr captured from WASI preview 1 pipe; returned to agent as Observation.

- **Python** (`tools/code_exec.rs` — subprocess path):
  - Spawned via `std::process::Command` with a stripped environment
    (`PATH=/usr/bin:/bin` only; no HOME, DBUS_SESSION, XDG_*, etc.).
  - Resource limits applied via `setrlimit` before exec:
    - `RLIMIT_AS = 256 MB` (virtual address space)
    - `RLIMIT_CPU = 30 s` (CPU time hard limit)
    - `RLIMIT_NOFILE = 64` (file descriptor cap)
  - Stdout, stderr, exit code captured; returned to agent.
  - Classified `DANGEROUS` in persona TOML → PAQ approval required before execution.

- **Bash** (`tools/code_exec.rs` — same subprocess path as Python):
  - Same resource limits and environment stripping as Python.
  - Classified `DANGEROUS` → PAQ approval required.

- **Generated code display** (FR-012): Agent-generated code is shown in the overlay UI
  before any execution path begins — user can edit or reject.

- **Auto-retry**: On script failure, agent re-generates a corrected script (max 3 retries)
  before surfacing to user. Each retry still requires PAQ re-approval.

## Consequences

### Positive

- **True sandbox for JS**: Wasmtime WASI provides OS-level isolation for JavaScript — no
  `process.exit()`, no `fs.readFileSync('~/.ssh/id_rsa')` possible from within WASM.
- **No additional system dependency for JS**: QuickJS.wasm is bundled; no Node.js required.
- **Pragmatic Python/Bash**: Resource limits prevent runaway memory usage or infinite
  loops. PAQ gate ensures user sees and approves every execution.
- **Small Wasmtime footprint**: `wasmtime` 20 adds ~4 MB to the compiled binary (stripped).
- **Graceful degradation**: If Python3 not found → code exec disabled with clear message.
  Overlay still functional for file/shell tasks.

### Negative

- **Python subprocess is not a true sandbox**: `setrlimit` + env stripping reduces blast
  radius but does not prevent reading arbitrary files the user has access to (that is the
  intended behaviour — the agent acts on the user's behalf) or making localhost network
  calls. All Python executions are DANGEROUS-class and require explicit PAQ approval.
- **No seccomp profile (this phase)**: A seccomp filter would block unexpected syscalls.
  Deferred — adds a Linux-only dependency and requires careful profiling to avoid
  breaking legitimate Python stdlib use. Noted as a hardening item for a future phase.
- **QuickJS WASM feature gap**: QuickJS supports ES2020; no npm module support. Sufficient
  for data-manipulation scripts; not suitable for UI or complex node packages.
- **Auto-retry requires re-approval**: Each retry generates a new PAQ entry — user must
  approve up to 3× for a failing script. Deliberate: every execution must be visible.

## Alternatives Considered

### Alternative A: Full CPython WASM (CPython compiled to Wasmtime)

Run Python code in Wasmtime for true sandboxing parity with JavaScript.

**Why rejected**: CPython WASM is ~60 MB and still experimental as of 2026. Startup time
per Python script is ~5–10s in Wasmtime (no JIT). Stdlib support is incomplete. Revisit
when CPython WASM reaches stable + < 2s cold start.

### Alternative B: Docker / Podman Container per Execution

Spawn a container for each code execution for full OS-level isolation.

**Why rejected**: Adds a Docker/Podman system dependency — breaks the "offline-first,
no-server" promise. Container spin-up latency is 2–5s per execution. Unacceptable for an
interactive overlay. Not installable as part of a standalone desktop app bundle.

### Alternative C: nsjail / bubblewrap Linux Namespace Sandbox

Use `nsjail` or `bwrap` (bubblewrap) to create a restricted Linux namespace for subprocesses.

**Why rejected for this phase**: More robust than `setrlimit` but adds a system dependency
(`bubblewrap` is not installed by default on all distros). Requires careful mount namespace
design to give the subprocess access to the user's working files. Deferred to a hardening
phase alongside seccomp.

### Alternative D: Subprocess with No Restrictions

Execute Python/Bash directly with the full user environment.

**Why rejected**: An agent bug or adversarial prompt could generate code that deletes home
directory files or exfiltrates SSH keys. The minimal restriction of env stripping + RLIMIT
is a hard floor; removing it entirely contradicts the safety mandate.

## References

- Feature Spec: `specs/002-agent-task-overlay/spec.md` (FR-012, FR-014, User Story 3)
- Implementation Plan: `specs/002-agent-task-overlay/plan.md` (AD-005, Phase 3)
- Research: `specs/002-agent-task-overlay/research.md` (R-005)
- Related ADRs: ADR-0004 (ReAct loop — code_exec is a tool dispatched by the loop)
- Constitution: `.specify/memory/constitution.md` (Principle V: PAQ Safety Gate, Sandboxed execution: Wasmtime/WASI lock)
