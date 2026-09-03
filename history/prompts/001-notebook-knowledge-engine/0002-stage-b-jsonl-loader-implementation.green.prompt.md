---
id: "0002"
title: Stage B JSONL Loader Implementation
stage: green
date: 2026-05-17
surface: agent
model: claude-sonnet-4-6
feature: 001-notebook-knowledge-engine
branch: feature/tier3-hierarchical-retrieval
user: Riaz Mehmood
command: subagent task instruction
labels: ["reranker-dataset", "stage-b", "jsonl-loader", "or-chain", "config-fix"]
links:
  spec: docs/superpowers/specs/2026-05-17-reranker-finetune-dataset-design.md
  ticket: null
  adr: null
  pr: null
files:
  - scripts/reranker_dataset/stage_b_audit.py
  - scripts/reranker_dataset/test_stage_b.py
  - scripts/reranker_dataset/config.py
tests:
  - scripts/reranker_dataset/test_stage_b.py::test_load_train_jsonl_parses_question_answer_pairs
  - scripts/reranker_dataset/test_stage_b.py::test_load_train_jsonl_skips_malformed_lines
---

## Prompt

You are the implementer subagent for Task 4 of the reranker fine-tuning dataset plan. Working dir: `/media/riaz/New Volume/echo-AI`. Branch: `feature/tier3-hierarchical-retrieval`.

## Context

Tasks 0–3 complete. Repo has `conftest.py` so `from scripts.reranker_dataset...` imports work in pytest. Venv at `.venv-reranker/`.

Training JSONL file is at `~/Desktop/fine tunning research & work/train(3).jsonl` (path configured via `TRAIN_JSONL` in `scripts/reranker_dataset/config.py`).

**Plan file:** `docs/superpowers/plans/2026-05-17-reranker-finetune-dataset.md` — read **only the `## Task 4: Stage B — Load + parse train(3).jsonl` section**.

## Your task: Task 4 — Stage B loader

Implement the 6 steps from the plan's Task 4 section:
1. Write failing tests at `scripts/reranker_dataset/test_stage_b.py`
2. Run, see ImportError
3. Implement loader at `scripts/reranker_dataset/stage_b_audit.py` (verbatim from plan)
4. Run tests — must PASS
5. **Inspect real file structure** with the one-shot command from the plan. Report what the actual top-level keys are. If neither `question`/`instruction`/`prompt` nor `answer`/`output`/`completion` is present, extend the `or`-chain in `load_train_jsonl` to map the actual keys, and re-run the inspection to confirm rows are now parsed.
6. Commit per the plan's message

## Constraints

- Use `.venv-reranker/bin/python3` and `.venv-reranker/bin/pytest`
- Only modify the two files specified in the plan
- If `TRAIN_JSONL` does not exist at the configured path, report BLOCKED (do not try to find a substitute file)
- If the inspection reveals an alternative schema (e.g. nested fields like `data.question`, or pure prompt-completion with no `question` key), make the minimum edit to the `or`-chain to handle it; do NOT rewrite the loader

## Self-review before reporting DONE

- Both tests in `test_stage_b.py` PASS
- The one-shot inspection prints a non-zero row count and a sample row that has `question` and `answer` fields populated
- `git log -1 --stat` shows the 2 expected files

## Report back

End with one of:
- **DONE** + commit SHA + pytest summary + the row count and sample row from the inspection + (if you extended the loader) which keys you added
- **DONE_WITH_CONCERNS** + concern
- **NEEDS_CONTEXT** + what
- **BLOCKED** + exact error

## Response snapshot

Implemented Task 4 (Stage B JSONL loader) end-to-end. Discovered config.py had a filename typo (`train(3).jsonl` vs actual `train (3).jsonl` with a space), fixed it. The real file uses `instruction`/`context`/`response` schema — `response` was not in the original or-chain, so added it as a minimum extension. Both tests pass, 238 rows parsed from the real file. Committed 3 files: `stage_b_audit.py`, `test_stage_b.py`, `config.py`.

## Outcome

- ✅ Impact: Stage B loader implemented and verified against real train data (238 rows); config path bug fixed; loader extended to handle `response` key
- 🧪 Tests: 2 passed — test_load_train_jsonl_parses_question_answer_pairs, test_load_train_jsonl_skips_malformed_lines
- 📁 Files: scripts/reranker_dataset/stage_b_audit.py (created), scripts/reranker_dataset/test_stage_b.py (created), scripts/reranker_dataset/config.py (fixed path)
- 🔁 Next prompts: Task 5 — Stage B audit + repair pipeline (classify_entry, _extract_repair, main runner)
- 🧠 Reflection: Always verify file paths against actual filesystem before trusting config defaults — the space-in-filename discrepancy would have silently returned 0 rows without the inspection step

## Evaluation notes (flywheel)

- Failure modes observed: Config default path had no space before `(3)` while actual filename has a space — would have caused silent 0-row parse without the required inspection step. Also `response` key not in original or-chain template.
- Graders run and results (PASS/FAIL): pytest 2/2 PASS; one-shot inspection: 238 rows, sample row populated with question+answer
- Prompt variant (if applicable): subagent-task-4-stage-b-v1
- Next experiment (smallest change to try): Add `context` field passthrough to loader so audit stage can use grounding context from the original training data
