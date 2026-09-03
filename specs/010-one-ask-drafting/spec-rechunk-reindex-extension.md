# Spec Extension (Feature 010): Re-chunk & Re-index for Citation-Grade Retrieval

**Extends**: Feature `010-one-ask-drafting` (this is an extension of 010, NOT a new feature)
**Feature Branch**: `011-postgres-pgvector-port` (worked on the existing branch — no new branch)
**Created**: 2026-06-15
**Status**: Draft
**Input**: User description: "Re-chunk and re-index the indexed corpus to fix chunk fragmentation that blocks citation-grade retrieval … add a small-leaf coalescing pass, repair OCR/newline artifacts, fix hierarchy/label capture, then re-index and re-embed."

> **Why an extension of 010**: 010 (one-ask drafting) owns the citation-grade drafting contract.
> The fragmentation discovered while validating 010 drafts is the root blocker to that contract,
> so the corpus-quality fix is scoped as a 010 extension rather than a standalone feature. The FR-/SC-
> IDs below are local to this extension document (010's base spec keeps its own numbering).

## Problem & Context

Empirically confirmed via live database inspection: the indexed corpus is **20,817 passages with a median length of 41 characters; 55% are under 50 characters.** Legal provisions are shredded into 3-to-6-word scraps. For example, the SPPRA "Direct Contracting" rule — the governing provision for single-source procurement — exists only as the fragment `"(1) Direct Contracting"` (22 characters) plus disconnected scraps; the operative text ("procurement from a single source without competition … (i) standardization … (ii) proprietary nature …") exists in **no single passage**.

Consequences observed in live drafts:
- The governing provision **cannot be retrieved or cited**, so the drafter falls back to whatever *complete* paragraphs happen to exist (generic Rules-of-Business text), producing off-topic, unusable summaries.
- Quoted text carries extraction garbage ("o ne hundred", "sha ll", "t he").
- Provision labels are wrong or bare ("Rule 2017" from a mis-read year; "(1)"/"(s)" with no parent rule).

This feature restores the corpus to coherent, correctly-labelled provision units so the citation-grade drafting contract can actually be met. It is the prerequisite gate for trustworthy drafting; retrieval-ranking tuning has been shown to have hit its ceiling because there is nothing coherent to retrieve.

## User Scenarios & Testing *(mandatory)*

### User Story 1 - The governing provision is retrievable and citable (Priority: P1)

An officer asks for a summary on a specific subject (e.g. single-source/direct-contracting procurement). The system finds and cites the **actual governing provision as one coherent, verbatim unit**, instead of unrelated boilerplate.

**Why this priority**: This is the whole point of the product — citation-grade drafting. Without coherent provision units, every subject-specific draft is wrong. Delivering only this story already makes drafting usable.

**Independent Test**: Re-index the corpus, then run the "Canon-scanner / direct contracting" ask. Confirm the draft cites the SPPRA Direct Contracting / single-source provision as a complete, readable unit (not a 22-character heading), and that it appears among the retrieved candidates for the relevant section.

**Acceptance Scenarios**:

1. **Given** a corpus containing the Direct Contracting rule, **When** the officer asks a single-source procurement question, **Then** the governing provision is retrieved as one coherent passage and cited verbatim.
2. **Given** any provision whose full text spans several markers/sub-clauses, **When** it is indexed, **Then** it is stored as a single coherent unit (not split per marker), bounded to a readable size.
3. **Given** the re-indexed corpus, **When** the median passage length is measured, **Then** it sits within a coherent-provision range rather than the previous 41-character median.

---

### User Story 2 - Quoted text is clean and paste-ready (Priority: P2)

When a provision is cited, the quoted text reads correctly — no broken words or stray spacing — so it can be pasted into official stationery without manual cleanup.

**Why this priority**: Even a correctly-retrieved provision is unusable in an official document if it reads "Procurements over o ne hundred thousand rupees". Clean quotes are required for the copy-paste promise, but they are second to retrieving the right provision at all.

**Independent Test**: After re-indexing, inspect citations from previously-garbled sources; confirm common extraction artifacts ("o ne" → "one", "sha ll" → "shall", broken mid-sentence lines) are repaired while the wording is otherwise unchanged.

**Acceptance Scenarios**:

1. **Given** a source with extraction artifacts, **When** its text is indexed, **Then** spurious mid-word spaces and broken lines are repaired without altering the actual words.
2. **Given** a repaired passage cited in a draft, **When** the officer copies it, **Then** the quoted text is grammatically intact and matches the source meaning verbatim.

---

### User Story 3 - Provisions carry their correct citation label (Priority: P3)

Each cited unit is labelled with its real provision reference (e.g. "Rule 42(1)(ii)"), not a bare marker ("(1)", "(s)") or a mis-captured year ("Rule 2017", "Section 2010").

**Why this priority**: Correct labels are essential for citation-grade output, but a correct provision with a slightly imperfect label is still more useful than today's state; hence P3 after retrieval (P1) and clean text (P2).

**Independent Test**: After re-indexing, sample units whose labels were previously bare or year-like; confirm they now show the hierarchical provision label derived from document structure.

**Acceptance Scenarios**:

1. **Given** a sub-clause unit, **When** it is labelled, **Then** the label includes its parent provision (e.g. "Rule 42(1)(ii)"), not a bare "(ii)".
2. **Given** a heading that contains a year, **When** it is labelled, **Then** the year is not mistaken for the provision number.

---

### User Story 4 - Apply the improvements to the existing corpus safely (Priority: P1)

The architect/officer can apply the re-chunk + re-embed to the already-indexed documents, with the existing searchable corpus remaining usable until the new one is ready.

**Why this priority**: The improvements (US1–US3) have no effect until the corpus is rebuilt; and the rebuild must not leave the officer with a broken or empty corpus mid-operation. This is co-P1 with US1 because US1 cannot be realized without it.

**Independent Test**: Trigger the re-index on a populated corpus; confirm progress is visible, the old corpus answers queries until the new one completes, and on completion the new coherent units are in place with embeddings regenerated.

**Acceptance Scenarios**:

1. **Given** a populated corpus, **When** a re-index is triggered, **Then** progress is reported and the operation can be observed to completion.
2. **Given** a re-index in progress, **When** the officer runs a query, **Then** they still get answers (the prior corpus is not destroyed until the new one is ready) **OR** they are clearly told indexing is in progress.
3. **Given** a completed re-index, **When** the corpus is inspected, **Then** every document has been re-chunked and re-embedded with the new coherent units.

---

### User Story 5 - The officer's discriminating words drive lexical retrieval (Priority: P2)

When the ask contains both common governance words ("secretary", "department", "approval") and the few rare words that actually identify the provision ("contracting", "distributor"), the lexical leg is weighted toward the rare/discriminating words so the governing provision is not buried under boilerplate that merely shares common vocabulary.

**Why this priority**: Coalescing (US1) makes the governing provision *exist* as a coherent unit, but the lexical retrieval leg uses `ts_rank_cd`, which has **no IDF** — a chunk matching many common query words can still outscore the one chunk matching the rare identifying words. Without this, a now-coherent provision can still lose the ranking to common-word boilerplate. It is P2 (alongside clean quotes): it sharpens retrieval of units US1 created, but US1 is the prerequisite.

**Independent Test**: For an ask mixing common and rare subject words, confirm the lexical query is built from the rarest (lowest corpus document-frequency) terms — ubiquitous terms (matching >50% of the corpus) and zero-match terms are dropped — verified by unit tests over corpus document-frequency inputs; retrieval never breaks if the DF lookup fails (falls back to the plain token query).

**Acceptance Scenarios**:

1. **Given** an ask containing both common and rare subject words, **When** the lexical leg is built, **Then** it keeps the rarest discriminating terms and drops ubiquitous (>50% corpus) and zero-match terms.
2. **Given** the corpus document-frequency lookup fails or returns nothing, **When** the lexical leg is built, **Then** it falls back to the plain officer-token query so retrieval still runs.

---

---

### User Story 6 - The draft is about the officer's actual matter, not the rules in the abstract (Priority: P1)

The officer's one-sentence request (e.g. "direct contracting with the sole distributor of Canon scanners for e-dafter") is the **subject** of every section. Each section explains how the retrieved provisions apply **to that specific matter** — it does not merely paraphrase the rules.

**Why this priority**: This is the product's core promise — "one sentence in → a case-specific summary out". Confirmed defect: the writer prompt received only the generic section title, generic guidance, and the retrieved rule cards, but **never the officer's request**, so a draft about purchasing a Canon scanner contained the words "Canon"/"scanner"/"e-dafter" zero times and read as a generic digest of procurement rules. Co-P1 with US1: coherent provisions are useless if the draft is not about the officer's matter.

**Independent Test**: Run a draft for a specific ask; confirm the officer's subject (its distinctive nouns) appears woven through the sections, and the provisions are presented as the legal basis for that matter — not described in isolation.

**Acceptance Scenarios**:

1. **Given** an officer ask naming a specific matter, **When** each section is composed, **Then** the writer prompt includes the officer's request as the subject and the prose is about that matter.
2. **Given** an ask that contains a monetary figure, **When** the subject is injected into the prompt, **Then** the raw number is NOT exposed as prose (it re-enters only via the figure-marker lane), preserving the verbatim-number gate.

---

### Edge Cases

- **Already-clean documents (e.g. the Constitution)** must not regress: well-structured provisions that are already coherent must remain coherent and correctly labelled after re-chunking. Coalescing must not over-merge distinct provisions into oversized blobs.
- **A genuinely short provision** (a one-line rule) must remain its own unit and not be force-merged into an unrelated neighbour.
- **A genuinely long provision** must still be bounded (the existing oversize handling must continue to apply) so units stay within a readable retrieval size.
- **Coalescing across provision boundaries**: adjacent fragments belonging to *different* provisions must not be merged together.
- **Interrupted re-index** (power loss, app close): the corpus must be recoverable to a consistent state — never left half-rebuilt and presented as complete.
- **A fragment with no recoverable parent label** must keep a truthful (if bare) label rather than inventing one.
- **Verbatim contract**: repaired text must preserve the actual words; the citation/verify gate must still confirm a quote exists in its source after repair.

## Requirements *(mandatory)*

### Functional Requirements

- **FR-001**: The system MUST merge adjacent under-sized sibling passages that belong to the same provision into a single coherent unit, targeting a readable provision size, so that a provision's operative text is retrievable as one passage.
- **FR-002**: Coalescing MUST respect provision boundaries — passages belonging to different provisions MUST NOT be merged together.
- **FR-003**: The system MUST continue to bound over-sized units so no passage is too large for retrieval, preserving today's behaviour for long provisions.
- **FR-004**: The system MUST repair common text-extraction artifacts (spurious mid-word spaces, broken-across-line words, stray separators) in indexed text **without changing the actual words**, so quoted text is clean and paste-ready.
- **FR-005**: Each indexed unit MUST carry a citation label derived from the document's structure that includes its parent provision (e.g. "Rule 42(1)(ii)"), rather than a bare sub-clause marker.
- **FR-006**: The system MUST NOT mistake a year (or other non-ordinal number) in a heading for a provision number when assigning labels.
- **FR-007**: The system MUST provide a way to re-chunk and re-embed the already-indexed corpus and apply the new units.
- **FR-008**: During a re-index, the previously-indexed corpus MUST remain queryable until the rebuilt corpus is ready, OR the officer MUST be clearly informed that indexing is in progress; the operation MUST report progress.
- **FR-009**: A re-index that is interrupted MUST leave the corpus in a consistent, recoverable state and MUST NOT present a partially-rebuilt corpus as complete.
- **FR-010**: Re-chunking MUST NOT regress already-coherent, well-structured documents (no over-merging, no label loss).
- **FR-011**: After re-indexing, the citation/verify guarantee MUST still hold — every quoted citation MUST be confirmable verbatim against its source, and unverifiable text MUST still be flagged, never invented.
- **FR-012**: The whole operation MUST run fully offline/local — no external services — and re-embedding MUST use the existing local embedding capability.
- **FR-014**: The per-section writer prompt MUST include the officer's request as the explicit subject of the section, instructing the model to write about that specific matter using the retrieved provisions as its legal basis (not to describe the provisions in the abstract). Any monetary figure in the request MUST be neutralised in the injected subject so the raw number is never exposed as prose — amounts re-enter only through the figure-marker lane, preserving the verbatim-number gate (FR-011).
- **FR-013**: The lexical retrieval leg MUST weight the officer's subject tokens by corpus rarity — dropping tokens that match nothing and tokens that are ubiquitous (matching the majority of the corpus), keeping the rarest, most-discriminating terms — derived purely from corpus document-frequency statistics (no hardcoded term lists), and MUST fall back to the plain token query if the statistics are unavailable so retrieval never breaks.

### Key Entities *(include if feature involves data)*

- **Provision Unit**: A coherent, retrievable passage representing one provision (or a sensible sub-part), with a correct hierarchical citation label, clean text, and a regenerated semantic representation. Replaces today's per-marker fragment.
- **Document Structure**: The parsed hierarchy (document → part/chapter → rule/section → clause → sub-clause) from which units and labels are derived.
- **Re-index Operation**: A user-observable corpus rebuild with progress, safe-until-complete semantics, and interruption recovery.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: After re-indexing, the corpus median passage length rises from 41 characters into a coherent-provision range, and the share of passages under 50 characters drops substantially (target: well under 20%, from 55%).
- **SC-002**: For the "direct contracting / single-source" test ask, the governing provision is retrieved among the candidate passages and cited as one coherent verbatim unit — verified against the previous run where it never appeared.
- **SC-003**: On a sample of previously-garbled citations, quoted text reads correctly (no broken words/stray spacing) in at least 90% of cases, with wording otherwise unchanged.
- **SC-004**: On a sample of previously bare/year-like labels, the displayed citation label is the correct hierarchical reference in at least 90% of cases.
- **SC-005**: The already-coherent reference document (Constitution) shows no regression: provision-level retrieval and labels remain at least as good as before re-indexing.
- **SC-006**: An officer can run a query throughout a re-index without ever facing a silently empty/broken corpus, and a re-index interrupted midway recovers to a consistent state on next launch.
- **SC-008**: For a specific ask, the composed draft is about the officer's actual matter — its distinctive subject nouns appear in the prose and the provisions are presented as the legal basis for that matter — verified by a prompt-construction unit test (subject present; figures neutralised) and a live draft where the subject is no longer absent.
- **SC-007**: For an ask mixing common and rare subject words, the lexical leg is composed of the rarest discriminating terms (ubiquitous and zero-match terms excluded), and a failed document-frequency lookup degrades gracefully to the plain token query — both verified by unit tests over document-frequency inputs.

## Assumptions

- **Re-index is officer/architect-initiated** (not silently automatic), surfaced where indexing is managed, and treated as a heavy operation with clear progress — consistent with the app's existing indexing UX and cautious-action handling.
- **Coalescing target size** is a readable provision range (≈300–800 characters) chosen to balance retrieval coherence against over-merging; exact thresholds are an implementation/tuning detail validated against SC-001 and SC-005.
- **Scope is all indexed documents** (the coalescing/repair is generic), not only OCR'd ones, since fragmentation also arises from dense-marker parsing on clean text.
- **The bundled local database and local embedding capability** are the persistence/representation layer; this feature changes the *content and shape* of units, not the storage technology.
- **OCR engine selection (PaddleOCR vs current)** is explicitly **out of scope** here and handled as a separate evaluation; this feature repairs artifacts in whatever text the current extraction produces.

## Relationship to Existing Work (No Duplication)

Verified 2026-06-15 against existing specs to avoid overlap:

- **Feature 011 (Postgres port) does NOT do this.** 011's corpus work (US3, since cancelled) was a
  *migration* that carries chunks/embeddings over **bit-faithfully — explicitly "not re-extracted,
  re-chunked, or re-embedded"** (`011/spec.md:83`). Its replacement ("wipe old DB, re-index fresh on
  PG") rebuilds the corpus **using the same chunker**, which is exactly why the live Postgres DB still
  shows median-41-char fragments. 011 never changes how documents are chunked. The "Phase 2" of the
  architecture pivot = the Postgres port (011, done); the "Phase 2" of the chunking-fragmentation doc
  = this re-chunk cure. Same label, different work.
- **Dependency, not duplication**: this extension MUST **reuse the existing fresh-re-index path**
  (rebuild-from-source on the bundled engine) rather than build a new one; it only improves the
  chunker/labeller/text-repair that path runs.
- **Dormant prior design to reconcile**: Feature 007's `deep_reindex/chunks_replace.rs` (tasks
  T049/T050, never implemented — stubs) specced *"one chunk per Section, with sub-section merging
  when Section is short"* — conceptually the same as the coalescing here. Planning MUST decide whether
  to revive that design or do a generic `structure_parser` coalescing pass, and MUST NOT create a
  third independent chunking path.

## Out of Scope (Non-Goals)

- Switching the OCR engine to PaddleOCR (separate evaluation/spec).
- The Note-drafting feature (separate spec; needs a note skeleton + prior-notings input channel).
- Postgres re-platform packaging/parity work (Feature 011 US2/US4).
- Retrieval-ranking algorithm changes beyond (a) the lexical-leg subject-noun fix already shipped separately and (b) the corpus-rarity weighting of the officer's own subject tokens specced as US5/FR-013 here. Both are scoped to *which of the officer's words drive the lexical leg* — neither introduces query- or domain-specific term lists, re-ranking models, or score fusion changes.
