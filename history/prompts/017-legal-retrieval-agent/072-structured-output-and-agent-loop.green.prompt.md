---
id: 072
title: Structured-output seam + agentic loop foundation
stage: green
date: 2026-07-08
surface: agent
model: claude-opus-4-8
feature: 017-legal-retrieval-agent
branch: release
user: Riaz Mehmood
command: work on for agentic approach integration and structured output extension
labels: [agent, F017, structured-output, inference-seam, TextGenerator]
links:
  spec: specs/017-legal-retrieval-agent/spec.md
  ticket:
  adr: history/adr/0018-server-side-inference-engine-vllm-continuous-batching-sidecar.md
  pr:
files:
  - src-tauri/src/services/inference/mod.rs
  - src-tauri/src/services/inference/remote.rs
  - src-tauri/src/services/inference/vllm.rs
  - src-tauri/src/services/query_router.rs
  - src-tauri/src/services/agent/mod.rs
  - src-tauri/src/services/agent/tools.rs
  - src-tauri/src/services/mod.rs
  - CHANGELOG_DEV.md
tests:
  - "VM cargo check --no-default-features --features llm,server,cuda: EXIT=0"
  - "VM cargo test --no-run --lib: EXIT=0"
  - "VM cargo test -- services::agent services::inference: 29 passed / 0 failed"
---

## Prompt

work on for agentic approach integration and structured output extension.

## Response snapshot

Built the F017 foundation in two layers on the F016 `TextGenerator` seam.

Layer 1 (structured output): added `ResponseFormat` (Text/JsonObject/JsonSchema) + a
`GenParams.response_format` field + a non-streaming `complete()` trait method (default impl
drains the stream). RemoteGenerator attaches OpenAI `response_format`; VllmGenerator maps to
`response_format`/`guided_json`. Hardened `chatml_to_messages` to fall back to a single user
message for plain prompts.

Layer 2 (agent): new server-only `services/agent/` — a `Tool`/`ToolRegistry` abstraction and
`run_agent()`, a bounded LLM-controller/Rust-executor loop. Controller decisions come back as
schema-constrained JSON (`parse_action`, brace-balanced, fence-tolerant, rejects unregistered
tools). Enforces MAX_ITER cap (FR-005), no-progress stop (FR-007), grounded refusal on
no-evidence/malformed output (FR-006), bounded retry (FR-003). Final answer via `synthesize()`.

Verified on the VM against the real prod feature set: check + test-build EXIT=0; 29 agent/inference
unit tests pass (scripted mock generator + mock tool cover tool→finish, refusal, no-progress,
malformed-safety, structured-body shapes).

## Outcome

- ✅ Impact: agent loop + structured-output seam exist, compile, and are unit-tested; run on any HTTP engine (vLLM or remote) with no engine coupling (FR-014).
- 🧪 Tests: 29 passed / 0 failed (services::agent + services::inference), on VM.
- 📁 Files: seam extension (3 inference files + 1 GenParams literal fix) + new agent module (2 files) + module registration.
- 🔁 Next prompts: wrap existing hybrid retrieval as a `HybridSearchTool`; wire `run_agent` behind an agent on/off flag (FR-012) with `chat://` status emit (FR-010) + fast-path gate (FR-013); live-verify guided_json/response_format JSON conformance; build the golden-set eval (FR-016).
- 🧠 Reflection: keeping `run_agent` DI'd on `&dyn TextGenerator` + `ToolRegistry` made the whole control state-machine unit-testable with a scripted mock — no model or DB needed to prove the loop bounds and safety.

## Evaluation notes (flywheel)

- Failure modes observed: one compile break — adding `GenParams.response_format` broke the `vllm_infer` struct literal (fixed with `..default()`); caught by VM cargo check.
- Graders run and results (PASS/FAIL): unit PASS (29/29). No golden-set eval yet.
- Prompt variant (if applicable): n/a
- Next experiment (smallest change to try): add `HybridSearchTool` + run one real multi-hop legal question through `run_agent` end-to-end on the remote engine; confirm the controller emits valid JSON actions and finishes with a cited answer.
