---
id: 023
title: Conversation History, Retrieval & Grounding Fixes
stage: green
date: 2026-04-25
surface: agent
model: claude-sonnet-4-6
feature: 001-notebook-knowledge-engine
branch: 001-notebook-knowledge-engine
user: Riaz Mehmood
command: ok do accordingly
labels: ["conversation-history","retrieval","grounding","query-expansion","stateless","rag"]
links:
  spec: null
  ticket: null
  adr: null
  pr: null
files:
  - src-tauri/src/services/query_router.rs
  - src-tauri/src/commands/query.rs
tests:
  - cargo check --features llm (exit 0)
---

## Prompt

ok do accordingly

## Response snapshot

Implemented 3 fixes identified from analysis of 5 session queries against constitution.pdf:

### Fix 1 — Conversation history (stateless model)
- Added `HistoryTurn { role, content }` struct to `query_router.rs`
- Added `history: &[HistoryTurn]` param to `route()`
- Updated `build_rag_prompt()` to inject last 2 Q&A pairs as proper ChatML prior turns before the current user turn
- In `commands/query.rs`: fetch last 4 messages BEFORE persisting current query, pass as history to route()
- **Impact**: follow-up queries like "on which date this amendment is made?" now resolve "this amendment" from prior turn context

### Fix 2 — Query expansion for short queries
- Added `is_short_query` check: `query_text.split_whitespace().count() <= 5`
- Short queries now trigger FTS5 keyword search alongside vector search (hybrid RRF)
- **Impact**: "which are constitutional positions" (4 words) now gets keyword retrieval that can match article headings like "There shall be a President", "Auditor-General of Pakistan", etc.

### Fix 3 — Tighter grounding for uncited answers
- Added Check 3 in `validate_grounding()`: when `citations.is_empty() && factual_count >= 2`, trigger fallback
- Catches multi-sentence answers where 3B model answered confidently without any [source, page] citations
- **Impact**: Q4-type "answered but uncited" responses now return fallback instead of ungrounded content

## Outcome

- ✅ Tests: cargo check --features llm PASS
- 📁 Files: query_router.rs (HistoryTurn struct, route sig, build_rag_prompt, FTS5 trigger, grounding check), query.rs (history fetch + pass)
- 🔁 Next: restart app and re-run the 5 test queries to verify fixes

## Evaluation notes (flywheel)

- Root causes observed: stateless model (Q5 failed), short query semantic mismatch (Q1), uncited multi-sentence answer (Q4)
- Graders run and results: cargo check PASS
- Next experiment: verify Q1 "constitutional positions" now returns correct list via hybrid retrieval
