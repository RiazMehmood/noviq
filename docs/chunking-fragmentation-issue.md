# Issue: Catastrophic chunk fragmentation & mislabeled hierarchy (RAG answer-quality root cause)

**Status:** Diagnosed (evidence-based), remedy proposed — not yet implemented
**Date:** 2026-06-13
**Surface affected:** Chat (`chat_query_legal`) and, to a lesser degree, Drafting (`one_ask`)
**Severity:** Blocking for citation-grade accuracy on OCR-extracted PDFs

---

## 1. Summary

Live query traces produced a wrong answer (fabricated "Rule 2017 of the Sindh Civil
Services Rules") and a correct answer with wrong citations. Deep investigation against the
live Postgres corpus + the code proves the dominant cause is **not** the model, the prompt,
or retrieval ranking — it is **indexing/chunking**. Provisions are shredded into
sub-sentence fragments (median **41 characters**), figures are split mid-value
("Rs. 10,000" | "million"), and the hierarchy is mislabeled (years/page-numbers captured as
"Rules"). A correct, complete provision frequently **does not exist in any single chunk**, so
no model and no retriever can answer correctly.

## 2. Evidence

### 2.1 Corpus-wide fragmentation (live DB, 20,817 chunks)

| Metric | Value |
|---|---|
| Median chunk length | **41 characters** |
| Mean chunk length | 89 characters |
| Chunks < 100 chars | **73 %** |
| Chunks < 50 chars | **55 %** (11,429 chunks) |
| `06_Manual_for_Development_Projects_2019_PC-I.pdf` | **7,964 chunks, avg 56 chars** |
| `10_SPSC_Regulations_2023.pdf` | 3,076 chunks, avg 74 chars |
| `09_Sindh_Delegation_of_Financial_Powers_2019.pdf` | 2,367 chunks, avg 80 chars |
| (healthy baseline) `03_Sindh_Public_Procurement_Act_2009.pdf` | 90 chunks, avg 363 chars |

The worst offenders are exactly the OCR-extracted PDFs the failing queries hit.

### 2.2 A single provision shredded across 6 chunks

Source provision (reassembled from `chunks` idx 962–967, source `bf7eb14d…`):

> "…in Punjab, Sindh, Khyber Pakhtunkhwa and Balochistan **are competent to approve
> development projects costing up to Rs. 10,000 million, provided that no external funding is
> involved.**"

| chunk_index | length | content |
|---|---|---|
| 962 | 48c | `…in Punjab, Sindh, Khyber Pakhtunkhwa` |
| 963 | 26c | `and Balochistan` |
| 964 | 13c | `are` |
| **965 (retrieved)** | 82c | `competent to approve development projects costing up to Rs . 10,000` |
| 966 (not retrieved) | 33c | `million, provided that` |
| 967 (not retrieved) | 42c | `no external funding is involved.` |

The retrieved chunk **stops at "Rs. 10,000"** — the word **"million"** lives in the next,
un-retrieved chunk. The model's "Rs. 10,000" was **not a hallucination — it was a truncated
chunk.** The complete answer exists in *no* single chunk.

### 2.3 Mislabeled hierarchy

- `Rule 2017` is a **mis-captured year**: chunk `unit_path = Rule_2017/(2)/(e)`. The structure
  parser turned the year *2017* into a Rule number; it is the hierarchy root for many chunks.
  This is the "rule" the model cited in the failing trace.
- Long runs of fragments share one junk label, e.g. `Section 2.10` under `Section_8/Section_2.10`.
- The Manual contains **7,377 `unit_type = "clause"` units** — one unit **per OCR line**, not
  per provision.
- OCR inserted spurious newlines **mid-word / mid-number**: `develop\nment`, `Rs\n.\n10,000`.

### 2.4 Contrast — when a chunk survives intact, the answer is correct

The "who conducts local-government elections?" query answered **correctly** because its
supporting chunk survived whole:

> `Rule 4` (138c): "Subject to the provisions of the Act, the Election … Commission shall
> organize and conduct an election."

The only variable separating a right from a wrong answer was **chunk integrity.**

### 2.5 Hypotheses ruled out (with proof)

- **Question truncation — RULED OUT.** ChatML template (`query_router.rs:1844`) places the
  question at the **tail**; truncation (`llm.rs:541‑555`) keeps **HEAD + TAIL** (≥256 tail
  tokens), preserving the question; and these prompts (2364 tok) were **under** the 2500 cap,
  so no truncation occurred at all. Both answers were on-topic → the model saw the question.
- **Model capability — NOT primary.** Given an intact chunk (Rule 4) the 0.5B answered
  correctly. It fails only when the data it is handed is incomplete.

## 3. Root-cause mechanism (code)

`heading_detector/{regex_pass,llm_pass,tree}.rs` over-detects headings — every OCR line and
stray number (years, page numbers) becomes a "heading." `structure_parser::build_tree_from_headings`
(line 340) then builds one tree node per detected heading, and `collect_chunks` (line 787)
emits **one chunk per leaf node**. The only size-normalisation that exists is **downward**:
`split_long_text` (179) / `split_oversized_leaves` (270) split *oversized* nodes — there is
**no merge step for undersized sibling leaves**. Combined with OCR newline noise, the result
is tens of thousands of sub-sentence chunks.

## 4. Impact

- Correct provisions are physically absent as retrievable units → wrong / incomplete answers.
- Figures and conditions are severed ("Rs. 10,000" without "million"; "provided that…" lost).
- Junk labels ("Rule 2017") surface as authoritative citations.
- Embeddings of ~41-char fragments are weak and noisy → retrieval degraded as well.

## 5. Remedies considered

### Option A — Ingest-side re-chunk: OCR-repair + provision-level coalescing *(preferred)*
1. **OCR newline normalisation** pre-pass before heading detection: collapse newlines that
   fall mid-word / mid-number / mid-figure (`develop\nment` → `development`,
   `Rs\n.\n10,000` → `Rs. 10,000`), so detection and chunking see whole sentences.
2. **Heading-label sanitisation**: reject 4-digit years and bare page numbers as
   provision labels in `heading_detector`; require a real provision token
   (Rule/Section/Article/clause marker) before promoting a line to a heading node.
3. **Small-leaf coalescing pass** in `structure_parser` (the missing inverse of
   `split_oversized_leaves`): merge adjacent sibling leaves under the same parent into
   provision-level chunks targeting ~**300–800 chars**, **never splitting a sentence or a
   number+unit pair**.
4. **Re-index** the affected (OCR) documents.

*Pros:* fixes the actual cause once, at the source; benefits both Chat and Drafting and the
embeddings; deterministic and testable with golden chunk snapshots.
*Cons:* requires a re-index of the OCR PDFs (one-time cost; corpus is small).

### Option B — Heading-detection fix only, then re-index
Tighten detection so the tree forms correct provision nodes from the start (subset of A,
without the explicit coalescing pass).
*Pros:* smaller change. *Cons:* doesn't repair already-split sentences caused by OCR newlines;
leaves many small leaves where a provision genuinely spans several detected lines.

### Option C — Route OCR-damaged PDFs through the LLM IR (Mistral) path
Use the 7B IR builder to reconstruct structure for these PDFs instead of the regex importer.
*Pros:* highest structural fidelity. *Cons:* slow/expensive (minutes/doc on CPU), and the IR
path is reserved for genuinely scanned input; overkill for text-extractable PDFs with newline
noise.

### Option D — Downstream gate fixes (necessary safety net, not a cure)
- Chat path must **refuse-or-flag when grounding score is 0.00** instead of "Ship"
  (`query_router.rs` grounding step), matching the Drafting verbatim gate.
- Display **only the chunks the answer actually cited**, never top-ranked fillers.

*These stop wrong/unsupported answers from reaching the officer, but do not make correct
answers retrievable — they are complementary to A, not a replacement.*

## 6. Preferred remedy & sequencing

**Option A (ingest-side re-chunk) + Option D (gate safety net).**

Rationale: A is the only remedy that makes correct answers *exist* in the corpus — it is
upstream of retrieval, the model, and the citation gate, so it lifts every surface at once.
D is cheap and prevents the unsafe "ship at grounding 0.00" behaviour while A is built and the
re-index runs. B and C are partial/expensive and are folded into or superseded by A.

Suggested order:
1. **D first** (small, immediate safety): refuse-or-flag on grounding 0.00 + cite-only-used.
2. **A.1 + A.2** (OCR newline repair + heading-label sanitisation) — biggest structural wins.
3. **A.3** (small-leaf coalescing) with golden-snapshot tests on the Manual / Delegation docs.
4. **Re-index** the OCR PDFs; re-run the two trace queries as the acceptance gate.

## 7. Acceptance criteria

- Median chunk length for `06_Manual…PC-I.pdf` rises from ~56 chars to provision scale
  (target ≥ ~300 chars median; no single sentence split across chunks).
- The approval provision resolves to **one** chunk containing
  "…Rs. 10,000 million, provided that no external funding is involved."
- No `unit_label` / `unit_path` equals a bare 4-digit year or page number.
- Re-running the two saved traces: the project-approval query returns the correct threshold
  with a citation to the actual provision; the election query keeps its correct answer but now
  cites Rule 4 (not the Public Service Commission definition).
- Chat never ships an answer at grounding score 0.00 (it refuses or flags).

## 8. References

- Traces: `~/Desktop/final testing/2026-06-13_09-17-23_77370a9b.log` (wrong),
  `~/Desktop/final testing/2026-06-13_09-27-13_a0cc2705.log` (right answer, wrong citations).
- Code: `services/structure_parser.rs` (`build_tree_from_headings`, `collect_chunks`,
  `split_oversized_leaves`, `long_chunk_threshold`), `services/heading_detector/*`,
  `services/query_router.rs` (prompt assembly ~1844, grounding/citation steps),
  `services/llm.rs:541‑555` (head+tail keep).
- Related backlog: F007 content-prefix/year-as-number mis-capture; ~4k duplicate boilerplate
  chunks; chunk-boundary mid-sentence excerpts — now proven to be the dominant failure, not
  minor backlog.

---

# Appendix A — Validation Report (2026-06-13)

**Question:** validate or invalidate that chunk fragmentation is the *primary* root cause of
answer failures. **Method:** live Postgres corpus (20,817 chunks), source code, the two saved
traces, and a faithful A/B model experiment (production 0.5B GGUF loaded into ollama as
`sindh05`, raw ChatML, temp 0.1). NOTE: there are **two** chunking paths; the failing OCR docs
use `structure_parser.rs` (structured importer), **not** the generic `chunker.rs`.

## A1. Current chunking configuration

| Item | File · function | Value | Purpose |
|---|---|---|---|
| chunk_size (structured — used by failing docs) | `structure_parser.rs::collect_chunks` (L787) | **one chunk per heading-tree node**; no floor | one chunk per parsed unit |
| chunk_size (generic — not used here) | `chunker.rs` (L14) | `MAX_TOKENS = 512` | sliding window for unstructured text |
| chunk_overlap (structured) | — | **0 (none)** | structured path has no overlap |
| chunk_overlap (generic) | `chunker.rs::OVERLAP_TOKENS` (L15) | 64 tokens | only in the unused path |
| long_chunk_threshold | `structure_parser.rs:160` | **500 chars** (`ECHO_LONG_CHUNK_THRESHOLD`, floor 100) | split a node **down** if >500c |
| split_long_text | `structure_parser.rs:179` | para→sentence→char | sentence-preserving **on split-down only** |
| split_oversized_leaves | `structure_parser.rs:270` | post-order, threshold 500 | split oversized node text into children |
| collect_chunks | `structure_parser.rs:787` | 1 chunk/node, `unit_label=node.citation` | no size floor, **no merge** |
| **merge/coalescing** | — | **DOES NOT EXIST** | the missing inverse of `split_oversized_leaves` |
| sentence-preservation | `split_long_text` only | when splitting >500c | **no protection for tiny nodes** |

Finding: the structured path normalises only downward (split >500c). No lower bound, no merge —
a 13-char node becomes a 13-char chunk.

## A2. Retrieval pipeline

| Stage | Value | Code |
|---|---|---|
| Vector top_k | 24 (`ECHO_VEC_K`) | `query_router.rs:103,548` (pgvector halfvec KNN) |
| BM25/FTS top_k | 24 (`ECHO_FTS_K`) | `query_router.rs:109,1478` (PG `websearch_to_tsquery`) |
| Hybrid weighting | RRF k=60, equal-weight | `query_router.rs:714` `rrf_merge` |
| Reranker | **OFF** (hardcoded) | `query_router.rs:23,76` |
| Adjacent expansion | **none for general queries**; `expand_citation_neighbours` only on citation fast-path, over graph edges not ±N index | `query_router.rs:568,2432` |
| Dedup | HashSet on chunk_id | `query_router.rs:587,594` |
| Citation selection | `[N]` → legacy `[file,page]` → **else top-3 by rank** | `query_router.rs:1257–1320` |

Finding: the 0.5B emits no `[N]`, so citation selection **always** falls to top-3 by rank — the
shown citations are not what the answer used.

## A3. Neighbor-chunk recovery (the "Rs. 10,000 million" case)

| idx | id | content | in 24-vec | in 24-fts | retrieved |
|---|---|---|---|---|---|
| 962 | ae53c886 | "in Punjab, Sindh, KP" | no | no | ❌ |
| 963 | 72f5ddfb | "and Balochistan" | no | no | ❌ |
| 964 | d5cd7d15 | "are" | no | no | ❌ |
| 965 | 8b8bc28a | "competent…costing up to Rs. 10,000" | **yes** | no | ✅ (ctx [3]) |
| 966 | ca112e15 | **"million, provided that"** | no | no | ❌ |
| 967 | 707ab24b | "no external funding is involved." | no | no | ❌ |

Only 965 retrieved; 962–964/966/967 were not even in the 48-candidate pool. No auto neighbor
fetch. **±2 chunk_index expansion** would be Low effort (~20–40 lines at context-build) and would
recover 963–967 incl. "million" + proviso — a cheap partial fix for this exact case.

## A4. Synthetic merge experiment (production 0.5B, identical system/question/temp 0.1)

- **A (fragment as retrieved):** fabricated — *"Section 8 of the Constitution and Article 246-A…
  Chief Minister… if it exceeds Rs. 10,000."*
- **B (merged 962–967):** correctly reproduced *"…competent to approve development projects
  costing up to **Rs. 10,000 million, provided that no external funding is involved.**"* — but then
  appended a fabricated sentence and emitted no `[N]`.

Verdict: merging is **necessary** (correct fact only accessible when whole) but **not sufficient**
(0.5B still confabulates + never cites). Answer-fact correctness materially improves.

## A5. Heading-detector audit (`06_Manual…PC-I.pdf`, idx 955–972)

One OCR line per chunk, all mislabeled `Section 2.10`, bogus `Section 8` content-prefix on each;
one sentence split across 8 chunks (960–967). Identified: **years as rules** (`Section 2010` ×23,
`Rule 2017` ×2, `Section 2013` ×2, `Rule 2016/2023`, `Section 1956`; **41 chunks** have a year in
`unit_label`); **page/section numbers as units** (`3. 2`, `3. 3`); **OCR line fragments as nodes**.

## A6. Chunk-quality metrics

Global (n=20,817): mean **89.4**, p10 **11**, p25 **16**, p50 **41**, p75 **108**, p90 **234**,
p95 **324**; <50 = **55%**, <100 = **73%**, <200 = **87.3%**.

| Document | chunks | mean | p50 | p90 | <50 | <200 |
|---|---|---|---|---|---|---|
| 06_Manual…PC-I | 7,964 | 56 | 31 | 122 | 65.5% | 95.3% |
| 10_SPSC_Regulations_2023 | 3,076 | 74 | 23 | 217 | 66.0% | 88.7% |
| 09_Delegation_Financial_Powers | 2,367 | 80 | 35 | 225 | 59.8% | 88.3% |
| 02_SPPRA_Rules | 1,588 | 75 | 34 | 207 | 59.5% | 89.4% |
| 11_APT_Rules_1974 | 385 | 83 | 30 | 236 | 59.0% | 86.5% |
| Secretariat_Instruction_2010 | 1,554 | 97 | 34 | 279 | 56.9% | 82.9% |
| 05_Compendium_SLGA | 3,005 | 166 | 127 | 352 | 17.6% | 68.9% |
| 01_Rules_of_Business | 658 | 224 | 117 | 377 | 17.6% | 73.9% |
| 04_SLGA_Act_2013 | 130 | 122 | 84 | 264 | 27.7% | 83.1% |
| 03_SPPA_2009 | 90 | 363 | 148 | 884 | 33.3% | 66.7% |

Bimodal and predictive: OCR-structured docs are shattered (Manual p50=31, SPSC p50=23); cleaner
docs are not (Compendium p50=127, RoB p50=117). **Log 1 failed on the Manual; log 2 succeeded on
the Compendium** — tracking chunk health exactly.

## A7. Citation-integrity audit

Limitation (itself a finding): `query_messages` has **0 assistant rows** — the chat path does
**not persist** answers/citations to the DB, so a 100-answer historical audit is impossible from
stored data. Evidence used: (a) proven mechanism — no `[N]` → top-3 by rank, over a corpus with
median 41 chars (Manual median 31); (b) the two traces — log 1 cited an 82-char fragment + two
unrelated chunks; log 2 cited the **Public Service Commission** definition (wrong commission)
while the supporting `Rule 4` was not cited. Every observed citation is fragmentary or mismatched.

## A8. Alternative root causes (attempt to disprove)

| Candidate | FOR primary | AGAINST |
|---|---|---|
| Retrieval ranking | "million" never retrieved | 966 is a 33-char fragment; ranking surfaced 965 fine; can't assemble a split answer → downstream of chunking |
| Embedding quality | 41-char fragments embed weakly | embeddings ranked 965 fine; a consequence of tiny chunks |
| Reranker | absent → no pruning | it's OFF; enabling can't create the missing chunk |
| Prompt construction | model ignores "cite [n]" | truncation ruled out (tail-placed, head+tail keep, under budget; on-topic answers) |
| Citation selection | top-3-by-rank → wrong cites | affects displayed citations only, not answer correctness |
| Grounding logic | ships at score 0.00 | a safety gate, not a cause of wrongness |
| Model limitations | exp. B confabulates w/ good context, no `[N]`, 0.4 tok/s | answered correctly when given whole chunks → real **secondary** |

No alternative independently explains log 1's wrong answer (the fact was absent from every chunk).

## A9. Confidence assessment

| Hypothesis | Probability | Justification |
|---|---|---|
| Chunking/indexing primary | **~65%** | necessary by experiment; failing docs == fragmented docs; answer absent from all chunks; neighbors not even candidates |
| Model limitations primary | **~20%** | confabulates + no citations even with good context, but answers correctly when given whole chunks → next blocker, not first |
| Citation layer primary | **~8%** | top-3-by-rank untrustworthy, but doesn't cause wrong answers |
| Retrieval/reranking primary | **~7%** | found the relevant fragment; reranker off; can't fix a split corpus |

Framing: chunking is primary for **answer correctness**; model + citation/grounding are the
primary blockers for **citation trust** that remain after chunking is fixed.

## A10. Recommended fix order

| # | Fix | Effort | Risk | Answer-quality gain | Re-index? |
|---|---|---|---|---|---|
| 1 | Chat safety gate: refuse/flag at grounding 0.00; stop showing top-3-by-rank as citations | S | Low | High (trust) | No |
| 2 | ±2 `chunk_index` expansion at context-build (interim recall) | S | Low | Medium | No |
| 3 | Re-chunk (cure): OCR newline repair + heading-label sanitisation + small-leaf coalescing to ~300–800c | M | Med | High | **Yes** |
| 4 | Stronger chat generator (Mistral-7B / noviq-3B) or enforced `[N]` | M | Med | Medium-High | No |
| 5 | Re-enable + tune reranker once chunks are provision-sized | S | Low | Low-Med | No |

**Bottom line:** hypothesis **validated as the primary cause of wrong answers (~65%)**, with the
experiment-backed caveat that fixing chunking alone won't fix citation trust or model
confabulation — those need the gate (#1) and a stronger/citation-enforced generator (#4). The
fragmentation is real, severe, systemic to the OCR-structured docs, and upstream of everything
else.

## A11. Reproduction (commands used)

- DB: `psql -h 127.0.0.1 -p 40371 -U postgres -d echo` (password in `~/.local/share/com.echoai.app/pg/.pgpass`).
- Percentiles: `percentile_cont(ARRAY[…]) WITHIN GROUP (ORDER BY length(content))` on `chunks`.
- Neighbor check: `SELECT chunk_index, id, content FROM chunks WHERE source_id='bf7eb14d…' AND chunk_index BETWEEN 962 AND 967`.
- A/B model: `ollama create sindh05 -f Modelfile` (FROM the production GGUF), then
  `POST /api/generate` with `raw:true`, `temperature:0.1` and the ChatML prompt.

---

# Appendix B — Context-window / n_ctx audit (2026-06-13)

**Question:** are `n_ctx` and the context-budget values correct, and does the LLM actually
receive all the selected information when building the prompt?

## B1. Values (all `query_router.rs`, Regular query)

| Value | Const / location | Value |
|---|---|---|
| n_ctx (`gen_n_ctx`) | L455 | 3584 |
| prompt cap (`gen_max_prompt`) | L455 | 2500 tok |
| response cap (`gen_max_tokens`) | L455 | 1024 tok |
| context budget (`CONTEXT_TOKEN_BUDGET`) | L59 | 2400 tok |
| top-K (`CONTEXT_TOP_K`) | L41 | 20 |
| per-chunk cap (`ECHO_PER_CHUNK_CAP_FRAC`) | L123 | 0.25 → 600 tok |
| token estimate | L851,931,2969 | `chars / 4` (+20/passage) |

**Consistency:** prompt 2500 + response 1024 = **3524 ≤ 3584** → fits n_ctx (~60 tok headroom),
no KV-cache overflow on chat. Real-token cap enforced with head+tail keep (`llm.rs:541`), so the
`chars/4` under-estimate (worse for OCR-garbled text) can't drop the tail-placed question.

## B2. Did the LLM receive all selected context? (failing trace) — YES

Log 1: 16 chunks selected, 8,312 chars ≈ 2,078 tok < 2,400 budget; prompt 2,414 tok < 2,500 cap →
**no truncation**. Every selected chunk reached the model. The "million" loss is **upstream**
(chunk 966 never retrieved), not a context-window cut.

## B3. Compounding finding — the <50-char anchor filter

`build_context_pass` (L2944) **Pass 1 skips chunks with content < 50 chars** (L2965, the
"anchor-only" heading-shell filter). With **55% of the corpus < 50 chars** (Manual 65.5%), this
discards the majority of fragments — *including* legitimate content such as `"million, provided
that"` (33c) and `"no external funding is involved."` (42c). So even if retrieval had surfaced
966/967, the context builder would have dropped them. Fragmentation → tiny chunks → the <50-char
filter eats the evidence. (Pass 2 ignores the filter but only runs when Pass 1 is empty.)

## B4. Conclusion

- n_ctx / budgets are **correct and fit**; the LLM gets everything selected — no context-window
  truncation in the failing trace.
- Information loss is **not** at the context-window stage; it is (1) retrieval not surfacing the
  neighbor and (2) the **<50-char anchor filter that would drop the neighbor anyway** due to
  fragmentation.
- Forward note: after re-chunking to ~300–800c, the <50-char filter is harmless and the
  2,400-budget / top-20 interplay is balanced (~19 chunks of ~125 tok fit) — no budget change
  needed first, but worth re-checking once chunks are provision-sized.

---

# Appendix C — Multi-article decomposition: scaling & latency (2026-06-13)

**Question:** what happens when a user asks for 5–6 (or more) different articles/sections?

## C1. How it works (`intent_router/decompose.rs` + `mod.rs`)

- `MAX_SUBQUESTIONS = 6` (`decompose.rs:17`). `split_multi_article` creates one sub-question per
  named provision.
- **2–6 provisions → decompose.** Each sub-question runs **sequentially** through
  `route_scoped` (retrieval + LLM) — Principle III, one model resident (cannot parallelise the
  LLM). Each provision's number is resolved **deterministically from the retrieved chunk title**
  (Fix B — never trusts the model's number; flags "unverified" if no title matches); bodies
  render verbatim; `merge_subanswers` stitches with a deterministic template (no LLM merge).
- **7+ provisions → NO decompose.** `detect_and_split` returns `None` when `subs.len() > 6`
  (`decompose.rs:103`, "over-cap → single-pass"), so it degrades to a single LLM pass over all of
  them — exactly the multi-fact case the 0.5B cannot handle.

## C2. Consequences

1. **Latency (the real blocker).** Sequential = N × (retrieval + LLM). Live traces show ~45 s per
   single-fact query on the 0.5B (CPU) → **6 provisions ≈ 4–5 minutes.**
2. **Fragmentation risk multiplies.** Each provision is retrieved independently and subject to the
   chunk-fragmentation problem; at 95%/provision, all-six-correct ≈ 74%.
3. **Number-safety holds.** Deterministic label resolution means it won't fabricate article
   numbers — worst case flags one "unverified".

## C3. Recommended additions

- **Phase 3 — fully extractive multi-article mode**: for pure "quote/list provisions X, Y, Z"
  requests, render the resolved verbatim chunks **with no LLM per sub-question** (near-instant);
  reserve the LLM for genuine synthesis. Closest to the "top chunks direct, no LLM" intent.
- **Parallelise retrieval** (not the LLM) across sub-questions to cut the non-LLM portion of the
  N× cost; and/or raise `MAX_SUBQUESTIONS` only alongside the extractive mode so latency stays
  bounded.
- This reinforces Phase 2 (re-chunk): per-article correctness is gated by chunk integrity.c
