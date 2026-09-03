# Retrieval Forensic Audit — COMPLETED (2026-06-29)

> **Filled by:** the Claude Code session on the Windows box, run against the **live
> `echo` PostgreSQL on the GPU VM** (`<user>@<PRIVATE_IP>`, bundled PG 17.10 on
> ephemeral port **46233**, db `echo`) + the running `noviq-server` (`:8080`, vLLM
> engine ready) + the repo. Every runtime answer cites SQL output, an `EXPLAIN`, a
> trace id, or a DB row. No speculation.
>
> **Headline:** Rule 16(1)(b) "Direct Contracting" **IS retrieved correctly — vector
> rank 3 of 72.** It never reaches the LLM. The loss is **downstream of retrieval**:
> the ~5,412-token context budget is exhausted by a handful of **oversized flattened-
> table chunks** (one Delegation chunk = 11,313 chars ≈ 2,848 tokens, 53% of the
> budget), and RRF demotes the FTS-absent Direct-Contracting chunk just below them.
> **OBS-1 (pool cutoff) and OBS-2 (AND-restrictive FTS) are REFUTED.** OBS-3 and
> OBS-4 are CONFIRMED.

**Live run of record:** trace **`ee0615b7`** (2026-06-29 18:33 UTC),
`/opt/noviq/echoAI/logs/ab_rerank/queries/2026-06-29_18-33-21_ee0615b7.log`
(`ECHO_LOG_DIR=/opt/noviq/echoAI/logs/ab_rerank`). HTTP 200, 23.2 s.
**SPPRA source id = `c41e3209-8ad9-4aeb-84c0-a83471718e3c`** (the memory's
`9e43e5ac` is stale — corpus was re-indexed; verify-don't-trust paid off).

---

## 0. Context

**Test query (verbatim):**
```
Draft note to Chief Secretary for allowing procurement through direct contracting
with the sole distributor of Canon scanners for 10 departments to be used in e-Daftar.
```
**Expected:** SPPRA **Rule 16 (Direct Contracting)**.
**Actual (trace ee0615b7, 5 chunks fed):** Delegation of Financial Powers p.73 (giant
agriculture table), Manual Rule 93 (Gazette), Manual Rule 152/156 (Press), SPPRA
Rule 55 (entry into force), SPPRA Rule 17 (advertisement). **Rule 16 absent from the
prompt — but present at vector rank 3.**

---

## Pipeline map — `[CODE ✓ — verified]`

All paths under `src-tauri/src/`. Entry `query_router::route` (`query_router.rs:370`).
Verified against current source; corrections noted.

| # | Stage | Symbol · file:line | Verified note |
|---|---|---|---|
| 1 | Query embedding | `embed_one` · `query_router.rs:541` | raw query, no rewrite. **768-dim** BGE (trace STEP 3), CPU ONNX |
| 2 | Semantic cache | `query_router.rs:554` | **DISABLED** in this deployment (`query_cache_enabled()=false`; trace STEP "MISS/DISABLED") |
| 3 | Citation fast-path | `citation_fast_path` · `query_router.rs:604` | query names no citation → **not taken** here |
| 4 | Vector search | `get_context_chunks` · `:705,1724`; SQL `:1759` | pgvector `<=>` cosine, **exact brute-force** (HNSW unused — see Phase 8/9) |
| 5 | FTS | `fts5_retrieve` · `:732,1636`; SQL `:1702` | tokens stopword-filtered + **OR-joined** (`:1671`) then `websearch_to_tsquery`; `LENGTH>150` filter (`:1710`) |
| 6 | RRF merge | `rrf_merge(fts,vec,60)` · `:753,3555` | `Σ 1/(60+rank)`, rank-position, app-layer |
| 7 | Salience boost | `apply_salience_boost` · `:764` | **no-op** this query (trace STEP 8) |
| 8 | Entity boost | `apply_entity_boost` · `:772` | 3 NPs, **no-op** (trace STEP 8) |
| 9 | Reranker | `:814` | **OFF** (`ECHO_RERANK=0` in `.env`; trace STEP 9) |
| 10 | Context builder | `build_context` · `:886,3481`; budget `:485` | **selected 5 chunks**, token-budget bound (trace STEP 10) |
| 11 | Prompt + LLM | `:1056` / Qwen3-8B (trace STEP 12) | — |

**Knobs (live):** `.env` sets only `ECHO_RERANK=0`, `ECHO_RERANK_POOL_K=24`. The rest
default. This query resolved to **fetch_limit = 72** for BOTH vector and FTS (trace
STEP 6/7 "Limit: 72") and **context budget ≈ 5,412 tokens / top-K 36** (trace STEP 2).

---

## PHASE 1 — Pipeline confirmation `[✓ verified]`
**ANSWER:** Map confirmed with four corrections: (2) semantic cache is **disabled** in
this deployment; (4) vector search runs **exact** (HNSW index present but unused —
type/operator mismatch, Phase 8/9); (5) FTS does **OR-join + stopword strip** then
`websearch_to_tsquery` (so it is *not* AND-restrictive); (9) reranker OFF. The test
query takes the **full hybrid path** (no cache, no citation fast-path).
**EVIDENCE:** `query_router.rs:541/554/604/705/732/753/814/1671/1710/3481`; trace
`ee0615b7` STEPs 2–10.

---

## PHASE 2 — Rule 16 exists? `[✓ RUNTIME]`
**ANSWER:** **YES.** Rule 16 is present and split across a heading shell + sub-clauses.
The "Direct Contracting" definition lives in **`0bb1e977` `unit_label='Rule 16(1)(b)'`
(subclause), page 11, 38 tokens, 255 chars**, content begins *"(b) Direct Contracting –
This method means procurement from a single source without competition and shall only be
applicable under any of the following conditions:"*. The `unit_label='Rule 16'` shell
chunk (`da64f91e`) is a **7-token heading** ("16. Alternate Methods of Procurements"),
page 10, parent `1c179fb9`. Rule 16 has 12 sub-chunks total (`Rule 16(1)`, `(1)(a)..(e)`,
several `(1)(i)`). No body is missing; it is **fragmented**, so no single chunk carries
the full provision + its conditions.
**EVIDENCE:** psql `/tmp/p2.sql` — 2 rows for the `ILIKE 'Rule 16' OR '%direct
contracting%'` probe; `/tmp/p6b.sql` listed all 12 `Rule 16%` chunks.

---

## PHASE 3 — Chunk embedding input `[✓ CODE + RUNTIME]`
**ANSWER:** The indexer embeds the **stored `content` string verbatim**, in batches:
`embedder.embed(batch)` at `indexer.rs:392`, inserted to `chunk_embeddings`
(`indexer.rs:445`). For sub-clauses, `structure_parser::node_content` prepends the
Article/Rule heading into `content`, so **the rule number IS embedded** — `0bb1e977`'s
embedded text starts with *"Rule 16 … (b) Direct Contracting – This method means
procurement from a single source without competition…"*. That heading-prepend is exactly
why this chunk earns vector rank 3. Document title is **not** embedded.
**EVIDENCE:** `indexer.rs:392,445`; `structure_parser.rs:1164` (node_content); Phase-2
content head.

---

## PHASE 4 — Query embedding `[✓ CODE + RUNTIME]`
**ANSWER:** The **raw query string** is embedded with no normalization/stopword/synonym/
expansion (`embed_one` at `:541`, `query_owned = retrieval_query.clone()`). Model =
**fastembed BGE ONNX (CPU), 768-dim**, 1,483 ms (trace STEP 3). `query_cache` could not
be dumped for the vector (cache disabled, table not populated), so Phase 5's exact query
vector was obtained **operationally via the live trace candidate lists** instead.
**EVIDENCE:** `query_router.rs:536,541`; trace STEP 3.

---

## PHASE 5 — pgvector ONLY, where is Rule 16? `[✓ RUNTIME]`
**ANSWER:** From the live vector candidate list (trace STEP 6, exact KNN, limit 72):
**`0bb1e977` Rule 16(1)(b) "Direct Contracting" is at VECTOR RANK 3 of 72.** Ranks 1–2
are `635adf00` (SPPRA Rule 17) and `03f580a7` (Delegation). Five further Rule 16
sub-clauses also appear in the pool (`2e43306a` ~r11, `f026ecb0` ~r14, `72d875e1`,
`f028a595`, `435e2f71`, `9b7e1273`). The 7-token `Rule 16` **heading shell** (`da64f91e`)
is **not** in the top-72 (too thin to rank, and would be `LENGTH>150`-filtered anyway).
**So vector recall is NOT the failure.**
**EVIDENCE:** trace `ee0615b7` STEP 6 (candidate id list; `0bb1e977` is the 3rd entry).

---

## PHASE 6 — FTS ONLY (OBS-2) `[✓ RUNTIME — OBS-2 REFUTED]`
**ANSWER:** **OBS-2 is REFUTED for the live path.** The app does **not** pass the raw NL
query to `websearch_to_tsquery`. `fts5_retrieve` (`:1658-1672`) lowercases, strips
stopwords + <3-char tokens, quotes each token, and **joins with `OR`**. The effective
query is `"draft" OR "note" OR "chief" OR "secretary" OR "allowing" OR "procurement" OR
"through" OR "direct" OR "contracting" OR "sole" OR "distributor" OR "canon" OR "scanners"
OR "departments" OR "used" OR "daftar"`, which matches **1,494 chunks** (not 0).
- Direct Contracting (`0bb1e977`) **does match** the OR query but ranks **#177 of 1,494**
  by `ts_rank_cd` (rank 0.5) — far outside the 72-pool, so it contributes **nothing** to
  FTS.
- FTS top-5 are Manual/Development/Delegation chunks (Section 156 etc., ranks 18.5→7.5)
  that match *more distinct* query tokens (chief, secretary, departments, used…), so they
  outrank the short rule.
- For the record, the *literal-NL* `websearch_to_tsquery` (the OBS-2 hypothesis) AND-joins
  all 18 lexemes → **0 corpus matches** — but that string is never what the code runs.
**Why Rule 17 > Rule 16 in the final answer:** Rule 17 is vector rank 1; Rule 16(1)(b) is
vector rank 3 — both FTS-absent. The gap is decided in RRF + context build, not FTS.
**EVIDENCE:** `query_router.rs:1658-1672,1702-1711`; psql `/tmp/p11.sql` (rank 177/1494,
or_matches 1494); psql `/tmp/p6q.sql` (literal-NL tsquery AND-joins → 0).

---

## PHASE 7 — Hybrid scoring `[✓ verified]`
**ANSWER:** **Reciprocal Rank Fusion**, `score(chunk) = Σ 1/(60 + rank_i)` over the two
lists it appears in, rank 1-based, dedup by chunk_id, sorted desc (`rrf_merge` `:3555-3591`).
List A = FTS, list B = vector (`:3567`). Per-candidate for this query:

| chunk | doc / label | vec rank | FTS rank | RRF score |
|---|---|---|---|---|
| `f53c2c6d` | Delegation "(i)" table | 22 | 8 | 1/82 + 1/68 = **0.0269** |
| `635adf00` | SPPRA Rule 17(1) | 1 | — | 1/61 = **0.01639** |
| **`0bb1e977`** | **SPPRA Rule 16(1)(b) Direct Contracting** | **3** | **— (#177)** | 1/63 = **0.01587** |
| `6f7dedf7` | SPPRA Rule 55 | 16 | ~61 | 1/76 + 1/121 = 0.0214 |

Both-list chunks (Delegation, Rule 55) accumulate from two lists and beat the
**vector-only** Direct-Contracting chunk; even vector-rank-1 Rule 17 edges it. So
`0bb1e977` lands at **~RRF position 6–8**, just behind the chunks that go on to fill the
budget.
**EVIDENCE:** `query_router.rs:3555-3591`; trace `ee0615b7` STEP 6/7 ranks; `/tmp/p11.sql`.

---

## PHASE 8 — Execution plans `[✓ RUNTIME]`
**ANSWER:** The vector query is an **exact brute-force KNN**: `EXPLAIN (ANALYZE)` shows
`Seq Scan on chunk_embeddings (8102 rows)` → `Hash Join` to `chunks` → `top-N heapsort`,
70 ms — **no HNSW scan**. Reason: the app orders by `ce.embedding <=> $::vector`
(`:1769`), but the only ANN index is on `((embedding)::halfvec(768)) halfvec_cosine_ops`
— the type/operator differs, so the planner can't use it. (Upside: exact KNN = **no ANN
recall loss**, so Rule 16(1)(b)'s rank-3 is a *true* exact rank, not an artifact.) FTS
uses the **GIN** index `idx_chunks_tsv`.
**EVIDENCE:** psql `/tmp/p8.sql` EXPLAIN output (Seq Scan + top-N heapsort, hit=49665);
`pg_indexes` (Phase 9).

---

## PHASE 9 — Index configuration `[✓ RUNTIME]`
**ANSWER:**
- `chunk_embeddings_hnsw` — **HNSW, m=16, ef_construction=64**, on
  `((embedding)::halfvec(768)) halfvec_cosine_ops`. **Present but UNUSED** by the live
  query (vector `<=>` vs halfvec operator class — Phase 8).
- `idx_chunks_tsv` — **GIN on `tsv`** → used by FTS.
- btree on `chunk_embeddings.chunk_id` (PK), `chunks.id` (PK), plus `source_id`,
  `parent_chunk_id`, `unit_label`, `unit_type`, `block_class`.
- Distance metric (intended): cosine. `hnsw.ef_search` is moot (index not hit).
**EVIDENCE:** psql `/tmp/p9.sql` `pg_indexes` dump (10 indexes).

---

## PHASE 10 — Exact SQL `[✓ verified]`
**ANSWER (verbatim):**
- **Vector** (`get_context_chunks` `:1759-1769`, notebook-scoped variant):
  ```sql
  SELECT c.id, c.source_id, c.content, c.page_number,
         COALESCE(s.file_name, s.file_path, 'unknown')
  FROM chunk_embeddings ce
  JOIN chunks c ON c.id = ce.chunk_id
  JOIN sources s ON s.id = c.source_id
  WHERE ce.chunk_id IN (
      SELECT c2.id FROM chunks c2
      JOIN notebook_sources ns ON ns.source_id = c2.source_id
      WHERE ns.notebook_id = $1 AND c2.is_boilerplate = FALSE )
  ORDER BY (ce.embedding <=> $2::vector) ASC LIMIT $3
  ```
- **FTS** (`fts5_retrieve` `:1702-1711`):
  ```sql
  SELECT c.id, c.source_id, c.content, c.page_number,
         COALESCE(s.file_name, s.file_path, 'unknown')
  FROM chunks c
  JOIN notebook_sources ns ON ns.source_id = c.source_id
  JOIN sources s ON s.id = c.source_id
  WHERE ns.notebook_id = $1 AND c.is_boilerplate = FALSE
    AND c.tsv @@ websearch_to_tsquery('english', $2)
    AND LENGTH(c.content) > 150
  ORDER BY ts_rank_cd(c.tsv, websearch_to_tsquery('english', $2)) DESC LIMIT $3
  ```
  with `$2` = the **OR-joined** token string built at `:1658-1672`.
- Context selection: `build_context` / `build_context_pass` (`:3481-3542`) — no SQL,
  in-memory token-budget walk.
**EVIDENCE:** `query_router.rs:1702-1711,1759-1769,3481-3542`.

---

## PHASE 11 — Ablation `[✓ RUNTIME]`
**ANSWER:** The deciding axis is **context build**, not pool width.

| Config | Rule 16(1)(b) | Note |
|---|---|---|
| 1. Vector only | **rank 3 / 72** | strong recall (exact KNN) |
| 2. FTS only | rank **177 / 1494** (pool 72 → absent) | OR-query, low ts_rank |
| 3. Hybrid (RRF) | **~pos 6–8** | demoted below both-list + vec-rank-1 chunks |
| 4. Vector LIMIT 50 | rank 3 (present) | unaffected — it's already rank 3 |
| 5. Vector LIMIT 200 | rank 3 (present) | unaffected |
| 6. FTS LIMIT 50 | absent (177) | still outside |
| 7/8. CONTEXT_TOP_K 5/10/20 | **excluded by token budget, not K** | only **5** chunks fit ~5,412 tokens regardless of K=36 |

**Key answer:** **NEITHER a pool-cutoff NOR a ranking-floor miss.** Rule 16(1)(b) is at
vector rank 3 and RRF ~pos 6–8 — it is **dropped by the token-budget guard** in
`build_context_pass` (`:3533`) because the few chunks ahead of it are oversized. The
single Delegation chunk `f53c2c6d` = **11,313 chars ≈ 2,848 est-tokens = 53% of the
5,412-token budget**; five such large chunks consume ~5,407/5,412, leaving ~5 tokens, so
the 83-token Direct-Contracting chunk (which *would* fit an empty budget) is skipped.
**EVIDENCE:** `/tmp/p11.sql` (f53c2c6d 11,313 chars/2,848 tok; 0bb1e977 255 chars/83 tok;
Rule 55 674/188; Rule 17(1) 392/118); trace STEP 10 ("Selected: 5 chunks", "21228 chars
~5307 tokens"); `query_router.rs:3522-3535`.

---

## PHASE 12 — Query understanding `[✓ verified]`
**ANSWER:** **No query rewriting, normalization, legal-synonym expansion, or ontology
lookup** before retrieval. The only NL processing: `extract_query_entities` (`:771`)
pulls noun-phrases for the **entity boost only** (which was a no-op here), and
`is_definitional` (`:590`) gates pool width (false here → pool_mult 1.0). The embedding
sees the raw string; the FTS sees stopword-stripped OR tokens. No "direct contracting" ↔
"single source" ↔ "sole distributor" legal synonymy is applied — which is why the
distinctive query lexeme "sole distributor" never connects to Rule 16's "single source".
**EVIDENCE:** `query_router.rs:590,771`; absence confirmed by grep of `query_router.rs` +
`intent_router/` for rewrite/synonym/expand/ontology (only entity-NP + definitional gate).

---

## PHASE 13 — Context builder `[✓ CODE + RUNTIME]`
**ANSWER:** Rule 16(1)(b) **was in the post-RRF candidate list** (vector rank 3 → RRF
~pos 6–8 of 139 unique) but **was dropped in `build_context_pass`**. The pass walks RRF
order, skips anchor-only (<50 char) chunks, dedups, and admits a chunk only if
`total_tokens + estimated ≤ token_budget` — except it **always admits the first chunk
whole** and **never slices** (F015/T008 removed the old per-chunk cap, `:3524-3530`).
Consequently the first ~5 oversized chunks fill the ~5,412-token budget; every later
chunk — including the small Direct-Contracting chunk — hits the `continue` at `:3533` and
is silently skipped. Net: **139 candidates → 5 fed** (trace STEP 10), Rule 16 not among
them.
**EVIDENCE:** `query_router.rs:3498-3542` (esp. 3524-3535); trace `ee0615b7` STEP 8
(139 unique) → STEP 10 (5 selected, sources = Delegation/Manual/SPPRA only).

---

## PHASE 14 — Database inspection `[✓ RUNTIME]`
**ANSWER:** Rule 16(1)(b) `0bb1e977` is fully indexable — **nothing NULL/missing**: it
has a `chunk_embeddings` row (it appears at vector rank 3, which requires the embedding),
a populated `tsv` (it matches the OR FTS query at rank 177), correct
`source_id=c41e3209…`, `page_number=11`, `unit_label='Rule 16(1)(b)'`,
`unit_path='Part_II/Rule_16/(1)/(b)'`, `parent_chunk_id=1d1138bf…`. **The chunk is
healthy; exclusion is purely the context-budget drop (Phase 13).**
**EVIDENCE:** psql `/tmp/p2.sql` (row fields) + presence in vector pool (trace STEP 6) +
FTS match (`/tmp/p11.sql` rank 177).

---

## PHASE 15 — End-to-end trace `[✓ RUNTIME]`
**ANSWER:** Issued via `POST /api/query` (admin bearer), conversation `audit-1782758001`,
HTTP 200 in 23.2 s → trace **`ee0615b7`**. Decisive steps:
- STEP 6 Vector: 72 candidates, **`0bb1e977` present at position 3**.
- STEP 7 FTS: 72 candidates, **`0bb1e977` absent** (it ranks 177 globally).
- STEP 8 Merge: 139 unique; top match Delegation; salience/entity boosts **no-op**.
- STEP 9 Reranker: skipped (off).
- STEP 10 Build context: **5 chunks selected**, 21,228 chars (~5,307 tok); sources =
  Delegation p73 / Manual Rule 93 / Manual Rule 152 / SPPRA Rule 55 / SPPRA Rule 17 —
  **Rule 16 not present.**
- STEP 13/16 Answer cites only [1] Delegation `f53c2c6d` p73 and [2] Rule 17 `635adf00`
  p13; explicitly says *"direct contracting … is not explicitly mentioned in the provided
  documents."*
**So `0bb1e977` lives through retrieval (vector) and dies at context build.**
**EVIDENCE:** trace `ee0615b7`
(`/opt/noviq/echoAI/logs/ab_rerank/queries/2026-06-29_18-33-21_ee0615b7.log`).

---

## PHASE 16 — Root cause `[✓ — from collected evidence]`
**Primary failure class: CONTEXT-BUDGET STARVATION BY OVERSIZED CHUNKS (not retrieval).**

| # | Root cause | Severity | ~Contribution | Proof |
|---|---|---|---|---|
| RC-1 | **Oversized flattened-table/list chunks monopolize the token budget.** One Delegation chunk = 2,848 tok (53% of 5,412). 5 big chunks exhaust the budget; the 83-tok Direct-Contracting chunk is skipped at `build_context_pass:3533`. | **Critical** | **~65%** | `/tmp/p11.sql` sizes; trace STEP 10; `:3524-3535` |
| RC-2 | **RRF demotes the FTS-absent provision.** Vector-only chunks (Rule 16(1)(b), rank 3) score 1/63 and fall below both-list chunks (Delegation 0.0269) and vec-rank-1 Rule 17 — landing ~pos 6–8, i.e. *behind* the budget-filling chunks. | **High** | **~25%** | Phase 7 table; `:3555-3591` |
| RC-3 | **No legal-synonym / query expansion.** "sole distributor / direct contracting" never links to Rule 16's "single source without competition"; FTS ranks the rule 177th. Hurts the FTS signal that RRF needs to rescue it. | **Medium** | **~10%** | Phase 12; `/tmp/p11.sql` rank 177 |
| RC-4 | **Provision fragmentation.** Rule 16 = 7-tok heading shell + 12 tiny sub-clauses; no single chunk holds the full provision, so even when one sub-clause is retrieved it is thin and easily out-weighed. | **Medium** | (compounds RC-1/2) | Phase 2 |

**Refuted / confirmed template hypotheses:**
- **OBS-1 pool cutoff — REFUTED.** Rule 16(1)(b) is at vector rank 3, deep inside the 72-pool.
- **OBS-2 AND-restrictive FTS — REFUTED.** The app OR-joins tokens → 1,494 matches; the
  rule simply ranks 177th. (Raw-NL AND would give 0, but the code never runs that.)
- **OBS-3 scores discarded — CONFIRMED.** Retrieval returns 5-tuples
  `(id, source_id, content, page, file)`; no distance/ts_rank survives, so the app cannot
  display scores and this audit needed the trace + manual SQL.
- **OBS-4 stale trace label — CONFIRMED.** `query_router.rs:715` prints engine
  `"sqlite-vec ANN"` while the SQL is pgvector `<=>` exact KNN (`:1769`).

---

## PHASE 17 — Recommendations `[✓]`
Ordered by impact-per-effort.

1. **Cap per-chunk budget share / re-enable a size guard (fixes RC-1, the 65%).**
   *Evidence:* one 2,848-tok chunk = 53% of budget. *Change:* in `build_context_pass`
   add a per-chunk ceiling (e.g. ≤ 30–35% of `token_budget`) **after** the first chunk,
   OR cap oversized chunks and continue admitting smaller high-rank chunks. *Improvement:*
   admits ~10–15 chunks instead of 5; Rule 16(1)(b) reaches the prompt. *Complexity:* Low.
   *Risk:* Low (the F015 removal was to avoid dropping a *correctly-top-ranked* provision —
   keep "always admit first whole", cap the rest). *Files:* `query_router.rs:3524-3535`.

2. **Split oversized table chunks at ingestion (root fix for RC-1/RC-4).**
   *Evidence:* Delegation "(i)" = 11,313-char flattened table (the chunking-audit RC-D/RC-T
   oversized + table-row findings). *Change:* row-/sub-item-level chunking for flattened
   tables and per-sub-clause atomicity. *Improvement:* removes budget hogs corpus-wide.
   *Complexity:* Med. *Risk:* Med (re-index). *Files:* `structure_parser.rs`, `chunker.rs`,
   doc_profiles. (Already on the chunking-audit backlog — RC-D/RC-E/RC-T.)

3. **Give RRF a score-aware floor or guarantee top-N-per-signal (fixes RC-2).**
   *Evidence:* vector rank-3 chunk demoted to RRF ~pos 6–8. *Change:* always carry the
   top ~5 *vector* chunks into context regardless of RRF (a "recall guarantee" slot), or
   weight RRF by normalized cosine. *Improvement:* a strong semantic hit can't be buried by
   both-list noise. *Complexity:* Low-Med. *Risk:* Med (tune weights). *Files:*
   `query_router.rs:753-772,3555`.

4. **Lightweight legal-synonym expansion for procurement terms (fixes RC-3).**
   *Evidence:* "sole distributor"↛"single source"; FTS rank 177. *Change:* a small curated
   synonym map applied to the FTS token set (and optionally a second embedding). *Improvement:*
   lifts the rule into the FTS pool so RRF rescues it. *Complexity:* Med. *Risk:* Med
   (over-expansion). *Files:* `fts5_retrieve` `:1658`, new synonym table.

5. **Instrumentation (OBS-3/OBS-4) — basis for the in-app Retrieval Debugger.**
   - Make `get_context_chunks`/`fts5_retrieve` return `(chunk, score)` and log top-50 with
     distance/ts_rank (`:705/732`). - Log `websearch_to_tsquery(q)::text`. - Log per-chunk
     `(vec_rank, fts_rank, rrf_score)` in `rrf_merge`. - Log each context candidate
     kept/dropped + **reason** ("budget: needed 83 tok, 5 left"). - Fix the `:715` label to
     "pgvector (cosine `<=>`, exact KNN)". *Complexity:* Low. *Risk:* Low.

6. **(Perf, not this bug) Make the HNSW index usable.** Either query via
   `embedding::halfvec(768) <=> $::halfvec(768)` or rebuild the index on `vector_cosine_ops`.
   *Note:* exact KNN currently *helps* recall; defer until corpus scale demands ANN. *Files:*
   `:1769`, migration.

---

## Reproduction (live, verified 2026-06-29)
```bash
ssh <user>@<PRIVATE_IP>
PSQL=/opt/noviq/data/pg/install/17.10.0/bin/psql
PORT=$(ps aux | grep "[p]ostgres -D" | grep -oE "\-p [0-9]+")   # ephemeral; was 46233
PGPASSWORD=$(cat /opt/noviq/data/pg/.pgpass) $PSQL -h 127.0.0.1 -p <PORT> -U postgres -d echo
# audit SQL files written this session: /tmp/{p2,p6q,p6b,p8,p9,p11}.sql
# end-to-end: POST /api/query (admin bearer from /api/login) → trace under
#   /opt/noviq/echoAI/logs/ab_rerank/queries/  (ECHO_LOG_DIR)
```

## Return protocol — DONE
All ANSWER/EVIDENCE slots filled from live evidence. Confirmed root cause:
**context-budget starvation by oversized chunks (RC-1) + RRF demotion of the FTS-absent
provision (RC-2)** — Rule 16 is retrieved (vector rank 3) but never reaches the LLM.
Next: turn RC-1/RC-2 into the in-app Retrieval Debugger + the Phase-17 fixes.
