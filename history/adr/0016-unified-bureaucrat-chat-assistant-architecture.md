# ADR-0016: Unified Bureaucrat Chat Assistant Architecture

> **Scope**: Document decision clusters, not individual technology choices. Group related decisions that work together.

- **Status:** Proposed
- **Date:** 2026-06-21
- **Feature:** 015-bureaucrat-chat
- **Context:** Feature 015 merges the separate chat pane and the drafting skeleton/template system (Features 009/010) into ONE intent-routed conversational surface with a permanent professional-bureaucrat persona. The single path serves every document-intelligence purpose: procedural Q&A/guidance, draft Summary/Note, summarize a whole law book, compare 2–3 acts/rules, conflict detection, amendment-chain view, and action-item extraction. Constraints that force these decisions: (1) **citation-grade 100% accuracy is the primary product contract** — an officer acts on the output, so a single fabricated section number is a contract-breaking failure; (2) the model is **hot-swappable** — whatever GGUF sits in the models folder is used (small, large, or reasoning models), so correctness cannot depend on model capability (V4 eval proved even a fine-tuned model confabulates article numbers with the correct chunk in front of it); (3) today's context budgeting is **scattered and unsafe** — per-call-site hardcoded `n_ctx` (3584 chat / 6144 draft / 2048 analysis) and independently-set prompt/context budgets that can already overflow the prompt cap and trigger silent mid-context truncation; (4) deterministic ingestion (F013) already produces structured amendment provenance; (5) deployment surface is the IBA GPU thin-client server (Spec 014); offline-first.

## Decision

Adopt a single integrated architecture for the assistant layer, composed of four coupled decisions introduced together for F015:

- **D1 — Unified intent-routed surface.** One conversational surface; an intent classifier dispatches each message to a capability (guidance, draft, summarize-doc, compare, conflict, amendment-chain, action-items) reusing the existing `intent_router`. The user never picks a mode (auto intent-detection), under a single always-on bureaucrat persona. This **retires** the slot/template schema, docset curation, gap detection, slot-render, and finalize-lock machinery (Features 009/010); only the verbatim citation gate and export carry forward.
- **D2 — Citation gate as model-independent safety floor.** Every capability enforces the `[CITE:n]` verbatim-substring gate + refuse-or-flag (now incl. offer-to-index). Correctness is therefore independent of model capability — a weak model is *safe* (ungrounded claims are flagged, never asserted), and quality scales with the model. This is the precondition that makes hot-swapping any GGUF acceptable.
- **D3 — Dynamic `n_ctx` + single priority context allocator.** Replace all hardcoded per-call-site `n_ctx` and scattered budgets with: (a) `n_ctx = min(model's GGUF trained context, RAM/VRAM-afforded)`, `ECHO_N_CTX` override; (b) one allocator computed in **real tokens** each turn that guarantees the assembled prompt ≤ window **before** generation, so the `llm.rs` mid-context truncation never fires. Priority: generation reserve → persona → pinned case facts → current question → retrieved law → recent turns verbatim → older turns as running summary. **Atomic provisions** (a law is included whole or omitted, never sliced). **Law outranks history** (history is summarized to keep law complete, never the reverse). **Multi-pass map-reduce** for whole-document/cross-document scope and overflow. Eviction = **summarize old turns** (reuse `conversation_summary`, migration 023) + a proactive "context-filling → start a fresh chat (carry pinned facts forward)" notice.
- **D4 — Deterministic-where-truth-is-structural.** The **amendment chain** (what amended what, ordering oldest→recent, dates, instruments) is built deterministically from F013 structured provenance (`footnote_refs` / `amended_by` edges); the LLM only narrates it. **Conflict detection** surfaces cited *candidate* contradictions with the provision from *both* documents, framed for human judgment — never an asserted fact.

## Consequences

### Positive

- **One contract, everywhere.** The citation gate + context guarantees apply uniformly to every capability, so adding a capability cannot quietly weaken grounding.
- **Model is a swappable component, safely.** Dynamic `n_ctx` makes a larger-window model actually deliver its window; the citation gate keeps even a weak/local model honest. Supports the GPU-server upgrade path (Spec 014) without code changes.
- **"Nothing truncates" becomes provable.** Real-token budgeting with sum ≤ window before generation + atomic provisions means the model never reasons from a half-law and the window never overflows — directly serving the citation-grade contract.
- **Whole-document/cross-document capabilities become possible** precisely because map-reduce is first-class, not a bolt-on.
- **Lower-risk facts stay deterministic.** Amendment order/dates and conflict claims do not depend on model generation, removing the highest-confabulation-risk surfaces.
- **Smaller system.** Retiring the slot/template/docset/gap/finalize subsystem reduces surface area and is simpler to serve over HTTP to the thin client.

### Negative

- **Latency cost of map-reduce + summarization** on whole-book/compare/conflict tasks — many LLM passes; acceptable on the GPU server, slow on CPU. Mitigated by the deployment surface (Spec 014) and the "start a fresh chat" notice.
- **Intent misrouting risk.** Auto-detection can dispatch the wrong capability; mitigated by document-target confirmation (FR-005e) and the citation gate (a misroute still cannot fabricate).
- **Summarization is lossy.** Mitigated by pinned case facts (kept verbatim) and the proactive new-chat notice, but a long single session still relies on a faithful running summary.
- **Conflict detection is intentionally non-authoritative** (candidates only) — it will not "declare" conflicts, which some users may expect; this is a deliberate trust tradeoff.
- **Dynamic `n_ctx` adds load-time logic** (read GGUF metadata, compute KV-cache RAM affordability) and a new failure mode if metadata is missing — falls back to a conservative default.

## Alternatives Considered

- **D1 alt — explicit per-capability modes / panes, or keep separate chat + drafting.** Rejected: contradicts the "single path for all purposes" goal and keeps the retired template subsystem; auto intent-detection chosen over a mode toggle for UX, with the citation gate covering misroute risk.
- **D2 alt — trust a fine-tuned model's grounding (no gate).** Rejected: V4 eval proved confabulation of article numbers even with the correct chunk supplied; incompatible with citation-grade and with hot-swapping arbitrary models.
- **D3 alts — (a) keep fixed `n_ctx`:** wastes a big model's window and risks overflow on long drafts; **(b) drop oldest turns verbatim:** abrupt information cliff (forgets the user's stated grade); **(c) shrink retrieved law to fit:** breaks citation-grade (less law = weaker grounding). All rejected in favor of dynamic window + summarize-with-pinned-facts + law-outranks-history.
- **D4 alt — let the LLM synthesize the amendment order/dates and declare conflicts.** Rejected: highest confabulation-risk surface; structural truth must come from F013 provenance, and conflicts must be human-judged from cited candidates.

## References

- Feature Spec: [specs/015-bureaucrat-chat/spec.md](../../specs/015-bureaucrat-chat/spec.md)
- Implementation Plan: pending (`/sp.plan`) — ADR authored at spec stage per stakeholder direction
- Related ADRs:
  - ADR-0008 (Deterministic citation integrity for answers and drafts) — D2 generalizes this into a model-independent floor across all capabilities.
  - ADR-0012 (Model-assisted template ingestion) and ADR-0013 (One-ask summary composition) — the template/slot + one-ask drafting machinery they describe is **superseded/retired** by D1.
  - ADR-0007 (Legal corpus hierarchical & graph document model) — D4's amendment chain consumes its graph/provenance.
  - ADR-0015 (PostgreSQL as single source of truth) — persistence substrate for chat turns, conversation summary, pinned facts.
- Evaluator Evidence: PHR `history/prompts/015-bureaucrat-chat/0001-…` (spec); MEMORY `project_noviq_citation_grade_accuracy_contract` (V4 confabulation evidence behind D2).
