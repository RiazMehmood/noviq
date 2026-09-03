---
id: 139
title: Chat sidebar groups by category, not folder
stage: green
date: 2026-08-03
surface: agent
model: claude-opus-5
feature: general
branch: release-deploy
user: Riaz Mehmood
command: (conversational)
labels: [corpus-organisation, migration-021, document-picker, f021-ownership, follow-up]
links:
  spec: n/a
  ticket: n/a
  adr: n/a
  pr: n/a
files:
  - src-tauri/src/db/repositories/corpus_category.rs
  - src-tauri/src/api/documents.rs
  - src-tauri/src/api/router.rs
  - src/stores/categories.ts
  - src/components/DocumentTree.tsx
  - CHANGELOG_DEV.md
tests:
  - "tsc --noEmit: clean"
  - "vite build: clean"
  - "cargo check --no-default-features --features server --bin noviq-server: clean on VM"
---

## Prompt

i grouped the documents it is showing in admin panel but not reflected to chat
tab where on right side it is showing all documents under admin folder only

## Response snapshot

Confirmed the report against the live database: the grouping was real
(Procurement 6, Uncategorised 17) but `DocumentTree` groups by **folder**, and
the four folders are `uploads/admin` (whole shared corpus) plus one private
folder per user — so everything appeared under one "admin" node. A gap in the
021 delivery: categories were wired into the admin list only.

Surfaced the design fork to the architect rather than assuming, because folders
were carrying F021 ownership that categories do not. Architect chose **categories
at top level** with personal uploads in their own group.

Implemented:
1. `GET /api/categories` for any signed-in user (`corpus_category::list_names`),
   because `/admin/categories` would 403 an officer. Counts deliberately omitted
   — computed over all of `sources`, so they would disclose the size of a corpus
   the viewer cannot see.
2. `stores/categories.ts` picks the endpoint by role — still ONE list app-wide.
3. `DocumentTree` regrouped: categories in display order → Uncategorised → "My
   documents". Ownership moved from the folder row to a per-document
   `Private` / `Shared · user` badge so F021 information is not lost.
4. Collapse state inverted (`collapsed`, default empty) so groups appearing after
   mount are open rather than silently collapsed.

## Outcome

- ✅ Impact: the officer's document picker now reflects the admin's corpus
  organisation, and personal uploads stay visibly distinct from the shared corpus.
- 🧪 Tests: tsc clean, vite build clean, cargo check clean on the VM.
- 📁 Files: 6 (3 Rust, 2 TS/TSX, 1 changelog).
- 🔁 Next prompts: deploy (same merge + tree-assertion recipe); then confirm in
  the browser as both an admin and a plain officer.
- 🧠 Reflection: the original 021 work treated "corpus organisation" as an admin
  feature and stopped at the admin console. The organisation exists FOR the
  officer — the surface that consumes it was the one I left out.

## Evaluation notes (flywheel)

- Failure modes observed: **scoped a feature to the surface that configures it
  rather than the surface that consumes it.** The admin groups documents so that
  readers can navigate them; shipping the manager without the reader's view left
  the feature inert from the user's point of view.
- Second-order catch: dropping the folder row would have silently destroyed the
  F021 `Private`/`Shared` distinction. Asked instead of assuming.
- Graders run and results (PASS/FAIL): tsc PASS, vite build PASS, cargo check PASS.
- Prompt variant (if applicable): n/a
- Next experiment (smallest change to try): sign in as a non-admin officer and
  confirm `/api/categories` returns names and the sidebar groups correctly.
