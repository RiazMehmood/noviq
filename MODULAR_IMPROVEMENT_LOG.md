# Modular Improvement Log — Noviq

> **Purpose.** This is the progress log for the new **modular, incremental** improvement
> approach (started 2026-06-19). It REPLACES `CHANGELOG_DEV.md` for this workstream —
> do **not** add `CHANGELOG_DEV.md` entries for modular-approach work; record them here.
>
> **Direction (final).** The LLM stays in the chat; we ship **AI-generated responses**.
> The earlier deterministic-drafting mode was test-only and is abandoned. Quality comes
> from cleaner inputs + better retrieval + checks around the answer.
>
> **Method.** Improve one stage at a time, in order, on a small controlled document set:
>
> 1. **Ingestion** (now) → 2. **Retrieval** → 3. **LLM response**
>
> Start with 3 simple documents (one simple-English prose doc, one small table doc, one
> digital PDF law doc; scans/OCR last). Get each clean before adding the next.
>
> **Companion files:** metrics live in `~/Desktop/Noviq_Ingestion_Tracker.xlsx`; the
> plain-English plan is `~/Desktop/Noviq_Ingestion_Improvement_Plan.pdf`.

---

## How to use this log

One dated entry per iteration. Keep entries short and factual. Suggested shape:

```
### YYYY-MM-DD — <stage> — <document> — <one-line headline>
- Observed: what the trace/response showed (the defect or the win).
- Changed: the single targeted fix (file:function if code).
- Result: before → after numbers; did the AI response improve? regressions?
- Next: the next single step.
```

Stages: `ingestion` | `retrieval` | `llm-response`.

---

## Entries

### 2026-06-22 — retrieval — SPPRA + Delegation-of-Financial-Powers — "single-contractor law + secretary's approval not coming"
- Observed (trace `c5f8b9a1`, query "draft note … direct contracting with sole distributor …"):
  the answer cited Rule 17(3) (advertisement waiver) instead of the actual direct-
  contracting rule, and omitted the Secretary's approval authority. Diagnosed against
  the live index (10,271 chunks):
  - The law IS indexed. Direct contracting = SPPRA **Rule 16(1)(b)** ("procurement
    from a single source … under the following conditions:") with the conditions in
    separate **Rule 16(1)(v…)** chunks (page 10). Secretary's authority = Delegation
    of Financial Powers **Rule 2(ix)/Rule 3** ("authorities in column 3 of the Second
    Schedule … to the extent in column 4") + Second-Schedule rows (546 chunks).
  - Root causes it didn't reach the answer:
    1. **Context cap too small** — generation used n_ctx=3584 with a 15-chunk / 2400-
       token context. Rule 16(1)(b) was literally in the vector candidate list but got
       dropped before the model. **(fixed below)**
    2. **Reranker hardcoded OFF** — RRF(vector+FTS) alone floats generic/frequent
       chunks (Rule 17 advertisement, Rule 26 Dak) above the precise provisions; no
       cross-encoder to lift the exact answer. (`ECHO_RERANK=1` re-enables; zero-shot
       reranker is domain-blind per earlier eval, so needs the fine-tune.)
    3. **Fragmented chunking** — the direct-contracting provision is split across a
       253-char stem + many small condition chunks; the Secretary's limits are split
       across Rule 3 + many Second-Schedule table rows. No single chunk is a strong,
       whole match → each fragment ranks weakly and the provision is never whole.
- Changed: **context + retrieval pool now scale to the model's real window**
  (`query_router::route_scoped_with_override`, commit `dbe75097`): n_ctx = resolved
  window (was 3584); ctx_budget = window − answer − overhead (≈30k tok on a 32768
  model, floored at the old 2400); ctx_top_k ≈ budget/150 (clamp 20..400); vector+FTS
  fetch (`fetch_k`) scales with it. This directly fixes root cause #1.
- Result: not yet re-run live (CPU 7B ≈ 11 min/answer; GPU later). Expected: Rule
  16(1)(b)+conditions and the Rule 3 delegation rows now fit the context together.
- Next: re-run the same query and confirm the direct-contracting conditions + the
  Secretary's column-3/4 authority appear; if precision is still off, (a) try
  `ECHO_RERANK=1`, then (b) fix chunking so a provision's stem+conditions stay whole
  (atomic provisions) — the highest-leverage ingestion fix.

### 2026-06-21 — llm-response — whole corpus — F015 assistant layer landed (the response stage)
- Observed: the "LLM response" stage of the modular plan needed a single, grounded,
  no-truncation conversational surface — the old separate chat + deterministic-drafting
  panes did not enforce the citation-grade contract uniformly, and long sessions could
  silently truncate law or lose the officer's case facts.
- Changed: shipped Feature 015 (`services/assistant/*`, `commands/assistant.rs`, single
  `ChatPage`) — one intent-routed surface with four invariants on EVERY capability:
  bureaucrat persona (config), the model-independent `[CITE:n]` verbatim citation gate
  (refuse-or-flag, never fabricate), the real-token priority context allocator
  (`Σ prompt + reserve ≤ n_ctx` proven before generation, law kept whole + ahead of
  history), and deterministic-where-structural (amendment chains from the
  `amendment_events` table, never model-invented). Capabilities: guidance, draft,
  summarize-doc / compare / conflict / action-items (sequential map-reduce over the
  single model slot, full document coverage), amendment-chain. Memory: deterministic
  pinned-fact extraction (never summarized away), summarize-old-turns eviction, and a
  fresh-chat notice that carries facts forward. Per-turn audit (capability + budget plan
  + citation audit) now written into the query trace.
- Result: `cargo test --lib` 611/0; `cargo test --test bureaucrat_chat_acceptance` 16/0
  (SC-001 zero fabricated citations, SC-002 no truncated law, SC-003 zero window overflow,
  SC-004 memory fidelity, SC-005 first-answer completeness, SC-008 fill notice + carry,
  SC-009 whole-doc coverage, SC-010 amendment fidelity, SC-011 cited-candidate conflicts,
  FR-020 model-independent grounding, FR-021 survives restart). No live LLM in the gates —
  the guarantees are enforced deterministically, so they hold across model swaps. No new
  migration (reuses migration 004 + baseline tables). Per-phase detail in `CHANGELOG_DEV.md`
  (Phases 5–8, 2026-06-21).
- Next: live-LLM smoke over the controlled doc set on the GPU server (Spec 014) — measure
  retrieval precision feeding the allocator's "all relevant law, complete" guarantee, then
  decide whether an LLM-assisted summary should replace the deterministic `compact_turn`
  eviction extract.

### 2026-06-19 — setup — n/a — Modular approach kicked off
- Observed: full-corpus testing made it impossible to see which change helped.
- Changed: switched to a controlled 3-document set + this log + the Desktop tracker;
  confirmed direction = AI responses only (deterministic drafting abandoned, LLM stays).
- Result: plan PDF + tracker workbook created; PaddleOCR measurement spike for the lone
  scan (`04_Sindh_Local_Government_Act_2013.pdf`) running on the big disk (scans are last).
- Next: pick the 3 test documents, index doc #1 (simple English prose), capture its
  chunks/trace and the AI's response, log the baseline.

### 2026-06-19 — ingestion — Simple_Guide_to_Safe_Drinking_Water.pdf — plain-prose doc wrongly escalated to 7B "deep" path
- Observed: indexing the simple prose doc hung on "processing". Log: pdftotext extracted
  2326 chars / 2 pages instantly, then the indexer loaded Mistral-7B and crawled. DB:
  source `pending`, 0 chunks. Root cause (indexer.rs:688-797): the direct structured
  importer only succeeds when it detects structure (headings/rules/sections); plain prose
  has none → `structured=false` → indexer escalates to the SECONDARY Mistral-7B LLM IR
  path (slow on CPU) instead of simple size-based chunking. NOTE: chat + one-ask drafting
  BOTH load the same `models/ir_builder/` GGUF (one_ask.rs:55, drafting.rs:1119), so
  disabling/renaming the model is NOT an option — it would break AI drafting.
- Changed (FINAL design, replaces the env-flag idea): **simple indexing is now the default
  for all files; deep (7B LLM IR) is an explicit per-file action.** The LLM IR path in
  `indexer.rs` is gated on `index_tier == 'deep'` (sticky, FR-007). New Tauri command
  `folders::deep_index_folder_source` (registered in `lib.rs`) sets `index_tier='deep'` +
  pending, then re-indexes. Frontend: `deepIndexFolderSource` in `api/index.ts`,
  `handleDeepIndexSource` + a ⚡ button in `SettingsPage.tsx` beside the reindex (↻) and
  remove (✕) buttons (shown for indexed/unindexed/failed). The env flag was dropped.
- Result: pending rebuild (user does it). After: normal Index-Folder / reindex never loads
  the 7B; the ⚡ button on a specific file runs the deep 7B extraction on demand.
- Next: after rebuild, re-index the folder (fast path), capture doc #1 baseline
  (chunks/median/% tiny + a test AI answer).

### 2026-06-19 — ingestion — BASELINE captured for all 3 test docs (fast path) — chunks dumped
- Setup: all 3 docs indexed via the new fast/simple path (`index_tier=fast`, extraction
  pdftotext, log confirms "fast-tier — skipping LLM IR"). No 7B loaded. Chunks dumped to
  `~/Desktop/ingestion_test_folder/chunk_captures/` (TSV per doc + ALL_chunks_by_document.xlsx).
- Baseline metrics (chunks / median chars / tiny<60 / empty / unresolved-label / table_rows / junk):
  - **Doc 1 simple prose**: 3 / 415 / 1 / 0 / 0 / 0 / 0  → CLEAN (the 1 tiny = doc-title node, expected). PASSES.
  - **Doc 2 table**: 154 / 162 / 33 / **23 empty** / **151 unresolved** / 1 / 0  → BROKEN. 23 zero-length
    chunks; 151/154 carry bare unresolved sub-markers "(v)/(i)"; only 1 row reconstructed as table_row;
    columns interleaved ("(i) Administrative Department. Full powers"). The known table problem.
  - **Doc 3 digital law**: 177 / 153 / **59 tiny** / 0 / 6 / 0 / **18 junk**  → MOSTLY GOOD. Labels resolve
    ("Section 7/8") and content verbatim, but ~1/3 chunks are tiny (fragmentation) and 18 boilerplate
    chunks (footnotes / page markers like "6", "See footnote") got indexed; footnote text bleeds into
    a couple of provision chunks.
- Verdict: doc 1 clean. doc 3 = fragmentation + footnote/marker contamination. doc 2 = tables (hardest).
- Next (per plan order simple→table→law): doc 1 done; tackle doc 2 table reconstruction next
  (empty chunks + unresolved bare markers + column interleave), then doc 3 fragmentation/footnotes.

### 2026-06-19 — ingestion — Doc 3 (digital law / fundamental_rights.pdf) — full judge-grade per-chunk audit
- Did a precise per-chunk comparison of all 177 chunks against the source PDF (acting as a
  legal-structure judge). Output: `modular_diagnosis/02_baseline_chunks/LAW_DOC_JUDGE_AUDIT.md`
  + a colour-coded **Findings** column added to the "3. Digital law" sheet of
  `ALL_chunks_by_document.xlsx` for cross-verification.
- 12 defect classes (D1–D12). Code root causes pinned:
  - **D1 (every provision labelled "Section N", must be "Article N")** — doc classified `generic`;
    `doc_profiles/generic.rs:12,25,30` mints `citation_unit:"Section"` / `"Section {num}"`. ~150 chunks.
  - **D9 ("shall not"→"shallnot", altered quote)** — `text_normalizer.rs:33` lists `"shallnot"` as a
    JOIN_TARGET, so `repair_spaced_words` fuses the space. 6 chunks.
  - **D12 (page_number NULL for all provisions)** — `structure_parser::persist_tree`
    (structured_importer.rs:121) never sets `page_number`; only `persist_chrome_chunks`(:223) does.
    persist_tree isn't given page_offsets. (User-reported.)
  - D2/D3: 9 articles lost/mislabelled (9, 10A, 19A, 25, 25A, 33, 34, 35, 39). D4 catchline split
    (~33 orphan fragments). D5 footnotes captured as provision text. D6 page digits leak into content.
    D7/D8 clause + amendment-bracket boundaries. D10 unit_type. D11 doc-title=filename.
- Wrote a single self-contained briefing for external-AI review: `~/Desktop/1st_iteration_results.md`.
- Next: agree a foolproof fix plan (recommended order: D1 label-type → D5/D6 page+footnote strip →
  D2/D3/D4 article+chapter detection + catchline-merge → D9 → D12 page_number → D7/D8 boundaries).

### 2026-06-19 — planning — fix order LOCKED + hardened plan written (post external review)
- Ran the Iteration-1 briefing past two external reviews; reconciled their (conflicting) orders.
- LOCKED order: **P0 D9+D12 (trivial/orthogonal) → P1 D5/D6 geometry zoning → P2 D2/D3/D4/D10 tree
  → P3 D1 vocabulary → P4 D7/D8 boundaries.** Adopts "structure before label" but pulls the two
  isolated trivial fixes forward.
- Architectural spine = two-pass (structure emits identifier+NodeLevel, NO citation word; vocabulary
  resolved later with confidence → unresolved on conflict). Key realisation: codebase already
  separates `NodeLevel` (structure) from profile `citation_fmt` (vocabulary) + has `label_status=
  'unresolved'` — evolve, don't rewrite. New stage = geometry-based zoning (bbox) for page-numbers
  + footnotes. PaddleOCR confirmed irrelevant to D1–D12 (parser defects, not extraction); scan-only,
  deferred.
- Plan written: `~/Desktop/ingestion_hardening_plan.md` (+ copy in modular_diagnosis/). No code yet.
- Next: on user go — implement Phase 0 (D9 JOIN_TARGETS + D12 page_offsets), re-index, verify.

### 2026-06-19 — SDD — Feature 013-ingestion-hardening spec created + graph/Pass-2 finding
- Moved the hardening work into SDD. New branch `013-ingestion-hardening`; `specs/013-ingestion-hardening/`
  has spec.md (US1–US5, FR-001..FR-015, SC-001..SC-009) + checklists/requirements.md (green) + PHR 0001.
- US5 added per stakeholder: capture amendment + cross-reference relationships into the EXISTING graph
  store. Investigation result:
  - `graph_builder.rs` ALREADY builds `cross_reference` edges (unit→"Article N", :91/:121) deterministically
    → US5 cross-refs largely exist.
  - `amended_by` intentionally NOT built (graph_builder.rs:10-14; migration 014 = 6 hierarchy levels only)
    → US5 amendments need a v2 schema (additive migration); provenance now on `domain_metadata.footnote_refs`.
  - LLM Pass-2 NER is HARDCODED OFF (`indexer.rs:1291`, since 2026-05-25, to free CPU). **Decision (ADR
    candidate): US5 populates the graph DETERMINISTICALLY; do NOT re-enable the LLM Pass-2 (keeps FR-013).**
- NEXT = `/sp.plan` → clarify/tasks/implement. Locked phase order unchanged (P0..P4). All code edits this
  session remain UNCOMMITTED + unbuilt on branch 013.

### 2026-06-19 — ingestion — 04_Sindh_Local_Government_Act_2013.pdf (scan) — PaddleOCR vs tesseract spike
- Observed: tesseract (app config: 300dpi, psm1, eng) INVENTS numbers on this 150ppi
  scan — read the date "Nov. 02, 2013" as `2613` (p3) and `7013` (p7). Same mechanism =
  the corpus `Section 20413` false citation. PaddleOCR 3.7.0 read those lines correctly
  ("Nov. 02, 2013"); every 3+ digit run in Paddle output is real; only artifact was
  `(040)` for page no. "(40)" (harmless chrome). Word/char counts otherwise comparable;
  Paddle also recovered some lines tesseract collapsed (e.g. spacing). Both still make
  letter-level slips (tess "Gasette"/"2"°"; paddle "Gobernment"/"2N0").
- Changed: nothing yet (measurement spike only). PaddleOCR installed in venv on big disk
  (`/media/riaz/New Volume/paddle_spike/`), MKLDNN disabled to dodge the paddle 3.x
  oneDNN crash. Cost ~14 min / 8 pages on CPU (~1.7 min/page).
- Result: VERDICT = PaddleOCR materially fixes the numeric-garble / false-citation root
  cause on scans; tesseract does not. Worth integrating for SCANNED docs specifically.
- Next: scans are LAST in the staged plan, so integration is deferred — evidence banked.
  Resume here when we reach the scan stage (decide ONNX/Rust integration vs python CLI
  sidecar vs digital-copy substitution for this one Act).

### 2026-06-19 — ingestion — fundamental_rights.pdf — P0+P1 shipped + live-verified (iteration 2)
- Observed: rebuilt app, forced re-index, queried live `echo` Postgres. Block classes now
  147 provision / 63 chrome / 10 footnote. Scorecard: D9 shallnot=0 ✅; D12 page_number
  146/147 filled (range 1–12, 1 NULL=root) ✅; D6 standalone page-number provisions=0 ✅;
  D5 footnotes isolated=10 ✅ but 7 provisions still leak footnote text ⚠️.
- Changed: P0 = remove "shallnot" from JOIN_TARGETS + guard test (text_normalizer.rs);
  thread page offsets → page_number via Approach B (build_tree returns unit_path→offset map;
  strip_chrome returns chrome-aligned cleaned_page_offsets; persist_tree_with_pages +
  collect_chunks inherit-or-NULL). P1 = chrome.rs BlockClass::Footnote + ChromeReason::PageNumber,
  is_footnote_apparatus (amendment vocab) + page_number_blocks (bare-int run ≥2 pages),
  strip_chrome separates footnotes; persist_footnote_chunks (block_class='footnote', kept
  retrievable, label_status='unresolved'). Approach B (text patterns) chosen because the fast
  path carries NO bbox geometry. EDIT-ONLY workstream; single build at end (29 min, exit 0).
- Result: chunks dumped to `modular_diagnosis/04_iteration2_chunks/` (TSV ×3 with new page +
  block_class cols) + `ITERATION2_RESULTS.md`. Profile = **manual** (P1 did NOT flip it).
- Next (P2): D2 (9/33/34/35/39 lost — sit under Part II as clauses; cause OPEN, sequence filter
  ruled out) → arabic CHAPTER (D3) → `\d+[A-Z]?` suffix for 10A/19A/25A (D3) → catchline/body
  merge (D4). Then revisit the 7 footnote leaks (P1 refinement).

### 2026-06-20 — ingestion — fundamental_rights.pdf — P2 structural tree (Phase 4, D2/D3/D4/D10) shipped + lib-tested
- Observed (D2 root cause PINNED — was OPEN): the "lost" Articles 9/33/34/35/39 were NOT dropped
  by the sequence filter; they were present in the baseline as `clause` chunks parented to
  `Part II`, `label_status=unresolved`, content `N. Title ¶ N. body`. Cause = **catchline↔body
  double-match collision**. Each provision prints its number twice — catchline (`9. Security of
  person`) AND a plain-prose body (`9. No person shall…`). Both line-start `N. <Capital>` forms
  match the manual bare-number pattern, yielding TWO `Section 9` headings → two nodes with an
  identical `unit_path` that collide at tree-build → the provision vanishes. Articles whose body
  opens with a subsection `(1)` (8, 10, …) match ONCE (the `(` isn't a letter) and survived. This
  exactly predicts the lost set {9,33,34,35,39} = the plain-prose-body articles.
- Changed (EDIT-ONLY, deterministic, no LLM):
  - **T019** `doc_profiles/manual.rs` — article patterns `(?P<num>\d+)` → `(?P<num>\d+[A-Z]?)`,
    recovering alpha-suffixed ordinals 10A/19A/25A that a bare `\d+` drops at the `.` boundary.
  - **T021** `doc_profiles/manual.rs` — CHAPTER pattern + detection accept arabic (`CHAPTER 1.`/
    `CHAPTER 2.`) as well as roman (`CHAPTER II`); real Chapter nodes now inserted (D3).
  - **T022** `heading_detector/regex_pass.rs::scan_with_profile` — collapse consecutive
    same-label headings (`hits.dedup_by(|a,b| a.label==b.label)`, after the offset-dedup) to the
    EARLIEST offset, so catchline+body become ONE Article node spanning both. Recovers
    9/33/34/35/39. Safe: distinct provisions never share a label AND sit adjacent.
  - **T023** falls out for free — those become `NodeLevel::Article` → `unit_type='article'`,
    resolved label (were `clause`/unresolved only because no Article node existed).
- Result: `cargo test --lib regex_pass` = 18/18 (2 new: `catchline_and_prose_body_collapse_to_one_article`,
  `manual_profile_captures_alpha_suffix_articles_and_arabic_chapters`). Full `cargo test --lib` =
  **496 passed / 2 failed**; the 2 failures are the KNOWN Phase-3 footnote-leak tests in
  `chrome.rs` (`footnote_apparatus_is_classified_footnote`, `footnotes_are_isolated_from_body_but_kept`),
  pre-existing red (the "7 provisions still leak footnote text ⚠️" gap) — untouched by this change.
- Deferred in P2: **T024** (25/25A out-of-order ⇒ flag-don't-reorder). After this fix both 25 and
  25A exist as Article nodes (were both missing); the residual is only the *order-ambiguity flag*,
  which needs a `label_status` thread from detection → DocNode → persist (more invasive). Refinement,
  not a correctness gap.
- NOT yet done: build the app + force re-index the control set to live-verify the tree (no running
  app this session), and the acceptance-harness tasks T001–T003/T016–T018. Code edits remain
  UNCOMMITTED on branch 013.
- Next: live re-index verify (all of Art 7–40 incl. 10A/19A/25A as `article` nodes; Chapter 1/2
  present; 9/33/34/35/39 no longer `clause` under Part II) → then P1 footnote-leak refinement (turn
  the 2 red chrome tests green) → P3 vocabulary (D1 Section→Article).

### 2026-06-20 — ingestion — fundamental_rights.pdf — P1 footnote-leak fix + P3 vocabulary resolver (D1)
- Observed: 2 red Phase-3 chrome tests (`footnote_apparatus_is_classified_footnote`,
  `footnotes_are_isolated_from_body_but_kept`). Root cause = ONE regex bug in
  `chrome.rs::is_footnote_apparatus`: the verb group `\b(subs\.|ins\.|…)\b` puts a `\b` AFTER the
  literal `.`, but a word boundary can't fall between `.` and a following space (both non-word) — so
  `"Subs. by the Constitution (Eighteenth Amdt.) Act…"` never matched; only lines carrying `w.e.f`
  slipped through the `contains` fallback. (`line_is_provision` was fine — it returns false for the
  footnote, so the keep-invariant wasn't the cause.)
- Changed (footnote): rewrote the verb alternation to `\b(?:subs|ins|ord|added|inserted|…)\b\.?` —
  match the bare stem with `\b…\b`, then an OPTIONAL trailing dot. Guard
  `ordinary_prose_saying_substituted_is_not_a_footnote` still passes (needs verb AND
  instrument/effect token). `cargo test --lib parsers::chrome` = 22/22.
- Changed (P3 / D1 — vocabulary resolver, the live `manual`→"Section N" leak):
  - `doc_profiles/mod.rs`: `CitationUnit{Word|Unresolved}` + `resolve_citation_unit(text, profile)`.
    Layering = profile default → internal self-reference evidence (`"<Unit> 12"` / `"this <Unit>"`,
    `UNIT_MIN_EVIDENCE=3`) → override only on ≥2× dominance over the runner-up; genuine mix ⇒
    `Unresolved`. So fundamental_rights (`manual` default "Section", body says "Article 251 / this
    Article / New Article 19A") resolves to **Article**.
  - `heading_detector/regex_pass.rs`: `scan_with_profile_unit(text, profile, unit_override)`
    (`scan_with_profile` is now a no-override wrapper). When `Some(word)`, Article/Clause labels
    whose leading word is the profile default are re-minted (`swap_unit_word`: "Section 9" →
    "Article 9"); Chapter/Part/Schedule labels untouched.
  - `structured_importer.rs`: compute the unit after `detect_profile`; pass the override into
    `scan_with_profile_unit`; `Unresolved` ⇒ keep default + `log::warn` (provisions surfaced for
    review). T027 (generic.rs "Section" leak) is now handled generally by the resolver for any
    profile, not just generic.
- Result: `cargo test --lib` = **502 passed / 0 failed** (was 496/2). New tests: resolver ×3,
  unit-override ×1, footnote ×0-new (2 pre-existing now green).
- Deferred: T029 conflict branch — when `Unresolved`, persist-flag the affected provision chunks
  `label_status='unresolved'` (currently only logged). T024 (25/25A order flag). Both are
  small follow-ups; neither is a correctness regression.
- NOT yet done (cross-cutting): build app + force-pending re-index of the 3-doc control set to
  live-verify D1 ("Article N" everywhere, no "Section N") + the P2 tree + footnote isolation. Code
  remains UNCOMMITTED on branch 013.
- Next: live re-index verify → Phase 6 (US4 clause/amendment-bracket boundaries) → Phase 7 (US5
  amendment + cross-reference capture) → Phase 8 (acceptance harness T001–T003 + SC-001…SC-009 gate).

### 2026-06-20 — ingestion — fundamental_rights.pdf — P4 clause & amendment-bracket boundaries (D7/D8)
- Observed (from the per-chunk judge audit): D7 — sub-clauses merge/swallow, e.g. Art 8 chunk
  "(a) … ; or" absorbs the lone footnote digit `2` AND the start of `[(b) any of the —` because the
  blank line falls AFTER `[(b)`, so the `\n\n` paragraph splitter groups `(a)`+start-of-`(b)`. D8 —
  amendment insertions `1[three months]`, `2\n[(b) …`, `1[Part II of the First Schedule]`: the
  inserted sub-clause opens with `[`, which the splitter never treated as a boundary.
- Root cause: `split_long_text` (structure_parser.rs) only split on blank lines / sentences / chars —
  it had no notion of legal sub-clause markers, and legal sub-clauses are frequently NOT
  `\n\n`-separated.
- Changed: added `split_at_clause_markers(text)` and made it step 0 in `split_long_text` (before the
  blank-line fallback). It splits at line-start `(2)(3)`, `(a)(b)`, `(i)(ii)` markers via
  `(?m)^[ \t]*\[?\((?:[0-9]{1,3}|[a-z]|[ivxlc]{1,4})\)`. The optional `\[?` makes an
  amendment-bracketed sub-clause (`[(b) …`) open its OWN piece instead of being swallowed (D7+D8
  together). Lead-in before the first marker (catchline + the article-line `(1)`) becomes the first
  piece. Pieces are contiguous slices ⇒ brackets + inserted text + footnote-ref digit kept VERBATIM
  (G1 holds); a mid-line "clause (3)" reference never matches (line-start anchored); a 4-digit year
  in parens never matches (`[0-9]{1,3}`).
- Result: `cargo test --lib` = **504 passed / 0 failed** (+2: `clause_marker_split_keeps_subclauses_separate`,
  `amendment_bracket_preserved_verbatim_through_split`). Existing split/coalesce tests unaffected
  (their fixtures carry no line-start markers ⇒ fall through to the old paths).
- Deferred (T033 sub-step): physically lifting the footnote-ref digit out of content into metadata —
  removing it from the body would VIOLATE the verbatim-substring invariant, so it needs a dedicated
  `footnote_refs` capture path (Phase 7 territory). Left verbatim for now (minor noise, never wrong).
  Note: marker pieces below the coalesce `min` (120c) can still be re-merged by
  `coalesce_undersized_leaves` — acceptable (tiny adjacent scraps SHOULD merge; the harmful case is a
  *substantial* (a) swallowing a distinct (b), which the splitter now prevents).
- NOT yet done: build + force-pending re-index to live-verify clause counts on the real doc.
- Next: Phase 7 (US5) — emit `cross_reference` edges (graph_builder already builds unit→"Article N";
  extend) + parse amendment events from the now-isolated footnotes → `amendment_events` +
  `amended_by` edges (needs the additive migration if a query column is required).

### 2026-06-20 — ingestion — fundamental_rights.pdf — P? US5 amendment + cross-reference capture (FR-014/15)
- Observed: cross-reference edges already existed (`graph_builder::collect_edges` emits unit→"Article
  N" `cross_reference` + `non_obstante`, self-ref skipped, non-existent target never invented). Gap =
  amendment events: the Phase-3-isolated footnotes ("Subs. by the Constitution (Eighteenth Amdt.)
  Act, 2010, s. 4.") were stored but never parsed into the `amendment_events` table.
- Changed (deterministic, NO model — keeps FR-013):
  - `legal_ir/amendment_parser.rs` (was an empty stub): `parse_amendment(footnote) -> AmendmentEvent`
    — `classify_verb` (Subs/Ins/Added/Omitted/Repealed → kind), `extract_instrument` (verbatim text
    after "<verb> by", cut before the `, for "…"` replaced-text clause / `w.e.f` parenthetical),
    `extract_effective_date` (`w.e.f` date). Returns None for non-amendment footnotes (cross-ref
    "See footnote…", plain prose, verb-without-instrument).
  - `structured_importer.rs`: `persist_amendment_events` (after graph insert) parses each footnote,
    anchors via `anchor_citation_for_page` (the SINGLE distinct Article on the footnote's page;
    ambiguous/absent → skip — footnote already flagged `unresolved`), inserts an `amendment_events`
    row (`node_id`, kind, verbatim `by_instrument`, `effective_date`, `source_page`). No `amended_by`
    edge: the instrument is not a `graph_nodes` row (FK), so capture lives in `amendment_events`
    (consistent with the existing graph_builder decision). Never invents a link.
- Result: `cargo test --lib` = **510 passed / 0 failed** (+6: amendment_parser ×5, anchor ×1).
- Conservative-anchor note: one-Article-per-page is SAFE but sparse on a multi-article-per-page doc
  like fundamental_rights (most footnotes won't anchor → skipped/unresolved, never wrong). Richer
  anchoring via the in-body footnote-marker position is the follow-up. `amendment_events` table +
  `graph_edges('amended_by')` already in `001_baseline.sql` ⇒ T039 migration NOT needed.
- NOT yet done: build + force-pending re-index to live-verify `amendment_events` rows + cross_reference
  edges on the real doc. Code UNCOMMITTED on branch 013.
- Phases 2–7 (US1–US5) now implemented + unit-tested. Remaining: Phase 8 — acceptance harness
  (T001–T003) + SC-001…SC-009 gate (T040–T043) + the cumulative live re-index/verify.

### 2026-06-20 — ingestion — Phase 8 acceptance harness (SC-001…SC-009) — END-TO-END GREEN + chapter-indent fix
- Built `src-tauri/tests/ingestion_hardening_acceptance.rs` (T001–T003, T040): runs the REAL
  deterministic `structured_importer::import` + `persist` on the frozen `fundamental_rights` layout
  fixture (`tests/fixtures/ingestion/fundamental_rights_layout.txt`) — the first true end-to-end
  check (no model). 5 tests: SC-001 all Art 7–40 (incl 10A/19A/25A) present as `article` nodes +
  Chapter 1/2 parentage; SC-002 zero "Section N" leak; SC-004/006 every chunk a whitespace-normalized
  verbatim substring of the cleaned source; SC-005 every article chunk page_number ∈ [1,page_count];
  SC-009 graph nodes populated + no dangling edges + amendment capture executed.
- BUG the harness caught (unit tests had missed): chapters came back `[]`. Cause = the real
  `CHAPTER 1.`/`CHAPTER 2.` headings are CENTERED (leading whitespace), but the manual CHAPTER regex
  was anchored `^CHAPTER` with no `[ \t]*` (the article patterns already had `^\s*`). My Phase-4
  unit test used an un-indented line, so it passed while the live doc failed — exactly why the
  end-to-end gate matters. Fix: `^[ \t]*CHAPTER` in the heading + detection patterns
  (`doc_profiles/manual.rs`); regression-locked by indenting the CHAPTER lines in the regex_pass test.
- Result: `cargo test --test ingestion_hardening_acceptance` = **5/5**; full `cargo test --lib`
  re-confirmed (chapter fix, no regressions).
- Verified END-TO-END on the real doc: D1 (Article not Section) ✅, D2/D3 (all articles + chapters) ✅,
  D4 (catchline merge) ✅, D7/D8 (clause/bracket via verbatim) ✅, D12 (page_number) ✅, US5 graph ✅.
- Deferred / still manual: T042 live force-pending re-index of the 3-doc control folder (needs the
  running app/DB) + tracker xlsx numbers; T041a `index_tier='fast'` log/DB-flag assertion (indexer
  scope); the earlier per-phase deferrals (T024 25/25A flag, T029 unresolved-vocab persist-flag,
  T033 footnote-digit→metadata, richer amendment anchoring).
- Feature 013 Phases 2–8 are CODE-COMPLETE + automated-gate-green on branch 013 (UNCOMMITTED).
  Next on user ask: live re-index verify → commit → then crate-slimming backlog (llguidance etc.).

### 2026-06-20 — ingestion — fundamental_rights.pdf — ITERATION 3 (live re-indexed DB) — defects fixed end-to-end
- Committed Feature 013 (`09c54a96` feat(013); `ce8e370b` sentence-Jaccard gate). User force-pending
  re-indexed the live `echo` DB (port 42339, source 950a9a1b…). Queried it directly + dumped chunks.
- Scorecard (baseline iter-1 → iteration 3):
  - Total chunks 177 → 209.
  - **Articles present 28/37 (9 missing) → 37/37 (0 missing)** — ALL of 9/10A/19A/25/25A/33/34/35/39
    recovered as `article` nodes. ✅
  - **Chapter nodes 0 → 2** (Chapter 1, Chapter 2). ✅
  - **D1 "Section N" leak ~150 → 0**; 119 chunks now `Article N`. ✅
  - **D12 article page_number NULL: all → 0** (pages 1–12). ✅
  - block_class provision 123 / chrome 63 / footnote 23; unit_type article 37 / clause 82 / leaf 86
    / chapter 2 / part 1 / document 1.
  - Tiny provision (<60c) ~59 → 23 (improved); unresolved labels 151 → 23 (= the 23 footnotes, by design).
  - US5: 1 `cross_reference` edge; `amendment_events` = 0 (conservative one-Article-per-page anchor —
    EXPECTED on a multi-article-per-page doc; footnotes are isolated + parseable, only the anchor is
    withheld; never a wrong link).
- Artifacts: `~/Desktop/modular_diagnosis/05_iteration3_chunks/` — `03_digital_law_chunks_iter3.tsv`
  (209 chunks), `iteration3_fundamental_rights.xlsx` (Scorecard + colour-coded Chunks sheets),
  `ITERATION3_RESULTS.md`.
- Verdict: every targeted defect (D1/D2/D3/D4/D5/D6/D12 + US5 capture) verified FIXED on the real
  re-indexed document. Residuals (all known, none wrong): amendment anchoring (events=0), 23 tiny
  fragments. Feature 013 done + committed + live-verified.

### 2026-06-20 — ingestion — ITERATION 6 — 7-doc "full test digital law" corpus — per-chunk vs real-source audit
- Live DB (port 35001) re-indexed 7 docs: 01 Rules of Business, 02 SPPRA Rules, 03 Procurement Act,
  05 Compendium SLGA, 06 Manual for Development Projects (PC-I), constitution, Secretariat Manual.
- RIGOROUS METHOD (not metrics-guessing, per user): `pdftotext -layout` all 7 PDFs, then compared
  ALL 9,063 chunks vs their real source — verbatim alnum-substring, article label-num=content-num,
  per-doc vocabulary word-dominance.
- GOOD (verified): **0 article label≠content mismatches across all 7 docs**; vocabulary correct on
  6/7 (Rule/Section/Article match source dominance); prose verbatim 94.5–97.6% (residual = page-
  spanning clauses with retained `\x0c`, not corruption); D12 page_number on every provision (0 NULL).
- PRIMARY DEFECT pinned — **D-TOC: Table-of-Contents entries → DUPLICATE provisions.** Front-matter
  TOC lines `N. Title ……… page` match the bare-number heading pattern → a 2nd `Article/Section N`.
  constitution: 163 short TOC stubs, **297 DUPLICATED article numbers**, junk `Article 2454/773`
  (717 art nodes vs ~280 real). Act 31, Compendium 29, doc-06 13. Root: TOC pages not stripped as
  chrome; dotted-leader line satisfies `^\s*(\d+)\.\s+[A-Z]`. Fix = detect dotted-leader/contiguous-
  TOC block as chrome, exclude from heading scan.
- SECONDARY — **doc-06 vocab+roster.** Labeled "Article" though source uses "Article N" only 5x
  (cross-refs to OTHER instruments) vs Section1/Rule1 — resolver 2x-override fired on weak evidence
  (manual default "Section" was safer). Plus committee ROSTERS / PC-I form tables (nums 1..46
  repeated) detected as Articles. Fix = self-vs-cross-ref evidence + roster/form suppression.
- Table-heavy verbatim lower (02=59%, 05=83%, 06=56%) = reconstructed table rows (by design), not
  corruption.
- Artifacts: `~/Desktop/modular_diagnosis/06_iteration6_chunks/` — `JUDGE_AUDIT_iter6.md`,
  `all_chunks_iter6.tsv` (9063), `chunk_defects_iter6.tsv`, `sources/*.txt`,
  `iteration6_full_test_digital_law.xlsx` (Audit-vs-source + Findings + Defect-samples + All-chunks).
- NEXT fix priority: (1) D-TOC strip (corpus-wide, cleanest win) → re-index → duplicates vanish;
  (2) doc-06 resolver self-vs-cross-ref + roster/form suppression; (3) coalesce tiny + strip `\x0c`.

### 2026-06-20 — ingestion — FRONT-MATTER ZONING implemented (D-TOC fix) — code-complete, lib-green
- Fixes the iteration-6 PRIMARY defect (Table-of-Contents entries → duplicate provisions). Front-matter
  sections (TOC / preface / foreword / abbreviations / drafting committee / list of annexes) are now
  lifted OUT of the body BEFORE heading detection and KEPT as their own retrievable chunks.
- New `services/parsers/front_matter.rs::strip_front_matter` (mirrors `strip_chrome`): detects
  (a) dotted-leader TOC runs (≥3 lines, `\.{4,}|…{2,}`), (b) named section headings; keeps only the
  FRONT CLUSTER — chained from the first span, STOPS at the first big gap (= body started) — so a
  mid-document form fill-blank is never mistaken for front-matter. Emits one `FrontMatterBlock` per
  section. Safety rails: first span must start within the top 20%; section cap 60 lines; chain gap 50.
- Wired in `structured_importer::import` (after `strip_chrome`, before profile/heading/tree) +
  `ImportOutcome.front_matter_blocks` + `persist_front_matter_chunks` (block_class='front_matter',
  unit_type='front_matter', section_kind in domain_metadata, KEPT retrievable — not boilerplate).
- Universal dotted-leader suppression added in `regex_pass::scan_with_profile_unit`
  (`heading_line_is_dotted`): drops any heading on a dotted line that survives in the body (stray
  mid-doc form-blanks). Structural boundaries are never dotted → unaffected.
- Validated against all 7 real sources (Python prototype mirroring the Rust algo): constitution
  region [14-599] (preface+TOC, ~505 lines removed), doc-06 [17-295] (TOC+abbrev+committee roster),
  Act [32-147], Compendium [35-95] (mid-body form-blanks at 493+ correctly EXCLUDED via chain-break);
  01/02/Secretariat → NO front-matter (no over-detection). 
- Gate: `cargo test --lib` green (front_matter 4/4 + no regressions). Edit found one real edge — a lone
  `…` must NOT count as a leader → tightened to `…{2,}`.
- NEXT: rebuild app + force-pending re-index → verify duplicate articles vanish (constitution 297→~0)
  and front_matter chunks appear; then doc-06 vocab self-vs-cross-ref + roster suppression.

### 2026-06-20 — spike — table extraction (digital PDFs) — pdfplumber vs camelot vs PyMuPDF vs pdftotext
- Goal: decide whether to replace the Rust projection-profile table engine (pdftotext -bbox) for
  DIGITAL tables. Measured on 4 docs in `~/Desktop/table_ingestion_test/` (02 SPPRA, 05 Compendium,
  06 PC-I, 09 Delegation of Financial Powers). venv on big disk `/media/riaz/New Volume/table_spike_venv`
  (pdfplumber 0.11.10, camelot 2.0.0, pymupdf 1.27.2).
- Counts (tables/rows; pymupdf+pdfplumber all pages, camelot 4 sample pages):
  - 02 SPPRA: **0/0 by ALL tools** → SPPRA has NO machine-detectable tables; its low verbatim % is
    amendment-bracket/list formatting, NOT a table problem.
  - 09 Delegation: pymupdf **76/927** == pdfplumber **76/927** (identical); camelot-lattice 4/33,
    stream 5/152.
  - 05 Compendium: pymupdf 39/155, pdfplumber 54/173; 06 PC-I: pymupdf 95/524, pdfplumber 100/565.
- Cell quality (the decisive part), Delegation p8 + Compendium p70 side-by-side:
  - pdftotext -layout (current) = unstructured WRAPPED TEXT (multi-line "To whom delegated" cell flows
    down many lines, blank Sr.# col) — exactly the column-interleave the projection engine fights.
  - **PyMuPDF == pdfplumber = byte-identical + structurally CORRECT** (every cell in the right column,
    4-col delegation / 7-col election form).
  - camelot-lattice ≈ correct but slower (Ghostscript) + fired on fewer pages; camelot-stream WORST
    (over-segments wrapped lines into rows).
- VERDICT: for digital ruled/grid tables an external extractor is a large win; **pdfplumber (MIT)
  chosen** — equals PyMuPDF quality, clean license (PyMuPDF=AGPL). camelot not needed.
- Artifacts: `~/Desktop/table_ingestion_test/spike_results/` — SUMMARY.md, SPIKE_VERDICT.md,
  `<doc>__p<N>.md` per-page side-by-sides.
- Decision recorded as a DEFERRED extra phase: `specs/013-ingestion-hardening/phase9-extraction-sidecars.plan.md`
  (Phase 9a = pdfplumber table sidecar, 9b = PaddleOCR scan sidecar — both indexing-time Python sidecars,
  deterministic/no-LLM, optional, behind the existing fallbacks). NO CODE yet — implement after the
  front-matter re-index (iteration 7) is verified + doc-06 vocab/roster fixed.

### 2026-06-20 — ingestion — ITERATION 7 — front-matter fix LIVE-VERIFIED (after rebuild + reindex)
- All 7 docs re-indexed clean (after the `unit_type='leaf'` constraint fix). front_matter chunks
  stored: constitution 22, 06 PC-I 20 (toc+abbreviations+committee+foreword+annex_list — correctly
  kinded), Act 3, Compendium 1; 01/02/Secretariat 0 (no front-matter — correct, no over-detection).
- Constitution iter6 → iter7: article nodes 717→**434**; duplicated article numbers 297→**68**;
  **TOC dotted stubs (<120c) 163→0** ✅; tiny(<60) 293→**217**; chunks 3573→3363.
- Front-matter zoning DELIVERED its scope: TOC-as-provision eliminated; each front section kept as a
  retrievable `block_class='front_matter'` chunk for summarisation / doc comparison.
- Residual 68 duplicates TRACED (not guessed) = **BACK-MATTER**, out of this phase's scope:
  the First/Second Schedule's numbered list of laws (`Article 4` = "4. The Life Insurance
  (Nationalization) Order, 1972"; `Article 23` = "23. The Co-operative Banks Act") on pages 157-177,
  promoted to false Articles (same class as doc-06 roster / delegation-table serials). Plus bare-number
  footnote/reference artifacts (`Article 2454` = "2454. For the Senate (Finance Committee) Rules…",
  `Article 773` = "773.").
- NEXT defects pinned: (1) back-matter zoning (schedules/annex numbered-law lists → suppress like
  front-matter); (2) bare-number junk guard (2454/773 — reference/footnote numbers); (3) doc-06 vocab
  self-vs-cross-ref + roster suppression. Then Phase 9a pdfplumber table sidecar.

### 2026-06-20 — ingestion — BACK-MATTER ZONING implemented (Schedule/Annex/Appendix tail) — code-complete, lib-green
- Fixes iteration-7 residual #1: the constitution's First/Second Schedule "list of laws" promoted to
  false Articles (`Article 4` = "4. The Life Insurance (Nationalization) Order, 1972"; the 68 dups).
- New `services/parsers/back_matter.rs::strip_back_matter` (symmetric to strip_front_matter): detects
  standalone ALL-CAPS SCHEDULE/ANNEX(URE)/APPENDIX headings (centered, TOLERATES a leading `[` —
  the constitution's `[FIRST SCHEDULE`/`[ANNEX` that the old boundary detector missed; case-sensitive
  uppercase so mid-sentence "…Second Schedule…" never matches; dotted TOC entry excluded). Region =
  the CONTIGUOUS TAIL cluster: chain backward from the last boundary across schedule-content gaps
  (<15% of doc), stop when the body resumes; the cluster START must be in the last 20% — else it's a
  mid-body schedule, left alone. Each section kept as a retrievable `block_class='back_matter'` chunk
  (unit_type='leaf', section_kind in domain_metadata) — schedules are operative legal content.
- Wired into `structured_importer::import` (after strip_front_matter) + ImportOutcome.back_matter_blocks
  + persist_back_matter_chunks.
- Validated on all 7 sources (prototype mirroring the Rust algo): constitution [84%-EOF] = ANNEX + 5
  Schedules (the false-article region); Compendium [97%-EOF] = 2 tail schedules (mid-body 43-60%
  EXCLUDED); 01 [99%-EOF] tiny tail; 06/Secretariat/02/03 = none (mid-body schedules untouched).
- Gate: `cargo test --lib` = 518 passed / 0 failed (+4 back_matter tests).
- NEXT: rebuild + reindex → verify constitution duplicate articles 68→~0 + back_matter chunks appear;
  then bare-number guard (2454/773) + doc-06 vocab/roster.

### 2026-06-20 — ingestion — constitution residuals: roman-numeral front-matter + reference-footnotes + bare-number guard
- Fixes iteration-8 residuals on constitution.pdf. Opened the real PDF to confirm (DB pages were
  mis-attributed): the 16 "duplicate articles" = TOC entries on the front CONTENTS pages (physical
  p3-9, footers roman i..ix); "Article 2454"/"773" = FOOTNOTE NUMBERS (constitution numbers footnotes
  into the thousands; #2454 on physical p61 bottom: "For the Senate (Finance Committee) Rules, 1973,
  see ibid., pp. 2479-2482").
- THREE fixes (user-directed: run BOTH TOC logics; resolve footnotes via footnote logic):
  1. **Roman-numeral front-matter** (`front_matter.rs::roman_front_matter_end`): the front pages are
     paginated i, ii, … (footer); region = [0, after the last page-spaced roman footer]. Run as a
     UNION with the existing dotted/named cluster (refactored into `front_cluster`). Removes the WHOLE
     TOC incl. the wrapped-dot survivors the dotted detector misses. No-op for non-roman docs.
     Validated on the real source: region [0,594) covers TOC survivors at lines 125/464.
  2. **Reference-footnote detection** (`chrome.rs::is_footnote_apparatus`): also classify editorial
     footnotes (`ibid`/`supra`/`op. cit.`/`see footnote`) — not just amendments — so their text is
     isolated. A provision cross-ref ("see Article 199") is NOT matched.
  3. **Bare-number forward-jump guard** (`regex_pass.rs::drop_out_of_sequence_articles`): drop an
     Article whose number leaps far ABOVE the established run (>2× AND >300 jump, run ≥20) — a
     footnote/reference number (773/2454), never the next provision (281 after 280 is safe).
- Gate: `cargo test --lib` = 521 passed / 0 failed (+3: roman, reference-footnote, far-forward jump).
- NEXT: rebuild + reindex constitution → verify the 16 TOC dups + Article 2454/773 gone; then doc-06
  vocab/roster + empty-stub fragmentation.

### 2026-06-20 — ingestion — ITERATION 9 — constitution residuals RESOLVED (live-verified)
- Reindexed constitution after the roman/reference-footnote/bare-number fixes.
- Duplicated article numbers 16→**0** ✅; bare-number junk (Article 2454/773) **GONE** ✅; max article
  number now **280** (real last article; no junk above); distinct articles 299 (all real). front_matter
  3 (preface + 2 toc), back_matter 6 (annex + 5 schedules).
- The 188 "empty" article nodes (len<22) are NOT lost content — they are CONTAINER nodes with content
  in child clause chunks (0 orphans; e.g. Article 175A = 21 children / 5774 chars). Correct hierarchy.
  The earlier "empty-stub" concern was the DUPLICATE stubs, now eliminated.
- Constitution is clean: every defect found (TOC-as-Article, Schedule-as-Article, footnote-number-as
  -Article, vocab, page_number, footnotes) resolved. Commits: 7d5173b4 (+ aada2214, 94df9c81 …).
- REMAINING corpus item: doc-06 (06_Manual_for_Development_Projects) vocab "Article" misfire + mid-body
  committee rosters (pages 12,22). Then Phase 9a pdfplumber table sidecar.

### 2026-06-20 — ingestion — doc-06 vocab misfire + committee-roster suppression
- Fixes the last deterministic-parser defect (06_Manual_for_Development_Projects). Verified against the
  real PDF: the rosters are on physical p14 (DB p12/22 were mis-attributed — page-accuracy bug noted).
- VOCAB (`doc_profiles/mod.rs::resolve_citation_unit`): split evidence into self-reference
  ("this <Unit>") vs bare ("<Unit> N", may be a CROSS-ref). Override the profile default only with a
  genuine self-reference + dominance, OR overwhelming bare evidence. doc-06 has 0 "this Article" + 5
  bare "Article N" (cross-refs to other instruments) → KEEPS the manual default "Section" instead of
  flipping to "Article". (constitution/fundamental_rights still resolve Article via "this Article".)
- ROSTER (`regex_pass.rs::heading_line_is_roster`/`is_roster_line`): drop Article-level bare-number
  hits whose line is a membership-table row — a numbered person/post with a parenthesised/right-aligned
  role ("(Chairman)"/"(Member)") or "ex-officio". A provision title ("5. Powers of the Chairman") has
  the role inline → never matched. Validated on the real p14 rosters + guards.
- BUG FOUND+FIXED en route: `line_at` returned the wrong line for headings whose `^\s*` match started
  on a preceding BLANK line (\s includes \n) — it returned the blank line, so dotted/roster suppression
  silently no-op'd. Now advances to the first non-whitespace char. (Also strengthens the dotted-leader
  suppression.)
- Gate: `cargo test --lib` = 525 passed / 0 failed (+5: vocab cross-ref, roster suppress, roster direct,
  provision-title guard, line_at).
- NEXT: rebuild + reindex doc-06 → verify it labels "Section" (not Article) and the roster rows are gone.
  Then Phase 9a pdfplumber table sidecar. (Page-attribution accuracy is a separate follow-up.)

### 2026-06-20 — ingestion — ITERATION 10 — doc-06 vocab + roster (live-verified)
- Reindexed doc-06 after the vocab + roster fixes.
- VOCAB FIXED ✅: label word now **Section 1072** (zero "Article"; was Article 1092). The cross-ref
  misfire is resolved — the manual default "Section" is kept.
- ROSTERS mostly suppressed: article nodes 169→156. Of 4 "roster-ish" matches, 3 (Section 40/2/5) are
  REAL provisions that merely mention "chairman" in body (correctly kept); 1 survivor = Section 7, a
  3-COLUMN committee table ("7. Chief Economist P&D Division | Member | …") where the role is a MIDDLE
  column, not line-end — missed by the role-at-end detector. Left as-is: it is a TABLE → Phase 9a
  (pdfplumber) will handle it; refining the regex to catch mid-line roles risks over-firing on real
  provisions ("rights of a member…").
- DETERMINISTIC-PARSER PHASE COMPLETE across the 7-doc corpus (all D1-D12 + TOC/back-matter/footnote/
  vocab/roster). Remaining: Phase 9a pdfplumber table sidecar; separate page-attribution (D12) accuracy
  follow-up.

### 2026-06-21 — ingestion — 04_Sindh_Local_Government_Act_2013.pdf (scan) — PaddleOCR@300 + two-column zoning fixes
- Context: re-indexed the lone scan with PaddleOCR @ **300 DPI** (after the 400-DPI run TIMED OUT at the
  26-min budget and fell back to tesseract). 300 DPI finished in ~20 min, extraction_method=`paddleocr`,
  14487 chars / 8 pages, 0 low-confidence. Char accuracy good: "Act No.XLII" ×20 verbatim, "(xxxv-a)",
  all clause figures correct (tesseract had garbled these → "(649)", "eee ELS <Sh").
- Observed (per-chunk audit vs the rendered pages — ground truth): residuals are NOT OCR-char garble,
  they are **two-column layout** defects. This gazette has a marginal amendment-notes column
  ("Amendment of section X of Sindh Act No.XLII of 2013") beside the body. PaddleOCR reads it as SHORT
  fragments interleaved into the body stream. Two concrete defects: (1) **page banner** — "PART-IV"
  (OCR "PART-1V") alone on every inner page leaked as a structural Part (empty shells), because
  `chrome::line_is_provision` grants it provision identity, shielding it from the repetition signal;
  (2) **margin notes** — the shattered side-note fragments splice into provision prose → orphan
  fragments + bare/unresolved sub-clause labels.
- Changed (`services/parsers/chrome.rs`, deterministic, no geometry needed):
  - **Banner**: new `is_bare_container_heading` (a `Part`/`Chapter` token + numeral ALONE on the line,
    OCR roman-slip "1V" tolerated). `classify_blocks` strips such a line as chrome when it ALSO recurs
    across pages (checked before the keep-invariant); `repeated_across_pages` now counts bare container
    headings despite their identity. A one-off real "PART VI" (page 1, key `part vi`) is never repeated
    → kept; a title-bearing "PART VI PROVINCIAL ASSEMBLY" is not bare → untouched.
  - **Margin notes**: extended `is_footnote_apparatus` with a SHORT, ^-anchored annotation grammar
    (`Amendment|Omission|Insertion… of`, `sections? N of`, `(Sindh )?Act No.<roman>`, `of <year>.`) →
    routed to the **footnote** class (lifted from body, KEPT retrievable for amendment provenance).
    Misfire-safe: any over-lift lands in the recoverable footnote class, never silently dropped.
- Result: `cargo test --lib parsers::chrome` = **28/28** (+5 new: banner repeat/one-off/with-title,
  margin-notes-are-footnote, body-sentence-not-margin-note); 0 regressions incl. the keep-invariant
  tests. NOT yet re-indexed live (app was stopped) — logic validated on the actual page-2/7 OCR lines.
- Residual / NOT covered: margin fragments that OCR MERGES inline onto a body sentence (e.g.
  "…section 28 shall be substituted Amendment of") aren't split by line-level rules — that needs
  **PaddleOCR bbox column-zoning** (sidecar returns text only today). Bare sub-clause labels (Issue 2a)
  are downstream of the parent-heading shells and should improve once banners/shells are gone; confirm
  after reindex.
- NEXT: user restarts app → re-index doc 04 → verify "PART-1V" Part shells are gone, margin fragments
  sit in footnote chunks (not provisions), and unresolved sub-clause count drops. If inline-merged
  margin tails remain, scope the bbox column-zoning (sidecar per-line boxes → x-band margin signal).

### 2026-06-21 — ingestion — doc 04 reindex (PaddleOCR@300) — margin-notes FIXED live; banner needed a 2nd fix
- Reindexed with the chrome.rs fixes. Counts before→after: chrome 81→**28**, footnote 4→**69**,
  provision 74→74, unresolved 19→19, total 159→171.
- MARGIN NOTES ✅ live-verified: the 69 footnote chunks are exactly the shattered side-note fragments
  ("Amendment of", "Act No.XLII", "of2013.", "Omission of", "Sindh Act No.XLII") — lifted out of
  provision prose, kept retrievable. Body pollution from the margin column is gone.
- BANNER ❌ chrome rule did NOT fire in production — 9 "Part-1V" provision shells survived (DB: all 9
  labelled exactly `Part-1V`, all stamped page 7 — a separate D12 page-collapse). Root cause: the
  banner is minted by `heading_detector/regex_pass.rs::scan_structural_boundaries` (profile-independent
  `PART[-]?[IVXLC0-9]+` boundary regex) running on the chrome-CLEANED text. The chrome repetition rule
  is correct in isolation (new `strip_chrome_repro_paddle_banner` test passes) but is bypassed live —
  the 300-DPI OCR + the text_normalizer (which runs BEFORE strip_chrome; my repro fed raw text) vary
  the banner text enough that the cross-page key never reaches the 4-page threshold. Cross-page
  key-matching is too fragile for OCR'd banners.
- Changed (`regex_pass.rs::scan_structural_boundaries`, the robust source-level fix): after collecting
  boundary headings, **drop any label that recurs `>= 3` times** — a genuine Part/Schedule/Form heading
  appears once; a label repeating on every page is a running header. Page/key-independent, so it
  survives OCR/DPI variance. The banner TEXT stays in the body (only the false container node is
  suppressed); a one-off "PART-I"/"SECOND SCHEDULE" is untouched.
- Gate: `cargo test --lib` = **540 passed / 0 failed** (new: regex_pass banner-dedup + chrome
  strip_chrome repro; chrome 29/29). NOT yet reindexed with this 2nd fix.
- Residual (unchanged, needs bbox column-zoning): inline-merged margin tails; bare sub-clause labels
  (19, downstream of parent-heading shells — should improve once banner shells are gone); D12
  page-collapse (all provisions stamped page 7) is a separate page-attribution bug.
- NEXT: rebuild + re-index doc 04 → confirm 0 "Part-1V" provisions, then look at the D12 page-collapse
  (paddle path page_offsets vs persist_tree page stamping) and the residual bare sub-clause labels.
