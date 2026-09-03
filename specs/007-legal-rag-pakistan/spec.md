# Feature Specification: Pakistan Government Legal RAG

**Feature Branch**: `007-legal-rag-pakistan`
**Created**: 2026-06-02
**Status**: Draft
**Input**: User description: "Pakistan government legal RAG product. Officers drop PDFs of Acts, Ordinances, Rules, Regulations, Policies, and the Constitution. Default indexing is fast (existing 004/005 pipeline, no LLM). On demand, officer triggers Deep re-index with Mistral which runs the legal-IR pipeline, replaces existing chunks, and populates an amendment chain + cross-reference graph + procedure flows. Sticky tier once deep. Single chat window with intent-routed answers across eight intents: Lookup, ListChildren, Summarize, Difference (amendment + instrument), CrossCompare, ActionItems, Timeline (amendment + thematic), Procedure (officer workflow flow). Every answer carries an officer-grade legal citation. Default LLM tier is offline (Qwen 0.5B/3B, Mistral 7B); Groq/Cerebras cloud is eval-only. Primary user: government officer / policy analyst. Out of scope v1: case law, point-in-time queries, foreign jurisdictions, Urdu, audit logs, saved queries. Reference design doc: docs/superpowers/specs/2026-06-02-legal-rag-pakistan-design.md."

**Design reference:** `docs/superpowers/specs/2026-06-02-legal-rag-pakistan-design.md` (architecture, data model, slice plan).

## Clarifications

### Session 2026-06-02

- Q: Chat answer latency target? → A: Median ≤ 5 s, P95 ≤ 15 s on offline-mid; fast-tier intents (Lookup, ListChildren) target ≤ 3 s median.
- Q: Corpus size target for v1? → A: Up to 100 instruments / ~50k chunks per machine (single-department scale).
- Q: Concurrent-user model for v1? → A: Single-officer desktop; one officer per machine at a time. LAN multi-officer deferred to v2.
- Q: Where does the "use cloud LLM" toggle live? → A: Persistent application Settings, not a per-query chip in chat. When on, every query routes cloud; when off (default), every query routes offline. (Updated FR-029.)

## User Scenarios & Testing *(mandatory)*

### User Story 1 — Officer asks plain questions of a freshly uploaded instrument (Priority: P1)

A government officer drops a PDF (an Act, Rules, or Policy) into a folder or notebook. Within seconds, without waiting for any heavy processing, they can open the chat panel and ask natural-language questions about it — "show me the section on penalties", "list the chapters", "summarize Part III", "what duties does this place on the Commissioner". The system answers with a citation that points to the document and the relevant span. This is the **fast path** and covers most everyday queries.

**Why this priority**: This is the entry point for every officer using the product. If fast indexing and basic intent routing do not work end-to-end, nothing else matters. It also delivers the bulk of day-to-day value on its own.

**Independent Test**: An officer uploads one Pakistan government PDF; within 60 seconds the document is queryable; the officer asks one Lookup, one Summarize, one ListChildren, and one ActionItems question and receives correctly-cited answers.

**Acceptance Scenarios**:

1. **Given** a freshly uploaded Pakistan government PDF (Act, Ordinance, Rules, Regulation, or Policy), **When** the officer asks "show me section 6", **Then** the system returns the text of that section with a citation pointing to the instrument and the section.
2. **Given** a freshly uploaded Pakistan government PDF, **When** the officer asks "what chapters are in this Act?", **Then** the system returns an ordered list of chapter titles with citations.
3. **Given** a freshly uploaded Pakistan government PDF, **When** the officer asks "summarize Chapter III", **Then** the system returns a short officer-grade summary anchored to the cited chapter.
4. **Given** a freshly uploaded Pakistan government PDF, **When** the officer asks "what duties does this place on the Commissioner?", **Then** the system returns a grouped list of duties, each citing the section that imposes it.
5. **Given** an uploaded document of a kind that is not in scope (e.g., a research paper), **When** the officer asks a question about it, **Then** the system still answers using its generic retrieval and clearly states which document it is citing.

---

### User Story 2 — Officer requests a deeper view of an important instrument (Priority: P2)

When the officer hits the edges of what fast-path answers can do — they want to see what an amendment changed, compare two versions of a section, or trace an instrument's amendment history — they click "Deep re-index with Mistral" on the document. The system processes the document into a richer legal structure (instrument → chapter → section → sub-section → clause → sub-clause), captures amendment events (insertion, substitution, repeal), and re-builds the searchable chunks. From that point on, the document supports the deeper question types and offers more precise citations. The deep state is **sticky**: re-uploading the same content does not lose it.

**Why this priority**: Officers need the deeper view selectively, on the documents they actually rely on, without paying the processing cost on every document. This story makes the upgrade explicit, controlled, and durable.

**Independent Test**: An officer triggers Deep re-index on a previously fast-indexed Act; progress is visible; once it completes, the same Difference and Timeline questions that previously returned an upgrade prompt now return correct answers with full hierarchical citations (Section → Sub-section → Clause).

**Acceptance Scenarios**:

1. **Given** a fast-indexed Act, **When** the officer clicks "Deep re-index with Mistral", **Then** progress is reported in real time and the operation is cancellable without corrupting the document's existing fast-indexed state.
2. **Given** a deep-indexed Act, **When** the officer asks "what changed in section 6 after the 2021 amendment?", **Then** the system returns a side-by-side or annotated view showing the prior text, the amending instrument, and the current text — each cited.
3. **Given** a deep-indexed Act, **When** the officer asks "show the amendment history of section 12", **Then** the system returns a chronological list of amendment events with the amending instrument cited at each step.
4. **Given** a deep-indexed Act, **When** the officer re-uploads the same PDF, **Then** the document remains deep-indexed (sticky) and re-processing reuses cached intermediate results where possible.
5. **Given** a fast-indexed Act, **When** the officer asks a question that requires the deep view, **Then** the system gives a best-effort answer where possible and surfaces a clear prompt to deep-index for a precise answer.

---

### User Story 3 — Officer plans an approval path that spans multiple instruments (Priority: P3)

An officer preparing to act on an application — granting a license, approving a building plan, processing an RTI request — needs to know the procedural path the matter must follow according to the law and the rules made under it. They ask the chat "what is the approval flow for X?" and get back an ordered sequence of actors and actions (Applicant → SHO → DPO → DC → Home Department → License issued), with each step citing the section or rule that mandates it. They can also ask for cross-comparisons across instruments ("how do Acts X, Y, Z handle penalties?"). For this, the relevant instruments must be deep-indexed and the cross-references between them must be captured.

**Why this priority**: This is the unique value an officer cannot easily get from reading PDFs. It is also the most demanding story technically (cross-document graph, procedural extraction), so it ships last.

**Independent Test**: With at least one Act + its Rules + an optional policy all deep-indexed, an officer asks for the approval flow of a real procedure (e.g., arms license) and the system returns an ordered actor-step path stitched across the Act and the Rules, each step citing its source.

**Acceptance Scenarios**:

1. **Given** an Act and its Rules are both deep-indexed, **When** the officer asks "what is the approval flow for [process]?", **Then** the system returns an ordered, linear path of actor → action steps with a citation per step.
2. **Given** the same setup, **When** a step has conditional branching ("if amount > 5 lakh, escalate to Secretary"), **Then** the linear render includes an inline conditional note and the officer can request a branching view on demand.
3. **Given** three related instruments are deep-indexed, **When** the officer asks "compare how Acts X, Y, Z handle penalties", **Then** the system returns a structured side-by-side comparison with citations from each instrument.
4. **Given** instruments are only fast-indexed, **When** the officer asks a procedure or cross-comparison question, **Then** the system clearly states the limitation and offers to deep-index the needed instruments.

---

### Edge Cases

- An uploaded PDF is image-only or scanned — fast indexing falls back to OCR (existing capability) before chunking.
- The classifier cannot determine whether a deep-index target is actually a Pakistan government legal instrument — the system asks the officer to confirm before running the expensive pipeline.
- An amendment marker in the source PDF uses a form the parser does not recognize — the section is still indexed; the amendment is logged as "unparsed marker" with the raw text preserved so it can be inspected.
- An officer cancels a Deep re-index halfway through — the document remains in its prior state (fast or previously-deep); no partial deep state is left visible.
- A query matches content across multiple instruments under the active scope — the citation block lists each contributing instrument separately.
- The officer narrows scope to a single document and asks a cross-comparison question — the system explains that cross-compare requires at least two in-scope instruments.
- An officer asks a question in the chat that does not fit any of the eight intents — the system answers using a generic retrieval-and-summarize fallback and notes that no specialized intent applied.

## Requirements *(mandatory)*

### Functional Requirements

#### Ingestion & indexing tiers

- **FR-001**: The system MUST accept PDF uploads of Pakistan government instruments (Constitution, Acts, Ordinances, Rules, Regulations, Policies) into either a folder or a notebook.
- **FR-002**: On upload, the system MUST index each document via the fast path by default — extracting text (including OCR fallback for scanned/image PDFs), chunking, embedding, and full-text indexing — without running any LLM IR build.
- **FR-003**: The system MUST tag every indexed document with an index tier (`fast` or `deep`) that is visible to the officer.
- **FR-004**: The system MUST expose a per-document "Deep re-index" action that triggers the LLM-driven legal-IR pipeline for that one document only.
- **FR-005**: Deep re-index MUST report progress in real time, be cancellable, and on cancellation MUST leave the document in its prior tier with no partial deep state visible.
- **FR-006**: On successful Deep re-index, the system MUST atomically replace the document's fast-path chunks with deep-derived chunks and persist the legal IR for reuse.
- **FR-007**: The deep tier MUST be sticky: a subsequent re-upload of the same content MUST NOT silently downgrade the document to fast; the system MUST reuse cached deep intermediate results where the content is unchanged.
- **FR-008**: Downgrading a document from deep back to fast MUST require explicit officer action and MUST remove the deep-only artifacts for that document.

#### Legal structure (deep tier only)

- **FR-009**: For deep-indexed instruments, the system MUST extract a hierarchical structure covering Instrument → Chapter/Part → Section/Article → Sub-section → Clause → Sub-clause, plus Preamble, Proviso, Explanation, Illustration, Schedule, Schedule Item, and Definition where present.
- **FR-010**: For each section the system MUST extract an amendment chain capturing insertion, substitution, repeal, and omission events, the amending instrument, and the marker text from the source.
- **FR-011**: The system MUST extract typed cross-references between nodes, distinguishing at least: a node citing another node, Rules implementing a section of an Act, an instrument operating under another, and an instrument amending another.
- **FR-012**: For sections identified as procedural, the system MUST extract a process flow of ordered steps with actor, action, optional condition, optional timeline, and a citation back to the source section/rule.

#### Chat: intent-routed answers

- **FR-013**: The system MUST present a single chat window — there MUST NOT be separate UI tabs or mode toggles for different question types.
- **FR-014**: The system MUST classify every officer query into one of eight intents: Lookup, ListChildren, Summarize, Difference, CrossCompare, ActionItems, Timeline, Procedure — with a Fallback handler for unclassifiable queries.
- **FR-015**: When intent classification confidence is low, the system MUST default to the Fallback handler and offer the officer suggestions for what intent they may have meant.
- **FR-016**: Each intent handler MUST render its answer in a uniform chat-bubble shape: headline, body, citation block, and optional "see also" related-instruments list.
- **FR-017**: When a query requires the deep tier on a document that is currently fast-indexed, the system MUST give a best-effort answer where possible AND surface a clear prompt to Deep re-index for a precise answer.
- **FR-018**: The Procedure handler MUST render flows linearly by default with conditional notes inline, and MUST expand to a branching view on explicit officer request.
- **FR-019**: The Difference handler MUST support both an amendment diff (same section before vs after) and an instrument diff (two related instruments).
- **FR-020**: The Timeline handler MUST support both a single-section amendment timeline and a thematic timeline of instruments ordered by enactment date.

#### Citations

- **FR-021**: Every chat answer MUST carry at least one citation; the system MUST NOT produce paraphrased legal content without a citation.
- **FR-022**: For deep-indexed sources the citation MUST render the full hierarchical legal path (instrument name + identifier + year + Chapter/Part → Section → Sub-section → Clause → Sub-clause, as applicable) and the latest amendment status.
- **FR-023**: For fast-indexed sources the citation MUST render the instrument name + page or span reference, and MUST clearly indicate that hierarchical detail is unavailable on the fast tier.
- **FR-024**: A single canonical citation format MUST be used wherever citations appear (chat, "see also", exports, copy-as-note).

#### Scope picker

- **FR-025**: The system MUST respect the active scope on every query. Scope choices: Both (default, folder + notebook), folder only, a specific folder document, notebook only, or a specific notebook document.
- **FR-026**: When a query intent requires multiple in-scope instruments (e.g., CrossCompare) and the scope contains fewer, the handler MUST tell the officer what the scope provides and what is missing.

#### LLM tiering and offline-first

- **FR-027**: The system MUST run as default in an offline LLM mode using locally available models; it MUST function with no internet access.
- **FR-028**: Cloud LLM providers MUST be available only for evaluation and regression testing, and MUST never be used as the default production path.
- **FR-029**: A persistent "use cloud LLM" toggle MUST live in application Settings (not in the chat surface itself). When **off** (the default), every query routes to the offline LLM tier. When **on**, every query routes to the cloud tier. Toggling the setting MUST be an explicit officer action; cloud usage MUST never happen silently regardless of toggle state at any earlier point.

#### Out-of-scope behaviors v1

- **FR-030**: The system MUST NOT attempt point-in-time reconstruction of repealed text ("as of date X" queries).
- **FR-031**: The system MUST NOT ingest or retrieve content in languages other than English in v1 (Urdu support deferred).
- **FR-032**: The system MUST NOT extract or attempt to answer questions about case law, judgments, foreign jurisdictions, or private legal practice content.
- **FR-033**: The v1 product MUST be designed for **single-officer use per machine** — one officer interacting with one running instance at a time. LAN multi-officer deployment is out of scope for v1 (a v2 follow-up).

### Key Entities

- **Instrument** — One Pakistan government legal document: Constitution, Act, Ordinance, Rules, Regulation, or Policy. Has a title, an official identifier (where applicable), a year, an index tier (`fast`/`deep`), and an originating source PDF.
- **Section / hierarchical node** — A unit within an instrument (Chapter, Part, Section, Article, Sub-section, Clause, Sub-clause, Proviso, Explanation, Illustration, Schedule, Schedule Item, Definition, Preamble, or Procedure). Carries its position in the hierarchy, its current text, an in-force flag, and a source-page reference. Present only when the instrument is deep-indexed.
- **Amendment event** — An insertion, substitution, repeal, or omission applied to one node. Carries the amending instrument identifier, the optional effective date, and the marker text from the source. Present only on deep-indexed nodes.
- **Cross-reference edge** — A typed relationship between two nodes: *cites*, *implements*, *under*, or *amends*. Present only between deep-indexed nodes.
- **Procedure** — A process flow attached to a section, with a process name and an ordered list of steps. Present only on deep-indexed instruments.
- **Procedure step** — One step in a procedure: an order, an actor (e.g., Director, Secretary, Town Officer, Municipal Officer, SHO, DPO, DC, Commissioner — free-text in v1), an action, an optional condition, an optional branching parent, an optional timeline, and a citation back to the source section/rule.
- **Officer** — The primary user; a government officer or policy analyst. Defaults (citation form, answer length, export shape, multi-turn behavior) are tuned for this user.
- **Citation** — The officer-grade rendering of a reference: instrument title, identifier and year (where applicable), hierarchical path on the deep tier or page/span on the fast tier, and amendment status when applicable.
- **Chat query** — A single natural-language officer question, carrying the active scope, classified to one of the eight intents (or Fallback), and answered by the corresponding handler.

### Assumptions

- The fast-path pipeline (text extraction, OCR fallback, chunking, embedding, FTS) already exists in the product and is reused as-is.
- A locally available LLM stack capable of running grammar-constrained generation (used internally for IR building on the deep tier) is available on the deployment machine. Specific model choices are an implementation concern and are documented in the design reference.
- Officers operate on documents they have authority to view; the product does not impose its own access control beyond the existing scope picker.
- Pakistan government instruments published electronically use the standard amendment-marker conventions (`X[…]`, "[Substituted by Act …]", "[Inserted by Ordinance …]", "[Omitted by …]") that the system targets. Unrecognized variants are tolerated as "unparsed marker" rather than failing the document.
- Network connectivity is not assumed to be present at runtime; the product is designed to run fully offline.
- The deployment-class machine is a CPU-class workstation (no required GPU) capable of running the offline LLM tier (small/mid models). GPU acceleration, where present, is opportunistically used but not required to meet the v1 success criteria.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: An officer can upload a Pakistan government PDF and ask their first useful chat question within **60 seconds** of upload (fast-path indexing completes inside that window for documents up to 200 pages).
- **SC-002**: For fast-indexed instruments, **at least 90%** of Lookup, ListChildren, Summarize, and ActionItems queries in a representative officer eval set return an answer with a correct citation to the right instrument and page or span.
- **SC-003**: Deep re-index reports progress at least **every 10 seconds**, is cancellable within **5 seconds** of the officer pressing cancel, and on cancellation leaves the document in its prior tier with no visible corruption.
- **SC-004**: For deep-indexed instruments, **at least 95%** of Lookup queries return a citation that renders the full hierarchical legal path correctly (Chapter → Section → Sub-section → Clause where applicable).
- **SC-005**: For deep-indexed instruments, the amendment chain captures **at least 90%** of insertions, substitutions, and repeals visibly marked in the source PDF, measured against a hand-built golden index.
- **SC-006**: On a representative cross-document set (one Act + its Rules + a related Policy), **at least 70%** of Procedure queries return a flow with all key steps, correct actors, and a correct citation per step.
- **SC-007**: The product runs end-to-end with **no network access**: officers can upload, index (fast and deep), and answer queries across all eight intents without any cloud call.
- **SC-008**: Officers report that the citation accompanying a deep-tier answer is precise enough to use directly in an official note **at least 90%** of the time, measured via a structured review of a sample of answers.
- **SC-009**: Switching the active scope (Both → folder → specific document → notebook → specific notebook document) changes which instruments are searched within **2 seconds** and is visible to the officer in the chat surface.
- **SC-010**: When an intent requires the deep tier and the target document is fast-indexed, the upgrade prompt is shown in **100%** of such cases; no query silently returns a poor answer where a deep upgrade would help.
- **SC-011**: Chat answers return within these latency targets on the default offline LLM tier: fast-tier intents (Lookup, ListChildren) — **median ≤ 3 seconds**; all other intents — **median ≤ 5 seconds, P95 ≤ 15 seconds**, measured over a representative officer eval set on the deployment-class machine.
- **SC-012**: The product supports a per-machine corpus of **up to 100 instruments and approximately 50,000 chunks** (single-department scale) without breaching the latency targets in SC-001, SC-009, and SC-011. Larger corpora may work but are not a v1 commitment.
