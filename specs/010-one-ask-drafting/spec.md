# Feature Specification: One-Ask Drafting

**Feature Branch**: `010-one-ask-drafting`
**Created**: 2026-06-11
**Status**: Draft
**Input**: User description: "One-ask drafting: officer types a single natural-language sentence stating what they want and the app generates the complete middle content of a government summary — numbered paragraphs ready to copy-paste into official stationery. Replaces the Feature 009 template/slot system entirely. Citation-grade contract preserved: model writes connective prose only with CITE markers; provisions spliced verbatim deterministically and verify-gated."

## Context & Problem

Government officers (directors, secretaries) spend **weeks to months** writing summaries: researching which rules, sections, and procedures govern their request across many legal documents, then composing a formal summary for approval. Feature 009 attempted to help via uploaded templates with typed slots, an intent Q&A, and document-set curation — but live testing showed per-case slot templates do not generalize (a template built from one real instrument produced 7/8 unfillable "needs a source" slots on a different subject), and the multi-step ceremony placed the burden back on the officer. Officers expect a ChatGPT-like experience: state what you want, receive the finished content.

This feature **replaces** the template/slot drafting flow with a single-ask flow, while preserving the project's primary contract: **100% citation accuracy — the system never invents a rule number, quote, or figure** (refuse-or-flag, never fabricate).

## User Scenarios & Testing *(mandatory)*

### User Story 1 - One sentence in, finished summary content out (Priority: P1)

An officer opens the drafting screen and types one sentence: *"I want to get 4 billion to develop schools/IT institutes for youngsters."* The app researches the indexed legal corpus on its own (which procedures govern development schemes, who has the power to sanction the funds, how the matter is routed) and returns the complete **middle content** of a summary: numbered paragraphs in the standard government shape — Background/Purpose, Legal & Procedural Position (with exact rule/section citations), Financial Aspect, Proposal/Recommendation, Prayer for Approval. The output contains **no letterhead, reference number, addressee, signature block, or CC section** — the officer's official stationery already has those. The officer copies the text and pastes it into their official document.

**Why this priority**: This IS the product promise — replacing weeks of cross-document research and composition with minutes of automated work. Without it nothing else matters.

**Independent Test**: With the Sindh legal corpus indexed, type one funding-request sentence and verify a complete multi-paragraph summary body is produced, every cited rule/section number traceable verbatim to an indexed document, and no boilerplate (letterhead/signature/etc.) present.

**Acceptance Scenarios**:

1. **Given** an indexed legal corpus, **When** the officer submits a single sentence stating a funding request, **Then** the system returns a complete summary body in numbered paragraphs covering background, legal position with citations, financial aspect, recommendation, and prayer — with no further mandatory questions asked.
2. **Given** a generated summary, **When** any rule/section number or quoted provision in it is checked against the indexed corpus, **Then** it matches the source verbatim (zero invented citations).
3. **Given** the officer stated a figure ("4 billion"), **When** the summary is generated, **Then** that figure appears exactly as stated and no other monetary figures are introduced except ones quoted verbatim from sources.
4. **Given** a generated summary, **When** the officer uses the copy action, **Then** the full body text is copied as clean text ready for pasting into their stationery.

---

### User Story 2 - Refuse-or-flag when sources are missing (Priority: P1)

The officer asks for something the indexed corpus cannot support (e.g. a subject no indexed document governs, or a section the corpus lacks). Instead of inventing plausible-looking citations, the system produces the summary with that portion **visibly flagged** ("no source available in the indexed corpus for X") or — if nothing at all is supported — tells the officer plainly that it cannot draft this from the current corpus.

**Why this priority**: The zero-tolerance accuracy contract is the reason this product is trusted over generic chatbots. A single fabricated citation in a document signed by a secretary is a career-level failure. This must hold from day one.

**Independent Test**: Submit a request on a subject absent from the corpus and verify the output contains explicit flags/refusal, and zero fabricated rule numbers or quotes.

**Acceptance Scenarios**:

1. **Given** a request whose legal basis is absent from the corpus, **When** the summary is generated, **Then** unsupported sections carry an explicit inline flag and no invented citation appears anywhere.
2. **Given** a generated summary containing flags, **When** the officer views it, **Then** flagged portions are visually distinct from verified portions, and each verified citation can be traced to its source document.

---

### User Story 3 - Conversational revision until done (Priority: P2)

After the first draft, the officer refines it conversationally: *"make paragraph 3 shorter"*, *"add the procurement angle"*, *"mention the PC-I requirement earlier"*. Each turn revises the targeted part while the rest stays stable; citations remain verified after every revision. The conversation and draft survive an app restart, so the officer can return later and continue.

**Why this priority**: First drafts are rarely final; revision is how the officer reaches sign-off quality without manual rewriting. Valuable, but only after a good first draft exists.

**Independent Test**: Generate a draft, issue a targeted revision command, verify only the addressed portion changed and all citations still verify; restart the app and verify the conversation and draft are restored.

**Acceptance Scenarios**:

1. **Given** a generated draft, **When** the officer sends "make paragraph 3 shorter", **Then** paragraph 3 is shortened, other paragraphs are unchanged, and every citation in the result still verifies against its source.
2. **Given** an in-progress drafting conversation, **When** the app is closed and reopened, **Then** the conversation history and current draft are restored and revision can continue.

---

### User Story 4 - Progress visibility during generation (Priority: P3)

Generation on local hardware takes minutes. While it runs, the officer sees what stage the system is at (researching the corpus → composing → verifying citations) rather than a frozen screen, and can cancel.

**Why this priority**: Minutes-long silent waits read as crashes (this happened in live testing — a 4.7-minute silent turn). Trust requires visible progress, but the feature works without it.

**Independent Test**: Start a generation and verify stage-by-stage progress is displayed and cancel aborts cleanly.

**Acceptance Scenarios**:

1. **Given** a submitted request, **When** generation is running, **Then** the officer sees the current stage and the screen never appears frozen for the duration.
2. **Given** a running generation, **When** the officer cancels, **Then** the system stops promptly and returns to a usable state.

---

### Edge Cases

- **Vague one-liner** (no amount, no clear action): system generates from what is stated, omits the financial-aspect content rather than inventing figures, and may note what the officer can add; it does not interrogate with mandatory questions.
- **Multiple figures in the ask** ("4 billion over 3 years, 200 institutes"): all officer-stated figures must appear verbatim; none altered or dropped silently.
- **Corpus not yet indexed / empty**: clear message that drafting requires an indexed corpus — never a degraded hallucinated draft.
- **Writer model emits a number or quote anyway** (model misbehavior): the verification layer must catch it — the unverifiable token is removed or flagged, never shipped as a citation.
- **Restart mid-generation**: the conversation persists; the incomplete generation is either resumed or cleanly marked failed and re-runnable — never a corrupt half-draft presented as finished.
- **Very large corpus**: the research pass must stay within the responsiveness target (see SC-005) regardless of corpus growth.
- **Mixed instruments later**: this feature targets the Summary instrument; Notes reuse the same engine later (out of scope here).

## Requirements *(mandatory)*

### Functional Requirements

- **FR-001**: The system MUST accept a single natural-language sentence as the only mandatory officer input and produce a complete summary body from it — no mandatory follow-up questions, template selection, or document curation steps.
- **FR-002**: The output MUST be the middle content only: numbered paragraphs with section flow Background/Purpose → Legal & Procedural Position → Financial Aspect → Proposal/Recommendation → Prayer for Approval. It MUST NOT contain letterhead, reference number, dateline, addressee, signature block, or CC/distribution content.
- **FR-003**: The system MUST extract the subject, any officer-stated figures, and the action sought from the officer's sentence, and reproduce officer-stated figures verbatim in the draft.
- **FR-004**: The system MUST autonomously research the whole indexed corpus for governing provisions relevant to the request and collect them as verbatim provision extracts with their true identifiers (rule/section/regulation numbers) before composition begins.
- **FR-005**: The writing engine MUST be restricted to connective narrative: every rule/section/regulation number, quoted provision text, and figure in the final output MUST originate from either (a) a verbatim provision extract or (b) the officer's own stated figures — never from generative text.
- **FR-006**: Every citation in the output MUST pass a deterministic verbatim verification against the indexed source before display; any content that fails verification MUST be removed or visibly flagged — fabricated citations MUST never be displayed unflagged. (Carries forward the Feature 009 verify-gate contract.)
- **FR-007**: Where the corpus contains no support for a needed portion, the system MUST flag that portion inline ("no source available") or refuse with a plain explanation — it MUST NOT substitute invented content.
- **FR-008**: Each verified citation MUST be traceable in the UI to its source document (e.g. hover/click shows source and matched text), and flagged content MUST be visually distinct from verified content.
- **FR-009**: The officer MUST be able to issue free-text revision turns scoped to parts of the draft (shorten, expand, add an angle, reorder emphasis); a revision MUST only change the addressed portion and MUST re-verify all citations in the result.
- **FR-010**: The full conversation and current draft MUST persist across app restarts; an officer can resume drafting where they left off.
- **FR-011**: The officer MUST be able to copy the final body text in one action as clean text suitable for pasting into official stationery.
- **FR-012**: The entire flow MUST run fully offline on the officer's machine; no document content or request text leaves the device.
- **FR-013**: During generation the system MUST display the current stage (researching / composing / verifying) and MUST support cancellation that returns to a clean state.
- **FR-014**: The corpus research step MUST remain responsive as the corpus grows (see SC-005); long silent waits during the research step are a defect. (Replaces the current exhaustive-scan behaviour observed at ~4.7 minutes.)
- **FR-015**: This feature REPLACES the template/slot drafting flow as the officer-facing path: template upload, slot schemas, intent Q&A, and document-set curation MUST NOT appear in the officer's drafting journey. Previously ingested gold instruments MAY be retained internally as style/format examples only.
- **FR-016**: The summary structure (section flow in FR-002) MUST be built into the product and work for any subject — it MUST NOT depend on a per-case uploaded template.

### Key Entities

- **Draft Request**: the officer's one-sentence ask plus what was extracted from it (subject, stated figures, action sought). The root of a drafting session.
- **Provision Extract**: a verbatim passage from an indexed source with its true identifier (rule/section number) and source document reference; the only permitted origin of citations.
- **Summary Draft**: the numbered-paragraph body, where each paragraph is composed of connective narrative plus spliced provision extracts and officer-stated figures; carries per-citation verification status.
- **Citation Flag**: an inline marker on content that could not be verified against the corpus, visible to the officer and distinct from verified citations.
- **Drafting Conversation**: the persisted sequence of officer asks, system replies, drafts, and revisions belonging to one summary.
- **Style Example**: a previously ingested real instrument retained only to inform tone/format of the narrative; never a source of structure slots or citations.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: An officer obtains a complete, copy-paste-ready summary body from exactly one typed sentence, with zero mandatory additional inputs, in 100% of supported-subject test cases.
- **SC-002**: 100% of rule/section numbers and quoted provisions in generated output are verbatim-traceable to an indexed source document; fabricated citations shipped unflagged = 0, across the full evaluation set.
- **SC-003**: End-to-end generation (submit → displayed draft) completes in ≤ 15 minutes on the reference officer machine (CPU-only laptop), and a targeted revision turn completes in ≤ 5 minutes.
- **SC-004**: Drafting effort for a summary drops from weeks of manual research to within one working day including officer review — measured on at least 3 realistic pilot cases with the Sindh corpus.
- **SC-005**: The corpus research stage completes in ≤ 15 seconds on a corpus of at least 10 documents / 100k+ passages, and the officer is never shown a silent, progress-free wait longer than 10 seconds at any stage.
- **SC-006**: In pilot evaluation, at least 80% of generated summaries require no structural rework (paragraph order/sections) before the officer would submit them — only wording-level edits.
- **SC-007**: When tested with at least 5 requests on subjects absent from the corpus, 100% produce explicit flags or refusal and 0 produce invented citations.

## Assumptions

- The Summary instrument is the sole target of this feature; Notes will reuse the engine in a later feature.
- Output language is English (the working language of the Sindh secretariat instruments in the corpus).
- The built-in five-section flow (FR-002) matches standard Sindh summary practice; gold instruments confirm it. Section headings may be implicit (numbered paragraphs) rather than printed headers.
- Officers tolerate minutes-long generation because it replaces weeks of work (explicitly confirmed); responsiveness is handled through progress visibility, not raw speed.
- The existing indexed corpus (the officer's "Index Folder") is the sole knowledge base; indexing documents is a separate existing capability and precondition.
- Existing drafting data (templates, slot schemas, prior sessions) need not be migrated into the new flow; gold instruments are reusable as style examples without conversion by the officer.
- Verification flags are acceptable in a draft shown to the officer; the officer resolves them by editing, supplying facts in revision turns, or indexing more documents.

## Out of Scope

- Letterhead/boilerplate rendering, reference numbers, signature blocks, CC lists (officer stationery provides these).
- Template upload and management UI, slot schemas, slot-level gap panels (superseded).
- Cloud generation of any kind.
- The Note instrument, multi-instrument batch drafting, and export formats beyond copyable text.
- Corpus acquisition/indexing improvements (existing capability; only its responsiveness inside the research pass is in scope via FR-014).
