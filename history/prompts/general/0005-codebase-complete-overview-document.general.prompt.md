---
id: "0005"
title: Codebase Complete Overview Document
stage: general
date: 2026-05-05
surface: agent
model: claude-sonnet-4-6
feature: none
branch: master
user: Riaz Mehmood
command: "write a md file to understand the complete code"
labels: ["documentation","reverse-engineering","codebase-overview"]
links:
  spec: null
  ticket: null
  adr: null
  pr: null
files:
  - docs/CODEBASE_OVERVIEW.md
tests:
  - null
---

## Prompt

i want to understand the complete code, so write a md file to get all these things easily

## Response snapshot

Created `docs/CODEBASE_OVERVIEW.md` — a comprehensive 700+ line document covering:
- What EchoAI is (fully-offline desktop AI assistant)
- Technology stack (Tauri v2, Rust 1.87, SolidJS, SQLite + sqlite-vec + FTS5)
- Full repository layout with file roles
- Architecture diagram (SolidJS → Tauri IPC → Services → SQLite)
- Startup sequence (2-phase: DB-only sync, then slow async background)
- Complete DB schema (14 tables across 12 migrations)
- Indexing pipeline (parse → chunk → embed → vec0 → NER 2-pass)
- 5-tier query router (cache → entity → FTS5 → semantic → LLM)
- Knowledge Graph, Meeting Agent, Batch Processing, Analysis Engine
- Scheduler priority system, hardware detection, cloud LLM (Groq)
- File watcher debounce mechanism
- Frontend architecture (SolidJS stores, component hierarchy)
- Complete Tauri IPC command list (50+ commands) + event list
- Key design decisions & constraints (ggml conflict, NTFS target, immutable embedding dim)
- File-by-file reference for all 50+ backend and frontend files

## Outcome

- ✅ Impact: Single reference document for onboarding, debugging, and planning — eliminates need to read multiple source files to understand system
- 🧪 Tests: n/a (documentation task)
- 📁 Files: docs/CODEBASE_OVERVIEW.md (created)
- 🔁 Next prompts: Use this doc as context for architecture decisions, feature planning, or debugging sessions
- 🧠 Reflection: sp.reverse-engineer skill invoked; deep exploration of lib.rs, Cargo.toml, schema.sql, query_router.rs, and frontend stores provided complete picture
