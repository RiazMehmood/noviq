---
id: 058
title: Permanent-delete and remove-deep-indexing per-file buttons
stage: green
date: 2026-07-03
surface: agent
model: claude-opus-4-8
feature: general
branch: release
user: Riaz Mehmood
command: (chat) "i want a button in index folder from which we can permanantly delet a file also option remove deep indexing button."
labels: [indexing, ui, api, folders]
links:
  spec: N/A
  ticket: N/A
  adr: N/A
  pr: N/A
files:
  - src-tauri/src/api/documents.rs
  - src-tauri/src/api/router.rs
  - src-tauri/src/db/repositories/sources.rs
  - src-tauri/src/services/indexing.rs
  - src/api/index.ts
  - src/pages/SettingsPage.tsx
  - CHANGELOG_DEV.md
tests:
  - frontend tsc --noEmit → exit 0
  - VM cargo check --release --no-default-features --features llm,server,cuda --bin noviq-server → exit 0
---

## Prompt

"i want a button in index folder from which we can permanantly delet a file also
option remove deep indexing button."

## Response snapshot

Grounded the request in the existing per-source actions (retry / deep-index /
remove) rendered in `SettingsPage.tsx` and served via `POST /api/<cmd>`. Added two
capabilities, smallest-diff, mirroring the existing patterns:

- **Permanent delete** — new `delete_folder_source` handler (txn: wipe
  chunk_embeddings/graph_nodes/graph_edges/chunks + `notebook_sources`, drop the
  `sources` row, then best-effort `std::fs::remove_file`; owner/admin-gated). FE:
  red trash button with `window.confirm`, removes the row + decrements folder count.
- **Remove deep indexing** — generalized `reindex_source(deep: bool)` →
  `reindex_source(tier: Option<&str>)` (`Some("deep")` / `Some("fast")` / `None`),
  new `undeep_index_folder_source` handler (tier=fast + re-index). FE: amber button
  shown only when `index_tier === "deep"`. Exposed `index_tier` through repo
  `Source` (FromRow; all reads `SELECT *`) + `SourceDto` to gate the button.

Routes `/undeep_index_folder_source` + `/delete_folder_source` in `router.rs`.

Verified: frontend `tsc` exit 0; VM `cargo check` exit 0 (2nd pass — 1st caught two
missed `SourceDto` pending-DTO builders in `index_sources`). Committed `e49de6b5`,
pushed to `release` (auto-deploy).

## Outcome

- ✅ Impact: two new per-file admin actions (hard delete + undo deep index) live via web transport.
- 🧪 Tests: tsc exit 0; VM cargo check (llm,server,cuda) exit 0.
- 📁 Files: 4 Rust (documents/router/sources/indexing) + 2 FE (index.ts/SettingsPage) + CHANGELOG.
- 🔁 Next prompts: register desktop Tauri commands for the two cmds if desktop is revived; consider a bulk-delete.
- 🧠 Reflection: adding a field to a shared FromRow struct needs a full-workspace check — grep for direct struct-literal construction, not just the `From` impl (two pending-DTO builders were missed until cargo check).

## Evaluation notes (flywheel)

- Failure modes observed: missed two direct `SourceDto {…}` construction sites (compiler-caught, fixed).
- Graders run and results (PASS/FAIL): frontend tsc PASS; VM cargo check PASS.
- Prompt variant (if applicable): N/A
- Next experiment (smallest change to try): browser-verify both buttons against a throwaway uploaded file post-deploy.
