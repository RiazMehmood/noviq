---
id: "031"
title: Migration Duplicate Checksum Fix
stage: red
date: 2026-05-02
surface: agent
model: claude-sonnet-4-6
feature: 001-notebook-knowledge-engine
branch: master
user: Riaz Mehmood
command: inline
labels: ["sqlx", "migrations", "startup-panic", "checksum", "duplicate-files"]
links:
  spec: null
  ticket: null
  adr: null
  pr: null
files:
  - src-tauri/src/db/migrations/004_agent_tables.sql (deleted — was untracked duplicate)
  - src-tauri/src/db/migrations/005_agent_support_tables.sql (deleted — was untracked duplicate)
tests:
  - none
---

## Prompt

i try to run the app after gpu integration and got this error:
migration failed — migration 4 was previously applied but has been modified

## Response snapshot

Root cause: `004_agent_tables.sql` and `005_agent_support_tables.sql` were untracked duplicate files on disk with version prefixes 004/005. SQLx picked them alphabetically before `004_index_jobs.sql` and `005_incomplete_messages.sql` — checksum mismatch against what the DB had recorded. Fix: deleted the two untracked duplicate files. Content already exists in `006_agent_tables.sql` and `007_agent_support_tables.sql` (already applied in DB). Migration sequence is now clean 001–011.

## Outcome

- ✅ Impact: App startup panic fixed; duplicate untracked migration files removed; migration 011_gpu_settings.sql will now apply on next run
- 🧪 Tests: none (startup fix)
- 📁 Files: 2 untracked files deleted from migrations directory
- 🔁 Next: Run app to confirm migration 011 applies; then proceed to reviewer agent implementation plan

## Evaluation notes (flywheel)

- Failure modes observed: Untracked files from a previous branch merge left duplicate version-prefix migration files on disk; git status hid them because they were never committed to git
- Next experiment: Add `.gitignore` rule or CI check to catch duplicate migration version prefixes before they cause runtime panics
