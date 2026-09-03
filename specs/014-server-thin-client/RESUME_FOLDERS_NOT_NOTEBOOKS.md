# ▶ Resume — F014 product pivot: "index folders, no notebooks"

> Written 2026-06-26. Branch `014-server-thin-client`. Decision (from architect):
> **stop creating notebooks; users index folders only.** This file captures the
> full code analysis behind that decision so the next session acts immediately.
> Read `CHANGELOG_DEV.md` first.

## The decision
The browser thin-client (and the product going forward) does **not** create
notebooks or add individual files. Users **index a folder** (a path on the
**server/VM** filesystem — the central corpus). Queries already run over the
**whole corpus**, so notebooks add nothing.

## What I verified (so you don't re-derive it)

1. **Folder indexing already works end-to-end server-side.** No FK bug on this path.
   - `services/indexing.rs::index_folder` → `db/repositories/folders.rs::insert`
     inserts the `indexed_folders` row **and** a stub `notebooks` row with id
     `__folder__<folder_id>` (ON CONFLICT DO NOTHING) in one tx — this satisfies
     `notebook_sources.notebook_id REFERENCES notebooks(id)` (migration 001).
   - `folders.rs::backfill_stub_notebooks` heals pre-existing folders missing the stub.
   - `notebooks.rs::list` already hides `__folder__%` and `__batch__%` ids, so the
     stubs never appear in any notebook UI.
   - `indexer::index_file` uses synthetic notebook id `__folder__<id>` and the
     attach succeeds because the stub exists.

2. **The T053 gap is ONLY the notebook-based path**, which this decision retires:
   - `POST /api/add_sources {notebookId, filePaths}` → fails the FK because no
     create-notebook HTTP route exists. **Superseded — deprecate/remove it.**

3. **Query/chat is whole-corpus, notebook-agnostic.** `services/assistant/mod.rs::
   all_indexed_source_ids` = `SELECT id FROM sources WHERE indexing_status='indexed'`.
   `conversation_id` is only a session/trace key (see `capabilities/guidance.rs:158`).
   → **No retrieval/scoping change needed.**

4. **Root cause of the original breakage:** SQLite doesn't enforce FKs by default;
   the PG port (F011/F012) does → the notebook-FK path that "worked" on SQLite now
   rejects. Folder path was already safe because of the stub insert.

## The real remaining gap = FRONTEND (browser can't index anything today)
`src/components/notebook/SourceList.tsx`:
- uses `@tauri-apps/plugin-dialog`'s `open()` (native dialog) → **no-op in a browser**,
- calls `addSources(notebookId, …)` → the broken notebook path.

Neither works over the web transport. A browser user currently **cannot add docs.**

### Plan for next session (local, no VM)
1. **Add a web folder-indexing surface.** Server folders are paths on the VM, so the
   browser needs a **text input for an absolute server path** + "Index folder" button
   → `addIndexedFolder(folderPath)` (already in `src/api/index.ts:282`, endpoint
   `add_indexed_folder` already mounted in `api/router.rs:61`).
   - Suggested home: a new `/sources` route (App.tsx routes at line 149-157) or a
     section in `SettingsPage.tsx`. Decide placement with architect.
   - List folders via `listIndexedFolders()` (`list_indexed_folders` mounted);
     per-file progress already streams via `onIndexProgress` (Tauri listen on
     desktop, `EventSource('/api/index/events?token=…')` on web — done in T024/T025).
   - Keep the native dialog path for desktop (`window.__TAURI__` present) as a branch.
2. **Deprecate notebook UI for the thin client** — hide/remove notebook create + file
   `addSources` in the web build. Leave desktop notebooks intact (smallest diff:
   gate the notebook surfaces, don't delete the modules).
3. Optionally remove `/api/add_sources` + `/api/list_notebook_sources` routes
   (`api/router.rs:54,60`) once the frontend no longer calls them.
4. `cargo check --lib` + `cargo check --features server --bin noviq-server` + `tsc --noEmit`.

## Still VM-only (unchanged, blocked on GPU box)
- T026 CUDA build — **DONE on VM** (CHANGELOG 2026-06-26); tasks.md checkbox is stale, tick it.
- T028 GPU-vs-CPU latency — needs an indexed folder first; now unblocked by the above.
- T021 DB-backed `api_auth` test (bundled PG, `--ignored`).
- T051 finalize `quickstart.md`; T052 acceptance SC-001..010.

## Files touched this session
None (analysis + this doc + CHANGELOG entry only). No code changed.
