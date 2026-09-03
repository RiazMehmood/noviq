# ADR-0004: ReAct Agent Loop with TOML Persona Configuration

- **Status:** Accepted
- **Date:** 2026-04-19
- **Feature:** 002-agent-task-overlay
- **Context:** App 2 requires an AI agent capable of breaking natural-language tasks into
  tool calls (file ops, shell, code execution, web scraping) and executing them
  autonomously. Two sub-decisions are tightly coupled and resolved together:
  (1) how the agent *reasons and acts* (orchestration pattern), and (2) how agent
  *behaviour is defined* (persona, tool list, safety classification). These determine
  whether adding a new tool requires a code change or a config change, how safety gates
  integrate with the loop, and whether the codebase accumulates hardcoded prompts.

## Decision

**ReAct (Reason+Act) loop implemented in Rust, driven by TOML persona config files.**

Components:
- **Loop** (`src-tauri/src/agent/react_loop.rs`): Thought → Action → Observation cycle.
  Formats prompt from persona TOML (system prompt + tool schema), calls `llm.rs`,
  parses `Action:` / `Action Input:` lines, dispatches to tool, appends `Observation:`,
  repeats until `Final Answer:` or iteration cap.
- **Persona TOML** (`config/personas/agent-default.toml`): `[persona]` block (id, version,
  system_prompt, max_iterations) + `[[tool]]` array (name, description,
  `safety_class = READ_ONLY | CAUTIOUS | DANGEROUS`, params).
- **Safety-by-config**: Each tool's `safety_class` maps directly to PAQ tier — policy
  changes require only a TOML edit, not a recompile.
- **Iteration cap**: Default 20, configurable per persona. On cap: partial result + warning.
- **Persona loader** (`src-tauri/src/agent/persona.rs`): Reads + validates TOML at startup;
  asserts every tool name has a registered Rust dispatch handler.

## Consequences

### Positive

- No Python dependency — full Rust, no subprocess overhead per reasoning step.
- New tools require only a Rust implementation + `[[tool]]` TOML block; loop unchanged.
- Safety tiers auditable without reading Rust code — security reviews examine TOML.
- Model-agnostic: ReAct is plain text, works with Qwen2.5 and any instruction model.
- ReAct `Thought:` tokens are shown in the overlay — users see why the agent acts.
- Loop parsing logic is pure functions — unit-testable without a real LLM.

### Negative

- **String parsing fragility**: Model must reliably emit `Action:` / `Final Answer:` tokens.
  Mitigated by explicit format instructions in system_prompt + regex-tolerant parser.
- **No structured output**: Free-text tool calls need parsing vs JSON grammar mode.
  Adds a thin parsing layer; more debuggable in return.
- **TOML/Rust schema drift**: Stale TOML tool names fail silently at dispatch.
  Mitigated by startup validation asserting every TOML tool name has a Rust handler.

## Alternatives Considered

### Alternative A: Python Orchestration (LangChain / LangGraph)

**Why rejected**: Constitution Principle VI prohibits Python orchestration. Requires Python
runtime on user machines. LangSmith telemetry violates Principle I. IPC overhead per
reasoning step adds latency visible in the overlay.

### Alternative B: llama.cpp JSON Grammar Mode (Function Calling)

Structured JSON tool calls via grammar-constrained generation.

**Why rejected for this phase**: Grammar files need maintenance per model version. Full
tool schema in every context window crowds out task history. ReAct `Thought:` tokens
provide better UX transparency. Can be adopted in a future phase without changing TOML schema.

### Alternative C: Hardcoded System Prompt in Rust Constants

**Why rejected**: Directly violates Constitution Principle VI. Every persona adjustment
requires a recompile. Prevents non-developer team members from tuning agent behaviour.

## References

- Feature Spec: `specs/002-agent-task-overlay/spec.md` (FR-007, FR-028, User Story 1)
- Implementation Plan: `specs/002-agent-task-overlay/plan.md` (AD-002, AD-006, Phase 1 T007)
- Research: `specs/002-agent-task-overlay/research.md` (R-002, R-008)
- Related ADRs: ADR-0001 (shared ModelSlot that the loop calls into)
- Constitution: `.specify/memory/constitution.md` (Principle V: PAQ, Principle VI: Persona-as-Config)
