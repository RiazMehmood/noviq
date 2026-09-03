# Feature Specification: Notebook & Knowledge Engine

**Feature Branch**: `001-notebook-knowledge-engine`
**Created**: 2026-04-13
**Status**: Draft
**Input**: User description: "App 1 - Notebook and Knowledge Engine"

## User Scenarios & Testing *(mandatory)*

### User Story 1 - Create Notebook and Chat with Documents (Priority: P1)

A user creates a named notebook, adds one or more local files (PDFs, Word docs,
spreadsheets, images, text files, code), and immediately begins asking questions.
The system answers exclusively from the content of those files and always shows
which document and section each answer came from. Follow-up questions stay grounded
in the same sources throughout the session.

**Why this priority**: This is the foundational capability. Without it, no other feature
has value. A user who can add files and get cited answers has immediate, standalone
value — the entire product's promise in its simplest form.

**Independent Test**: Load 3 PDFs into a new notebook. Ask "What are the main
conclusions across these documents?" Verify: answer is generated, cites specific
documents and sections, is grounded in content (not hallucinated), completes within
acceptable time on minimum hardware.

**Acceptance Scenarios**:

1. **Given** a new empty notebook, **When** the user adds a 50-page PDF, **Then**
   the document is processed and a "ready to chat" indicator appears within 60 seconds.
2. **Given** a notebook with 3 documents, **When** the user asks a question, **Then**
   the answer includes inline citations referencing the source document and section.
3. **Given** a notebook, **When** the user asks something not covered by any source,
   **Then** the system clearly states no relevant content was found rather than fabricating.
4. **Given** an active chat session, **When** the user asks a follow-up question,
   **Then** the system maintains context from the prior exchange within the session.
5. **Given** a notebook with an image file on capable hardware (16 GB+), **When**
   the user asks about visual content in the image, **Then** the system answers
   based on what the image contains. On constrained hardware (8 GB) where the
   vision model is unavailable, **Then** the system displays a clear capability
   notice explaining that image analysis requires more RAM, and marks the image
   source as unanalyzed rather than silently failing.

---

### User Story 2 - Semantic Folder Indexing and Global Recall (Priority: P2)

A user grants the app read access to one or more local folders (e.g., Documents,
a project folder). The system indexes these folders silently in the background. The
user can then search across ALL indexed content using natural language — meaning-based,
not keyword-only — and get results with file references. This works across thousands
of files and surfaces content even when the exact words aren't present.

**Why this priority**: This transforms EchoAI from a per-notebook tool into an
always-on knowledge layer over the user's entire file system. The value compounds
with every file the user already has. It is the "private Recall" feature that no
cloud service can match.

**Independent Test**: Index a folder with 50+ mixed files. Ask "Find everything
related to marketing budgets." Verify: relevant files surface even without the
literal phrase present; results include file name and excerpt; no noticeable
CPU/fan impact during active use while indexing is running.

**Acceptance Scenarios**:

1. **Given** a folder added for indexing, **When** indexing completes, **Then**
   its contents are searchable from the global search bar via natural language.
2. **Given** the user is actively using the app, **When** background indexing runs,
   **Then** the user experiences no UI lag and hears no sustained fan activity.
3. **Given** a folder with 1,000 files, **When** the user performs a semantic search,
   **Then** results appear within 3 seconds on minimum supported hardware.
4. **Given** indexed content, **When** the user searches, **Then** each result shows
   the file path, last-modified date, and a relevant excerpt from the file.
5. **Given** the user revokes a folder's access, **When** revocation is confirmed,
   **Then** all data derived from that folder is removed within 5 seconds.

---

### User Story 3 - Multi-Source Deep Analysis (Priority: P3)

Within a notebook, a user can run structured analysis across multiple documents
simultaneously using one-click templates: compare sources side by side, extract all
action items, find contradictions between documents, generate a timeline of events,
or produce a summary table. The output cites every claim back to its source passage.

**Why this priority**: This moves from "I asked a question" to "the AI did the work
for me." A legal analyst comparing 5 contracts, a marketer synthesizing 3 campaign
briefs — this is where hours of manual work become minutes. It compounds P1 value
without requiring any new architecture.

**Independent Test**: Add 3 quarterly reports to a notebook. Run "Compare Key Metrics."
Verify: output table shows figures from all 3 reports side by side, each cell cites
its source, no figures are fabricated or misattributed.

**Acceptance Scenarios**:

1. **Given** a notebook with 2+ documents, **When** the user selects an analysis
   template, **Then** the output is produced with citations for all contributing sources.
2. **Given** a comparison output with a data point, **When** the user clicks that point,
   **Then** the app highlights the originating passage in the source document.
3. **Given** two sources that contradict each other, **When** "Find Contradictions" runs,
   **Then** both contradicting passages are shown side by side with their source references.
4. **Given** an analysis result, **When** the user exports it, **Then** a formatted file
   (PDF or plain text) is saved to a user-chosen local path.

---

### User Story 4 - Knowledge Graph and Structured Entity Queries (Priority: P4)

After indexing, the system automatically extracts named entities — people,
organizations, project names, dates, decisions, monetary amounts — and stores them
in a structured, queryable form separate from the document text. The user can query
this structured layer directly: "What decisions were made about Project X?", "All
invoices over $10,000", "Every meeting involving Sarah in Q1 2025." Results return
instantly without re-reading documents. A visual graph shows entity relationships.

**Why this priority**: Structured entity queries are faster and more reliable than
asking the AI to rediscover the same facts repeatedly. This is the "office memory"
layer — EchoAI doesn't just search documents; it knows the user's business entities.

**Independent Test**: Index 20 meeting notes. Ask "What decisions were made about
the product launch?" Verify: specific decisions returned with source file and date,
result arrives in under 1 second, no document re-analysis is triggered.

**Acceptance Scenarios**:

1. **Given** indexed documents, **When** the user queries a named entity or date range,
   **Then** results return in under 1 second without re-analyzing source documents.
2. **Given** meeting notes, **When** the user asks "What was decided about [project]?",
   **Then** a list of decisions with source file and date context is returned.
3. **Given** entity extraction has completed, **When** the user opens the Knowledge
   Graph view, **Then** a navigable visual map of entities and relationships is shown.
4. **Given** an entity in the graph, **When** the user clicks it, **Then** all source
   documents referencing that entity are listed with relevant excerpts.

---

### User Story 5 - Batch Processing on File Collections (Priority: P5)

A user points at a folder and applies a processing operation to all files inside it:
"Summarize every contract in this folder", "Extract all payment deadlines from
Invoices/", "Flag any document mentioning a penalty clause." The system processes
each file and delivers a unified output — a summary table, a flagged list, a combined
report — entirely in the background. The user is notified when done.

**Why this priority**: Batch processing is the multiplier. A single user can process
the work that would take a team of people hours. It requires all lower-priority stories
to be functional first, but dramatically raises the product's professional value.

**Independent Test**: Point at a folder with 20 mixed PDFs. Trigger "Summarize all —
one paragraph each." Verify: one entry per file in the output, each summary is
accurate, processing runs in the background, notification appears on completion.

**Acceptance Scenarios**:

1. **Given** a folder selected for batch processing, **When** the user defines an
   operation, **Then** the system processes all compatible files and delivers a
   unified results view.
2. **Given** a batch job running, **When** the user navigates away and continues
   other work, **Then** processing continues in the background without interruption.
3. **Given** a completed batch run, **When** the user views results, **Then** each
   result is linked to its source file and can be opened directly.
4. **Given** a batch job with one unreadable file, **When** processing continues,
   **Then** the failed file is individually flagged with an error reason and all
   other files complete normally.

---

### Edge Cases

- What happens when a document is corrupted, password-protected, or in an unsupported
  format? → File is flagged with a clear reason; all other files continue processing.
- What happens when a folder contains more than 10,000 files? → Indexing queues
  incrementally with a progress indicator; UI remains responsive throughout.
- What happens when a file is modified after indexing? → A filesystem watcher detects
  the change and re-indexes that file automatically without user action.
- What happens when the user asks a question spanning both a notebook and global
  indexed folders? → System clearly distinguishes notebook-scoped answers from
  global recall results in the response.
- What happens when storage drops below 1 GB? → Indexing pauses with a clear
  notification; the existing index is not corrupted.
- What happens when a source document contains mostly images (scanned PDF)? → The
  system performs visual content analysis on each page before indexing.
- What happens when two indexed files have identical content? → Deduplication
  prevents redundant storage; both file paths are retained as sources.
- What happens when the same file appears in both a notebook and a globally
  indexed folder? → Path-based deduplication: one index entry per unique file
  path is maintained and shared across both contexts. The file is not indexed
  twice. Results from both notebook chat and global recall draw from the same
  single index entry.

## Requirements *(mandatory)*

### Functional Requirements

**Notebook Management**

- **FR-001**: Users MUST be able to create, rename, and delete named notebooks.
- **FR-002**: Each notebook MUST support an unlimited number of local source files.
- **FR-003**: The system MUST accept these source formats: PDF, DOCX, XLSX, PPTX,
  TXT, MD, PNG, JPG/JPEG, MP3, MP4, and common code file extensions.
- **FR-004**: Users MUST be able to add sources via drag-and-drop, file picker dialog,
  or folder selection (all files in folder added as sources).
- **FR-005**: Notebooks MUST record when each source was added; users MUST be able
  to view and remove individual sources.

**Document Processing and Indexing**

- **FR-006**: The system MUST process each source file into a searchable, queryable
  form upon addition; processing status MUST be visible to the user. Each source
  MUST display its current lifecycle state: pending, processing, indexed, or failed.
  Failed sources MUST show the error reason and offer a manual retry action.
- **FR-007**: Audio and video source files MUST be transcribed to text before indexing.
- **FR-008**: Image files and image-heavy documents MUST have visual content analyzed
  and described as part of their indexed representation.
- **FR-009**: All indexing and processing MUST run in the background and MUST NOT
  block user interaction with the app.
- **FR-010**: The system MUST watch indexed folders for file changes and re-index
  modified or newly added files automatically.
- **FR-011**: Background indexing MUST pause when any of these conditions are met:
  the user is actively interacting with the app, system CPU usage exceeds 40%,
  or battery level drops below 20% on battery power. It MUST prefer to run
  during system idle on AC power and MUST resume automatically when conditions allow.

**Query and Chat**

- **FR-012**: Users MUST be able to ask natural language questions within a notebook
  and receive answers grounded exclusively in that notebook's sources.
- **FR-013**: Every answer MUST include citations identifying the source document
  and relevant section or passage.
- **FR-014**: The system MUST escalate query answering through tiers in strict order:
  semantic cache hit → exact match → semantic search → AI synthesis.
  Each tier MUST be exhausted before escalating to the next; the cheapest
  satisfying tier determines the response.
- **FR-015**: The system MUST clearly state when a question cannot be answered from
  available sources; it MUST NOT generate unsupported answers.
- **FR-016**: Chat history within a session MUST be maintained so follow-up questions
  retain prior context.
- **FR-017**: A semantic cache MUST prevent re-running inference for queries with
  the same meaning. The cache operates at two levels: within the current session
  (in-memory, instant) and across sessions within the last 7 days (persisted,
  sub-second). A query whose meaning matches a cached answer within a cosine
  similarity threshold MUST return the cached answer without invoking any model.

- **FR-039**: When both FTS5 (keyword) and sqlite-vec (semantic) retrieval produce
  results for the same query, the system MUST merge and re-rank them using
  Reciprocal Rank Fusion (RRF, k=60) before any LLM context is built. The unified
  RRF-ranked list MUST be the sole input to context chunk selection. Using only one
  retrieval method's ranking for LLM context is PROHIBITED.

- **FR-040**: A context builder MUST select the top 5–8 highest-ranked unique chunks
  from the RRF-merged list (K is configurable in `config.toml`; default 8). The
  builder MUST deduplicate by chunk_id (a chunk that ranks in both FTS5 and vec
  results MUST appear only once), and MUST enforce that the total context token count
  fits within the active model's context window minus a 512-token overhead reserved
  for the system prompt and response. Source metadata (file name, page number,
  chunk_id) MUST be preserved in the context and passed to the LLM for citation.

- **FR-041**: The LLM MUST be invoked with a system prompt that explicitly enforces
  all four grounding rules: (1) answer exclusively from the provided context chunks,
  (2) never use prior training knowledge for factual claims, (3) respond with the
  exact phrase "No relevant information found in provided documents" when context is
  insufficient, and (4) include an inline citation `[source_name, page N]` for every
  factual claim. The system prompt MUST be a versioned constant — not dynamically
  generated — so its grounding behavior is testable and auditable.

- **FR-042**: A post-processing grounding validator MUST run after every LLM response.
  The validator MUST: (a) check that each inline citation maps to a chunk_id present
  in the retrieved context set, (b) detect uncited factual sentences (sentences with
  factual content but no citation marker), and (c) replace the full response with the
  standard fallback "No relevant information found in provided documents" if validation
  fails. The validator MUST log its result (grounding_score, citation_count,
  fallback_triggered) to the audit_log. The validator MUST be rule-based and MUST NOT
  invoke any LLM.

**Semantic Recall**

- **FR-018**: Users MUST be able to grant read-only access to local folders for
  global background indexing.
- **FR-019**: A global search bar MUST support semantic queries across all indexed
  folders and notebooks.
- **FR-020**: Global search results MUST display source file path, last-modified
  date, and a relevant excerpt for each match.
- **FR-021**: Users MUST be able to revoke folder access at any time; revocation
  MUST immediately and permanently remove all data derived from that folder.

**Knowledge Graph**

- **FR-022**: The system MUST automatically extract named entities (people,
  organizations, dates, project names, decisions, monetary amounts) from all
  indexed content during the indexing pass.
- **FR-023**: Users MUST be able to query extracted entities directly using natural
  language (e.g., "All decisions about Project X", "Invoices before March 2025").
- **FR-024**: Entity queries MUST return results without requiring AI re-analysis
  of source documents.
- **FR-025**: A visual knowledge graph view MUST display entities and relationships
  as a navigable, interactive map.
- **FR-026**: Clicking an entity in the graph MUST list all source documents
  referencing that entity.

**Analysis Templates**

- **FR-027**: The system MUST provide these built-in analysis templates within
  notebooks: Compare Sources, Extract Action Items, Find Contradictions, Generate
  Timeline, Summarize All.
- **FR-027a**: Analysis templates MUST be available on all supported hardware tiers.
  The system MUST internally route each template to the best available inference
  tier for the detected hardware. When running on constrained hardware (8 GB RAM,
  no Tier 4 model available), the system MUST display a visible quality notice
  before and within the output (e.g., "Running on lightweight model — results
  may be less detailed"). Templates MUST NOT be hidden, locked, or greyed out
  based on hardware tier.
- **FR-028**: Analysis outputs MUST cite the specific source passages contributing
  to each output element.
- **FR-029**: Clicking a cited element in an analysis output MUST highlight the
  originating passage in the source document.
- **FR-030**: Analysis results MUST be exportable to PDF or plain text to a
  user-chosen local path.

**Batch Processing**

- **FR-031**: Users MUST be able to define and launch a processing operation
  (Summarize, Extract Dates, Flag Keywords, Extract Action Items) on all files
  in a selected local folder.
- **FR-032**: Batch jobs MUST run in the background; the user MUST receive a
  system notification on completion.
- **FR-033**: Batch results MUST link each output item to its source file; clicking
  a result MUST open the source file.
- **FR-034**: If a file in a batch fails to process, it MUST be flagged individually
  with an error reason; the remainder of the batch MUST complete normally.

**Privacy and Data Control**

- **FR-035**: All processing, indexing, and storage MUST occur entirely on the
  user's local device with zero external network calls during normal operation.
- **FR-035a**: The system MUST use path-based deduplication: each unique file
  path produces exactly one index entry, shared between notebook and global index
  contexts. Adding a file to a notebook that is already globally indexed MUST NOT
  create a second index entry; it MUST reference the existing one.
- **FR-036**: Users MUST be able to permanently delete any notebook, its index,
  and all derived data (entities, chat history, analysis results) in one action.
- **FR-036a**: Users MUST be able to export any notebook as a self-contained archive
  file containing sources, index, entity graph, and chat history. The archive MUST
  be importable on the same or another device running EchoAI.
- **FR-036b**: Users MUST be able to import a previously exported notebook archive.
  On import, the system MUST restore the notebook with all sources, chat history,
  and derived data intact. If the archive's index format is outdated, the system
  MUST re-index automatically rather than fail.
- **FR-037**: An audit trail MUST record all indexing events, query events, and
  data deletion events; this log MUST be readable by the user.
- **FR-038**: When available local storage drops below 1 GB, the system MUST pause
  all indexing and batch operations, notify the user with a clear message, and
  MUST NOT corrupt or partially write the existing index.

### Key Entities

- **Notebook**: A named workspace containing source files, chat history, analysis
  results, and derived knowledge. The primary organizational unit.
- **Source**: A local file added to a notebook or indexed folder. Tracks format,
  path, last-modified date, and processing errors. Indexing status follows this
  lifecycle: `pending` → `processing` → `indexed` | `failed`. A file-change
  detection or import version mismatch returns the source to `processing`
  (re-indexing); `failed` sources retain their last error reason and can be
  retried manually.
- **Chunk**: A semantically meaningful segment of a source document. The retrieval
  unit for search and citation.
- **Entity**: A named real-world object extracted from source content. Has type
  (person, org, date, project, decision, amount), canonical value, and links to
  all source chunks where it appears.
- **Relationship**: A typed connection between two entities extracted from co-occurrence
  or explicit mentions in source text.
- **QuerySession**: An ordered chat exchange within a notebook scope. Contains
  user messages, system responses, and citation references per response.
- **BatchJob**: A user-initiated processing operation over a folder. Has status
  (queued / running / complete / partial), per-file results, and unified output.
- **IndexedFolder**: A local directory path granted read access for global indexing.
  Tracks indexing status, file count, and last-scan timestamp.
- **AuditEntry**: A timestamped record of a system event (file indexed, query
  answered, data deleted, folder access revoked).

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: A first-time user can add documents to a notebook and receive a cited
  answer within 5 minutes of opening the app for the first time.
- **SC-002**: Questions answerable by exact or semantic search return results in
  under 3 seconds on minimum supported hardware (8 GB RAM, no dedicated GPU).
- **SC-003**: The system correctly declines to answer (rather than hallucinate) on
  out-of-scope questions at 99%+ accuracy measured against a test query set.
- **SC-004**: Background indexing of a 1,000-file folder completes without CPU
  sustaining above 25% or fans activating during user-active hours.
- **SC-005**: Global semantic search across 10,000 indexed files returns relevant
  results in under 3 seconds.
- **SC-006**: Entity extraction identifies people, dates, and project names from
  standard office documents at 85%+ precision against a labeled test corpus.
- **SC-007**: A batch job across 50 documents runs to background completion and
  delivers unified output without any additional user action.
- **SC-008**: Zero external network calls are made during any normal operation
  (indexing, querying, analysis, batch processing), verifiable by network monitor.
- **SC-009**: Revoking folder access removes all derived data from that folder
  within 5 seconds, verifiable by querying for the folder's content afterward.
- **SC-010**: Any analysis result or chat session is exportable to a local file
  within 2 user interactions.

## Clarifications

### Session 2026-04-13

- Q: How should analysis templates behave on 8 GB hardware (no Tier 4 model)? → A: Templates remain available on all hardware tiers; system uses best available model and displays a visible quality notice on constrained hardware. Templates are never locked or hidden.
- Q: Should notebook archive export/import both be in v1? → A: Bidirectional — both export and import in v1. User can restore or transfer notebooks between devices. Outdated index formats trigger automatic re-indexing on import.
- Q: What are the valid Source indexing states? → A: `pending → processing → indexed | failed`; re-indexing re-enters `processing`. Failed sources retain error reason and support manual retry.
- Q: How to handle the same file appearing in both a notebook and global index? → A: Path-based deduplication — one index entry per unique file path, shared across both contexts. No double-indexing.
- Q: Should notebook creation offer office persona or domain presets? → A: No presets in v1 — plain notebooks only; uniform default experience for all users. Domain-specific behavior is scoped to App 2 (Agent personas).

## Assumptions

- **Minimum hardware**: 8 GB RAM, CPU from 2019 or newer. No dedicated GPU is
  required for P1–P3 functionality. P4/P5 may be slower on minimum hardware but
  MUST still function.
- **Language support**: English-language documents are the primary target for v1.
  Multilingual document support (auto-detect language) is a v1.1 enhancement.
- **Single user, no accounts**: The app is local and single-user. No login,
  user profiles, or multi-user separation in v1.
- **File size limit**: Individual source files up to 500 MB are fully supported
  (stored + indexed). Audio/video files between 500 MB and 2 GB are accepted for
  transcription-only indexing — the transcript is indexed, the raw media file is
  not copied into notebook archives. Files exceeding 2 GB are rejected with a
  clear size-limit message.
- **Knowledge graph rendering**: The visual graph view renders up to 500 entities
  interactively; larger entity sets paginate or filter by type.
- **Notebook portability**: Users can export a notebook as a self-contained archive
  (sources + index + history) and import it on the same or another device. Both
  directions are supported in v1. Outdated archive formats re-index automatically.
- **No real-time collaboration**: Concurrent multi-user access to a shared notebook
  is out of scope for v1.
- **Embedding model first-run**: The semantic search embedding model is downloaded
  on first launch (one-time, ~274 MB, user-initiated). A lightweight fallback model
  (~22 MB) is bundled in the installer so the app functions fully offline from the
  first launch if the download is declined. This is the only permitted network
  exception beyond GGUF model downloads, consistent with Principle I.
- **No persona or domain presets**: Notebooks are plain and uniform in v1. Office
  persona presets (Legal, Marketing, Finance) and domain-specific analysis
  configurations are explicitly out of scope for App 1; they belong to App 2
  (Agent Task Executor) persona system.
