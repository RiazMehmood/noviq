---
id: "0003"
title: "plan legal RAG feature"
stage: plan
date: 2026-06-02
surface: agent
model: claude-opus-4-7
feature: 007-legal-rag-pakistan
branch: 007-legal-rag-pakistan
user: Riaz Mehmood
command: /sp.plan
labels: [feature-007, legal-rag, pakistan, plan]
links:
  spec: specs/007-legal-rag-pakistan/spec.md
  ticket: ""
  adr: ""
  pr: ""
files:
  - path: specs/007-legal-rag-pakistan/plan.md
    change: created
  - path: specs/007-legal-rag-pakistan/research.md
    change: created
  - path: specs/007-legal-rag-pakistan/data-model.md
    change: created
  - path: specs/007-legal-rag-pakistan/contracts/tauri-commands.md
    change: created
  - path: specs/007-legal-rag-pakistan/contracts/intent-classifier.schema.json
    change: created
  - path: specs/007-legal-rag-pakistan/contracts/legal-ir.schema.json
    change: created
  - path: specs/007-legal-rag-pakistan/contracts/procedure.schema.json
    change: created
  - path: specs/007-legal-rag-pakistan/quickstart.md
    change: created
  - path: CLAUDE.md
    change: modified (agent context refreshed)
tests:
  - name: Constitution Check (13 principles)
    status: PASS
  - name: Post-Phase-1 Constitution re-check
    status: PASS
---

## Prompt

`/sp.plan` invoked with no user-input block. Goal: take the clarified spec (`specs/007-legal-rag-pakistan/spec.md`) and constitution v1.1.0 (13 principles) and produce the Phase 0–1 SDD artifacts: plan.md, research.md, data-model.md, contracts/, quickstart.md, plus the agent-context refresh.

## Response snapshot

Wrote:

- **`plan.md`** — Summary, Technical Context, Constitution Check (PASS on all 13 principles, no violations, Complexity Tracking empty), Project Structure (single Tauri project, additive module layout under `src-tauri/src/services/{citation,legal_ir,deep_reindex,intent_router}/`), Slice Plan (Slice 0 Foundation already drafted in earlier superpowers work + Slice A/B/C/D each with eval gates), Phase 0/1 pointers, and a post-Phase-1 re-check note.

- **`research.md`** — 13 R-numbered decisions covering: default offline LLM tier composition (R1), cloud-only-for-eval policy (R2), grammar-constrained IR generation via `llguidance` (R3), two-tier indexing (R4), Section-boundary chunking on deep tier (R5), deterministic amendment-marker parser (R6), two-stage cross-reference edge extraction (R7), two-stage procedure extraction (R8), rule-first intent classifier (R9), single canonical citation formatter (R10), quality-floor model per tier (R11), 70-prompt officer eval construction (R12), engine inheritance from Feature 006 (R13). Each decision records the chosen path, rationale, and rejected alternatives. Two `/sp.clarify`-deferred items (fast-index UX feedback, JSON sidecar input format) resolved inline at plan level.

- **`data-model.md`** — Entities E1–E9: document tier (E1, sticky transitions), Instrument (E2), legal NodeKind enum (E3), AmendmentEvent (E4), GraphEdge (E5), Procedure (E6), ProcedureStep (E7), JSON IR sidecar (E8, cache not authoritative), in-memory ChatQuery + IntentClassification (E9). Validation rules table, ER sketch, migration ordering (strictly additive — Migration 020), and the only data-removal operation (per-doc deep→fast downgrade).

- **`contracts/tauri-commands.md`** — Five command contracts: `deep_reindex_document` (CAUTIOUS), `cancel_deep_reindex` (READ-ONLY), `downgrade_document_to_fast` (DANGEROUS — UI confirmation required per §V), extended `chat_query` (READ-ONLY) with `Scope` discriminated union and `ChatResponse`/`ChatBubble`/`Citation`/`UpgradePrompt`/`FlowDiagram` shapes, and `set_scope` (unchanged). Latency contracts inline (SC-011). Progress event channel `importer://progress` with `phase_tick` ≥ every 10 s.

- **`contracts/intent-classifier.schema.json`** — Grammar-constrained classifier output: `intent` (9-value enum including `fallback`), `confidence` ∈ [0,1], `slots` (instrument, section_path, topic, comparands ≤ 8, timeline_anchor).

- **`contracts/legal-ir.schema.json`** — Mistral output / sidecar shape: `schema_version: 16` constant, `instrument_title/id/year`, `nodes[]` with `kind` (15-value legal enum), `parent_id` (null only at root), optional `amendments[]` per node with AmendmentEvent shape.

- **`contracts/procedure.schema.json`** — Slice D extraction shape: `process_name`, `steps[]` with `order`, `actor` (free-text v1 — examples Director/Secretary/Town Officer/Municipal Officer/SHO/DPO/DC/Commissioner), `action`, `condition`, `branch_of_order`, `timeline`, `source_node_id`.

- **`quickstart.md`** — Officer walkthrough mapped one-to-one onto US1/US2/US3. Prerequisites list explicit (three Sindh PDFs, three GGUFs on disk, no internet). Acceptance signals per-slice table. Troubleshooting (RAM tier insufficient, Mistral hang/retry, intent misclassification, citation correction).

Ran `.specify/scripts/bash/update-agent-context.sh claude` — `CLAUDE.md` updated with Feature 007 technologies under Active Technologies; Recent Changes line for 007 appended.

Constitution Check: PASS on all 13 principles before Phase 0 and re-checked after Phase 1. No violations, Complexity Tracking section intentionally empty.

## Outcome

- ✅ Impact: Phase 0 + Phase 1 SDD artifacts complete. `/sp.tasks` can now generate a fully-detailed task list from this plan + research + data-model + contracts.
- 🧪 Tests: Constitution Check PASS (13/13 principles); Post-Phase-1 re-check PASS.
- 📁 Files: plan.md, research.md, data-model.md, contracts/{tauri-commands.md, intent-classifier.schema.json, legal-ir.schema.json, procedure.schema.json}, quickstart.md, CLAUDE.md (refreshed).
- 🔁 Next prompts: `/sp.tasks` to generate the dependency-ordered task list; `/sp.analyze` to cross-check spec/plan/tasks; `/sp.implement`. Optional `/sp.adr` for the legal-pivot architectural decision (see ADR suggestion below).
- 🧠 Reflection: The brainstorming-era design doc (`docs/superpowers/specs/2026-06-02-legal-rag-pakistan-design.md`) carried 95% of the work; this `/sp.plan` was largely a translation pass into the SDD format. The biggest value-add was the explicit Constitution Check, which made the §V (Pending Actions Queue) classification of each new Tauri command explicit (downgrade is DANGEROUS — UI must confirm).

## ADR suggestion

📋 Architectural decision detected: **Hard pivot from domain-neutral to Pakistan government legal RAG**, with two-tier indexing (fast default + Mistral deep on demand) as the central new architectural choice — long-term impact on data model, retrieval pipeline, and product framing; multiple viable alternatives evaluated (single-tier deep-on-upload, always-fast, auto-upgrade-on-demand); cross-cutting across constitution, spec, plan, data model, and UI. Recommend running `/sp.adr legal-pivot-two-tier-indexing` to formally record the decision and its tradeoffs. Awaiting user consent before creating.

## Evaluation notes (flywheel)

- Failure modes observed: none.
- Graders run and results (PASS/FAIL): Constitution Check PASS; Post-Phase-1 re-check PASS.
- Prompt variant (if applicable): n/a.
- Next experiment (smallest change to try): when `/sp.tasks` runs, audit whether the Slice A/B/C/D structure in plan.md surfaces as natural task groupings without manual hinting.
