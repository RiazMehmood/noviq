# ADR-0008: Deterministic Citation Integrity for Answers and Drafts

> **Scope**: Cross-cutting decision spanning Feature 008 (query decomposition / Q&A answering) and Feature 009 (LGD Summary & Note drafting). Governs how every legal citation, number, and quoted rule reaches the user.

- **Status:** Accepted
- **Date:** 2026-06-08
- **Feature:** 008-query-decomposition, 009-lgd-drafting
- **Context:** The product's primary contract is citation-grade output: an officer must be able to copy a generated answer or draft and forward it for approval with zero manual correction. A single wrong rule/article/section number is a career-level error, not a minor eval miss — the bar is 100%. The V4 evaluation (0.5B fine-tuned generator, fundamental_rights set) proved a generative model cannot meet this bar: with the correct labelled chunk (`Section 31`) present in context, the model still emitted `Article 29`. This holds in principle at every model size — a model in the loop for *producing* a number can lower the error rate but never zero it. Constitution Principle XI (Citation Discipline, NON-NEGOTIABLE) requires every surfaced legal claim to carry a citation rendered in a single canonical hierarchical form. The decision is therefore architectural and must be consistent across both the answering and drafting surfaces rather than re-invented per feature.

## Decision

Citations, numbers, and quoted rule/regulation text are **produced deterministically from retrieved chunk metadata/text — never generated, altered, or inferred by a model**. The model's role is confined to connective prose that is structurally incapable of introducing or changing a number or citation. A unified mechanism is shared across features:

- **Selector:** `intent_router::decompose::resolve_provision_label` — picks the chunk whose label/title matches the requested topic by majority content-token overlap (title-match, *not* retrieval rank, since the correct chunk is frequently not rank 1). The number/label is read from the chunk (e.g. `Section 31`), not from the model.
- **Refuse-or-flag:** when the selector cannot anchor a topic to a chunk above threshold, it returns a sentinel (`NUMBER_UNVERIFIED` in Q&A; a `FlaggedBlock` "unverified — source not available" in drafting). Silence/flag always beats a confident wrong citation. "I'm not certain" is a valid and required output.
- **Citation rendering:** the hierarchical citation is built deterministically from chunk metadata (instrument + article/section/rule + page, plus amendment status where exposed), per Principle XI's single canonical form.
- **Feature 008 (Q&A):** enumeration/multi-article sub-questions get their numbers from the selector; the model writes only the surrounding answer prose.
- **Feature 009 (drafting):** each template **cited slot** is filled via the same selector over the confirmed standing-rules documents; **fact slots** are extractive (verbatim span + attribution) from confirmed case documents; **prose slots** are model-written and contain no numbers/citations.

## Consequences

### Positive

- Numbers/citations become **un-hallucinatable by construction** — validated in V5 (Q15 went from 29/28/29 all-wrong to 31/32/39 all-correct after the selector landed).
- One citation-integrity implementation is reused across Q&A and drafting, so the contract is enforced in a single place and cannot drift between surfaces.
- Honors Constitution XI (Citation Discipline) and the Tiered Intelligence principle (citation resolution is retrieval/Tier 1–2, before any model call).
- Degrades safely: when the generator is unavailable (hardware tier) or a source is missing, cited/extractive content still renders and unanchored slots are flagged rather than faked.
- Makes acceptance testing tractable: correctness is checkable deterministically (does the rendered number/quote appear verbatim in a source chunk?) without an LLM in the assertion path.

### Negative

- Output is more **extractive/templated** and less fluent than free-form model prose; connective narrative is deliberately bounded.
- Correctness now depends on **retrieval + chunk-label quality**; a mislabeled or missing chunk surfaces as a flag (an honest gap) rather than a smooth answer, shifting effort to corpus/label hygiene.
- The title-match selector has a tuning surface (content-token overlap threshold) that must be maintained as corpora grow.
- Genuinely generative phrasings the officer might want must be added as explicit prose slots, never by relaxing the citation path.

## Alternatives Considered

- **Generative model fills citations from context** — Rejected. V4 proved confabulation even with the correct labelled chunk present; cannot reach the 100% bar at any model size.
- **LLM merge/rewrite of multi-fact answers or draft bodies** — Rejected. A model rewrite step re-corrupts already-correct numbers/figures (the original Q15 scrambling failure mode). Merge is deterministic/templated instead.
- **Per-template hardcoded FTS query strings / keyword tables for cited slots** — Rejected. Brittle and edges toward document-specific hardcoding (violates the diagnose-before-code / no-hardcoded-keywords standard). The query is derived per draft from the slot's required-source category + intent.
- **Trust retrieval rank-1 chunk for the number** — Rejected. The correct chunk is frequently not rank 1 (V5 traces: correct `Section 39` at rank 8); title-match-not-rank is required.

## References

- Feature Spec: specs/009-lgd-drafting/spec.md ; specs/008-query-decomposition/spec.md
- Implementation Plan: specs/009-lgd-drafting/plan.md (R1, R2, R7) ; specs/009-lgd-drafting/research.md
- Related ADRs: ADR-0007 (Legal Corpus Hierarchical and Graph Document Model) — provides the chunk metadata/labels this decision reads from
- Evaluator Evidence: V4 analysis (0.5B confabulation proof) and V5 confirmation (Q15 31/32/39 correct) — `~/Desktop/Noviq Documents reports/.../V4/ANALYSIS_0.5B_V4_2026-06-07.md` and V5 traces; PHRs history/prompts/008-query-decomposition/ and 009-lgd-drafting/0003.
