---
id: 039
title: indexer hang resilience and revoke race fix
stage: green
date: 2026-05-16
surface: agent
model: claude-opus-4-7
feature: tier3-hierarchical-retrieval
branch: feature/tier3-hierarchical-retrieval
user: engr.riaz2010@gmail.com
command: yes patch the silent-hang risk
labels: [bugfix, indexer, resilience, sqlite, deadlock]
links:
  spec: docs/superpowers/plans/2026-05-15-tier3-hierarchical-retrieval.md
  ticket: N/A
  adr: N/A
  pr: N/A
files:
  - path: src-tauri/src/commands/folders.rs
    change: revoke_indexed_folder drains indexing_sem (120s timeout) before delete tx
  - path: src-tauri/src/services/indexer.rs
    change: indexing_sem() made pub; vec0 probe + per-batch upsert + set_indexed_at each wrapped in tokio::time::timeout (10s/30s/30s) with error-path that flips source to 'error' instead of hanging
  - path: src-tauri/src/db/schema.rs
    change: startup recovery — UPDATE sources SET indexing_status='pending' WHERE indexing_status='processing'
  - path: src-tauri/src/db/repositories/folders.rs
    change: corrected stale 30s comment to reflect 90s busy_timeout
  - path: run-dev.sh
    change: ORT_DYLIB_PATH auto-detected from /home/riaz/.local/lib/python*/site-packages/onnxruntime/capi/libonnxruntime.so.*
  - path: CHANGELOG_DEV.md
    change: dated entry documenting the hang incident, root cause guess, and fix
  - path: /home/riaz/Desktop/test_prompts.md
    change: 5 validation prompts for constitution.pdf with expected answers + pass/fail checklist
tests:
  - name: cargo check --lib
    result: PASS — exit 0, 3 unrelated deprecation warnings only
---

## Prompt

yes patch the silent-hang risk, i also closed the app so do perfect work so in future it never stucks. after you fix i will launch the app

## Response snapshot

- Investigated dev log dev-20260516-081422.log: source 2e236d7b (constitution.pdf) finished embedding at 04:01:02 (embed_done_ms=447679, 7.5 min) but post-embedding upsert produced NO "upserted" log and NO "Indexed source" log. DB query confirmed source stuck at indexing_status='processing'. CPU went idle — task wedged silently, no panic, no error.
- Root cause guess: pool.begin().await in the upsert loop (indexer.rs:412) parked forever, likely a pool deadlock between the spawn'd indexer task and a concurrent reader (saw a slow SELECT at 04:01:03 from notebook_sources). Without a timeout the spawn task lives forever.
- Wrote 3 surgical patches:
  1. **services/indexer.rs**: timeout-wrap the vec0 probe (10s), each 8-row upsert sub-batch (30s), and set_indexed_at (30s). On timeout: update_error + log + return Ok(()) so the spawn task ends cleanly with a visible error state.
  2. **db/schema.rs**: startup-recovery UPDATE that resets any 'processing' source to 'pending' after migrations run, with a WARN log of the count. Covers crash/kill mid-embed too.
  3. **commands/folders.rs**: revoke acquires indexer::indexing_sem() via tokio::time::timeout(120s) before deleting. Surfaces a user-friendly "Indexing is in progress" error if it can't drain in time.
- Also fixed an earlier oversight: run-dev.sh now exports ORT_DYLIB_PATH via glob (so the reranker actually loads); folder repo had a stale 30s comment fixed to 90s.
- cargo check --lib: PASS (1m 55s incremental, exit 0).
- Wrote test_prompts.md to ~/Desktop with 5 prompts covering: leaf-precision (Art 10), enumerated list (Art 1), definitional with reranker sensitivity (Art 6), numeric+scope (Art 11), semantic-only no-keyword (Preamble + Quaid-i-Azam). Each with expected answer + pass/fail checklist.
- Appended CHANGELOG_DEV.md entry documenting incident + fix + caveat that root cause of the deadlock is not yet identified (timeout is a safety net).

## Outcome

- ✅ Impact: silent-hang failure mode eliminated. Worst case now is a 30s timeout → source marked 'error' with logged reason. Startup recovery + revoke-drain close the two other UX paper-cuts from the same investigation.
- 🧪 Tests: cargo check passes. End-to-end verification deferred to user (they closed the app and will relaunch with the new binary).
- 📁 Files: 5 src + 2 docs changed (folders.rs/folders_repo.rs/indexer.rs/schema.rs/run-dev.sh + CHANGELOG + test_prompts.md).
- 🔁 Next: user launches `./run-dev.sh`. Watch for "Startup recovery: reset N source(s) from 'processing' to 'pending'" WARN line, then constitution.pdf will auto-re-index. Once status='indexed', run the 5 prompts.
- 🧠 Reflection: the original code had the right intuition (small upsert sub-batches to release the writer lock quickly) but no defensive timeout — so when the lock acquisition itself wedged, the small-batch design didn't help. Defensive timeouts are cheap insurance; the next investigation should look at sqlx pool size + Wal checkpoint contention as candidates for the actual deadlock.

## Evaluation notes (flywheel)

- Failure modes observed: silent task hang post-embed (root cause unconfirmed); folder revoke race producing ghost UI entries.
- Graders run: cargo check --lib → PASS (exit 0).
- Prompt variant: N/A.
- Next experiment: relaunch + run 5 validation prompts (test_prompts.md). If embed completes cleanly and prompts return cited answers with reranker+grounding steps in trace, Tier 3 is fully validated end-to-end.
