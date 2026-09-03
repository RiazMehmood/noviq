# Feature Specification: LLM-Driven PDF → Hierarchical JSON Importer

**Feature Branch**: `006-llm-pdf-importer`
**Created**: 2026-05-29
**Status**: Draft
**Input**: User description: "LLM PDF→JSON importer (v2 of feature 005): local Mistral 7B converts PDFs into the same hierarchical IR JSON that constitution.json uses today, then routes through the existing graph_builder + indexer. … sha256-keyed JSON cache in app data dir with 7-day orphan sweep; GBNF grammar + 3 chunk retries / 2 doc retries; partial-success ingest with low-confidence flagging; split-screen repair UI (rendered PDF page + JSON editor + toast notify); user_repaired nodes never overwritten by re-runs; local Mistral 7B only (no cloud in v1), falls back to legacy rule-based parser if GGUF missing."

## Context

Feature 005 (generic structured importer) ships a rule-based PDF parser that works on documents matching a known profile (constitution, act, rules, contract, manual) but degrades on documents whose layout the rule set has not been tuned for — e.g. Sindh provincial PDFs in the V5 22-prompt eval. The constitution document ingests cleanly only because it has a hand-curated JSON intermediate. This feature generalizes that "clean JSON intermediate" path: every imported PDF is converted by a local language model into the same hierarchical Intermediate Representation (IR) the constitution path already consumes, so every document benefits from the existing graph + index pipeline without per-document tuning.

This is a v2 upgrade path for feature 005 — the rule-based parser stays as a deterministic fallback; the LLM path becomes the primary route when the local model is available.

## User Scenarios & Testing *(mandatory)*

### User Story 1 - Import a previously-unsupported PDF and get clean structure (Priority: P1)

A user drops a structured legal/contractual PDF (e.g. an Act, SRO, or contract) into a watched folder. Today the rule-based parser either rejects it or produces poor structure that hurts retrieval. With this feature, the system extracts the document automatically into a hierarchical structure (parts → chapters → sections → clauses, with footnotes and amendment annotations preserved) without the user authoring a profile or curating a JSON file. The document becomes searchable through the existing chat/notebook interface with the same quality the hand-curated constitution shows today.

**Why this priority**: This is the core value of the feature — every other capability supports this. Without it, the feature has no reason to exist.

**Independent Test**: Take one Sindh PDF from the V5 eval corpus that currently fails rule-based extraction, run it through the new importer, then run the 22-prompt eval against just that document. Compare retrieval accuracy on that document's questions against the V5 baseline. Pass if accuracy on that document materially improves *and* the document's sections, clauses, and footnotes appear in the index in their correct parent/child relationships.

**Acceptance Scenarios**:

1. **Given** a structured PDF with multi-level numbering (e.g. Part III › Chapter 2 › Section 14 › sub-clause (a)) and the local LLM is available, **When** the user imports the PDF, **Then** the resulting index contains nodes whose parent/child relationships match the document's actual outline and whose text content covers ≥90% of the source token count.
2. **Given** a PDF containing footnotes and amendment markers (insertions, substitutions, omissions), **When** import completes, **Then** footnotes appear as first-class searchable nodes and amendment annotations are preserved on the affected nodes (showing what was changed, by which instrument, when).
3. **Given** the same PDF is re-imported within seven days, **When** the importer runs, **Then** the cached structured representation is reused and the LLM is not re-invoked.
4. **Given** the local LLM model file is missing from the models directory, **When** the user imports any PDF, **Then** the system falls back to the existing rule-based parser, surfaces a one-time notification explaining the LLM is unavailable, and does not block the import.

---

### User Story 2 - Review and repair sections the importer wasn't confident about (Priority: P2)

After import, the user receives a notification that some sections need human review (e.g. "Import done — 2 sections need your review"). The document is already searchable for the parts the importer was confident about. The user opens the document, sees a clear badge listing how many sections need attention, and opens a split-screen repair view: the rendered PDF page on one side, the structured representation editor on the other. They correct the structure or text for those sections, save, and only the corrected sections are re-indexed — the rest of the document is untouched. The user's corrections are preserved and never overwritten by a future re-run of the importer.

**Why this priority**: Without this, low-confidence sections either silently degrade retrieval or block the whole document from importing. Partial-success + repair turns "all or nothing" into "ship now, polish later" — which materially shortens the path from drop-the-PDF to useful-search.

**Independent Test**: Import a PDF that produces at least one low-confidence section. Verify: (a) the confident sections are searchable immediately, (b) a notification surfaces the count of sections needing review, (c) the repair view opens with the correct PDF page rendered next to the editable structure, (d) saving a repair updates only that section in the index, (e) re-running the importer on the same PDF does not overwrite the human edits.

**Acceptance Scenarios**:

1. **Given** an import where some sections passed the confidence check and some did not, **When** import completes, **Then** the confident sections are searchable and the document card shows a badge indicating how many sections need review.
2. **Given** a low-confidence section, **When** the user opens the repair view for it, **Then** the corresponding PDF page is rendered alongside an editor pre-populated with the importer's best-effort structure for that section.
3. **Given** a user has saved a repaired section, **When** the importer later re-runs on the same source document, **Then** the user's repaired version is preserved and not overwritten.
4. **Given** the user repairs a section, **When** the save completes, **Then** only that section (and its KG neighborhood) is re-indexed — the rest of the document is not re-embedded.

---

### User Story 3 - Trust that cached structure stays in sync with the file system (Priority: P3)

A user adds a PDF, removes it, then adds the same file (or a copy with a different filename but identical content) back a day later. The importer recognizes the content has been seen before and skips the expensive LLM step. If a PDF is removed and not re-added within seven days, the cached structure is automatically cleaned up so storage doesn't grow unbounded.

**Why this priority**: Quality-of-life and storage hygiene. Important for users with churn-heavy folders but not needed for the feature to deliver its core value.

**Independent Test**: Import a PDF, observe a cached JSON record is written. Delete the PDF from the watched folder, observe the cached record is marked orphaned but not yet deleted. Re-add the same PDF (or a copy under a different filename) within seven days — observe LLM is not invoked. Advance the clock (or wait) past seven days for an orphaned record — observe the record is removed.

**Acceptance Scenarios**:

1. **Given** a PDF was previously imported, **When** an identical-content PDF appears in a watched folder (same or different filename), **Then** the importer reuses the cached structure and does not invoke the LLM.
2. **Given** a PDF was previously imported and is then removed from all watched folders, **When** the daily housekeeping pass runs more than seven days later, **Then** the orphaned cached structure is deleted.
3. **Given** an orphaned cached structure exists, **When** the original PDF is re-added within the seven-day grace window, **Then** the cache is reactivated and the LLM is not invoked.

---

### Edge Cases

- **Image-only PDF, OCR yields garbage (very low text density):** importer must not invoke the LLM on unintelligible text. The document is flagged unprocessable; the user sees an explanatory state on the document card. (Existing behavior from feature 004 governs this.)
- **User cancels mid-import:** any partial structure produced so far is discarded and the document row is rolled back to its pre-import state. The user sees a clear cancellation state, not a half-imported document.
- **PDF re-imported after a schema-version bump:** the importer re-runs on the document, but any sections the user previously repaired are migrated forward by content match rather than being thrown away.
- **Multiple PDFs queued at once:** the importer processes them one at a time. The user can see queue position and per-document progress.
- **Document classifies as free-form / unstructured (no clear hierarchy):** importer falls back to a relaxed structure (paragraphs + headings only) rather than failing. The document is still searchable; structure is just shallower.
- **LLM emits structurally valid but semantically wrong output (e.g. swaps section numbers):** the validator catches some classes of this (coverage, monotonic numbering, parent_id consistency); the rest surfaces through the repair UI when the user spots it. The feature does not claim to be infallible — it claims to be auditable.

## Requirements *(mandatory)*

### Functional Requirements

#### Extraction & structure

- **FR-001**: System MUST convert imported PDFs into a hierarchical structured representation whose node kinds, parent/child links, numbering, text content, footnotes, and amendment annotations match the source document's outline.
- **FR-002**: System MUST classify each imported PDF into a document type (constitution, act, rules, statutory regulatory order, contract, generic) before extraction so type-appropriate metadata is captured.
- **FR-003**: System MUST capture footnotes as first-class searchable nodes (not inline annotations) so they appear in retrieval results.
- **FR-004**: System MUST preserve amendment markers (insertions, substitutions, omissions, renumberings) on the affected nodes, including the instrument and date of amendment when present.
- **FR-005**: System MUST preserve omitted sections as structural placeholders so cross-reference numbering remains intact.
- **FR-006**: Every structured node MUST be traceable back to a page number in the source PDF.

#### Quality gating

- **FR-007**: System MUST validate the produced structure against: required field completeness, valid node-kind values, parent/child reference integrity (no orphans, no cycles, no duplicate IDs), text coverage ≥90% of source **non-whitespace character count** (tokenizer-neutral; same measure as SC-002), and per-document-type annotation rules.
- **FR-008**: When validation fails on a portion of the document, the system MUST retry that portion before giving up, up to a bounded number of attempts.
- **FR-009**: Portions that fail validation after retries MUST be flagged as low-confidence and made visible to the user without blocking the rest of the document from being indexed.

#### Partial-success ingest & repair

- **FR-010**: System MUST index sections that passed validation immediately — the document MUST be searchable for those sections before low-confidence sections are repaired.
- **FR-011**: System MUST notify the user, via an in-app notification visible without opening the document, when any imported document has low-confidence sections.
- **FR-012**: System MUST surface a per-document indicator (e.g. badge) showing the count of sections needing review.
- **FR-013**: Users MUST be able to open a repair view for any document (not only those with flagged sections) that shows the rendered PDF page alongside an editor for the structured representation of the same region.
- **FR-014**: Users MUST be able to navigate sequentially through all low-confidence sections in a document from the repair view.
- **FR-015**: Users MUST be able to add structural nodes the importer missed entirely (e.g. a section the importer skipped).
- **FR-016**: On save, the repair view MUST re-run validation on the edited nodes and only persist changes that pass.
- **FR-017**: After a successful repair save, the system MUST re-embed and re-index only the changed nodes and their graph neighborhood — not the whole document.
- **FR-018**: User-repaired nodes MUST be preserved across future re-imports of the same source document. They MUST NOT be overwritten by the importer.

#### Cache & lifecycle

- **FR-019**: System MUST cache the structured representation, keyed by source PDF content hash (not filename), so identical content under different filenames shares one cached representation.
- **FR-020**: When a source PDF is removed from all watched folders, the cached representation MUST be marked orphaned (timestamped) but not immediately deleted.
- **FR-021**: System MUST delete cached representations that have been orphaned for more than seven days, via an automatic daily housekeeping pass.
- **FR-022**: When a previously-removed PDF is re-added within the seven-day grace window, the cached representation MUST be reactivated and the conversion step MUST be skipped.
- **FR-023**: When the cached representation's schema version is older than the current schema version, the system MUST regenerate it on next import, while migrating any user-repaired nodes forward by content match.

#### Progress, concurrency & cancellation

- **FR-024**: System MUST process imports one document at a time and MUST show queue position for pending documents.
- **FR-025**: System MUST surface per-document progress (current section / total sections, retry count, estimated time remaining) while a conversion is running.
- **FR-026**: Users MUST be able to cancel an in-progress import. On cancel, any partial state MUST be discarded so the document is left in its pre-import state.

#### Fallback & availability

- **FR-027**: When the local language model is unavailable (model file missing or fails to load), system MUST fall back to the existing rule-based parser without blocking imports, and MUST surface a one-time, non-blocking notification explaining that LLM extraction is disabled and how to enable it.
- **FR-028**: When an imported PDF yields too little text to be meaningfully structured (e.g. image-only with failed OCR), system MUST flag it as unprocessable and MUST NOT invoke the language model on it.

#### Provenance

- **FR-029**: Every indexed node MUST record which extraction method produced it (LLM, rule-based fallback, or user-repaired) so retrieval and audit can distinguish them.
- **FR-030**: Each imported document MUST expose an overall quality indicator derived from the fraction of nodes that passed validation cleanly versus required retry or human repair.

### Key Entities

- **Source Document**: a PDF file in a watched folder. Identified by content hash. May be present, absent, or in a grace-period after removal.
- **Intermediate Representation (IR)**: a hierarchical structured form of one document. Carries document type, title, source pointer, extraction provenance, schema version, and an ordered set of Structure Nodes.
- **Structure Node**: one element of a document's outline (part, chapter, section, sub-clause, footnote, preamble, schedule, etc.). Carries its number, title, text, parent reference, source page, type-specific metadata, amendment annotations, footnote cross-references, and a quality flag (clean / low-confidence / user-repaired).
- **Amendment Annotation**: a record on a Structure Node describing how that node was changed by a later instrument (insertion, substitution, omission, renumbering) and by which instrument/date.
- **Cached IR Record**: a stored Intermediate Representation keyed by source content hash, with a schema version, an optional orphaned-at timestamp, and a reference to the source PDF path(s) that produced it.
- **Repair Edit**: a user modification to one or more Structure Nodes. Persists across re-imports of the same source document.
- **Document Type**: a classifier output that drives which metadata overlay the importer applies (constitution, act, rules, SRO, contract, generic).

## Assumptions

- The user runs the importer on a machine with enough resources to host the local language model; if not, the fallback path (rule-based parser) is acceptable. The local-only constraint is a deliberate v1 choice driven by privacy/cost considerations cited by the user during brainstorming.
- The existing graph builder and indexer downstream of the IR are unchanged and are the correct consumers — this feature ends at "valid IR written"; downstream behavior is out of scope.
- PDF rasterization for the repair view's PDF panel will reuse the same OCR/image-rendering plumbing introduced in feature 004.
- The seven-day orphan grace window is sufficient for the typical accidental-delete / move / re-add cycle; longer retention is not needed in v1.
- Typical structured documents produce cached records under one megabyte each; no per-cache-size or total-cache-size cap is enforced in v1. Storage growth is bounded by the orphan sweep.
- "Low confidence" is a system-derived flag, not a user-set one — users may still edit nodes the system considered confident.

## Out of Scope (v1)

- Cloud-hosted language model option for IR extraction. (Cloud LLMs continue to serve chat; they are not wired into the importer in v1.)
- Bulk-repair tools (edit multiple documents at once).
- Diffing the repaired IR against the original IR for audit.
- User-authored doc-type overlays / custom document types.
- Concurrent multi-document conversion (queued one-at-a-time is acceptable for v1).

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: On the V5 22-prompt eval, **at least 4 of the 6 Sindh PDFs from Feature 004's acceptance corpus (`data/sindh/*.pdf`)** import cleanly enough that questions targeting them return correct answers grounded in correctly-attributed sections — measured against the V5 baseline numbers committed at `2f6a6d77`. Up to 2 permitted misses must each have a written diagnosis attached to the acceptance PR.
- **SC-002**: For documents the importer accepts (i.e. not flagged unprocessable), the indexed structure covers ≥90% of the source PDF's text content (measured as total non-whitespace characters across all node `text` fields divided by total non-whitespace characters extracted from the PDF).
- **SC-003**: For documents the importer accepts, ≥95% of structure nodes pass validation without human repair, **averaged across the Feature 004 Sindh corpus (`data/sindh/*.pdf`, n=6)**. The average is computed as `sum(nodes_ok) / sum(nodes_total)` across all 6 documents — no per-document minimum is enforced (one outlier doc may be carried by the others).
- **SC-004**: A user who imports a document with low-confidence sections sees a notification within five seconds of import completion and can reach the repair view for the first flagged section in no more than two clicks.
- **SC-005**: A repaired section is re-indexed and searchable within ten seconds of the user saving the repair, without the rest of the document being re-embedded.
- **SC-006**: Re-importing a previously-cached PDF (same content) skips the language model entirely and completes in under five seconds end-to-end.
- **SC-007**: Across a corpus of 100 mixed structured PDFs, total cached-IR storage stays under 500 MB without manual cleanup.
- **SC-008**: When the local language model is unavailable, 100% of imports fall through to the rule-based parser successfully and the user is notified exactly once per session about why LLM extraction is disabled.
- **SC-009**: A user-repaired node survives a re-import of the same source document — verified by hash of the repaired node's content matching pre- and post-re-import.
