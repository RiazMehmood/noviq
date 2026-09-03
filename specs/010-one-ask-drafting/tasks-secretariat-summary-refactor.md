# Tasks (Feature 010 extension): Secretariat-Summary Drafting Refactor

**Spec**: `spec-secretariat-summary-refactor.md` · **Branch**: `011-postgres-pgvector-port`
**Cadence**: batch a slice, ONE `cargo test --lib drafting` per slice (NTFS cold compile ~36 min).
**Locked decisions**: D1 verbatim-compressed · D2 generic (no keyword lists) · D3 one skeleton +
profiles · D4 structural now, precision post-clean-text.

Legend: [ ] todo · [x] done · (file) primary touch · ⛔ blocked-until.

> **Status 2026-06-16**: DONE (committed-ready, `cargo test --lib drafting::one_ask` 57/57) —
> T001, T002, T003, T004, T005 (anti-starvation removed), T007, T009, T010, T011, T012, + "Paragraph N:"
> scrub. PENDING — T006 (explicit no-dup assertion; behaviour enforced by single-ownership), T013 (audit
> logging), T014 (live gate: rebuild+relaunch, no re-index). Integration `one_ask_acceptance.rs` assertion
> updated to `1..=3` but not yet run (PG). Slices 2–4 (doc-type profiles, tabular re-extraction, relevance
> floor) not started.
>
> **Update 2026-06-16 (Slice 3a)**: T015/T016/T017 (doc-type classifier + profile + wiring) DONE & GREEN
> (`drafting::one_ask` 61/61, +4 doc_type tests) — Procurement-Note Legal hints drop the dev-scheme bias.
> **NOT live-validated yet** (precision is judged on the live corpus). US6 distance-floor SKIPPED
> (distances non-discriminative per H diagnosis); doc-set narrowing is the fallback lever if doc-type
> hints don't surface Direct Selection. T018 live gate + US8 tabular re-extraction pending.

---

## Slice 1 — Structural, corpus-independent (US2, US3, US4, US5, US7)

### Citation policy + caps (US2 / FR-003)
- [ ] **T001** Add a per-section citation cap to the skeleton/profile model: `max_citations`
      (BG 0, Legal 2, Financial 1, Proposal 0, Prayer 0). (skeleton.rs / new profile struct)
- [ ] **T002** Enforce caps in `assemble_and_verify`: count rendered `Citation` runs per section;
      drop markers beyond the cap (style cut, NOT a Flag); suppress all citations in 0-cap
      sections. (verify.rs) + tests: over-cap Legal keeps 2; BG marker suppressed.

### Single-section ownership + best-fit (US3 / FR-004,005)
- [ ] **T003** `MAX_SECTIONS_PER_CARD = 1`. (research.rs)
- [ ] **T004** Replace first-come-first-served assignment with **argmax**: track each card's best
      section score during research; assign the card to that section only. (research.rs) + test:
      a card scoring highest for Legal serves Legal only.
- [ ] **T005** Reconcile anti-starvation with cap=1: a starved section may adopt only a card not
      already owned elsewhere; otherwise stays uncited (defer to US5 narrative fallback). (research.rs)
- [ ] **T006** Assert no chunk id renders in >1 section (assembly-level guard + test). (verify.rs)

### Verbatim-compressed citations (US4 / FR-006,007 · D1)
- [ ] **T007** Tighten `citation_snippet`: target one operative clause (lower `SNIPPET_MAX_WORDS`
      ~30→~20; keep sentence-boundary + abbreviation guards), exact char-slice. (verify.rs) + test:
      long excerpt → single clause; `quote_verifies` still true.
- [ ] **T008** Inline render shape: `rendered_label` + compressed quote read as application
      (e.g. "…direct contracting from a single source (Rule 46, SPPRA 2010)"). No model-authored
      meaning. (verify.rs render path) + test: shape + invariant 1 holds.

### Secretariat style + structure (US5 / FR-008,009,010)
- [ ] **T009** Rewrite `build_section_prompt` rules block to the Secretariat style profile:
      concise/formal/decision-oriented; ban advocacy, greetings, "we request/kindly approve/we
      await", academic commentary, repetition. (compose.rs)
- [ ] **T010** §1 establishes the case; §2–§5 prompts say "Do not restate facts already
      established in previous sections unless essential." (compose.rs / skeleton guidance) + test:
      prompt contains the no-restate instruction for non-BG sections.
- [ ] **T011** Prayer guidance → 1–2 sentence approval, no citation (compose.rs/skeleton §5);
      enforced by the 0-cap from T002.
- [ ] **T012** Structured financial fallback string when figures empty (compose.rs financial
      branch) — replace "to be determined later". + test.

### Audit logging (US7 / FR-012)
- [ ] **T013** Structured per-card accept log (section, vec, lex, fusion, reason) + per-card
      reject log (below-floor placeholder / duplicate / over-cap / out-of-doc-set). (research.rs +
      verify.rs) — floor/doc-set reasons stubbed until US6.

- [ ] **T014** Slice-1 gate: `cargo test --lib drafting` green; one live re-run of the
      Canon-scanner ask; confirm SC-002/003/004/006 (dedup, no-restate, ≤3 cites, crisp prayer).

## Slice 2 — Document-type profiles (US1 / FR-001,002 · D2,D3)

- [ ] **T015** New `doc_type.rs`: lightweight classifier (intent verb + subject nouns →
      ProcurementNote / AdministrativeApproval / FinancialSanction / TransferPosting /
      PolicyApproval / CabinetSummary / DevelopmentScheme / General). Generic — no domain literals
      baked into internals (derive emphasis from request + profile config). (one_ask/doc_type.rs)
- [ ] **T016** `DocTypeProfile` struct: per-section guidance overrides, citation caps (from T001),
      retrieval emphasis weights. Default `General` = current behavior. + tests.
- [ ] **T017** Wire profile into `mod.rs` pipeline: classify after intent parse; pass profile to
      research (emphasis) + compose (guidance) + verify (caps). (mod.rs)
- [ ] **T018** Slice-2 gate: classification correct for the Canon-scanner ask; profile applied;
      `cargo test --lib drafting` green.

## Slice 3 — Ingestion structural quality (US8 R1+R2 / FR-013, 013a, 013b) ⛔ extraction
> Re-scoped 2026-06-17 (`docs/post-meeting-roadmap-2026-06-17.md`). One re-index covers all.
> Decisions: rows headers-folded · `ECHO_TABLE_REFLOW` env toggle default ON · R1+R2 together.

> **Update 2026-06-17 (Slice 3, R1+R2)**: T019–T025 DONE & GREEN (`cargo test --lib` 451/0, +10
> tests). One re-index + live verification (T026 b/c/d) still pending. SDD (not TDD). PHR 0020.

### R1 — Tabular module (FR-013)
- [x] **T019** New `parsers/table.rs` pure core: `WordBox { text, x, y, w, h, page }` +
      `reflow_tables(words: &[WordBox], layout_text: &str) -> String`. No IO. Algorithm:
      split by page (`\f`); per page group words→lines (y-band), detect a table region
      (≥3 consecutive lines conforming to the same ≥2 stable x-column boundaries), assign
      words→columns, group logical rows (non-empty col-1 starts a row; empty col-1 = wrapped
      continuation appended per column), linearize **headers-folded** verbatim
      (`"{c1}. {c2} — {h3}: {c3}; {h4}: {c4}"`); pages with no table keep their `-layout` text;
      ambiguous/ragged region falls back to `-layout` (confident-only). + unit tests:
      Delegation-§3 fixture (serial/power/authority/extent + one wrapped cell) → in-order
      self-describing rows; prose-only words → unchanged; ragged region → fallback.
- [x] **T020** Thin IO shell in `parsers/pdf.rs`: add `extract_with_pdftotext_bbox` (`-bbox`),
      parse the XML into `Vec<WordBox>`, and when `ECHO_TABLE_REFLOW != "0"` (default ON) call
      `table::reflow_tables` on the Tier-1 `-layout` text; recompute `page_offsets` from the
      reflowed text. Prose-only docs unaffected. (parsers/pdf.rs)

### R1 — Year-rule (FR-013a)
- [x] **T021** Port `looks_like_year` into the structure-parser label assignment so a bare
      1800–2099 4-digit in a title position is never emitted as a Rule/Section ordinal
      (`doc_profiles/rules.rs` num-capture guard or `structure_parser` label step). + unit
      test: `"…Rules, 2017…"` does NOT yield `Rule 2017`; real ordinals (260/25A) still parse.

### R2 — Generic boilerplate de-prioritisation (FR-013b)
- [x] **T022** Pure classifier `is_boilerplate(content, has_provision_identity) -> bool`:
      generic/structural (low prose-content ratio AND no provision label/marker), NO per-doc
      keyword list. **Provision-label-keep invariant**: returns false whenever a provision
      identity is present. (new `parsers/boilerplate.rs` or `text_quality`) + unit tests
      (contact-page junk → true; terse real rule "6. Language." → false; labelled chunk → false).
- [x] **T023** Persist the flag: additive migration `chunks.is_boilerplate BOOLEAN DEFAULT
      FALSE`; set it from T022 at persist time (`structured_importer`). **Flag, not delete** —
      the chunk row stays.
- [x] **T024** Retrieval exclusion: vec + lexical retrieval (`one_ask/research.rs` and any
      shared retrieval) filter `WHERE NOT is_boilerplate` so flagged chunks never rank.
- [x] **T025** **Dry-run-review gate** (FR-013b iv): a command/log that emits the exact
      would-flag chunk list (id, source, label, snippet) WITHOUT changing the index, for review
      against the 101 labeled junk (87 in `10_SPSC_Regulations_2023.pdf`). Must show zero
      false positives (no labelled / real-content chunk) before the re-index is committed.

### Gate
- [ ] **T026** `cargo test --lib` green (single run at end). Then ONE re-index; verify (a)
      DG/Ombudsman financial-powers rows coherent + labeled, (b) 0 year-as-rule labels,
      (c) dry-run zero false positives, (d) non-table prose zero regression.

## Slice 4 — Precision, post-clean-text (US6 / FR-011 · D2) ⛔ Slice 3

- [ ] **T022** Absolute `MIN_RELEVANCE` floor alongside the relative margin; thin-over-wrong
      (uncited narrative when nothing clears it). (research.rs) + test.
- [ ] **T023** Generic top-N doc-set narrowing: rank source documents by aggregate relevance to
      the request, retrieve only within the top-N. No keyword lists. (research.rs) + test.
- [ ] **T024** Tune floor/N on the re-indexed corpus using the US7 audit log.
- [ ] **T025** Final success-criteria gate: SC-001…SC-008 on the Canon-scanner ask.

---

## Notes / cross-cutting
- US7 audit logging (T013) is intentionally early so Slices 3–4 can be tuned on evidence.
- T007/T008 must keep `quote_verifies` true by construction (char-slice), preserving SC-008.
- Changelog entry + PHR per slice; this doc tracks status.
