# Feature Specification: General Structured-Table Capture at Ingestion

**Feature Branch**: `023-structured-table-capture`
**Created**: 2026-07-12
**Status**: Draft
**Input**: User description: "General structured-table capture at ingestion — preserve detected table structure through to chunking; emit a whole-table unit plus per-row units that bypass the legal clause parser, and a graph table-node with row children, so counting / list / summary see complete tables. General across all document types. First validation: RoB Schedule-I 39-department table."

## User Scenarios & Testing *(mandatory)*

### User Story 1 - "How many / list all" over a table returns the complete set (Priority: P1)

An officer asks a counting or enumeration question whose answer lives in a table — e.g. "how many
departments are in the Rules of Business?" or "list all the departments and their attached departments."
Today the answer is wrong (12–14 of 39) because the table was shredded into hundreds of fragments and only
a handful reach the answer. After this feature the complete table is held as one coherent unit, so the
count and the list are complete.

**Why this priority**: This is the core defect the feature exists to fix and the primary validation case.
A wrong count on a well-known document destroys officer trust; completeness here is the whole point.

**Independent Test**: Re-index a document containing a real table (RoB Schedule-I), then ask "how many
departments" and "list all departments." Verify the count equals the true number of rows (39) and every
department appears — with no other change to the system.

**Acceptance Scenarios**:

1. **Given** a document containing an N-row table, **When** it is ingested, **Then** the corpus holds one
   whole-table unit that contains all N rows (header + every row).
2. **Given** the RoB Schedule-I table (39 departments), **When** an officer asks "how many departments",
   **Then** the answer is 39, not 12–14.
3. **Given** the same table, **When** an officer asks "list all departments", **Then** the response
   enumerates all 39 with their attached departments, none dropped.

---

### User Story 2 - Whole-document summary covers table content completely (Priority: P1)

An officer asks to summarize a document that contains tables. Today table content is silently dropped or
mangled because it was shredded. After this feature the whole-table unit is a first-class part of the
document structure, so the whole-document summary reflects the table (e.g. "distributes business across 39
departments, including …") rather than omitting it.

**Why this priority**: Whole-document summaries are the flagship use case for this corpus; a summary that
silently omits a document's central table is misleading. Feeds directly into the graph-guided summarizer.

**Independent Test**: Generate the pre-baked whole-document summary for a table-bearing document and confirm
the table's existence, size, and key rows are represented in the summary.

**Acceptance Scenarios**:

1. **Given** a table-bearing document, **When** its whole-document summary is generated, **Then** the table
   is represented (its subject and scale, e.g. row count), not omitted.
2. **Given** a document whose structure is walked to build the summary, **When** a table unit is reached,
   **Then** the summarizer can enumerate or aggregate its rows rather than seeing scattered fragments.

---

### User Story 3 - Targeted question about one table row is answerable (Priority: P2)

An officer asks about a single entry in a table — e.g. "what attached departments fall under the Works
Department?" The system can retrieve that specific row (with the table's header for context) without
pulling the entire table, so the answer is precise.

**Why this priority**: Targeted row lookup is valuable but secondary to completeness; it is the reason the
feature keeps per-row units in addition to the whole-table unit.

**Independent Test**: Ask about a specific known row in a table and confirm the answer reflects that row's
cells, attributed to the correct table and row.

**Acceptance Scenarios**:

1. **Given** a table with per-row units, **When** an officer asks about one row's subject, **Then**
   retrieval can surface that row (with header context) as a distinct unit.
2. **Given** a row that carries an amendment note (inserted/deleted by notification), **When** it is
   captured, **Then** the note travels with the row rather than being lost or split.

---

### Edge Cases

- **No detectable tables**: a document with no tables ingests exactly as it does today (no regressions, no
  empty table units).
- **Table extractor unavailable**: when the structured-table extractor cannot run, ingestion falls back to
  the current prose behavior rather than failing the index.
- **Malformed / ragged tables**: rows with missing or extra cells, merged cells, or a missing header are
  captured best-effort without crashing; a table with no usable rows produces no table unit.
- **Very large tables**: a table with hundreds of rows is captured whole without dropping rows (the
  whole-table unit may be split by size only if every row is still retained and countable).
- **Table spanning multiple pages**: a single logical table broken across pages is captured as one unit
  where the extractor reports it as one, not as separate fragments.
- **Non-table columnar prose**: ordinary enumerated clauses that merely look columnar MUST NOT be captured
  as tables (the current over-tagging failure mode must be avoided).
- **Re-index idempotency**: re-indexing a document replaces its prior table units cleanly (no duplicates).

## Requirements *(mandatory)*

### Functional Requirements

- **FR-001**: The system MUST preserve the structure of each detected table (its header and its rows as
  discrete cells) from extraction through to the point where document units are created — the structure MUST
  NOT be discarded after linearization.
- **FR-002**: For each detected table, the system MUST create one **whole-table unit** that contains the
  header and every row, such that a deterministic count over that unit equals the true number of rows.
- **FR-003**: For each detected table, the system MUST create **per-row units**, each carrying that row's
  cells together with the table's header for context.
- **FR-004**: Table units MUST bypass the provision/clause parser so that table content is never shredded
  into sub-clause fragments or mis-labeled as provisions.
- **FR-005**: The system MUST represent each table in the document graph as a **table node with its rows as
  child nodes**, so that a structure-walking process can reach the table and enumerate its rows.
- **FR-006**: Table capture MUST be general — driven by detected table structure, not by any per-document or
  per-document-type hand-tuned rule — so it applies uniformly to legal rulebooks, meeting minutes,
  audit-para documents, and other official documents.
- **FR-007**: Table capture MUST be deterministic and MUST NOT require a language model.
- **FR-008**: When no table is detected, or the table extractor is unavailable or fails, ingestion MUST
  proceed with existing behavior and MUST NOT fail the index.
- **FR-009**: Ordinary prose and enumerated clauses that are not genuine tables MUST NOT be captured as
  tables (no false-positive table units).
- **FR-010**: Row-level content that carries an amendment or editorial note MUST retain that note with the
  row it belongs to.
- **FR-011**: Re-indexing a document MUST replace its previous table units and table graph nodes without
  leaving duplicates or orphans.
- **FR-012**: The whole-table unit MUST be retrievable and countable as a single unit for
  counting/enumeration questions, while per-row units remain independently retrievable for targeted
  questions.

### Key Entities *(include if feature involves data)*

- **Detected Table**: a table found in a source document, with a header (column labels) and an ordered set
  of rows; the authoritative structure produced by the ingestion table extractor.
- **Whole-Table Unit**: one document unit representing an entire table (header + all rows), used for
  counting, listing, and summarization; typed distinctly from prose provisions.
- **Table Row Unit**: one document unit representing a single table row plus header context, used for
  targeted retrieval.
- **Table Graph Node**: a node in the document graph representing a table, whose children are its row nodes;
  lets a structure walk reach and enumerate the table.
- **Amendment Note (on a row)**: an editorial marker (inserted/deleted by notification) attached to a
  specific row, preserved alongside that row.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: For the RoB Schedule-I validation case, "how many departments" returns **39** (was 12–14), and
  "list all departments" enumerates all 39 with no omissions.
- **SC-002**: For every document containing at least one detected table, the corpus contains exactly one
  whole-table unit per detected table that holds 100% of that table's rows (verified by comparing the unit's
  row count to the extractor's reported row count).
- **SC-003**: No table's content is split into more units than it has rows plus one (one whole-table unit +
  one per row) — i.e. the historical "hundreds of fragments per table" outcome is eliminated.
- **SC-004**: Documents with no tables show **zero change** in their produced units versus prior behavior
  (no regression on prose-only documents).
- **SC-005**: A whole-document summary of a table-bearing document represents the table (subject and scale)
  rather than omitting it, verified on at least the RoB and one other table-bearing document.
- **SC-006**: The feature runs at ingestion within the existing deterministic pipeline with no added model
  calls, and completes within the current table-extraction time budget for a 200+ page document.

## Assumptions

- The existing ingestion table extractor already produces per-table structured header + rows for
  machine-readable PDFs; this feature consumes that output rather than introducing a new detector.
- "Table" means a structure the extractor reports as a table; scanned-image-only tables without an
  extractable grid are out of scope (they remain whatever the OCR path produces).
- Counting and summarization layers already exist and will consume the new whole-table unit / table node;
  this feature's responsibility ends at producing those units and nodes correctly.
- Storing an additional document-unit type and graph-node type via an additive migration is acceptable and
  does not require changing existing rows.

## Out of Scope

- Building a new table-detection engine or OCR table reconstruction for scanned images.
- The graph-guided whole-document summarizer itself (separate feature) — this feature only guarantees the
  table units/nodes it will consume exist and are complete.
- Document-type-specific parsing profiles (meeting minutes, audit paras) — separate follow-on work; table
  capture here is doc-type-agnostic and benefits them once their prose structure is parsed.
- Cross-document / entity synthesis ("across all departments in all documents").
