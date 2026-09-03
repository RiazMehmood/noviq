---
id: 0003
title: ADR Postgres single source of truth
stage: plan
date: 2026-06-11
surface: agent
model: claude-fable-5
feature: 011-postgres-pgvector-port
branch: 011-postgres-pgvector-port
user: riaz
command: /sp.adr postgres-single-source-of-truth
labels: [adr, postgres, pgvector, constitution-amendment, bundled-engine, supersession]
links:
  spec: specs/011-postgres-pgvector-port/spec.md
  ticket: null
  adr: history/adr/0015-postgresql-as-single-source-of-truth-bundled-engine-pgvector-hard-cutover.md
  pr: null
files:
  - history/adr/0015-postgresql-as-single-source-of-truth-bundled-engine-pgvector-hard-cutover.md
  - history/adr/0014-database-portability-seam-for-postgresql-re-platform.md
tests:
  - "significance checklist: impact + alternatives + scope all hold; clustered (not atomic); 7 alternatives with rationale"
---

## Prompt

run /sp.adr postgres-single-source-of-truth

## Response snapshot

Created **ADR-0015** "PostgreSQL as Single Source of Truth (Bundled Engine,
pgvector, Hard Cutover)" — one clustered ADR covering: PG 17 replaces SQLite for
ALL state (hard cutover, no dual backend); engine bundled inside the exe via
postgresql_embedded `bundled` (compile-time embed, offline runtime, app-managed
child process, loopback-only, dev/prod parity); pgvector vendored, column
vector(768) full-precision + halfvec-expression HNSW (index-only reversible
precision cut); FTS5 → generated tsvector + GIN (RRF unchanged, rebuilt not
migrated); 25 migrations → 1 PG baseline; one-time binary-COPY migration with
resume/verify; per-tier RAM tuning; constitution v2.0.0 amendment (§IV + 3 stack
rows) as the gating follow-through before cutover merges (FR-013). Status:
Accepted-with-gate. **Conflict detected and resolved**: ADR-0014 had explicitly
rejected "embedded-Postgres per machine as a stepping stone" — ADR-0015
supersedes that single rejection bullet (context changed: oomd kill made
brute-force KNN a single-machine blocker; §IV is being amended, not avoided);
ADR-0014's seam design rules remain in force. ADR-0014 status line annotated to
point at ADR-0015. Referenced existing: ADR-0014, ADR-0002, ADR-0013.

## Outcome

- ✅ Impact: the architectural decision gating Feature 011 cutover is documented; ADR chain coherent (0014 ↔ 0015 cross-annotated).
- 🧪 Tests: significance checklist PASS (clustered, alternatives w/ rationale, pros+cons, concise).
- 📁 Files: ADR-0015 created; ADR-0014 status annotated.
- 🔁 Next prompts: /sp.tasks 011 (task generation); during implementation, the actual constitution v2.0.0 amendment edit (Sync Impact Report + version bump) as its own task gated before master merge.
- 🧠 Reflection: an ADR that contradicts a prior ADR's rejection must supersede it by name with the changed context — silent contradiction would have left the record incoherent.

## Evaluation notes (flywheel)

- Failure modes observed: create-adr.sh requires --title flag (first call without it failed cleanly).
- Graders run and results (PASS/FAIL): ADR significance + clustering checklist PASS.
- Prompt variant (if applicable): n/a
- Next experiment (smallest change to try): n/a — proceed to /sp.tasks.
