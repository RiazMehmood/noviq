---
id: "0006"
title: Component File Path Reference
stage: general
date: 2026-05-05
surface: agent
model: claude-sonnet-4-6
feature: none
branch: master
user: Riaz Mehmood
command: "also share the detailed path of those components build in path.md file which component is where?"
labels: ["documentation","file-paths","components","reference"]
links:
  spec: null
  ticket: null
  adr: null
  pr: null
files:
  - docs/path.md
tests:
  - null
---

## Prompt

also share the detailed path of those components build in path.md file which component is where?

## Response snapshot

Created `docs/path.md` — a complete file path reference organized into 29 sections covering every source file in the project:
- Frontend entry points (`src/index.tsx`, `src/App.tsx`)
- 5 pages with their routes
- 6 SolidJS stores with state descriptions
- API layer (`src/api/index.ts`)
- All components grouped by feature: Chat (5 files), Notebook (4), KG (3), Analysis (2), Batch (2), Meeting (1), Search (2), Overlay (4), Shared (3)
- Backend app entry + wiring
- 17 command files with the exact commands each exposes
- Services grouped by concern: Core Pipeline, LLM & Embeddings, Scheduler & Budget, File Watcher, Parsers (5 files), Exporters (3), Meeting Agent (6), Agent Overlay (9)
- Database: schema/init, 12 migrations (each described), 11 repository files
- Integration tests (2 files)
- Config & Tauri configuration files
- Quick cross-reference table: Feature → Frontend files + Backend files

## Outcome

- ✅ Impact: Complete navigational reference — any component can be found instantly by feature or by file path
- 🧪 Tests: n/a (documentation task)
- 📁 Files: docs/path.md (created)
- 🔁 Next prompts: Use alongside CODEBASE_OVERVIEW.md for complete understanding
- 🧠 Reflection: Used `find` to enumerate all .tsx/.ts/.rs files before writing; discovered agent/ subsystem and overlay/ components not covered in overview
