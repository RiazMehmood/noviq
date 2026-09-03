# Reranker Fine-Tuning Dataset — Design

**Date:** 2026-05-17
**Author:** Riaz (architect) + Claude (agent)
**Status:** Approved — ready for implementation plan
**Branch:** `feature/tier3-hierarchical-retrieval`
**Target model:** `BAAI/bge-reranker-base` (cross-encoder, ~278M params)
**Training target environment:** Google Colab, T4 GPU (16 GB VRAM)

---

## 1. Purpose

Build a small, clean, **legal/govt-domain** dataset of `(query, positive_chunk, hard_negative_chunks)` triples to fine-tune `bge-reranker-base`. Goal: raise reranker quality on EchoAI's Tier 3 hierarchical retrieval path, especially on the failing test cases (tests 5–8 = 0/15) where the current base reranker misorders top candidates.

Dataset construction only. Training script changes are **out of scope** for this spec.

## 2. Inputs

| # | Source | Path | Role |
|---|---|---|---|
| 1 | Sindh govt source docs (6 files) | `~/Desktop/Sindh Govt Documents/` | Indexed to provide ground-truth corpus |
| 2 | Old Qwen2.5-0.5B training set | `~/Desktop/fine tunning research & work/train(3).jsonl` | Q&A pairs (audited, repaired in place) |
| 3 | Existing test results | EchoAI repo (locate during impl) | Real-failure queries against `constitution.pdf` |
| 4 | Current vector DB | `~/.local/share/com.echoai.app/echo.db` | Holds `constitution.pdf` (1 source / 655 chunks); becomes hard-negative mining pool after stage A |

**Verified DB state at design time:** 1 source (`constitution.pdf`), 655 chunks. Memory entry claiming 44 sources / 2410 chunks was stale (dated 2026-05-11) and has been corrected.

## 3. Pipeline

### Stage A — Index Sindh govt docs

- Import the 6 docs from `~/Desktop/Sindh Govt Documents/` into `echo.db` via the existing indexer (incremental, non-destructive — `constitution.pdf` stays).
- Trigger NER + chunking via current indexer pipeline (no chunking parameter changes).
- **Expected end state:** 7 sources, ~1500–2500 chunks (depending on doc sizes).

### Stage B — Audit + repair `train(3).jsonl`

For each `(question, answer)` entry:

1. Retrieve top-k candidate chunks for `question` using dense + BM25 over the indexed corpus (stage A output).
2. Verify whether `answer` is **grounded** in the retrieved candidates' source text (substring / semantic-overlap check, manual-LLM-style verification).
3. Classify:
   - **Grounded** → keep verbatim, mark `repaired: false`.
   - **Wrong / ungrounded but answerable from corpus** → **repair** by rewriting the answer to match the source text. Mark `repaired: true`.
   - **Unanswerable from the indexed corpus** → drop. Log to audit report.

**Outputs of stage B:**

- `corrected_train.jsonl` — clean repaired set (one JSON object per line, fields: `question`, `answer`, `repaired`, `source_chunk_id`, `source_doc`).
- `audit_report_train.md` — human-readable report listing every modified or dropped entry: original Q&A, repaired Q&A (if repaired), reason, source chunk reference, and a short justification.

### Stage C — Build positives

- For each entry in `corrected_train.jsonl`: map `question → source_chunk_id` to form `(query, positive_chunk)`.
- For each entry in the existing test results: same mapping for queries where ground-truth chunk is known.
- Discard duplicates by `(normalized_query, positive_chunk_id)`.

### Stage D — Mine hard negatives

For each `(query, positive_chunk)`:

1. Retrieve top-15 candidates via dense embedding (current embedder) + BM25 from `echo.db`, *excluding* the positive chunk.
2. **Score-band filter** to avoid false negatives and uninformative easy negatives:
   - Drop candidates with similarity > 0.95 of the positive (near-duplicates → likely false negatives).
   - Drop candidates with similarity < 0.30 (trivially unrelated → no training signal).
3. **Sample 7 hard negatives per positive** (BGE FlagEmbedding standard). If fewer than 7 candidates survive filtering, accept what's available with a minimum floor of 3; otherwise drop the query.

### Stage E — Emit dataset

**File format** (BGE FlagEmbedding standard JSONL, one example per line):

```json
{"query": "...", "pos": ["positive chunk text"], "neg": ["neg1 text", "neg2 text", "...", "neg7 text"]}
```

**Split:** 90% train / 10% eval. Random seed = `20260517` (pinned for reproducibility).

**Output directory:** `data/reranker/v1/`

```
data/reranker/v1/
├── train.jsonl
├── eval.jsonl
├── corrected_train.jsonl
├── audit_report_train.md
└── dataset_card.md
```

`dataset_card.md` contents:

- Source files and their hashes
- Per-stage counts (input Q&A entries, kept/repaired/dropped, positives built, triples emitted)
- Mining settings (k, score-band thresholds, negatives per positive)
- Reproducibility seed
- Known limitations (single domain, single language if applicable, size constraints)

## 4. Expected dataset size

| Source | Positives (est.) |
|---|---|
| `train(3).jsonl` after audit/repair | 50–150 |
| Test-result queries (constitution.pdf) | 15–30 |
| **Total positives** | ~70–180 |
| **Total triples (× 7 negatives)** | ~490–1260 |

Sufficient for a small `bge-reranker-base` fine-tune (typically converges in 2–4 epochs on this scale).

## 5. Training target: Google Colab T4 GPU

The dataset is built to fit comfortably within a T4 (16 GB VRAM) Colab session. Implications for dataset shape:

- **Max sequence length:** truncate `query + chunk` to **512 tokens** (BGE reranker default). Longer chunks must be split or truncated; record any truncation in the dataset card.
- **Per-example payload size cap:** keep each `pos`/`neg` chunk text under ~2 KB so that batch size 8 with 7 negatives + 1 positive (= 8 pairs/example × 8 examples = 64 forward passes) fits in T4 memory at fp16.
- **Total dataset file size budget:** target < 50 MB JSONL total so the dataset uploads to Colab Drive in seconds, not minutes.
- **No streaming required:** in-memory load on T4 must remain feasible — at ~1k triples × ~2 KB × 8 chunks = ~16 MB, this is fine.

These are dataset-construction constraints; the actual training script (LR, batch size, gradient accumulation, fp16/bf16) is a separate concern handled when training is configured on Colab.

## 6. Scope guards (explicit non-goals)

- **No synthetic LLM-generated queries.** Keeps audit surface small. Deferrable to v2 dataset.
- **No re-chunking of `constitution.pdf`.** Use chunks as currently stored in `echo.db`.
- **No changes to the reranker training script or Rust reranker integration.** Dataset-only.
- **No multi-domain mix.** Stays in legal/govt domain. A v2 dataset can broaden later if needed.
- **No changes to indexer chunking parameters.** Stage A uses current defaults.

## 7. Risks and mitigations

| Risk | Mitigation |
|---|---|
| Audit/repair introduces hallucinated "corrections" | Every repair is logged with source chunk reference in `audit_report_train.md`; user reviews report before training |
| Dataset too small after audit drops many entries | Floor: if total positives < 50, halt and surface to user before mining — do not ship a too-small dataset silently |
| Hard-negative score-band thresholds wrong for this corpus | Threshold values (0.30 / 0.95) recorded in dataset card; first run logs the distribution so we can re-tune in v2 |
| Test results reference chunks that no longer exist after re-indexing | Stage C handles this — chunk-ID lookup will fail for stale references, those queries get dropped with a logged warning |
| T4 OOM during training due to long chunks | Stage E truncates query+chunk to 512 tokens with a logged count of truncations |

## 8. Acceptance checks

- [ ] `data/reranker/v1/train.jsonl` and `eval.jsonl` exist and parse as valid JSONL
- [ ] Every example has exactly 1 `pos` entry and between 3 and 7 `neg` entries
- [ ] `corrected_train.jsonl` exists; every entry has a `source_chunk_id` and `source_doc` field
- [ ] `audit_report_train.md` lists every repaired/dropped entry with reason and source reference
- [ ] `dataset_card.md` reports counts, mining settings, seed, and truncation stats
- [ ] Train/eval split honors seed `20260517` and is reproducible
- [ ] Total dataset size < 50 MB
- [ ] No query appears in both train and eval splits (leakage check)

## 9. Follow-ups (out of scope, but tracked)

- v2 dataset: broaden beyond legal/govt domain once base reranker quality is verified
- Add synthetic-query generation pipeline (LLM-generated queries from corpus chunks) as a separate audited stage
- Training script + Colab notebook (separate spec)
- Eval methodology beyond loss: nDCG@10 / MRR against held-out test queries (separate spec)
