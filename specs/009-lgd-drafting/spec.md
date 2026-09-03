# Feature Specification: LGD Summary & Note Drafting

**Feature Branch**: `009-lgd-drafting`
**Created**: 2026-06-08
**Status**: Draft
**Input**: User description: Template-driven, citation-grade drafting of official Summaries and Notes for Sindh Local Government officers. The app drafts copy-paste-ready instruments with deterministic citations lifted verbatim from indexed corpus chunks (never model-generated), using officer-uploaded format templates parsed into a slot schema, an intent Q&A, an officer-curated document set, gap detection with index-or-proceed, and a refuse-or-flag gate on any cited slot that cannot be anchored.

## Overview

A Local Government officer must routinely place **Notes** on files and draft **Summaries** for approval up the chain (e.g. a Summary for the Chief Secretary to release ADP funds for a department scheme). These instruments must carry exact citations — rule numbers, article/section numbers, financial thresholds, and verbatim rule text — because a single wrong reference in a document forwarded for approval is a career-level error. This feature lets the officer produce a draft that is correct **by construction**: every citation and quoted rule is lifted verbatim from the officer's own indexed document corpus, never written by a model, and any reference the app cannot anchor to a source is flagged rather than guessed.

The officer supplies the *format* once (uploads a template that the app turns into a reusable structure). All substantive documents — both the standing rules and the case-specific papers — live in the existing indexed corpus. For each draft the app asks a few questions, proposes the documents it will rely on, lets the officer curate that set, points out any required document that is missing, and only then produces the draft.

## Clarifications

### Session 2026-06-08

- Q: Output rendering form for the first cut? → A: Structured, labelled, cited blocks per slot (first cut); layout-faithful document reproduction is a later enhancement.
- Q: Which file formats may an officer upload as a template? → A: The same document formats the existing indexer accepts (PDF, DOCX, TXT, Markdown), reusing the existing extraction pipeline.
- Q: At what granularity does gap detection match a cited slot's required source? → A: By kind/category of source (e.g. "financial-powers rules", "scheme cost / PC-I"), not a specific filename; officer curation resolves the category to the actual document.
- Q: Where does the officer drop a missing required document? → A: Into the existing indexed/corpus folder the app already watches (reuse the current file-watch; the dropped document becomes part of the single-source-of-truth corpus).
- Q: First-cut instrument focus — Summary/Proposal, both, or Noting? → A: **Summary/Proposal first** (the ADP-funds headline scenario). Noting (5 Ps) rides the SAME engine as a second template right after; the `instrument_type` field stays (summary|note) so no code is thrown away. Noting is deferred from the first cut, not removed from the design.
- Q: Is drafting one-shot or iterative? → A: **Iterative.** Drafting a summary is a lifecycle: the app holds a persistent, revisable **draft session** and the officer keeps refining it (swap a source document, re-resolve a cited slot, regenerate/edit a prose section, adjust intent/scheme, add/drop a fact, re-curate the doc set) with a re-render after each change, until they explicitly **finalize**. Every iteration still obeys the citation-grade contract — re-rendered cited slots re-resolve deterministically (a number can never be hand-edited into a citation), regenerated prose stays number-free, unanchored slots stay flagged. Officer free-text edits are allowed on prose blocks only.

### Session 2026-06-09

- Q: How does the officer interact with the drafting flow — a form/panel sequence, or a conversation? → A: **A single continuous conversational session (whole flow).** The officer talks to the app; the app asks one question at a time. Intent gathering, document-set proposal/confirmation, gap reporting/resolution, draft rendering, and refinement all happen in **one chat transcript** (replacing the static IntentForm boxes). The conversation is an interaction layer only — it drives the existing deterministic pipeline by parsing each turn into a structured action and introduces **no new generative authority** over citations, numbers, or facts. Natural UI end-state = chat alongside live draft/doc panels.
- Q: Must the conversation be stored? → A: **Yes — every chat message (officer + system) is persisted durably in the DB**, linked to the draft session, in order, for resume + future retrieval/audit (single source of truth, constitution §IV).
- Q: How is long-conversation context handled so the small-context model isn't fed the whole transcript? → A: **Rolling-summary compaction.** When the dialogue exceeds a configured token threshold, older turns are compacted into a running summary; the model receives only `summary + most-recent turns`, never the full transcript, while all raw messages remain in the DB. Semantic retrieval over stored turns is a deferred later upgrade, not in this cut.

### Session 2026-06-10

- Q: When a required-source category isn't in the confirmed set but IS covered by a document already in the indexed corpus, what should gap detection do? → A: **Two-tier**: a category covered by an un-confirmed corpus document is reported as an **add-this-document suggestion** (one-turn officer accept), not a missing-document gap; only a category covered by nothing in the corpus is a true gap (index or proceed-without).
- Q: How are `fact:` slot gaps (case-specific facts existing in no indexed document) handled? → A: **Officer-supplied channel**: a fact gap prompts the officer in-chat to either point to a case document OR type the value directly; a typed value is rendered under an explicit **"officer-supplied"** attribution, visually distinct from cited content, never dressed as a citation.
- Q: During template ingestion, what happens when the deterministic verbatim-verify gate rejects a model-proposed slot (text/provision not a verbatim substring of the source)? → A: **Flag for review**: the rejected region is surfaced in the schema-confirmation step marked low-confidence with the rejection reason; the officer classifies it manually. The gate never silently drops source content.
- Q: What happens when the ingestion segmentation model is unavailable or its output is unparseable after retries? → A: **Refuse**: ingestion fails with an actionable error naming the missing model; no schema is produced. The heuristic-only fallback is removed (proven to produce garbage schemas on real instruments), and a wrong schema would silently poison every future draft.

## User Scenarios & Testing *(mandatory)*

### User Story 1 - Produce a citation-grade draft from a confirmed set (Priority: P1)

An officer selects a saved template, answers a few intent questions, confirms the document set the app proposes, and receives a Summary or Note in which every cited rule/number is a verbatim, attributed lift from an indexed document, every fact is drawn from the confirmed case documents, and any reference that cannot be anchored is clearly flagged.

**Why this priority**: This is the headline value — the copy-paste-ready, 100%-accurate instrument. Without it the feature delivers nothing; with it alone the officer already gets a usable draft (assuming a template and corpus exist).

**Independent Test**: With a saved template, an indexed corpus, and a confirmed document set, request a draft and verify (a) every citation/number/quoted rule appears verbatim in a source document, (b) no citation or number is present that does not trace to a source, (c) unanchored cited slots are flagged, not filled.

**Acceptance Scenarios**:

1. **Given** a saved template with cited slots whose required sources are all present in the confirmed set, **When** the officer requests a draft, **Then** each cited slot is filled with a verbatim quote and an attribution (document + article/section/rule + page), and each fact slot is filled from the confirmed case documents.
2. **Given** a cited slot whose required source is not present in the confirmed set, **When** the officer requests a draft, **Then** that slot is rendered as an explicit "unverified — source not available" flag rather than a guessed citation, and the rest of the draft still completes.
3. **Given** any citation rendered in the output, **When** it is checked against the corpus, **Then** the cited number and quoted text match the source chunk exactly (no paraphrase, no altered number).
4. **Given** a connective-prose slot, **When** the draft is produced, **Then** the narrative contains no rule/article/section number or citation (numbers and citations appear only in cited slots).

---

### User Story 2 - Set up a reusable template from an uploaded format (Priority: P2)

An officer uploads a format — either a blank skeleton or a real (sanitized) past instrument. The app proposes a structure that classifies every region as fixed boilerplate, a cited slot, or a prose slot, and for each cited slot proposes what kind of source it requires. The officer corrects the proposal once and saves it as a reusable template. Uploaded filled examples are additionally retained as style/gold references.

**Why this priority**: A template must exist before US1 can run, but it is set up occasionally, not per draft. It is independently valuable and testable: the deliverable is a saved, correct slot schema.

**Independent Test**: Upload a blank skeleton and a filled example; verify the app proposes a slot schema for each, the officer can re-classify any region and set/adjust each cited slot's required source, the confirmed schema persists and is reusable, and the filled example is stored as a style/gold reference without entering the retrieval corpus.

**Acceptance Scenarios**:

1. **Given** an uploaded format, **When** ingestion runs, **Then** the app presents a proposed schema marking each region as boilerplate / cited slot / prose slot, with a proposed required-source for each cited slot.
2. **Given** a proposed schema, **When** the officer re-classifies a region or changes a cited slot's required source and confirms, **Then** the corrected schema is saved and available for selection in future drafts.
3. **Given** a filled example is uploaded, **When** ingestion completes, **Then** the example is retained as a style/gold reference and does NOT appear in retrieval results for cited or fact slots.
4. **Given** the structure is derived from the uploaded document, **When** a different format is uploaded, **Then** the proposed schema reflects that document's own structure (no document-specific structure is assumed or hardcoded).

---

### User Story 3 - Curate the document set for a draft (Priority: P2)

After answering the intent questions, the officer is shown the documents the app intends to rely on. The officer adds or removes documents by selection. This confirmed set is what the draft is built from — it both scopes retrieval to the right scheme and keeps the officer accountable for provenance.

**Why this priority**: This is how the officer controls which scheme's facts are used and prevents the wrong scheme's numbers from bleeding in. It is the trust mechanism that makes US1's output forwardable.

**Independent Test**: Run the intent Q&A, verify a proposed document list is shown, verify the officer can add and remove entries, and verify that only the confirmed set is used as source material for the resulting draft.

**Acceptance Scenarios**:

1. **Given** completed intent answers, **When** the proposal step runs, **Then** the officer sees a numbered list of the documents the app proposes to use.
2. **Given** a proposed list, **When** the officer removes or adds a document, **Then** the working set updates accordingly and only the working set is used for drafting.
3. **Given** the officer has not given a manual scope, **When** the proposal is built, **Then** the whole corpus is considered as the candidate pool (no manual scope selection is required).

---

### User Story 4 - Detect and resolve missing required documents (Priority: P3)

When a template's cited slot requires a kind of source that is not present in the confirmed set, the app tells the officer which document is needed and where to put it (a folder path the officer can open). The officer either supplies the document — after which the app notices it has been indexed, updates the set, and tells the officer it will be used — or declares it unavailable and proceeds, leaving the affected slots flagged.

**Why this priority**: This closes the loop that makes the refuse-or-flag gate productive rather than a dead end. It is valuable but depends on US1–US3 being in place.

**Independent Test**: Configure a template whose cited slot requires a source absent from the corpus; verify the app reports the gap with a destination folder, verify that supplying the document there causes the set to update and a confirmation message, and verify that choosing "proceed without" produces a draft with the affected slots flagged.

**Acceptance Scenarios**:

1. **Given** a required-source category covered by an indexed document not in the confirmed set, **When** gap detection runs, **Then** the app suggests adding that document to the set (accepted in one turn) instead of reporting a missing-document gap.
1. **Given** a required source covered by no document in the indexed corpus, **When** gap detection runs, **Then** the app reports the specific missing document and an openable destination location for it.
2. **Given** a reported gap, **When** the officer supplies the document at the indicated location and it becomes indexed, **Then** the app updates the document set and notifies the officer that the new document will be used.
3. **Given** a reported gap, **When** the officer declares the document unavailable and chooses to proceed, **Then** the draft is produced with the affected cited slots flagged as unverified.

---

### User Story 5 - Iteratively refine a draft until finalized (Priority: P1)

The officer treats a draft as a living document: after the first render they request changes — swap a source document, re-resolve a cited slot, regenerate or hand-edit a prose section, adjust the intent/scheme, or add/drop a fact — and the app re-renders the affected parts, holding the working state across the whole session, until the officer explicitly finalizes. Every revision preserves the citation-grade contract.

**Why this priority**: This is how officers actually produce an approvable instrument — the headline value is not a single render but a converged, officer-owned draft. P1 alongside US1 because a one-shot draft is not usable in practice.

**Independent Test**: Render a draft, issue a revision (e.g. swap the rules document feeding a cited slot, or change the scheme), and verify only the affected blocks change, the rest is preserved, the session persists across the change, cited slots re-resolve deterministically, and a finalize action locks the result.

**Acceptance Scenarios**:

1. **Given** a rendered draft, **When** the officer swaps the source document for a cited slot and re-renders, **Then** that cited block re-resolves from the new source (verbatim + citation) while unaffected blocks are unchanged.
2. **Given** a rendered draft, **When** the officer requests regeneration of a prose block, **Then** only that prose block changes and it still contains no number/citation.
3. **Given** a rendered draft, **When** the officer hand-edits a prose block, **Then** the edit is accepted and preserved; the same edit attempted on a cited block is not allowed to alter its number/citation.
4. **Given** an in-progress draft session, **When** the officer finalizes, **Then** the draft is locked as the final output and is exportable; before finalizing, the session remains revisable.

---

### Edge Cases

- A cited slot's required source resolves to multiple candidate documents → the candidates appear in the proposed set for the officer to curate.
- An uploaded format is ambiguous (the app cannot confidently classify a region) → the region is surfaced for the officer's decision rather than silently assigned.
- A filled example's own citations are stale or wrong → the example is used only for structure/style, never as a citation source, so its citations cannot leak into a draft.
- The corpus contains two schemes with similar names → the officer's document-set curation (US3) is the safeguard; facts come only from the confirmed set.
- The newly supplied document fails to index (unreadable/corrupt) → the gap remains open and the affected slots stay flagged; the officer can retry or proceed.
- No template is selected → drafting cannot start; the officer is directed to select or set up a template.
- A draft has one or more flagged slots → the output clearly distinguishes verified content from flagged gaps so the officer never mistakes a flag for a citation.

## Requirements *(mandatory)*

### Functional Requirements

**Instruments & output**

- **FR-001**: System MUST support drafting by instrument type via an `instrument_type` field (`summary` | `note`). **First cut focuses on Summary/Proposal**; Noting reuses the identical engine as a second template and is deferred (see Clarifications 2026-06-08), not removed.
- **FR-002**: System MUST produce a draft as labelled sections corresponding to the template's slots, with cited content rendered as a verbatim quote plus an attribution (source document + article/section/rule identifier + page).
- **FR-003**: System MUST visually distinguish, in the output, verified cited content from flagged (unverified) slots so the two can never be confused.

**Citation-grade contract**

- **FR-004**: System MUST lift every citation, number, and quoted rule/regulation text verbatim from an indexed source chunk; the model MUST NOT generate, alter, or infer any citation, number, or quoted reference.
- **FR-005**: System MUST build each attribution deterministically from the source chunk's metadata (document, article/section/rule, page).
- **FR-006**: System MUST, when a cited slot cannot be anchored to a source chunk in the confirmed set, render that slot as an explicit unverified flag and never substitute a guessed value (refuse-or-flag).
- **FR-007**: System MUST restrict connective-prose slots to narrative that contains no rule/article/section number or citation.
- **FR-008**: System MUST source factual values (amounts, scheme names, dates) only from the case documents in the confirmed set **or from explicitly officer-supplied values (FR-008a)**, and rule/citation content only from the standing-rules documents in the corpus.
- **FR-008a**: System MUST, when a fact slot's value exists in no indexed document, prompt the officer in the conversation to either point to a case document or type the value directly; a typed value MUST be rendered under an explicit "officer-supplied" attribution, visually distinct from cited content, and MUST never be presented as a citation. The model MUST NOT generate or alter officer-supplied values.

**Templates**

- **FR-009**: System MUST accept an uploaded format as either a blank skeleton or a filled example and normalize both into a single internal slot schema.
- **FR-009a**: System MUST accept template uploads in the same document formats the existing indexer supports (PDF, DOCX, TXT, Markdown), reusing the existing text-extraction pipeline to obtain the format's content for classification.
- **FR-010**: System MUST propose, from an uploaded format, a classification of each region as fixed boilerplate, cited slot, or prose slot, deriving the structure from the uploaded document itself without assuming any document-specific structure. Structural boilerplate (letterhead, dateline, addressee block, C.C. list, signature block) MUST NOT be classified as cited slots, and citations MUST be detected inline within body text (including hyphenated forms such as "Rule-6(c)"), not only in headings.
- **FR-010a**: Template ingestion MAY use a local model to propose segmentation and classification (one-time, offline operation — not on the per-draft path), but every model-proposed slot MUST pass a deterministic verbatim-verify gate: a slot's text and provision reference MUST be verbatim substrings of the uploaded source, with provision numbers re-resolved deterministically. A slot that fails the gate MUST be surfaced in the schema-confirmation step as low-confidence with the rejection reason for the officer to classify manually — never silently dropped or silently accepted.
- **FR-010b**: When the ingestion segmentation model is unavailable or its output is unparseable after retries, ingestion MUST fail with an actionable error naming the missing model and produce no schema. There is no heuristic-only fallback (a degraded schema would silently poison every draft built from the template).
- **FR-011**: System MUST, for each cited slot, capture a required-source descriptor that states the KIND/CATEGORY of source needed to fill it (e.g. "financial-powers rules", "scheme cost / PC-I"), not a specific filename.
- **FR-012**: Officers MUST be able to review and correct the proposed schema — re-classify any region and adjust any cited slot's required source — and save the result once as a reusable template.
- **FR-013**: System MUST retain uploaded filled examples as style/gold references and MUST NOT expose them in retrieval results used to fill cited or fact slots.
- **FR-014**: System MUST render fixed-boilerplate regions verbatim in every draft produced from the template.

**Corpus & inputs**

- **FR-015**: System MUST treat the existing indexed corpus as the single source of truth for all substantive documents (standing rules and case documents); the feature MUST NOT introduce a separate ingestion path for substantive documents.
- **FR-016**: System MUST consider the whole corpus as the candidate pool for a draft by default, without requiring the officer to choose a scope manually.

**Drafting flow**

- **FR-017**: System MUST collect the officer's intent through a short series of questions (at minimum: instrument type, addressee/routing, purpose, and the scheme/subject) before proposing documents.
- **FR-018**: System MUST present a proposed document set for the draft and allow the officer to add or remove documents; only the confirmed set MUST be used as source material.
- **FR-019**: System MUST, using the template's required-source descriptors, detect required-source categories not covered by the confirmed set. Coverage MUST be judged against document **content** (semantic match of the category against a document's indexed chunks), not against document titles. Detection searches the whole indexed corpus in two tiers: (a) a category covered by an indexed document not yet in the confirmed set MUST be reported as an add-this-document suggestion the officer can accept in one turn; (b) only a category covered by no indexed document MUST be reported as a missing-document gap, together with an openable destination location — the existing watched corpus/indexed folder — into which the officer can place it.
- **FR-020**: System MUST, when the officer supplies a document at the indicated location and it becomes indexed, update the document set and notify the officer that the document will be used.
- **FR-021**: System MUST allow the officer to declare a missing document unavailable and proceed, producing the draft with the affected cited slots flagged.
- **FR-022**: System MUST require an explicit final confirmation of the document set from the officer before producing the draft.

**Iterative drafting lifecycle**

- **FR-023**: System MUST hold a persistent, revisable draft session for the lifetime of a draft (selected template, intent, confirmed doc set, current rendered blocks, and per-prose officer edits), so the officer can refine across multiple turns until finalizing.
- **FR-024**: System MUST allow targeted revisions without restarting the draft: re-resolve/swap the source of a cited slot, regenerate a prose slot, edit a prose slot's text, change intent fields, and add/remove documents — re-rendering only the affected blocks while preserving the rest.
- **FR-025**: System MUST preserve the citation-grade contract on every revision — re-rendered cited slots re-resolve deterministically from chunk metadata (a number/citation can never be introduced or altered by an officer edit), regenerated prose remains free of numbers/citations, and unanchored cited slots remain flagged. Free-text editing is permitted on prose blocks only.
- **FR-026**: System MUST provide an explicit finalize action that locks the draft as the final output; before finalize the session remains revisable, after finalize the locked output is exportable.

**Conversational drafting session**

- **FR-027**: System MUST conduct the entire drafting flow — intent gathering, document-set proposal/confirmation, gap reporting/resolution, draft rendering, and refinement — as a single continuous conversational session (one transcript), not as separate form/panel steps. This supersedes the form-based interaction of FR-017/FR-018/FR-022/FR-024 while preserving their underlying contracts.
- **FR-028**: System MUST drive every pipeline state change from officer chat turns by parsing each turn into a structured action (set intent field · confirm/add/remove document · resolve/declare-unavailable a gap · request a targeted revision · finalize) mapped to the existing deterministic command path. The conversation is an interaction layer only — it introduces no new generative authority over citations, numbers, or facts (FR-004–FR-008 remain in force).
- **FR-029**: System MUST surface pipeline artifacts (proposed/confirmed document set, gaps, rendered draft blocks, flags) within or alongside the conversation so current state is always visible; the verified-vs-flagged visual distinction (FR-003) MUST hold in the conversational surface.
- **FR-030**: System MUST, when a turn is ambiguous or unparseable, ask a targeted clarifying question rather than guess a pipeline action.

**Conversation persistence & context compaction**

- **FR-031**: System MUST persist every chat message (officer and system turns) of a drafting session durably in the database, associated with the draft session, in creation order, for the lifetime of the session and future retrieval/audit (single source of truth, constitution §IV).
- **FR-032**: System MUST restore a drafting session's full conversation and current pipeline state on resume (e.g. after app restart), consistent with the persistent draft session (FR-023).
- **FR-033**: System MUST bound the conversational context supplied to the model: when the running dialogue exceeds a configured size/token threshold, the System MUST compact older turns into a rolling summary and supply the model only that summary plus the most-recent turns — never the full transcript — while retaining all raw messages in the database (FR-031).
- **FR-034**: System MUST ensure compaction preserves decision-relevant state (current intent fields, confirmed document set, unresolved gaps, finalize status) so summarization never drops information the pipeline depends on.

### Key Entities

- **Template**: A reusable structure derived from an uploaded format. Holds an ordered list of slots and the source format reference.
- **Slot**: A region of a template, of one of three kinds — boilerplate (verbatim copy), cited (verbatim quote + attribution), or prose (bounded narrative). A cited slot additionally carries a required-source descriptor.
- **Required-source descriptor**: A statement of the kind of document/content a cited slot needs (e.g. a financial-powers threshold, a scheme cost document, a concurrence rule), used for gap detection.
- **Gold/style example**: An uploaded filled instrument retained for structure/style reference and evaluation, excluded from the retrieval corpus.
- **Draft session**: The persistent, revisable per-draft working state across the whole lifecycle — selected template, intent answers, candidate and confirmed document sets, detected gaps and resolution choices, the current rendered blocks, per-prose officer edits, and a `finalized` flag. Survives multiple revision turns until finalized.
- **Draft output**: The produced instrument — boilerplate, filled cited slots with attributions, filled fact slots, prose slots, and any flagged unverified slots.
- **Corpus document** (existing): An indexed document, either a standing-rules document or a case document, retrieved to fill cited and fact slots.
- **Chat message**: A single durably-stored turn in a drafting session's conversation — role (officer | system), content, ordering, and whether it has been folded into the rolling summary. Linked to the draft session; every turn is retained.
- **Conversation summary**: The rolling compacted memory of older turns for a draft session, supplied to the model in place of the full transcript, advanced as turns are compacted while raw messages remain stored.

## Assumptions

- **First-cut output rendering** is structured, labelled, cited blocks per slot (reusing the established extractive output shape) — confirmed in clarification. Layout-faithful reproduction of the uploaded format (e.g. a downloadable document matching the original layout) is a deliberate future enhancement and out of scope for the first cut.
- **Filled examples influence style/structure only** — they may inform prose tone and slot ordering but never serve as a citation or fact source.
- **Indexing, folder/file watching, retrieval, and the deterministic citation/refuse-or-flag mechanisms already exist** in the product and are reused; this feature composes them rather than re-implementing them.
- **Gap detection granularity** matches required-source descriptors at the kind-of-document/category level (e.g. "a financial-powers rules document"), not a specific file name — confirmed in clarification. The officer's curation resolves a category to the actual document.
- **The officer is the accountable approver**: human confirmation is required at template setup, document-set curation, gap resolution, and final draft.

## Dependencies

- Existing indexed corpus and its ingestion pipeline (single source of truth).
- Existing folder/file watch capability (to notice newly supplied documents).
- Existing retrieval and deterministic citation/labeling capability and the refuse-or-flag concept.
- User-provided sanitized real Summaries/Notes to serve as template examples and as the evaluation set (not available from public sources).

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: In a draft produced from a confirmed set, 100% of rendered citations, numbers, and quoted rule text match their source documents verbatim (zero generated or altered references).
- **SC-002**: 100% of cited slots that cannot be anchored to a source in the confirmed set are flagged as unverified; zero are filled with a guessed value.
- **SC-003**: An officer can take a produced draft and forward it for approval with no manual correction of any citation or number (measured against a held-out set of real instruments: zero citation corrections required).
- **SC-004**: After a one-time template setup, an officer can produce a complete draft for a new case without uploading any substantive document during the draft (all substantive inputs come from the indexed corpus).
- **SC-005**: When a required document is supplied at the indicated location, the document set reflects it and the officer is notified without restarting the draft.
- **SC-006**: Template setup yields a correct, reusable schema with at most one round of officer correction for a typical format.
- **SC-007**: No uploaded filled example ever appears as a citation or fact source in any produced draft (zero leakage into retrieval results).
