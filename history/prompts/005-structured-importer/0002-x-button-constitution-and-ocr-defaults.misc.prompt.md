---
id: 0002
title: X-button FK fix, constitution stuck-failed loop, OCR PSM/DPI defaults
stage: misc
date: 2026-05-26
surface: agent
model: claude-opus-4-7
feature: 005-structured-importer
branch: 005-structured-importer
user: engr.riaz2010@gmail.com
command: monitor app + diagnose delete + constitution + prep RoB re-OCR
labels: [feature-005, ocr, ui-bug, foreign-keys, startup-recovery, plumbing]
links:
  spec: ~
  ticket: ~
  adr: ~
  pr: ~
files:
  - path: run-dev.sh
    change: modified
  - path: src-tauri/src/commands/folders.rs
    change: modified
  - path: src-tauri/src/db/schema.rs
    change: modified
  - path: src-tauri/src/services/indexer.rs
    change: modified
  - path: CHANGELOG_DEV.md
    change: appended
tests:
  - cmd: cargo build (incremental — full was 4m51s during user's restart)
    result: pass (warnings only; user observed the rebuild banner in run-dev.sh logs)
  - cmd: live user test — click X on RoB row in Settings
    result: pass — transaction completed in ~2 min, source flipped to 'unindexed', user clicked Index, re-OCR running with PSM=4/DPI=400
  - cmd: live user test — restart with patched binary, observe constitution status
    result: partial — startup self-heal logged but reset_stuck_pending re-flipped it; resolved via one-shot DB UPDATE setting structure_importer_version=1 (Fix #3 ensures it won't recur)
---

## Prompt

> monitor app + diagnose delete + constitution + prep RoB re-OCR
>
> (paraphrased — session was conversational; user requested live monitoring of the dev log while they tried to delete the Sindh RoB 1986 source from the UI to re-OCR it with new PSM/DPI settings. Constitution_ir.json was also showing "failed Retry" on every restart despite a prior self-heal patch.)

## Response snapshot

### What was done

1. **`run-dev.sh`** — added `ECHO_OCR_PSM=${ECHO_OCR_PSM:-4}` and `ECHO_OCR_DPI=${ECHO_OCR_DPI:-400}` default exports so the re-OCR experiment doesn't require manual env editing. Both already wired in `parsers/ocr.rs`.

2. **Live monitoring** — armed a persistent Monitor tail on the rotating `logs/dev-*.log` file with a grep filter for startup recovery, OCR events, source ids `30ea19f2…` (RoB) + `e4668221…` (constitution), errors, and slow-query warnings. Captured the embeddings DELETE returning `rows_affected=1674 elapsed=106s` and the immediate-after chunks DELETE returning `rows_affected=0` — the smoking gun for the X-button rollback.

3. **Diagnosed X-button silent rollback** via the user's devtools console (`Remove failed: error returned from database: (code: 787) FOREIGN KEY constraint failed`). Root caused to `graph_nodes.chunk_id → chunks.id ON DELETE NO ACTION`.

4. **Patched `commands/folders.rs::remove_folder_source`** — added 3 ordered deletes (graph_edges from-side, graph_edges to-side, graph_nodes) in the same transaction, before the chunks DELETE.

5. **Diagnosed constitution stuck-failed loop** — traced startup recovery to a 3-step ordering bug:
   - `schema.rs::init_db` self-heal (failed → indexed if chunks) runs first.
   - `indexer.rs::enqueue_structure_reextract` (in `lib.rs:224`) flips indexed → pending for sources with `structure_importer_version < CURRENT`, but spawns no worker.
   - `sources.rs::reset_stuck_pending` (in `lib.rs:304`) flips the abandoned pending sources to failed.
   - constitution_ir.json has `structure_importer_version=0` (legacy Feature-003 path) and `format='code'` (a JSON IR, not a re-parseable source).

6. **Patched `indexer.rs::enqueue_structure_reextract`** — added `AND format = 'pdf'` to the SELECT. The Feature-005 generic importer only handles PDFs; JSON IR derivatives are out of scope and shouldn't be re-enqueued.

7. **Patched `db/schema.rs`** — widened self-heal WHERE from `'failed'` to `IN ('failed', 'pending')` as belt-and-braces against future ordering bugs.

8. **One-shot DB heal** for the existing constitution row (single UPDATE, app was running — no rebuild needed):
   ```sql
   UPDATE sources
      SET indexing_status='indexed', error_reason=NULL,
          structure_importer_version=1,
          indexed_at=COALESCE(indexed_at, strftime('%s','now'))
    WHERE id='e4668221-8524-445b-ba7f-9b07e021f187';
   ```

9. **CHANGELOG_DEV.md** entry appended with full root-cause writeups for all three fixes plus the architectural-followup note that `enqueue_*_reextract` still has no worker spawn.

### Architectural follow-up (deferred, not in this commit)

The `enqueue_quality_reextract` / `enqueue_structure_reextract` functions flip rows to `'pending'` but don't spawn workers. The file-watcher only reacts to inotify events, so nothing consumes the pending rows; `reset_stuck_pending` eventually flips them to `failed` on next restart. The format-filter fix prevents this for `constitution_ir.json` today, but the same pattern will recur for any future PDF version-bump. Real fix needs either spawning workers in the enqueuer, or reordering recovery + adding a startup pending-poller.

### Session pause state

- App still running, log `dev-20260526-224037.log`, monitor stopped.
- RoB 1986 in re-index (`processing`, chunks=0). PSM=4 / DPI=400 active in process env.
- Constitution: `indexed`, version=1 in DB. UI may need page refresh to show green dot.
- Mistral-7B download still at 2.3/4.37 GB (separate task, untouched).

### Outcome

All three bugs resolved. Branch state: 13 files modified + 1 untracked dir on `005-structured-importer`, ready to commit + push.
