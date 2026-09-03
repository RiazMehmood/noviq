# Feature 019 — RAG-SFT Distillation Pipeline (Qwen3-4B legal assistant)

**Status:** draft / design sketch (2026-07-03)
**Owner:** architect
**Related:** memory `project_qwen3b-finetune-eval` (why the 3B failed), F016 (vLLM serving)

## 1. Problem & premise

The first fine-tune (Qwen2.5-3B, `noviq_qwen3b`) was trained to **memorize** the
Constitution (`question → recalled answer`). It learned the citation *format*
perfectly but hallucinated *facts* (wrong Art 19, fabricated an SPPRA rule). A small
model at Q4 cannot store the corpus verbatim, so it confabulates in-format — the most
dangerous failure mode for legal work.

**Premise of this feature:** a citation assistant must **retrieve-and-quote, not
memorize**. We fine-tune a small model on `(retrieved passages + question) →
grounded, cited answer` — i.e. we **distill the production 8B's grounded behavior**
into a Qwen3-4B, in the *exact* serving contract (prompt v5.4.0, numeric `[N]`
citations, two-part answer, refusal template).

## 2. Goals / non-goals

**Goals**
- Produce a clean SFT dataset of `(system, user=context+question, assistant=answer)`
  records that faithfully mirror what production sends the model.
- Teacher = the running prod 8B (via `/api/query`), optionally a stronger teacher
  (Claude/GPT-4-class) for a gold slice.
- Explicit refusal / hard-negative coverage.
- Every record passes an automated **citation-validity** gate before training.

**Non-goals**
- No memorization objective. No training on `question → answer` without context.
- No new retrieval logic — reuse the live pipeline exactly.
- Not (yet) the training run itself; this feature is the **data pipeline + eval
  harness**. Training config is documented but run externally (GPU box / Colab).

## 3. Target model & serving

- **Base:** Qwen3-4B-Instruct (same family/recipe as the prod 8B — the realistic size
  floor for multi-chunk synthesis + citation discipline; below 4B repeats the 3B
  failure).
- **Method:** LoRA/QLoRA SFT, 2–3 epochs, low LR (~1e-4 LoRA), pack to ~8k ctx.
- **Quantize:** merge → **AWQ 4-bit** (NOT GGUF — higher served fidelity, stays on the
  vLLM engine, no engine flip).
- **Serve:** vLLM via `NOVIQ_VLLM_MODEL` swap; keep the 8B as fallback (A/B on one eval
  set). See `reference_model-swap-vm`.

## 4. Record schema

```json
{
  "messages": [
    {"role": "system",    "content": "<prompt v5.4.0 system prompt>"},
    {"role": "user",      "content": "Context:\n[1] (<instrument> — <path>) <verbatim>\n[2] …\n\nQuestion: <officer question>"},
    {"role": "assistant", "content": "<grounded answer citing [1][2]… + one-line refine offer>"}
  ],
  "meta": {"kind": "answer|refusal|multihop|verbatim", "citations": [{"n":1,"chunk":"<uuid>","path":"Rule 16(1)(a)"}], "grounding": 32.0, "source_q": "query_log|provision|teacher|adversarial"}
}
```

The `user` context block is rebuilt from the `/api/query` response `citations[]`
(each has `n`, `instrument`, `path`, `verbatim`, `page`) — self-consistent: the answer
cites exactly the passages in the block. (v2: use the *full* retrieved set incl.
un-cited distractors, from the query trace, to teach robustness — see §7.)

## 5. Pipeline stages

| Stage | Script | Output |
|---|---|---|
| 1 Question sourcing | `make_questions.py` | `questions.jsonl` (real query_log + provision-seeded + teacher-gen + adversarial) |
| 2+3 Retrieve + teacher answer | `generate_dataset.py` | `raw.jsonl` (drives `/api/query`, builds records from response citations) |
| 4 Refusal mining | `make_questions.py --adversarial` feeds Stage 2 | out-of-corpus Qs → natural refusals |
| 5 Quality gates | `validate_dataset.py` | `clean.jsonl` + `rejects.jsonl` + `report.json` |
| 6 Assemble/split | `validate_dataset.py --split` | `train/val/test.jsonl` |
| 7 Train→quantize→serve | external | AWQ model on vLLM |

### Composition target (of the clean set)
~65% grounded-answer · ~15% refusal/negative · ~10% multi-hop (2–3 chunks) ·
~10% single-provision verbatim. **Volume ~4–8k** (quality ≫ quantity — 12k
memorization examples proved volume alone doesn't help).

## 6. Quality gates (Stage 5 — reject before training)

1. **Citation validity** — every `[N]` in the answer exists in the context block
   (the check that would have caught the `[3]…[34]` dump).
2. **No leakage** — no article/rule number in the answer that is absent from context.
3. **Grounding** — `grounding` score above threshold (reuse pipeline's value).
4. **Refusal correctness** — negative examples actually refuse; positives actually answer.
5. **Format sanity** — two-part shape present; non-empty; marker-density < 30% (not a
   bracket dump); has the refine offer for `answer` kind.

Expect to discard 15–30%. That is healthy.

## 7. v2 extensions (after v1 baseline ships)
- **Distractor-robust context** from query traces (STEP 10 full retrieved set) so the
  model learns to ignore irrelevant chunks.
- **Stronger teacher** (Claude) for a 500–1k gold slice; keep 8B for bulk.
- **Best-of-n** teacher sampling with the grounding validator as the selector.
- **Wrong-context negatives** (real question + deliberately mis-retrieved chunks → refuse).

## 8. Eval harness (accept/reject gate — reuse F018 trace tooling)
On held-out test + a fixed 30-Q suite (incl. today's SPPRA Rule 16 + Constitution cases):
- **Citation accuracy** (do cited `[N]` support the claim?) — the metric that matters most
- **Refusal precision/recall** (refuses when it should, only when it should)
- **Answer quality vs 8B** (A/B, same retrieval)
- **Latency/throughput** at 10 concurrent

**Ship only if:** citation accuracy ≥ 8B, high refusal recall (no fabrication), quality
within a small delta, meaningfully faster. Else keep the 8B.

## 9. Risks
- Teacher (8B) errors propagate → mitigate with the citation gate + a gold teacher slice.
- v1 tight-context (only cited passages) may under-teach distractor rejection → v2.
- 4B may still miss the quality bar → the whole feature is gated on §8; 8B stays as fallback.
```
