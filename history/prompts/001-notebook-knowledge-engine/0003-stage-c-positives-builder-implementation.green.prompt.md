---
id: "0003"
title: Stage C Positives Builder Implementation
stage: green
date: 2026-05-17
surface: agent
model: claude-sonnet-4-6
feature: 001-notebook-knowledge-engine
branch: feature/tier3-hierarchical-retrieval
user: Riaz Mehmood
command: /sp.phr
labels: ["reranker-dataset", "stage-c", "positives", "tdd", "pipeline"]
links:
  spec: docs/superpowers/specs/2026-05-17-reranker-finetune-dataset-design.md
  ticket: null
  adr: null
  pr: null
files:
  - scripts/reranker_dataset/stage_c_positives.py
  - scripts/reranker_dataset/test_stage_c.py
  - data/reranker/v1/positives.jsonl
tests:
  - scripts/reranker_dataset/test_stage_c.py::test_dedupe_by_query_and_chunk
---

## Prompt

You are the implementer subagent for Task 6 of the reranker fine-tuning dataset plan. Working dir: `/media/riaz/New Volume/echo-AI`. Branch: `feature/tier3-hierarchical-retrieval`.

## Context

Tasks 0–5 complete. `data/reranker/v1/corrected_train.jsonl` has 221 entries (from Stage B). Test results in `history/prompts/` exist but a grep showed they do NOT contain `expected_chunk_id` fields — they're stage prompt records, not labeled test queries. So `load_test_results()` will return 0 rows; the positives will come from `corrected_train.jsonl` alone.

**Plan file:** `docs/superpowers/plans/2026-05-17-reranker-finetune-dataset.md` — read **only the `## Task 6: Stage C — Build positives` section**.

## Your task: Task 6 — Build positives

Implement the 6 steps from the plan's Task 6 section:
1. Write failing test at `scripts/reranker_dataset/test_stage_c.py`
2. Run, see ImportError
3. Implement `scripts/reranker_dataset/stage_c_positives.py` verbatim from plan
4. Run pytest — must PASS
5. Run `python -m scripts.reranker_dataset.stage_c_positives` — should print N positives, write `data/reranker/v1/positives.jsonl`
   - With 221 corrected entries and 0 test-result rows, expect ~221 positives minus dedupe collisions and stale chunk-id drops
   - If N < 50, surface as DONE_WITH_CONCERNS so the controller can decide
6. Commit per plan message; include the new output file (use `git add -f` for `data/reranker/v1/positives.jsonl` since `data/` is gitignored)

## Constraints

- Use venv: `.venv-reranker/bin/python3`, `.venv-reranker/bin/pytest`
- Verbatim from plan unless you discover an actual bug (and if so, report it)
- The `load_test_results()` function will scan `history/prompts/**/*.prompt.md` looking for `query:` and `expected_chunk_id:` lines. None exist; that's expected. Don't "fix" by inventing test-result mappings.

## Self-review before reporting DONE

- Test passes
- `positives.jsonl` has > 50 entries
- Spot-check 2 entries: confirm `positive_text` matches the chunk content in echo.db for the given `positive_chunk_id`
- `git log -1 --stat` shows new files

## Report back

End with one of:
- **DONE** + commit SHA + positive count + breakdown (corrected_train origin vs test origin) + 1 spot-checked entry confirmation
- **DONE_WITH_CONCERNS** + concern (especially if count < 50)
- **NEEDS_CONTEXT** + what
- **BLOCKED** + exact error

## Response snapshot

Implemented Task 6 verbatim from the plan. Wrote `test_stage_c.py` (1 test), confirmed ImportError, implemented `stage_c_positives.py` with `dedupe()`, `load_corrected_train()`, `load_test_results()`, and `main()`. Test passed. Stage produced 221 positives (all from `corrected_train` origin; 0 from test results as expected). Both spot-checked entries (indices 0 and 49) confirmed `positive_text == DB chunk content`. Committed as `d14b246f` with 3 new files.

## Outcome

- ✅ Impact: 221 (query, positive_chunk_id, positive_text) records written to `data/reranker/v1/positives.jsonl`, unblocking Stage D hard-negative mining.
- 🧪 Tests: test_stage_c.py::test_dedupe_by_query_and_chunk — PASS (1/1)
- 📁 Files: stage_c_positives.py (created), test_stage_c.py (created), data/reranker/v1/positives.jsonl (created, 221 lines)
- 🔁 Next prompts: Task 7 — Stage D hard-negative mining (`stage_d_mine.py`)
- 🧠 Reflection: The `str | None` return type annotation in `_kv()` requires Python 3.10+ union syntax; the venv runs Python 3.10.12 so it works, but worth noting for portability if venv is ever rebuilt on 3.9.

## Evaluation notes (flywheel)

- Failure modes observed: None. Plan was verbatim-executable; no bugs discovered. `load_test_results()` returning 0 rows was pre-declared expected behavior, not a defect.
- Graders run and results (PASS/FAIL): pytest test_stage_c.py — PASS; spot-check DB content match — PASS (2/2)
- Prompt variant (if applicable): subagent-task-6-v1
- Next experiment (smallest change to try): Add a second test to `test_stage_c.py` that mocks `load_corrected_train()` and `db.load_chunks()` to verify `main()` drops stale chunk IDs without touching the real DB.
