# Reranker A/B Eval Harness — Implementation Plan

**Date:** 2026-05-18
**Spec:** `docs/superpowers/specs/2026-05-18-reranker-ab-eval-harness-design.md`
**Branch:** `feature/tier3-hierarchical-retrieval`
**Stage:** plan
**Non-invasiveness rule:** No production code edits. Anything under
`src-tauri/src/**` stays byte-identical (`git diff --exit-code` must pass).
All new code lives in `src-tauri/tests/`, `scripts/reranker_eval/`,
`data/reranker/eval_runs/` (gitignored).

---

## §1 Architecture overview

```
data/reranker/v1/eval.jsonl  ──┐
                                │
echo.db (existing corpus)  ─────┤
                                ▼
        ┌────────────────────────────────────────────┐
        │  cargo test --test reranker_ab_eval         │
        │  (src-tauri/tests/reranker_ab_eval.rs)      │
        │                                             │
        │  For each of 23 questions:                  │
        │   1. Dense top-50 (embedder + sqlite-vec)   │
        │   2. FTS5 top-50 (sqlite MATCH)             │
        │   3. Hybrid fusion (RRF, k=60) → top-20     │
        │   4a. Branch A: top-8 → prompt → LLM        │
        │   4b. Branch B: rerank 20 → top-8 → prompt  │
        │                       → LLM                 │
        │   5. Score answers vs gold (substring +     │
        │      ROUGE-L)                               │
        │   6. Emit q<NN>.trace.json (machine)        │
        └─────────────────┬──────────────────────────┘
                          │
                          ▼
                logs/eval/<ts>/q<NN>.trace.json   (raw, per question)
                logs/eval/<ts>/results.jsonl      (one line per q)
                logs/eval/<ts>/run_manifest.json
                          │
                          ▼
        ┌────────────────────────────────────────────┐
        │  python scripts/reranker_eval/render.py     │
        │  + scripts/reranker_eval/summarize.py       │
        │  - reads *.trace.json                        │
        │  - emits q<NN>_<slug>.log  (Layer 1+2 text) │
        │  - emits eval_summary.md   (roll-up)        │
        └─────────────────┬──────────────────────────┘
                          ▼
       logs/eval/<ts>/q<NN>_<slug>.log     ← human-readable
       logs/eval/<ts>/eval_summary.md      ← human-readable
```

**Separation of concerns:**

- **Rust test** = pipeline runner. Emits machine-readable JSON per question.
  Knows nothing about formatting.
- **Python renderer** = formatter. Reads JSON, produces the dual-layer
  human-readable `.log` and the summary. Lets us iterate on the wording
  without re-running expensive LLM calls.

This split is critical: re-rendering the report after a wording tweak
takes seconds, not 30 minutes.

## §2 File inventory (new files only)

| Path | Type | Lines (est.) | Owner |
|---|---|---|---|
| `src-tauri/tests/reranker_ab_eval.rs` | Rust integration test | ~600 | retrieval orchestration |
| `src-tauri/tests/common/eval_corpus.rs` | Rust helper module | ~150 | DB + model init |
| `scripts/reranker_eval/__init__.py` | empty | 0 | — |
| `scripts/reranker_eval/render.py` | Python | ~400 | per-question `.log` rendering |
| `scripts/reranker_eval/summarize.py` | Python | ~200 | `eval_summary.md` roll-up |
| `scripts/reranker_eval/rouge.py` | Python | ~80 | ROUGE-L (no external deps) |
| `scripts/reranker_eval/README.md` | docs | ~60 | how to run |
| `data/reranker/eval_runs/.gitkeep` | placeholder | 0 | output dir |

Nothing else changes. **Zero edits to `src-tauri/src/**`.**

If during stage T2 (§3) we discover a needed service function is not
`pub`, the plan is **paused** and the architect is asked to approve a
single visibility change, which is then logged in CHANGELOG_DEV.md and
reverted at the end via the cleanup task T9.

## §3 Task breakdown

Tasks are ordered. Each task ends with a verification step. No task is
"done" until its verification passes.

### T1 — Test scaffold & corpus probe (Rust)

**Goal:** prove we can read `echo.db`, list chunks, and confirm the eval
set's gold passages are findable in the corpus.

**Deliverables:**

- `src-tauri/tests/reranker_ab_eval.rs` with a single `#[test]` named
  `probe_corpus` that:
  - opens `echo.db` read-only (path from env `ECHO_DB_PATH`);
  - prints chunk count, source count;
  - loads `data/reranker/v1/eval.jsonl`;
  - for each row, searches the corpus for `pos[0]` via a substring match
    on `chunks.content` and reports `found / total`.
- `src-tauri/tests/common/eval_corpus.rs` exposing a `Corpus` struct with
  read-only DB access.

**Verification:** `cargo test --test reranker_ab_eval probe_corpus --
--nocapture` prints `corpus chunks: 510`, `eval rows: 23`, `gold found
in corpus: N/23`. N is reported; if < 20 we revisit §10-R2 of spec.

### T2 — Dense retrieval (Rust)

**Goal:** for each question, get top-50 chunks by cosine similarity using
the same embedder the app uses.

**Deliverables:**

- In `eval_corpus.rs`: `fn embed_query(&self, q: &str) -> Vec<f32>` that
  calls the existing `services::embedder::Embedder` (or its public
  equivalent — confirmed during T1).
- In the test: `fn dense_top_k(corpus, qvec, k=50) -> Vec<DenseHit>`
  using sqlite-vec or the same KNN path the app uses.
- Each `DenseHit` carries: `chunk_id, source_path, page, heading_path,
  cosine, content_full` (no truncation).

**Verification:** new `#[test] dense_smoke` runs query 0, asserts >=10
hits, prints rank/cosine/source for top-5.

### T3 — FTS5 retrieval (Rust)

**Goal:** for each question, get top-50 chunks by BM25 against the
existing FTS5 table.

**Deliverables:**

- A query expander that mirrors the app's logic (probably 3-keyword cap
  with 2/1 fallback per `feedback_speed_first_priority.md` / Stage B
  audit code). Confirmed against `services/query_router.rs` during T1.
- `fn fts5_top_k(corpus, q_text, k=50) -> Vec<FtsHit>` returning
  `chunk_id, source_path, page, bm25, matched_terms[], content_full`.

**Verification:** `#[test] fts_smoke` runs query 0, asserts >=1 hit,
prints matched terms and BM25 for top-5.

### T4 — Hybrid fusion (Rust)

**Goal:** merge dense + FTS5 candidate sets into a single ranked top-20
list, using **the same fusion algorithm the production app uses**.

**Deliverables:**

- Inspect `services/query_router.rs` (read-only) to identify the fusion
  method. If RRF: implement RRF in the test with the same `k` constant.
  If weighted: same.
- `fn hybrid_fuse(dense, fts, top_k=20) -> Vec<HybridHit>` returning
  `chunk_id, dense_rank, dense_cosine, bm25_rank, bm25_score, fused_score,
  fused_rank, content_full, source_path, page, heading_path`.

**Verification:** `#[test] fusion_smoke` runs query 0, asserts each hit
appears in either dense or FTS5 input (or both), and `fused_rank` is
strictly ascending.

### T5 — Reranker invocation (Rust)

**Goal:** call the reranker on the 20 hybrid candidates, capture per-pair
score and truncation diagnostics.

**Deliverables:**

- Use `services::reranker::Reranker::score_pairs(query, candidates)`
  (already public, confirmed at line 35).
- Reranker model path comes from env `ECHO_RERANKER_MODEL_PATH` (reused
  from PHR 042) so we can swap zero-shot vs fine-tuned without code
  changes.
- For each pair, also compute: pair character length, whether the
  passage exceeded 512 whitespace tokens (our truncation proxy from
  dataset card §3), and the byte offset where truncation would cut.

**Verification:** `#[test] rerank_smoke` runs query 0 with 20 candidates,
asserts all 20 get a finite score, prints sorted table.

### T6 — Branch A & B prompt assembly + LLM call (Rust)

**Goal:** build the exact prompt the app builds and call the LLM with
temp=0, identical for both branches except for chunk order/selection.

**Deliverables:**

- Identify the prompt template in `services/llm.rs` (read-only). If it's
  buried in a private function, copy the **template string** into the
  test (with a comment citing source line). This is not a "code change"
  — it's a test fixture mirroring production prompt.
- `fn assemble_prompt(question, top_chunks) -> String`.
- `fn run_llm(prompt) -> LlmOutput { answer, prompt_tokens, gen_ms }`
  using existing `services::llm` entry point. Temp=0, seed=42, top_p=1.
- Determinism self-check: run Branch A twice in `#[test] determinism`;
  identical output bytes required.

**Verification:** `#[test] determinism` passes. `#[test] branches_smoke`
runs query 0 through both branches and prints both answers.

### T7 — Per-question trace emission (Rust)

**Goal:** for each of 23 questions, emit a single JSON file with every
artifact from T2–T6.

**Deliverables:**

- `#[test] run_full_eval` (gated by env `ECHO_RUN_FULL_EVAL=1` so it
  doesn't run by default — too slow for `cargo test`).
- Output directory: `logs/eval/<ISO-timestamp>/`.
- Schema per question (`q<NN>.trace.json`):

```json
{
  "id": 7,
  "query": "...",
  "gold_passage": "...",
  "gold_passage_chars": 412,
  "gold_chunk_id_if_present": "c_412",
  "timestamp_utc": "2026-05-18T...",
  "stages": {
    "dense": {
      "embedder": "BAAI/bge-base-en-v1.5",
      "elapsed_ms": 42,
      "hits": [
        { "rank": 1, "chunk_id": "c_412", "cosine": 0.821,
          "source": "constitution.pdf", "page": 14,
          "heading_path": ["Part II", "Chapter 1", "Article 25"],
          "content": "<full chunk body>" },
        ...50 hits
      ]
    },
    "fts5": {
      "fts_query": "article AND rights AND equality",
      "elapsed_ms": 11,
      "hits": [
        { "rank": 1, "chunk_id": "c_412", "bm25": -8.2,
          "matched_terms": ["article", "equality"],
          "source": "...", "page": 14, "content": "..." },
        ...
      ]
    },
    "hybrid": {
      "method": "RRF", "k": 60, "top_k_kept": 20,
      "hits": [
        { "rank": 1, "chunk_id": "c_412",
          "dense_rank": 1, "bm25_rank": 1,
          "fused_score": 0.0328, "content": "..." },
        ...20
      ]
    },
    "branch_a": {
      "top_k_to_llm": 8,
      "selected_chunk_ids": ["c_412", "c_088", ...],
      "prompt_full": "<system>...</system>...",
      "prompt_tokens": 3412,
      "answer": "...",
      "answer_chars": 612,
      "llm_gen_ms": 41200,
      "gold_substring_hit": true,
      "rouge_l": 0.42
    },
    "branch_b": {
      "reranker_model": "BAAI/bge-reranker-base",
      "reranker_model_sha256": "...",
      "elapsed_ms": 412,
      "pairs": [
        { "chunk_id": "c_412", "rerank_score": 0.71,
          "hybrid_rank": 1, "rerank_rank": 2,
          "delta_rank": 1, "truncated": true,
          "truncation_cut_at_chars": 1880,
          "pair_chars": 2104,
          "rerank_score_bucket": "medium" },
        ...20
      ],
      "top_k_to_llm": 8,
      "selected_chunk_ids": ["c_088", "c_412", ...],
      "prompt_full": "...",
      "prompt_tokens": 3389,
      "answer": "...",
      "llm_gen_ms": 41600,
      "gold_substring_hit": true,
      "rouge_l": 0.58
    },
    "verdict": {
      "gold_in_a_topk": true, "gold_rank_in_a": 1,
      "gold_in_b_topk": true, "gold_rank_in_b": 2,
      "rerank_delta_rank_of_gold": -1,
      "answer_winner": "b",
      "latency_overhead_ms": 412,
      "truncation_count": 1
    }
  }
}
```

- A `run_manifest.json` at the run root with git commit, model paths,
  model file sha256s, embedder version, reranker version, corpus chunk
  count, eval set sha256, seed=42, timestamp.

**Verification:** running with `ECHO_RUN_FULL_EVAL=1` produces exactly
23 `.trace.json` files + `results.jsonl` (each line == one verdict
summary) + `run_manifest.json`. Total directory size logged.

### T8 — Python renderer (plain-English + technical layers)

**Goal:** convert each `.trace.json` into a dual-layer `.log` text file
matching spec §6 and §6b.

**Deliverables:**

- `scripts/reranker_eval/render.py` reads a single `.trace.json` and
  emits `q<NN>_<slug>.log` with:
  - **Layer 1**: plain-English narrative obeying §6b style rules.
    Scores converted to words via fixed buckets:
    - cosine ≥0.85 → "very similar"; 0.70–0.85 → "fairly similar";
      <0.70 → "somewhat similar".
    - rerank score (sigmoid 0–1): ≥0.7 → "high"; 0.4–0.7 → "medium";
      <0.4 → "low".
    - BM25: relative ranking within result set, not raw score.
  - Per-step narrative includes **full chunk text** for every passage
    that appears in the hybrid top-20 (so the reader can read the actual
    content without leaving the file). Example:
    ```
    Passage 1 (constitution.pdf, page 14):
    ──────────────────────────────────────────
    "Article 25 of the Constitution provides that all citizens are
    equal before the law and entitled to equal protection..."
    [full text — 412 characters]
    ──────────────────────────────────────────
    ```
  - Selection process narrated for every chunk: "kept because of
    strong meaning match"; "kept because of strong keyword match";
    "kept because both methods agreed"; "dropped after reranker
    judged it less relevant".
  - Both prompts shown in full (the spec requires the reader to see
    exactly what the LLM was given).
  - Both answers shown in full, with `gold_substring_hit` indicated
    in plain words ("matched the expected answer" / "did not match").
  - Verdict in plain words.
  - A separator line, then Layer 2 (the raw JSON tables, lightly
    formatted as Markdown tables for readability).

**Verification:** `python scripts/reranker_eval/render.py
logs/eval/<ts>/q07.trace.json` produces `q07_article_25_equality.log`.
A human reviewer (you) reads it cold and confirms:

- can summarize the result in 2 sentences after reading only Layer 1;
- every chunk that appeared in any stage has its full text visible
  somewhere in the file;
- no engineer jargon in Layer 1.

### T9 — Python summarizer

**Goal:** roll-up `eval_summary.md` per spec §7.

**Deliverables:**

- `scripts/reranker_eval/summarize.py` reads all `.trace.json` in a run
  dir and emits `eval_summary.md` with:
  - **Plain-English section** at top (matching spec §7 example):
    "We tested 23 questions… reranker helped on N, hurt on M, …".
  - **One-line recommendation** computed from numbers (if recall@5
    delta ≥ +0.05 → "proceed with fine-tune"; if truncation > 25% →
    "tune chunker before fine-tune"; if delta ≤ 0 → "abandon").
  - Retrieval quality table (recall@1/5/8, MRR, mean Δrank).
  - Answer quality table (substring hit rate, ROUGE-L mean).
  - Reranker diagnostics (pairs scored, truncation rate, latency,
    gold demoted count).
  - Per-question table (q, gold_rank_A, gold_rank_B, answer winner).

**Verification:** running on a 23-question run produces the summary;
all six tables present; recommendation line is non-empty and consistent
with the metrics.

### T10 — Cleanup & non-invasiveness check

**Deliverables:**

- `git diff --exit-code src-tauri/src/` passes (no production code
  changes).
- `scripts/reranker_eval/README.md` with the three commands:
  1. `ECHO_DB_PATH=... ECHO_RUN_FULL_EVAL=1 cargo test --test
     reranker_ab_eval -- --nocapture`
  2. `python scripts/reranker_eval/render.py logs/eval/<ts>/*.trace.json`
  3. `python scripts/reranker_eval/summarize.py logs/eval/<ts>/`
- `.gitignore` already covers `logs/` — verify and adjust if needed.
- CHANGELOG_DEV.md entry summarizing the harness.

**Verification:** new clone + run produces a complete `logs/eval/<ts>/`
directory with 23 `.log` files + summary, with no Rust src diff.

## §4 Execution order & checkpoints

1. **T1 → T2 → T3 → T4** are sequential; each has a smoke `#[test]` that
   must pass before moving on.
2. **T5 → T6** sequential, depend on T4.
3. **T7** integrates T2–T6.
4. **T8** and **T9** can run in parallel after T7 emits sample JSON.
5. **T10** is the final checkpoint.

Architect review checkpoints (where work pauses for sign-off):

- **After T1**: confirm gold-passage findability rate (R2 in spec §10).
- **After T6 first run**: read both answers for question 0; confirm
  Branch A & B determinism is real and prompts are formed correctly.
- **After T8 first render**: read `q00_*.log` end-to-end; confirm
  plain-English layer meets §6b.
- **Before T10**: final architect read of full run.

## §5 Risks & mitigations (delta from spec §10)

- **R-plan-1**: Embedder/reranker/LLM may not initialize cleanly from a
  test binary because they expect Tauri app state. *Mitigation:* T1
  probes this. If app state is required, we add a minimal `AppState`
  builder in `tests/common/` that mimics what the dev binary does. This
  is still test-only code.
- **R-plan-2**: Prompt template duplication (T6) could drift from
  production. *Mitigation:* the test file cites the source line(s) of
  the template; CHANGELOG_DEV.md notes that touching `services/llm.rs`
  prompt code requires updating the test fixture.
- **R-plan-3**: Full-text in JSON inflates trace size. Estimated 200KB
  per question × 23 ≈ 5MB per run. Acceptable; not a blocker.

## §6 Acceptance (mirrors spec §8, restated for the plan)

- [ ] `cargo test --test reranker_ab_eval -- --nocapture` runs without
      production code edits.
- [ ] `git diff --exit-code src-tauri/src/` passes after T10.
- [ ] 23 `.log` files produced, each with Layer 1 + Layer 2; every chunk
      that participated in any stage has its full text visible.
- [ ] `eval_summary.md` produced with plain-English section + 6 tables +
      recommendation line.
- [ ] Determinism self-check passes (Branch A run twice = identical
      answer bytes).
- [ ] Reverting the experiment = `rm src-tauri/tests/reranker_ab_eval.rs
      src-tauri/tests/common/eval_corpus.rs scripts/reranker_eval/`
      with no further edits.

---

**Next:** on plan approval, generate `tasks.md` (one row per T-task with
file paths, acceptance check, est. effort) via `/sp.tasks`.
