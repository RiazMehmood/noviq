# Phase 0 Research — Feature 012

Decisions that resolve the open questions before design. Each: **Decision /
Rationale / Alternatives rejected.** Grounded in the senior-engineer design
review (D1–D7), `docs/chunk-defects-remedies-status-2026-06-17.md`, and live-DB
verification (prod PG, 4277 chunks, post-US8 re-index).

## R0 — Disposition of the carried-over US8 code (uncommitted, GREEN 451/0)

**Decision.**
- **`parsers/pdf.rs` bbox IO** (`extract_with_pdftotext_bbox`, `parse_bbox`, `WordBox`, `maybe_reflow_tables`, `page_offsets` recompute): **KEEP and reuse** — this is the trusted single geometry source US1 + US5 both build on.
- **`parsers/table.rs`** (`reflow_tables`, `reflow_page` pairwise-gap heuristic): **EVOLVE** into the projection-profile engine (US5). The geometry intake (`WordBox`, region detection) stays; the column-assignment core is replaced.
- **Year-rule** (`heading_detector/regex_pass.rs::looks_like_year` guard): **KEEP as-is** — it is parser correctness (a bare 1800–2099 is never a Rule/Section ordinal), already regression-locked, and US3 builds on the same scan path.
- **`parsers/boilerplate.rs` + `is_boilerplate` flag + `flag_boilerplate_chunks`/`dry_run_boilerplate` + `retrieval.rs` filter + migration `002`**: **SUPERSEDE.** The downstream flag-by-label approach was proven non-functional live (flagged 0/4277 because every junk chunk had inherited a provision label upstream). US1 fixes this at the source by classifying chrome **before** tree-build.
  - **Migration `002_chunks_boilerplate.sql`: keep the column, repurpose.** It is already additive and applied. Rather than a fresh migration, US2's `block_class` can subsume `is_boilerplate` (chrome = one block class), and the existing retrieval filter `AND c.is_boilerplate = FALSE` is retained as a cheap exclusion of chrome-classed chunks. The pure `is_boilerplate()` *function* and `flag_boilerplate_chunks` persist-time call are removed (replaced by upstream classification); the *column* lives on as the retrieval-side exclusion that the upstream classifier now populates correctly.

**Rationale.** The user direction is "evolve, not delete" and the US8 tree is a GREEN checkpoint. Reusing the bbox intake and the migration avoids churn; replacing only the proven-insufficient pieces (pairwise-gap core, label-based flag) is the smallest viable diff that still fixes the live defect at its root.

**Alternatives rejected.** (a) Delete the whole US8 tree and start clean — discards a working geometry intake + an applied migration for no benefit. (b) Keep the downstream flag and tune its thresholds — the live result (0/4277) proves the stage is wrong, not the threshold.

## R1 — Chrome-strip signals (US1, fixes D3)

**Decision.** Classify each pre-tree block as `provision` vs `chrome` using generic structural signals in power order, **no per-document keyword lists**:
1. **Cross-page positional repetition (strongest).** Text recurring at the same vertical band (top/bottom margin, normalized y) across ≥K pages = chrome (mastheads, running headers/footers, "P a g e" markers). K conservative (≥3 or ≥40% of pages, whichever larger).
2. **Margin geometry.** Short text isolated in the top/bottom margin band (from `-bbox` y-coordinates) with no provision marker.
3. **Form-field structure.** Fill-blanks (`____`, dotted leaders), isolated date lines, all-caps masthead blocks, signature/stamp lines, contact lines (email/URL/phone token shape — structural, not a keyword list).

Chrome is **kept** as a separate block class (auditable, reversible), never deleted, never attached under a provision node.

**Rationale.** D3 root cause is that chrome was swept under real provisions and inherited labels (masthead → `Rule 345(3)(b)`). Positional repetition is the one signal that generalizes across all 10 sources without naming any of them, and it directly kills mastheads/page-markers — the dominant junk class (SPSC=87 of 101). Keeping (not deleting) preserves the G2 audit trail and is reversible.

**Alternatives rejected.** Keyword/regex lists for "Contact Us"/"INSTRUCTIONS TO CANDIDATES" — brittle, per-document, forbidden by spec non-goals. Downstream flag (the superseded US8 R2) — too late in the pipeline.

## R2 — Projection-profile table engine (US5, fixes D1)

**Decision.** Replace the pairwise-gap column detector with a **projection profile**: project `-bbox` word x-extents onto the x-axis across a candidate table region; consistent vertical whitespace bands (zero-density columns of the profile) = column boundaries. Then: assign words to columns by band, group logical rows (non-empty leading column starts a row; empty leading column = wrapped continuation), **carry the column signature across page breaks** (`\x0C`) to stitch cross-page rows and drop repeated headers, and linearize each row into one self-describing unit with headers folded in and **verbatim cells (G1)**. Confident-only: any ragged/ambiguous region falls back to the original `-layout` text (a missed table is acceptable; a scramble is not). Ruling-line recovery **deferred** (pdfium only if ever; never lopdf).

**Rationale.** Live verification showed real tables are vertically-stacked, multi-level, and cross-page — exactly the cases that beat a pairwise-gap heuristic. A projection profile is the standard borderless-table technique, is pure geometry from the already-trusted `-bbox` source, and preserves G1 because cells are lifted verbatim.

**Alternatives rejected.** lopdf ruling-lines (demoted for low-quality text; user pushed back). PaddleOCR/PP-Structure on digital pages (violates G1 — OCR is not verbatim; reserved for true scans, of which the corpus has none).

## R3 — Heading detection: numbering-sequence prior + boundary-stop (US3, fixes D4)

**Decision.** Add to `scan_with_profile`: (a) a **numbering-sequence prior** — the expected next ordinal is a strong recovery signal for missed headings (degraded `N. Title - body` shapes) and a rejection signal for out-of-sequence restarts (form fields restarting at "1."); (b) **boundary-stop** — never absorb a block whose leading token is a plausible next-in-sequence provision number; start a new node; (c) **promote `detect_label_mismatches` from warning to a split action** — when content begins with a provision number disagreeing with its label, split; (d) record **heading confidence**; low confidence → flag, not silent mislabel. Builds on the existing `drop_out_of_sequence_articles` + `leading_provision_number` already in `regex_pass.rs`.

**Rationale.** D4 mislabels are deterministic data bugs from heading-detection misses; the sequence prior is generic (no keywords) and the existing helpers already model "out-of-sequence restart". Promoting the mismatch detector to a split closes the label≠content gap structurally.

**Alternatives rejected.** Widening regex alone (high recall, promotes form-fields — the Compendium false-positive class). Wholesale OCR (regresses clean digital text).

## R4 — Resolved labels + provenance, kill synthetic-id (US2, fixes D5)

**Decision.** At persist time, set `unit_label` = the **resolved citation from the hierarchy** (e.g. `Rule 27(i)`), and persist alongside: `raw_marker` (the literal marker as seen), `unit_path` (existing), `block_class` (provision | chrome | table-row), `page`, `char_span`, extraction+reconstruction method, and `boundary_confidence`. **Eliminate the silent synthetic-id fallback** (`m2`/`m66`): a parse that cannot assign a marker emits an **`unresolved` flag**, never a meaningless id. (Exact synthesis site to be pinned at implementation; the contract is: replace fallback id with a surfaced flag.)

**Rationale.** D5 root cause is bare/synthetic labels that cite nothing real. Resolving from the hierarchy at persist + keeping the raw marker gives a correct citation with an audit trail; flagging instead of synthesizing honors refuse-or-flag-over-confident-wrong.

**Alternatives rejected.** Keep synthetic ids and resolve at query time (runtime guard never saw stored labels — the diagnosed gap). Drop unresolved chunks (loses content; violates keep-not-delete).

## R5 — Deterministic drafting (US4, fixes D7)

**Decision.** Compose summaries/notings from a **fixed slot+template structure**; the LLM authors **no paragraph prose**. Slots filled deterministically: verbatim subject, verbatim provision quote + resolved label, verbatim figures, extracted stakeholders. Verification becomes a **decidable whitelist check** (templates + an allowed-connector list), not an open-ended fabrication judgment. Optional later concession only if prose reads too stiff: sentence-level connectors from a fixed whitelist — never free prose. Evolves the existing `compose.rs`/`skeleton.rs`/`verify.rs` and the US10 move-per-paragraph skeleton already in place.

**Rationale.** D7 (the ¶2 fabricated gloss) is only possible because the 7B authors prose. Removing the prose surface makes fabrication structurally impossible and turns G1 verification into a decidable check. The secretariat register is formulaic enough that templates read naturally; verifiability outweighs stylistic flexibility (Officer-First, Citation Discipline).

**Alternatives rejected.** Keep LLM prose with a stronger verify gate — the gate cannot be made decidable against free prose; the live ¶2 violation proves the risk is real, not hypothetical.

## R6 — Re-index / re-embed sequencing

**Decision.** Wave A (US1→US2→US3) is **one re-index with a full re-embed** (boundaries change ⇒ stored embeddings invalid). Wave B (US4) is compose-time, **no re-index**. Wave C (US5) is **one re-index** gated by G1 + golden table fixtures. Each wave batches into a single ~30-min NTFS build/re-index. The G2 golden gate runs (cargo, no LLM) before each attended re-index.

**Rationale.** Minimizes the expensive NTFS re-index/re-embed cost while keeping each wave independently shippable (Principle VIII). G2 catches regressions cheaply before paying for a re-index.
