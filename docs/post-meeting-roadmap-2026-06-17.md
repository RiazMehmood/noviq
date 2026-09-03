# Post-Meeting Roadmap — 2026-06-17

Captures expert advice from the 2026-06-17 meeting (Kaggle Master + AI HoD) and
settles it into four sequenced work-streams with story numbering and
dependencies. This is the source-of-record for the upcoming specs; each stream
gets its own SDD spec → plan → tasks cycle when picked up.

Precedence reminder: this doc is research/planning. Code changes still require
the brainstorming → design-approval gate and the repo's SDD flow.

---

## Expert advice received (verbatim intent)

1. **Tabular ingestion** — convert tabular data into text during ingestion
   (the current US8 TAB-band-aid is insufficient; §3 Delegation-of-Financial-
   Powers still garbles after re-index).
2. **Draft without the LLM** — compose summaries/notes deterministically.
3. **Knowledge graph over vector search** — legal data is relational; a
   rule-based / learning-based KG can answer relational queries instantly and
   feed deterministic drafting.

---

## Settled decisions (discussion outcomes)

- **D-A — Vector search is NOT replaced.** It is the only offline tool that
  bridges the *semantic gap* (officer's words ≠ statute's words; e.g. "sole
  distributor of Canon scanners" → "single-source / Direct Contracting").
  The KG is added as a **relational fast-path in front of** hybrid retrieval,
  not a replacement. Router decides per query class.
- **D-B — Rule-based KG backbone, learning-based only at the fuzzy edge.**
  Legal cross-references follow predictable surface patterns ("under section X",
  "as amended by", "subject to rule Y", "as defined in") → deterministic,
  explainable, offline, no training data. Reserve learning-based extraction
  for semantic-contradiction *candidate* surfacing only.
- **D-C — Deterministic frames align with the citation-grade contract better
  than the LLM.** The 7B is already confined to the prose lane (no numbers /
  quotes / refs — `compose.rs` + `verify.rs`). Every recent failure (¶2
  fabricated gloss, advocacy survival, grammar break from the strip, 3-sentence
  prayer, restating) is a *model artifact* that deterministic frames remove by
  construction. Cost = stiffer prose, acceptable for secretariat register.
- **D-D — Clean ingestion is a hard prerequisite for the KG + error-detection.**
  Most "mistakes" the KG would find in the current corpus are our own
  ingestion artifacts (year-as-rule labels D2, junk chunks D1, mislabels), not
  errors in the source law. `detect_label_mismatches()` (Fix-2, `f9835baa`)
  already proves this — it currently catches ingestion defects. US8 must land
  before the graph is meaningful.
- **D-E — Error-detection splits.** Structural/relational errors (dangling
  cross-refs, broken amendment chains, numbering defects, orphan definitions,
  cycles) are deterministic and high-value. Semantic contradictions are
  research-grade, high-false-positive → human-reviewed candidates only, never
  auto-asserted.

---

## Story numbering (settled)

| Story | Title | Layer | Re-index? |
|-------|-------|-------|-----------|
| **US8** | Ingestion structural quality | index-time | **yes (one)** |
| **US11** | Deterministic-frames drafting (no-LLM) | compose-time | no |
| **US12** | Knowledge-graph relational fast-path (search) | retrieval | needs graph build |

- **US8 absorbs the old US11 scope** (junk filter D1 + year-guard D2) so a single
  re-index covers table reconstruction + junk + year. ACs stay **itemized** per
  defect so the story remains falsifiable.
- **US11 is re-assigned** to deterministic-frames drafting (was junk+year).
- US12 is new.
- **US13 (KG error / inconsistency detection) is OUT of current scope** —
  deferred to a future cycle (see "Deferred" below).

---

## US8 — Ingestion structural quality (index-time, one re-index)

**Goal:** clean, citable chunks; replace the `text_normalizer` TAB band-aid.

- **R1 — Tabular module** (`parsers/table.rs`, recommended Option A):
  - Detect: `pdftotext -bbox` per-word boxes; region is tabular when consecutive
    y-bands share ≥2 stable x-column boundaries. **Simple grids only in v1**
    (Delegation §3 is a simple grid); merged/multi-line-header tables out of scope.
  - Reconstruct: cluster words → cells by (column x-range, row y-band); fold a
    wrapped cell's extra lines into its logical row until the next serial key.
  - Linearize: each row → ONE self-describing unit, headers folded in
    (`"Power to declare surplus motor vehicles — Authority: Administrative
    Department; Extent: Full powers."`). First column (serial) = unit label.
  - Emit: one chunk per row, **verbatim cell text**, correctly labeled.
- **R2 — Junk filter (D1):** index-time reject of boilerplate (contact / page /
  form markers) AND low prose-content ratio. Generic, no per-doc terms.
  Target: 0 of the 101 junk chunks (87 in `10_SPSC_Regulations_2023.pdf`).
- **R3 — Year-guard (D2):** port `looks_like_year` into the structure_parser
  label assignment so a bare 1800–2099 4-digit is never a Rule/Section number.
  Target: 0 of the 39 year-as-rule labels.

**ACs (itemized):** §3 Delegation rows coherent + labeled · 0 junk chunks ·
0 year-labels · non-table prose zero regression.

---

## US11 — Deterministic-frames drafting (compose-time, no re-index)

**Goal:** compose the summary/noting from fixed sentence frames filled by
already-deterministic slots; drop the 7B from the drafting prose path.

- Per-section frames hard-code the existing `skeleton.rs` guidance.
- Slots: officer subject (verbatim/neutralised), provision card (verbatim quote +
  label), figures (verbatim), stakeholders (extracted nouns).
- Removes the fabrication surface entirely (no `[CITE:n]` gloss possible).
- **Complement, not substitute, for US8:** removes fabrication at output; US8
  removes junk at input. Both needed.
- **Decision still open:** fully deterministic (drop 7B) vs. hybrid (frames
  default, optional 7B polish). Recommendation: fully deterministic.
- Loops back to `verify.rs` — caps/quotes/number gates still apply.

---

## US12 — Knowledge-graph relational fast-path (search)

**Goal:** answer navigational/relational queries by exact graph traversal,
instantly, deterministically; keep hybrid vector for situation-discovery.

- Substrate exists: `graph_nodes`, `doc_ir` node trees (`parent_id` hierarchy),
  Pass-2 NER.
- New: **edge extraction** (rule-based, D-B) for cross-references / amendments /
  definitions; a graph query path; a router rule (`query_router` /
  `intent_router::decompose`) that classifies relational vs semantic queries.
- Precedent: the existing citation fast-path + `is_definitional` regex.
- Query classes it owns: by-number, definition, cross-ref, amendment chain,
  hierarchy/children.

---

## Dependency graph & sequence (current scope)

```
US8 (clean ingestion)  ──┬──>  US12 (KG search)
                         │
                         └──>  US11 (det. frames)   [benefits from US8, but
                                                     independently removes
                                                     fabrication; can start
                                                     in parallel]
```

**Recommended order:** US8 first (foundation + the one re-index) → US11 in
parallel (compose-time, independent) → US12 (needs clean graph). US8 is the
gating dependency for the graph stream (D-D).

---

## Deferred — NOT in current scope

### US13 — KG error / inconsistency detection (future cycle)

Captured for when we pick it up; explicitly out of current scope.

**Goal:** surface defects — first in the corpus, ultimately in the app's own
drafts (tie to `verify.rs`).

- **Structural (deterministic, high-value):** dangling cross-refs, broken
  amendment chains, numbering defects (dupes/gaps/out-of-sequence), orphan
  definitions, circular references (cycle detection). Generalizes the existing
  `detect_label_mismatches()` seed.
- **Semantic (research-grade, careful):** contradiction *candidates* via
  embeddings + learning-based comparison → human review, never auto-assert.

Depends on US12 (graph edges) which depends on US8 (clean ingestion).
