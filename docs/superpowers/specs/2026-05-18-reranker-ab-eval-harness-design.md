# Reranker A/B Eval Harness — Design Spec

**Date:** 2026-05-18
**Branch:** `feature/tier3-hierarchical-retrieval`
**Stage:** spec
**Status:** draft — pending architect sign-off

---

## §1 Problem statement

We need to measure the value of the reranker stage on EchoAI's retrieval
pipeline **before** fine-tuning `bge-reranker-base` on Colab T4, and **again
after** fine-tuning, to quantify the delta.

Today there is no harness that:

1. Runs an eval set through the full pipeline twice — once with the reranker
   and once without — under identical conditions.
2. Captures every intermediate artifact (dense candidates, FTS5 candidates,
   hybrid fusion, reranker scores, final prompt, LLM answer) in a single
   human-readable file per question.
3. Aggregates per-question results into a summary table (recall, MRR,
   answer-match, latency).

Without this we cannot prove the reranker fine-tune was worth the Colab time.

## §2 Goals

- **G1** — Produce a per-question trace file rich enough that a non-engineer
  can read it end-to-end and understand what the app did, why it picked each
  chunk, and what the LLM saw.
- **G2** — Compare **Branch A (hybrid → LLM)** vs **Branch B (hybrid →
  reranker → LLM)** under deterministic conditions (temp=0, fixed seeds).
- **G3** — Roll all questions up into a numeric summary suitable for
  pre-fine-tune vs post-fine-tune comparison.
- **G4** — Zero production code changes. The harness lives entirely in
  `src-tauri/tests/` + `scripts/reranker_eval/` and is removable by deleting
  two paths.

## §3 Non-goals

- Not a production logging change. `services/query_trace.rs` stays untouched.
- Not a UI feature. No Tauri commands, no SolidJS work.
- Not a generic benchmark framework. Scoped to the reranker A/B question.
- Not an end-to-end answer-quality grader. Answer scoring is best-effort
  (substring + ROUGE-L); human spot-check remains the gold.

## §4 Inputs

- **Eval set**: `data/reranker/v1/eval.jsonl` — 23 BGE-format triples.
  - Each row: `{ query, pos: [text], neg: [text, ...] }`.
  - The harness extracts `query` as the question and uses `pos[0]` as the
    *gold passage* for recall/MRR scoring.
- **Corpus**: existing indexed corpus in `echo.db` (510 leaf chunks,
  6 Sindh govt PDFs + Constitution of Pakistan).
- **Models**:
  - Embedder: whatever is currently configured (BGE base-en-v1.5 expected).
  - Reranker: `bge-reranker-base` for the pre-fine-tune run; fine-tuned
    checkpoint path for the post-fine-tune run. Selected via env var
    `ECHO_RERANKER_MODEL_PATH` (reuse mechanism from PHR 042).
  - LLM: fine-tuned Qwen2.5-0.5B (project's main generator) — temp=0,
    top_p=1.0, fixed seed.

## §5 Outputs

All written under `logs/eval/<ISO-timestamp>/`:

1. **`q<NN>_<slug>.log`** — one human-readable file per question. Layout
   defined in §6.
2. **`results.jsonl`** — one machine-readable line per question, with all
   numeric fields (scores, ranks, latencies, answer text).
3. **`eval_summary.md`** — roll-up tables. Layout defined in §7.
4. **`run_manifest.json`** — capture of env: model paths, model sha,
   embedder version, reranker version, git commit, corpus row count, seed.

## §6 Per-question trace layout

Each trace file has **two layers**:

- **Layer 1 — Plain-English story** (top of file): a non-technical narrative
  that anyone (manager, lawyer, domain expert) can read and understand. No
  jargon. No scores. Just "the app searched for X, found these documents,
  picked these as most relevant, and the AI answered Y."
- **Layer 2 — Technical detail** (below): the full numeric tables, scores,
  prompts, and raw chunk text — for engineers debugging the pipeline.

A reader can stop after Layer 1 and still understand the result. Layer 2 is
opt-in.

### Layer 1 — Plain-English story (mandatory, top of file)

```
================================================================
  QUESTION 7 of 23                                     2026-05-18
================================================================

THE QUESTION
  "What does Article 25 of the Constitution say about equality?"

WHAT WE EXPECTED TO FIND (gold answer from our test set)
  A passage from the Constitution stating all citizens are equal
  before the law and entitled to equal protection.

----------------------------------------------------------------
  HOW THE APP SEARCHED
----------------------------------------------------------------

Step 1 — Meaning search (vector search)
  The app converted the question into a numeric "fingerprint" and
  compared it against fingerprints of all 510 passages in your
  documents. The 50 most similar passages were kept.

  Top 3 passages by meaning similarity:
    1. constitution.pdf, page 14 — "Article 25 — All citizens..."
       (88% similar)
    2. sindh_local_govt_act_2013.pdf, page 3 — "Equality of..."
       (82% similar)
    3. ...

Step 2 — Keyword search (FTS5)
  Separately, the app looked for the actual words in the question
  ("article", "equality", "constitution") inside the documents.
  This catches matches that meaning-search might miss.

  Top 3 passages by keyword match:
    1. constitution.pdf, page 14 — matched "article", "equality"
    2. ...

Step 3 — Combining both searches
  The app merged both lists, favoring passages that scored well
  on either approach. The combined top 20 passages went to the
  next stage.

----------------------------------------------------------------
  TWO WAYS TO PICK THE FINAL ANSWERS
----------------------------------------------------------------

We tested two approaches in parallel on the SAME 20 passages:

  APPROACH A — Trust the combined search ranking.
              Take the top 8 passages as-is.

  APPROACH B — Re-judge with a smarter model (the reranker).
              The reranker reads the question and each passage
              together (not just compares fingerprints) and gives
              a fresh relevance score. Then we take the top 8.

----------------------------------------------------------------
  APPROACH A — Without reranker
----------------------------------------------------------------

The 8 passages sent to the AI:
  1. constitution.pdf p.14 — "Article 25 — All citizens are equal..."
  2. sindh_local_govt_act_2013.pdf p.3 — "..."
  ...

The AI's answer:
  > Article 25 of the Constitution states that all citizens are
  > equal before the law and entitled to equal protection of law.
  > ...

Did this match what we expected?  YES (partial match)
Time taken:  41.2 seconds

----------------------------------------------------------------
  APPROACH B — With reranker
----------------------------------------------------------------

The reranker re-ordered the passages. Notable changes:
  • Passage from sindh_local_govt_act_2013.pdf moved UP (was #3,
    now #1) — the reranker thought it was more relevant.
  • Passage from constitution.pdf p.14 moved DOWN (was #1, now #2).

The 8 passages sent to the AI (new order):
  1. sindh_local_govt_act_2013.pdf p.3 — "..."
  2. constitution.pdf p.14 — "Article 25 — All citizens..."
  ...

The AI's answer:
  > ...

Did this match what we expected?  YES (full match)
Time taken:  41.6 seconds  (reranker added 0.4s)

----------------------------------------------------------------
  VERDICT
----------------------------------------------------------------

  Better approach for this question:  B (with reranker)
  Why: the reranker promoted a more specific passage to position 1,
       which gave the AI cleaner context.

  Caveats: 1 of the 20 passages was too long (>512 words) and got
           cut off before the reranker could read all of it. This
           may have affected scoring.

================================================================
  TECHNICAL DETAIL BELOW — skip if not debugging
================================================================
```

### Layer 2 — Technical detail (existing tables below the narrative)

Single text file, sections in order:

```
=== QUESTION q07 ===
id:              7
text:            <verbatim question>
gold_passage:    <first 200 chars + char_len of pos[0]>
scope:           notebook=<id> / folder=<id|none>
timestamp:       2026-05-18T...

=== STAGE 1: DENSE (vector) RETRIEVAL ===
embedder:        BAAI/bge-base-en-v1.5
query_vec_dim:   768
top_k:           50
elapsed_ms:      <n>

rank  chunk_id  cosine  source                              page  preview
 1    c_412     0.821   constitution.pdf                    14    "Article 25 — All citizens..."
 2    c_088     0.798   sindh_local_govt_act_2013.pdf       3     ...
 ...
(full text of each chunk shown below)

--- DENSE CANDIDATE FULL TEXT ---
[c_412] <full chunk body, no truncation>
[c_088] ...

=== STAGE 2: FTS5 (lexical) RETRIEVAL ===
fts_query:       <expanded query — quoted terms>
matched_terms:   ["article", "fundamental", "rights"]
top_k:           50
elapsed_ms:      <n>

rank  chunk_id  bm25   source                              matched_terms
 1    c_412     -8.2   constitution.pdf                    article,rights
 ...

--- FTS5 CANDIDATE FULL TEXT (chunks not in dense set) ---
[c_201] ...

=== STAGE 3: HYBRID FUSION ===
method:          RRF (k=60)   # or whatever current impl uses
union_size:      <n>
top_k_kept:      20

rank  chunk_id  dense_rank  bm25_rank  hybrid_score
 1    c_412     1           1          0.0328
 ...

=== STAGE 4A: BRANCH A — NO RERANKER ===
top_k_to_llm:    8
selected:        [c_412, c_088, ...]

--- ASSEMBLED PROMPT (full) ---
<system>...</system>
<context>
  [1] (constitution.pdf p.14) <full chunk text>
  [2] ...
</context>
<question>...</question>
--- END PROMPT ---
prompt_tokens:   3412
llm_temp:        0
elapsed_ms:      <n>

--- LLM ANSWER (Branch A) ---
<verbatim answer>
--- END ANSWER ---
answer_chars:    <n>
gold_substring_hit: true|false
rouge_l_vs_gold: 0.42

=== STAGE 4B: BRANCH B — WITH RERANKER ===
reranker_model:  BAAI/bge-reranker-base   # or fine-tuned-v1
input_pairs:     20
elapsed_ms:      <n>

rerank table (sorted by rerank_score desc):
rank  chunk_id  rerank_score  hybrid_rank  Δrank   truncated  pair_chars
 1    c_088     0.94          2            ↑1      no         812
 2    c_412     0.71          1            ↓1      yes@512    2104
 ...

top_k_to_llm:    8
selected:        [c_088, c_412, ...]

--- ASSEMBLED PROMPT (full) ---
<same layout as Branch A>
--- END PROMPT ---
prompt_tokens:   3389
elapsed_ms:      <n>

--- LLM ANSWER (Branch B) ---
...
--- END ANSWER ---
answer_chars:    <n>
gold_substring_hit: true|false
rouge_l_vs_gold: 0.58

=== VERDICT q07 ===
gold_chunk_in_A_topK:      true (rank 1)
gold_chunk_in_B_topK:      true (rank 2)
rerank_Δrank_of_gold:      -1 (demoted)   # negative = bad for this question
answer_match_A:            partial
answer_match_B:            full
better_branch:             B
latency_overhead_rerank_ms: 412
truncation_count:          1/20 pairs
```

## §6b Plain-English style rules (Layer 1)

To keep Layer 1 readable by non-technical reviewers, the generator must
obey these rules:

- **No raw scores** in Layer 1. Convert to plain words:
  cosine 0.88 → "88% similar"; BM25 negative numbers → "strong/weak
  keyword match"; logits → "the reranker rated this high/medium/low".
- **No internal IDs** in Layer 1. Use `filename.pdf p.N` instead of
  `chunk_id c_412`. The technical layer keeps the IDs.
- **No model names** in Layer 1 prose (e.g., don't say
  "BAAI/bge-reranker-base"). Use "the reranker" or "the smarter model".
  Model identity goes in the manifest + Layer 2.
- **No tokenizer/truncation jargon** in Layer 1. "Too long and got cut
  off" instead of "truncated at 512 tokens".
- **Short sentences**. One idea per line. Bulleted lists for sets of 3+
  items.
- **Each major step gets a one-sentence "why we did this"** so a reader
  who has never seen retrieval can follow.
- **Verdict block at the bottom of Layer 1** must answer in plain words:
  did the reranker help on this question, and why.

A reader who reads only Layer 1 should be able to summarize the result
in two sentences to a colleague.

## §7 Eval summary layout (`eval_summary.md`)

The summary file ALSO has a plain-English section at the top, before
the metric tables, e.g.:

```
# Reranker A/B Eval — <timestamp>

## In plain English

We tested the app on 23 questions about Pakistani legal and government
documents. For each question, we let the app answer twice — once
without the reranker, once with it — using the same retrieved passages.

Overall:
  • The reranker helped on 11 of 23 questions.
  • It hurt on 4 questions (promoted a less-relevant passage).
  • It made no difference on 8 questions.
  • It added on average 0.4 seconds per question.

Key issue spotted: on 19% of passages, the reranker could only read
the first half because the passage was too long. Fixing this (or
fine-tuning on shorter passages) is likely to improve results.

Recommendation:  <one line — proceed with fine-tune / re-tune chunker
                  / abandon reranker — based on the numbers below>

## The numbers

# Reranker A/B Eval — <timestamp>

Model: bge-reranker-base (zero-shot)        # or fine-tuned-v1
Eval set: data/reranker/v1/eval.jsonl (23 q)
Git commit: <sha>

## Retrieval quality (gold-passage)

metric              Branch A (no rerank)  Branch B (rerank)  Δ
recall@1            0.30                  0.43               +0.13
recall@5            0.65                  0.78               +0.13
recall@8            0.78                  0.83               +0.05
MRR                 0.41                  0.56               +0.15
mean Δrank of gold  —                     -1.4 (promoted)

## Answer quality (vs gold answer text)

metric              Branch A   Branch B   Δ
exact substring     0.35       0.52       +0.17
ROUGE-L (mean)      0.41       0.55       +0.14

## Reranker diagnostics

pairs scored total:        460   (23 q × 20 candidates)
truncated @ 512 tokens:    87    (18.9%)
mean rerank latency:       380 ms / question
gold demoted by rerank:    4 / 23 questions

## Per-question table

q  gold_rank_A  gold_rank_B  answer_A  answer_B  winner
01  1            1            full      full      tie
02  3            1            partial   full      B
...
```

## §8 Acceptance criteria

- [ ] Running `cargo test --test reranker_ab_eval -- --nocapture` against a
      pre-existing populated `echo.db` produces exactly 23 trace files +
      1 `results.jsonl` + 1 `run_manifest.json`.
- [ ] Running `python scripts/reranker_eval/summarize.py
      logs/eval/<ts>/` produces `eval_summary.md` with all §7 tables.
- [ ] Branch A and Branch B use **identical** dense + FTS5 + hybrid stages
      (verified by checksum of `top_k_kept` chunk-id list per question).
- [ ] LLM is invoked with temp=0 in both branches; identical context order →
      identical answer (verified by a self-check that runs Branch A twice).
- [ ] All chunk bodies in trace files are **full text**, not previews
      (`char_len` field matches `len(text)`).
- [ ] Reranker truncation events are logged with the cut position.
- [ ] Reverting the experiment is achievable by:
      `rm src-tauri/tests/reranker_ab_eval.rs scripts/reranker_eval/`
      with **no further code edits required**.
- [ ] `services/query_trace.rs` is byte-identical to its pre-change state
      (`git diff --exit-code` passes for that file).

## §9 Non-invasiveness contract

- The harness is an integration test under `src-tauri/tests/`. It is
  compiled only by `cargo test`; the release binary is unaffected.
- It calls existing service functions as a library consumer would. It does
  not introduce new public types in production modules.
- If a required function is currently `pub(crate)` and must be lifted to
  `pub` for test access, that single visibility change is documented in the
  plan and reverted post-experiment.
- All outputs go under `logs/eval/` which is `.gitignore`d.
- The Python summarizer has no runtime dependency on the Rust app.

## §10 Open risks

- **R1** — If the embedder or LLM is not loadable from a test binary
  (e.g., requires Tauri app state), we may need a thin test fixture that
  initializes them. Mitigation: prototype the test scaffold first; if
  initialization is heavyweight, fall back to spawning the dev binary with
  a `--eval-mode` flag that *is* a production change (last resort).
- **R2** — `data/reranker/v1/eval.jsonl` queries may not all have their gold
  passage actually present in the current `echo.db` (corpus drift since
  dataset build). Mitigation: harness emits a `gold_missing_from_corpus`
  flag per question; those rows are excluded from recall metrics but kept
  in the trace.
- **R3** — Reranker truncation rate (~19% estimated from training set) may
  dominate the signal. The diagnostics block in §6 surfaces this explicitly
  so it doesn't hide.
- **R4** — ROUGE-L on legal text is noisy. Treat answer-quality numbers as
  directional, not definitive; the trace files are the real evidence.

## §11 Out of scope (future work)

- Multi-corpus eval (broaden beyond legal/govt).
- Automatic answer grading via a judge LLM.
- CI integration / regression gates.
- Comparison against `bge-reranker-large`.

---

**Sign-off needed before plan:** architect confirms §4 inputs, §6 layout,
§8 acceptance, §9 non-invasiveness.
