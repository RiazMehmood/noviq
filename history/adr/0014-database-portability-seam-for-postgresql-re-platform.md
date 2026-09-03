# ADR-0014: Database Portability Seam for PostgreSQL Re-Platform

- **Status:** Accepted (seam design now); the PostgreSQL migration half is **now executing via ADR-0015** (2026-06-11, Feature 011) — ADR-0015 also supersedes this ADR's rejection of "embedded-Postgres per machine" (context changed: oomd kill made ANN a single-machine blocker)
- **Date:** 2026-06-11
- **Feature:** 010-one-ask-drafting (cross-cutting; applies to all new persistence/retrieval code)
- **Context:** The product direction (decided 2026-06-09, execution parked until the drafting pipeline ships) is a client/server LAN deployment: one installer, first-run role selection (client/server/both), WiFi/LAN-only with no internet, where the server hosts the database and compute. That architecture replaces per-machine SQLite with PostgreSQL + pgvector as the shared store. Constitution §IV currently mandates "SQLite as Single Source of Truth — no external database processes, no Postgres", so the migration itself cannot proceed without a constitution amendment (governance §1–3). Meanwhile Feature 010 is adding new persistence (`one_ask_session`, migration 025) and a new retrieval helper (vec0 KNN). The user has directed: design 010 accordingly, so the re-platform later is a swap, not a rewrite.

## Decision

Design-for-portability rules, binding on Feature 010 and all subsequent new code (an integrated cluster — these rules only pay off together):

- **Single vector-search seam.** All vector retrieval in new code goes through one helper module (`services/drafting/retrieval.rs`, later promoted to a shared service): `knn(query_embedding, k, filter) -> Vec<Hit>`. The sqlite-vec `MATCH ?/k=?` KNN form is an implementation detail *inside* the seam; a pgvector backend (`ORDER BY embedding <=> $1 LIMIT k`, HNSW/IVFFlat index) replaces the internals without touching callers. No new call sites may inline vec0 SQL.
- **Repository-shaped persistence for new tables.** `one_ask_session` access is confined to `state.rs` (save/load/delete by draft_id) — the same shape as 009's `session.rs`. State is a versioned serde JSON blob (`state_json`), which maps 1:1 to Postgres `JSONB`; no SQLite-specific column types or features (no FTS5 dependence, no vec0 columns) in new feature tables.
- **Portable SQL subset in new code.** New queries stick to ANSI-compatible SQL (parameterised, no SQLite pragmas/functions beyond what the seam encapsulates); timestamps as ISO-8601 TEXT (→ `timestamptz` later); ids as TEXT uuids (→ `uuid`).
- **Pool-handle discipline as the future transport seam.** New code takes `&SqlitePool` (or the seam's handle) as a parameter — never constructs connections — so the parked Local/Remote transport trait can wrap the same call sites when client/server lands.
- **Explicitly out of scope now:** actually introducing Postgres, dual-backend abstraction layers (no `sqlx::Any`, no trait-per-table ceremony), and migrating existing 001–009 tables. The constitution amendment + full migration plan is its own future ADR at execution time.

## Consequences

### Positive

- The Postgres re-platform becomes: amend constitution → reimplement the seam internals (pgvector KNN, pool type) → move data; 010's pipeline, gate, commands, and frontend are untouched.
- `state_json` blobs and ANSI SQL make 010's data trivially portable (`TEXT`→`JSONB`, `TEXT` ids→`uuid`).
- The retrieval seam fixes today's problem too: it is exactly where the 281 s brute-force scan is replaced by indexed KNN, so performance work and portability work land in the same module.
- No premature abstraction cost: zero dual-backend code ships now; rules are conventions, not frameworks.

### Negative

- Conventions without enforcement can drift — existing 009/query_router code still inlines vec0 SQL and is *not* being retrofitted now, so the codebase is temporarily two-style; the future migration must sweep old call sites (known debt, listed in the pivot plan).
- sqlite-vec and pgvector rank differently in edge cases (L2-on-normalised vs native cosine, different ANN recall characteristics); the knn_parity test protects within-SQLite correctness but a pgvector swap needs its own parity/recall validation.
- FTS5-dependent features elsewhere (chat retrieval, citation fast-path) have no Postgres equivalent designed yet (`tsvector` differs materially) — out of 010's scope but a real re-platform cost this ADR does not solve.
- Slightly more ceremony in 010 (everything routed through seam modules) for a payoff that depends on the parked pivot actually happening.

## Alternatives Considered

- **Ignore the pivot, write vec0/SQLite-idiomatic code freely**: rejected — the user has explicitly committed to the re-platform; inlined vec0 SQL across new modules multiplies future rewrite surface (009 already demonstrates this debt).
- **Adopt `sqlx::Any` / runtime dual-backend now**: rejected — heavy abstraction tax and test matrix for a backend that doesn't exist yet; violates smallest-viable-diff; `sqlx::Any` also doesn't cover vec0/pgvector divergence, which is the hard part anyway.
- **Migrate to Postgres now, before 010**: rejected — sequencing was explicitly locked by the user (finish drafting pipeline first); also blocked on a constitution §IV amendment and the client/server transport design.
- **Embedded-Postgres per machine as a stepping stone**: rejected — pays the operational cost of Postgres without the client/server benefit, and still violates constitution §IV today.

## References

- Feature Spec: specs/010-one-ask-drafting/spec.md
- Implementation Plan: specs/010-one-ask-drafting/plan.md (research.md R1, R7; data-model.md migration 025)
- Related ADRs: ADR-0013 (composition architecture — its retrieval/persistence land inside this seam), ADR-0002 (embedding model & schema strategy — vec0 origins)
- Constitution: §IV (SQLite single source of truth — amendment REQUIRED before any actual Postgres execution; this ADR deliberately does not amend it)
- Pivot plan: memory `project_noviq_architecture_pivot_plan` (client/server LAN + Postgres+pgvector, 6-phase, parked until after drafting)
