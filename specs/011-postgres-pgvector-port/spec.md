# Feature Specification: PostgreSQL + pgvector Persistence Re-platform (Single-Machine)

**Feature Branch**: `011-postgres-pgvector-port`
**Created**: 2026-06-11
**Status**: Draft
**Input**: User description: "Re-platform persistence from SQLite to PostgreSQL + pgvector (single-machine mode). Driver: the one-ask drafting research stage runs vec0 KNN MATCH queries on the live 65 GB chunk_embeddings table; sqlite-vec 0.1.x vec0 is brute-force (not ANN), each query took 1678s/1247s, causing page-cache thrash and a systemd-oomd kill of the dev app. T035's 'research ≤15 s' gate fails ~100×. Scope = Phases 0–2 of the parked architecture pivot plan. Single-machine 'both' mode only — NO LAN networking, NO client/server roles, NO mDNS/auth (later spec). End users must never need to install a database or Docker — the database engine ships inside the app."

## User Scenarios & Testing *(mandatory)*

### User Story 1 - Drafting research is fast and never crashes the app (Priority: P1)

An officer with the full indexed corpus (8+ large legal PDFs, tens of gigabytes of
stored passage vectors) types a one-sentence drafting request. The research stage —
finding supporting passages for each section of the summary skeleton — completes in
seconds per lookup instead of ~28 minutes, and the application is never terminated by
the operating system for exhausting memory while searching.

**Why this priority**: This is the driver for the whole feature. Today the research
stage takes 1678 s / 1247 s per lookup on the live corpus, thrashes the page cache,
and the OS kills the app mid-draft. The one-ask drafting product (Feature 010) is
unusable on a real corpus until this is fixed.

**Independent Test**: On a machine holding the full live corpus, run a one-ask draft
end-to-end. Measure each research lookup and total memory pressure. Delivers value
alone: drafting becomes usable even before any other story lands.

**Acceptance Scenarios**:

1. **Given** the full live corpus is stored and indexed, **When** the drafting
   research stage performs a similarity lookup, **Then** each lookup returns in ≤ 15
   seconds (Feature 010 T035 gate).
2. **Given** a one-ask draft is running on the full corpus, **When** the research and
   composition stages execute back-to-back, **Then** the application completes the
   draft without being killed by the operating system's memory-pressure protection.
3. **Given** the new storage engine, **When** the same research query is run against
   the old and new systems on the same corpus, **Then** the passages found are
   materially the same (citation-grade contract intact — see SC-005).

---

### User Story 2 - One installer, zero database setup for end users (Priority: P2)

A non-technical officer (or IT staff) installs the app from a single installer (e.g.
a Windows .exe / Linux package). On first launch the app provisions its own database
engine silently — no Docker, no separate PostgreSQL installation, no command line, no
internet connection. Everything stays on the local machine, consistent with the
offline/local-first product ethos.

**Why this priority**: The product is distributed to government officers as a
packaged desktop app. If re-platforming added a "first install Docker/PostgreSQL"
step, deployment would be dead on arrival. The database engine must be an invisible
internal component, bundled and lifecycle-managed by the app itself.

**Independent Test**: On a clean machine (no database software installed, no
internet), install the app from the installer and complete an index + draft cycle.
Zero manual database steps observed.

**Acceptance Scenarios**:

1. **Given** a clean machine with no database software and no internet access,
   **When** the user installs and launches the app, **Then** the app starts, creates
   its data store automatically, and is ready to index documents with no additional
   installation steps.
2. **Given** the app is closed (normally or by crash/power loss), **When** it is
   launched again, **Then** the bundled database engine starts cleanly, recovers, and
   all previously indexed data is intact.
3. **Given** the bundled database engine fails to start (e.g. port conflict with an
   existing PostgreSQL, corrupted data directory), **When** the user launches the
   app, **Then** the user sees a plain-language error with a concrete remediation
   step, never a silent hang.
4. **Given** the app is running, **When** another machine on the network attempts to
   connect to the database port, **Then** the connection is refused (local-machine
   access only; networking is a later feature).

---

### User Story 3 - Existing corpus carries over without re-indexing (Priority: P3)

A user upgrading from the current version has a large indexed corpus (the live
database is ~65 GB, dominated by passage vectors that took many hours of compute to
produce). After upgrading, a one-time migration moves all data — documents, chunks,
vectors, drafting sessions, chat history — into the new store. No document is
re-extracted, re-chunked, or re-embedded.

**Why this priority**: Re-indexing the corpus costs many hours of compute and was
already a pain point. Valuable, but only after Stories 1–2 prove the new engine
works; a fallback (re-index from the source folder) exists if migration fails.

**Independent Test**: Point the migration at the live old database, run it once,
then verify counts and spot-check retrieval parity against pre-migration results.

**Acceptance Scenarios**:

1. **Given** an existing populated old database, **When** the one-time migration
   runs, **Then** every table's row count matches the source and all vectors are
   preserved bit-faithfully (no re-embedding).
2. **Given** the migration is interrupted (crash, power loss), **When** it is
   re-run, **Then** it resumes or restarts safely without duplicating or corrupting
   data, and the old database remains untouched until migration is verified.
3. **Given** insufficient free disk space for a full copy, **When** migration starts,
   **Then** it checks space up front and refuses with a clear message instead of
   failing partway.

---

### User Story 4 - Developers and tests see one storage contract (Priority: P4)

A developer runs the existing test suites (drafting, retrieval, indexing,
acceptance) against the new store and they pass unchanged in behavior. Storage
access goes through the repository seam (ADR-0014) so no service code reaches around
it, and the governance document that previously named the old engine as the single
source of truth is formally amended.

**Why this priority**: Protects the citation-grade contract and prevents the
migration tax from being paid twice when networking (later spec) arrives.

**Independent Test**: Run the full test suites against a provisioned new-engine
instance; audit that no module outside the seam issues raw storage queries.

**Acceptance Scenarios**:

1. **Given** the ported persistence layer, **When** the existing drafting/retrieval/
   indexing suites run, **Then** they pass with unchanged behavioral assertions.
2. **Given** the constitution's storage principle (§IV), **When** this feature
   lands, **Then** an accepted architecture decision record supersedes it, naming
   the new single source of truth and the rationale.

---

### Edge Cases

- Another PostgreSQL instance already occupies the default port on the user's
  machine → bundled engine must pick a free port (or socket) and connect to *its own*
  instance only, never to a foreign database.
- App killed (crash, power loss, forced shutdown) while the bundled engine is mid-
  write → next launch must recover automatically; an orphaned engine process from a
  previous run must be detected and adopted or replaced, not duplicated.
- Approximate vector search returns slightly different neighbors than exact scan →
  bounded by the parity criterion (SC-005); the verbatim verify gate downstream still
  guarantees no invented citations.
- Migration source database is from an older schema version → migration must refuse
  with a clear message rather than guess.
- Disk fills while the vector index is being built during migration → fail with a
  clear message; old database remains the intact fallback.
- Antivirus/endpoint protection on user machines flags the bundled engine binary →
  installer/packaging must use signed, reputable binaries (risk noted for rollout).

## Requirements *(mandatory)*

### Functional Requirements

- **FR-001**: All application persistence (documents, chunks, vectors, search index,
  knowledge graph, drafting sessions, chat history, settings) MUST be served by the
  new single store; no feature may remain on the old engine after cutover.
- **FR-002**: Similarity search over passage vectors MUST use an approximate-nearest-
  neighbor index (not exhaustive scan), and MUST return each top-K lookup on the
  full live corpus in ≤ 15 seconds (ties to Feature 010 T035 gate).
- **FR-003**: Hybrid retrieval MUST be preserved: keyword/full-text search and vector
  search both feed the existing rank-fusion (RRF) and boost stack with unchanged
  downstream behavior.
- **FR-004**: The database engine MUST ship inside the application package; end users
  MUST NOT be required to install Docker, a database server, or any third-party
  runtime. First launch MUST provision the data store automatically, offline.
- **FR-005**: The app MUST own the engine's lifecycle: start on app launch, health-
  check, clean shutdown on app exit, recovery after crash, and detection/handling of
  an orphaned engine process from a prior run.
- **FR-006**: The engine MUST accept connections only from the local machine
  (loopback/socket). Remote access is out of scope and MUST be refused.
- **FR-007**: A one-time migration MUST move all data from the existing old database
  into the new store, preserving vectors bit-faithfully (no re-extraction,
  re-chunking, or re-embedding) and verifying row counts per table.
- **FR-008**: Migration MUST be safe to interrupt and re-run, MUST pre-check free
  disk space, and MUST leave the old database untouched until the user confirms the
  migrated system works.
- **FR-009**: All storage access MUST go through the repository/seam layer
  (ADR-0014 design rules); service and command code MUST NOT embed engine-specific
  queries outside the seam.
- **FR-010**: The complete existing schema (all 25 migrations' worth of structure)
  MUST be re-expressed natively for the new engine as a fresh baseline; future
  schema changes continue as additive migrations on the new engine.
- **FR-011**: Indexing write throughput (documents/minute end-to-end) MUST be no
  slower than the current system on the same corpus and hardware.
- **FR-012**: Engine startup failures MUST surface as plain-language, actionable
  errors in the UI (what failed + one concrete remediation), never a silent hang.
- **FR-013**: An architecture decision record MUST be accepted that supersedes
  constitution §IV ("SQLite single source of truth") before cutover lands on the
  main branch.
- **FR-014**: The vector similarity *index* SHOULD use a reduced-precision
  representation if it preserves retrieval parity (SC-005), targeting ≥ 40%
  reduction of the **index** footprint and build time; stored vectors remain
  full-precision (bit-faithful, FR-007). Reduced-precision *column storage* (which
  would cut the ~65 GB bulk itself) is an explicitly parity-gated follow-up after
  live parity data exists — not part of this feature's cutover.
- **FR-015**: Developer workflows (tests, dev runs) MUST work against the same
  bundled-engine mechanism end users get, so dev/prod parity holds; an externally
  managed engine MAY be supported as an opt-in developer convenience only.

### Key Entities

- **Document / Chunk / Vector**: existing corpus entities; carried over 1:1. Vectors
  are the bulk of the data (~65 GB) and the costliest to recreate.
- **Search index (full-text)**: keyword index rebuilt natively on the new engine
  from carried-over chunk text (cheap to rebuild, unlike vectors).
- **Drafting session / chat history / one-ask state**: append-only and versioned
  state records; carried over 1:1.
- **Bundled engine instance**: the app-managed database server process — its data
  directory, port/socket, credentials, and lifecycle state.
- **Migration job**: one-time transfer with per-table verification counts, resume
  marker, and a final verification report shown to the user.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: On the full live corpus, each research-stage similarity lookup
  completes in ≤ 15 seconds (today: 1247–1678 s; ≥ 80× improvement).
- **SC-002**: A complete one-ask draft on the full live corpus finishes without the
  app being terminated by OS memory protection, in 10 consecutive runs.
- **SC-003**: On a clean machine with no internet and no database software, a user
  can install the app and reach "ready to index" with zero manual database steps.
- **SC-004**: The existing ~65 GB corpus migrates in one unattended run (target
  ≤ 4 hours on current dev hardware) with 100% row-count verification and zero
  re-embedding.
- **SC-005**: Retrieval parity: on the standing evaluation prompt set, top-K results
  from the new store overlap ≥ 95% with the old exact-scan results, and every
  citation surfaced in drafts still passes the verbatim verify gate (zero invented
  references — citation-grade contract unchanged).
- **SC-006**: Indexing a new document is no slower end-to-end than the current
  system (±10%) on the same hardware.
- **SC-007**: All existing automated suites (drafting, retrieval/query-router,
  indexing, acceptance gates) pass against the new store with no weakened
  assertions.

## Scope Change — 2026-06-12 (user decision during implementation)

**No data migration.** The user chose to wipe the existing ~65 GB `echo.db`
entirely and re-index the source documents fresh on the new engine (the corpus
is 8 PDFs; re-indexing is cheap relative to building and verifying a 65 GB
migration). Consequences: **User Story 3, FR-007, FR-008 and SC-004 are
withdrawn**; SC-005 (retrieval parity) is now verified as HNSW-vs-exact-scan on
the same fresh corpus instead of new-vs-old store; the old `echo.db` file is
deleted (by the user) once User Story 1's live gate passes.

## Assumptions

- **Hard cutover, no dual backend**: after this feature, the old engine is retired
  from the app; the seam (ADR-0014) exists for portability, not for maintaining two
  live backends. The old database file is kept on disk read-only until the user
  confirms migration success.
- **Bundled engine, also in dev**: developers run the same app-provisioned engine
  as end users (FR-015); Docker is at most an opt-in developer convenience and never
  a user-facing requirement.
- **Approximate search is acceptable** because the downstream verbatim verify gate —
  not retrieval — is what guarantees citation fidelity; parity is still bounded by
  SC-005.
- **Full-text index is rebuilt, not migrated**: chunk text carries over; the keyword
  index is regenerated natively (minutes, not hours).
- **Out of scope** (later spec, per the pivot plan): LAN client/server roles,
  network discovery, authentication/accounts, multi-user ACLs, moving embed/LLM
  compute server-side.
- **Existing 010 work continues to function**: this feature branches from
  `010-one-ask-drafting` and must keep the one-ask pipeline green throughout.
