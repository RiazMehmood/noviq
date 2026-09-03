# Phase 0 — Research & Decisions: Ingestion Hardening

All open questions from the spec's Assumptions and the Iteration-1 audit are resolved here. There are
**no remaining NEEDS CLARIFICATION** items — the spec quality gate already cleared three candidate
ambiguities (footnote retention, catchline representation, vocabulary-on-conflict) with documented
refuse-or-flag defaults. This file records the technical decisions that back them.

---

## D-1. Two-pass spine: structure before vocabulary

- **Decision**: Keep structure detection (`structure_parser` `DocNode` tree, `NodeLevel`,
  `chunks.unit_type`/`parent_chunk_id`) strictly separate from vocabulary (the citation word
  Article/Section/Rule). Structure emits **identifier + level only**; a later resolver binds the word.
- **Rationale**: The codebase *already* has this separation plus a `label_status='unresolved'` state
  (migration `003_chunks_provenance.sql`). D1 (everything labelled "Section") is a layering bug —
  `generic.rs` mints `Section {num}` *during detection*. Fixing at the resolver layer is the smallest
  correct diff and is generic across profiles.
- **Alternatives considered**: (a) Rewrite the parser around a new IR — rejected (huge diff, risks the
  verbatim invariant). (b) Hardcode "Article" for the Constitution — rejected (violates generic-domain;
  would not fix Acts/Rules).

## D-2. Geometry zoning, not text regex, for page-numbers & footnotes (D5/D6)

- **Decision**: Classify footer page-numbers and footnote apparatus using **layout geometry** —
  `Block.y_band` (already on the chrome `Block`) + `pdftotext -bbox` `WordBox` positions + cross-page
  repetition — never a text pattern. Footnotes → `block_class='footnote'` (kept, retrievable);
  page-numbers → chrome.
- **Rationale**: Legal text is dense with bare integers ("fourteen years", cross-references, "First
  Schedule"). A text rule that strips standalone numbers would eat real content (edge case in spec).
  Geometry (footer y-band + the integer increments across pages) is the reliable discriminator. The
  `pdftotext -bbox` path and `WordBox` already exist (used by the F012 table reflow).
- **Alternatives considered**: Pure text-regex stripping — rejected per the spec edge case. Discarding
  footnotes like chrome — rejected: amendment history has citation value (US5 depends on it).

## D-3. Footnote anchoring by marker, not by page

- **Decision**: Anchor a footnote to the provision indicated by **its superscript marker position**.
  If the marker cannot be confidently located, store `anchor_reference = NULL` + `label_status =
  'unresolved'`. Never anchor "to the article on the same page".
- **Rationale**: Pages hold multiple articles (spec edge case). A same-page heuristic produces
  confident-wrong anchors — the exact failure class this feature exists to eliminate.

## D-4. Catchline ↔ body merge representation (D4)

- **Decision**: A provision's catchline (title line) and its enacting text are **one logical Article
  node** — catchline → title/marginal-note metadata, enacting text → body — with the source spans
  preserved as a span list so verbatim text + `page_number` survive.
- **Rationale**: Matches spec Assumption + FR-003. The audit shows ~33 orphan bare-label chunks
  ("Section 8" with 9 chars) caused by splitting these. Discriminator: catchline = number + short
  Title-Case phrase, no enacting marker; enacting = number + `(1)`/sentence. Provisions with no
  separate catchline are handled without inventing a title.
- **Alternatives considered**: Catchline as a titled parent with body as first child — rejected
  (complicates citation display and parentage; metadata-on-node is simpler and lossless).

## D-5. Vocabulary confidence layering (D1, FR-004)

- **Decision**: Resolve the citation word by layered evidence, first match wins, conflict → unresolved:
  1. **Document profile** (`doc_profiles`: constitution/act/rules/manual/generic).
  2. **Internal linguistic evidence** — the document's own wording ("in this **Article**", "New
     **Article** 10A ins.", "Article N substituted").
  3. **Explicit user override** (optional, none required this iteration).
  4. None agree / insufficient → `citation_unit = NULL`, `label_status = 'unresolved'`.
- **Rationale**: Spec FR-004 + Assumption "vocabulary on conflict resolves to unresolved". The
  fundamental_rights audit proves the evidence is *inside* the document (footnotes literally say
  "New Article …"). Profile-primary with linguistic corroboration is generic and auditable.

## D-6. 25/25A anomaly — flag, never auto-correct

- **Decision**: The source prints Article 25/25A catchlines out of order. The system reflects the
  source and marks the ambiguity `unresolved` where ordering cannot be faithfully determined. The
  acceptance test asserts the *flag*, not a rewritten order.
- **Rationale**: Spec Edge Case + FR-011 + SC-007. Silent "correction" = synthesized structure =
  exactly the confident-wrong failure class we forbid.

## D-7. page_number provenance (D12)

- **Decision**: Thread the already-available `parsed.page_offsets` into
  `structure_parser::persist_tree`; map each node's `char_start` → physical page; set
  `chunks.page_number`. Mirrors what `persist_chrome_chunks` already does for chrome blocks.
- **Rationale**: Root cause is code-confirmed: `persist_tree` never sets `page_number` (stays `None`)
  and isn't given `page_offsets`. Pure plumbing; verify monotonic + within `[1, page_count]`.

## D-8. "shallnot" / JOIN_TARGETS audit (D9)

- **Decision**: Remove `"shallnot"` from `text_normalizer.rs` `JOIN_TARGETS` (:33) and audit the whole
  list, keeping **only true intra-word repairs** ("o ne"→"one", "sha ll"→"shall"); drop any entry that
  fuses two real words across a legitimate space. Add a guard test: a normalization rule may not turn a
  2-token legal phrase into a non-dictionary single token.
- **Rationale**: `repair_spaced_words` fused "shall not" → "shallnot", altering quoted law text — a
  citation-grade killer (FR-007). The fix is deletion + a property guard so it can't recur.

## D-9. US5 capture target — existing graph substrate

- **Decision**: Capture amendment events into `amendment_events` (kind ∈
  inserted/substituted/repealed/omitted; `by_instrument` verbatim) and cross-references into
  `graph_edges` (`edge_type='cross_reference'`; amendments also `edge_type='amended_by'`). Both anchor
  to `graph_nodes`. Capture only — no traversal/ranking/routing.
- **Rationale**: Tables already exist (migration 001). `graph_builder.rs` already has
  `article_refs()` regex for in-text references — extend it to emit edges. FR-014/FR-015 require
  verbatim identifiers or `unresolved`; never invented links (SC-009). Graph *querying* is a Non-Goal.

## D-10. OCR / extraction routing (decided, deferred)

- **Decision**: Digital PDF → `pdftotext` (primary). Scanned PDF → PaddleOCR (scan-only) **deferred**.
  Hybrid PDFs route per page. **None of D1–D12 are OCR-fixable** — they are parser/normalizer/persist
  defects on clean digital text.
- **Rationale**: Spec Non-Goals. The PaddleOCR spike is parked (scans are last in the modular plan).

---

## Schema readiness (confirms minimal/zero migration)

Already present and reused (no migration needed for most of the feature):

| Need | Existing column / table |
|---|---|
| Page provenance | `chunks.page_number BIGINT` (set it for provisions) |
| Block class (footnote/chrome/provision) | `chunks.block_class` (default `'provision'`, migration 003) |
| Flag-don't-fabricate | `chunks.label_status` (`resolved`/`unresolved`, migration 003) |
| Raw marker / confidence | `chunks.raw_marker`, `chunks.boundary_confidence` (migration 003) |
| Structure level + parentage | `chunks.unit_type` (incl. `chapter`,`article`,`clause`,`subclause`), `parent_chunk_id` |
| Cross-reference edges | `graph_edges.edge_type='cross_reference'` |
| Amendment edges/events | `graph_edges.edge_type='amended_by'`, `amendment_events` table |

**Only possible additive migration (004)**: a footnote **anchor** column if `domain_metadata` JSON is
deemed insufficient for `anchor_reference`. Decision: prefer storing the anchor + resolved/unresolved
in existing `domain_metadata` JSON first; add a column only if a query path needs it. Migrations stay
additive (Principle: no drops).
