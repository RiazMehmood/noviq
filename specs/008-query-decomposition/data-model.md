# Phase 1 Data Model: Query Decomposition

No database entities (no migration). These are in-memory types in
`src-tauri/src/services/intent_router/decompose.rs`.

## Capability (gate result)

```rust
/// Whether the decomposition path is allowed for the active deployment.
pub(crate) enum Capability {
    /// Capacity-limited small model — decomposition allowed.
    SmallModel,
    /// Capable model / GPU tier — decomposition disabled (answer natively).
    Capable,
}
```

- Derived by `decomposition_enabled(model_size: Option<f32>, tier: &HardwareTier) -> bool`.
- Inputs: parsed GGUF param size (`Option<f32>`), `HardwareTier`.
- Override: `ECHO_DECOMPOSE` env (`1/on` → force `SmallModel`; `0/off` → force `Capable`).
- Rule: `size.map(|s| s <= 1.5)` else `!tier.has_discrete_gpu && tier.ram_gb < 16.0`.

## SubQuestion

```rust
/// One single-fact question derived from a multi-fact parent.
pub(crate) struct SubQuestion {
    /// Display label for the merged block: "(a) the Islamic way of life"
    /// or "Article 23".
    pub label: String,
    /// The self-contained question text sent through the RAG seam.
    pub text: String,
}
```

**Validation**: a decomposition is valid only when it yields `>= 2` `SubQuestion`s;
otherwise the detector returns `None` (single-pass fallback). `label` and `text` are
non-empty after trim. A sane upper cap (`MAX_SUBQUESTIONS = 6`) bounds latency; parts
beyond the cap are not silently dropped — detection returns `None` (fall back) if a
question exceeds the cap rather than answering a truncated subset.

## SubAnswer

```rust
/// The result of answering one SubQuestion through the seam.
pub(crate) struct SubAnswer {
    pub label: String,
    /// Answer text from route_scoped_with_override (may be the grounded
    /// "not found in the retrieved passages" fallback).
    pub answer: String,
    /// Citations for this sub-answer, in this block's local order.
    pub citations: Vec<Citation>,   // reuses query_router::Citation
}
```

## MergedAnswer (output of merge)

```rust
/// Deterministic assembly fed back into the ChatResponse.
pub(crate) struct MergedAnswer {
    /// Full template-assembled text with globally-renumbered [n] markers.
    pub answer: String,
    /// Global citation list; index i (1-based in text) ↔ citations[i-1].
    pub citations: Vec<Citation>,
}
```

## State / flow

```
dispatch(query)
  → slots = extract(query)
  → enabled = decomposition_enabled(model_size, tier)         // Capability
  → if enabled:
        plan = detect_and_split(query, &slots) -> Option<Vec<SubQuestion>>
        if Some(subs) && subs.len() >= 2:
            answers = []                                       // sequential
            for sq in subs:
                (qresp, cancelled) = route_scoped_with_override(sq.text, ...)
                if cancelled: return cancelled ChatResponse
                answers.push(SubAnswer{ sq.label, qresp.answer, qresp.citations })
            merged = merge_subanswers(&answers)                // MergedAnswer
            return ChatResponse from merged
  → // else: existing single-pass route_scoped_with_override(query.text, ...)
```

## Relationships

- `SubQuestion` → (one RAG pass) → `SubAnswer` → (merge) → `MergedAnswer`.
- `MergedAnswer` maps onto the existing `ChatResponse`/`ChatBubble` (answer + citations),
  so no wire-format change for the frontend.
- `Citation` is the existing `query_router::Citation` throughout merge; the merged list is
  mapped to `intent_router::Citation` via `qr_citation_to_chat` **after** merge, in
  `dispatch`. Merge renumbers/synthesizes `[n]` markers; no shape change. (`parse_param_size_b`
  is private to query_router and is not used — the gate reads size from
  `discover_local_gguf`'s label.)
</content>
