# ADR-0011: Indexing Routing — Structured Importer Primary, LLM IR Fallback

> **Scope**: Feature 005 (generic structured importer) and the indexer dispatch it shares with Feature 006. Governs the order in which ingestion paths are attempted for a document.

- **Status:** Accepted
- **Date:** 2026-06-08
- **Feature:** 005-structured-importer (with 006-llm-pdf-importer)
- **Context:** The app has two ways to turn a document into hierarchical IR: the **direct structured importer** (Feature 005 — fast, deterministic, rule/heading-based; no model) and the **LLM IR builder** (Feature 006 — Mistral 7B, slow, for genuinely unstructured input). An early dispatch ordering ran the LLM IR path first/primary, which meant text-based and already-structured PDFs were handed to a 7B model on CPU — producing the "stuck at 5%" stall where a document that the rule-based importer could parse in seconds instead occupied the bounded model slot for minutes. The two paths are independent and could diverge, so the *routing order* between them is itself an architectural decision: which path is the default, and what condition escalates to the other.
- 
## Decision

The **direct structured importer is the primary ingestion path; the LLM IR builder is a secondary fallback**, invoked only when structure is genuinely insufficient:

- On dispatch, already-structured inputs short-circuit first: an existing JSON IR sidecar (e.g. `try_load_constitution`) is used before anything else; Markdown/Txt/Code use their structured paths directly.
- For the remaining documents, the **rule-based structured importer (005) runs first**. It uses heading/structure detection to build IR deterministically.
- The **LLM IR builder (006) is reached only for PDF/scanned/image inputs that lack usable structure** for the rule-based path — consistent with ADR-0010's PDF/image-only scope — and only when the model is available (else the rule-based result stands).
- The indexer's format filter and dispatch reflect this ordering (the routing fix committed on branch 007 as `a516d93f`); the active GGUF lives in `models/ir_builder/` as a normal `.gguf` (the earlier `.disabled` workaround is obsolete).

## Consequences

### Positive

- Eliminates the "7B on a text PDF" stall — fast deterministic ingestion is the default; the expensive model is the exception.
- Keeps the single bounded model slot free for documents that actually need it (and for chat escalation) most of the time.
- Deterministic-first means most ingestion is reproducible and model-independent, which is easier to test and debug.
- Cleanly layered with ADR-0010: scope (PDF/image-only) and order (structured-first) together ensure the LLM runs only where it adds value.

### Negative

- Correct routing depends on **format/profile detection quality**: a structured document misdetected as unstructured could needlessly invoke the LLM, and vice-versa a poorly-structured PDF the rule parser "succeeds" on may yield thinner IR than the LLM would.
- Two ingestion paths plus the escalation condition are more moving parts than a single path.
- The escalation heuristic ("structure insufficient") needs maintenance as new document shapes appear.

## Alternatives Considered

- **LLM IR builder primary (original ordering)** — Rejected. Ran 7B over text/structured PDFs, monopolized the model slot, and stalled ingestion; the defect this ADR corrects.
- **LLM IR for everything (no rule-based path)** — Rejected. Slow and slot-bound for documents that don't need a model; also a privacy/latency regression.
- **Rule-based only (no LLM path)** — Rejected. Loses hierarchical fidelity on genuinely unstructured/scanned PDFs (the reason Feature 006 exists).
- **User picks the path per document** — Rejected. Pushes an implementation detail onto the officer; automatic structured-first with LLM fallback is the better default (overridable by detection, not by manual choice).

## References

- Feature Spec: specs/006-llm-pdf-importer/spec.md (Feature 005 has no separate specs/ dir; routing lives in the shared indexer dispatch)
- Implementation Plan: specs/006-llm-pdf-importer/plan.md
- Related ADRs: ADR-0010 (LLM IR Pipeline — Grammar-Constrained and PDF/Image-Only; defines the fallback path and its scope); ADR-0007 (hierarchical/graph IR target)
- Evaluator Evidence: routing fix commit `a516d93f` on branch 007; MEMORY pointer project_noviq_indexer_direct_primary_2026_06_06
