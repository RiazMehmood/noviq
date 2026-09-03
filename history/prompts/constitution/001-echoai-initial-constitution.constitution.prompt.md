---
id: "001"
title: EchoAI Initial Constitution
stage: constitution
date: 2026-04-13
surface: agent
model: claude-sonnet-4-6
feature: none
branch: master
user: Riaz Mehmood
command: /sp.constitution
labels: ["constitution","architecture","privacy","local-ai","agents","hardware-optimization"]
links:
  spec: null
  ticket: null
  adr: null
  pr: null
files:
  - .specify/memory/constitution.md
  - .specify/templates/plan-template.md
  - history/prompts/constitution/001-echoai-initial-constitution.constitution.prompt.md
tests:
  - N/A
---

## Prompt

How we will build agents? Ans this then go for /sp.constitute.

[Full agent architecture discussion preceded this command. Key question: how to build
multi-agent systems on low-RAM local hardware without Python frameworks or multiple
simultaneous LLM processes.]

Agent architecture established:
- Orchestrator (3B, always warm) + single Worker slot (7B, demand-loaded)
- Personas defined as TOML config (system prompt + tool schema)
- ReAct loop in Rust using llama.cpp tool_call + GBNF grammar
- SQLite Blackboard pattern for agent communication
- Pending Actions Queue as mandatory safety gate for Dangerous-tier tools
- Tool safety tiers: READ-ONLY / CAUTIOUS / DANGEROUS
- Wasmtime/WASI for sandboxed code execution

Then /sp.constitution was invoked to encode all architectural decisions into the project
constitution.

## Response snapshot

Created EchoAI Constitution v1.0.0 at `.specify/memory/constitution.md` with 8 core
principles:

1. Privacy-First, Offline-Always (NON-NEGOTIABLE)
2. Tiered Intelligence — Cheapest Tier First (5-tier query routing)
3. Single Model Slot — Bounded RAM Budget (Q4_K_M minimum, 90s idle eviction)
4. SQLite as Single Source of Truth (sqlite-vec + FTS5, no external DB processes)
5. Pending Actions Queue — Safety Gate (NON-NEGOTIABLE)
6. Persona-as-Config — Agents Defined in TOML (no Python orchestration)
7. Hardware-Adaptive Behavior (4 hardware tiers, graceful degradation)
8. Incremental Feature-App Delivery (4 Feature-Apps, independently shippable)

Technology stack invariants locked: Tauri 2.0, SolidJS, Rust, llama.cpp, fastembed-rs,
whisper.cpp, Kokoro TTS, sqlite-vec, GGUF Q4_K_M, Wasmtime/WASI.

Updated `.specify/templates/plan-template.md` Constitution Check section with gates
for all 8 principles.

## Outcome

- ✅ Impact: Project constitution established; all architectural decisions from the
  EchoAI design discussion encoded as enforceable, testable principles.
- 🧪 Tests: N/A (constitution is a governance document)
- 📁 Files: .specify/memory/constitution.md (created), .specify/templates/plan-template.md (updated)
- 🔁 Next prompts: /sp.specify for App 1 (Notebook + Knowledge Engine)
- 🧠 Reflection: The tiered intelligence architecture and single model slot constraint
  are the two most critical principles — they make hardware frugality enforceable rather
  than aspirational. Persona-as-Config ensures agent extensibility without code changes.

## Evaluation notes (flywheel)

- Failure modes observed: None
- Graders run and results (PASS/FAIL): No automated graders; manual validation — no
  unresolved bracket tokens, all 8 principles declarative and testable, dates ISO format.
- Prompt variant (if applicable): N/A
- Next experiment (smallest change to try): Add ADR for agent execution model and
  model hosting isolation strategy before first spec.
