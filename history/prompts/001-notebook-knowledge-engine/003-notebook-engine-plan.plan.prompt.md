---
id: "003"
title: Notebook Knowledge Engine Plan
stage: plan
date: 2026-04-13
surface: agent
model: claude-sonnet-4-6
feature: 001-notebook-knowledge-engine
branch: 001-notebook-knowledge-engine
user: Riaz Mehmood
command: /sp.plan
labels: ["plan","architecture","tauri","rust","solidjs","sqlite","llama-cpp","rag","knowledge-graph"]
links:
  spec: specs/001-notebook-knowledge-engine/spec.md
  ticket: null
  adr: null
  pr: null
files:
  - specs/001-notebook-knowledge-engine/plan.md
  - specs/001-notebook-knowledge-engine/research.md
  - specs/001-notebook-knowledge-engine/data-model.md
  - specs/001-notebook-knowledge-engine/quickstart.md
  - specs/001-notebook-knowledge-engine/contracts/tauri-commands.ts
  - CLAUDE.md
tests:
  - N/A
---

## Prompt

/sp.plan (against specs/001-notebook-knowledge-engine — post /sp.clarify)

Generate full implementation plan for App 1: Notebook & Knowledge Engine.
Includes research, data model, Tauri IPC contracts, quickstart guide, and
complete source layout. Constitution Check must pass all 8 principles.

## Response snapshot

**Phase 0 — Research (research.md)**: 10 decisions documented:
1. Chunking: 512-token, 64-token overlap, semantic boundaries
2. Embedding: nomic-embed-text-v1.5 (768d) via fastembed-rs; MiniLM fallback on 8 GB
3. Entity extraction: two-pass (regex Pass 1 + LLM GBNF Pass 2)
4. Tier routing: rule-based classifier (Tier 0→4 in 5 steps)
5. Semantic cache: L1 in-memory + L2 SQLite (7-day TTL, cosine ≥ 0.92)
6. ModelSlot: Arc<Mutex<ModelSlot>> + 90s Tokio idle eviction timer
7. TaskBudget: 3-state machine (Active/Throttled/Idle), sysinfo + 5s poll
8. Archive format: .echoai (ZIP: manifest.json + sources/ + db.sqlite)
9. PDF parsing: lopdf + Qwen2-VL fallback for scanned docs
10. File watcher: notify crate v6+ with 500ms debounce

**Phase 1 — Design**:
- data-model.md: Full SQLite schema (14 tables, 3 virtual tables: fts5, vec0, auto-triggers)
- contracts/tauri-commands.ts: 30 typed Tauri IPC commands covering all 5 user stories
- quickstart.md: Developer onboarding with model download, env setup, test scenarios
- plan.md: Technical context, Constitution Check (all 8 ✅), architecture decisions (AD-1–5),
  RAM budgets for 8 GB and 16 GB hardware, risk table with mitigations, source layout

**Constitution Check**: All 8 principles pass. No violations.

**2 ADRs recommended** (not auto-created):
- model-hosting-isolation (ModelSlot vs sidecar)
- embedding-model-selection (nomic vs MiniLM schema implications)

## Outcome

- ✅ Impact: Complete implementation plan ready for /sp.tasks; all architecture
  decisions made; data model fully specified; IPC contract defines entire API surface.
- 🧪 Tests: Key test scenarios defined in quickstart.md (P1–P4 manual verification)
- 📁 Files: plan.md, research.md, data-model.md, quickstart.md,
  contracts/tauri-commands.ts, CLAUDE.md (agent context updated)
- 🔁 Next prompts: /sp.adr model-hosting-isolation, /sp.adr embedding-model-selection,
  then /sp.tasks
- 🧠 Reflection: The two-pass entity extraction design is the most important decision
  in the plan — it prevents entity indexing from blocking chat readiness while still
  delivering the knowledge graph. The TaskBudget 3-state machine ensures thermal
  safety without complex heuristics.

## Evaluation notes (flywheel)

- Failure modes observed: None
- Graders run and results (PASS/FAIL): Constitution Check all 8 ✅; RAM budget
  verified mathematically for both hardware tiers; no placeholder tokens remaining.
- Prompt variant (if applicable): N/A
- Next experiment (smallest change to try): Run /sp.tasks to generate dependency-ordered
  task list before implementation.
