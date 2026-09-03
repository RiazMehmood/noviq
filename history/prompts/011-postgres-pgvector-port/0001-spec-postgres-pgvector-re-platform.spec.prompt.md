---
id: 0001
title: Spec Postgres pgvector re-platform
stage: spec
date: 2026-06-11
surface: agent
model: claude-fable-5
feature: 011-postgres-pgvector-port
branch: 011-postgres-pgvector-port
user: riaz
command: /sp.specify
labels: [postgres, pgvector, re-platform, architecture-pivot, bundled-engine, migration]
links:
  spec: specs/011-postgres-pgvector-port/spec.md
  ticket: null
  adr: history/adr/0014-database-portability-seam-for-postgresql-re-platform.md
  pr: null
files:
  - specs/011-postgres-pgvector-port/spec.md
  - specs/011-postgres-pgvector-port/checklists/requirements.md
tests:
  - "spec quality checklist: all items pass"
---

## Prompt

resume — then: "yes were planning to move to postgres now" / "due to oomd kill" /
"and u suggest dockerd for postgres but my question was that if we do that for now,
how our clients like when we convert the app into exe file then how users will get that"

/sp.specify Re-platform persistence from SQLite to PostgreSQL + pgvector (single-machine mode). Driver: the one-ask drafting research stage runs vec0 KNN MATCH queries on the live 65 GB chunk_embeddings table; sqlite-vec 0.1.x vec0 is brute-force (not ANN), each query took 1678s/1247s, causing page-cache thrash and a systemd-oomd kill of the dev app. T035's "research ≤15 s" gate fails ~100×. Scope = Phases 0–2 of the parked architecture pivot plan: (0) ADR amending constitution §IV "SQLite single source of truth"; (1) complete the repository/transport seam started by ADR-0014 (retrieval.rs vector seam already landed in 010) with no behavior change; (2) port persistence to Postgres + pgvector with HNSW ANN index for chunk_embeddings, halfvec considered for ~50% storage cut, FTS5 → tsvector/GIN keeping the RRF hybrid retrieval, all 25 SQLite migrations re-expressed, INSERT OR REPLACE → ON CONFLICT, plus a one-time data migration path from the existing echo.db. Single-machine "both" mode only — Tauri app talks to a local Postgres instance; NO LAN networking, NO client/server roles, NO mDNS/auth (those are a later spec). Success criteria: research-stage KNN query ≤ 15 s on the live corpus (T035 gate), indexing insert path no slower than today, all existing drafting/retrieval test suites green against Postgres, existing corpus migrated without re-indexing or re-embedding.

## Response snapshot

Resumed session: committed the verified elapsed-timer change as b1b47aa2 on 010.
User un-parked the architecture pivot ("move to postgres now, due to oomd kill");
chose scope = Phases 0–2 only via AskUserQuestion. User's distribution question
(how do exe users get Postgres if we use Docker?) answered and baked into the spec
as FR-004/FR-015 + User Story 2: the engine ships INSIDE the app package (portable
bundled binaries, app-managed child process, loopback-only), Docker is never a user
requirement and at most an opt-in dev convenience. Created branch
011-postgres-pgvector-port (branched from 010 HEAD — retrieval.rs seam lives there),
wrote spec.md: 4 prioritized stories (fast research P1, zero-setup install P2,
no-re-embed migration P3, test/seam parity P4), 15 FRs, 7 SCs (≤15 s lookup, 10
clean runs, ≥95% top-K parity, ±10% indexing, ≤4 h migration), edge cases (port
conflict, orphan process, interrupted migration, disk space, ANN drift, AV
flagging), assumptions (hard cutover, dev/prod parity, FTS rebuilt not migrated).
Checklist requirements.md: all items pass, 0 NEEDS CLARIFICATION.

## Outcome

- ✅ Impact: Feature 011 specced; pivot Phases 0–2 formally un-parked; bundled-engine distribution decision captured.
- 🧪 Tests: spec quality checklist all-pass (no automated tests at spec stage).
- 📁 Files: spec.md + checklists/requirements.md created on new branch.
- 🔁 Next prompts: /sp.plan (research bundled-Postgres mechanism, e.g. postgresql_embedded crate + pgvector extension provisioning; HNSW vs halfvec; migration tool design); ADR superseding constitution §IV (FR-013, consent already implied by pivot decision); optionally /sp.clarify first.
- 🧠 Reflection: user's mid-flight distribution question was a spec requirement in disguise — captured as FR-004/FR-015 instead of answering ephemerally.

## Evaluation notes (flywheel)

- Failure modes observed: none this exchange; Write-to-scaffold required prior Read (template file pre-created by script).
- Graders run and results (PASS/FAIL): checklist self-validation PASS (all items).
- Prompt variant (if applicable): n/a
- Next experiment (smallest change to try): in /sp.plan, prototype app-managed bundled Postgres + pgvector startup on this machine before committing to the mechanism.
