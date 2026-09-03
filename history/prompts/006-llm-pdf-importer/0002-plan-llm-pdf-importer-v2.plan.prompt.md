---
id: "0002"
title: plan llm pdf importer v2
stage: plan
date: 2026-05-29
surface: agent
model: claude-opus-4-7
feature: 006-llm-pdf-importer
branch: 006-llm-pdf-importer
user: engr.riaz2010@gmail.com
command: /sp.plan
labels: [plan, llm, pdf, importer, llguidance, mistral-7b, constitution-check]
links:
  spec: specs/006-llm-pdf-importer/spec.md
  ticket: ""
  adr: ""
  pr: ""
files:
  - path: specs/006-llm-pdf-importer/plan.md
    change: filled
  - path: specs/006-llm-pdf-importer/research.md
    change: created
  - path: specs/006-llm-pdf-importer/data-model.md
    change: created
  - path: specs/006-llm-pdf-importer/contracts/ir_builder_trait.md
    change: created
  - path: specs/006-llm-pdf-importer/contracts/tauri_commands.md
    change: created
  - path: specs/006-llm-pdf-importer/contracts/ir_json_schema.md
    change: created
  - path: specs/006-llm-pdf-importer/quickstart.md
    change: created
  - path: CLAUDE.md
    change: agent-context-updated
tests:
  - target: cargo test --test llm_ir_importer_acceptance
    status: planned
  - target: cargo test --test ir_validator
    status: planned
  - target: cargo test --test ir_cache_lifecycle
    status: planned
---

## Prompt

Plan feature 006 (LLM PDF→JSON importer) on branch 006-llm-pdf-importer. Spec at specs/006-llm-pdf-importer/spec.md is the requirements source-of-truth. Approved implementation design (Mistral 7B local, GBNF, sha256 cache, repair UI, etc.) lives in memory file /home/riaz/.claude/projects/-media-riaz-New-Volume-echo-AI/memory/project_echoai_llm_pdf_to_json_importer_design.md — use it as the technical design source-of-truth, do not re-litigate. Stack: Rust 1.87 + SolidJS, SQLite at echo.db. Slot the LLM path beside existing rule-based parsers in src-tauri/src/services/doc_profiles/.

## Response snapshot

Generated full Phase 0 + Phase 1 artefacts for feature 006 on branch `006-llm-pdf-importer`.

**Phase 0 (research.md)** — 9 technology decisions:
- R1: llama-cpp-2 0.1.143's **llguidance constrained sampler** with a JSON-schema grammar — replaces the brainstorm's "GBNF" framing because llguidance is strictly more capable and is what the crate ships.
- R2: Mistral 7B Instruct v0.3 Q4_K_M as a Tier-4 demand-loaded worker (Constitution III).
- R3: New `DocIr` type alongside legacy `ConstitutionIr`; one-way converter to keep Feature 003 acceptance intact.
- R4: Two-pass validator — `jsonschema` crate for schema, Rust fn for semantic (parent integrity, cycles, dup ids, monotonic numbering, ≥90% coverage, footnote refs, omission markers, page bounds).
- R5: Chunker = heading-boundary pre-pass (reuse `heading_detector::regex_pass`) + 2400-token budget; carries breadcrumb context.
- R6: Cache at `{app_data_dir}/ir_cache/<sha>.json`; SQLite index table `ir_cache_index` via migration 018; daily orphan sweep via `scheduler.rs`.
- R7: pdftoppm at 150 DPI for the repair view PDF panel; LRU page cache capped ~40 MB.
- R8: Partial re-index on repair save — drop+re-embed only the edited nodes' chunks; rebuild only their incident graph edges.
- R9: `IrBuilder` trait with one impl `MistralIrBuilder`; ordered dispatch in `indexer::do_index` — constitution → LLM IR → rule-based → flat hierarchical.

**Phase 1 (data-model.md + contracts/ + quickstart.md)** — Rust types (`DocIr`, `StructureNode`, `NodeKind`, `QualityFlag`, `Annotation`, `AnnotationKind`), JSON envelope on disk, migration 018 SQL (additive only — `ir_cache_index`, `ir_repair_log`, 4 columns on `sources`), cache+node lifecycle diagrams, 12 validation issue codes. Three contract docs: trait+dispatch, Tauri command+event wire format, full JSON schema.

**Constitution Check** — passed 8/8 with 3 JUSTIFIED items captured in Complexity Tracking:
1. Mistral 7B monopolizes Tier-4 worker during conversion (acceptable: user-initiated, progress-visible, released between docs, chat keeps Tier 3).
2. Two parallel IR types coexist temporarily (smallest-diff: refactoring `ConstitutionIr` now re-opens Feature 003 gate).
3. JSON payloads on disk vs SQLite blobs (derived artefacts; index in SQLite preserves Principle IV intent).

Re-check after Phase 1: still 8/8. No new violations from contracts.

**Agent context** — updated CLAUDE.md via update-agent-context.sh (added llama-cpp-2 llguidance, jsonschema, async-trait, pdftoppm, Mistral 7B GGUF to active stack).

**Project structure** — single new module `src-tauri/src/services/doc_ir/` (13 files) + 1 migration + 1 new command file + 4 new frontend files + 3 new test files. Existing files touched: `indexer.rs` (dispatch branch), `scheduler.rs` (sweep tick), `commands/mod.rs` + `lib.rs` (register).

## Outcome

- ✅ Impact: feature 006 Phase 0 + Phase 1 complete; ready for `/sp.tasks`. 8/8 constitution gate pass (3 justified exceptions). 30 FRs mapped to concrete code locations.
- 🧪 Tests: 3 new test targets planned — `llm_ir_importer_acceptance` (P1+P2+P3 gate, model-free frozen stub), `ir_validator` (unit), `ir_cache_lifecycle` (state transitions + sweep).
- 📁 Files: plan.md filled; 6 new artefacts (research, data-model, 3 contracts, quickstart); CLAUDE.md agent-context updated.
- 🔁 Next prompts: `/sp.tasks` to produce dependency-ordered tasks.md; optional `/sp.adr "Local-only LLM IR importer + flat-nodes DocIr + partial-success repair"` to capture the 3 architectural decisions logged in Complexity Tracking.
- 🧠 Reflection: llguidance turned out to be a strict upgrade over the brainstorm's GBNF assumption — already in the bound llama-cpp-2 version and accepts JSON-schema directly, which means one schema file feeds both the sampler and the validator. Worth recording as a lesson when planning constrained-output features.

## Evaluation notes (flywheel)

- Failure modes observed: none.
- Graders run and results (PASS/FAIL): constitution-check 8/8 PASS (3 justified); no [NEEDS CLARIFICATION] left after research.md.
- Prompt variant (if applicable): n/a.
- Next experiment (smallest change to try): on `/sp.tasks`, see whether the single new module can be implemented in <12 dependency-ordered tasks while still allowing P1, P2, P3 to ship independently per Constitution VIII.
