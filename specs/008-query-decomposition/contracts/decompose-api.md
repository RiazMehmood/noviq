# Internal Module Contract: `intent_router::decompose`

No HTTP/GraphQL surface — this feature is internal to the Rust backend. The "contract"
is the public(crate) function set of `decompose.rs` and the one orchestration hook in
`dispatch`. Each function is independently unit-testable (pure, except the orchestrator).

## `decomposition_enabled`

```rust
pub(crate) fn decomposition_enabled(model_size: Option<f32>, tier: &HardwareTier) -> bool
```

- **Purpose**: the capability gate (FR-011/012/013).
- **Behaviour**: `ECHO_DECOMPOSE=1|on` → `true`; `=0|off` → `false`. Else:
  `Some(s) if s <= 1.5` → `true`; `Some(_)` → `false`;
  `None` → `!tier.has_discrete_gpu && tier.ram_gb < 16.0`.
- **Pure** (reads env once). Unit-testable with synthetic `(size, tier)` pairs.

## `detect_and_split`

```rust
pub(crate) fn detect_and_split(query: &str, slots: &Slots) -> Option<Vec<SubQuestion>>
```

- **Purpose**: rule-based detection + split (FR-001/002/006).
- **Returns** `Some(subs)` with `2..=MAX_SUBQUESTIONS` entries when the query matches the
  enumeration shape OR the multi-article shape; `None` otherwise (→ single-pass).
- **Order**: enumeration detector first, then multi-article. Over-cap → `None`.
- **Pure**. Unit-testable on query strings + pre-built `Slots`.

## `merge_subanswers`

```rust
pub(crate) fn merge_subanswers(answers: &[SubAnswer]) -> MergedAnswer
```

- **Purpose**: deterministic template merge + citation remap (FR-004/005/008/009).
- **Behaviour**: one labelled block per answer in order; global sequential citation
  numbering; each block's trailing `[n]` is **synthesized** from that block's resolved
  citation (the small model usually emits none); any pre-existing local `[k]` markers are
  rewritten to the global indices; "not found" sub-answers rendered verbatim with no
  fabricated citation.
- **Citation type**: operates on `query_router::Citation`. The `MergedAnswer.citations`
  list is mapped to `intent_router::Citation` via the existing `qr_citation_to_chat`
  **after** merge, in `dispatch`, when building the `ChatResponse`.
- **Pure**. Unit-testable with synthetic `SubAnswer`s (incl. citation collisions, a
  marker-less block, and a not-found block).

## Orchestration hook (in `intent_router::dispatch`)

- After `slots::extract`, compute `model_size` as
  `query_router::discover_local_gguf().and_then(|(_, label)| label.parse::<f32>().ok())`
  (the label IS the parsed size; `parse_param_size_b` is private and not used) and call
  `decomposition_enabled(...)`.
- If enabled and `detect_and_split` returns `Some(subs)`: loop **sequentially** over
  `subs`, calling `route_scoped_with_override(sq.text, …)` with the shared cancel token;
  on `cancelled` return the standard cancelled `ChatResponse`; collect `SubAnswer`s; call
  `merge_subanswers`; build `ChatResponse` from the `MergedAnswer`.
- Else: unchanged single `route_scoped_with_override(query.text, …)` call.

## Invariants

- ≤ 1 model resident (sequential loop only — no `join`/parallel).
- Single-fact path byte-for-byte unchanged when gate off or no split.
- Every legal block in the merged answer carries ≥ 1 citation OR is the explicit
  "not found" fallback (Principle XI).
- No new crates, no DB migration, no frontend wire-format change.
</content>
