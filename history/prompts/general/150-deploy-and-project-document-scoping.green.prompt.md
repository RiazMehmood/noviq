---
id: 150
title: Deploy the chat fixes, then scope project documents out of the global chat
stage: green
date: 2026-08-11
surface: agent
model: claude-opus-5
feature: general
branch: 027-my-projects
user: Riaz Mehmood
command: (conversational)
labels: [deploy, f027, migration, scoping, verification]
links:
  spec: n/a
  ticket: n/a
  adr: n/a
  pr: n/a
files:
  - src-tauri/src/db/pg/migrations/024_project_uploads.sql (new)
  - src-tauri/src/api/uploads.rs
  - src-tauri/src/api/documents.rs
  - src-tauri/src/db/repositories/sources.rs
  - CHANGELOG_DEV.md
tests:
  - "cargo check --features server: EXIT=0 (both commits, on the VM)"
  - "cargo test --lib persona:: 7 passed; guidance:: 16 passed"
  - "migration 024 validated in a rolled-back transaction against the prod schema"
  - "post-deploy: 4 scoping queries run against prod — 0 / 1 / 0 / 1 as intended"
---

## Prompt

yes run the vm compile check
[then] and one question why you set it to 4000 characters?
[then] yes 2000 is ok
[then] deploy it
[then] still it is showing my projects documents in chat window also why?

## Response snapshot

**VM compile check** (recipe: bundle → temp ref → detached worktree → separate
target dir → tmux): `cargo check --features server` EXIT=0. Also ran the persona
and guidance test modules deliberately — the persona TOML is parsed at RUNTIME
and a bad fragment falls back silently, so a green compile proves nothing about
the "Rule 10" fix.

**Query cap** revised 4,000 → 2,000 after grounding it in the deployed numbers
rather than my estimate: `NOVIQ_VLLM_MAX_MODEL_LEN=8192` less the 1,024
`gen_reserve_qa` leaves ~7,168 tokens for persona + history + law + question,
so 2,000 chars ≈ 500 tokens ≈ 7%.

**Deploy 1** (`5c23e094`): fast-forward push to `release`, picked up by the
2-minute poller. Verified by ARTIFACTS, not the log — binary mtime and size
moved, bundle hash `CudbUp3J` → `dZ8Nzghg`, service restarted.

**Follow-up defect**: project documents still showed in the chat window.
Different cause from the morning's leak, correctly so — the owner sees their own
documents by `owner_id`, which that fix never touched. Also found a wrinkle in
my own fix: `list_sources_in_folder` matches `file_path LIKE '<folder>%'`, a
PREFIX, so the nested `__project__` directory still matched the parent listing.

**Deploy 2** (`7b483480`): migration 024 adds `sources.project_id` (origin) beside
`conversation_id`, distinct from `project_source` (membership) — so an uploaded
document is hidden from the global tree and general retrieval while a corpus
document ADDED to a project stays visible everywhere (architect's decision).
Settings deliberately still lists them so they stay deletable. One legacy row was
repaired: it was `shared` with `owner_id NULL` — ownerless and world-visible.

## Outcome

- ✅ Impact: two verified deploys; the project-document leak is closed in both
  directions (other users, and the owner's own global chat + retrieval).
- 🧪 Tests: compile + unit checks green on the VM; migration validated in a
  rolled-back transaction before it could run at startup; four post-deploy
  scoping queries confirm 0 / 1 / 0 / 1.
- 📁 Files: 1 migration, 3 Rust files, changelog.
- 🔁 Next prompts: hydrate a turn lost to a page reload; empty "New chat" rows;
  device-check the responsive pass; Build Summaries.
- 🧠 Reflection: I twice tested the wrong thing and caught it only by looking
  closer — grepping the BINARY for a removed prompt string (the whole TOML is
  embedded, so my own warning comment matched), and assuming a per-project
  DIRECTORY would isolate listings when the query matches on prefix. Both would
  have read as green.

## Evaluation notes (flywheel)

- Failure modes observed: verification that confirms the artifact changed rather
  than that the BEHAVIOUR changed; path conventions used where a column is the
  established precedent.
- Graders run and results (PASS/FAIL): n/a
- Prompt variant (if applicable): n/a
- Next experiment (smallest change to try): sample a day of "Next steps" chips
  and count distinct provisions — the real test of the persona rewrite.
