# Feature Specification: Fast-Tier Ingestion Hardening (Citation-Grade)

**Feature Branch**: `013-ingestion-hardening`
**Created**: 2026-06-19
**Status**: Draft
**Input**: User description: Fast-tier ingestion hardening for citation-grade legal/government documents (digital PDFs) — refactor the deterministic ingestion pipeline so structure detection, citation labeling, normalization, and persistence produce correct, verbatim, correctly-structured chunks. Driven by the Iteration-1 audit of `fundamental_rights.pdf` (Constitution of Pakistan, Part II, Articles 7–40), which surfaced 12 defects (D1–D12).

## Overview

When the assistant drafts a summary or note for an officer, every rule/article/section it cites
must be **correct and verbatim**, because the output goes up for a Secretary's signature. Today the
ingestion stage (which turns a document into the labelled pieces everything else relies on)
introduces errors that make confident-but-wrong citations possible: provisions are mislabelled,
some are lost entirely, footnotes and page numbers contaminate the legal text, and quoted wording is
altered. This feature hardens ingestion so the stored pieces faithfully represent the source
document's structure and text.

The guiding principle is **separation of structure from vocabulary**: first determine *what* a
provision is (its number and structural level) and *where* it sits; only then decide *what to call
it* (Article vs Section vs Rule), and when that cannot be decided with confidence, mark it uncertain
rather than guess.

## User Scenarios & Testing *(mandatory)*

### User Story 1 - Trustworthy provision identity (Priority: P1)

Every provision in a document is detected, correctly numbered, nested under the correct
Part/Chapter, and labelled with the correct citation word — or explicitly flagged as uncertain.
No provision is silently lost or given a confidently wrong label.

**Why this priority**: This is the citation-grade core. A lost or mislabelled provision is a
career-level error for the officer. Without it, nothing downstream can be trusted.

**Independent Test**: Index the Constitution Part II sample and confirm every Article 7–40 (including
10A, 19A, 25A) is present, nested under the right Part/Chapter, and labelled "Article N" — never
"Section N" — or flagged unresolved.

**Acceptance Scenarios**:

1. **Given** a document whose provisions are numbered "7.", "10A.", "25A.", **When** it is ingested,
   **Then** each appears as its own provision with that exact identifier and none is absorbed into a neighbour.
2. **Given** a document that uses Articles, **When** it is ingested, **Then** no provision is labelled
   "Section"; provisions are labelled "Article N" or, if the document type is genuinely ambiguous,
   flagged unresolved (identifier kept, citation word omitted).
3. **Given** a document with Chapters inside a Part, **When** it is ingested, **Then** the Chapters
   are real structural levels and each Article is nested under its correct Chapter.
4. **Given** a provision printed as a catchline (title line) followed by enacting text, **When** it is
   ingested, **Then** it is stored as one logical provision (title + body), not split into an orphan
   heading and a separate body.

---

### User Story 2 - Uncontaminated, verbatim provision text (Priority: P1)

A provision's stored text contains only the law's own text. Footnotes (amendment history), printed
page numbers, and running headers do not appear inside provisions, and no quoted wording is ever
altered.

**Why this priority**: Foreign text inside a provision and altered wording both produce wrong
citations directly. Equally citation-grade-critical as identity.

**Independent Test**: Inspect every provision chunk of the sample and confirm none contains
amendment-history text, a footer page number, or a running header, and that the exact phrase
"shall not" is never rendered as "shallnot".

**Acceptance Scenarios**:

1. **Given** a page with footnotes at the bottom, **When** ingested, **Then** the footnote text is
   stored separately (not inside any provision) and is itself retrievable and auditable.
2. **Given** printed page numbers in the footer, **When** ingested, **Then** no provision chunk
   contains or equals a page number, and no standalone page-number chunk is created as content.
3. **Given** the phrase "shall not" in the source, **When** ingested, **Then** the stored text
   contains "shall not" exactly, never a fused form.
4. **Given** any stored provision, **When** compared to the source, **Then** its text (ignoring
   whitespace differences) is a verbatim substring of the source — never synthesized.

---

### User Story 3 - Page-accurate citations (Priority: P2)

Every provision carries the source page it came from, so citations and review can reference the page.

**Why this priority**: Page references strengthen citations and reviewer trust, but the citation
itself is usable without them; hence P2.

**Independent Test**: Confirm every provision chunk has a page number that is within the document's
page range and increases consistently through the document.

**Acceptance Scenarios**:

1. **Given** an ingested document, **When** any provision is examined, **Then** it has a non-empty
   page number within the valid page range.
2. **Given** provisions in reading order, **When** their page numbers are read in sequence, **Then**
   they are non-decreasing.

---

### User Story 4 - Correct clause and amendment structure (Priority: P2)

Sub-clauses ((1), (a), (i), …) are split at the right boundaries, and amendment markers (bracketed
insertions/substitutions) are preserved verbatim rather than corrupting boundaries or text.

**Why this priority**: Sub-clause precision matters for pin-point citations, but is lower-leverage
than provision identity and clean text, and is most reliable once contamination is removed — hence P2.

**Independent Test**: For a provision with enumerated sub-clauses, confirm each sub-clause is its own
unit, none swallows the start of the next, and bracketed amendment text appears verbatim.

**Acceptance Scenarios**:

1. **Given** a provision with sub-clauses (a) and (b), **When** ingested, **Then** (a) does not
   contain the opening of (b).
2. **Given** text containing a bracketed amendment insertion, **When** ingested, **Then** the
   brackets and inserted text are preserved exactly as in the source.

---

### User Story 5 - Amendment & cross-reference relationships captured (Priority: P3)

The amendment history we now isolate from footnotes/brackets, and the cross-references provisions
make to each other ("subject to Article 251", "Nothing in clause (1) or in Article 270", "First
Schedule"), are recorded as relationships in the existing knowledge-graph store rather than
discarded — so the assistant can later answer "what amended Article 25A?" or "what does Article 8
reference?" and show amendment provenance alongside a citation.

**Why this priority**: High long-term value (relational answers, amendment provenance) but it builds
on US2 (footnote isolation) and US4 (amendment-bracket handling) and is not required for a correct
citation; hence P3. It is the first concrete use of the graph substrate that already exists but is
currently unused.

**Independent Test**: After ingesting the sample, confirm that an amendment captured from a footnote
(e.g., "Subs. by the Constitution (Eighteenth Amdt.) Act, 2010") is stored as an amendment
relationship anchored to the correct provision, and that an in-text cross-reference (e.g., Article 24
→ Article 253) is stored as a reference relationship between the two provisions.

**Acceptance Scenarios**:

1. **Given** a footnote recording an amendment to a provision, **When** ingested, **Then** an
   amendment relationship links that provision to the amending instrument (verbatim), or is flagged
   unresolved if the anchored provision is uncertain.
2. **Given** a provision that cites another provision by number, **When** ingested, **Then** a
   reference relationship links the two; an unresolvable target is flagged, never invented.
3. **Given** relationships are recorded, **When** the source text is consulted, **Then** every
   captured relationship's identifiers are verbatim substrings of the source (no synthesized links).

---

### Edge Cases

- **Genuine source disorder (25/25A).** The sample's source prints Article 25/25A catchlines out of
  order. The system MUST NOT silently "correct" or invent an ordering; it reflects the source and
  flags the ambiguity as unresolved where it cannot be determined faithfully.
- **Provision with no separate catchline.** Some provisions have only enacting text; the system must
  handle these without inventing a title.
- **Multiple provisions per page.** Footnotes must be anchored to the provision indicated by their
  marker, not merely "the provision on the same page"; uncertain anchors are flagged, not guessed.
- **Bare integers in legal text.** Numbers like "fourteen years" or cross-references must never be
  mistaken for page numbers and stripped.
- **Mixed/hybrid documents.** A document with both digital-text and image-only pages is routed
  per page; digital-text pages are hardened by this feature, image-only pages are deferred (OCR is a
  Non-Goal). Per-page *routing* is captured; image-only *extraction* is not implemented here.

## Requirements *(mandatory)*

### Functional Requirements

- **FR-001**: The system MUST detect every provision in a document, including provisions with an
  alphabetic suffix (e.g., 10A, 19A, 25A), and MUST NOT absorb a provision into a neighbouring one.
- **FR-002**: The system MUST represent the document hierarchy with the correct structural levels
  (e.g., Part → Chapter → Article → Clause → Sub-clause), creating Chapter-level nodes where present.
- **FR-003**: The system MUST store a provision's title (catchline) and its body as one logical
  provision, never as separate orphan pieces.
- **FR-004**: The system MUST assign the citation word (Article/Section/Rule) based on layered
  evidence (document profile, the document's own internal wording, an optional explicit override);
  when evidence is insufficient or conflicting, it MUST keep the identifier but mark the citation
  word unresolved rather than emit a wrong word.
- **FR-005**: The system MUST NOT include footnote/amendment-history text inside any provision; such
  text MUST be stored separately, remain retrievable and auditable, and be anchored to the provision
  indicated by its marker (or flagged unresolved when the anchor is uncertain).
- **FR-006**: The system MUST exclude printed page numbers and running headers from provision text,
  and MUST NOT create standalone page-number content pieces.
- **FR-007**: The system MUST NOT alter the wording of source text during normalization; specifically,
  no normalization rule may fuse two distinct words across a legitimate space (e.g., "shall not").
- **FR-008**: Every stored provision's text MUST be a verbatim substring of the source document
  (whitespace-normalized comparison); the system MUST NOT synthesize text.
- **FR-009**: The system MUST record the source page number for every provision.
- **FR-010**: The system MUST split sub-clauses at correct boundaries and MUST preserve bracketed
  amendment markers and inserted text verbatim.
- **FR-011**: When a provision cannot be confidently identified or labelled, the system MUST surface
  it as unresolved (flag) rather than present it as a confident citation.
- **FR-012**: The system MUST provide an automated regression fixture (the Constitution Part II
  sample) whose checks fail if any of the above outcomes regress.
- **FR-013**: Ingestion MUST remain deterministic in the fast tier (no model/LLM involvement in
  producing structure, labels, or text).
- **FR-014**: The system MUST capture amendment relationships detected from isolated footnotes/markers
  (provision ← amended-by → amending instrument) as relationships in the existing knowledge-graph
  store, recording the amending instrument verbatim, or flag the relationship unresolved when its
  anchor provision is uncertain.
- **FR-015**: The system MUST capture in-text cross-references between provisions (provision →
  references → target provision) as relationships in the knowledge-graph store; an unresolvable
  target MUST be flagged, never invented.

### Key Entities *(include if feature involves data)*

- **Provision**: A unit of law (Part, Chapter, Article, Clause, Sub-clause). Attributes: structural
  level, numeric/alpha identifier, source text span(s), source page, citation word (or unresolved),
  parent provision.
- **Footnote**: Page-bottom apparatus (amendment history). Attributes: text, source page, anchor
  reference to the marked provision (or unresolved). Kept separate from provisions; retrievable.
- **Document profile**: The classification that informs the citation vocabulary and detection rules
  for a document type (constitution, act, rules, manual, regulation, generic).
- **Amendment relationship**: A link from a provision to the instrument that amended it (e.g., a
  named Amendment Act and year), derived from isolated footnotes/markers. Attributes: source
  provision, amending instrument (verbatim), amendment kind (insert/substitute/omit) where stated,
  resolved/unresolved.
- **Cross-reference relationship**: A link from one provision to another it cites by number.
  Attributes: source provision, target identifier, resolved/unresolved.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: For the Constitution Part II sample, **100%** of Articles 7–40 (including 10A, 19A,
  25A) are present, correctly numbered, and nested under the correct Part/Chapter.
- **SC-002**: **Zero** provisions are labelled with the wrong citation word (e.g., "Section" where the
  document uses Articles); any not confidently labelled are flagged unresolved.
- **SC-003**: **Zero** provision chunks contain footnote/amendment-history text, a footer page number,
  or a running header.
- **SC-004**: **Zero** occurrences of altered wording (e.g., "shallnot") across the corpus.
- **SC-005**: **100%** of provision chunks have a valid, in-range source page number.
- **SC-006**: **100%** of provision text passes the verbatim-substring check against the source.
- **SC-007**: The regression fixture fails CI if any of SC-001…SC-006 regress, and detects the 25/25A
  anomaly as flagged (not silently rewritten).
- **SC-008**: Re-indexing the three controlled test documents introduces **no** regression in
  previously-passing documents (the prose document stays clean; the law document improves to the
  above outcomes).
- **SC-009**: For the sample, amendment relationships captured from footnotes and in-text
  cross-references are stored in the knowledge-graph store with **zero** synthesized (non-verbatim)
  identifiers; uncertain anchors/targets are flagged unresolved rather than invented.

## Assumptions

- **Footnotes are retained** as a separate, retrievable, auditable class (not discarded), per the
  reviews — preferred over deletion because amendment history has citation value.
- **Catchline representation**: the catchline becomes the provision's title/marginal-note and the
  enacting text its body, within one logical provision.
- **Vocabulary on conflict** resolves to unresolved (identifier kept, word omitted) — refuse-or-flag
  over confident-wrong.
- **Page-number/footnote detection uses document layout/geometry** rather than text pattern matching,
  to avoid stripping legitimate numbers in legal text. (Mechanism is an implementation concern for the plan.)
- The three controlled documents (simple prose, small table, this digital law PDF) are the working
  test set; the law PDF (177 chunks) is frozen as the primary regression fixture.

## Non-Goals (Out of Scope)

- **OCR / scanned-document extraction** (PaddleOCR vs tesseract): scan-only and deferred to a later
  stage; none of the defects here are extraction-caused.
- **Table reconstruction** (the delegation-of-financial-powers document): a separate workstream.
- **Retrieval and LLM-response stages**: addressed in later stages of the modular plan.
- **Graph-based querying / retrieval routing**: this feature only *captures* amendment and
  cross-reference relationships into the existing graph store during ingestion (US5). Using the graph
  to answer relational queries (routing, traversal, ranking) is a later retrieval-stage workstream.
- **Drafting behaviour changes**: this feature only changes the stored representation of documents.
