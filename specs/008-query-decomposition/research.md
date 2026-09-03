# Phase 0 Research: Query Decomposition

All decisions below were resolvable from the existing codebase + the spec's locked
decisions; no NEEDS CLARIFICATION remained.

## R1 — Capability gate signal & threshold

**Decision**: Enable decomposition only when the active generator is capacity-limited.
Primary signal: parsed GGUF parameter size via the existing `pub(crate)`
`query_router::discover_local_gguf()`, which returns `(path, label)` where `label` is
already the parsed size string ("0.5"/"1.5"/"3"/"7"). The gate does
`label.parse::<f32>().ok()`. NOTE: `parse_param_size_b` is **private** to `query_router`
and is NOT callable cross-module — the label already carries the size, so it is not
needed.
Rule: enabled when `size ≤ 1.5` (covers shipped 0.5B + 1.5B; excludes 3B+). When size is
indeterminate (no size token in filename), fall back to `HardwareTier`: capacity-limited
when `!has_discrete_gpu && ram_gb < 16`. Operator override env var
`ECHO_DECOMPOSE` (`1`/`on` force-on, `0`/`off` force-off) wins over auto-detection.

**Rationale**: The model size is the most direct proxy for "can it do multi-fact." It is
already computed; no new detection. Hardware fallback mirrors Principle VII. The env knob
mirrors the established `ECHO_FORCE_TIER` / `ECHO_RERANK` pattern.

**Alternatives considered**: (a) gate purely on `HardwareTier.max_model_tier` — rejected,
the user can hot-swap a big GGUF onto a small machine, so model size is more accurate;
(b) always-on — rejected, wastes time on capable models (US4); (c) gate on the
`tier_used` label returned *after* a call — rejected, the gate must decide *before* the
first call.

## R2 — Rule-based detection of the two shapes

**Decision**: Two independent detectors, both pure functions on the query string (+ slots):
- **Enumeration**: regex for ≥2 ordered lettered markers `(\(([a-z])\))` — e.g. `(a)`,
  `(b)`, `(c)` — appearing in sequence. Split points are the marker positions.
- **Multi-article**: reuse `slots::extract(query).section_paths`; trigger when
  `section_paths.len() >= 2` (e.g. `["Article 23", "Article 24"]`). Already battle-tested
  (Fix 1/2 hardened it).

Enumeration takes precedence when both match. If neither fires, or a fired detector
yields `< 2` sub-questions, return `None` → single-pass fallback.

**Rationale**: Both signals already exist or are trivial regex; deterministic and fast
(microseconds). Reusing `section_paths` avoids a second provision parser.

**Alternatives considered**: numeric `1. 2. 3.` and bullet markers — deferred (out of
first-cut scope, lower frequency, higher false-positive risk on legal text that contains
sub-clause numbering); LLM detection — explicitly rejected by spec.

## R3 — Sub-question construction (no model)

**Decision**: Build each sub-question by template, preserving the parent's framing:
- **Enumeration**: `stem` = text before the first `(a)` marker; each facet = text between
  consecutive markers. Sub-question = `"{stem} {facet}".trim()` with a trailing `?` if the
  stem implies a question. Example: stem *"Identify the Principles of Policy articles
  dealing with"* + facet *"the Islamic way of life"* → *"Identify the Principles of Policy
  articles dealing with the Islamic way of life"*.
- **Multi-article**: split the question on clause conjunctions (`, and `, `; `, ` and `)
  into clauses; keep each clause that contains a section reference. Each clause is already
  self-contained and names its article (e.g. *"What property rights does Article 23
  grant"*). If clause count ≠ provision count, fall back to a per-article template
  `"What does {Article N} provide?"` so every named provision still gets a sub-question.

**Rationale**: Reusing the stem/clause keeps the officer's intent and the legal framing
intact, which is what makes each sub-question a clean single-fact lookup.

**Alternatives considered**: fixed canned templates ignoring the stem — rejected, loses
question nuance (e.g. "additional protection").

## R4 — Deterministic template merge + citation remap

**Decision**: Merge with a fixed, model-free template. One labelled block per
sub-answer, in order:

```
(a) the Islamic way of life — {sub-answer text} [1]
(b) promotion of local Government institutions — {sub-answer text} [2]
(c) participation of people in the Armed Forces — {sub-answer text} [3]
```

(For multi-article: label = the provision, e.g. `Article 23 — …`.) Citation remap:
concatenate all sub-answers' citation lists into one global list; renumber sequentially.
Because the 0.5B usually emits **no** `[n]` markers (citations are auto-attached at
compose), the merge **synthesizes** each block's trailing `[n]` deterministically from
that block's resolved citation rather than assuming a marker is present; if a sub-answer
*does* contain local `[k]` markers, they are rewritten to the new global indices. A part
whose sub-answer was the grounded "not found" fallback renders that text verbatim with no
fabricated citation. Merge operates on `query_router::Citation`; the merged citation list
is mapped to `intent_router::Citation` via the existing `qr_citation_to_chat` **after**
merge, when building the `ChatResponse`.

**Rationale**: Deterministic assembly cannot alter facts/figures (FR-004) and keeps
citations valid (FR-009, Principle XI). Renumbering avoids duplicate/colliding `[1]`s
across blocks.

**Alternatives considered**: LLM "make it readable" pass — rejected by spec (re-corrupts
facts); keeping per-block independent `[1]` numbering — rejected, ambiguous in one answer.

## R5 — Sequential execution, cancellation, tracing

**Decision**: Loop sub-questions **sequentially**, awaiting each
`route_scoped_with_override` before the next (Principle III — one model resident).
Thread the existing `CancellationToken` into every sub-call; on cancel, stop the loop and
return the standard cancelled `ChatResponse` (no partial merge committed). Emit a trace
note per sub-question ("Sub-question k/N: …") reusing the existing `QueryTrace`.

**Rationale**: Sequential is mandatory for the RAM invariant and is also simpler. Cancel
semantics match the current single-pass behaviour.

**Alternatives considered**: parallel sub-questions — rejected (violates Principle III,
risks OOM on 8 GB).

## R6 — Orchestration seam

**Decision**: All orchestration lives in `intent_router::dispatch`. After
`slots::extract`, compute the gate; if enabled, call `decompose::detect_and_split`; if it
returns `Some(subs)` with `len ≥ 2`, run the sequential loop + `decompose::merge_subanswers`
and build the `ChatResponse` from the merged answer + remapped citations. Otherwise the
existing single call runs unchanged.

**Rationale**: `dispatch` already owns `tier`, `slots`, the cancel token, and the seam
call — the smallest possible diff. `decompose.rs` stays pure/testable; `dispatch` only
wires it.
</content>
