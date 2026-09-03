# Phase 0 Research: LGD Summary & Note Drafting

Resolves the items deferred from `/sp.clarify` (latency, template lifecycle, cited-slot retrieval mechanics) plus the key reuse decisions.

## R1 — Cited-slot retrieval mechanics (how a required-source category fills a cited slot)

**Decision**: For each cited slot, build a scoped retrieval query from (a) the slot's required-source category text and (b) the intent answers (scheme/subject), restricted to the **confirmed document set**. Run it through the existing `query_router` retrieval seam (FTS5 + ANN, Tier 1/2 — no LLM). Select the chunk with the existing Feature 008 Fix B `resolve_provision_label` title-match-not-rank selector. Render the slot as the chunk's verbatim quote + a deterministic hierarchical citation built from chunk metadata. If `resolve_provision_label` returns `None` (no chunk whose label/title matches above the content-token-overlap threshold), emit the refuse-or-flag sentinel for that slot.

**Rationale**: Keeps the citation-grade contract in one place — the number/citation is never produced by a model, exactly as proven in V5 (Q15 31/32/39). Title-match-not-rank already handles the "correct chunk is not rank 1" case. Scoping to the confirmed set is what prevents another scheme's facts/rules from leaking in. Honors Tiered Intelligence (retrieval before any model).

**Alternatives considered**:
- *Let the generator fill cited slots from context* — rejected; V4/V5 proved the 0.5B confabulates numbers even with the correct labelled chunk present. This is the whole reason for Fix B.
- *Embed the required-source as a fixed FTS query string per template* — rejected; brittle and edges toward hardcoded keywords. Deriving the query from the slot category + intent each draft keeps it document-agnostic.

## R2 — Fact slots vs cited slots (provenance separation)

**Decision**: Tag each corpus document in the confirmed set as **standing-rules** vs **case** at selection time (heuristic from folder/source metadata + intent), surfaced in the doc-set list so the officer can correct it. Cited slots draw only from standing-rules documents; fact slots (amounts, scheme name, dates) draw only from case documents. Facts are still extractive (verbatim span + attribution to the case document), not model-invented.

**Rationale**: FR-008 requires the split. Making the rules/case tag visible and correctable in the curation step (US3) keeps the human accountable for provenance and avoids a hidden classifier deciding silently.

**Alternatives considered**:
- *Single undifferentiated retrieval pool* — rejected; risks citing a case document as if it were a rule, or pulling a "scheme cost" number from the rules book.

## R3 — Latency / performance shape

**Decision**: Treat a draft as a **progress-reported batch operation**, not a single sub-2s query. Resolve all cited/fact slots first via retrieval, **in parallel** (no LLM involved → safe under Single Model Slot). Then generate prose slots **sequentially** on the one warm generator, each budgeted < 10s (Tier 3/4 constitution budget), emitting a progress event per slot. Minimize prose slots (most of a Summary/Note is boilerplate + cited extract). Per-cited-slot retrieval target < 2s (Tier 2 budget).

**Rationale**: Honors Principle III (only retrieval is parallel; LLM calls stay serial) and the constitution latency budgets (p95 < 2s Tier 2, < 10s Tier 3/4) applied per slot rather than to the whole multi-step draft. Progress events reuse the existing importer/query progress pattern.

**Alternatives considered**:
- *Single mega-prompt that drafts the whole instrument* — rejected; reintroduces model-generated numbers (contract violation) and blows the context/latency budget.
- *Parallel LLM prose generation* — rejected; violates Single Model Slot.

## R4 — Template lifecycle & persistence

**Decision**: Persist templates in `echo.db` via an **additive migration**: a `template_schema` row (id, name, instrument_type, created/updated, source-format reference) with child `template_slot` rows (ordering, kind, label, required-source category, boilerplate text). Operations: create-on-confirm, list, select, update (re-confirm overwrites the schema in place), delete. **No version history in v1** (overwrite). Gold-example metadata lives in a `gold_example` table referencing on-disk content; gold rows are explicitly excluded from any retrieval query.

**Rationale**: SQLite single source of truth (Principle IV); additive-only (development constraint). Overwrite-on-reconfirm is the smallest viable lifecycle that satisfies "save once, reusable, correctable." Versioning is a documented future enhancement.

**Alternatives considered**:
- *Store templates as JSON sidecars on disk* — rejected; violates SSOT and complicates listing/selection.
- *Full version history per template* — deferred; not needed for first cut, adds schema + UI weight.

## R5 — Template ingestion / auto-classification — **SUPERSEDED 2026-06-10 by R5a**

> The heuristic decision below shipped and was **proven inadequate on real instruments** (2026-06-09 live test + 2026-06-10 gold-ingest diagnostic): heading-shape + line-length segmentation collapsed a flawless real instrument (GOLD_SPSC_age_relaxation_2025) into 13 garbage slots — letterhead/dateline/C.C. lines became false cited slots, while the actual citations (Rule-6(c) SPSC Reg-2023, Rule-23 SCUG Rules 2020) appeared in NO slot. Root causes: (1) segmentation assumed headings, real instruments are numbered running paragraphs; (2) citations were detected only in headings, never inline, and the provision regex required whitespace so hyphenated `Rule-6(c)` never matched; (3) structural boilerplate tripped the provision-noun check. Retained for history; do not re-implement.

## R5a — Template ingestion: model-assisted segmentation + deterministic verbatim-verify gate (Option C, 2026-06-10)

**Decision**: Rewrite `services/drafting/ingest.rs::propose_schema` as a two-stage pipeline:

1. **Model proposes** — the Tier-4 worker (**Mistral-7B-Instruct-v0.3 Q4_K_M**, already on disk at `models/ir_builder/` from Feature 006) is demand-loaded and prompted in **free-text JSON** mode (NOT llguidance grammar — see evidence) to segment the extracted template text into ordered regions `{ kind: boilerplate|cited|fact|prose, label, text, provision? }`, with explicit copy-verbatim instructions. Output goes through a tolerant JSON parser (reuse the F006 lenient-parse pattern). This is a **one-time, offline, admin-side operation** — never on the per-draft hot path.
2. **Deterministic gate verifies** — for every model-proposed slot: (a) `text` must be a verbatim substring of the extracted source (whitespace-normalized); (b) any `provision` must also appear verbatim and is re-resolved via `decompose::resolve_provision_label` extended with **hyphen tolerance** (`Rule-6(c)`, `Regulations-2023`); (c) the cited slot's `required_source_category` is derived from the **matched provision**, not heading tokens; (d) structural boilerplate (letterhead, dateline, addressee, C.C., signature) must not be classified cited. A slot failing any check is surfaced in the officer-confirm step as **low-confidence with the rejection reason** — never silently dropped or accepted (FR-010a).
3. **Refuse, don't degrade** — if no segmentation model is on disk, or output is unparseable after bounded retries, ingestion **fails with an actionable error naming the missing model** and produces no schema (FR-010b). The heuristic fallback is removed: a degraded schema silently poisons every future draft from that template.

**Rationale (empirical — spike 2026-06-10, results at `~/Desktop/Noviq Documents reports/option_c_ingestion_spike_20260610/`)**:
- Mistral-7B free-text on the real gold: clean slots, verbatim provisions (full Law-Dept opinion chain incl. notification numbers), boilerplate correctly tagged — Defect 1 fixed. ~18 min CPU acceptable for a one-time op.
- The gate makes the model structurally powerless over content: it can only *propose* segmentation; every byte that survives is verbatim-verified, and numbers re-resolve deterministically (citation-grade contract, Principle XI).
- Constitution: Principle II — escalation to Tier 4 is justified because Tier 1 (regex/heuristic) demonstrably cannot satisfy the request; Principle III — the 7B occupies the single slot for the duration of ingestion (exclusive, demand-loaded, evicted after); Principle VII — on RAM tiers that cannot host the 7B, ingestion is unavailable *with a user-visible actionable explanation* (= FR-010b refusal), not a silent failure.

**Alternatives considered (all empirically eliminated in the spike)**:
- *Keep the deterministic heuristic* — produced 13 garbage slots on a flawless real instrument; algorithmic failure, not a bad-input artifact.
- *`noviq_qwen3b` (4.5 min)* — unfaithful: paraphrased source text, dropped citation anchors (`provision: null`), lumped regions. The gate can reject paraphrase but cannot recover omissions — reproduces the citations-swallowed failure.
- *Mistral-7B + llguidance grammar* — hard C++ abort after `init_tokenizer` (same crash class as F006 IR-builder); grammar path is dead.
- *Qwen 0.5B* — skipped by user decision 2026-06-10 (Mistral-7B locked).

## R11 — Two-tier semantic gap coverage (2026-06-10, amends R6)

**Decision**: Replace `gaps.rs::category_covered`'s title-token overlap with **content-semantic coverage**: embed the cited slot's `required_source_category` text (existing fastembed embedder) and compare against the **indexed chunk embeddings** of each document (existing `chunk_embeddings`); a document covers a category when its best chunk similarity clears a tunable threshold (calibrated against the 8-PDF live corpus where RoB 1986 must cover "consultations"). Detection then runs in two tiers (FR-019): **(a)** category covered by an indexed document *not in the confirmed set* → an **add-this-document suggestion** the officer accepts in one turn; **(b)** category covered by *no* indexed document → a true missing-document gap (index into the watched folder, or proceed-without → refuse-or-flag). R6's watch/notify mechanics are unchanged.

**Rationale**: The 2026-06-09 live test showed title matching is structurally blind — "consultations" can never lexically match the title "Rules of Business 1986" even though Rule 17 governs consultations and its content is indexed. Coverage must be judged where the evidence lives: chunk content. Tier (a) is the value split the user demanded — the AI auto-assembles citation scaffolding from what is already indexed and only asks the officer for what genuinely does not exist.

**Alternatives considered**:
- *Title-token overlap (shipped)* — proven to over-report (~10–11 false gaps on an 8-document corpus).
- *FTS5 lexical match over chunk content* — better than titles but still misses synonymy ("concurrence" vs "consultation"); embeddings already exist, marginal cost is a handful of vector comparisons.
- *LLM-judged coverage* — violates Tiered Intelligence (Tier 2 suffices) and adds a model call to the per-draft path.

## R12 — Officer-supplied fact channel (2026-06-10)

**Decision**: `fact:` slots whose value exists in no indexed document (case-specific facts: "100 schools", "Rs 500B") get a distinct conversational prompt: point to a case document **or type the value directly**. A typed value is stored in the session (`GapResolution::OfficerSupplied { value }`), rendered in the fact block under an explicit **"officer-supplied"** attribution, visually distinct from cited content (FR-003), never dressed as a citation. The model never generates or alters officer-supplied values; they pass through verbatim (FR-008a).

**Rationale**: These gaps are *legitimate* — no corpus holds per-case facts — but reporting them as "index a document" gaps made the officer fill every field manually. The intent Q&A already accepts typed purpose/scheme; the officer is the accountable approver (spec Assumptions). An explicit attribution keeps the provenance ledger honest.

**Alternatives considered**:
- *Strict document-only facts* — rejected in clarify: in the headline ADP scenario the sanctioning facts often exist in no indexable document at draft time.
- *Typed facts rendered as flagged/unverified* — rejected: conflates a deliberate officer input with a citation failure; distinct attribution is clearer.

## R6 — Missing-document handling & watch

**Decision** *(coverage test superseded 2026-06-10 by R11 — two-tier semantic coverage; watch/notify mechanics below remain in force)*: Gap detection compares each cited slot's required-source **category** against the categories present in the confirmed set. For each missing category, report the gap with an openable link to the **existing watched corpus/indexed folder**. Reuse `file_watcher` + `indexer`: when a new document is indexed there, re-run doc-set proposal, update the working set, and emit a notification event. "Proceed without" marks the affected slots for refuse-or-flag at render time.

**Rationale**: Reuses the existing watcher/indexer with zero new ingestion path (FR-015, Principle IV). Category-level matching (clarified) keeps templates reusable across schemes.

**Alternatives considered**:
- *Per-gap ad-hoc folder watching* — rejected in clarify (single watched folder chosen).

## R7 — Output rendering

**Decision**: Render the draft as ordered, labelled blocks per slot (boilerplate verbatim; cited = quote + hierarchical citation; fact = extractive value + attribution; prose = generated narrative; flagged = explicit "unverified — source not available" marker visually distinct). Reuse the extractive output shape and the existing `exporters` (txt/pdf) for copy/save. Layout-faithful document reproduction is out of scope for the first cut (clarified).

**Rationale**: Confirmed in clarify; fastest path to the citation-grade guarantee; exporters already exist for the copy/save hand-off.

## R8 — Conversational session over the existing pipeline (2026-06-09)

**Decision**: Make the whole drafting flow one continuous conversation that drives the *existing* deterministic pipeline. Each officer turn is parsed into a structured `DraftAction` (set-intent-field, confirm/add/remove-document, resolve/declare-unavailable-gap, targeted-revision, finalize) and dispatched to the already-built `session.rs`/`docset.rs`/`gaps.rs` command paths. The conversation never generates or alters citations/numbers/facts (FR-028).

**Rationale**: The pipeline contracts (FR-004–FR-008, FR-024/025) are proven and citation-grade; wrapping them in a conversation keeps the guarantee intact while delivering the officer-first interactive UX (Principle X). Parsing-to-action keeps generative scope out of the citation surface.

**Alternatives considered**:
- *Free-form LLM agent that calls tools ad hoc* — rejected: too much generative authority near citations, harder to keep deterministic; violates the spirit of FR-004/XI.
- *Keep panels, add a chat box only for intent (scope A)* — rejected by user in favour of full-B single transcript.

## R9 — Chat persistence model (2026-06-09)

**Decision**: Persist every turn in a dedicated additive table `chat_message` (migration 023), linked to `draft_session` (migration 022), append-only, ordered. Do **not** stuff messages into the `DraftState` JSON blob (it is rewritten per save; chat is append-heavy and needs indexed retrieval/audit).

**Rationale**: Satisfies "every chat datum stored" + resume/audit (FR-031/032) while staying in `echo.db` (Principle IV). Separate table keeps writes cheap and history queryable.

**Alternatives considered**:
- *Messages inside DraftState JSON* — rejected (rewrite cost, no indexed history).
- *Separate chat DB file* — rejected (violates Principle IV).

## R10 — Context compaction: rolling summary, single slot (2026-06-09)

**Decision**: Bound model context with a rolling summary. When the live dialogue exceeds a tier-derived token threshold, compact the oldest un-summarized turns into `conversation_summary` via **one call on the single resident model slot**; thereafter feed the model `summary + most-recent turns`, never the full transcript. All raw turns stay in `chat_message`. Compaction must preserve decision-relevant state (current intent, confirmed doc set, unresolved gaps, finalize status) — FR-034.

**Rationale**: Fits the small-context generator (~3584 `n_ctx`); cheap; keeps current state authoritative. Honours Principle III — no second model is loaded for summarization.

**Alternatives considered**:
- *Semantic retrieval over embedded turns* — feasible (embedder + vec exist) but heavier (embeds + slow HNSW insert per turn, risk of surfacing stale state) and overkill for a short linear drafting dialogue. **Deferred** as a later upgrade; ties to the pending option #3 in the chat-memory backlog.
- *Hard truncation of old turns* — rejected: silently drops decision-relevant state.

## Open follow-ups (non-blocking, for /sp.tasks or later)

- Real sanitized LGD summaries/notes from the user are required as template examples **and** the acceptance/eval gold (not available publicly) — acceptance test will use a small synthetic fixture until gold arrives.
- Optional: consult the xref graph (Principle XIII) in gap detection ("rules under section X") — deferred.
- ~~Optional: LLM-assisted template classification as an accelerator over the heuristic~~ — **now mandatory and load-bearing (R5a)**; the heuristic is removed.
- Perf follow-up (separate from this amendment): `retrieve_whole_corpus` is a brute-force linear `vec_distance_cosine` scan over all chunk embeddings — fine on the 8-PDF pilot corpus, needs vec0 ANN before large corpora.
- Still needed from user: a TRUE "Summary for the Chief Secretary" gold instrument (the SPSC gold is an inter-departmental letter) for the eval set.
