# ADR-0015: PostgreSQL as Single Source of Truth (Bundled Engine, pgvector, Hard Cutover)

- **Status:** Accepted (decision); **gating follow-through**: constitution amendment to v2.0.0 (§IV + stack-invariants rows) MUST be ratified before Feature 011 cutover merges to master (FR-013)
- **Date:** 2026-06-11
- **Feature:** 011-postgres-pgvector-port (cross-cutting: all persistence, retrieval, indexing, packaging)
- **Context:** The one-ask drafting research stage (Feature 010) is unusable on the real corpus: sqlite-vec 0.1.x vec0 KNN is optimized brute force, not ANN — each top-K lookup on the live 65 GB `chunk_embeddings` took 1247–1678 s, thrashed the page cache, and triggered a systemd-oomd kill of the app (T035's "research ≤ 15 s" gate fails ~100×). The previously parked client/server pivot (memory: `project_noviq_architecture_pivot_plan`) was un-parked by the user 2026-06-11 ("move to postgres now, due to oomd kill"), scoped to Phases 0–2 only: single-machine mode, no LAN/networking. A hard distribution constraint surfaced by the user: the app ships to government officers as a plain installer/.exe — users can never be asked to install Docker or a database server. Constitution §IV ("SQLite as Single Source of Truth — no external database processes, no Postgres") and three Technology Stack Invariants rows (sqlite-vec, FTS5, SQLite via sqlx) directly conflict; governance permits deviation only via amendment with ADR — this is that ADR.

## Decision

An integrated cluster — these components stand or fall together:

- **Store**: PostgreSQL 17.x replaces SQLite as the single source of truth for ALL persistent state. Hard cutover — no dual-backend abstraction, no `sqlx::Any`; the old `echo.db` is retained on disk read-only only until the user confirms migration success.
- **Distribution/lifecycle**: the engine ships **inside the app** via the `postgresql_embedded` crate's `bundled` feature (PG binaries embedded into the application binary at *compile* time; runtime is fully offline). The app owns the engine as a managed child process: first-run initdb into `{app_data_dir}/pgdata/`, health-check, orphan adoption, clean shutdown, loopback/Unix-socket only. End users install one exe — no Docker, no system PostgreSQL, no admin steps. Developers and tests use the same bundled path (dev/prod parity); an external instance is an opt-in dev convenience only.
- **Vector search**: pgvector ≥ 0.8, vendored into the package and installed into the bundled engine offline. Column stays full-precision `vector(768)` (bit-faithful migration); ANN via **HNSW over a halfvec expression index** (`(embedding::halfvec(768)) halfvec_cosine_ops`, m=16, ef_construction=64) — the precision cut is index-only and reversible if the ≥ 95 % retrieval-parity gate fails. All vector SQL stays inside the ADR-0014 seam (`retrieval.rs`).
- **Full-text search**: FTS5 → stored generated `tsvector` column + GIN index, queried via `websearch_to_tsquery` and ranked with `ts_rank_cd`; the RRF fusion + boost stack downstream is engine-agnostic and unchanged. FTS index is rebuilt, not migrated.
- **Schema**: the 25 SQLite migrations are squashed into one native PG baseline; the additive-migration discipline and `TARGET_SCHEMA_VERSION` runner continue on PG. Dialect mapping: IDENTITY PKs + `RETURNING id`, `ON CONFLICT`, `$N` placeholders, JSON-TEXT → JSONB.
- **Data migration**: one-time in-app streaming migration (binary COPY via sqlx `copy_in_raw`), per-table resume markers, pre-flight disk check, HNSW built after bulk load, per-table row-count verification + vector bit-fidelity spot-check, final report UI.
- **RAM coexistence**: engine tuned per hardware tier (shared_buffers 256 MB–1 GB) so PG + the warm 3B + demand-loaded 7B respect the single-model-slot RAM budget (Principle III untouched).
- **Governance**: constitution v2.0.0 amends §IV to "a single app-managed local store (bundled PostgreSQL + pgvector); no user-installed or remote database processes" and updates the three stack-invariant rows. This ADR also **activates the parked half of ADR-0014** (the seam ADR), and supersedes ADR-0014's rejection of "embedded-Postgres per machine" (see Alternatives).

## Consequences

### Positive

- Research-stage lookup goes from 1247–1678 s (brute force over 65 GB) to sub-second indexed ANN — removes 010's blocking issue and the oomd-kill memory-pressure driver itself (no more whole-table page-cache scans).
- Distribution UX is unchanged from SQLite: one installer, zero setup, fully offline (Principle I intact) — the engine is an invisible internal component.
- The LAN/client-server pivot (Phases 3–5, still parked) becomes a networking change, not a storage change: same engine, same SQL, flip `listen_addresses` + add auth when that spec lands. The migration tax is paid once.
- Index-only halfvec precision cut halves index size/build time while keeping stored vectors exact, so FR-014 is reversible by re-indexing alone.
- 65 GB sqlite-vec bloat and the slow vec0/HNSW insert path (historic indexing pain) are retired for free.

### Negative

- A child database process is real operational surface a single-file DB never had: lifecycle bugs, port/socket conflicts, orphan processes, AV/signing flags on Windows for bundled server binaries, +60–80 MB installer.
- Hard cutover means a one-shot migration of a 65 GB live DB (COPY + multi-hour HNSW build, SC-004 budget ≤ 4 h) with no dual-backend safety net beyond keeping the old file read-only.
- ANN is approximate and `ts_rank_cd` ranks differently than FTS5 bm25 — retrieval drift is real and only bounded by the SC-005 parity gate (≥ 95 % top-K overlap + verbatim verify gate downstream).
- 009-era code that inlines vec0/FTS5 SQL (debt noted in ADR-0014) must be swept now rather than later — the port pays down that debt on a deadline.
- PG memory tuning vs the model slot is a new balancing act on 8–16 GB machines (mitigated by tier-based settings, but a real regression risk to watch in SC-002's 10-run gate).

## Alternatives Considered

- **Stay on SQLite + sidecar ANN index (usearch/hnswlib next to echo.db)**: fastest local fix for the KNN problem only — rejected: creates a second source of truth for vectors with custom WAL/compaction/crash-consistency burden, keeps FTS5 + the 65 GB bloat, and dead-ends before the LAN pivot (pays a migration twice).
- **SQLite + pre-filtering/partition tricks on vec0**: rejected — still O(corpus) per query with no index; the PHR-0008 experiment idea was superseded by the user's pivot decision.
- **Docker-distributed PostgreSQL**: rejected — unshippable to officer desktops (the user's exe question); Docker remains, at most, an opt-in developer convenience.
- **Require a system PostgreSQL install**: rejected — violates zero-setup/offline install (FR-004), admin rights and version conflicts on government machines.
- **pgvecto.rs / VectorChord / pgvectorscale instead of pgvector**: rejected for v1 — extra/less-mature extensions; pgvector HNSW is sufficient below ~50 M vectors. pgvectorscale (StreamingDiskANN) noted as a server-phase option.
- **Dual-backend abstraction (run SQLite and PG side by side)**: rejected — heavy abstraction tax and double test matrix for a transition state nobody ships; ADR-0014 already rejected `sqlx::Any` for the same reason.
- **Note on ADR-0014's earlier rejection of "embedded-Postgres per machine as a stepping stone"**: that rejection assumed the only benefit was client/server preparation and that §IV stood. Context changed: brute-force KNN is now a *blocking* single-machine problem (oomd kill), and §IV is being amended, not avoided. This ADR explicitly supersedes that single rejection bullet; ADR-0014's seam design rules remain in force and are what make this port small.

## References

- Feature Spec: specs/011-postgres-pgvector-port/spec.md
- Implementation Plan: specs/011-postgres-pgvector-port/plan.md (+ research.md R1–R8, data-model.md, contracts/storage-seam.md, quickstart.md)
- Related ADRs: ADR-0014 (portability seam — design rules in force; its parked "migration-execution half" is activated here; its embedded-PG rejection superseded), ADR-0002 (embedding model/schema — vec0 origins), ADR-0013 (one-ask composition — its retrieval rides this seam)
- Constitution: §IV + Technology Stack Invariants — amendment to v2.0.0 required before cutover merges (FR-013); Governance amendment procedure §1–3
- Evaluator Evidence: PHR 010/0008 (oomd diagnosis: journalctl + sqlx slow-statement logs `logs/dev-20260611-181141.log`, 1678 s/1247 s queries); PHR 011/0001–0002; live T035 gate failures
