# Quickstart: My Projects (F027)

How to build, run and — most importantly — **disprove** this feature. The isolation
guarantee is the deliverable; everything else is plumbing around it.

---

## Build and test

Compile checks and tests run on the VM, not on Windows (there is no Rust toolchain here).
Both flags are required:

```bash
cd /opt/noviq/<worktree>/src-tauri     # cargo manifest lives at src-tauri/, not root
NOVIQ_TEST_PG_INSTALL_DIR=/home/appadmin/test-pg-install \
  cargo test --no-default-features --features server --lib
```

- Missing `--no-default-features --features server` → glib-2.0 build failure.
- Missing `NOVIQ_TEST_PG_INSTALL_DIR` → ~71 bogus DB failures.
- Baseline before this feature: **925 passed / 2 failed**. The two failures
  (`rcb_parliament_articles_with_bracket_openers_all_survive`,
  `sources::scope_tests::conversation_scope_includes_own_attachment`) are pre-existing.
  ⚠️ The second one is in the very module this feature changes — re-read it before
  assuming a scope-test failure is yours.
- `cargo test --lib` leaves a test Postgres in `/tmp/echoai-test-pg-<pid>/` running after
  the run. Kill it and `rm -rf` the dir, or `/tmp` (4.7 G) fills.

Use a detached worktree with its own `CARGO_TARGET_DIR` so the deploy poller stays inert.

---

## Turn it on

The gate defaults to **off**, so migration 023 changes nothing user-visible until:

Admin console → My Projects → Enable. Or directly:

```sql
INSERT INTO db_config (key, value) VALUES ('my_projects_enabled', '1')
ON CONFLICT (key) DO UPDATE SET value = excluded.value;
```

Applies at runtime; no restart (FR-039).

---

## The tests that matter

### 1. Isolation — the negative test (SC-001)

This is the feature. Everything else can be re-done later; this cannot be discovered
late.

```
Given  a project containing exactly one uploaded document
And    the shared corpus contains "Rule 4" of five instruments
When   the user asks "what does Rule 4 say?" WITHOUT ticking the corpus
Then   zero shared-corpus chunks are retrieved
And    zero chunks from any other project — including a copy of this one
And    zero chunks owned by any other user
```

Run it four times, once per route, because four routes reach documents and it is the
forgotten route that fails open:

| Route | Handler |
|---|---|
| chat answering | `api/query.rs` |
| global search | `documents::global_search` |
| citation → passage | `documents::source_chunks` |
| citation → file | `documents::source_file` |

⚠️ **Measure at the stage that writes the answer, not at the retrieval helper.** A
diagnostic taken one level too high has read 0 on this system while production had 105
bad rows. Assert on the citations in the response body.

### 2. Copy independence (SC-005)

```
A shares project P with B
B's copy answers questions immediately          → no reprocessing (SC-003, <5s)
B renames, adds a doc, removes a doc            → A sees no change
A renames, adds a doc, removes a doc            → B sees no change
A deletes a document                            → B's copy STILL answers from it (FR-024)
A deletes the whole project                     → B's copy is untouched
B asks a question in the copy                   → retrieves only from B's copy
```

The fourth line is the one to write first. It is counter-intuitive, it is what FR-024
requires, and it is what the delete paths will get wrong: before migration 023 every
reference to a document belonged to one user, so "delete my document" could only ever
destroy the deleter's own data. Now it can destroy someone else's.

### 3. Share links (SC-006, SC-007)

```
Open the link signed out            → 401, and nothing of the chat in the body
Open it as another signed-in user   → transcript reads, citations open
Try to ask a question in it         → no route accepts one
Global-search as that viewer        → the owner's project documents DO NOT appear
Revoke                              → link dead; copies already taken still work
```

The fourth line is the real test of R5. If viewing a shared chat widened
`visible_source_ids`, the transcript would look right and global search would quietly
leak the entire project.

### 4. The gate (SC-008, SC-009)

```
Turn it off
  → no project entry point in the UI
  → POST /api/list_projects refused          (the UI is not the gate)
  → POST /api/query on a project chat refused
  → an existing share link refused
  → ordinary chat, global search, corpus answers all UNCHANGED  (FR-040)
Turn it back on
  → every project, document, chat and link present and unchanged
```

---

## Manual smoke test

1. Enable the gate. Create a project, upload two PDFs, wait for both to read **ready**.
2. Ask something answerable from one of them → citations point only there.
3. Ask something answerable only from the Constitution → "nothing to search" **without**
   the corpus tick; answered **with** it, and the corpus citations are visibly labelled
   as law rather than as your upload (FR-016).
4. Ask a third question → the tick is back off by itself (FR-017).
5. Share the project with a second account → it appears there, searchable at once.
6. Delete a document from the original → the copy still answers from it.
7. Share a chat → open the link in a second browser signed in as another user → readable,
   citations open, no way to ask. Sign out → refused.
8. Turn the gate off → everything disappears; the share link refuses. Turn it on →
   everything is back.

---

## Traps this codebase has already sprung

- ⚠️ **The stale bundle.** Rule out a stale frontend build before believing any
  "doesn't work" report.
- ⚠️ **`deploy.sh` is out of tree.** The timer runs `/opt/noviq/deploy.sh`; the repo copy
  is reference only, and `release` is maintained by cherry-pick. Fetch the VM's `release`
  and diff against deployed HEAD *before* cherry-picking — this trap has fired three
  times.
- ⚠️ **Refusals return HTTP 200.** "Nothing to search" (FR-012) will too. Assert on the
  body, not the status.
- ⚠️ **Two Postgres listeners is normal** after a compile check. Resolve prod by data dir
  (`/opt/noviq/data/pg/data`), never by port.
- ⚠️ **`regexp_replace(… E'\\s+' …)` over ssh** becomes a literal `s` and eats every
  letter s, looking exactly like an OCR defect. Check suspected text defects against raw
  content with no regex.
- ⚠️ **Migration 023 is a one-way deploy.** `ignore_missing = false` means a pre-023
  binary fails at startup with `VersionMissing` once it has applied. Rolling back the
  code requires deleting the row from `_sqlx_migrations` first.
