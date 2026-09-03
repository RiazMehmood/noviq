---
id: 0002
title: Plan Postgres pgvector re-platform
stage: plan
date: 2026-06-11
surface: agent
model: claude-fable-5
feature: 011-postgres-pgvector-port
branch: 011-postgres-pgvector-port
user: riaz
command: /sp.plan
labels: [postgres, pgvector, hnsw, bundled-engine, postgresql_embedded, migration, constitution-amendment]
links:
  spec: specs/011-postgres-pgvector-port/spec.md
  ticket: null
  adr: history/adr/0014-database-portability-seam-for-postgresql-re-platform.md
  pr: null
files:
  - specs/011-postgres-pgvector-port/plan.md
  - specs/011-postgres-pgvector-port/research.md
  - specs/011-postgres-pgvector-port/data-model.md
  - specs/011-postgres-pgvector-port/contracts/storage-seam.md
  - specs/011-postgres-pgvector-port/quickstart.md
  - CLAUDE.md
tests:
  - "constitution check: 11/13 pass; §IV + stack invariants = intended violations gated on amendment ADR (FR-013)"
---

## Prompt

run /sp.plan — args: 011-postgres-pgvector-port

## Response snapshot

Plan workflow executed for feature 011. Codebase recon sized the port: 57 files
reference SqlitePool but pass the handle (swap at one construction point in
db/mod.rs); dialect fixes enumerable (9× INSERT OR REPLACE in 7 files, 2×
last_insert_rowid, vec0/FTS5 SQL in 15 files); 25 migrations at
src-tauri/src/db/migrations/ squash into one PG baseline. Web research confirmed
postgresql_embedded (theseus-rs) `bundled` feature = compile-time embed of PG 17
binaries into the exe (runtime fully offline, answers the user's exe-distribution
question); pgvector vendored into the bundle (postgresql_extensions or
file-based install). Key decisions (research.md R1–R8): column stays vector(768)
full-precision (bit-faithful migration) with halfvec *expression* HNSW index
(m=16, ef_construction=64) so FR-014 precision cut is index-only and reversible;
FTS5 → generated tsvector column + GIN with websearch_to_tsquery (RRF fusion
untouched); migration via binary COPY (copy_in_raw) with index-built-after-load,
per-table resume + row-count verify; engine lifecycle in db/pg/engine.rs
(loopback only, orphan adoption, per-tier shared_buffers 256M–1G); tests run an
ephemeral bundled instance (dev/prod parity FR-015). Constitution check: §IV and
stack-invariants rows are intended violations — gate = amendment ADR (v2.0.0)
accepted before cutover merges to master; Complexity Tracking justifies vs
sidecar-ANN/Docker/system-PG alternatives. Artifacts: plan.md, research.md,
data-model.md, contracts/storage-seam.md (Rust seam contracts C1–C5, no REST),
quickstart.md (spike gate first: offline initdb+CREATE EXTENSION vector proof).
Agent context updated.

## Outcome

- ✅ Impact: full Phase 0+1 design for the re-platform; offline-bundling mechanism validated as the critical path with a spike gate as first task.
- 🧪 Tests: constitution gate documented (2 intended violations, ADR-gated); no code yet.
- 📁 Files: 5 spec artifacts + CLAUDE.md context update.
- 🔁 Next prompts: /sp.adr postgres-single-source-of-truth (constitution v2.0.0 amendment — consent pending); /sp.tasks 011; then implement starting with the pg_spike offline proof.
- 🧠 Reflection: pool-handle discipline from ADR-0014 turned a 57-file port into a one-point pool swap + ~20 files of dialect fixes — the seam investment paid off before the pivot even started.

## Evaluation notes (flywheel)

- Failure modes observed: none; web search needed two passes to pin the pgvector-offline story (vendored artifacts, not runtime download).
- Graders run and results (PASS/FAIL): constitution check PASS-with-gated-violations; artifact completeness PASS.
- Prompt variant (if applicable): n/a
- Next experiment (smallest change to try): pg_spike test — initdb→start→CREATE EXTENSION vector→vector(768) round-trip with networking disabled, before any port work.
