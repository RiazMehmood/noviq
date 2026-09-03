---
id: "0005"
title: implement phase 1 and 2
stage: green
date: 2026-05-29
surface: agent
model: claude-opus-4-7
feature: 006-llm-pdf-importer
branch: 006-llm-pdf-importer
user: engr.riaz2010@gmail.com
command: /sp.implement
labels: [green, phase-2, validator, doc-ir, jsonschema, ntfs-target]
links:
  spec: specs/006-llm-pdf-importer/spec.md
  ticket: ""
  adr: ""
  pr: ""
files:
  - path: src-tauri/Cargo.toml
    change: edited
  - path: src-tauri/src/services/mod.rs
    change: edited
  - path: src-tauri/src/db/repositories/mod.rs
    change: edited
  - path: src-tauri/src/services/doc_ir/mod.rs
    change: created
  - path: src-tauri/src/services/doc_ir/types.rs
    change: created
  - path: src-tauri/src/services/doc_ir/schema.json
    change: created
  - path: src-tauri/src/services/doc_ir/validator.rs
    change: created
  - path: src-tauri/src/services/doc_ir/builder.rs
    change: created
  - path: src-tauri/src/services/doc_ir/progress.rs
    change: created
  - path: src-tauri/src/services/doc_ir/converter_from_constitution.rs
    change: created
  - path: src-tauri/src/services/doc_ir/cache.rs
    change: created
  - path: src-tauri/src/services/doc_ir/chunker.rs
    change: created
  - path: src-tauri/src/services/doc_ir/prompts.rs
    change: created
  - path: src-tauri/src/services/doc_ir/repair.rs
    change: created
  - path: src-tauri/src/services/doc_ir/retry.rs
    change: created
  - path: src-tauri/src/services/doc_ir/queue.rs
    change: created
  - path: src-tauri/src/services/doc_ir/grammar.rs
    change: created
  - path: src-tauri/src/services/doc_ir/mistral_builder.rs
    change: created
  - path: src-tauri/src/db/migrations/018_llm_ir_importer.sql
    change: created
  - path: src-tauri/src/db/repositories/ir_cache.rs
    change: created
  - path: src-tauri/tests/ir_validator.rs
    change: created
  - path: src-tauri/tests/ir_converter_parity.rs
    change: created
tests:
  - target: cargo test --test ir_validator
    status: 16/16 PASS
  - target: cargo test --test ir_converter_parity
    status: 4/4 PASS
  - target: cargo test --lib doc_ir/ir_cache
    status: 17/17 PASS (validator inline, converter inline, ir_cache inline)
---

## Prompt

Execute tasks.md for feature 006 on branch 006-llm-pdf-importer. Start with Phase 1 + Phase 2 (T001–T014 + T010b/c), validate `cargo test --test ir_validator` green, then proceed to US1 if time allows. Spec / plan / data-model / contracts / tasks are all in specs/006-llm-pdf-importer/. Constitution: smallest viable diff, additive migrations only, no refactoring of unrelated code.

## Response snapshot

Completed **T001–T014 + T010 + T010c** (Phase 1 + Phase 2, minus T010b which depends on the Phase-3 chunker). Tests **37/37 GREEN**.

**Setup (T001–T004):**
- Added `jsonschema = "0.18"` (with `draft202012` feature — required since the schema is JSON Schema 2020-12) and `async-trait = "0.1"` to `Cargo.toml`.
- Scaffolded `src-tauri/src/services/doc_ir/` with 13 sub-modules (`types`, `validator`, `builder`, `progress`, `converter_from_constitution`, `cache`, `chunker`, `prompts`, `repair`, `retry`, `queue`, `grammar`, `mistral_builder`) — last two feature-gated under `llm`.
- Registered the module in `services/mod.rs`. Added `LLM_IR_IMPORTER_VERSION = 1` + `MIN_TEXT_CHARS = 200`.

**Foundational (T005–T014):**
- Migration 018 written **with the I2 fix**: dropped duplicate `ALTER TABLE sources ADD COLUMN extraction_method / quality_score` (already added by migration 016). Kept `ir_cache_sha`, `ir_quality_score`, `nodes_low_confidence` as new columns. Migration applies cleanly via `sqlx::migrate!`.
- IR types (`DocIr`, `StructureNode`, `NodeKind`, `QualityFlag`, `Annotation`, `AnnotationKind`, `DocType`, `DocIrEnvelope`) with serde rename_all = "snake_case". Canonical JSON schema at `services/doc_ir/schema.json` embedded via `include_str!`.
- Validator: two-pass (`validate_schema` via `jsonschema::JSONSchema` + `validate_semantic` walking the IR). 12 codes implemented (`MissingRequiredField`, `InvalidEnum`, `OrphanParent`, `CycleDetected`, `DuplicateId`, `EmptyLeafText`, `LowCoverage`, `NonMonotonicNumber`, `UnresolvedFootnoteRef`, `AnnotationMissingBy`, `OmissionMissingMarker`, `PageOutOfRange`). Coverage uses **non-whitespace character count** per remediation A4.
- `IrBuilder` trait + `BuilderInput`/`BuilderOutput`/`IrBuildError`/`RetryCounts` per contract. Built around `async-trait` so trait objects are dispatched in the indexer.
- `ProgressSink` enum (`Noop` for tests, `Tauri(AppHandle)` for prod) emitting on `importer://progress`.
- `constitution_ir_to_doc_ir` converter — full nested-tree → flat-nodes walk, preserving footnote references (FR-003) and amendment annotations (FR-004).
- `ir_cache` SQLx repo: `insert_or_update`, `mark_orphaned`, `clear_orphan`, `find_orphans_older_than`, `delete`, `get`, `list_needing_review`.

**Two issues hit during testing**, both fixed in <5 lines:
1. Schema rejected `metadata: null` because the schema required `object`. Relaxed schema to `["object", "null"]` for both `doc_metadata` and `metadata`.
2. `jsonschema`'s enum-error message phrasing is "X is not one of [...]", not "enum". `classify_schema_error` now recognizes "not one of" / "does not match" / " enum" / "const" as `InvalidEnum`.

**Operational incident**: cargo dirs initially set to `$HOME/.cache/echo-ai-{target,cargo}` which is on `/dev/sda5` (root) — caused root partition to hit 100% used (4.1 GB of build output on 39/41 GB used disk). User flagged it ("dont write anything to root partition"). Resolved by:
- `pkill -9 cargo rustc`
- `rm -rf ~/.cache/echo-ai-target ~/.cache/echo-ai-cargo` (recovered 4 GB)
- Switched to the existing NTFS dirs at `/media/riaz/New Volume/cargo-{target,home}`. Subsequent `cargo check + test` runs stayed off root.

This re-confirms `feedback_ntfs_cargo_target` and `feedback_no_root_disk` — should never have used `$HOME/.cache` to begin with. Recording in CHANGELOG_DEV.md so it doesn't happen again.

**Test results:**
- `cargo test --test ir_validator` → 16/16 (covers all 12 issue codes + page bounds + monotonic + valid passthrough)
- `cargo test --test ir_converter_parity` → 4/4 (kind counts, parent chain, footnote-ref resolution, annotation populated)
- `cargo test --lib -- doc_ir ir_cache` → 17/17 (11 validator inline + 3 converter inline + 3 ir_cache inline)
- **Total Phase 2 tests: 37/37 GREEN**

## Outcome

- ✅ Impact: Phase 1+2 complete on `006-llm-pdf-importer`. The foundational types, schema, validator, migration, trait, progress sink, converter, and SQLite repo all land green. Phase 3 (US1 — chunker, prompts, llguidance binding, MistralIrBuilder, retry, dispatch wiring) can now begin against a known-good substrate.
- 🧪 Tests: 37/37 green, all NTFS-resident, no root-disk usage.
- 📁 Files: 23 created, 3 edited (`Cargo.toml`, `services/mod.rs`, `db/repositories/mod.rs`). Tasks.md marked `[X]` for T001–T014 + T010 + T010c (T010b deferred to Phase 3).
- 🔁 Next prompts: continue `/sp.implement` for Phase 3 (US1) — T015–T030 plus T010b. Mistral 7B GGUF must be present at `models/mistral-7b-instruct-v0.3.Q4_K_M.gguf` to validate the runtime path; otherwise the fallback path will be exercised.
- 🧠 Reflection: two technical wins: (1) `jsonschema` v0.18 needs the `draft202012` feature flag — easy miss; (2) llguidance JSON-schema mode is in llama-cpp-2 0.1.143 natively, so the same `schema.json` will feed both sampler and validator in Phase 3 — single source of truth confirmed. One process loss: I burned ~20 minutes building on root disk before the user caught it. Tighter pre-flight: always `df -h $HOME` before setting CARGO_TARGET_DIR.

## Evaluation notes (flywheel)

- Failure modes observed: jsonschema feature flag missed; metadata: null schema mismatch; enum-error classification missed "not one of" phrasing; cargo-target on root partition (operational).
- Graders run and results (PASS/FAIL): all 37 tests PASS.
- Prompt variant (if applicable): n/a.
- Next experiment (smallest change to try): in T021 (Mistral builder), measure first-attempt JSON-validity rate with the llguidance JSON-schema constraint vs. unconstrained baseline on a 3-chunk sample — the brainstorm assumed 85%→99% lift; record actuals for future constrained-decoding planning.
