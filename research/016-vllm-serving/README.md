# Research: F016 vLLM Concurrent Inference Serving

Self-contained research record + raw evidence for the vLLM-vs-llama.cpp inference
cutover study on the IBA A40-12Q time-sliced vGPU. Everything needed to compile a
write-up (or re-run the experiments) lives in this folder.

## Layout

- **`FINDINGS.md`** — the research report (abstract, environment, methodology,
  experiments, measured data tables, root-cause analysis, open questions). Start here.
- **`data/`** — raw evidence captured from the live VM:
  | file | what it is |
  |------|-----------|
  | `out_llamacpp.jsonl` | A/B arm — llama.cpp GGUF, 30 questions, full answers + verdicts + latency |
  | `out_awq.jsonl` | A/B arm — vLLM Qwen3-8B-AWQ, same 30 questions |
  | `ab_compare.txt` | side-by-side per-question verdict + aggregate (re-scored) |
  | `baseline.log` | llama.cpp 10-concurrent load test (the serialized baseline) |
  | `vllm_engine.log` | vLLM engine-level concurrency (direct `/v1/completions`, 10 & 20) |
  | `loadtest_vllm.log` | vLLM full-stack 10-concurrent `/api/query` |
  | `diag_conc.log` | correlated 10-concurrent run + vLLM `num_running/waiting` timeline |
  | `eval_set_sindh.json` | the 30 complex/ambiguous questions (from the indexed PDFs) |
- **`scripts/`** — the harnesses used, snapshotted for reproducibility
  (`run_ab.py`, `load_test.sh`). Canonical copies live in `tools/vllm_eval/`.

## Reproduce

1. Stand up the sidecar: `deploy/vllm/setup-vllm.sh` then `run-vllm.sh` (see
   `deploy/vllm/`). Set `NOVIQ_INFER_ENGINE=vllm` + `NOVIQ_VLLM_*` in `.env`.
2. A/B: `python3 scripts/run_ab.py run --engine <llamacpp|awq> --questions
   data/eval_set_sindh.json --pass <admin> --out out_<engine>.jsonl`, then
   `run_ab.py compare out_llamacpp.jsonl out_awq.jsonl`.
3. Concurrency: `load_test.sh 10` (full stack) and the engine-level / `diag` scripts.

## One-line status

Quality (no-regression) gate **passed**; full-stack 10-concurrent latency gate
(p95 ≤ 10 s) **not yet met** on this time-sliced vGPU — see `FINDINGS.md` §7–§8.
