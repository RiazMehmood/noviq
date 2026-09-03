# ADR-0012: Model-Assisted Template Ingestion with Deterministic Verbatim-Verify Gate

> **Scope**: Feature 009 (LGD drafting) template ingestion. Clusters the decisions that define how an uploaded format/instrument becomes a slot schema: model choice, generation mode, the deterministic verification layer, and the failure policy.

- **Status:** Accepted
- **Date:** 2026-06-10
- **Feature:** 009-lgd-drafting
- **Context:** Template ingestion (`services/drafting/ingest.rs::propose_schema`) turns an uploaded format or real past instrument into the slot schema (boilerplate / cited / fact / prose) that every future draft from that template is built on. The shipped deterministic heuristic (heading-shape + line-length segmentation, heading-token categories) was proven **algorithmically inadequate on real instruments**: the 2026-06-10 gold-ingest diagnostic ran a flawless real Sindh instrument (GOLD_SPSC_age_relaxation_2025) through it and got 13 garbage slots — letterhead, dateline, and C.C. lines became false cited slots while the actual citations (Rule-6(c) SPSC Regulations-2023, Rule-23 SCUG Rules 2020, Law-Dept opinion chain) landed in no slot. Real instruments are numbered running paragraphs, not heading-structured documents. A wrong schema is silently poisonous: it propagates into every draft, producing un-satisfiable gaps and false citation demands (the officer's "what's the purpose of AI?" failure). Constraints: citation-grade contract (Constitution XI — no model may author a citation/number), single bounded model slot (III), tiered intelligence (II), offline-only (I), hardware-adaptive degradation (VII).

## Decision

Adopt **Option C — a two-stage ingestion pipeline**, validated by the 2026-06-10 spike:

- **Model proposes (Tier-4, one-time, offline):** **Mistral-7B-Instruct-v0.3 Q4_K_M** — already on disk at `models/ir_builder/` from Feature 006 — is demand-loaded exclusively for ingestion and prompted to segment the extracted template text into ordered regions `{kind, label, text, provision?}` with copy-verbatim instructions.
- **Free-text JSON generation, NOT llguidance grammar:** output is parsed with a tolerant JSON parser (F006 lenient-parse pattern). The llguidance grammar path **hard-aborts** (C++ crash after `init_tokenizer`, same crash class F006 hit) — it is dead for this stack.
- **Deterministic verbatim-verify gate (the authority):** every model-proposed slot must pass: (a) `text` is a verbatim substring of the source (whitespace-normalized); (b) any `provision` appears verbatim and re-resolves via `decompose::resolve_provision_label` extended with hyphen tolerance (`Rule-6(c)`, `Regulations-2023`); (c) the cited slot's required-source category derives from the **matched provision**, not heading tokens; (d) structural boilerplate (letterhead/dateline/addressee/C.C./signature) is never classified cited. The model proposes structure; only verbatim-verified bytes survive — the citation-grade contract (ADR-0008) is preserved by construction.
- **Flag-for-review on gate rejection:** a rejected slot is surfaced in the officer-confirm step as low-confidence with the rejection reason — never silently dropped or accepted (FR-010a).
- **Refuse, don't degrade (FR-010b):** if the model is missing or output is unparseable after bounded retries, ingestion fails with an actionable error naming the model and produces **no schema**. The heuristic fallback is removed — a degraded schema is worse than none.
- **Hot-path isolation:** ingestion is a one-time admin operation (~18 min CPU acceptable); the per-draft path remains model-free for citations. Persistence adds `template_slot.provision_ref` (additive migration 024).

## Consequences

### Positive

- **Defect 1 fixed at the root**: on the real gold, the 7B free-text run produced clean slots, verbatim provisions (full Law-Dept opinion chain incl. notification numbers), and correctly tagged boilerplate — the heuristic's three failure modes (paragraph blindness, heading-only citation detection, boilerplate-as-cited) all eliminated.
- **Citation-grade contract intact**: the gate makes the model structurally powerless over content — every surviving byte is verbatim-verified, every number re-resolved deterministically (Constitution XI, ADR-0008).
- **Zero new assets**: the GGUF is already on disk from F006; the slot occupancy pattern (demand-load, exclusive, evict) is the established Tier-4 worker discipline (Constitution III).
- **Honest failure surface**: gate rejections become officer decisions with reasons; missing model becomes an actionable refusal — no silent garbage (Constitution VII as *visible* degradation).

### Negative

- **~18 min CPU per ingestion** — accepted because it is one-time and admin-side, but it monopolizes the single model slot for the duration (no chat escalation meanwhile).
- **Heuristic fallback removed**: on RAM tiers that cannot host the 7B, template ingestion is simply unavailable (refusal with explanation). Officers there must ingest on capable hardware.
- **Free-text JSON means parse variance**: the tolerant parser + bounded retries are ongoing surface area (mitigated by the gate — a parse that survives but lies is still rejected byte-wise).
- **Diverges from ADR-0010's grammar premise**: 0010 chose llguidance for structural validity; this ADR documents that the grammar path crashes outright with this model/stack, so validity is enforced *after* generation (gate) instead of *during* (grammar). ADR-0010's own pipeline already fell back to free-text in practice; if F006 IR generation is revisited, this evidence applies there too.

## Alternatives Considered

- **Keep the deterministic heuristic (shipped)** — rejected: 13 garbage slots on a flawless real instrument; failure is algorithmic (paragraph-shaped instruments), not bad input.
- **`noviq_qwen3b` fine-tune (4.5 min, 4× faster)** — rejected: unfaithful — paraphrased source text (rewrote C.C. names), dropped citation anchors (`provision: null`), lumped regions. The gate can reject paraphrase but **cannot recover omissions**; a verified-but-incomplete schema reproduces the citations-swallowed defect.
- **Mistral-7B + llguidance grammar** — rejected: hard C++ abort after `init_tokenizer`; crashes the process. Grammar-constrained sampling is unusable on this stack.
- **Qwen 0.5B** — not run; skipped by user decision 2026-06-10 (Mistral-7B locked on spike evidence).
- **Hybrid retry ladder (3B first, escalate to 7B on gate failure)** — deferred, not rejected: legitimate latency optimization but adds a second code path to a one-time operation; revisit only if ingestion frequency makes 18 min painful.
- **Cloud LLM** — rejected: violates Privacy-First/Offline-Always (Constitution I).

## References

- Feature Spec: specs/009-lgd-drafting/spec.md (FR-010/010a/010b, Clarifications 2026-06-10)
- Implementation Plan: specs/009-lgd-drafting/plan.md (Amendment 2026-06-10, Phase D); research.md R5 (superseded) → R5a
- Related ADRs: ADR-0008 (Deterministic Citation Integrity — the contract this gate preserves); ADR-0010 (LLM IR Pipeline — grammar premise diverged from, see Negative consequences); ADR-0001 (Model Hosting Isolation — the slot discipline reused)
- Evaluator Evidence: Option-C spike 2026-06-10, raw model outputs + analysis at `~/Desktop/Noviq Documents reports/option_c_ingestion_spike_20260610/` (mistral7b / noviq3b / grammar-crash runs on GOLD_SPSC_age_relaxation_2025); PHRs 0020–0021 in history/prompts/009-lgd-drafting/
