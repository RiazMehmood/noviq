# Feature Specification: Unified Bureaucrat Chat Assistant

**Feature Branch**: `015-bureaucrat-chat` *(spec authored on `013-ingestion-hardening`; promote to its own branch after F013 is committed — see Assumptions)*
**Created**: 2026-06-21
**Status**: Draft
**Input**: User description: "Merge chat + drafting into one conversational, citation-grounded, offline assistant with a professional-bureaucrat persona. ONE path serves ALL purposes: procedural Q&A/guidance, draft Summary/Note, summarize a whole law book, compare 2–3 law/rule/act books, find conflicts between them, view the amendment chain (oldest→recent), and extract action items from documents. Emphasis on context-window management so nothing truncates and every relied-upon law is present in full."

## User Scenarios & Testing *(mandatory)*

### User Story 1 - Grounded guidance, general answer first (Priority: P1)

An officer asks a procedural/legal question in one chat pane (e.g. *"I want long leave — what is the procedure and rules?"*). The assistant, in a professional-bureaucrat voice, immediately returns a **complete general answer**: the standard procedure plus the governing laws, each with a citation drawn from the indexed corpus. It then **offers** to go specific — *"if you tell me your grade, length of service, and leave type, I can give the exact steps and the rules that apply to your case, plus the approval chain."* The officer is never blocked by questions to get a first useful answer.

**Why this priority**: This is the product. Without a grounded general answer on turn one, there is no assistant.

**Independent Test**: Ask a procedural question with the relevant docs indexed; verify a complete cited answer returns on the first turn, followed by an explicit offer to tailor — with no clarifying question gating the first answer.

**Acceptance Scenarios**:

1. **Given** the governing rules are indexed, **When** the officer asks a procedural question, **Then** the assistant returns a general procedure + the governing laws each cited, then offers to tailor — all in one turn.
2. **Given** the officer accepts the offer and states their situation, **When** they send it, **Then** the assistant returns tailored, cited steps including the approval pipeline.
3. **Given** a follow-up turn, **When** the officer refers back ("and for my case?"), **Then** the assistant uses the situation stated earlier without re-asking.

---

### User Story 2 - Citation-grade grounding, never confabulate (Priority: P1)

Every legal reference the assistant states (law name, article/section number, quoted text) maps to a retrieved passage and appears verbatim. When the assistant lacks a grounded source for a claim, it does not invent one — it says so and offers to index the missing document.

**Why this priority**: Officers act on this guidance. A single fabricated section number is a contract-breaking failure (the zero-tolerance citation contract).

**Independent Test**: Pose a question whose answer is NOT in the corpus; verify the assistant flags the gap and offers to index, and never emits a number/quote absent from retrieved context.

**Acceptance Scenarios**:

1. **Given** a question whose governing law is indexed, **When** the assistant answers, **Then** every cited number/quote appears verbatim in a retrieved passage.
2. **Given** a question whose governing law is NOT indexed, **When** the assistant answers, **Then** it states it could not find the source and offers to index the document — and emits no fabricated citation.
3. **Given** a reasoning model that emits internal thinking, **When** it answers, **Then** the thinking is excluded from both the user-facing prose and the citation check.

---

### User Story 3 - No-truncation context guarantee (Priority: P1)

Across a long, multi-turn conversation that mixes guidance, the user's situation, retrieved law, and drafting, the assistant's working context never silently drops a law and never overflows the model's window. Every law the answer relies on is present **in full**; the model never reasons from a half-provision.

**Why this priority**: The user identified this as the central risk. Truncation here = either a dropped/half law (wrong citation) or a forgotten user fact (wrong guidance) — both break the product.

**Independent Test**: Drive a conversation long enough to exceed the window; verify (a) no law appears truncated in the assembled context, (b) the window is never exceeded (no mid-context truncation event), and (c) facts the user stated early are still honored late.

**Acceptance Scenarios**:

1. **Given** retrieved law for a turn, **When** context is assembled, **Then** each included provision is whole — never sliced mid-provision — or omitted entirely.
2. **Given** a tight window, **When** law and history compete, **Then** conversation history yields (is summarized) to keep law complete — never the reverse.
3. **Given** the relevant law exceeds the window, **When** the assistant answers, **Then** it processes the law across multiple passes so every relevant provision is read — none silently dropped.
4. **Given** a long conversation, **When** the window fills, **Then** the assembled prompt size is ≤ the model window before generation, so the runtime truncation path never fires.
5. **Given** the user stated load-bearing facts early ("BS-17, 8 years service"), **When** many turns pass, **Then** those facts remain available verbatim and are not lost to summarization.
6. **Given** context utilization crosses the threshold, **When** the next turn is shown, **Then** the assistant displays a non-blocking notice suggesting a fresh chat with a focused query.
7. **Given** the user starts a new chat from that notice, **When** the new session opens, **Then** it begins with an empty window plus the carried-forward pinned facts and a brief recap.

---

### User Story 4 - Drafting as a conversation outcome (Priority: P2)

When the conversation calls for it (the officer asks for a Summary/Note), the assistant first ensures it understands the situation, then produces a **plain-prose** draft in the same thread, explaining which law supports each part and what approvals/pipeline are required — all cited. The draft can be copied/exported. There is no separate drafting workspace, template, or slot skeleton.

**Why this priority**: High value, but depends on US1/US2 being solid first.

**Independent Test**: In a conversation, ask for a Summary; verify a cited plain-prose draft is produced with supporting laws and the approval pipeline, and can be exported.

**Acceptance Scenarios**:

1. **Given** a gathered situation, **When** the officer asks for a Summary/Note, **Then** the assistant returns a plain-prose draft with each substantive point cited and the approval pipeline stated.
2. **Given** a produced draft, **When** the officer chooses export, **Then** the draft is exported as plain text.
3. **Given** a drafting turn, **When** context is allocated, **Then** a larger generation reserve is set than for a Q&A turn, and the no-truncation guarantee (US3) still holds.

---

### User Story 5 - Hot-swappable model with dynamic window (Priority: P2)

Whatever model file is placed in the models folder is the model used — small, large, or a reasoning model — chosen by file placement alone. The usable context window is derived from that model (and the hardware), not hardcoded, so a larger model actually delivers its larger window.

**Why this priority**: Enables testing different models and the GPU-server upgrade path; correctness does not depend on it (US2 holds on any model).

**Independent Test**: Swap a small model for one with a larger trained window; verify the usable window grows accordingly (subject to hardware), and that correctness/grounding behavior is unchanged.

**Acceptance Scenarios**:

1. **Given** a model file in the folder, **When** the assistant loads, **Then** it uses that model with a window = min(model's trained window, hardware-afforded), honoring an explicit override when set.
2. **Given** a swap to a larger-window model, **When** a long conversation runs, **Then** more law/history fits without violating the no-truncation guarantee.
3. **Given** any model size, **When** answering, **Then** the citation-grade behavior (US2) is identical.

---

### User Story 6 - Summarize a whole law book (Priority: P2)

The officer asks the assistant to summarize an entire act/rule book. The assistant produces a structured, cited summary covering the whole document — processing it in passes because the book exceeds any single window — without dropping sections.

**Why this priority**: High-value, frequently requested; exercises the whole-document map-reduce path that the context guarantees make possible.

**Independent Test**: Ask for a summary of a multi-section indexed act; verify the summary covers all major parts/sections, each point cited, with no section silently skipped.

**Acceptance Scenarios**:

1. **Given** an indexed act larger than the window, **When** the officer asks for its summary, **Then** the assistant returns a structured summary covering all major parts, each point cited to its provision, with no section dropped.
2. **Given** the same request, **When** assembled, **Then** the document is processed across passes (map-reduce) and no provision is truncated.

---

### User Story 7 - Compare books and surface conflicts (Priority: P2)

The officer names 2–3 acts/rules and asks to compare them, or to find conflicts between them. The assistant retrieves the relevant provisions from each named document, aligns them by topic, and returns a comparison — and, for conflicts, surfaces **candidate** contradictions with the provision from **each** side cited, presented for the officer to judge (never asserted as fact).

**Why this priority**: Distinctive, high-value cross-document capability; depends on solid single-document grounding first.

**Independent Test**: Name two indexed acts with a known overlapping topic; verify the comparison aligns provisions and cites both sides, and that conflicts are shown as cited candidates, not unsupported assertions.

**Acceptance Scenarios**:

1. **Given** 2–3 named indexed documents, **When** the officer asks to compare them on a topic, **Then** the assistant returns aligned provisions with each side cited.
2. **Given** the same documents, **When** the officer asks for conflicts, **Then** the assistant surfaces candidate conflicts each citing the conflicting provision from both documents, framed for human judgment.
3. **Given** a topic present in only one document, **When** compared, **Then** the assistant states the absence rather than inventing a counterpart.

---

### User Story 8 - View the amendment chain (Priority: P2)

The officer asks how a provision (or an act) has been amended. The assistant returns the amendment chain ordered oldest→recent — what amended what, when — built deterministically from structured amendment provenance, with the LLM only narrating it. Each link is cited.

**Why this priority**: High-value and citation-grade because it is deterministic (leverages F013 amendment provenance); low confabulation risk.

**Independent Test**: Ask for the amendment history of a provision with known amendments in the corpus; verify the chain is ordered correctly oldest→recent and each link cites its amending instrument — with no LLM-invented dates/order.

**Acceptance Scenarios**:

1. **Given** a provision with recorded amendments, **When** the officer asks its amendment history, **Then** the assistant returns the chain ordered oldest→recent, each link citing the amending instrument.
2. **Given** the chain data, **When** presented, **Then** the order and instruments come from structured provenance (not LLM-generated), and the LLM only narrates them.
3. **Given** a provision with no recorded amendments, **When** asked, **Then** the assistant states there are none rather than inventing a chain.

---

### User Story 9 - Extract action items from documents (Priority: P3)

The officer asks for the action items / obligations / directives in a document (or scope). The assistant extracts them as a cited list — each action item pointing to the provision that mandates it.

**Why this priority**: Useful, but lower frequency; reuses the map-reduce path.

**Independent Test**: Ask for action items from an indexed document containing directives; verify each extracted item cites its source provision and no item is fabricated.

**Acceptance Scenarios**:

1. **Given** a document with directives, **When** the officer asks for action items, **Then** the assistant returns a list where each item cites the provision that mandates it.
2. **Given** a document with no actionable directives, **When** asked, **Then** the assistant says so rather than inventing items.

---

### Edge Cases

- **No relevant law found**: assistant flags the gap and offers to index — never fabricates (US2).
- **Relevant law larger than the window**: multi-pass map-reduce; no provision dropped (US3 #3).
- **Window too small for persona + question + one provision**: assistant degrades by summarizing/omitting history first; if even a single required provision cannot fit, it states the limitation rather than emitting a truncated law.
- **Reasoning model emits `<think>`**: thinking is stripped from user output and from the citation check (US2 #3).
- **Conversation longer than the window many times over**: oldest turns are summarized incrementally; pinned case facts survive verbatim (US3 #5). In parallel, the user is nudged (FR-018a) to start a fresh chat before quality degrades, carrying their pinned facts forward (FR-018b).
- **User restates/changes a pinned fact**: the newer fact supersedes the older pinned fact.
- **Model swapped mid-session**: next turn re-derives the window from the new model; existing summary + pinned facts are reused.
- **Ambiguous document target** (summarize/compare/conflict without naming docs): the assistant confirms which documents before proceeding (FR-005e).
- **Conflict detection on non-conflicting docs**: returns "no candidate conflicts found" rather than manufacturing one.
- **Amendment chain with no structured provenance for the provision**: states none are recorded rather than inventing a chain (FR-005f).
- **Whole-book summarize/compare cost**: map-reduce over a whole act is many passes — acceptable latency on the GPU server, slow on CPU; the deployment surface (Spec 014) absorbs this.
- **Comparison topic present in only one document**: states the absence rather than fabricating a counterpart.

## Requirements *(mandatory)*

### Functional Requirements — Conversation

- **FR-001**: The system MUST present a single conversational surface; there is no separate drafting pane, template, or slot skeleton.
- **FR-002**: The assistant MUST maintain a constant professional-bureaucrat persona across all turns.
- **FR-003**: On every question, the assistant MUST return a complete general answer (procedure + governing laws, each cited) without requiring clarifying input first.
- **FR-004**: After the general answer, the assistant MUST explicitly offer to tailor guidance if the user provides their situation — as an offer, not a precondition.
- **FR-005**: When the user provides their situation, the assistant MUST return tailored, cited guidance including the relevant approval pipeline.

### Functional Requirements — Universal capability path

- **FR-005a**: The single conversational surface MUST serve all capabilities — procedural Q&A/guidance, draft Summary/Note, summarize a whole document, compare 2–3 documents, conflict detection, amendment-chain view, and action-item extraction — with no separate tool, mode, or pane per capability.
- **FR-005b**: The system MUST classify the user's intent from their message and dispatch to the appropriate capability; the user MUST NOT be required to pick a capability/mode explicitly (though they may name it).
- **FR-005c**: Every capability MUST obey the same invariants: the bureaucrat persona, the verbatim citation gate (FR-006), the context-window allocator and no-truncation guarantee (FR-010..FR-018b). Output of every capability MUST be grounded and cited (a summary cites the provisions summarized; a comparison cites both sides; an action item cites its source; a conflict cites both conflicting provisions).
- **FR-005d**: Whole-document and cross-document capabilities (summarize a book, compare, conflict, whole-doc action items) MUST process content via multiple passes (map-reduce) so the entire relevant scope is read; no section may be silently skipped or truncated.
- **FR-005e**: Document-scoped capabilities (summarize/compare/conflict/action-items) MUST operate on the documents the user names; when the target is ambiguous, the assistant MUST confirm which documents before proceeding.
- **FR-005f**: The amendment-chain capability MUST derive the chain (what amended what, ordering oldest→recent, dates, instruments) from structured amendment provenance, NOT from model generation; the model MAY only narrate the deterministic chain.
- **FR-005g**: Conflict detection MUST present candidate conflicts with the conflicting provision cited from each document, framed for human judgment; it MUST NOT assert a conflict as established fact without showing both grounded provisions.

### Functional Requirements — Grounding (citation contract)

- **FR-006**: The assistant MUST NOT state any law name, provision number, or quotation that is not present verbatim in a retrieved passage for that turn.
- **FR-006a**: Every surfaced citation MUST use the single canonical citation format and render the full hierarchical legal path (instrument + identifier + year + Chapter/Part → Section → Sub-section → Clause as applicable) **and the latest amendment status** of the cited provision (constitution §XI/§XII). Where amendment status is unavailable, the citation MUST disclose that rather than omit it silently.
- **FR-007**: When a claim cannot be grounded in retrieved content, the assistant MUST say it could not find the source and offer to index the relevant document, instead of producing the claim.
- **FR-008**: Approval-pipeline guidance MUST be grounded in retrieved documents; if the workflow is not in the corpus, the assistant MUST flag it rather than invent a chain.
- **FR-009**: For reasoning models, internal "thinking" output MUST be excluded from both the user-facing answer and the grounding check.

### Functional Requirements — Context window (no truncation)

- **FR-010**: The system MUST size the model context window dynamically as min(model's trained context length, hardware-afforded), with an explicit environment override for testing.
- **FR-011**: For each turn, the system MUST compute the prompt budget in real token counts (not a character-based estimate) and guarantee the assembled prompt size is ≤ the window BEFORE generation, so the runtime mid-context truncation path never fires.
- **FR-012**: The system MUST allocate the window by fixed priority each turn: (1) generation reserve first, (2) system persona, (3) pinned case facts, (4) current question, (5) retrieved law for this turn, (6) recent turns verbatim, (7) older turns as a running summary.
- **FR-013**: The generation reserve MUST be larger for drafting turns than for question-answer turns.
- **FR-014**: A retrieved provision MUST be included whole or omitted entirely — never truncated mid-provision. (The per-chunk fractional cap that can slice a provision is removed.)
- **FR-015**: When the window is tight, conversation history MUST yield (be summarized) to keep retrieved law complete; law MUST NOT be reduced to preserve history.
- **FR-016**: When all relevant law exceeds the window, the system MUST process the law across multiple passes (map-reduce) so every relevant provision is read; no relevant provision may be silently dropped.
- **FR-017**: When conversation history will not fit, the system MUST fold the oldest turns into a running conversation summary rather than hard-dropping them.
- **FR-018**: When the user states load-bearing situational facts, the system MUST capture them as structured pinned facts kept in an always-present slot, never evicted and never reduced to lossy prose summary; a later restatement supersedes the earlier value.
- **FR-018a**: When context utilization crosses a configurable threshold, the system MUST proactively notify the user that the conversation is getting long and suggest starting a new chat with a focused query — a non-blocking notice that does not interrupt the current turn.
- **FR-018b**: When the user starts a new chat from that notice, the system MUST offer to carry forward the pinned case facts (and a brief recap of the situation) into the new session, so the user need not restate their situation — giving a fresh, empty window while preserving continuity.

### Functional Requirements — Model & lifecycle

- **FR-019**: The system MUST use whichever model file is present in the models folder, selected by file placement, with no code change required to swap models.
- **FR-020**: Citation-grade grounding behavior (FR-006..FR-009) MUST be identical regardless of model size or type.
- **FR-021**: The system MUST persist conversation turns and the running summary so a session survives restart.
- **FR-022**: A produced draft MUST be exportable as plain text.

### Functional Requirements — Retire (scope reduction)

- **FR-023**: The slot/template schema, document-set curation, gap detection, slot-render, and finalize-lock workflows MUST be removed from the user-facing flow; only the verbatim citation gate and export carry forward.

### Key Entities

- **Conversation turn**: a user or assistant message in the single thread; persisted; ordered.
- **Pinned case fact**: a structured, load-bearing fact the user stated about their situation (grade, service length, leave type, etc.); always kept verbatim; supersedable.
- **Conversation summary**: a running, lossy summary of evicted older turns; one compact block always present.
- **Retrieved provision**: a whole unit of law retrieved for a turn; atomic (included whole or omitted); carries its citation.
- **Citation**: the binding between an assistant claim and the verbatim retrieved passage that grounds it.
- **Capability intent**: the classified purpose of a user message (guidance, draft, summarize-doc, compare, conflict, amendment-chain, action-items) used to dispatch within the single path.
- **Amendment link**: a deterministic record (from structured provenance) that an instrument amended a provision on a date; ordered into a chain oldest→recent.
- **Comparison unit**: a topic-aligned pairing of provisions across named documents, each side cited.
- **Conflict candidate**: a surfaced potential contradiction citing both provisions, presented for human judgment (never an asserted fact).
- **Action item**: an extracted directive/obligation from a document, citing the provision that mandates it.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: Zero fabricated citations — across an evaluation set, 100% of stated provision numbers/quotes appear verbatim in that turn's retrieved context (0 confabulations).
- **SC-002**: Zero truncated laws — in 100% of turns, every provision present in the assembled context is whole; no provision appears sliced.
- **SC-003**: Zero window overflows — across a stress conversation that exceeds the window many times over, the assembled prompt is ≤ the window on 100% of turns (0 runtime truncation events).
- **SC-004**: Memory fidelity — load-bearing facts stated early are correctly honored in 100% of later turns within a session (no "forgotten grade").
- **SC-005**: First-answer completeness — 100% of in-corpus questions receive a complete cited general answer on the first turn, with no clarifying-question gate.
- **SC-006**: Honest gaps — for out-of-corpus questions, 100% produce a flag + offer-to-index and 0 fabricated citations.
- **SC-007**: Window scales with model — swapping to a model with an N× larger trained window increases usable context proportionally (subject to hardware), verified by the amount of law/history retained.
- **SC-008**: Proactive reset — when utilization crosses the threshold, the user receives the new-chat notice 100% of the time, and starting from it carries forward the pinned facts with no loss of the user's stated situation.
- **SC-009**: Whole-document coverage — a summary of a multi-section act covers 100% of its major parts/sections with citations; 0 sections silently skipped.
- **SC-010**: Amendment-chain fidelity — chains are ordered correctly oldest→recent and every link's date/instrument matches the structured provenance; 0 model-invented links.
- **SC-011**: Conflicts are cited candidates — 100% of surfaced conflicts cite the provision from both documents; 0 conflicts asserted without both grounded citations.

## Assumptions

- **Spec authored on `013-ingestion-hardening`** to avoid disturbing uncommitted, live-verified F013 fixes; the `specs/015-bureaucrat-chat/` directory is untracked and will carry over when the `015-bureaucrat-chat` branch is created after F013 is committed.
- Deterministic ingestion (Feature 013) is complete; this feature is the assistant layer (retrieval + LLM response over the indexed corpus).
- Drafting is triggered by the user explicitly asking for a Summary/Note within the conversation (not silently inferred).
- "All laws" means **all RELEVANT laws, complete** — the guarantee couples to retrieval precision; it is not "all law in the corpus."
- Recent-turns-kept-verbatim count and exact generation-reserve sizes are tuning parameters with sensible defaults, refined during planning.
- The corpus contains the procedural/approval-workflow documents needed for pipeline guidance (per stakeholder); missing workflows are surfaced via FR-007/FR-008.
- Reuses existing persistence for chat turns and the conversation summary, and existing model auto-discovery.

## Dependencies

- The indexed corpus and retrieval path (Features 011–013).
- The existing verbatim citation gate (from one-ask drafting) re-homed into chat.
- The IBA GPU thin-client server (Spec 014) is the deployment surface; this assistant is designed to be served over HTTP and is the product that server serves.
- The amendment-chain capability depends on F013's structured amendment provenance (`footnote_refs` and the amendment graph / `amended_by` edges); if that provenance is incomplete, the chain reflects only what is recorded (FR-005f / FR-007).
- Intent routing reuses the existing `intent_router` dispatch path.

## Non-Goals

- Role-based access control / multi-user permissions (separate follow-on).
- Letterhead/signature/official-document formatting (output is plain prose).
- Re-introducing deterministic template/slot drafting (abandoned).
- Online/cloud model inference as the default (offline-first).
