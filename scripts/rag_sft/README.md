# RAG-SFT Distillation Pipeline (Feature 019)

Scripts to build an SFT dataset that distills the **production 8B's grounded,
citation-disciplined behavior** into a small model (Qwen3-4B). Trains on
`(retrieved context + question) → cited answer` — **not** memorization, which is why
the first 3B fine-tune hallucinated (see `memory/project_qwen3b-finetune-eval`).

Full design: `specs/019-rag-sft-distillation/spec.md`.

## Requirements
- Python 3.8+ (stdlib only — `generate_dataset.py` and `validate_dataset.py` use
  `urllib`; no pip installs).
- Network access to a running noviq-server with the **8B/vLLM engine active** (the
  teacher). Run near the VM and use the loopback URL, or tunnel `:8080`.
- Admin/user credentials for `/api/login`.

## Flow

```
make_questions.py  ->  questions.jsonl
generate_dataset.py (drives /api/query, teacher=prod 8B)  ->  raw.jsonl
validate_dataset.py (citation-validity + refusal + format gates)  ->  clean.jsonl (+split)
                                                                       train/val/test.jsonl
```

### 1. Questions
Export CSVs from prod PG (port drifts — `ss -tlnp | grep postgres`), then:
```bash
python make_questions.py --out questions.jsonl \
    --query-log-csv query_log.csv \
    --provisions-csv provisions.csv \
    --adversarial 300
```
(psql export commands are documented at the top of `make_questions.py`.)

### 2. Generate (teacher = live 8B)
```bash
python generate_dataset.py \
    --base-url http://127.0.0.1:8080 \
    --username admin --password '<pw>' \
    --questions questions.jsonl \
    --out raw.jsonl --sleep 0.3
```
Each record is built from the `/api/query` response `citations[]` (verbatim passages)
+ the answer. Fresh conversation per question (no carry-over).

### 3. Validate + split
```bash
python validate_dataset.py --in raw.jsonl \
    --clean clean.jsonl --rejects rejects.jsonl --report report.json \
    --split --out-dir data/
```
Rejects: invalid `[N]` citations, marker-dumps, mis-refusals, leaked legal refs, dupes.
Inspect `report.json` for the reject-reason histogram and the clean kind-mix; aim for
~65/15/10/10 answer/refusal/multihop/verbatim and a 15–30% reject rate.

## Then (external)
Train Qwen3-4B (LoRA/QLoRA, 2–3 epochs) on `train.jsonl` → merge → **AWQ** → serve on
vLLM via `NOVIQ_VLLM_MODEL` (keep the 8B as fallback). Evaluate per spec §8 — ship only
if citation accuracy ≥ 8B and refusal recall is high.

## Notes / caveats
- **Verify `SYSTEM_PROMPT` in `rag_sft_lib.py`** matches the current prod prompt before a
  real run — the fine-tune must be trained on the exact system message it will be served
  with. (Captured 2026-07-03 from a live trace; the pipeline may have moved on.)
- v1 context = only the *cited* passages (tight, clean grounding). For distractor
  robustness (v2), rebuild context from full-retrieval traces — see spec §7.
- The teacher (8B) can be wrong; the citation-validity gate + a future gold-teacher slice
  (Claude) mitigate this.
