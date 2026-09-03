---
id: 105
title: Fix folder-delete cascade bug, Postgres cleanup, delete confirmation dialog
stage: green
date: 2026-07-26
surface: agent
model: claude-opus-5
feature: corpus-management
branch: release-deploy
user: Riaz Mehmood
command: (freeform request)
labels: [bugfix, data-integrity, postgres, admin-ui, gpu-indexing]
links:
  spec: n/a
  ticket: n/a
  adr: n/a
  pr: n/a
files:
  - src-tauri/src/db/repositories/folders.rs
  - src-tauri/src/api/documents.rs
  - src-tauri/src/api/router.rs
  - src/api/index.ts
  - src/components/IndexedFolders.tsx
  - CHANGELOG_DEV.md
tests:
  - "frontend tsc --noEmit — PASS"
  - "Rust: NOT COMPILED (no toolchain on this box); hand-verified type bounds and callers"
  - "no new unit tests (folders.rs tests need a live Postgres)"
---

## Prompt

fix this bug, and update code from sqlite era to postgres, and you need two fixes, one as i press delet folder from admin ui it shows warning it will delet those files completly do u want to proceed, then if user says yes it removes each and everything from db, and user again indexes files from admin ui it indexed via gpu.

## Response snapshot

**The bug.** `folders::delete()` cascaded from one predicate — `sources.file_path LIKE
'<folder_path>%'` — inside `if let Some(f) = folder`, while `DELETE FROM indexed_folders` ran
**outside** that block, unconditionally. Uploaded corpora link sources to a folder through the
`__folder__<id>` stub notebook rather than by path (migration 009's backfill joins exactly that way),
so the LIKE matched zero rows: folder disappeared from the UI, every chunk and embedding survived,
caller reported success. That is why re-uploading 18 documents produced 30 (= 12 stale + 18 new),
with two competing answers to the SPPRA Rule 17(1) threshold.

**Fixed** by selecting sources on **both** membership signals (path prefix UNION stub-notebook link)
and keying every delete on that id set; returning `FolderDeleteCounts`; `warn!` when a folder row is
removed having matched 0 sources; new read-only `delete_preview()` + `POST /folder_delete_preview`;
counts recorded in the audit event; and a frontend `confirm()` quoting real numbers plus a visible
warning when the server reports 0 sources removed.

**SQLite→Postgres.** Verified against the real schema rather than trusting the comments:
`chunk_embeddings.chunk_id → chunks(id) ON DELETE CASCADE` and `chunks.source_id → sources(id) ON
DELETE CASCADE` both exist, so the "vec0 virtual table, FK CASCADE does not apply" comment was
false. Kept the explicit deletes (the counts we report are a promise) but rewrote the comments to
say why. `graph_nodes`/`graph_edges` pre-deletes are genuinely still required — FK NO ACTION.

**GPU indexing — declined to build; it is an infrastructure regression, not a missing feature.**
The button already exists (`Admin.tsx:596` → `/admin/gpu_reindex` → trigger file → systemd path unit
→ `gpu-reindex.sh`), with its own confirm dialog. It broke when the 2026-07-25 disk cleanup deleted
`/opt/noviq/ort-gpu-spike/`. Live state: `ECHO_EMBED_GPU=1` set, but `ORT_DYLIB_PATH` points at a
**CPU-only** ORT (16.5 MB, 0 CUDA dynamic symbols) and `LD_LIBRARY_PATH` is unset — and `embedder.rs`
appends `CPUExecutionProvider` as a deliberate fallback, so it degrades silently. In-process GPU
indexing at upload time is not achievable here anyway: ~1.4 GB free VRAM because vLLM holds 9.6 GB of
the 12 GB slice, which is why the script stops vLLM and restarts the server first.

## Outcome

- ✅ Impact: folder delete now actually deletes, reports what it removed, and warns before doing it.
  The silent-success path that corrupted the corpus is closed.
- 🧪 Tests: `tsc --noEmit` PASS. **Rust not compiled** — no toolchain locally. Hand-verified:
  `crate::db::Driver` does not exist (corrected the generic bound to `sqlx::Postgres`), `serde`/`log`
  in scope, desktop caller safely discards the new return value.
- 📁 Files: 3 Rust, 2 TS, changelog.
- 🔁 Next prompts: build on the VM; clean up the 30 live sources via per-document delete; restore
  `/opt/noviq/ort-gpu/` from the surviving uv cache and repoint `gpu-reindex.sh`.
- 🧠 Reflection: the diagnosis came from arithmetic, not from reading code — "30 = 12 + 18" pointed
  straight at the surviving rows, and only then did the `if let` / unconditional-delete asymmetry
  become obvious. Worth remembering that a user-reported number is often the fastest route into a
  silent bug.

## Evaluation notes (flywheel)

- Failure modes observed: I initially read the access-time evidence as "the stale documents are not
  in the index", when it only showed they were not *re-read* during that run — their chunks were
  already there from 30 June. Narrower claim than the evidence supported; corrected once the count
  came back 30.
- Graders run and results (PASS/FAIL): tsc PASS · cargo NOT RUN · DB assertions NOT RUN
- Prompt variant (if applicable): n/a
- Next experiment (smallest change to try): after deploying, delete a test folder that was created by
  upload (not by path registration) and confirm the returned counts are non-zero — that is the exact
  case the old code got wrong, and the counts now make it observable.
