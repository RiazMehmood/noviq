# ADR-0009: Query Decomposition Gated to Capacity-Limited Generators

> **Scope**: Feature 008 (query decomposition). Governs *when* the decomposition path runs as a function of the active deployment's generator capacity.

- **Status:** Accepted
- **Date:** 2026-06-08
- **Feature:** 008-query-decomposition
- **Context:** Small fine-tuned generators (the shipped 0.5B; 1.5B) are strong at single-fact lookups but collapse on multi-fact questions — they scramble numbers and self-contradict (e.g. an (a)(b)(c) enumeration answered 1/29/32 instead of 31/32/39). Decomposing a multi-fact question into single-fact sub-questions, answering each via RAG, and merging deterministically fixes this for small models. But decomposition is a **crutch, not a universal good**: a capable generator (3B+ on a good CPU/GPU) answers multi-fact questions natively in a single pass, and forcing it through decomposition would add latency and sub-question retrieval overhead for no accuracy gain. The same binary ships to very different deployments (small model on a low-RAM CPU vs a larger model on capable hardware). The decision is therefore *not* "should we decompose" but "under what runtime signal do we enable it," and it must be dynamic per deployment rather than a compile-time or always-on choice.

## Decision

Decomposition is **enabled dynamically based on the active generator's capacity**, not always-on:

- **Primary signal — model parameter size.** Parse the active GGUF's param size (`discover_local_gguf` / `parse_param_size_b`, already classifying 0.5/1.5/3/7; label e.g. `local-0_5b`). Default threshold: **≤1.5B enables** decomposition (covers the shipped 0.5B and 1.5B); **3B+ disables** it (answer natively, single pass).
- **Fallback signal — hardware tier.** When model size is unknown, fall back to `HardwareTier` (`ram_gb`, `has_discrete_gpu`): low RAM / no discrete GPU ⇒ treated as capacity-limited ⇒ enable.
- **Operator override.** An `ECHO_*`-style env knob can force decomposition on or off, independent of the signals above (testing / unusual deployments).
- Encoded in `decomposition_enabled` (the gate) in `intent_router::decompose`; FR-011/012/013, US4, SC-006/007 of Feature 008.

## Consequences

### Positive

- Same binary adapts: small-model/low-RAM deployments get the accuracy crutch; capable deployments keep native single-pass speed.
- No latency or retrieval overhead imposed on generators that don't need decomposition.
- Honors Hardware-Adaptive Behavior (Principle VII) and Tiered Intelligence (Principle II) — behavior is driven by detected capacity, not hardcoded.
- The override knob keeps the decision testable and lets operators handle edge deployments without a code change.

### Negative

- The ≤1.5B threshold is a heuristic; a future small-but-capable model (or a large-but-weak one) could be mis-gated until the threshold is revisited.
- Two code paths (decomposed vs single-pass) must both be maintained and tested.
- Capacity is inferred (param size / hardware), not measured against actual task accuracy — a coarse proxy.
- Unknown-size models lean entirely on the hardware-tier fallback, which is a weaker signal than param size.

## Alternatives Considered

- **Always-on decomposition (all models)** — Rejected. Adds latency + sub-question retrieval cost and can *reduce* quality for capable models that answer multi-fact natively; wasteful on good hardware.
- **Never decompose (rely on model capacity)** — Rejected. Fails the shipped small-model deployments outright (the multi-fact collapse this feature exists to fix).
- **Compile-time flag per build/deployment** — Rejected. Requires separate builds; the same binary must adapt at runtime to whatever model/hardware it finds.
- **Hardware tier as the primary signal** — Rejected as primary (kept as fallback). Param size is a more direct proxy for the capability that drives the multi-fact failure; a small model on big hardware still confabulates.

## References

- Feature Spec: specs/008-query-decomposition/spec.md (FR-011/012/013, US4, SC-006/007)
- Implementation Plan: specs/008-query-decomposition/plan.md
- Related ADRs: ADR-0008 (Deterministic Citation Integrity) — decomposition produces the sub-questions whose numbers ADR-0008 then resolves deterministically
- Evaluator Evidence: fundamental_rights V3/V4/V5 evals (multi-fact collapse on 0.5B; decomposition mechanically working) — history/prompts/008-query-decomposition/
