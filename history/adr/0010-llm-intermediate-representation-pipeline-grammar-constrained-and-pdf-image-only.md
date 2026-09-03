# ADR-0010: LLM Intermediate-Representation Pipeline — Grammar-Constrained and PDF/Image-Only

> **Scope**: Feature 006 (LLM PDF → hierarchical JSON importer). Clusters the decisions that define how, and for which inputs, a local LLM produces the hierarchical IR consumed by `graph_builder`/`indexer`.

- **Status:** Accepted
- **Date:** 2026-06-08
- **Feature:** 006-llm-pdf-importer
- **Context:** Structured documents (the Constitution, Acts, Rules) ingest cleanly via the rule-based structured importer (Feature 005), but unstructured/scanned PDFs and images have no reliable structure to parse. To make those searchable at the same hierarchical fidelity, a local 7B model must emit the *same* IR JSON the constitution path already consumes. Two risks dominate: (1) free-text LLM output is frequently invalid JSON or wrong-shaped, which would corrupt the graph; and (2) running a 7B model over documents that are *already* structured (existing JSON sidecars, Markdown, Txt, Code) wastes the single bounded model slot and time for zero benefit (the "stuck at 5% running 7B on a text PDF" failure). Constitution constraints bind hard here: offline-only (I), single resident model with a bounded RAM budget (III), hardware-adaptive degradation (VII), SQLite as single source of truth (IV).
- 
## Decision

Adopt a **grammar-constrained, scope-limited LLM IR pipeline** as an integrated cluster:

- **Grammar-constrained generation.** `trait IrBuilder` with `MistralIrBuilder` (Mistral 7B Q4_K_M) generates IR using llama-cpp-2's **llguidance constrained sampler against a JSON-schema grammar**, so output is structurally valid by construction. A two-stage validator (schema-then-semantic) runs after generation; failures trigger bounded per-chunk retries (temp escalation), and exhaustion emits a single `LowConfidence` sentinel node rather than failing the document.
- **Input scope limited to PDF/scanned/image only.** The LLM IR path runs *only* for PDFs, scanned documents, and images. Already-structured inputs (JSON sidecars like `constitution_ir.json`, Markdown, Txt, Code) short-circuit to their existing structured paths *before* LLM dispatch; the indexer's format filter is narrowed accordingly (`Pdf|Image`).
- **Graceful fallback.** When the GGUF is absent or hardware tier < 3 (or load OOMs), `MistralIrBuilder::available()` returns false and ingestion falls back to Feature 005's rule-based parser with a one-time user notification.
- **Offline + bounded slot.** No cloud LLM in v1 (Principle I). Mistral 7B occupies the Tier-4 worker slot demand-loaded, released between documents, evicted after 90 s idle (Principle III).
- **Cache & partial success.** IR cached sha256-keyed on disk (`{app_data_dir}/ir_cache/<sha>.json`) with a SQLite index (migration 018, additive) and a 7-day orphan sweep; confident nodes are searchable immediately, low-confidence nodes go to a split-screen repair UI, and user repairs are sticky across re-runs.

## Consequences

### Positive

- IR is **structurally valid by construction** — the grammar removes the entire class of malformed-JSON failures that would corrupt the graph.
- The 7B model runs **only where it adds value**; structured inputs stay fast and cheap (no model slot contention), directly fixing the "7B on a text PDF" stall.
- Downstream is unchanged: the same IR feeds `graph_builder`/`indexer`, so retrieval doesn't care whether IR came from the rule-based or LLM path.
- Honors Constitution I/III/IV/VII: offline, single bounded slot released between docs, SQLite SSOT, hardware-adaptive fallback with no silent failure.
- Partial-success + repair UI + sticky repairs make a long, imperfect conversion productive rather than all-or-nothing.

### Negative

- Constrained sampling is **slower** than free generation, and 7B CPU conversion is minutes-per-document (acceptable: user-initiated, progress-visible).
- During a conversion the importer **monopolizes the worker slot** — chat cannot escalate to Tier 4 meanwhile (mitigated: Tier 3 chat still works; slot released between docs). Logged in the plan's Complexity Tracking.
- Maintaining the JSON-schema grammar + two-stage validator + tolerant parser is ongoing surface area.
- The PDF/image scope rule means a *structured-but-misdetected* document could skip the LLM path; relies on correct format/profile detection upstream.

## Alternatives Considered

- **Free-text LLM generation + lenient parsing** — Rejected as the primary path. Early rounds showed frequent invalid JSON / wrong shapes; even a 6-shape tolerant parser couldn't guarantee validity. (Kept only as a degraded fallback when llguidance is unavailable.)
- **Run the LLM IR path for all document types** — Rejected. Wastes the bounded model slot and time on already-structured inputs; caused the production stall. Scope-limiting to PDF/image is the fix.
- **Cloud LLM for higher-quality IR** — Rejected for v1. Violates Privacy-First/Offline-Always (Principle I).
- **No LLM; rule-based parsing only** — Rejected for unstructured/scanned PDFs: there is no reliable structure for rules to parse, so hierarchical fidelity would be lost. (Remains the fallback when the model is unavailable.)

## References

- Feature Spec: specs/006-llm-pdf-importer/spec.md ; IR schema: specs/006-llm-pdf-importer/ir_json_schema.md
- Implementation Plan: specs/006-llm-pdf-importer/plan.md (Constitution Check III/VII justifications; Complexity Tracking)
- Related ADRs: ADR-0007 (Legal Corpus Hierarchical and Graph Document Model — defines the IR/graph this pipeline targets); ADR-0001 (LLM Model Hosting Isolation — the model slot this respects)
- Evaluator Evidence: Feature 006 acceptance (`llm_ir_importer_acceptance`) + the structured-importer-primary routing fix (see ADR-0011) — history/prompts/006-llm-pdf-importer/
