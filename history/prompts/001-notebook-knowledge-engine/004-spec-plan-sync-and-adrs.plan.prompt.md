---
id: "004"
title: Spec Plan Sync and ADRs
stage: plan
date: 2026-04-13
surface: agent
model: claude-sonnet-4-6
feature: 001-notebook-knowledge-engine
branch: 001-notebook-knowledge-engine
user: Riaz Mehmood
command: sync specs with plan then /sp.adr model-hosting-isolation and embedding-model-selection
labels: ["plan","adr","sync","model-hosting","embedding","sqlite-vec","llama-cpp"]
links:
  spec: specs/001-notebook-knowledge-engine/spec.md
  ticket: null
  adr: history/adr/0001-llm-model-hosting-isolation-strategy.md
  pr: null
files:
  - specs/001-notebook-knowledge-engine/spec.md
  - history/adr/0001-llm-model-hosting-isolation-strategy.md
  - history/adr/0002-embedding-model-selection-and-schema-strategy.md
tests:
  - N/A
---

## Prompt

First sync specs with plan, then go /sp.adr both issues (model-hosting-isolation
and embedding-model-selection).

## Response snapshot

**Spec-Plan Sync (4 gaps fixed)**:
1. FR-017: Extended semantic cache scope from "same session" to "cross-session 7-day window"
2. US1 scenario 5: Added 8 GB hardware conditional for vision model unavailability
3. Assumptions file size: Clarified 500 MB full store / 500 MB–2 GB transcription-only / >2 GB rejected
4. Assumptions: Added embedding model first-run internet exception with bundled MiniLM fallback

**ADR-0001: LLM Model Hosting Isolation Strategy** (Accepted)
Decision: In-process ModelSlot singleton for Tier 3/4; separate VisionSlot for Tier 5.
Rejected: sidecar process (IPC latency, lifecycle overhead), WASM (4 GB memory limit),
Python subprocess (violates Principle VI).
Key tradeoff: LLM crash takes down UI process, mitigated by catch_unwind + SQLite WAL.

**ADR-0002: Embedding Model Selection and Schema Strategy** (Accepted)
Decision: nomic-embed-text-v1.5 (768d, primary) + all-MiniLM-L6-v2 (384d, bundled fallback);
hardware-detected at first launch; DIM baked into db_config and vec0 schema at creation.
Rejected: single model only (quality vs offline tradeoff), runtime switching (mathematically
impossible), BGE-M3 1024d (storage too large for desktop, multilingual deferred to v1.1).
Key tradeoff: schema dimension is irreversible — model upgrade requires full re-index.

## Outcome

- ✅ Impact: Spec fully synced with plan (4 gaps); 2 ADRs created covering the two
  highest-risk architectural decisions before implementation begins.
- 🧪 Tests: N/A
- 📁 Files: spec.md (4 edits), ADR-0001, ADR-0002
- 🔁 Next prompts: /sp.tasks to generate dependency-ordered implementation task list
- 🧠 Reflection: The embedding model ADR is the most consequential — the vec0 schema
  dimension is irreversible, making this a permanent architectural commitment. Capturing
  it as an ADR before /sp.tasks prevents the schema from being decided implicitly during
  task implementation.

## Evaluation notes (flywheel)

- Failure modes observed: None
- Graders run and results (PASS/FAIL): Both ADRs pass all 4 checklist criteria —
  clustered decisions, explicit alternatives, positive/negative consequences, references.
- Prompt variant (if applicable): N/A
- Next experiment (smallest change to try): /sp.tasks
