---
id: "0001"
title: spec llm pdf importer v2
stage: spec
date: 2026-05-29
surface: agent
model: claude-opus-4-7
feature: 006-llm-pdf-importer
branch: 006-llm-pdf-importer
user: engr.riaz2010@gmail.com
command: /sp.specify
labels: [spec, llm, pdf, importer, ir, feature-005-v2]
links:
  spec: specs/006-llm-pdf-importer/spec.md
  ticket: ""
  adr: ""
  pr: ""
files:
  - path: specs/006-llm-pdf-importer/spec.md
    change: created
  - path: specs/006-llm-pdf-importer/checklists/requirements.md
    change: created
tests:
  - none
---

## Prompt

LLM PDF→JSON importer (v2 of feature 005): local Mistral 7B converts PDFs into the same hierarchical IR JSON that constitution.json uses today, then routes through the existing graph_builder + indexer. Full approved design lives at /home/riaz/.claude/projects/-media-riaz-New-Volume-echo-AI/memory/project_echoai_llm_pdf_to_json_importer_design.md — use it as the source of truth (skip re-brainstorm; all 5 sections approved by user). Key points: sha256-keyed JSON cache in app data dir with 7-day orphan sweep; GBNF grammar + 3 chunk retries / 2 doc retries; partial-success ingest with low-confidence flagging; split-screen repair UI (rendered PDF page + JSON editor + toast notify); user_repaired nodes never overwritten by re-runs; local Mistral 7B only (no cloud in v1), falls back to legacy rule-based parser if GGUF missing. Slot under existing feature 005 (extension/v2, not a new feature folder).

User clarifications during this run:
- Feature folder: chose to create a new branch (`006-llm-pdf-importer`) rather than slot inside `specs/005-…/`. Spec lives at `specs/006-llm-pdf-importer/spec.md`.

## Response snapshot

Created feature branch `006-llm-pdf-importer` (next available number; 005 was the highest existing). Wrote `specs/006-llm-pdf-importer/spec.md` translating the approved 5-section brainstorm into user-facing requirements:

- 3 prioritized user stories (P1 import a previously-unsupported PDF; P2 review/repair low-confidence sections; P3 cache + orphan sweep) — each independently testable.
- 30 functional requirements grouped: Extraction & structure (FR-001…006), Quality gating (FR-007…009), Partial-success ingest & repair (FR-010…018), Cache & lifecycle (FR-019…023), Progress/concurrency/cancel (FR-024…026), Fallback (FR-027…028), Provenance (FR-029…030).
- 7 Key Entities, 6 Assumptions, 5 out-of-scope items.
- 9 measurable Success Criteria (SC-001 anchors to V5 22-prompt eval at commit `2f6a6d77`; SC-002 ≥90% text coverage; SC-003 ≥95% nodes pass without repair; SC-006 cached re-import <5s).

Wrote `specs/006-llm-pdf-importer/checklists/requirements.md`. All 16 checklist items pass on first self-review iteration. No `[NEEDS CLARIFICATION]` markers needed — the brainstorm already covered the ambiguities.

Deliberately kept implementation specifics (Mistral 7B, GBNF, sha256, llama.cpp) out of the spec. Those land in plan.md.

## Outcome

- ✅ Impact: feature 006 scaffolded with spec + checklist; ready for `/sp.plan`. v2 LLM upgrade path to feature 005 is now formally tracked.
- 🧪 Tests: none yet (spec stage; acceptance scenarios are the test contract for plan/tasks stages).
- 📁 Files: 2 created (spec.md, checklists/requirements.md).
- 🔁 Next prompts: `/sp.plan` to design the IR provider trait, GBNF grammar, validator, cache, repair UI plumbing; then `/sp.tasks`.
- 🧠 Reflection: skipping re-brainstorm and going straight from approved memory to spec saved a full design pass. The brainstorm memory file was thorough enough to translate directly into FRs/SCs without re-asking the user anything.

## Evaluation notes (flywheel)

- Failure modes observed: none in this run.
- Graders run and results (PASS/FAIL): self-review checklist 16/16 PASS.
- Prompt variant (if applicable): n/a.
- Next experiment (smallest change to try): on `/sp.plan`, see if the IR provider trait can be designed without changing existing `constitution_ir.rs` consumers (i.e. typed view over generic base IR, as section 3 of brainstorm pinned).
