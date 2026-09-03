---
id: 0007
title: T018 gate pass and draft quality Fix A
stage: green
date: 2026-06-12
surface: agent
model: claude-fable-5
feature: 011-postgres-pgvector-port
branch: 011-postgres-pgvector-port
user: riaz
command: live gate + draft-quality iteration
labels: [T018, live-gate, oomd, password-persistence, draft-quality, retrieval-relevance]
links:
  spec: specs/011-postgres-pgvector-port/spec.md
  ticket: null
  adr: history/adr/0015-bundled-postgresql-pgvector-engine.md
  pr: null
files:
  - src-tauri/src/db/pg/engine.rs
  - src-tauri/src/services/drafting/one_ask/research.rs
  - CHANGELOG_DEV.md
  - specs/011-postgres-pgvector-port/tasks.md
tests:
  - cargo test --lib drafting::one_ask → 24/24
  - cargo test --test one_ask_acceptance → 3/3; one_ask_knn_parity → 2/2
---

## Prompt

User ran the T018 live gate across the session: app crashes diagnosed in sequence
(NTFS app-data symlink → EINVAL on engine extract; oomd kill at model load;
"password authentication failed for user postgres" on relaunch), then fresh-indexed
10 docs, ran a one-ask summary, pasted the full output, and said:
"start on draft quality and also delete that 65GB echo.db" … "after fix A save
progress i am going for some work then as i return we will perform fix B and C"

## Response snapshot

T018 PASSED: research KNN 2.0 s (was 1678 s), 0 oomd kills, draft end-to-end,
278 MB store vs 65 GB. Fixes en route: pg root relocated to ext4 (PG cannot run
on fuseblk/NTFS); root oomd drop-in applied (95% threshold); engine.rs now
reuses the persisted .pgpass password (Settings::default() generated a new
random password per launch → every restart failed auth). Old 65 GB echo.db
deleted (user-authorized; NTFS now 98 GB free). Draft-quality diagnosis tied
each symptom to code: (1) verify.rs splices the WHOLE chunk excerpt per
citation; (2) research.rs has no relevance cutoff and unlimited card reuse
across sections; (3) Flag runs render mid-sentence. Fix A implemented:
relative DISTANCE_MARGIN=0.15 cutoff + MAX_SECTIONS_PER_CARD=2, with unit
tests; acceptance + parity suites green.

## Outcome

- ✅ Impact: F011 MVP path complete (T001–T018); draft-quality workstream opened on the 010 writer pipeline with Fix A landed.
- 🧪 Tests: one_ask lib 24/24, one_ask_acceptance 3/3, one_ask_knn_parity 2/2.
- 📁 Files: engine.rs (password persistence, committed 92b1da33), research.rs (Fix A), CHANGELOG, tasks.md.
- 🔁 Next prompts: Fix B (short verbatim snippet per citation in verify.rs), Fix C (flags collected at paragraph end), then ONE live summary re-run compared against today's baseline output.
- 🧠 Reflection: the citation-safety gate held under a hostile real run (every invented ref flagged); quality failures were all deterministic pipeline choices, not model behavior — exactly where the citation-grade architecture wants them.

## Evaluation notes (flywheel)

- Failure modes observed: KNN without relevance cutoff invents relevance; per-launch random credentials break restart; PG on fuseblk fails at extract with bare EINVAL (needs T021 detection).
- Graders run and results (PASS/FAIL): T018 gate PASS; baseline draft saved in conversation for B/C comparison.
- Prompt variant (if applicable): n/a
- Next experiment (smallest change to try): Fix B — quoted snippet = first sentence (≤ ~30 words) of the card excerpt, still verbatim-verified.
