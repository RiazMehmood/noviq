---
id: 0008
title: Diagnose oomd kill + ETA to elapsed timer
stage: misc
date: 2026-06-11
surface: agent
model: claude-fable-5
feature: 010-one-ask-drafting
branch: 010-one-ask-drafting
user: riaz
command: direct request
labels: [diagnosis, systemd-oomd, perf, vec0-knn, ux, elapsed-timer]
links:
  spec: specs/010-one-ask-drafting/spec.md
  ticket: null
  adr: history/adr/0014-vec0-knn-retrieval-seam.md
  pr: null
files:
  - src/components/drafting/OneAskView.tsx
  - CHANGELOG_DEV.md
tests:
  - "npx tsc --noEmit (clean)"
  - "cargo test --lib drafting (70/70)"
  - "cargo test --test one_ask_acceptance (3/3)"
---

## Prompt

recall memory, and diagnose the reason for killed state? and i dont want estimated time it is showing 500 min sometime 600 min, so i want it to show total time of executive just like timer which calculate total time how much time it take to execute the summary, percentage is ok.

## Response snapshot

Diagnosis (definitive, from journalctl): systemd-oomd killed `echoai-dev.scope`
(119 processes) at 19:25:38 PKT 2026-06-11 — user-slice memory pressure 57.01% >
50% for >20 s. Not a panic, not kernel OOM, not the tauri file-watcher, not battery
(GNOME shutdown came 24 min later at 19:49). run-dev.sh's existing
`ManagedOOMMemoryPressureLimit=100%` scope wrapper is ineffective because oomd
monitors the parent `user@1000.service` (Ubuntu 50% default) and selects the
heaviest descendant cgroup as victim; `ManagedOOMPreference=avoid` on user units
requires systemd ≥252 (machine runs 249). Root driver: the vec0 KNN `MATCH ?/k=?`
research queries took 1678 s and 1247 s each on the live 65 GB chunk_embeddings
(sqlx slow-statement warnings in logs/dev-20260611-181141.log) — sqlite-vec vec0
is brute force, not ANN, so each query reads the whole table from disk →
page-cache thrash → sustained memory pressure → oomd kill. This also explains the
"500–600 min" ETA: linear extrapolation at 7% after ~40 min is ~530 min — honest
math over a broken stage. UI fix shipped: ETA replaced with elapsed stopwatch
(`m:ss` / `h:mm:ss`) next to the kept percent; new `finishedAt` signal; finished
draft header shows "⏱ drafted in X" beside Copy. Mitigation commands for oomd
(root drop-in raising the user@.service pressure limit) given to user to run.

## Outcome

- ✅ Impact: killed-state root-caused (oomd + brute-force KNN on 65 GB); ETA replaced by elapsed timer + total-time-on-completion; pending 43ce1354 test verification closed.
- 🧪 Tests: tsc clean; drafting lib 70/70; one_ask_acceptance 3/3.
- 📁 Files: OneAskView.tsx (timer), CHANGELOG_DEV.md (entry), memory updated.
- 🔁 Next prompts: research-stage perf strategy (T035 gate failing ~100×; ANN/pre-filter vs pgvector pivot); apply oomd drop-in; commit UI change.
- 🧠 Reflection: the watcher-rebuild hypothesis from the battery-emergency note was wrong — journal evidence beats inference; sqlx slow-statement logs were the key artifact.

## Evaluation notes (flywheel)

- Failure modes observed: previous session saved an unverified hypothesis as the leading explanation; vec0 KNN parity test on a small fixture gave false confidence about live-scale latency.
- Graders run and results (PASS/FAIL): tsc PASS; drafting lib PASS; acceptance PASS.
- Prompt variant (if applicable): n/a
- Next experiment (smallest change to try): time one vec0 KNN query on the live DB with a metadata pre-filter (folder/source scoping) to see if the scan can be bounded before considering ANN/pgvector.
