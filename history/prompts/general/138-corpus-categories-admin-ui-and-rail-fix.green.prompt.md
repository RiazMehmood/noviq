---
id: 138
title: Corpus categories admin UI and chat-rail ordering fix
stage: green
date: 2026-08-02
surface: agent
model: claude-opus-5
feature: general
branch: release-deploy
user: Riaz Mehmood
command: (conversational)
labels: [corpus-organisation, migration-021, admin-ui, retrieval-scope, chat-rail]
links:
  spec: n/a
  ticket: n/a
  adr: n/a
  pr: n/a
files:
  - src-tauri/src/db/repositories/sources.rs
  - src-tauri/src/services/indexing.rs
  - src/stores/categories.ts
  - src/components/CorpusCategories.tsx
  - src/components/IndexedFolders.tsx
  - src/components/DocumentTree.tsx
  - src/api/index.ts
  - src/pages/Admin.tsx
  - src/pages/ChatPage.tsx
  - CHANGELOG_DEV.md
tests:
  - "tsc --noEmit: clean"
  - "vite build: clean"
  - "cargo check --no-default-features --features server --bin noviq-server: clean on VM (2nd run; 1st caught a real miss)"
---

## Prompt

build the categories admin UI and rail fix also.

(Preceded by: "recall where we were? last session was closed accidently, so check
for working in pipeline also on local and pending work also")

## Response snapshot

Session resumed from `CHANGELOG_DEV.md` + git state. Found the previous session
had left the **migration 021 corpus-organisation backend complete but
uncommitted** (migration, `corpus_category` repository, six admin routes, the
`visible_source_ids` filter) with **no frontend at all**, plus a separate
uncommitted `ChatPage` rail-ordering fix.

Completed the feature:

1. **Backend gap closed.** `SourceDto` and `repositories::sources::Source` had no
   `category_id`/`disabled_at`, so the admin list could not group or grey
   anything. Added to both plus the three `SourceDto` construction sites. All six
   `query_as::<_, Source>` sites use `SELECT *`, so no query changed.
2. **`stores/categories.ts`** — one shared signal + loader, so the manager and the
   document list cannot disagree after a rename. Every mutation re-reads the
   server list rather than patching locally, because `document_count` is computed
   server-side and a delete releases documents.
3. **`components/CorpusCategories.tsx`** — create / rename in place / reorder by
   swapping `sort_order` / delete, with the confirm stating explicitly that
   documents survive and quoting the server's count.
4. **`IndexedFolders`** — grouped by category with Uncategorised FIRST (preserves
   the "the file you just uploaded is easiest to find" rule), per-group select-all,
   a "File N into…" bulk menu, and a per-row Withdraw/Restore button.
5. **`DocumentTree`** — the defect that mattered. The officer scope picker would
   have offered withdrawn documents as selectable, then refused the answer because
   the server excludes them. Withdrawn documents are now out of the default
   selection, folder select-all and the @-mention catalog, and rendered struck
   through + "Unavailable".
6. **Rail fix** reviewed and kept as written, changelogged separately.

## Outcome

- ✅ Impact: the corpus can be grouped and individual documents withdrawn from
  service, admin-controlled, label-only, no re-index — and the officer-facing
  scope picker agrees with what retrieval will actually honour.
- 🧪 Tests: `tsc --noEmit` clean; `vite build` clean; `cargo check
  --no-default-features --features server --bin noviq-server` clean on the VM in
  a throwaway worktree (prod repo, live binary and running server all untouched).
- 📁 Files: 10 (2 Rust, 7 TS/TSX, 1 changelog).
- 🔁 Next prompts: `cargo check --no-default-features --features server` on the VM
  before any deploy; then decide whether this ships alone (like 020) or waits with
  the twelve held commits.
- 🧠 Reflection: the backend was written to be enforced in exactly one place
  (`visible_source_ids`), which is why withdrawal reaches chat, global search and
  citation opening at once. The frontend then had to be audited for the SECOND
  place that measures the same thing — the scope picker — which is the recurring
  bug shape in this codebase.

## Evaluation notes (flywheel)

- Failure modes observed: **the grep that audited the struct change was too
  narrow**. Searching `source_repo::Source {` and `sources::Source {` returned
  nothing and I read that as "no literal construction sites", but the one site
  that exists is inside the type's own module and writes the bare `Source {` —
  precisely where the qualified name is never used. The compiler caught it
  (E0063); the audit did not. Rule: when adding a field to a struct, grep the
  bare name too.
- Near-miss avoided: a scope picker whose offered set differed from the
  retrievable set (`DocumentTree`).
- Graders run and results (PASS/FAIL): tsc PASS, vite build PASS, cargo check
  FAIL then PASS after the fix.
- Prompt variant (if applicable): n/a
- Next experiment (smallest change to try): exercise withdraw → ask a question
  scoped to the withdrawn document → confirm the picker never offered it. Then
  decide the deploy path, since 021 cannot be cherry-picked onto `release` alone.
