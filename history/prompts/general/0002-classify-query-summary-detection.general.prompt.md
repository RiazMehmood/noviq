---
id: 0002
title: classify_query() — query type classifier
stage: green
date: 2026-05-02
surface: agent
model: claude-haiku-4-5-20251001
feature: none
branch: master
user: Riaz Mehmood
command: Task 1: classify_query() — query type classifier
labels: ["implementation","rust","testing","query-classification"]
links:
  spec: null
  ticket: null
  adr: null
  pr: null
files:
  - src-tauri/src/services/query_router.rs
tests:
  - classify_summary_keywords
  - classify_regular_queries
---

## Prompt

Implement Task 1: classify_query() — query type classifier

Follow these steps exactly:
1. Write failing tests for classify_query()
2. Run test to verify it fails
3. Add QueryType enum and classify_query() function after GROUNDING_FALLBACK constant
4. Run tests to verify they pass
5. Commit the changes

The implementation should:
- Add a QueryType enum with Regular and Summary variants
- Implement classify_query(query: &str) -> QueryType function
- Match broad-overview trigger words to classify as Summary
- Include 9 test cases for summary keywords
- Include 4 test cases for regular queries

## Response snapshot

Implemented Task 1 successfully:

1. Added failing tests to query_router.rs test block with 13 assertions covering summary keywords and regular queries
2. Ran tests which failed as expected (QueryType not defined)
3. Added QueryType enum and classify_query() function after GROUNDING_FALLBACK constant (line 40-66)
4. Ran tests which both passed:
   - test services::query_router::tests::classify_regular_queries ... ok
   - test services::query_router::tests::classify_summary_keywords ... ok
5. Committed with message: "feat(reviewer): classify_query() — detect summary/outline requests"

Commit SHA: 1f88ec9e

## Outcome

- ✅ Impact: Task completed as specified. QueryType enum and classify_query() function added with full test coverage. Both unit tests pass without warnings.
- 🧪 Tests: 2 tests added and passing (classify_summary_keywords, classify_regular_queries); 13 assertions total covering all keyword triggers and edge cases
- 📁 Files: src-tauri/src/services/query_router.rs modified (50 insertions: 27 for implementation, 23 for tests)
- 🔁 Next prompts: Task 2 (Update system prompts in llm.rs) can now proceed
- 🧠 Reflection: Straightforward TDD implementation following exact specification. Function is pure, non-allocating beyond lowercase conversion, and uses substring matching for trigger word detection.

## Evaluation notes (flywheel)

- Failure modes observed: None
- Graders run and results (PASS/FAIL): PASS - cargo test classify_ --no-default-features --features llm (exit code 0)
- Prompt variant (if applicable): null
- Next experiment: Integration with route() in Task 4
