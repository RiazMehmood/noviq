---
id: 029
title: Offline 7-Day Trial Lock
stage: green
date: 2026-04-28
surface: agent
model: claude-sonnet-4-6
feature: 001-notebook-knowledge-engine
branch: master
user: Riaz Mehmood
command: offline for now
labels: ["trial", "license", "sqlite", "offline"]
links:
  spec: null
  ticket: null
  adr: null
  pr: null
files:
  - src-tauri/src/db/migrations/010_trial_meta.sql
  - src-tauri/src/commands/trial.rs
  - src-tauri/src/commands/mod.rs
  - src-tauri/src/lib.rs
  - src/api/index.ts
  - src/App.tsx
tests:
  - cargo check (pending)
---

## Prompt

offline for now

## Response snapshot

Implemented offline 7-day trial lock:
- Migration 010: `trial_meta` table (single-row, id=1, install_date TEXT)
- `trial.rs`: `get_trial_status` command — inserts install_date on first launch, returns `{expired, days_remaining}`
- `App.tsx`: calls `getTrialStatus()` on mount; shows `TrialExpiredScreen` full-screen overlay when expired
- Lock screen: dark overlay with lock icon, "Trial Expired" heading, contact admin message

## Outcome

- ✅ Impact: App auto-locks after 7 days from first launch; no server required
- 🧪 Tests: cargo check running
- 📁 Files: 6 files changed
- 🔁 Next prompts: commit + push to trigger Windows CI rebuild
- 🧠 Reflection: Single-row sentinel (id=1 CHECK) prevents duplicate install_date rows; chrono already in Cargo.toml
