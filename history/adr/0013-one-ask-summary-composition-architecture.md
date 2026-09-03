# ADR-0013: One-Ask Summary Composition Architecture

- **Status:** Accepted
- **Date:** 2026-06-11
- **Feature:** 010-one-ask-drafting
- **Context:** Live officer testing (2026-06-11) killed the Feature 009 slot-template approach: a real gold instrument's slot schema, applied to a different subject (ADP funding), produced 7/8 unfillable "needs a source" cited slots and gibberish prose — per-case slot templates do not generalize, and the template-upload/intent-Q&A/doc-curation ceremony put the research burden back on the officer. Officers expect one typed sentence → finished, copy-paste-ready summary middle content. The non-negotiable constraint is the citation-grade contract (constitution §XI, ADR-0008): zero fabricated rule numbers, quotes, or figures. Additional hard facts: llguidance grammar-constrained sampling hard-crashes llama-cpp-2 (reproduced twice: F006 IR builder, Option-C spike); 3B/0.5B local models paraphrase and confabulate numbers (V4/V5 evals); the drafting retrieval query brute-force-scanned all vec0 embeddings (281 s observed on the 65 GB production DB).

## Decision

An integrated composition pipeline (components designed together, change together):

- **Structure source: built-in skeleton, not templates.** A hardcoded 5-section Sindh summary skeleton (Background/Purpose → Legal & Procedural Position → Financial Aspect → Proposal/Recommendation → Prayer for Approval) with per-section narrative guidance and retrieval-role hints. Per-case slot templates are abolished from the officer path; ingested gold instruments are retained only as future style examples.
- **Intent: deterministic regex parse** (figures verbatim, action keyword, raw sentence as retrieval seed). No model call (constitution §II).
- **Research: per-section vec0 KNN retrieval → provision cards.** sqlite-vec `MATCH ?/k=?` KNN form (two-step: KNN over-fetch, then hydrate/filter to indexed sources) replaces the brute-force scalar scan. Cards = verbatim chunk excerpt + provision label resolved deterministically via `decompose::resolve_provision_label` + source ref. Cards are the writer's only citation vocabulary.
- **Writing: two-lane marker composition.** Mistral-7B-Instruct-v0.3 Q4_K_M (demand-loaded, single slot, free-text — no llguidance) writes connective narrative per section, emitting `[CITE:n]`/`[FIG:n]` markers only. The model proposes *placement*; deterministic code owns *content*.
- **Assembly + gate: deterministic splice and verify.** Cards/figures spliced at markers; every quote re-checked as a verbatim (whitespace/hyphen-tolerant) substring of its source chunk; a post-scan over remaining prose flags any stray provision-number or monetary figure the model emitted. Unverifiable content renders as a visually distinct Flag run — never as a citation, never silently dropped.
- **Interaction: per-section revision + staged progress.** Free-text revision turns route to the owning section and regenerate only it; pipeline stages stream as events with cancel support (no silent wait > 10 s).

## Consequences

### Positive

- Officer experience collapses to one sentence in → copy-paste middle content out; the weeks-long research burden moves to the machine.
- The 100%-accuracy contract is enforced structurally: a fabricated number can only ever surface as a flag, regardless of model behaviour — the guarantee lives in the gate, not the prompt (extends ADR-0008 to full-document generation).
- Generalizes to any subject without per-case authoring (fixes the structural failure that killed 009 templates).
- Per-section generation fits the 7B's n_ctx budget on CPU, isolates failures, and makes targeted revision cheap (~1/5 of a full draft).
- KNN retrieval makes the research stage corpus-scalable (≤ 15 s target vs 281 s observed).

### Negative

- The skeleton is opinionated: instruments that deviate from the 5-section Sindh summary shape (e.g. Notes, multi-case summaries) need skeleton variants — acceptable, skeletons are cheap consts; risk is officers in other departments expecting different flows.
- First drafts take minutes on CPU (5+ sequential 7B calls); accepted explicitly by the user, mitigated by progress events, but it bounds iteration speed.
- Marker protocol relies on the 7B following instructions for *placement*; a model that emits no markers produces citation-poor (heavily flagged) drafts — quality depends on prompt + model, though accuracy never does.
- 009's slot/template backend becomes dead weight (kept this feature for smallest-diff; needs a later cleanup pass).
- L2-vs-cosine ranking equivalence of the KNN form assumes normalised embeddings; guarded by a parity test, but a future embedder change could silently violate it.

## Alternatives Considered

- **Per-case slot templates (009 status quo)**: rejected — empirically failed to generalize on first off-template subject; ingestion burden contradicts the product promise.
- **Single mega-prompt generation**: rejected — exceeds n_ctx 6144 with 60 chunks + draft, no targeted revision, one failure redoes everything.
- **Grammar-constrained output (llguidance)**: dead — hard C++ abort, reproduced twice.
- **Smaller local writers (3B/0.5B)**: rejected — proven unfaithful (paraphrase, confabulated article numbers even with the correct chunk in context).
- **Cloud writer (Groq/Cerebras)**: rejected — violates constitution §I and the user's explicit offline decision for this product.
- **Trust-the-prompt accuracy (no post-gate)**: rejected — V4 proved models emit numbers despite instructions; deterministic verification is the only acceptable guarantee.

## References

- Feature Spec: specs/010-one-ask-drafting/spec.md
- Implementation Plan: specs/010-one-ask-drafting/plan.md (+ research.md R1–R9, data-model.md, contracts/one-ask-commands.md)
- Related ADRs: ADR-0008 (deterministic citation integrity — extended), ADR-0012 (template ingestion gate — officer-facing role superseded; gate philosophy carried forward), ADR-0001/0010 (single model slot, free-text-over-grammar precedent)
- Evaluator Evidence: history/prompts/010-one-ask-drafting/0002-plan-one-ask-drafting-pipeline.plan.prompt.md; Option-C spike results at ~/Desktop/Noviq Documents reports/option_c_ingestion_spike_20260610/
