# Phase 0 Research — LLM PDF → IR Importer

**Feature**: 006-llm-pdf-importer
**Date**: 2026-05-29
**Spec**: [spec.md](./spec.md)

This phase resolves every NEEDS-CLARIFICATION raised in Technical Context. The brainstorm memory `project_echoai_llm_pdf_to_json_importer_design.md` is treated as the design source-of-truth; this document records technology-level decisions needed to implement it on the current stack.

---

## R1 — Constrained JSON decoding for llama.cpp in-process

**Decision**: Use llama-cpp-2's built-in **llguidance** constrained-sampler (`create_llg_sampler` in `llama-cpp-2-0.1.143/src/llguidance_sampler.rs`) with a **JSON-schema grammar**. Drop the brainstorm's "GBNF" framing — llguidance is strictly more capable (JSON-schema, Lark, regex) and is the one already wired into the bound version of llama-cpp-2.

**Rationale**:
- The repo already depends on `llama-cpp-2 = "0.1"` and the installed crate is 0.1.143 which ships llguidance support.
- JSON-schema grammar lets the validator (R4) reuse the same schema artefact the sampler consumes — single source of truth.
- llguidance enforces token-level constraints, lifting first-attempt valid-JSON from ~85% (unconstrained) to ~99% per the brainstorm's accepted estimate.

**Alternatives considered**:
- *Raw GBNF*: matches the brainstorm wording but requires hand-authored grammar files; llguidance JSON-schema gives the same enforcement from the schema we already need.
- *Post-hoc JSON repair (e.g. `serde_json::from_str` → retry)*: works but cost is much higher — a malformed token mid-stream wastes the whole chunk; constrained decoding avoids the waste.
- *Cloud-hosted structured output*: violates Constitution Principle I.

---

## R2 — Model selection and tier placement

**Decision**: Use **Mistral 7B Instruct v0.3 GGUF Q4_K_M** as the IR builder. Load via the existing `priority_model_slot::infer_with_cancel` path at `n_ctx=4096`, `max_prompt_tokens=3200`. It occupies the Constitution's **Tier 4 worker slot** (demand-loaded, evicted after 90s idle). The orchestrator 3B stays warm; the worker is loaded only while a PDF is being converted.

**Rationale**:
- Constitution Principle III: at most one LLM resident. The IR builder is heavy and infrequent — perfect demand-load worker. Chat continues to use the warm 3B orchestrator; while a PDF is converting, chat queries that would have escalated to a 7B worker either reuse cache/Tier-1/Tier-2 or wait — acceptable trade documented in plan.md complexity-tracking.
- 4096 context fits ~3000 token chunks + system prompt + JSON output budget.
- Q4_K_M is the constitution's mandated minimum quantization.

**Alternatives considered**:
- *Qwen2.5-7B*: comparable quality, but Mistral 7B's instruction-following on long structured prompts is better on the brainstorm's sample doc set.
- *Reusing the warm 3B*: 3B produces noticeably worse hierarchical structure on legal docs; quality regressions cancel the value.
- *Cloud LLM*: violates Principle I.

---

## R3 — IR schema versioning and the existing `ConstitutionIr`

**Decision**: Define a new **`DocIr`** root type (the generic flat-nodes schema from brainstorm §3) at `schema_version = 1`. Keep `constitution_ir::ConstitutionIr` as-is for the constitution path; add a **converter `ConstitutionIr → DocIr`** so the constitution document can also flow through the generic indexer once acceptance proves parity. v1 ships both paths in parallel — no behaviour change to the constitution route.

**Rationale**:
- Smallest-viable-diff (CLAUDE.md): refactoring `ConstitutionIr` into a typed view *now* risks regressing the closed Feature 003 acceptance gate. The converter is a one-way bridge we can introduce safely and remove on a later, separate task.
- Brainstorm §3 picked flat `nodes[] + parent_id`; the converter clarifies that legacy nested-tree input maps cleanly to flat.
- Schema version lives on the cache record so a future bump triggers re-extraction without code-side migration.

**Alternatives considered**:
- *Refactor ConstitutionIr into typed view of DocIr immediately*: rejected — out of scope for this feature, would re-open Feature 003 acceptance.
- *Two parallel schemas forever*: rejected — adds permanent duplication; the converter keeps the path open to consolidate later.

---

## R4 — Validator architecture

**Decision**: Validator runs in two passes:
1. **Schema pass** — `jsonschema` crate against the same JSON-schema used by llguidance. Catches required-field / enum / type errors.
2. **Semantic pass** — Rust function `validate_doc_ir(&DocIr) -> Vec<ValidationIssue>` checks parent_id integrity, no cycles, no duplicate ids, monotonic numbering within parent (tolerant of roman/arabic mix), text coverage ≥90%, annotation rules, page-index references.

Each `ValidationIssue` carries: `node_id`, `severity` (error|warn), `code` (machine-readable enum), `message` (human-readable). Errors at any node mark that node `low_confidence`; the document is **not** rejected — partial-success ingest (FR-009, FR-010).

**Rationale**:
- Two-pass keeps the cheap structural check separate from the expensive semantic check; the retry loop only needs to re-run the failing semantic checks on the corrected chunk.
- Same JSON-schema feeds llguidance and `jsonschema` — one schema file, zero drift.

**Alternatives considered**:
- *Single combined pass*: harder to retry surgically; tighter coupling.
- *Validator as LLM judge*: unreliable, expensive, would violate Principle II (tiered-intelligence-cheapest-first).

---

## R5 — Chunking strategy for the LLM converter

**Decision**: Two-stage chunker:
1. **Heading-boundary pre-pass** — reuse `heading_detector::regex_pass` with all profiles' patterns to find candidate splits. This is the *same* code Feature 005 already runs.
2. **Token budget** — accumulate text from one heading to the next; if the running token count exceeds 2400, split at the next paragraph boundary even if no heading is seen. Each chunk carries `parent_context_breadcrumb` (the active Part/Chapter/Section path from previous chunks).

**Rationale**:
- Reusing the regex pass means heading-detection improvements made for Feature 005 automatically benefit this feature.
- 2400 token budget leaves ~800 tokens for system prompt + grammar overhead + JSON output before the 4096 ctx limit.
- Breadcrumb lets the LLM know where it is in the document without re-feeding earlier chunks.

**Alternatives considered**:
- *Whole-document single pass*: doesn't fit 4096 ctx for any real legal document.
- *Page-based chunking*: ignores semantic boundaries, produces ragged structure.

---

## R6 — Cache location and storage primitives

**Decision**:
- Cache directory: `{app_data_dir}/ir_cache/<sha256>.json` (via `tauri::AppHandle::path().app_data_dir()` — same primitive `cloud_llm.rs:94` and `query_trace.rs:148` already use).
- Cache **index** in SQLite: new table `ir_cache_index (sha256 PK, schema_version, doc_type, orphaned_at NULLABLE, nodes_total, nodes_ok, nodes_low_confidence, nodes_user_repaired, created_at, updated_at)`. The JSON file holds payload; the index holds enumeration metadata for sweeps and UI queries.
- Migration: `018_llm_ir_importer.sql` adding `ir_cache_index` + new columns on `sources` (`ir_cache_sha`, `extraction_method`, `quality_score`, `nodes_low_confidence`).
- Orphan sweep: cron-style task at app start + once per 24h, run by `scheduler.rs` (existing scheduler service).

**Rationale**:
- Constitution Principle IV (single SQLite source of truth) — but JSON payloads on disk are not "persistent state in another DB"; they're computed artefacts. Storing the **index** in SQLite preserves the principle while keeping JSON readable for inspection/repair.
- Additive-only migration satisfies the Development Constraint.

**Alternatives considered**:
- *Blob column for JSON inside SQLite*: makes `ir_cache_index` rows balloon; harder for users/devs to read. JSON files on disk match how `constitution_ir.json` already lives.
- *Tauri app local-store*: more indirection, no inspection ergonomics.

---

## R7 — Repair UI rendering: rendered PDF page

**Decision**: Use **pdftoppm** (already required by Feature 004's OCR fallback tier) to rasterize the relevant page at 150 DPI on demand. Cache rasterized PNGs at `{app_data_dir}/page_cache/<sha256>/p<N>.png` with a small LRU sweep (max 200 pages × ~200 KB ≈ 40 MB).

**Rationale**:
- pdftoppm is already a build dependency from Feature 004 — no new external tooling.
- 150 DPI is the standard repair-view density; produces ~1200×1550 PNGs that scale cleanly in the SolidJS panel without re-rasterizing.
- LRU cap keeps growth bounded without per-user knobs.

**Alternatives considered**:
- *PDF.js in a webview*: ships JS dependency, mismatch with the Rust-first architecture, no real win.
- *On-the-fly rasterization on every open*: laggy UX; cache amortizes.

---

## R8 — Repair save → partial re-index

**Decision**: On save, the changed nodes' chunk rows are deleted (via `chunks::delete_by_ids`) and re-inserted by re-running the `structure_parser::persist_tree` path against just those nodes + their immediate descendants. Graph neighbourhood: drop and rebuild edges incident to those nodes; do not touch edges between unaffected nodes. Embedding: re-embed only the changed-node chunks.

**Rationale**:
- Matches FR-017's "only changed nodes re-embedded".
- Reuses existing persistence and embedder code paths — no new low-level code, only a narrower scoping function.

**Alternatives considered**:
- *Full re-index of the document*: simple but slow; violates the spec's incremental-repair promise (SC-005, 10s target).
- *Mark stale + lazy re-embed*: more state to manage; complicates retrieval.

---

## R9 — Provider trait and fallback wiring

**Decision**: Define `trait IrBuilder { async fn build(&self, ParsedDocument) -> Result<DocIr, IrBuildError>; }` with one impl `MistralIrBuilder`. Wire into `indexer::do_index` so the dispatch becomes:

```
if try_load_constitution(path).is_some() { route to constitution path }
else if MistralIrBuilder::available() { route to LLM IR path }
else { route to feature 005 rule-based parser }
```

`available()` checks for the GGUF file at `models/mistral-7b-instruct-v0.3.Q4_K_M.gguf`. Missing model emits a one-time notification (FR-027) via `tauri::AppHandle::emit("importer://llm-unavailable", …)`.

**Rationale**:
- Trait-based dispatch lets us add a cloud impl in v2 without touching the call site (even though v1 forbids cloud).
- The constitution-path early-out preserves Feature 003's behaviour exactly.

**Alternatives considered**:
- *Replace feature 005 entirely*: too risky in one diff; the rule-based parser is a deterministic fallback we want.
- *Async stream the LLM into the indexer*: complicates cancellation; batch-then-validate is simpler and still meets SC targets.

---

## Resolved technical-context unknowns

| Topic | Status |
|---|---|
| Constrained decoding library / format | ✅ R1: llguidance, JSON schema |
| Worker model + tier placement | ✅ R2: Mistral 7B, Tier 4 demand-load |
| IR schema vs existing ConstitutionIr | ✅ R3: new `DocIr`, converter from ConstitutionIr |
| Validator architecture | ✅ R4: schema + semantic, two-pass |
| Chunking | ✅ R5: heading-boundary + 2400-tok budget |
| Cache storage | ✅ R6: disk JSON + SQLite index table |
| PDF page rasterization | ✅ R7: pdftoppm at 150 DPI, LRU page cache |
| Incremental re-index | ✅ R8: scoped node-set re-index |
| Trait + fallback | ✅ R9: `IrBuilder` trait, ordered dispatch |
