---
id: 009
title: Fix All Integration Tests Green
stage: green
date: 2026-04-14
surface: agent
model: claude-sonnet-4-6
feature: 001-notebook-knowledge-engine
branch: 001-notebook-knowledge-engine
user: Riaz Mehmood
command: continue
labels: ["integration-tests","fts5","sqlite","indexer","query-router","bugfix"]
links:
  spec: null
  ticket: null
  adr: null
  pr: null
files:
  - tests/integration/test_indexing_pipeline.rs
  - tests/integration/test_query_router.rs
  - src/services/indexer.rs
  - src/services/query_router.rs
tests:
  - test_schema_integrity
  - test_index_pdf_status_transitions
  - test_empty_notebook_returns_no_content
  - test_fts5_keyword_tier1
---

## Prompt

Continue — fix all integration tests and run the full suite green. Real PDFs found on disk. All 4 tests must pass.

## Response snapshot

Fixed 4 bugs across test infrastructure and production code:
1. `setup_db()` used `;` split which broke `BEGIN...END` triggers → replaced with `sqlx::migrate!()`
2. Chunk INSERT in test missing NOT NULL columns (`char_start`, `char_end`, `token_count`)
3. `indexer.rs` `upsert_embedding` hard-errored when sqlite-vec extension not loaded → graceful skip
4. `fts5_query` in `query_router.rs` passed raw query to FTS5 MATCH — dates like `2026-03-15` parsed as `NOT` operators → sanitized by quoting each token

## Outcome

- ✅ Impact: All 4 integration tests passing. Full suite exit code 0.
- 🧪 Tests: 4/4 passed — test_schema_integrity, test_index_pdf_status_transitions (Grade 10 Chemistry.pdf), test_empty_notebook_returns_no_content, test_fts5_keyword_tier1
- 📁 Files: 4 files modified (2 test, 2 production)
- 🔁 Next prompts: Check tasks.md for remaining tasks needed to run the app
- 🧠 Reflection: Root cause of most failures was the `;` SQL splitter in setup_db — a single fix unblocked FTS5 triggers. The FTS5 date sanitization is also a real production bug fix.

## Evaluation notes (flywheel)

- Failure modes observed: SQL splitter breaking triggers; sqlite-vec extension absence; FTS5 operator injection via raw query text
- Graders run and results (PASS/FAIL): cargo test --no-default-features -- --test-threads=1 → PASS (4/4)
- Prompt variant (if applicable): null
- Next experiment: Run E2E validation (T048) — create notebook, add PDF, ask question, verify cited answer
