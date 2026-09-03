---
id: 042
title: Reranker model selectable via ECHO_RERANKER_MODEL env var + bge-reranker-large download
stage: refactor
date: 2026-05-16
surface: agent
model: claude-opus-4-7
feature: 001-notebook-knowledge-engine
branch: feature/tier3-hierarchical-retrieval
user: engr.riaz2010@gmail.com
command: implement Recommendation #1 — swap to bge-reranker-large zero-shot
labels: [retrieval, reranker, tier3, model-swap]
links:
  spec: specs/001-notebook-knowledge-engine/spec.md
  ticket: n/a
  adr: n/a
  pr: n/a
files:
  - path: src-tauri/src/lib.rs
    change: lines 570-584 — reranker model directory now driven by ECHO_RERANKER_MODEL env var (default bge-reranker-base preserves prior behaviour); resolve_model_path applied to both onnx + tokenizer
  - path: models/bge-reranker-large/
    change: new 2.26 GB ONNX bundle — model.onnx (618 KB graph) + model.onnx_data (2.24 GB external weights) + tokenizer.json (17 MB)
  - path: CHANGELOG_DEV.md
    change: appended 2026-05-16 (night) entry documenting the env knob, download saga (curl truncation, wget loop), launch instructions, and acceptance test plan
  - path: /home/riaz/Desktop/fine tunning research & work/bge_reranker_fine_tunning.md
    change: research doc for the fine-tuning fallback path (Recommendation #2) — 12 sections, 8 Colab cells, data-mining strategy from existing test logs
tests:
  - status: cargo check -p echo-ai clean (9.39s) after env-var refactor
  - note: empirical test8 acceptance run pending — user will execute with ECHO_RERANKER_MODEL=bge-reranker-large and capture test8_logs.md
---

## Prompt

> write these recommendations also inside our progress file and now go for 1
> [...followed by: yes do step 1 and guide me]

## Response snapshot

Implemented Recommendation #1 (zero-shot swap to bge-reranker-large) via a
single-line env knob in `lib.rs` so no future code change is needed to swap
models again. Default (`bge-reranker-base`) is preserved when the var is
unset.

Downloaded `bge-reranker-large` (2.26 GB) into `models/bge-reranker-large/`.
The download was painful: ~5 h wall-clock on slow link (80–150 KB/s).
Curl's `-C -` truncated the file from 561 MB → 86 MB after an HF signed-URL
expiry returned 200-instead-of-206 on resume. Recovered by wrapping wget in
a bash `while true; do wget -c URL && break; sleep 10; done` loop that
re-fetches a fresh redirect on every retry. ~200 wget restarts later,
2,239,565,824 bytes matched expected size exactly.

Flattened `onnx/model.onnx{,_data}` → root to match the base layout
(`models/<name>/model.onnx` is what `lib.rs` resolves).

Updated `CHANGELOG_DEV.md` with full context (symptom → why → fix → files →
launch flags → acceptance criteria → caveats).

Wrote separate research doc on Desktop (`bge_reranker_fine_tunning.md`)
covering Recommendation #2 — 12 sections including 4 data sources, full
8-cell Colab notebook for LoRA fine-tuning, ONNX export, deploy back into
the same env knob. Provides clean fallback if test8 lift is insufficient.

## Outcome

- ✅ Impact: reranker is now a config knob, not a code constant. `ECHO_RERANKER_MODEL=bge-reranker-large` swaps to 560M model; `=bge-reranker-base` reverts. Future models drop into `models/<name>/` and Just Work.
- 🧪 Tests: cargo check -p echo-ai clean (9.39s); empirical 15-prompt sweep deferred to user (test8 run).
- 📁 Files: src-tauri/src/lib.rs, CHANGELOG_DEV.md, models/bge-reranker-large/, /home/riaz/Desktop/fine tunning research & work/bge_reranker_fine_tunning.md
- 🔁 Next prompts: launch `ECHO_RERANKER_MODEL=bge-reranker-large ECHO_MIN_RERANK_SCORE=0.0 ./run-dev.sh`, run 15 v2 prompts → test8_logs.md, parse + add sheet `12_Test8_Findings` to echoai_tier3_test_report.xlsx
- 🧠 Reflection: making the model a knob (not a swap) keeps iteration cheap — try large zero-shot first, fall back to fine-tune only if needed. Same env contract works for both.

## Evaluation notes (flywheel)

- Failure modes observed (carried over from test7): F1/F2/F4/F5 — base reranker confidently scores topically-similar but factually-wrong constitutional articles above the correct one
- Graders run and results (PASS/FAIL): n/a — test8 acceptance run pending
- Prompt variant (if applicable): v2 prompts (15 prompts × 3 phrasing variants — Tech/Prof/Plain), same set used for test5/6/7
- Next experiment (smallest change to try): single env-var change — `ECHO_RERANKER_MODEL=bge-reranker-large` with `ECHO_MIN_RERANK_SCORE=0.0` retained. Target: ≥ 50% of test7-failed prompts score gold chunk #1. If lift <20%, switch to fine-tuning path (research doc).
